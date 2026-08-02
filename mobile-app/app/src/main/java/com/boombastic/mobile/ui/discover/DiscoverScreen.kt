package com.boombastic.mobile.ui.discover

import android.widget.Toast
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
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
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.Key
import androidx.compose.material.icons.filled.PlaylistAdd
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.boombastic.mobile.BoomBasticApp
import com.boombastic.mobile.data.discovery.LastfmResult
import com.boombastic.mobile.data.discovery.LastfmTrack
import com.boombastic.mobile.playback.ExtractionResult
import com.boombastic.mobile.playback.MusicController
import com.boombastic.mobile.playback.WebSearchService
import com.boombastic.mobile.ui.components.ArtworkImage
import com.boombastic.mobile.ui.components.PlaylistPickerSheet
import com.boombastic.mobile.ui.theme.AccentGreen
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText
import kotlin.math.roundToInt
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** The track recommendations are seeded from (desktop: current play, else last track). */
private data class SeedTrack(val title: String, val artist: String)

private data class PlaylistDownloadRequest(
    val tracks: List<LastfmTrack>,
    val batch: Boolean
)

private sealed interface DiscoverUiState {
    data object Idle : DiscoverUiState
    data object Loading : DiscoverUiState
    data class Error(val message: String) : DiscoverUiState
    data class Ready(val seed: SeedTrack, val tracks: List<LastfmTrack>) : DiscoverUiState
}

/**
 * Discover — Last.fm-powered recommendations (desktop Discovery Hub parity).
 *
 * Seeded by the current track (or the last played / last added track when
 * nothing is playing), fetched through [LastfmService]'s two-stage
 * `track.getSimilar` → `artist.getTopTracks` logic, filtered against the
 * user's library, and downloadable individually or in batch through the
 * normal acquisition pipeline (YouTube search → audio extraction →
 * [com.boombastic.mobile.data.repository.DownloadRepository]).
 */
