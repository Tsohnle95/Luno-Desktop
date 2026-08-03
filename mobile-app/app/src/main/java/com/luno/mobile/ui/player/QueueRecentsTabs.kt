package com.luno.mobile.ui.player

import android.widget.Toast
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.DragHandle
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.RadioButtonUnchecked
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Tab
import androidx.compose.material3.TabRow
import androidx.compose.material3.TabRowDefaults
import androidx.compose.material3.TabRowDefaults.tabIndicatorOffset
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import com.luno.mobile.playback.MediaTrack
import com.luno.mobile.playback.MusicController
import com.luno.mobile.ui.components.ArtworkImage
import com.luno.mobile.LunoApp
import com.luno.mobile.ui.components.BulkSelectionToolbar
import com.luno.mobile.ui.theme.AccentGreen
import com.luno.mobile.ui.theme.Dimens
import com.luno.mobile.ui.theme.PrimaryText
import com.luno.mobile.ui.theme.SecondaryText
import com.luno.mobile.ui.theme.SurfaceDark
import com.luno.mobile.ui.theme.SurfaceElevated
import kotlin.math.roundToInt
import kotlinx.coroutines.launch

/**
 * The two Queue/Recents tabs (desktop "Playing Next" + "Recently Played"
 * contract): [PLAYING_NEXT] is the live playback queue with drag-to-reorder,
 * [RECENTLY_PLAYED] the persisted history.  Shared by the full player's
 * [QueueSheet] and the Recents screen opened from the Settings drawer.
 */
enum class QueueRecentsTab(val label: String) {
    PLAYING_NEXT("Playing Next"),
    RECENTLY_PLAYED("Recently played")
}

/** Accent-green tab row shared by the queue sheet and the Recents screen. */
@Composable
fun QueueRecentsTabRow(
    selected: QueueRecentsTab,
    onSelect: (QueueRecentsTab) -> Unit
) {
    val selectedIndex = selected.ordinal
    TabRow(
        selectedTabIndex = selectedIndex,
        containerColor = SurfaceDark,
        contentColor = AccentGreen,
        indicator = { tabPositions ->
            TabRowDefaults.SecondaryIndicator(
                modifier = Modifier.tabIndicatorOffset(tabPositions[selectedIndex]),
                color = AccentGreen
            )
        }
    ) {
        QueueRecentsTab.entries.forEach { tab ->
            Tab(
                selected = tab == selected,
                onClick = { onSelect(tab) },
                text = {
                    Text(
                        text = tab.label,
                        style = MaterialTheme.typography.labelLarge,
                        color = if (tab == selected) AccentGreen else SecondaryText
                    )
                }
            )
        }
    }
}

/**
 * "Playing Next" tab — the live playback queue from
 * [MusicController.getQueue]: artwork rows with long-press drag-to-reorder
 * via [MusicController.moveQueueItem].  [modifier] sizes the list (the
 * sheet caps it at 400dp; the Recents screen lets it fill the page).
 */
@Composable
fun PlayingNextTab(
    musicController: MusicController,
    modifier: Modifier = Modifier
) {
    val currentTrack by musicController.currentTrack.collectAsState()
    val queueRevision by musicController.queueRevision.collectAsState()
    var queueItems by remember { mutableStateOf(musicController.getQueue()) }

    // Refresh the snapshot when the tab appears (queue may have changed
    // since it was last shown) and whenever the playing item changes
    // (queue edits or track auto-advance).
    LaunchedEffect(Unit) {
        queueItems = musicController.getQueue()
    }
    LaunchedEffect(currentTrack?.uri, queueRevision) {
        queueItems = musicController.getQueue()
    }

    var draggingIndex by remember { mutableStateOf<Int?>(null) }
    var dragOffsetY by remember { mutableFloatStateOf(0f) }
    val rowHeightPx = with(LocalDensity.current) { QUEUE_ROW_HEIGHT.toPx() }

    if (queueItems.isEmpty()) {
        Text(
            text = "Queue is empty",
            style = MaterialTheme.typography.bodyMedium,
            color = SecondaryText,
            modifier = Modifier.padding(vertical = Dimens.paddingMedium)
        )
    } else {
        LazyColumn(modifier = modifier) {
            itemsIndexed(
                queueItems,
                key = { index, track -> "${index}:${track.uri}" },
                contentType = { _, _ -> "queue-row" }
            ) { index, track ->
                QueueRow(
                    track = track,
                    isDragging = draggingIndex == index,
                    offsetY = if (draggingIndex == index) dragOffsetY else 0f,
                    onDragStart = {
                        draggingIndex = index
                        dragOffsetY = 0f
                    },
                    onDrag = { y -> dragOffsetY += y },
                    onDragEnd = {
                        val from = draggingIndex
                        if (from != null) {
                            val to = (from + (dragOffsetY / rowHeightPx).roundToInt())
                                .coerceIn(0, queueItems.lastIndex)
                            if (to != from) {
                                musicController.moveQueueItem(from, to)
                                queueItems = musicController.getQueue()
                            }
                            draggingIndex = null
                            dragOffsetY = 0f
                        }
                    }
                )
            }
        }
    }
}

