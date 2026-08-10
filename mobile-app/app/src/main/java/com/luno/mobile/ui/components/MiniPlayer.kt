package com.luno.mobile.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.luno.mobile.playback.MusicController
import com.luno.mobile.ui.theme.AccentGreen
import com.luno.mobile.ui.theme.Dimens
import com.luno.mobile.ui.theme.MiniPlayerBorder
import com.luno.mobile.ui.theme.MiniPlayerSurface
import com.luno.mobile.ui.theme.PrimaryText
import com.luno.mobile.ui.theme.SecondaryText

@Composable
fun MiniPlayer(
    musicController: MusicController,
    onMiniPlayerTap: () -> Unit = {},
    onNext: () -> Unit = { musicController.skipToNext() },
    onPrevious: () -> Unit = { musicController.skipToPrevious() }
) {
    val isPlaying by musicController.isPlaying.collectAsState()
    val currentTrack by musicController.currentTrack.collectAsState()
    val progress by musicController.progress.collectAsState()
    val duration by musicController.duration.collectAsState()
    val swipeThresholdPx = with(LocalDensity.current) { 64.dp.toPx() }
    val (gradientTop, gradientBottom) = rememberArtworkColors(currentTrack?.artworkUri)

    if (currentTrack == null) return

    val progressFraction = if (duration > 0L) {
        (progress.toFloat() / duration.toFloat()).coerceIn(0f, 1f)
    } else {
        0f
    }

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(MiniPlayerSurface)
            .background(
                Brush.verticalGradient(
                    colors = listOf(
                        gradientTop.copy(alpha = 0.42f),
                        gradientBottom.copy(alpha = 0.28f)
                    )
                )
            )
            .pointerInput(musicController, swipeThresholdPx) {
                var horizontalDrag = 0f
                detectHorizontalDragGestures(
                    onHorizontalDrag = { change, dragAmount ->
                        change.consume()
                        horizontalDrag += dragAmount
                    },
                    onDragEnd = {
                        when {
                            horizontalDrag <= -swipeThresholdPx -> onNext()
                            horizontalDrag >= swipeThresholdPx -> onPrevious()
                        }
                        horizontalDrag = 0f
                    },
                    onDragCancel = { horizontalDrag = 0f }
                )
            }
    ) {
        LinearProgressIndicator(
            progress = { progressFraction },
            modifier = Modifier
                .fillMaxWidth()
                .height(2.dp),
            color = AccentGreen,
            trackColor = MiniPlayerBorder.copy(alpha = 0.7f),
            strokeCap = StrokeCap.Round
        )

        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(Dimens.miniPlayerHeight)
                .padding(horizontal = Dimens.paddingMedium),
            verticalAlignment = Alignment.CenterVertically
        ) {
            // Artwork thumbnail
            RecommendationArtworkImage(
                artist = currentTrack?.artist.orEmpty(),
                title = currentTrack?.title.orEmpty(),
                artworkUri = currentTrack?.artworkUri,
                modifier = Modifier
                    .size(Dimens.albumArtSmall)
                    .clip(RoundedCornerShape(Dimens.cornerSmall)),
                placeholderIconSize = 24.dp,
                decodeSizePx = 192
            )

            Spacer(modifier = Modifier.width(Dimens.paddingMedium))

            // Track info (tap to open the full player)
            Column(
                modifier = Modifier
                    .weight(1f)
                    .clickable(
                        interactionSource = remember { MutableInteractionSource() },
                        indication = null,
                        onClick = onMiniPlayerTap
                    )
            ) {
                Text(
                    text = currentTrack?.title ?: "Unknown Track",
                    style = MaterialTheme.typography.titleSmall,
                    color = PrimaryText,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis
                )
                Text(
                    text = buildString {
                        append(currentTrack?.artist ?: "Unknown Artist")
                        val album = currentTrack?.album
                        if (!album.isNullOrBlank()) {
                            append(" · ")
                            append(album)
                        }
                    },
                    style = MaterialTheme.typography.bodySmall,
                    color = SecondaryText,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis
                )
            }

            Spacer(modifier = Modifier.width(Dimens.paddingMedium))

            // Play/Pause button
            IconButton(
                onClick = { musicController.togglePlayPause() },
                modifier = Modifier.size(Dimens.touchTargetMin)
            ) {
                Icon(
                    imageVector = if (isPlaying) Icons.Default.Pause else Icons.Default.PlayArrow,
                    contentDescription = if (isPlaying) "Pause" else "Play",
                    tint = PrimaryText,
                    modifier = Modifier.size(Dimens.iconSizeMedium)
                )
            }
        }
    }
}

/** Height occupied above the navigation bar when the mini-player is shown. */
val MiniPlayerOverlayHeight = Dimens.miniPlayerHeight + 2.dp