@Composable
fun DiscoverScreen(musicController: MusicController) {
    val context = LocalContext.current
    val app = context.applicationContext as BoomBasticApp
    val repository = app.discoveryRepository
    val downloadRepository = app.downloadRepository
    val apiKey by repository.apiKey.collectAsState()
    val currentTrack by musicController.currentTrack.collectAsState()
    val recentlyPlayed by musicController.recentlyPlayed.collectAsState()
    val allTracks by app.libraryData.tracks.collectAsState()

    // Seed resolution (desktop parity): the track currently playing; when
    // nothing plays, the most recent play; otherwise the last-added track.
    val seed = remember(currentTrack, recentlyPlayed, allTracks) {
        currentTrack?.let { SeedTrack(it.title, it.artist) }
            ?: recentlyPlayed.firstOrNull()?.let { SeedTrack(it.title, it.artist) }
            ?: allTracks.firstOrNull()?.let { SeedTrack(it.title, it.artist) }
    }

    var showKeyDialog by rememberSaveable { mutableStateOf(false) }
    var refreshTrigger by remember { mutableIntStateOf(0) }
    var state by remember { mutableStateOf<DiscoverUiState>(DiscoverUiState.Idle) }

    val scope = rememberCoroutineScope()
    var downloadingKeys by remember { mutableStateOf<Set<String>>(emptySet()) }
    var batchRunning by remember { mutableStateOf(false) }
    var batchQueued by remember { mutableIntStateOf(0) }
    var batchTotal by remember { mutableIntStateOf(0) }
    var batchFailures by remember { mutableIntStateOf(0) }
    var playlistDownloadRequest by remember { mutableStateOf<PlaylistDownloadRequest?>(null) }
    var fallbackArtwork by remember { mutableStateOf<Map<String, String>>(emptyMap()) }
    var fallbackArtworkRequested by remember { mutableStateOf<Set<String>>(emptySet()) }
    val ready = state as? DiscoverUiState.Ready

    fun requestFallbackArtwork(rec: LastfmTrack) {
        val key = recKey(rec)
        if (key in fallbackArtworkRequested) return
        fallbackArtworkRequested = fallbackArtworkRequested + key
        scope.launch {
            app.recommendationArtworkService.findArtwork(rec.artist, rec.title)?.let { url ->
                fallbackArtwork = fallbackArtwork + (key to url)
            }
        }
    }

    LaunchedEffect(ready?.tracks) {
        fallbackArtwork = emptyMap()
        fallbackArtworkRequested = emptySet()
        ready?.tracks.orEmpty()
            .filter { it.imageUrl.isNullOrBlank() }
            .forEach(::requestFallbackArtwork)
    }

    LaunchedEffect(apiKey, seed, refreshTrigger) {
        if (apiKey.isNullOrBlank() || seed == null) {
            state = DiscoverUiState.Idle
            return@LaunchedEffect
        }
        state = DiscoverUiState.Loading
        // Let Compose present the loading frame before even a cached result or
        // a fast response can transition straight to Ready.
        withFrameNanos { }
        when (val result = repository.getSimilar(
            artist = seed.artist,
            title = seed.title,
            limit = 20,
            libraryTracks = allTracks
        )) {
            is LastfmResult.Success -> state = DiscoverUiState.Ready(seed, result.tracks)
            is LastfmResult.Failure -> state = DiscoverUiState.Error(result.message)
        }
    }

    // Shared download plumbing: YouTube search → audio extraction →
    // download queue (the same acquisition pipeline SearchScreen uses).
    suspend fun resolveAndEnqueue(
        artist: String,
        title: String,
        fallbackImage: String?,
        playlistId: Long? = null
    ): Boolean {
        val query = "$artist $title"
        val search = withContext(Dispatchers.IO) {
            WebSearchService.searchYouTube(query, limit = 1)
        }
        val firstResult = (search as? ExtractionResult.Success)?.data?.firstOrNull()
            ?: return false
        val audio = withContext(Dispatchers.IO) {
            WebSearchService.getAudioStreamUrl(firstResult.videoId)
        }
        return when (audio) {
            is ExtractionResult.Error -> false
            is ExtractionResult.Success -> {
                downloadRepository.enqueueDownload(
                    sourceUrl = audio.data.url,
                    title = title,
                    artist = artist,
                    playlistId = playlistId,
                    thumbnailUrl = firstResult.thumbnailUrl.ifBlank { fallbackImage.orEmpty() }
                )
                true
            }
        }
    }

    fun downloadOne(rec: LastfmTrack, playlistId: Long? = null) {
        val key = recKey(rec)
        if (key in downloadingKeys) return
        downloadingKeys = downloadingKeys + key
        scope.launch {
            val queued = resolveAndEnqueue(rec.artist, rec.title, rec.imageUrl, playlistId)
            val message = if (queued) {
                "Download queued: ${rec.title}"
            } else {
                "Could not find \"${rec.title}\" by ${rec.artist} on YouTube"
            }
            Toast.makeText(context, message, Toast.LENGTH_SHORT).show()
            downloadingKeys = downloadingKeys - key
        }
    }

    fun downloadAll(tracks: List<LastfmTrack>, playlistId: Long? = null) {
        if (tracks.isEmpty() || batchRunning) return
        batchRunning = true
        batchQueued = 0
        batchFailures = 0
        batchTotal = tracks.size
        scope.launch {
            for (rec in tracks) {
                if (resolveAndEnqueue(rec.artist, rec.title, rec.imageUrl, playlistId)) {
                    batchQueued++
                } else {
                    batchFailures++
                }
            }
            batchRunning = false
            val message = "Queued $batchQueued of $batchTotal" +
                if (batchFailures > 0) " ($batchFailures not found)" else ""
            Toast.makeText(context, message, Toast.LENGTH_SHORT).show()
        }
    }

    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(top = Dimens.paddingLarge, bottom = Dimens.paddingXLarge)
    ) {
        item(key = "title") {
            Column(modifier = Modifier.padding(horizontal = Dimens.paddingLarge)) {
                Text(
                    text = "Discover",
                    style = MaterialTheme.typography.headlineLarge,
                    color = PrimaryText,
                    fontWeight = FontWeight.Bold
                )
                Text(
                    text = "Recommendations from Last.fm",
                    style = MaterialTheme.typography.bodyMedium,
                    color = SecondaryText
                )
            }
        }

        when {
            apiKey.isNullOrBlank() -> {
                item(key = "setup") {
                    KeySetupState(onSetKey = { showKeyDialog = true })
                }
            }

            seed == null -> {
                item(key = "no-seed") {
                    EmptyState(
                        title = "Nothing to seed from yet",
                        subtitle = "Play a track or add music to your library to get recommendations."
                    )
                }
            }

            else -> {
                item(key = "seed-header") {
                    SeedHeader(
                        seed = seed,
                        trackCount = (ready?.tracks ?: emptyList()).size,
                        batchRunning = batchRunning,
                        batchQueued = batchQueued,
                        batchTotal = batchTotal,
                        onRefresh = { refreshTrigger++ },
                        onKeySettings = { showKeyDialog = true },
                        onDownloadAll = { downloadAll(ready?.tracks ?: emptyList()) },
                        onChoosePlaylistForAll = {
                            playlistDownloadRequest = PlaylistDownloadRequest(
                                tracks = ready?.tracks ?: emptyList(),
                                batch = true
                            )
                        }
                    )
                }

                when (state) {
                    is DiscoverUiState.Loading -> {
                        item(key = "loading") {
                            Box(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(vertical = Dimens.paddingXLarge),
                                contentAlignment = Alignment.Center
                            ) {
                                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                                    DiscoverLoadingSpinner()
                                    Spacer(modifier = Modifier.height(Dimens.paddingMedium))
                                    Text(
                                        text = "Fetching recommendations from Last.fm…",
                                        style = MaterialTheme.typography.bodyMedium,
                                        color = SecondaryText
                                    )
                                }
                            }
                        }
                    }

                    is DiscoverUiState.Error -> {
                        item(key = "error") {
                            Column(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(horizontal = Dimens.paddingLarge, vertical = Dimens.paddingXLarge),
                                horizontalAlignment = Alignment.CenterHorizontally
                            ) {
                                Text(
                                    text = (state as DiscoverUiState.Error).message,
                                    style = MaterialTheme.typography.bodyMedium,
                                    color = MaterialTheme.colorScheme.error,
                                    textAlign = TextAlign.Center
                                )
                                Spacer(modifier = Modifier.height(Dimens.paddingMedium))
                                Button(
                                    onClick = { refreshTrigger++ },
                                    colors = ButtonDefaults.buttonColors(
                                        containerColor = AccentGreen,
                                        contentColor = PrimaryText
                                    ),
                                    shape = RoundedCornerShape(Dimens.cornerMedium)
                                ) {
                                    Text("Retry")
                                }
                            }
                        }
                    }

                    is DiscoverUiState.Ready -> {
                        val tracks = (state as DiscoverUiState.Ready).tracks
                        if (tracks.isEmpty()) {
                            item(key = "empty") {
                                EmptyState(
                                    title = "No new recommendations",
                                    subtitle = "You already have everything Last.fm considers similar to this track. " +
                                        "Try a different seed or refresh later."
                                )
                            }
                        } else {
                            items(
                                tracks,
                                key = { recKey(it) },
                                contentType = { "recommendation" }
                            ) { rec ->
                                RecommendationRow(
                                    rec = rec,
                                    artworkUri = fallbackArtwork[recKey(rec)] ?: rec.imageUrl,
                                    downloading = recKey(rec) in downloadingKeys,
                                    onArtworkError = {
                                        requestFallbackArtwork(rec)
                                    },
                                    onDownload = { downloadOne(rec) },
                                    onChoosePlaylist = {
                                        playlistDownloadRequest = PlaylistDownloadRequest(
                                            tracks = listOf(rec),
                                            batch = false
                                        )
                                    }
                                )
                            }
                        }
                    }

                    is DiscoverUiState.Idle -> Unit
                }
            }
        }
    }

    if (showKeyDialog) {
        LastfmKeyDialog(
            currentKey = apiKey,
            onSave = { key ->
                repository.setApiKey(key)
                showKeyDialog = false
            },
            onClear = {
                repository.clearApiKey()
                showKeyDialog = false
            },
            onDismiss = { showKeyDialog = false }
        )
    }

    playlistDownloadRequest?.let { request ->
        PlaylistPickerSheet(
            onPick = { playlist ->
                playlistDownloadRequest = null
                if (request.batch) {
                    downloadAll(request.tracks, playlist.id)
                } else {
                    request.tracks.firstOrNull()?.let { downloadOne(it, playlist.id) }
                }
            },
            onDismiss = { playlistDownloadRequest = null }
        )
    }
}