/**
 * "Recently played" tab — the persisted history (most recent first, max
 * 100, desktop move-to-front replay semantics).  Tapping a row plays it
 * within the history context (next/prev walk recent plays); "Clear all"
 * empties the history.
 */
@Composable
fun RecentlyPlayedTab(
    musicController: MusicController,
    modifier: Modifier = Modifier
) {
    val history by musicController.recentlyPlayed.collectAsState()
    val context = LocalContext.current
    val app = context.applicationContext as LunoApp
    val allTracks by app.libraryData.tracks.collectAsState()
    val scope = androidx.compose.runtime.rememberCoroutineScope()
    var selectionMode by remember { mutableStateOf(false) }
    var selectedUris by remember { mutableStateOf(setOf<String>()) }
    val selectedTracks = allTracks.filter { it.uri in selectedUris }

    fun toggleSelection(uri: String) {
        val next = if (uri in selectedUris) selectedUris - uri else selectedUris + uri
        selectedUris = next
        selectionMode = next.isNotEmpty()
    }

    fun exitSelection() {
        selectionMode = false
        selectedUris = emptySet()
    }

    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically
    ) {
        if (selectionMode) {
            BulkSelectionToolbar(
                selectedTracks = selectedTracks,
                selectedPlaylists = emptyList(),
                allSelected = history.isNotEmpty() && history.all { it.uri in selectedUris },
                onSelectAll = { selectAll ->
                    if (selectAll) {
                        selectionMode = true
                        selectedUris = history.map { it.uri }.toSet()
                    } else {
                        exitSelection()
                    }
                },
                onDismiss = ::exitSelection,
                onAddToPlaylist = { playlist, trackUris ->
                    scope.launch {
                        app.playlistRepository.addTracksToPlaylist(playlist.id, trackUris)
                        Toast.makeText(context, "Added to ${playlist.name}", Toast.LENGTH_SHORT).show()
                    }
                    exitSelection()
                },
                onCreatePlaylist = { name, description, trackUris ->
                    scope.launch {
                        app.playlistRepository.createPlaylist(name, description).onSuccess { playlist ->
                            app.playlistRepository.addTracksToPlaylist(playlist.id, trackUris)
                            Toast.makeText(context, "Created ${playlist.name}", Toast.LENGTH_SHORT).show()
                        }
                    }
                    exitSelection()
                },
                onRemoveTracks = { uris ->
                    scope.launch { uris.forEach { app.libraryRepository.deleteTrack(it) } }
                    exitSelection()
                }
            )
        } else {
            Text(
                text = "${history.size} played",
                style = MaterialTheme.typography.bodySmall,
                color = SecondaryText,
                modifier = Modifier.weight(1f)
            )
            TextButton(
                onClick = { musicController.clearRecentlyPlayed() },
                enabled = history.isNotEmpty()
            ) {
                Text("Clear all", color = AccentGreen)
            }
        }
    }

    if (history.isEmpty()) {
        Text(
            text = "Nothing played yet this session. Played tracks appear here.",
            style = MaterialTheme.typography.bodyMedium,
            color = SecondaryText,
            modifier = Modifier.padding(vertical = Dimens.paddingMedium)
        )
    } else {
        LazyColumn(modifier = modifier) {
            // Move-to-front semantics guarantee unique URIs.
            itemsIndexed(
                history,
                key = { _, track -> track.uri },
                contentType = { _, _ -> "recent-row" }
            ) { index, track ->
                RecentRow(
                    track = track,
                    selected = if (selectionMode) track.uri in selectedUris else null,
                    onClick = {
                        if (selectionMode) toggleSelection(track.uri)
                        else musicController.play(history, index)
                    },
                    onLongPress = {
                        selectionMode = true
                        selectedUris = selectedUris + track.uri
                    }
                )
            }
        }
    }

}

