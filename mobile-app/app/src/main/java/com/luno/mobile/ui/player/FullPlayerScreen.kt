package com.luno.mobile.ui.player

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
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
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.PauseCircle
import androidx.compose.material.icons.filled.PlayCircle
import androidx.compose.material.icons.filled.QueueMusic
import androidx.compose.material.icons.filled.Repeat
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Shuffle
import androidx.compose.material.icons.filled.SkipNext
import androidx.compose.material.icons.filled.SkipPrevious
import androidx.compose.material3.Badge
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.media3.common.Player
import com.luno.mobile.LunoApp
import com.luno.mobile.data.discovery.LastfmResult
import com.luno.mobile.data.discovery.LastfmTrack
import com.luno.mobile.playback.MediaTrack
import com.luno.mobile.playback.MusicController
import com.luno.mobile.ui.components.ArtworkImage
import com.luno.mobile.ui.components.rememberArtworkColors
import com.luno.mobile.ui.theme.AccentGreen
import com.luno.mobile.ui.theme.Dimens
import com.luno.mobile.ui.theme.MiniPlayerBorder
import com.luno.mobile.ui.theme.PrimaryBackground
import com.luno.mobile.ui.theme.PrimaryText
import com.luno.mobile.ui.theme.SecondaryText
import kotlin.math.roundToInt
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

private const val FULL_PLAYER_RECOMMENDATION_LIMIT = 6

private sealed interface FullPlayerRecommendationState {
    data object Idle : FullPlayerRecommendationState
    data class Loading(val seedUri: String) : FullPlayerRecommendationState
    data class Ready(val seedUri: String, val tracks: List<LastfmTrack>) : FullPlayerRecommendationState
    data class Error(val seedUri: String, val message: String) : FullPlayerRecommendationState
}

private fun recommendationKey(track: LastfmTrack): String =
    "${track.artist.lowercase()}|${track.title.lowercase()}"

