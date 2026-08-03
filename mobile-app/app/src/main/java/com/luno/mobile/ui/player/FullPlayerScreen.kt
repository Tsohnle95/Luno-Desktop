package com.luno.mobile.ui.player

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.PauseCircle
import androidx.compose.material.icons.filled.PlayCircle
import androidx.compose.material.icons.filled.QueueMusic
import androidx.compose.material.icons.filled.Repeat
import androidx.compose.material.icons.filled.Shuffle
import androidx.compose.material.icons.filled.SkipNext
import androidx.compose.material.icons.filled.SkipPrevious
import androidx.compose.material3.Badge
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.media3.common.Player
import com.luno.mobile.playback.MusicController
import com.luno.mobile.ui.components.ArtworkImage
import com.luno.mobile.ui.components.rememberArtworkColors
import com.luno.mobile.ui.theme.AccentGreen
import com.luno.mobile.ui.theme.Dimens
import com.luno.mobile.ui.theme.MiniPlayerBorder
import com.luno.mobile.ui.theme.PrimaryBackground
import com.luno.mobile.ui.theme.PrimaryText
import com.luno.mobile.ui.theme.SecondaryText

/**
 * Full-screen player: large artwork (real embedded artwork with a
 * dominant-color gradient backdrop), title/artist, scrub bar with m:ss
 * time labels, transport row (shuffle / previous / play-pause / next /
 * repeat), queue sheet trigger, and action-sheet trigger.
 */