@Composable
@OptIn(ExperimentalFoundationApi::class)
private fun RecentRow(
    track: MediaTrack,
    selected: Boolean?,
    onClick: () -> Unit,
    onLongPress: () -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .combinedClickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = onClick,
                onLongClick = onLongPress
            )
            .padding(vertical = Dimens.paddingSmall),
        verticalAlignment = Alignment.CenterVertically
    ) {
        if (selected != null) {
            Icon(
                imageVector = if (selected) Icons.Filled.CheckCircle else Icons.Filled.RadioButtonUnchecked,
                contentDescription = if (selected) "Selected" else "Not selected",
                tint = if (selected) AccentGreen else SecondaryText,
                modifier = Modifier.size(Dimens.iconSize)
            )
            Spacer(modifier = Modifier.width(Dimens.paddingSmall))
        }
        ArtworkImage(
            artworkUri = track.artworkUri,
            modifier = Modifier
                .size(Dimens.albumArtSmall)
                .padding(end = Dimens.paddingMedium),
            placeholderIconSize = 20.dp,
            decodeSizePx = 192
        )
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = track.title,
                style = MaterialTheme.typography.titleSmall,
                color = PrimaryText,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
            Text(
                text = track.artist,
                style = MaterialTheme.typography.bodySmall,
                color = SecondaryText,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
        }
    }
}

@Composable
private fun QueueRow(
    track: MediaTrack,
    isDragging: Boolean,
    offsetY: Float,
    onDragStart: () -> Unit,
    onDrag: (Float) -> Unit,
    onDragEnd: () -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(QUEUE_ROW_HEIGHT)
            .background(if (isDragging) SurfaceElevated else Color.Transparent)
            .graphicsLayer {
                translationY = offsetY
                scaleX = if (isDragging) 1.02f else 1f
                scaleY = if (isDragging) 1.02f else 1f
                alpha = if (isDragging) 0.92f else 1f
            }
            .zIndex(if (isDragging) 1f else 0f)
            .pointerInput(track.uri) {
                detectDragGesturesAfterLongPress(
                    onDragStart = { onDragStart() },
                    onDrag = { change, dragAmount ->
                        change.consume()
                        onDrag(dragAmount.y)
                    },
                    onDragEnd = { onDragEnd() },
                    onDragCancel = { onDragEnd() }
                )
            }
            .padding(vertical = Dimens.paddingSmall),
        verticalAlignment = Alignment.CenterVertically
    ) {
        ArtworkImage(
            artworkUri = track.artworkUri,
            modifier = Modifier
                .size(Dimens.albumArtSmall)
                .padding(end = Dimens.paddingMedium),
            placeholderIconSize = 20.dp,
            decodeSizePx = 192
        )
        Column(
            modifier = Modifier.weight(1f)
        ) {
            Text(
                text = track.title,
                style = MaterialTheme.typography.titleSmall,
                color = PrimaryText,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
            Text(
                text = track.artist,
                style = MaterialTheme.typography.bodySmall,
                color = SecondaryText,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
        }
        Spacer(modifier = Modifier.width(Dimens.paddingSmall))
        Icon(
            imageVector = Icons.Filled.DragHandle,
            contentDescription = "Reorder",
            tint = SecondaryText,
            modifier = Modifier.size(Dimens.iconSize)
        )
    }
}

private val QUEUE_ROW_HEIGHT = 64.dp
