package com.boombastic.mobile.ui.library

import android.widget.Toast
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Clear
import androidx.compose.material.icons.filled.FilterList
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Shuffle
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import com.boombastic.mobile.BoomBasticApp
import com.boombastic.mobile.data.db.entity.Playlist
import com.boombastic.mobile.data.db.entity.Track
import com.boombastic.mobile.playback.MediaTrack
import com.boombastic.mobile.playback.MusicController
import com.boombastic.mobile.ui.components.PlaylistCard
import com.boombastic.mobile.ui.components.BulkSelectionToolbar
import com.boombastic.mobile.ui.components.SortChip
import com.boombastic.mobile.ui.components.TrackActionsSheet
import com.boombastic.mobile.ui.components.TrackRowCard
import com.boombastic.mobile.ui.components.TrackSortMode
import com.boombastic.mobile.ui.components.sortedByMode
import com.boombastic.mobile.ui.components.sortedPlaylistsByMode
import com.boombastic.mobile.ui.home.SectionHeader
import com.boombastic.mobile.ui.theme.AccentGreen
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText
import com.boombastic.mobile.ui.theme.SurfaceDark
import com.boombastic.mobile.ui.theme.SurfaceElevated
import kotlinx.coroutines.launch

/**
 * Library tab modeled on the desktop "All Music" page: a centered 2x2
 * collage of the first four album covers (~60% width), a Play + Shuffle
 * row with a "Playlist view" filter tab on the right, a search bar, then
 * either all songs (track card rows) or all playlists (full-width
 * playlist cards).
 */
