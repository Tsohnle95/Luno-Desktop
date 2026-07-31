package com.boombastic.mobile.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.text.style.TextOverflow
import com.boombastic.mobile.playback.MusicController
import com.boombastic.mobile.ui.theme.AccentGreen
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.MiniPlayerBorder
import com.boombastic.mobile.ui.theme.MiniPlayerSurface
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText

@Composable
fun MiniPlayer(
    musicController: MusicController,
    onMiniPlayerTap: () -> Unit = {}
) {
    val isPlaying by musicController.isPlaying.collectAsState()
    val currentTrack by musicController.currentTrack.collectAsState()
    val progress by musicController.progress.collectAsState()
    val duration by musicController.duration.collectAsState()

    if (currentTrack == null) return

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(MiniPlayerSurface)
    ) {
        // Progress bar
        val progressFraction = if (duration > 0) progress.toFloat() / duration.toFloat() else 0f
        LinearProgressIndicator(
            progress = { progressFraction },
            modifier = Modifier
                .fillMaxWidth()
                .height(Dimens.progressBarHeight),
            color = AccentGreen,
            trackColor = MiniPlayerBorder,
            strokeCap = StrokeCap.Round,
        )

        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(Dimens.miniPlayerHeight)
                .padding(horizontal = Dimens.paddingMedium),
            verticalAlignment = Alignment.CenterVertically
        ) {
            // Track info (tap to open the full player)
            Column(
                modifier = Modifier
                    .weight(1f)
                    .clickable(onClick = onMiniPlayerTap)
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
