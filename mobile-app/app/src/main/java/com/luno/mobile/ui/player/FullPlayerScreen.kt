package com.luno.mobile.ui.player

import android.widget.Toast
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
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.PlaylistAdd
import androidx.compose.material.icons.automirrored.filled.QueueMusic
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.PauseCircle
import androidx.compose.material.icons.filled.PlayCircle
import androidx.compose.material.icons.filled.Repeat
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.filled.StarBorder
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
import androidx.compose.runtime.withFrameNanos
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
import com.luno.mobile.data.db.entity.Playlist
import com.luno.mobile.data.discovery.LastfmResult
import com.luno.mobile.data.discovery.LastfmTrack
import com.luno.mobile.playback.MediaTrack
import com.luno.mobile.playback.MusicController
import com.luno.mobile.playback.RecommendationPreviewManager
import com.luno.mobile.playback.RecommendationPreviewState
import com.luno.mobile.playback.RecommendationSaveOutcome
import com.luno.mobile.ui.components.ArtworkImage
import com.luno.mobile.ui.components.PlaylistPickerSheet
import com.luno.mobile.ui.components.rememberArtworkColors
import com.luno.mobile.ui.create.CreatePlaylistSheet
import com.luno.mobile.ui.theme.AccentGreen
import com.luno.mobile.ui.theme.Dimens
import com.luno.mobile.ui.theme.MiniPlayerBorder
import com.luno.mobile.ui.theme.PrimaryBackground
import com.luno.mobile.ui.theme.PrimaryText
import com.luno.mobile.ui.theme.SecondaryText
import kotlin.math.roundToInt
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

private const val FULL_PLAYER_RECOMMENDATION_LIMIT = 1_000

private sealed interface FullPlayerRecommendationState {
    data object Idle : FullPlayerRecommendationState
    data class Loading(val seedUri: String) : FullPlayerRecommendationState
    data class Ready(val seedUri: String, val tracks: List<LastfmTrack>) : FullPlayerRecommendationState
    data class Error(val seedUri: String, val message: String) : FullPlayerRecommendationState
}

private data class FullPlayerScrollPosition(
    val index: Int,
    val offset: Int
)