@Composable
fun LibraryScreen(
    musicController: MusicController,
    onPlay: (List<MediaTrack>, Int, Boolean) -> Unit = { _, _, _ -> },
    onOpenPlaylist: (Long) -> Unit = {}
) {
    val context = LocalContext.current
    val app = context.applicationContext as BoomBasticApp
    val scope = rememberCoroutineScope()
    // Rendered from the app-warmed LibraryData flows (queries run once at
    // startup, so switching to this tab shows the full content in the same
    // frame as the transition — no progressive pop-in, no spinner churn).
    val libraryData = app.libraryData
    val allTracks by libraryData.tracks.collectAsState()
    val playlistsWithTracks by libraryData.playlists.collectAsState()
    val downloads by libraryData.downloads.collectAsState()
    val libraryLoaded by libraryData.loaded.collectAsState()
    val shuffleEnabled by musicController.shuffleEnabled.collectAsState()
    var query by rememberSaveable { mutableStateOf("") }
    var playlistView by rememberSaveable { mutableStateOf(false) }
    var sortMode by rememberSaveable { mutableStateOf(TrackSortMode.AZ) }

    // All-or-nothing first render: hold the whole screen behind one static
    // placeholder until the library data has emitted its first values
    // (normally already done at startup — this only shows on the very
    // first app frames).  Rendering the chrome before the data arrived
    // made the screen visibly "load in pieces".
    if (!libraryLoaded) {
        Box(
            modifier = Modifier.fillMaxSize(),
            contentAlignment = Alignment.Center
        ) {
            CircularProgressIndicator(color = AccentGreen)
        }
        return
    }

    // Multi-select / batch actions (3-dot menu on the section header)
    var selectionMode by remember { mutableStateOf(false) }
    var selectedKeys by remember { mutableStateOf(setOf<String>()) }
    var showBatchMenu by remember { mutableStateOf(false) }

    // Track actions sheet — hoisted out of the LazyColumn: composing a
    // ModalBottomSheet inside a lazy item makes it scroll with the list.
    var actionsTrack by remember { mutableStateOf<Track?>(null) }

    fun playlistHasActiveJobs(playlistId: Long): Boolean =
        downloads.any {
            it.playlistId == playlistId &&
                (it.state == com.boombastic.mobile.data.db.entity.DownloadState.QUEUED ||
                    it.state == com.boombastic.mobile.data.db.entity.DownloadState.DOWNLOADING)
        }

    // URL edit + delete dialogs hosted here (the shared PlaylistCard menu
    // only signals intent via its callbacks).
    var urlDialogPlaylist by remember { mutableStateOf<Playlist?>(null) }
    var deleteDialogPlaylist by remember { mutableStateOf<Playlist?>(null) }
    var clearDialogPlaylist by remember { mutableStateOf<Playlist?>(null) }

    val filteredPlaylists = if (query.isBlank()) {
        playlistsWithTracks
    } else {
        playlistsWithTracks.filter {
            it.playlist.name.contains(query, ignoreCase = true)
        }
    }
    val filteredTracks = if (query.isBlank()) {
        allTracks
    } else {
        allTracks.filter {
            it.title.contains(query, ignoreCase = true) ||
                it.artist.contains(query, ignoreCase = true) ||
                it.album.contains(query, ignoreCase = true)
        }
    }

    // Tracks and playlists share one sort mode (A–Z / Z–A / Recently added /
    // Duration — Duration sorts longest-first: tracks by their own length,
    // playlists by total length), available in both views.  Derived lists
    // are memoized so recompositions (selection toggles, dialog state,
    // etc.) don't re-sort/re-map the whole library.
    val sortedTracks = remember(filteredTracks, sortMode) {
        filteredTracks.sortedByMode(sortMode)
    }
    val displayPlaylists = remember(filteredPlaylists, sortMode) {
        filteredPlaylists.sortedPlaylistsByMode(sortMode)
    }

    val mediaTracks = remember(sortedTracks) { sortedTracks.map { it.toMediaTrack() } }

    // Batch-action helpers: keys are track URIs in songs view, "p<id>" in
    // playlist view.
    val allKeys = if (playlistView) {
        displayPlaylists.map { "p${it.playlist.id}" }.toSet()
    } else {
        sortedTracks.map { it.uri }.toSet()
    }
    val selectedSongs = selectedKeys.filter { !it.startsWith("p") }
    val selectedPlaylistIds = selectedKeys.filter { it.startsWith("p") }
        .mapNotNull { it.removePrefix("p").toLongOrNull() }
    val selectedTracks = allTracks.filter { it.uri in selectedSongs }
    val selectedPlaylists = playlistsWithTracks.filter { it.playlist.id in selectedPlaylistIds }

    fun toggleSelection(key: String) {
        val next = if (key in selectedKeys) selectedKeys - key else selectedKeys + key
        selectedKeys = next
        selectionMode = next.isNotEmpty()
    }

    fun beginSelection(key: String) {
        selectionMode = true
        selectedKeys = selectedKeys + key
    }

    fun exitSelection() {
        selectionMode = false
        selectedKeys = emptySet()
    }

    val allSelected = allKeys.isNotEmpty() && allKeys.all { it in selectedKeys }

    fun setAllVisibleSelected(selectAll: Boolean) {
        if (!selectAll) {
            selectedKeys = selectedKeys - allKeys
            selectionMode = selectedKeys.isNotEmpty()
        } else {
            selectedKeys = selectedKeys + allKeys
            selectionMode = true
        }
    }

    fun addSelectionToPlaylist(target: com.boombastic.mobile.data.db.entity.Playlist, trackUris: List<String>) {
        scope.launch {
            app.playlistRepository.addTracksToPlaylist(target.id, trackUris)
            Toast.makeText(context, "Added to ${target.name}", Toast.LENGTH_SHORT).show()
        }
        exitSelection()
    }

    fun createPlaylistFromSelection(name: String, description: String, trackUris: List<String>) {
        scope.launch {
            app.playlistRepository.createPlaylist(name, description).onSuccess { playlist ->
                app.playlistRepository.addTracksToPlaylist(playlist.id, trackUris)
                Toast.makeText(context, "Created ${playlist.name}", Toast.LENGTH_SHORT).show()
            }
        }
        exitSelection()
    }

    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        verticalArrangement = Arrangement.spacedBy(Dimens.paddingSmall),
        contentPadding = PaddingValues(vertical = Dimens.paddingLarge)
    ) {
        item {
            Text(
                text = "Your Library",
                style = MaterialTheme.typography.headlineLarge,
                color = PrimaryText,
                modifier = Modifier.padding(
                    start = Dimens.paddingLarge,
                    end = Dimens.paddingLarge,
                    bottom = Dimens.paddingSmall
                )
            )
        }

        if (selectionMode) {
            item {
                BulkSelectionToolbar(
                    selectedTracks = selectedTracks,
                    selectedPlaylists = selectedPlaylists,
                    allSelected = allSelected,
                    onSelectAll = ::setAllVisibleSelected,
                    onDismiss = ::exitSelection,
                    onAddToPlaylist = ::addSelectionToPlaylist,
                    onCreatePlaylist = ::createPlaylistFromSelection,
                    onRemoveTracks = { uris ->
                        scope.launch { uris.forEach { app.libraryRepository.deleteTrack(it) } }
                        exitSelection()
                    },
                    onDeletePlaylists = { ids ->
                        scope.launch { ids.forEach { app.playlistRepository.deletePlaylist(it) } }
                        exitSelection()
                    },
                    modifier = Modifier.padding(horizontal = Dimens.paddingLarge)
                )
            }
        }

        // (4-quadrant collage header disabled — see change record)
        // Play — Shuffle sits ~1rem (16dp) to the right, desktop style
        item {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = Dimens.paddingLarge),
                verticalAlignment = Alignment.CenterVertically
            ) {
                if (allTracks.isNotEmpty()) {
                    Button(
                        // Full sorted context — next/prev walk the whole
                        // (filtered/sorted) list, not a single track.
                        onClick = { onPlay(mediaTracks, 0, false) },
                        colors = ButtonDefaults.buttonColors(containerColor = AccentGreen)
                    ) {
                        Icon(
                            imageVector = Icons.Filled.PlayArrow,
                            contentDescription = null,
                            modifier = Modifier.size(Dimens.iconSize)
                        )
                        Spacer(modifier = Modifier.width(Dimens.paddingSmall))
                        Text("Play")
                    }
                    Spacer(modifier = Modifier.width(Dimens.paddingLarge))
                    // Shuffle — icon + text only, no box; text in accent green
                    Row(
                        modifier = Modifier
                            .clip(RoundedCornerShape(Dimens.cornerMedium))
                            .clickable(
                                interactionSource = remember { MutableInteractionSource() },
                                indication = null,
                                onClick = {
                                    onPlay(mediaTracks, 0, true)
                                }
                            )
                            .padding(vertical = Dimens.paddingSmall),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Icon(
                            imageVector = Icons.Filled.Shuffle,
                            contentDescription = null,
                            tint = AccentGreen,
                            modifier = Modifier.size(Dimens.iconSize)
                        )
                        Spacer(modifier = Modifier.width(Dimens.paddingSmall))
                        Text(
                            text = "Shuffle",
                            style = MaterialTheme.typography.labelLarge,
                            color = AccentGreen,
                            fontWeight = if (shuffleEnabled) {
                                FontWeight.Bold
                            } else {
                                FontWeight.Medium
                            },
                            textDecoration = if (shuffleEnabled) {
                                TextDecoration.Underline
                            } else {
                                TextDecoration.None
                            }
                        )
                    }
                }

                Spacer(modifier = Modifier.weight(1f))
                Row(
                    modifier = Modifier
                        .clip(RoundedCornerShape(Dimens.cornerMedium))
                        .clickable(
                            interactionSource = remember { MutableInteractionSource() },
                            indication = null,
                            onClick = { playlistView = !playlistView }
                        )
                        .padding(Dimens.paddingSmall),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Icon(
                        imageVector = Icons.Filled.FilterList,
                        contentDescription = null,
                        tint = AccentGreen,
                        modifier = Modifier.size(Dimens.iconSize)
                    )
                    Spacer(modifier = Modifier.width(Dimens.paddingSmall))
                    Text(
                        text = if (playlistView) "All songs view" else "Playlist view",
                        style = MaterialTheme.typography.labelLarge,
                        color = AccentGreen
                    )
                }
                SortChip(
                    mode = sortMode,
                    onModeChange = { sortMode = it },
                    compact = true
                )
            }
        }

        // Search bar
        item {
            OutlinedTextField(
                value = query,
                onValueChange = { query = it },
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = Dimens.paddingLarge),
                placeholder = { Text("Search your library", color = SecondaryText) },
                leadingIcon = {
                    Icon(
                        imageVector = Icons.Filled.Search,
                        contentDescription = null,
                        tint = SecondaryText
                    )
                },
                trailingIcon = {
                    if (query.isNotEmpty()) {
                        Icon(
                            imageVector = Icons.Filled.Clear,
                            contentDescription = "Clear search",
                            tint = SecondaryText,
                            modifier = Modifier
                                .clickable(
                                    interactionSource = remember { MutableInteractionSource() },
                                    indication = null,
                                    onClick = { query = "" }
                                )
                                .padding(Dimens.paddingSmall)
                        )
                    }
                },
                singleLine = true,
                colors = OutlinedTextFieldDefaults.colors(
                    focusedTextColor = PrimaryText,
                    unfocusedTextColor = PrimaryText,
                    cursorColor = AccentGreen,
                    focusedBorderColor = AccentGreen,
                    unfocusedBorderColor = SurfaceElevated,
                    focusedContainerColor = SurfaceDark,
                    unfocusedContainerColor = SurfaceDark
                )
            )
        }

        if (playlistView) {
            // Playlist view — all playlists, full-width cards
            item {
                if (selectionMode) {
                    Text(
                        text = "Playlists (${displayPlaylists.size})",
                        style = MaterialTheme.typography.titleMedium,
                        color = PrimaryText,
                        modifier = Modifier.padding(horizontal = Dimens.paddingLarge)
                    )
                } else {
                    BatchSectionHeader(
                        title = "Playlists (${displayPlaylists.size})",
                        showMenu = showBatchMenu,
                        onMenuToggle = { showBatchMenu = !showBatchMenu },
                        onMenuDismiss = { showBatchMenu = false },
                        selectionMode = false,
                        selectedCount = 0,
                        onSelectAll = { setAllVisibleSelected(true) },
                        onOptions = {},
                        onCancelSelection = ::exitSelection
                    )
                }
            }
            if (displayPlaylists.isEmpty()) {
                item {
                    Text(
                        text = if (query.isBlank()) {
                            "No playlists yet. Long-press a song and choose \"New playlist\"."
                        } else {
                            "No playlists match \"$query\"."
                        },
                        color = SecondaryText,
                        style = MaterialTheme.typography.bodyMedium,
                        modifier = Modifier.padding(horizontal = Dimens.paddingLarge)
                    )
                }
            } else {
                items(displayPlaylists, key = { it.playlist.id }) { playlistWithTracks ->
                    val playlistKey = "p${playlistWithTracks.playlist.id}"
                    PlaylistCard(
                        playlist = playlistWithTracks.playlist,
                        tracks = playlistWithTracks.tracks,
                        selected = if (selectionMode) playlistKey in selectedKeys else null,
                        onClick = {
                            if (selectionMode) {
                                toggleSelection(playlistKey)
                            } else {
                                onOpenPlaylist(playlistWithTracks.playlist.id)
                            }
                        },
                        onLongClick = { beginSelection(playlistKey) },
                        onSync = {
                            if (playlistWithTracks.playlist.playlistUrl.isNotBlank()) {
                                scope.launch {
                                    app.downloadRepository.syncPlaylist(
                                        playlistId = playlistWithTracks.playlist.id,
                                        playlistName = playlistWithTracks.playlist.name,
                                        playlistUrl = playlistWithTracks.playlist.playlistUrl
                                    )
                                }
                            }
                        },
                        onStopSync = if (playlistHasActiveJobs(playlistWithTracks.playlist.id)) {
                            {
                                scope.launch {
                                    app.downloadRepository.cancelPlaylistSync(playlistWithTracks.playlist.id)
                                }
                            }
                        } else {
                            null
                        },
                        onUrlChanged = { urlDialogPlaylist = playlistWithTracks.playlist },
                        onClearPlaylist = { clearDialogPlaylist = playlistWithTracks.playlist },
                        onDelete = { deleteDialogPlaylist = playlistWithTracks.playlist },
                        modifier = Modifier.padding(horizontal = Dimens.paddingLarge)
                    )
                }
            }
        } else {
            // All-songs view — every track in the track card layout
            item {
                if (selectionMode) {
                    Text(
                        text = "Tracks (${sortedTracks.size})",
                        style = MaterialTheme.typography.titleMedium,
                        color = PrimaryText,
                        modifier = Modifier.padding(horizontal = Dimens.paddingLarge)
                    )
                } else {
                    BatchSectionHeader(
                        title = "Tracks (${sortedTracks.size})",
                        showMenu = showBatchMenu,
                        onMenuToggle = { showBatchMenu = !showBatchMenu },
                        onMenuDismiss = { showBatchMenu = false },
                        selectionMode = false,
                        selectedCount = 0,
                        onSelectAll = { setAllVisibleSelected(true) },
                        onOptions = {},
                        onCancelSelection = ::exitSelection
                    )
                }
            }
            if (sortedTracks.isEmpty()) {
                item {
                    Text(
                        text = if (query.isBlank()) {
                            "Import audio files or use Download to find music."
                        } else {
                            "No tracks match \"$query\"."
                        },
                        color = SecondaryText,
                        style = MaterialTheme.typography.bodyMedium,
                        modifier = Modifier.padding(horizontal = Dimens.paddingLarge)
                    )
                }
            } else {
                items(sortedTracks, key = { it.uri }) { track ->
                    TrackRowCard(
                        track = track,
                        selected = if (selectionMode) track.uri in selectedKeys else null,
                        onClick = {
                            if (selectionMode) {
                                toggleSelection(track.uri)
                            } else {
                                // Play the track within the current sorted
                                // (filtered) context — next/prev walk the
                                // visible list, like the desktop.
                                val index = sortedTracks.indexOfFirst { it.uri == track.uri }
                                onPlay(
                                    mediaTracks,
                                    index.coerceAtLeast(0),
                                    false
                                )
                            }
                        },
                        onMenuClick = { actionsTrack = track },
                        onLongClick = { beginSelection(track.uri) },
                        modifier = Modifier.padding(horizontal = Dimens.paddingLarge)
                    )
                }
            }
        }
    }

    // Track actions sheet — lives outside the LazyColumn so it overlays
    // the list instead of scrolling with it.
    actionsTrack?.let { track ->
        TrackActionsSheet(
            track = track,
            onDismiss = { actionsTrack = null }
        )
    }

    // URL edit dialog (menu action from any playlist card)
    urlDialogPlaylist?.let { playlist ->
        PlaylistUrlDialog(
            initialUrl = playlist.playlistUrl,
            onDismiss = { urlDialogPlaylist = null },
            onSave = { url ->
                scope.launch {
                    app.playlistRepository.updatePlaylistUrl(playlist.id, url)
                }
                urlDialogPlaylist = null
            }
        )
    }

    // Delete confirm dialog (menu action from any playlist card) — removing
    // a playlist never deletes songs from the phone.
    deleteDialogPlaylist?.let { playlist ->
        AlertDialog(
            onDismissRequest = { deleteDialogPlaylist = null },
            containerColor = SurfaceDark,
            titleContentColor = PrimaryText,
            textContentColor = SecondaryText,
            title = { Text("Delete playlist?") },
            text = { Text("\"${playlist.name}\" will be removed from the app. Tracks stay in your library and the audio files stay on your phone.") },
            confirmButton = {
                TextButton(onClick = {
                    scope.launch {
                        app.playlistRepository.deletePlaylist(playlist.id)
                    }
                    deleteDialogPlaylist = null
                }) {
                    Text("Delete", color = AccentGreen)
                }
            },
            dismissButton = {
                TextButton(onClick = { deleteDialogPlaylist = null }) {
                    Text("Cancel", color = SecondaryText)
                }
            }
        )
    }

    // Clear-playlist confirm dialog — metadata-only, songs stay on the phone.
    clearDialogPlaylist?.let { playlist ->
        AlertDialog(
            onDismissRequest = { clearDialogPlaylist = null },
            containerColor = SurfaceDark,
            titleContentColor = PrimaryText,
            textContentColor = SecondaryText,
            title = { Text("Clear playlist?") },
            text = { Text("All songs will be removed from \"${playlist.name}\". They stay in your library and the audio files stay on your phone.") },
            confirmButton = {
                TextButton(onClick = {
                    scope.launch {
                        app.playlistRepository.clearPlaylist(playlist.id)
                    }
                    clearDialogPlaylist = null
                }) {
                    Text("Clear", color = AccentGreen)
                }
            },
            dismissButton = {
                TextButton(onClick = { clearDialogPlaylist = null }) {
                    Text("Cancel", color = SecondaryText)
                }
            }
        )
    }

}

