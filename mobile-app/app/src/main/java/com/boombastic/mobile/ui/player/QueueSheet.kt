package com.boombastic.mobile.ui.player

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.DragHandle
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
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
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import com.boombastic.mobile.playback.MediaTrack
import com.boombastic.mobile.playback.MusicController
import com.boombastic.mobile.ui.components.ArtworkImage
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText
import com.boombastic.mobile.ui.theme.SurfaceDark
import com.boombastic.mobile.ui.theme.SurfaceElevated
import kotlin.math.roundToInt

/**
 * "Playing Next" bottom sheet backed by the live playback queue from
 * [MusicController.getQueue].  Rows show artwork thumbnails and support
 * long-press drag-to-reorder via [MusicController.moveQueueItem].
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun QueueSheet(
    musicController: MusicController,
    onDismiss: () -> Unit
) {
    val currentTrack by musicController.currentTrack.collectAsState()
    var queueItems by remember { mutableStateOf(musicController.getQueue()) }

    // Refresh the snapshot whenever the playing item changes (queue edits
    // or track auto-advance).
    LaunchedEffect(currentTrack?.uri) {
        queueItems = musicController.getQueue()
    }

    var draggingIndex by remember { mutableStateOf<Int?>(null) }
    var dragOffsetY by remember { mutableFloatStateOf(0f) }
    val rowHeightPx = with(LocalDensity.current) { QUEUE_ROW_HEIGHT.toPx() }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        containerColor = SurfaceDark
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = Dimens.paddingLarge)
        ) {
            Text(
                text = "Playing Next",
                style = MaterialTheme.typography.headlineMedium,
                color = PrimaryText
            )
            Spacer(modifier = Modifier.height(Dimens.paddingMedium))

            if (queueItems.isEmpty()) {
                Text(
                    text = "Queue is empty",
                    style = MaterialTheme.typography.bodyMedium,
                    color = SecondaryText,
                    modifier = Modifier.padding(vertical = Dimens.paddingMedium)
                )
            } else {
                LazyColumn(
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(max = 400.dp)
                ) {
                    itemsIndexed(queueItems, key = { index, _ -> index }) { index, track ->
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

            Spacer(modifier = Modifier.height(Dimens.paddingXLarge))
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
            placeholderIconSize = 20.dp
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