private fun recommendationKey(track: LastfmTrack): String =
    RecommendationPreviewManager.recommendationKey(track)

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
    val previewState by app.recommendationPreviewManager.state.collectAsState()
    val discoverMode = previewState.discoverMode

    var showQueueSheet by rememberSaveable { mutableStateOf(false) }
    var showActionSheet by rememberSaveable { mutableStateOf(false) }
    var recommendationRefresh by rememberSaveable { mutableIntStateOf(0) }
    var recommendationState by remember {
        mutableStateOf<FullPlayerRecommendationState>(FullPlayerRecommendationState.Idle)
    }
    var fallbackArtwork by remember { mutableStateOf<Map<String, String>>(emptyMap()) }
    var fallbackArtworkRequested by remember { mutableStateOf<Set<String>>(emptySet()) }
    var recommendationToSaveKey by rememberSaveable { mutableStateOf<String?>(null) }
    var recommendationForNewPlaylistKey by rememberSaveable { mutableStateOf<String?>(null) }
    val recommendationScope = rememberCoroutineScope()
    val playerListState = rememberLazyListState()
    var scrollRestorePosition by remember { mutableStateOf<FullPlayerScrollPosition?>(null) }
    val currentLibraryTrack = remember(allTracks, currentTrack?.uri) {
        currentTrack?.uri?.let { uri -> allTracks.firstOrNull { it.uri == uri } }
    }
    val isFavorite = currentLibraryTrack?.isFavorite == true
    val canFavorite = currentLibraryTrack != null && currentTrack?.isTransient != true

    val artworkUri = currentTrack?.artworkUri ?: currentLibraryTrack?.albumArtUri()

    val recommendationSeedUri = if (previewState.active || currentTrack?.isTransient == true) {
        previewState.seedUri
    } else {
        currentTrack?.uri
    }

    LaunchedEffect(
        recommendationSeedUri,
        apiKey,
        allTracks,
        recommendationRefresh,
        previewState.active
    ) {
        if (previewState.active || currentTrack?.isTransient == true) return@LaunchedEffect
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
                tracks = result.tracks.distinctBy(::recommendationKey)
            )
            is LastfmResult.Failure -> FullPlayerRecommendationState.Error(
                seedUri = track.uri,
                message = result.message
            )
        }
    }

    val fetchedRecommendations = (recommendationState as? FullPlayerRecommendationState.Ready)
        ?.takeIf { it.seedUri == recommendationSeedUri }
        ?.tracks
        .orEmpty()
    val managedRecommendations = previewState.recommendations.takeIf {
        it.isNotEmpty() && previewState.seedUri == recommendationSeedUri
    }
    val readyRecommendations = managedRecommendations ?: fetchedRecommendations
    val recommendationLoading = !previewState.active &&
        recommendationState is FullPlayerRecommendationState.Loading
    val selectedRecommendation = previewState.selectedKey?.let { selectedKey ->
        readyRecommendations.firstOrNull { recommendationKey(it) == selectedKey }
    }
    val previewIsLoading = previewState.active &&
        previewState.currentKey == null &&
        selectedRecommendation != null
    val displayedCurrentTrack = currentTrack
    val currentMetadataMissing = displayedCurrentTrack == null ||
        displayedCurrentTrack.title.equals("Unknown", ignoreCase = true) ||
        displayedCurrentTrack.artist.equals("Unknown", ignoreCase = true)
    val useRecommendationMetadata = selectedRecommendation != null &&
        (previewIsLoading || (displayedCurrentTrack?.isTransient == true && currentMetadataMissing))
    val displayTitle = if (useRecommendationMetadata) {
        selectedRecommendation?.title
    } else {
        currentTrack?.title
    } ?: "Unknown Track"
    val displayArtist = if (useRecommendationMetadata) {
        selectedRecommendation?.artist
    } else {
        currentTrack?.artist
    } ?: "Unknown Artist"
    val pendingArtworkUri = selectedRecommendation?.let { recommendation ->
        fallbackArtwork[recommendationKey(recommendation)] ?: recommendation.imageUrl
    }
    val displayArtworkUri = if (selectedRecommendation != null && (
            currentTrack == null ||
                (currentMetadataMissing && artworkUri.isNullOrBlank()) ||
                (displayedCurrentTrack?.isTransient == true && artworkUri.isNullOrBlank())
            )
    ) {
        pendingArtworkUri
    } else {
        artworkUri
    }
    val previewStatus = if (previewIsLoading) {
        val selectedKey = previewState.selectedKey
        when {
            selectedKey in previewState.resolvingKeys -> "Retrieving song data..."
            selectedKey in previewState.preparingKeys -> "Downloading song..."
            selectedKey in previewState.waitingKeys -> "Preparing recommended song..."
            else -> "Preparing recommended song..."
        }
    } else {
        null
    }
    val (gradientTop, gradientBottom) = rememberArtworkColors(displayArtworkUri)

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
            .take(20)
            .forEach(::requestFallbackArtwork)
    }

    LaunchedEffect(
        previewState.active,
        previewState.selectedKey,
        previewState.discoverMode,
        recommendationSeedUri,
        readyRecommendations,
        recommendationState
    ) {
        val position = scrollRestorePosition ?: return@LaunchedEffect
        if (!previewState.active && (currentTrack == null ||
                currentTrack?.isTransient == true || recommendationLoading)
        ) {
            return@LaunchedEffect
        }
        withFrameNanos { }
        val itemCount = playerListState.layoutInfo.totalItemsCount
        if (itemCount == 0) return@LaunchedEffect
        playerListState.scrollToItem(
            index = position.index.coerceIn(0, itemCount - 1),
            scrollOffset = position.offset
        )
        scrollRestorePosition = null
    }

    LaunchedEffect(
        recommendationSeedUri,
        readyRecommendations,
        previewState.active,
    ) {
        if (!previewState.active && recommendationSeedUri != null) {
            app.recommendationPreviewManager.prefetchRecommendations(
                seedUri = recommendationSeedUri,
                recommendations = readyRecommendations,
                artworkByKey = readyRecommendations.associate { recommendation ->
                    val key = recommendationKey(recommendation)
                    key to (fallbackArtwork[key] ?: recommendation.imageUrl)
                }
            )
        }
    }

    fun saveRecommendation(recommendation: LastfmTrack, playlist: Playlist) {
        val key = recommendationKey(recommendation)
        if (key in previewState.savingKeys || key in previewState.queuedKeys ||
            key in previewState.permanentKeys
        ) {
            return
        }
        val artwork = fallbackArtwork[key] ?: recommendation.imageUrl
        app.appScope.launch {
            val result = app.recommendationPreviewManager.saveToPlaylist(
                recommendation = recommendation,
                playlistId = playlist.id,
                artworkUri = artwork
            )
            val message = result.fold(
                onSuccess = { outcome ->
                    app.rememberRecentlySavedPlaylist(playlist.id)
                    when (outcome) {
                        is RecommendationSaveOutcome.Saved ->
                            "Saved ${recommendation.title} to ${playlist.name}"
                        is RecommendationSaveOutcome.AddedToPlaylist ->
                            "Added ${recommendation.title} to ${playlist.name}"
                        is RecommendationSaveOutcome.Queued ->
                            "Download queued for ${playlist.name}"
                    }
                },
                onFailure = { error ->
                    "Could not save ${recommendation.title}: ${error.message ?: "Try again"}"
                }
            )
            withContext(Dispatchers.Main.immediate) {
                Toast.makeText(context.applicationContext, message, Toast.LENGTH_SHORT).show()
            }
        }
    }

    fun preservePlayerScrollPosition() {
        if (playerListState.layoutInfo.visibleItemsInfo.isEmpty()) return
        scrollRestorePosition = FullPlayerScrollPosition(
            index = playerListState.firstVisibleItemIndex,
            offset = playerListState.firstVisibleItemScrollOffset
        )
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
            // Top bar: back arrow, queue, favorite, action sheet
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
                        imageVector = Icons.AutoMirrored.Filled.QueueMusic,
                        contentDescription = "Queue",
                        tint = PrimaryText,
                        modifier = Modifier.size(Dimens.iconSize)
                    )
                }
                IconButton(
                    onClick = {
                        val track = currentLibraryTrack ?: return@IconButton
                        val updatedFavorite = !track.isFavorite
                        app.appScope.launch {
                            app.libraryRepository.setFavorite(track.uri, updatedFavorite)
                        }
                    },
                    enabled = canFavorite
                ) {
                    Icon(
                        imageVector = if (isFavorite) Icons.Filled.Star else Icons.Filled.StarBorder,
                        contentDescription = if (isFavorite) {
                            "Remove from favorites"
                        } else {
                            "Add to favorites"
                        },
                        tint = if (isFavorite) AccentGreen else PrimaryText,
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
                state = playerListState,
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
                artworkUri = displayArtworkUri,
                modifier = Modifier
                    .size(Dimens.albumArtLarge)
                    .clip(RoundedCornerShape(Dimens.cornerMedium)),
                placeholderIconSize = 96.dp
            )

            Spacer(modifier = Modifier.height(Dimens.paddingXLarge))

            // Title / artist
            Text(
                text = displayTitle,
                style = MaterialTheme.typography.headlineMedium,
                color = PrimaryText,
                textAlign = TextAlign.Center,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
            Spacer(modifier = Modifier.height(Dimens.paddingMedium))
            Text(
                text = displayArtist,
                style = MaterialTheme.typography.bodyMedium,
                color = SecondaryText,
                textAlign = TextAlign.Center,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
            Box(
                modifier = Modifier.height(24.dp),
                contentAlignment = Alignment.Center
            ) {
                val status = when {
                    currentTrack?.isTransient == true -> "Temporary preview"
                    previewStatus != null -> previewStatus
                    else -> null
                }
                status?.let {
                    Text(
                        text = it,
                        style = MaterialTheme.typography.labelMedium,
                        color = AccentGreen
                    )
                }
            }

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
                    onClick = {
                        if (!app.recommendationPreviewManager.skipToPrevious()) {
                            musicController.skipToPrevious()
                        }
                    },
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
                    onClick = {
                        if (!app.recommendationPreviewManager.skipToNext()) {
                            musicController.skipToNext()
                        }
                    },
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

                if (currentTrack != null) {
                    TextButton(
                        onClick = {
                            preservePlayerScrollPosition()
                            app.recommendationPreviewManager.setDiscoverMode(
                                enabled = !discoverMode,
                                recommendations = if (discoverMode) {
                                    emptyList()
                                } else {
                                    readyRecommendations
                                }
                            )
                        },
                        enabled = discoverMode || !apiKey.isNullOrBlank()
                    ) {
                        Text(
                            text = if (discoverMode) {
                                "Discover mode: On"
                            } else {
                                "Start discover mode"
                            },
                            color = if (discoverMode) AccentGreen else SecondaryText
                        )
                    }
                }

            }
            fullPlayerRecommendations(
                seedUri = recommendationSeedUri,
                apiKeyConfigured = !apiKey.isNullOrBlank(),
                state = recommendationState,
                recommendations = readyRecommendations,
                previewState = previewState,
                previewPlaying = isPlaying,
                fallbackArtwork = fallbackArtwork,
                onRefresh = { recommendationRefresh++ },
                onArtworkError = ::requestFallbackArtwork,
                onPreview = { recommendation ->
                    val index = readyRecommendations.indexOf(recommendation)
                    val seedUri = recommendationSeedUri
                    if (index >= 0 && seedUri != null) {
                        preservePlayerScrollPosition()
                        app.recommendationPreviewManager.startPreview(
                            seedUri = seedUri,
                            recommendations = readyRecommendations,
                            startIndex = index,
                            artworkByKey = readyRecommendations.associate { item ->
                                val key = recommendationKey(item)
                                key to (fallbackArtwork[key] ?: item.imageUrl)
                            }
                        )
                    }
                },
                onSave = { recommendationToSaveKey = recommendationKey(it) }
            )
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
    recommendationToSaveKey
        ?.let { key -> readyRecommendations.firstOrNull { recommendationKey(it) == key } }
        ?.let { recommendation ->
            PlaylistPickerSheet(
                title = "Download to playlist",
                showRecentlySaved = true,
                onPick = { playlist ->
                    recommendationToSaveKey = null
                    saveRecommendation(recommendation, playlist)
                },
                onDismiss = { recommendationToSaveKey = null },
                onCreateNew = {
                    recommendationToSaveKey = null
                    recommendationForNewPlaylistKey = recommendationKey(recommendation)
                }
            )
        }
    recommendationForNewPlaylistKey
        ?.let { key -> readyRecommendations.firstOrNull { recommendationKey(it) == key } }
        ?.let { recommendation ->
            CreatePlaylistSheet(
                onDismiss = { recommendationForNewPlaylistKey = null },
                onCreated = { playlist ->
                    recommendationForNewPlaylistKey = null
                    saveRecommendation(recommendation, playlist)
                }
            )
        }
}

private fun LazyListScope.fullPlayerRecommendations(
    seedUri: String?,
    apiKeyConfigured: Boolean,
    state: FullPlayerRecommendationState,
    recommendations: List<LastfmTrack>,
    previewState: RecommendationPreviewState,
    previewPlaying: Boolean,
    fallbackArtwork: Map<String, String>,
    onRefresh: () -> Unit,
    onArtworkError: (LastfmTrack) -> Unit,
    onPreview: (LastfmTrack) -> Unit,
    onSave: (LastfmTrack) -> Unit
) {
    val matchingState: FullPlayerRecommendationState = seedUri?.let { currentSeedUri ->
        when (val value = state) {
            FullPlayerRecommendationState.Idle -> value
            is FullPlayerRecommendationState.Loading ->
                value.takeIf { it.seedUri == currentSeedUri } ?: FullPlayerRecommendationState.Idle
            is FullPlayerRecommendationState.Ready ->
                value.takeIf { it.seedUri == currentSeedUri } ?: FullPlayerRecommendationState.Idle
            is FullPlayerRecommendationState.Error ->
                value.takeIf { it.seedUri == currentSeedUri } ?: FullPlayerRecommendationState.Idle
        }
    } ?: FullPlayerRecommendationState.Idle

    item(key = "recommendations-header", contentType = "recommendations-header") {
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
                if (apiKeyConfigured && seedUri != null && !previewState.active) {
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
                seedUri == null -> RecommendationMessage("Play a song to see recommendations")
                !apiKeyConfigured -> RecommendationMessage(
                    "Add a Last.fm API key in Settings to see recommendations"
                )
                !previewState.active && (
                    matchingState is FullPlayerRecommendationState.Loading ||
                        matchingState is FullPlayerRecommendationState.Idle
                    ) -> {
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
                !previewState.active && matchingState is FullPlayerRecommendationState.Error -> {
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
                recommendations.isEmpty() -> {
                    RecommendationMessage("No recommendations available for this song")
                }
                else -> {
                    Text(
                        text = if (previewState.active && previewState.discoverMode) {
                            "Temporary preview queue - tap a song to restart from there"
                        } else if (previewState.active) {
                            "Temporary preview - Next returns to your library"
                        } else {
                            "${recommendations.size} similar songs - tap to preview"
                        },
                        style = MaterialTheme.typography.bodySmall,
                        color = SecondaryText,
                        modifier = Modifier.padding(bottom = Dimens.paddingSmall)
                    )
                }
            }
        }
    }

    if (recommendations.isNotEmpty() &&
        (previewState.active || matchingState is FullPlayerRecommendationState.Ready)
    ) {
        items(
            items = recommendations,
            key = { recommendation -> "recommendation-${recommendationKey(recommendation)}" },
            contentType = { "recommendation-row" }
        ) { recommendation ->
            val key = recommendationKey(recommendation)
            FullPlayerRecommendationRow(
                recommendation = recommendation,
                artworkUri = fallbackArtwork[key] ?: recommendation.imageUrl,
                resolving = key in previewState.resolvingKeys,
                preparing = key in previewState.preparingKeys ||
                    key in previewState.prefetchingKeys,
                queuedPreview = key in previewState.waitingKeys ||
                    key in previewState.prefetchQueuedKeys,
                playing = previewPlaying && key == previewState.currentKey,
                ready = key in previewState.readyKeys ||
                    key in previewState.prefetchedKeys,
                saving = key in previewState.savingKeys,
                queued = key in previewState.queuedKeys,
                permanent = key in previewState.permanentKeys,
                failure = previewState.failures[key],
                saveFailure = previewState.saveFailures[key],
                onArtworkError = { onArtworkError(recommendation) },
                onPreview = { onPreview(recommendation) },
                onSave = { onSave(recommendation) }
            )
        }
    }
}

@Composable
private fun FullPlayerRecommendationRow(
    recommendation: LastfmTrack,
    artworkUri: String?,
    resolving: Boolean,
    preparing: Boolean,
    queuedPreview: Boolean,
    playing: Boolean,
    ready: Boolean,
    saving: Boolean,
    queued: Boolean,
    permanent: Boolean,
    failure: String?,
    saveFailure: String?,
    onArtworkError: () -> Unit,
    onPreview: () -> Unit,
    onSave: () -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(Dimens.cornerMedium))
            .clickable(onClick = onPreview)
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
                color = if (playing) AccentGreen else PrimaryText,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
            Text(
                text = when {
                    saving -> "Saving to playlist..."
                    queued -> "Download queued for playlist"
                    permanent -> "Saved to library"
                    saveFailure != null -> "Save failed: $saveFailure"
                    playing -> "Previewing now"
                    failure != null -> failure
                    queuedPreview -> "Queued for temporary preview"
                    resolving -> "Finding a playable source..."
                    preparing -> "Downloading temporary preview..."
                    ready -> "Ready in temporary queue"
                    else -> "${recommendation.artist} - " +
                        "${(recommendation.match * 100).roundToInt()}% match"
                },
                style = MaterialTheme.typography.bodySmall,
                color = if (saveFailure != null ||
                    (failure != null && !saving && !queued && !permanent)
                ) {
                    MaterialTheme.colorScheme.error
                } else {
                    SecondaryText
                },
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
        }
        Box(
            modifier = Modifier.size(Dimens.touchTargetMin),
            contentAlignment = Alignment.Center
        ) {
            when {
                resolving || preparing -> CircularProgressIndicator(
                    modifier = Modifier.size(Dimens.iconSizeSmall),
                    color = AccentGreen,
                    strokeWidth = 2.dp
                )
                ready && !permanent -> Icon(
                    imageVector = Icons.Filled.CheckCircle,
                    contentDescription = "Ready to play",
                    tint = AccentGreen,
                    modifier = Modifier.size(Dimens.iconSizeSmall)
                )
            }
        }
        IconButton(
            onClick = onSave,
            enabled = !saving && !queued && !permanent
        ) {
            if (saving) {
                CircularProgressIndicator(
                    modifier = Modifier.size(Dimens.iconSizeSmall),
                    color = AccentGreen,
                    strokeWidth = 2.dp
                )
            } else {
                Icon(
                    imageVector = if (permanent) {
                        Icons.Filled.CheckCircle
                    } else {
                        Icons.AutoMirrored.Filled.PlaylistAdd
                    },
                    contentDescription = "Download ${recommendation.title} to a playlist",
                    tint = AccentGreen
                )
            }
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