/**
 * Section header row with the 3-dot menu used to enter selection by selecting
 * all visible items. Once selection is active, BulkSelectionToolbar owns the
 * visible select-all checkbox and complete actions menu.
 */
@Composable
private fun BatchSectionHeader(
    title: String,
    showMenu: Boolean,
    onMenuToggle: () -> Unit,
    onMenuDismiss: () -> Unit,
    selectionMode: Boolean,
    selectedCount: Int,
    onSelectAll: () -> Unit,
    onOptions: () -> Unit,
    onCancelSelection: () -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(start = Dimens.paddingLarge, end = Dimens.paddingLarge),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Text(
            text = if (selectionMode) "$selectedCount selected" else title,
            style = MaterialTheme.typography.titleMedium,
            color = if (selectionMode) AccentGreen else PrimaryText,
            fontWeight = androidx.compose.ui.text.font.FontWeight.Bold,
            modifier = Modifier
                .weight(1f)
                .padding(vertical = Dimens.paddingSmall)
        )
        Box {
            IconButton(onClick = onMenuToggle) {
                Icon(
                    imageVector = Icons.Filled.MoreVert,
                    contentDescription = "Batch actions",
                    tint = SecondaryText
                )
            }
            DropdownMenu(
                expanded = showMenu,
                onDismissRequest = onMenuDismiss
            ) {
                DropdownMenuItem(
                    text = { Text("Select all", color = PrimaryText) },
                    onClick = {
                        onMenuDismiss()
                        onSelectAll()
                    }
                )
                if (selectionMode && selectedCount > 0) {
                    DropdownMenuItem(
                        text = { Text("Options", color = PrimaryText) },
                        onClick = {
                            onMenuDismiss()
                            onOptions()
                        }
                    )
                }
                if (selectionMode) {
                    DropdownMenuItem(
                        text = { Text("Cancel selection", color = PrimaryText) },
                        onClick = {
                            onMenuDismiss()
                            onCancelSelection()
                        }
                    )
                }
            }
        }
    }
}