private fun recKey(rec: LastfmTrack): String = "${rec.artist.lowercase()}|${rec.title.lowercase()}"

@Composable
private fun KeySetupState(onSetKey: () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = Dimens.paddingLarge, vertical = Dimens.paddingXLarge),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Text(
            text = "Recommendations need your Last.fm API key",
            style = MaterialTheme.typography.titleMedium,
            color = PrimaryText,
            textAlign = TextAlign.Center
        )
        Spacer(modifier = Modifier.height(Dimens.paddingMedium))
        Text(
            text = "Get a free key at last.fm/api, then paste it here. It is stored encrypted " +
                "on your device and only used to talk to Last.fm.",
            style = MaterialTheme.typography.bodyMedium,
            color = SecondaryText,
            textAlign = TextAlign.Center
        )
        Spacer(modifier = Modifier.height(Dimens.paddingXLarge))
        Button(
            onClick = onSetKey,
            colors = ButtonDefaults.buttonColors(
                containerColor = AccentGreen,
                contentColor = PrimaryText
            ),
            shape = RoundedCornerShape(Dimens.cornerMedium)
        ) {
            Text("Set Last.fm API key")
        }
    }
}

@Composable
private fun EmptyState(title: String, subtitle: String) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = Dimens.paddingLarge, vertical = Dimens.paddingXLarge)
    ) {
        Text(
            text = title,
            style = MaterialTheme.typography.titleMedium,
            color = PrimaryText
        )
        Text(
            text = subtitle,
            style = MaterialTheme.typography.bodyMedium,
            color = SecondaryText,
            modifier = Modifier.padding(top = Dimens.paddingSmall)
        )
    }
}