@Composable
fun FullPlayerScreen(
    musicController: MusicController,
    onBack: () -> Unit
) {
    val isPlaying by musicController.isPlaying.collectAsState()
    val currentTrack by musicController.currentTrack.collectAsState()
    val progress by musicController.progress.collectAsState()
    val duration by musicController.duration.collectAsState()
    val repeatMode by musicController.repeatMode.collectAsState()
    val shuffleEnabled by musicController.shuffleEnabled.collectAsState()

    var showQueueSheet by rememberSaveable { mutableStateOf(false) }
    var showActionSheet by rememberSaveable { mutableStateOf(false) }

    val artworkUri = currentTrack?.artworkUri
    val (gradientTop, gradientBottom) = rememberArtworkColors(artworkUri)

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(
                Brush.verticalGradient(
                    colors = listOf(gradientTop, gradientBottom, PrimaryBackground)
                )
            )
    ) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = Dimens.paddingLarge),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            // Top bar: back arrow, queue, action sheet
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(vertical = Dimens.paddingSmall),
                verticalAlignment = Alignment.CenterVertically
            ) {
                IconButton(onClick = onBack) {
                    Icon(
                        imageVector = Icons.AutoMirrored.Filled.ArrowBack,
                        contentDescription = "Back",
                        tint = PrimaryText,
                        modifier = Modifier.size(Dimens.iconSize)
                    )
                }
                Spacer(modifier = Modifier.weight(1f))
                IconButton(onClick = { showQueueSheet = true }) {
                    Icon(
                        imageVector = Icons.Filled.QueueMusic,
                        contentDescription = "Queue",
                        tint = PrimaryText,
                        modifier = Modifier.size(Dimens.iconSize)
                    )
                }
                IconButton(onClick = { showActionSheet = true }) {
                    Icon(
                        imageVector = Icons.Filled.MoreVert,
                        contentDescription = "More options",
                        tint = PrimaryText,
                        modifier = Modifier.size(Dimens.iconSize)
                    )
                }
            }

            Spacer(modifier = Modifier.weight(1f))

            // Artwork: real embedded artwork, gradient placeholder fallback
            ArtworkImage(
                artworkUri = artworkUri,
                modifier = Modifier
                    .size(Dimens.albumArtLarge)
                    .clip(RoundedCornerShape(Dimens.cornerMedium)),
                placeholderIconSize = 96.dp
            )

            Spacer(modifier = Modifier.height(Dimens.paddingXLarge))

            // Title / artist
            Text(
                text = currentTrack?.title ?: "Unknown Track",
                style = MaterialTheme.typography.headlineMedium,
                color = PrimaryText,
                textAlign = TextAlign.Center,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
            Spacer(modifier = Modifier.height(Dimens.paddingMedium))
            Text(
                text = currentTrack?.artist ?: "Unknown Artist",
                style = MaterialTheme.typography.bodyMedium,
                color = SecondaryText,
                textAlign = TextAlign.Center,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )

            Spacer(modifier = Modifier.weight(1f))

            // Scrub bar with m:ss time labels
            val maxDuration = duration.coerceAtLeast(0L).toFloat()
            var sliderPosition by rememberSaveable(currentTrack?.uri) {
                mutableStateOf(0f)
            }
            var isScrubbing by rememberSaveable(currentTrack?.uri) {
                mutableStateOf(false)
            }
            val progressTarget = progress.toFloat().coerceIn(0f, maxDuration)
            // The controller samples Media3 every 250 ms. Interpolate those
            // samples locally so the thumb and elapsed time move every frame
            // instead of visibly stepping between controller updates.
            val animatedProgress = key(currentTrack?.uri) {
                val value by animateFloatAsState(
                    targetValue = progressTarget,
                    animationSpec = tween(
                        durationMillis = PROGRESS_SAMPLE_INTERVAL_MS,
                        easing = LinearEasing
                    ),
                    label = "playbackProgress"
                )
                value
            }
            val scrubValue = if (isScrubbing) sliderPosition else animatedProgress
            Slider(
                value = scrubValue,
                onValueChange = {
                    sliderPosition = it
                    isScrubbing = true
                },
                onValueChangeFinished = {
                    musicController.seekTo(sliderPosition.toLong())
                    isScrubbing = false
                },
                valueRange = 0f..maxDuration.coerceAtLeast(1f),
                colors = SliderDefaults.colors(
                    thumbColor = AccentGreen,
                    activeTrackColor = AccentGreen,
                    inactiveTrackColor = MiniPlayerBorder
                )
            )
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween
            ) {
                Text(
                    text = formatTime(scrubValue.toLong()),
                    style = MaterialTheme.typography.bodySmall,
                    color = SecondaryText
                )
                Text(
                    text = formatTime(duration.coerceAtLeast(0L)),
                    style = MaterialTheme.typography.bodySmall,
                    color = SecondaryText
                )
            }

            Spacer(modifier = Modifier.height(Dimens.paddingLarge))

            // Transport row: shuffle, previous, play/pause, next, repeat
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceEvenly,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    IconButton(
                        onClick = { musicController.toggleShuffle() },
                        modifier = Modifier.size(Dimens.touchTargetMin)
                    ) {
                        Icon(
                            imageVector = Icons.Filled.Shuffle,
                            contentDescription = "Shuffle",
                            tint = if (shuffleEnabled) AccentGreen else SecondaryText,
                            modifier = Modifier.size(Dimens.iconSizeSmall)
                        )
                    }
                    if (shuffleEnabled) {
                        Box(
                            modifier = Modifier
                                .width(Dimens.iconSizeSmall)
                                .height(2.dp)
                                .background(AccentGreen)
                        )
                    }
                }
                IconButton(
                    onClick = { musicController.skipToPrevious() },
                    modifier = Modifier.size(Dimens.touchTargetMin)
                ) {
                    Icon(
                        imageVector = Icons.Filled.SkipPrevious,
                        contentDescription = "Previous",
                        tint = PrimaryText,
                        modifier = Modifier.size(Dimens.iconSizeMedium)
                    )
                }
                Box(
                    modifier = Modifier
                        .size(56.dp)
                        .clip(CircleShape)
                        .clickable { musicController.togglePlayPause() },
                    contentAlignment = Alignment.Center
                ) {
                    Icon(
                        imageVector = if (isPlaying) {
                            Icons.Filled.PauseCircle
                        } else {
                            Icons.Filled.PlayCircle
                        },
                        contentDescription = if (isPlaying) "Pause" else "Play",
                        tint = PrimaryText,
                        modifier = Modifier.size(56.dp)
                    )
                }
                IconButton(
                    onClick = { musicController.skipToNext() },
                    modifier = Modifier.size(Dimens.touchTargetMin)
                ) {
                    Icon(
                        imageVector = Icons.Filled.SkipNext,
                        contentDescription = "Next",
                        tint = PrimaryText,
                        modifier = Modifier.size(Dimens.iconSizeMedium)
                    )
                }
                IconButton(
                    onClick = { musicController.toggleRepeatMode() },
                    modifier = Modifier.size(Dimens.touchTargetMin)
                ) {
                    Box {
                        Icon(
                            imageVector = Icons.Filled.Repeat,
                            contentDescription = "Repeat",
                            tint = if (repeatMode != Player.REPEAT_MODE_OFF) {
                                AccentGreen
                            } else {
                                SecondaryText
                            },
                            modifier = Modifier.size(Dimens.iconSizeSmall)
                        )
                        if (repeatMode == Player.REPEAT_MODE_ONE) {
                            Badge(
                                containerColor = AccentGreen,
                                contentColor = PrimaryBackground,
                                modifier = Modifier.align(Alignment.TopEnd)
                            ) {
                                Text(
                                    text = "1",
                                    style = MaterialTheme.typography.labelSmall
                                )
                            }
                        }
                    }
                }
            }

            Spacer(modifier = Modifier.height(Dimens.paddingXLarge))
        }
    }

    if (showQueueSheet) {
        QueueSheet(
            musicController = musicController,
            onDismiss = { showQueueSheet = false }
        )
    }
    if (showActionSheet) {
        PlayerActionSheet(
            musicController = musicController,
            onDismiss = { showActionSheet = false }
        )
    }
}

private const val PROGRESS_SAMPLE_INTERVAL_MS = 250

/**
 * Formats milliseconds as `m:ss`, matching the desktop `utils.py format_time`
 * and the Flet prototype's `format_time_ms`.
 */
internal fun formatTime(ms: Long): String {
    val seconds = (ms.coerceAtLeast(0L)) / 1000
    val minutes = seconds / 60
    val remainingSeconds = seconds % 60
    return "$minutes:${remainingSeconds.toString().padStart(2, '0')}"
}