@Composable
private fun PlaylistUrlDialog(
    initialUrl: String,
    onDismiss: () -> Unit,
    onSave: (String) -> Unit
) {
    var urlInput by rememberSaveable { mutableStateOf(initialUrl) }

    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = SurfaceDark,
        titleContentColor = PrimaryText,
        textContentColor = PrimaryText,
        title = { Text("YouTube playlist URL") },
        text = {
            OutlinedTextField(
                value = urlInput,
                onValueChange = { urlInput = it },
                modifier = Modifier.fillMaxWidth(),
                placeholder = { Text("https://www.youtube.com/playlist?list=...", color = SecondaryText) },
                singleLine = true,
                colors = OutlinedTextFieldDefaults.colors(
                    focusedTextColor = PrimaryText,
                    unfocusedTextColor = PrimaryText,
                    cursorColor = AccentGreen,
                    focusedBorderColor = AccentGreen,
                    unfocusedBorderColor = SurfaceElevated,
                    focusedContainerColor = SurfaceDark,
                    unfocusedContainerColor = SurfaceDark
                )
            )
        },
        confirmButton = {
            TextButton(onClick = { onSave(urlInput.trim()) }) {
                Text("Save", color = AccentGreen)
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text("Cancel", color = SecondaryText)
            }
        }
    )
}

private fun Track.toMediaTrack(): MediaTrack = MediaTrack(
    uri = uri,
    title = title,
    artist = artist,
    album = album,
    durationMs = durationMs,
    artworkUri = albumArtUri()
)