@Composable
private fun SeedHeader(
    seed: SeedTrack,
    trackCount: Int,
    batchRunning: Boolean,
    batchQueued: Int,
    batchTotal: Int,
    onRefresh: () -> Unit,
    onKeySettings: () -> Unit,
    onDownloadAll: () -> Unit,
    onChoosePlaylistForAll: () -> Unit
) {
    Column(modifier = Modifier.padding(top = Dimens.paddingXLarge)) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = Dimens.paddingLarge),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = "Based on",
                    style = MaterialTheme.typography.labelSmall,
                    color = SecondaryText
                )
                Text(
                    text = "${seed.title} — ${seed.artist}",
                    style = MaterialTheme.typography.titleSmall,
                    color = PrimaryText,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis
                )
            }
            IconButton(onClick = onKeySettings) {
                Icon(
                    imageVector = Icons.Filled.Key,
                    contentDescription = "Last.fm API key",
                    tint = SecondaryText
                )
            }
            IconButton(onClick = onRefresh) {
                Icon(
                    imageVector = Icons.Filled.Refresh,
                    contentDescription = "Refresh recommendations",
                    tint = AccentGreen
                )
            }
        }
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = Dimens.paddingLarge, vertical = Dimens.paddingSmall),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Text(
                text = if (batchRunning) {
                    "Queuing downloads… $batchQueued/$batchTotal"
                } else {
                    "$trackCount recommendations"
                },
                style = MaterialTheme.typography.bodySmall,
                color = SecondaryText
            )
            Spacer(modifier = Modifier.weight(1f))
            if (batchRunning) {
                CircularProgressIndicator(
                    modifier = Modifier.size(Dimens.iconSizeSmall),
                    color = AccentGreen,
                    strokeWidth = 2.dp
                )
            } else {
                Button(
                    onClick = onDownloadAll,
                    enabled = trackCount > 0,
                    colors = ButtonDefaults.buttonColors(
                        containerColor = AccentGreen,
                        contentColor = PrimaryText,
                        disabledContainerColor = MaterialTheme.colorScheme.surfaceVariant,
                        disabledContentColor = SecondaryText
                    ),
                    shape = RoundedCornerShape(Dimens.cornerMedium)
                ) {
                    Text("Download all")
                }
                IconButton(onClick = onChoosePlaylistForAll) {
                    Icon(
                        imageVector = Icons.Filled.PlaylistAdd,
                        contentDescription = "Download all to a playlist",
                        tint = AccentGreen
                    )
                }
            }
        }
    }
}