/**
 * Full-screen player: large artwork (real embedded artwork with a
 * dominant-color gradient backdrop), title/artist, scrub bar with m:ss
 * time labels, transport row (shuffle / previous / play-pause / next /
 * repeat), queue/action-sheet triggers, and a scrollable recommendation
 * section seeded from the current track.
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
    val context = LocalContext.current
    val app = context.applicationContext as LunoApp
    val apiKey by app.discoveryRepository.apiKey.collectAsState()
    val allTracks by app.libraryData.tracks.collectAsState()

    var showQueueSheet by rememberSaveable { mutableStateOf(false) }
    var showActionSheet by rememberSaveable { mutableStateOf(false) }
    var recommendationRefresh by rememberSaveable { mutableIntStateOf(0) }
    var recommendationState by remember {
        mutableStateOf<FullPlayerRecommendationState>(FullPlayerRecommendationState.Idle)
    }
    var fallbackArtwork by remember { mutableStateOf<Map<String, String>>(emptyMap()) }
    var fallbackArtworkRequested by remember { mutableStateOf<Set<String>>(emptySet()) }
    val recommendationScope = rememberCoroutineScope()

    val artworkUri = currentTrack?.artworkUri
    val (gradientTop, gradientBottom) = rememberArtworkColors(artworkUri)

    LaunchedEffect(currentTrack?.uri, apiKey, recommendationRefresh) {
        val track = currentTrack
        if (track == null || apiKey.isNullOrBlank()) {
            recommendationState = FullPlayerRecommendationState.Idle
            return@LaunchedEffect
        }

        recommendationState = FullPlayerRecommendationState.Loading(track.uri)
        val result = try {
            app.discoveryRepository.getSimilar(
                artist = track.artist,
                title = track.title,
                limit = FULL_PLAYER_RECOMMENDATION_LIMIT,
                libraryTracks = allTracks
            )
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Exception) {
            LastfmResult.Failure(error.message ?: "Could not load recommendations")
        }
        recommendationState = when (result) {
            is LastfmResult.Success -> FullPlayerRecommendationState.Ready(
                seedUri = track.uri,
                tracks = result.tracks.take(FULL_PLAYER_RECOMMENDATION_LIMIT)
            )
            is LastfmResult.Failure -> FullPlayerRecommendationState.Error(
                seedUri = track.uri,
                message = result.message
            )
        }
    }

    val readyRecommendations = (recommendationState as? FullPlayerRecommendationState.Ready)
        ?.takeIf { it.seedUri == currentTrack?.uri }
        ?.tracks
        .orEmpty()

    fun requestFallbackArtwork(track: LastfmTrack) {
        val key = recommendationKey(track)
        if (key in fallbackArtworkRequested) return
        fallbackArtworkRequested = fallbackArtworkRequested + key
        recommendationScope.launch {
            app.recommendationArtworkService.findArtwork(track.artist, track.title)?.let { url ->
                fallbackArtwork = fallbackArtwork + (key to url)
            }
        }
    }

    LaunchedEffect(readyRecommendations) {
        fallbackArtwork = emptyMap()
        fallbackArtworkRequested = emptySet()
        readyRecommendations
            .filter { it.imageUrl.isNullOrBlank() }
            .forEach(::requestFallbackArtwork)
    }

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

            // Keep the artwork gradient on the viewport while this content
            // list moves over it.
            LazyColumn(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f),
                contentPadding = PaddingValues(
                    top = Dimens.paddingLarge,
                    bottom = Dimens.paddingXLarge
                ),
                horizontalAlignment = Alignment.CenterHorizontally
            ) {
                item(key = "player-controls", contentType = "player-controls") {
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

            Spacer(modifier = Modifier.height(Dimens.paddingLarge))

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
            item(key = "recommendations", contentType = "recommendations") {
                FullPlayerRecommendations(
                    currentTrack = currentTrack,
                    apiKeyConfigured = !apiKey.isNullOrBlank(),
                    state = recommendationState,
                    fallbackArtwork = fallbackArtwork,
                    onRefresh = { recommendationRefresh++ },
                    onArtworkError = ::requestFallbackArtwork
                )
            }
        }
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

@Composable
private fun FullPlayerRecommendations(
    currentTrack: MediaTrack?,
    apiKeyConfigured: Boolean,
    state: FullPlayerRecommendationState,
    fallbackArtwork: Map<String, String>,
    onRefresh: () -> Unit,
    onArtworkError: (LastfmTrack) -> Unit
) {
    val matchingState: FullPlayerRecommendationState = currentTrack?.let { track ->
        when (val value = state) {
            FullPlayerRecommendationState.Idle -> value
            is FullPlayerRecommendationState.Loading ->
                value.takeIf { it.seedUri == track.uri } ?: FullPlayerRecommendationState.Idle
            is FullPlayerRecommendationState.Ready ->
                value.takeIf { it.seedUri == track.uri } ?: FullPlayerRecommendationState.Idle
            is FullPlayerRecommendationState.Error ->
                value.takeIf { it.seedUri == track.uri } ?: FullPlayerRecommendationState.Idle
        }
    } ?: FullPlayerRecommendationState.Idle

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(top = Dimens.paddingSmall)
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Text(
                text = "Recommended for this song",
                style = MaterialTheme.typography.titleLarge,
                color = PrimaryText,
                modifier = Modifier.weight(1f)
            )
            if (apiKeyConfigured && currentTrack != null) {
                IconButton(onClick = onRefresh) {
                    Icon(
                        imageVector = Icons.Filled.Refresh,
                        contentDescription = "Refresh recommendations",
                        tint = AccentGreen
                    )
                }
            }
        }

        when {
            currentTrack == null -> RecommendationMessage("Play a song to see recommendations")
            !apiKeyConfigured -> RecommendationMessage(
                "Add a Last.fm API key in Settings to see recommendations"
            )
            matchingState is FullPlayerRecommendationState.Loading ||
                matchingState is FullPlayerRecommendationState.Idle -> {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(vertical = Dimens.paddingLarge),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(Dimens.iconSizeSmall),
                        color = AccentGreen,
                        strokeWidth = 2.dp
                    )
                    Text(
                        text = "Finding similar songs...",
                        style = MaterialTheme.typography.bodyMedium,
                        color = SecondaryText,
                        modifier = Modifier.padding(start = Dimens.paddingMedium)
                    )
                }
            }
            matchingState is FullPlayerRecommendationState.Error -> {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(vertical = Dimens.paddingMedium)
                ) {
                    Text(
                        text = matchingState.message,
                        style = MaterialTheme.typography.bodyMedium,
                        color = SecondaryText
                    )
                    TextButton(onClick = onRefresh) {
                        Text("Retry", color = AccentGreen)
                    }
                }
            }
            matchingState is FullPlayerRecommendationState.Ready -> {
                if (matchingState.tracks.isEmpty()) {
                    RecommendationMessage("No recommendations available for this song")
                } else {
                    Text(
                        text = "${matchingState.tracks.size} similar songs",
                        style = MaterialTheme.typography.bodySmall,
                        color = SecondaryText,
                        modifier = Modifier.padding(bottom = Dimens.paddingSmall)
                    )
                    matchingState.tracks.forEach { recommendation ->
                        FullPlayerRecommendationRow(
                            recommendation = recommendation,
                            artworkUri = fallbackArtwork[recommendationKey(recommendation)]
                                ?: recommendation.imageUrl,
                            onArtworkError = {
                                onArtworkError(recommendation)
                            }
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun FullPlayerRecommendationRow(
    recommendation: LastfmTrack,
    artworkUri: String?,
    onArtworkError: () -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = Dimens.paddingSmall),
        verticalAlignment = Alignment.CenterVertically
    ) {
        ArtworkImage(
            artworkUri = artworkUri,
            modifier = Modifier
                .size(Dimens.albumArtSmall)
                .clip(RoundedCornerShape(Dimens.cornerMedium)),
            placeholderIconSize = 24.dp,
            decodeSizePx = 192,
            onError = if (artworkUri == recommendation.imageUrl) onArtworkError else null
        )
        Spacer(modifier = Modifier.width(Dimens.paddingMedium))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = recommendation.title,
                style = MaterialTheme.typography.titleSmall,
                color = PrimaryText,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
            Text(
                text = "${recommendation.artist} - " +
                    "${(recommendation.match * 100).roundToInt()}% match",
                style = MaterialTheme.typography.bodySmall,
                color = SecondaryText,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
        }
    }
}

@Composable
private fun RecommendationMessage(message: String) {
    Text(
        text = message,
        style = MaterialTheme.typography.bodyMedium,
        color = SecondaryText,
        modifier = Modifier.padding(vertical = Dimens.paddingMedium)
    )
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