@Composable
private fun RecommendationRow(
    rec: LastfmTrack,
    artworkUri: String?,
    downloading: Boolean,
    onArtworkError: () -> Unit,
    onDownload: () -> Unit,
    onChoosePlaylist: () -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = Dimens.paddingLarge, vertical = Dimens.paddingSmall),
        verticalAlignment = Alignment.CenterVertically
    ) {
        ArtworkImage(
            artworkUri = artworkUri,
            modifier = Modifier
                .size(Dimens.albumArtSmall)
                .clip(RoundedCornerShape(Dimens.cornerMedium)),
            placeholderIconSize = 24.dp,
            decodeSizePx = 128,
            onError = if (artworkUri == rec.imageUrl) onArtworkError else null
        )
        Spacer(modifier = Modifier.width(Dimens.paddingMedium))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = rec.title,
                style = MaterialTheme.typography.titleSmall,
                color = PrimaryText,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
            Text(
                text = "${rec.artist} • ${(rec.match * 100).roundToInt()}% match",
                style = MaterialTheme.typography.bodySmall,
                color = SecondaryText,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
        }
        if (downloading) {
            CircularProgressIndicator(
                modifier = Modifier.size(Dimens.iconSize),
                color = AccentGreen,
                strokeWidth = 2.dp
            )
        } else {
            IconButton(onClick = onDownload) {
                Icon(
                    imageVector = Icons.Filled.Download,
                    contentDescription = "Download ${rec.title}",
                    tint = AccentGreen
                )
            }
            IconButton(onClick = onChoosePlaylist) {
                Icon(
                    imageVector = Icons.Filled.PlaylistAdd,
                    contentDescription = "Download ${rec.title} to a playlist",
                    tint = AccentGreen
                )
            }
        }
    }
}

@Composable
private fun DiscoverLoadingSpinner() {
    val transition = rememberInfiniteTransition(label = "discoverLoadingSpinner")
    val rotation by transition.animateFloat(
        initialValue = 0f,
        targetValue = 360f,
        animationSpec = infiniteRepeatable(
            animation = tween(durationMillis = 900, easing = LinearEasing),
            repeatMode = RepeatMode.Restart
        ),
        label = "discoverLoadingSpinnerRotation"
    )
    Canvas(modifier = Modifier.size(40.dp)) {
        drawArc(
            color = AccentGreen,
            startAngle = rotation - 90f,
            sweepAngle = 270f,
            useCenter = false,
            style = Stroke(width = 4.dp.toPx(), cap = StrokeCap.Round)
        )
    }
}
