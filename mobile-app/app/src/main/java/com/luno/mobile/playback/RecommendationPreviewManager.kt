package com.luno.mobile.playback

import android.content.Context
import android.media.MediaMetadataRetriever
import android.net.Uri
import androidx.media3.common.Player
import com.luno.mobile.data.artwork.ArtworkStorage
import com.luno.mobile.data.db.entity.DownloadState
import com.luno.mobile.data.db.entity.Track
import com.luno.mobile.data.discovery.LastfmTrack
import com.luno.mobile.data.discovery.normalizedRecommendationKey
import com.luno.mobile.data.repository.DownloadRepository
import com.luno.mobile.data.repository.PlaylistRepository
import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.util.concurrent.Executors
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withTimeoutOrNull
import okhttp3.ConnectionSpec
import okhttp3.OkHttpClient
import okhttp3.Request

data class RecommendationPreviewState(
    val active: Boolean = false,
    val seedUri: String? = null,
    val recommendations: List<LastfmTrack> = emptyList(),
    val discoverMode: Boolean = false,
    val waitingKeys: Set<String> = emptySet(),
    val resolvingKeys: Set<String> = emptySet(),
    val preparingKeys: Set<String> = emptySet(),
    val prefetchingKeys: Set<String> = emptySet(),
    val prefetchQueuedKeys: Set<String> = emptySet(),
    val readyKeys: Set<String> = emptySet(),
    val prefetchedKeys: Set<String> = emptySet(),
    val failures: Map<String, String> = emptyMap(),
    val currentKey: String? = null,
    val selectedKey: String? = null,
    val permanentKeys: Set<String> = emptySet(),
    val savingKeys: Set<String> = emptySet(),
    val queuedKeys: Set<String> = emptySet(),
    val saveFailures: Map<String, String> = emptyMap()
)

sealed interface RecommendationSaveOutcome {
    data class Saved(val track: Track) : RecommendationSaveOutcome
    data class AddedToPlaylist(val trackUri: String) : RecommendationSaveOutcome
    data class Queued(val jobId: Long) : RecommendationSaveOutcome
}

data class ResolvedRecommendation(
    val sourceUrl: String,
    val thumbnailUrl: String,
    val mimeType: String
)

private data class PreviewAsset(
    val recommendation: LastfmTrack,
    val file: File,
    val sourceUrl: String,
    val thumbnailUrl: String,
    val mediaTrack: MediaTrack
)

private data class PreviewPlaybackContext(
    val tracks: List<MediaTrack>,
    val startIndex: Int
)

private enum class PreviewDispatchResult {
    SUCCESS,
    RETRY,
    INTERVENED
}

/**
 * Process-lifetime owner of recommendation previews. Audio is stored only in
 * cache, appended to one finite Media3 context, and removed when normal
 * playback replaces the preview session or on the next process launch.
 */
class RecommendationPreviewManager(
    context: Context,
    private val appScope: CoroutineScope,
    private val downloadRepository: DownloadRepository,
    private val playlistRepository: PlaylistRepository,
    private val resolver: suspend (LastfmTrack) -> Result<ResolvedRecommendation> =
        { recommendation -> resolveRecommendation(recommendation, previewMode = true) },
    private val durableResolver: suspend (LastfmTrack) -> Result<ResolvedRecommendation> =
        { recommendation -> resolveRecommendation(recommendation, previewMode = false) },
    private val discoverRecommendations: suspend (MediaTrack) -> Result<List<LastfmTrack>> =
        { Result.success(emptyList()) },
    private val previewDownloader: suspend (
        ResolvedRecommendation,
        LastfmTrack,
        String?,
        File
    ) -> Result<MediaTrack> = { resolved, recommendation, artworkUri, target ->
        downloadPreview(context, resolved, recommendation, artworkUri, target)
    },
    private val resolveTimeoutMs: Long = PREVIEW_RESOLVE_TIMEOUT_MS,
    private val downloadTimeoutMs: Long = PREVIEW_DOWNLOAD_TIMEOUT_MS
) {
    private val context = context.applicationContext
    private val previewDirectory = File(
        this.context.cacheDir,
        DownloadRepository.PREVIEW_DOWNLOAD_DIR
    )
    private val lock = Any()
    private val assetsByKey = linkedMapOf<String, PreviewAsset>()
    private val prefetchedAssetsByKey = linkedMapOf<String, PreviewAsset>()
    private val keyByUri = mutableMapOf<String, String>()
    private val savedTrackUris = mutableMapOf<String, String>()
    private val savingKeys = mutableSetOf<String>()
    private val queuedJobByKey = mutableMapOf<String, Long>()
    private val saveFailures = mutableMapOf<String, String>()
    private val filesBeingPromoted = mutableSetOf<File>()

    private val _state = MutableStateFlow(RecommendationPreviewState())
    val state: StateFlow<RecommendationPreviewState> = _state.asStateFlow()

    @Volatile private var musicController: MusicController? = null
    private var monitorJob: Job? = null
    private var sessionJob: Job? = null
    private var generation = 0L
    private var observedPreviewTrack = false
    private var previewPlaybackStarted = false
    private var preparationComplete = false
    private var recoveringPlaybackError = false
    private var initialNormalTrackUri: String? = null
    private var baselineController: MusicController? = null
    private var baselinePlayRequestRevision = 0L
    private var continuationTracks: List<MediaTrack> = emptyList()
    private var forcedAdvanceUri: String? = null
    private var forcedAdvanceInProgress = false
    private var deferredInitialPlayback = false
    private var queuedPreviewTailUri: String? = null
    private var advanceToDeferredRecommendation = false
    private var physicalQueueBeforePreview: List<MediaTrack> = emptyList()
    private var physicalCurrentIndexBeforePreview = -1
    private var prefetchJob: Job? = null
    private var prefetchGeneration = 0L
    private var prefetchSeedUri: String? = null
    private var discoverRequestJob: Job? = null
    private var discoverRequestGeneration = 0L
    private var discoverStartedUri: String? = null
    private var endAfterCurrentRecommendation = false
    private var discoverExtensionPending = false
    private val discoverPreparingKeys = mutableSetOf<String>()

    private val startupCleanup: Deferred<Unit> = appScope.async(Dispatchers.IO) {
        previewDirectory.deleteRecursively()
        previewDirectory.mkdirs()
    }

    fun attach(controller: MusicController) {
        synchronized(lock) {
            musicController = controller
            if (_state.value.active) {
                baselineController = controller
                baselinePlayRequestRevision = controller.playRequestRevision.value
            }
        }
        monitorJob?.cancel()
        monitorJob = appScope.launch(start = CoroutineStart.UNDISPATCHED) {
            launch(start = CoroutineStart.UNDISPATCHED) {
                controller.currentTrack.collect { track ->
                    val stalePrefetch = synchronized(lock) {
                        prefetchSeedUri != null && !_state.value.active &&
                            (track?.uri != prefetchSeedUri || track?.isTransient == true)
                    }
                    if (stalePrefetch) {
                        // The current track moved away from the prefetch seed:
                        // stop any in-flight prefetch work, but keep the
                        // already-downloaded assets and their state — they may
                        // still match the recommendations the UI shows, and
                        // wiping them here is what made toggling sessions
                        // "re-download the same songs". A prefetch for a
                        // genuinely different seed replaces them itself.
                        synchronized(lock) {
                            prefetchGeneration++
                            prefetchJob?.cancel()
                            prefetchJob = null
                        }
                    }

                    val transition = synchronized(lock) {
                        if (!_state.value.active || musicController !== controller ||
                            !controller.isConnected.value
                        ) {
                            return@synchronized null
                        }
                        val key = track?.uri?.let(keyByUri::get)
                        val recoveryTransition = recoveringPlaybackError
                        if (recoveryTransition) recoveringPlaybackError = false
                        val forcedTransition = forcedAdvanceInProgress
                        val newerNormalPlaybackRequest = baselineController === controller &&
                            controller.playRequestRevision.value != baselinePlayRequestRevision
                        val normalPlaybackIntervention = track != null && !track.isTransient &&
                            (newerNormalPlaybackRequest ||
                                (initialNormalTrackUri != null &&
                                    track.uri != initialNormalTrackUri))
                        // The preview block ended naturally and the appended
                        // physical handoff is playing: rebuild the full
                        // physical queue (only a short handoff window was
                        // appended behind the previews).
                        val naturalHandoffToLibrary = track != null && !track.isTransient &&
                            !newerNormalPlaybackRequest &&
                            initialNormalTrackUri != null && track.uri != initialNormalTrackUri
                        val shouldFinish = when {
                            key != null -> {
                                observedPreviewTrack = true
                                if (forcedTransition) {
                                    forcedAdvanceInProgress = false
                                    forcedAdvanceUri = null
                                }
                                _state.value = _state.value.copy(currentKey = key)
                                if (endAfterCurrentRecommendation) {
                                    // Discover was turned off while this
                                    // session's previous recommendation was
                                    // playing: it has now finished, so end
                                    // the session and let the physical
                                    // library resume instead of the next
                                    // recommendation.
                                    endAfterCurrentRecommendation = false
                                    generation to true
                                } else {
                                    null
                                }
                            }
                            forcedTransition -> null
                            recoveryTransition -> null
                            normalPlaybackIntervention -> generation to naturalHandoffToLibrary
                            observedPreviewTrack && track != null ->
                                generation to naturalHandoffToLibrary
                            observedPreviewTrack -> generation to false
                            else -> null
                        }
                        shouldFinish
                    }
                    if (transition != null && isCurrent(transition.first)) {
                        val discoverTrack = track?.takeUnless { it.isTransient }
                        val discoverMode = synchronized(lock) { _state.value.discoverMode }
                        finishForPlaybackIntervention(restoreNormalQueue = transition.second)
                        if (discoverMode && discoverTrack != null) {
                            requestDiscoverRecommendations(discoverTrack)
                        }
                    } else if (track != null && !track.isTransient) {
                        requestDiscoverRecommendations(track)
                    }
                }
            }
            launch(start = CoroutineStart.UNDISPATCHED) {
                controller.playbackError.collect { error ->
                    if (!error.isMediaItemFailure) return@collect
                    val failedKey = synchronized(lock) {
                        if (!_state.value.active || musicController !== controller) null
                        else {
                            (error.mediaUri ?: controller.currentTrack.value?.uri)
                                ?.let(keyByUri::get)
                                ?: _state.value.currentKey.takeIf { observedPreviewTrack }
                        }
                    }
                    if (failedKey != null) {
                        recoverFromPlaybackError(controller, failedKey, error.message)
                    }
                }
            }
            launch(start = CoroutineStart.UNDISPATCHED) {
                controller.playbackState.collect { state ->
                    if (state != Player.STATE_ENDED) return@collect
                    val shouldCheckQueue = synchronized(lock) {
                        _state.value.active && musicController === controller &&
                            !discoverExtensionPending && discoverPreparingKeys.isEmpty()
                    }
                    if (!shouldCheckQueue) return@collect
                    val resumeLibrary = withContext(Dispatchers.Main.immediate) {
                        synchronized(lock) {
                            if (!_state.value.active || musicController !== controller) {
                                false
                            } else {
                                // The queue ran out while a session was active
                                // (e.g. the Discover block was exhausted with
                                // nothing queued behind it): resume the captured
                                // physical playlist.
                                controller.getQueueAfterCurrent().orEmpty().isEmpty()
                            }
                        }
                    }
                    if (resumeLibrary) {
                        finishForPlaybackIntervention(restoreNormalQueue = true)
                    }
                }
            }
        }
    }

    fun detach(controller: MusicController) {
        synchronized(lock) {
            if (musicController !== controller) return
            musicController = null
        }
        monitorJob?.cancel()
        monitorJob = null
    }

    /** Enables or disables automatic recommendation playback around normal tracks. */
    fun setDiscoverMode(
        enabled: Boolean,
        recommendations: List<LastfmTrack> = emptyList()
    ) {
        val currentTrack: MediaTrack?
        val stopActivePreview: Boolean
        val immediateDiscoverTrack: MediaTrack?
        val immediateDiscoverSeedUri: String?
        val immediateDiscoverStartIndex: Int
        synchronized(lock) {
            if (_state.value.discoverMode == enabled) return
            discoverRequestGeneration++
            discoverRequestJob?.cancel()
            discoverRequestJob = null
            discoverStartedUri = if (enabled) null else discoverStartedUri
            _state.value = withSaveState(_state.value.copy(discoverMode = enabled))
            stopActivePreview = !enabled && _state.value.active
            currentTrack = musicController?.currentTrack?.value
            val currentRecommendationIndex = currentTrack?.uri?.let(keyByUri::get)?.let { key ->
                recommendations.indexOfFirst { recommendationKey(it) == key }
            } ?: -1
            immediateDiscoverStartIndex = if (currentRecommendationIndex >= 0) {
                currentRecommendationIndex + 1
            } else {
                0
            }
            immediateDiscoverTrack = if (enabled && recommendations.isNotEmpty() &&
                immediateDiscoverStartIndex < recommendations.size
            ) {
                currentTrack?.takeIf { !it.isTransient || currentRecommendationIndex >= 0 }
            } else {
                null
            }
            immediateDiscoverSeedUri = if (currentTrack?.isTransient == true) {
                _state.value.seedUri
            } else {
                currentTrack?.uri
            }
            if (immediateDiscoverTrack != null) {
                discoverStartedUri = immediateDiscoverSeedUri
            }
        }

        if (stopActivePreview) {
            // Decide from the current track itself whether a recommendation
            // is actually playing: `observedPreviewTrack` is unreliable here
            // because restarting the session for Discover resets it even when
            // the already-current recommendation keeps playing.
            val currentIsRecommendation = synchronized(lock) {
                val current = musicController?.currentTrack?.value
                current != null && (current.isTransient || current.uri in keyByUri)
            }
            if (!currentIsRecommendation) {
                // The seed is still current — no recommendation has played
                // yet: tear the session down now and keep the seed playing.
                // Nothing else changes.
                finishForPlaybackIntervention(restoreNormalQueue = true)
            } else {
                // A recommendation is playing: let it finish, then resume
                // the physical library in normal order. Media keys return
                // to the physical library immediately (discoverMode is
                // already off).
                synchronized(lock) { endAfterCurrentRecommendation = true }
            }
        } else if (enabled) {
            // Deliberately NOT cancelPrefetch(): startPreview retains the
            // already-downloaded prefetch assets into the Discover session,
            // so toggling Discover on never re-downloads ready songs or
            // wipes their checkmarks. startPreview cancels the prefetch job
            // itself after retention.
            if (immediateDiscoverTrack != null) {
                startPreview(
                    seedUri = immediateDiscoverSeedUri ?: immediateDiscoverTrack.uri,
                    recommendations = recommendations,
                    startIndex = immediateDiscoverStartIndex,
                    artworkByKey = emptyMap(),
                    deferUntilCurrentEnds = true
                )
            } else {
                currentTrack?.takeUnless { it.isTransient }
                    ?.let(::requestDiscoverRecommendations)
            }
        }
    }

    /** Keeps an active Discover session alive while its next page is fetched. */
    fun setDiscoverExtensionPending(pending: Boolean) {
        val shouldCheckEndedPlayback: Boolean
        synchronized(lock) {
            if (!_state.value.active || !_state.value.discoverMode) return
            discoverExtensionPending = pending
            shouldCheckEndedPlayback = !pending && discoverPreparingKeys.isEmpty()
        }
        if (shouldCheckEndedPlayback) {
            appScope.launch(Dispatchers.Main.immediate) {
                val controller = synchronized(lock) {
                    musicController?.takeIf {
                        _state.value.active && _state.value.discoverMode &&
                            !discoverExtensionPending && discoverPreparingKeys.isEmpty() &&
                            it.playbackState.value == Player.STATE_ENDED
                    }
                }
                if (controller != null &&
                    controller.getQueueAfterCurrent().orEmpty().isEmpty()
                ) {
                    finishForPlaybackIntervention(restoreNormalQueue = true)
                }
            }
        }
    }

    /** Appends a fetched page without replacing the currently playing item. */
    fun appendRecommendations(seedUri: String, recommendations: List<LastfmTrack>): Boolean {
        if (recommendations.isEmpty()) return false
        synchronized(lock) {
            if (_state.value.seedUri != seedUri) return false
            val existingKeys = _state.value.recommendations.mapTo(mutableSetOf(), ::recommendationKey)
            val additions = recommendations
                .distinctBy(::recommendationKey)
                .filter { recommendationKey(it) !in existingKeys }
            if (additions.isEmpty()) return _state.value.active && _state.value.discoverMode

            _state.value = withSaveState(
                _state.value.copy(
                    recommendations = _state.value.recommendations + additions
                )
            )
            if (_state.value.active && _state.value.discoverMode) {
                discoverPreparingKeys += additions.map(::recommendationKey)
                return true
            }
            return false
        }
    }

    private fun requestDiscoverRecommendations(track: MediaTrack) {
        val requestGeneration: Long
        synchronized(lock) {
            if (!_state.value.discoverMode || _state.value.active ||
                discoverStartedUri == track.uri || discoverRequestJob != null
            ) {
                return
            }
            if (discoverStartedUri != track.uri) {
                advanceToDeferredRecommendation = false
            }
            discoverStartedUri = track.uri
            discoverRequestGeneration++
            requestGeneration = discoverRequestGeneration
        }

        val job = appScope.launch(start = CoroutineStart.LAZY) {
            val result = try {
                discoverRecommendations(track)
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Exception) {
                Result.failure(error)
            }

            val recommendations = result.getOrNull()
                ?.distinctBy(::recommendationKey)
                .orEmpty()
            synchronized(lock) {
                if (discoverRequestGeneration != requestGeneration) return@launch
                discoverRequestJob = null
                if (recommendations.isEmpty()) {
                    advanceToDeferredRecommendation = false
                    return@launch
                }
            }
            withContext(Dispatchers.Main.immediate) {
                synchronized(lock) {
                    if (discoverRequestGeneration != requestGeneration ||
                        !_state.value.discoverMode || _state.value.active ||
                        musicController?.currentTrack?.value?.uri != track.uri
                    ) {
                        return@withContext
                    }
                }
                startPreview(
                    seedUri = track.uri,
                    recommendations = recommendations,
                    startIndex = 0,
                    artworkByKey = emptyMap(),
                    deferUntilCurrentEnds = true
                )
            }
        }
        synchronized(lock) {
            if (discoverRequestGeneration == requestGeneration &&
                _state.value.discoverMode
            ) {
                discoverRequestJob = job
            } else {
                job.cancel()
                return
            }
        }
        job.start()
    }

    /** Passively prepares the first five recommendations without changing playback. */
    fun prefetchRecommendations(
        seedUri: String,
        recommendations: List<LastfmTrack>,
        artworkByKey: Map<String, String?>
    ) {
        val targets = recommendations
            .distinctBy(::recommendationKey)
            .take(PREFETCH_RECOMMENDATION_COUNT)
        if (targets.isEmpty()) return
        val controller = musicController ?: return
        val keys = targets.map(::recommendationKey).toSet()
        val oldFiles: List<File>
        val requestGeneration: Long
        synchronized(lock) {
            val current = controller.currentTrack.value
            if (_state.value.active || current?.uri != seedUri || current?.isTransient == true) return
            if (prefetchJob?.isActive == true && keys.all { key ->
                    key in _state.value.prefetchingKeys ||
                        key in _state.value.prefetchQueuedKeys ||
                        (key in _state.value.prefetchedKeys &&
                            prefetchedAssetsByKey[key]?.file?.isFile == true)
                }
            ) {
                return
            }

            prefetchGeneration++
            requestGeneration = prefetchGeneration
            prefetchJob?.cancel()
            prefetchJob = null
            // Reuse already-downloaded assets that are still targets of this
            // window (recommendation lists overlap between adjacent tracks);
            // only genuinely stale files are deleted and re-downloaded.
            val stillUseful = prefetchedAssetsByKey
                .filterKeys { it in keys }
                .filterValues { it.file.isFile }
            oldFiles = prefetchedAssetsByKey.values.map { it.file }
                .filterNot(filesBeingPromoted::contains)
                .filterNot { file -> stillUseful.values.any { it.file == file } }
            prefetchedAssetsByKey.clear()
            prefetchedAssetsByKey.putAll(stillUseful)
            prefetchSeedUri = seedUri
            _state.value = withSaveState(
                _state.value.copy(
                    seedUri = seedUri,
                    // Keep the state's recommendations paired with its seed:
                    // after a preview session ends, `seedUri` alone must not
                    // re-point the retained list of a previous song at the
                    // currently playing one.
                    recommendations = recommendations.toList(),
                    prefetchingKeys = targets.firstOrNull()
                        ?.let { setOf(recommendationKey(it)) }
                        .orEmpty(),
                    prefetchQueuedKeys = targets.drop(1).map(::recommendationKey).toSet(),
                    prefetchedKeys = stillUseful.keys.toSet(),
                    failures = _state.value.failures - keys
                )
            )
        }
        deleteFiles(oldFiles)

        val job = appScope.launch {
            startupCleanup.await()
            for ((index, recommendation) in targets.withIndex()) {
                if (!isCurrentPrefetch(requestGeneration)) return@launch
                val key = recommendationKey(recommendation)
                val alreadyCached = synchronized(lock) {
                    prefetchedAssetsByKey[key]?.takeIf { it.file.isFile }
                }
                if (alreadyCached != null) continue
                synchronized(lock) {
                    if (!isCurrentPrefetchLocked(requestGeneration)) return@launch
                    _state.value = withSaveState(
                        _state.value.copy(
                            prefetchingKeys = setOf(key),
                            prefetchQueuedKeys = targets.drop(index + 1)
                                .map(::recommendationKey)
                                .toSet()
                        )
                    )
                }
                val resolved = runTimedResult(
                    timeoutMs = resolveTimeoutMs,
                    timeoutMessage = "Finding a playable source timed out. Tap to retry."
                ) {
                    resolver(recommendation)
                }
                if (!isCurrentPrefetch(requestGeneration)) return@launch
                if (resolved.isFailure) {
                    markPrefetchFailure(
                        requestGeneration,
                        key,
                        resolved.errorMessage()
                    )
                    continue
                }

                val stream = resolved.getOrThrow()
                val extension = extensionForMimeType(stream.mimeType)
                val target = File(
                    previewDirectory,
                    "prefetch-$requestGeneration-$index-${safeName(recommendation)}.$extension"
                )
                val artworkUri = artworkByKey[key]?.takeUnless { it.isNullOrBlank() }
                    ?: stream.thumbnailUrl.takeUnless { it.isBlank() }
                val downloaded = runTimedResult(
                    timeoutMs = downloadTimeoutMs,
                    timeoutMessage = "Preview download timed out. Tap to retry."
                ) {
                    previewDownloader(
                        stream,
                        recommendation,
                        artworkUri,
                        target
                    )
                }
                if (!isCurrentPrefetch(requestGeneration)) {
                    target.delete()
                    return@launch
                }
                if (downloaded.isFailure) {
                    markPrefetchFailure(
                        requestGeneration,
                        key,
                        downloaded.errorMessage()
                    )
                    continue
                }

                val asset = PreviewAsset(
                    recommendation = recommendation,
                    file = target,
                    sourceUrl = stream.sourceUrl,
                    thumbnailUrl = stream.thumbnailUrl.ifBlank { artworkUri.orEmpty() },
                    mediaTrack = downloaded.getOrThrow()
                )
                synchronized(lock) {
                    if (!isCurrentPrefetchLocked(requestGeneration)) {
                        target.delete()
                        return@launch
                    }
                    prefetchedAssetsByKey[key] = asset
                    _state.value = withSaveState(
                        _state.value.copy(
                            prefetchingKeys = _state.value.prefetchingKeys - key,
                            prefetchedKeys = _state.value.prefetchedKeys + key,
                            failures = _state.value.failures - key
                        )
                    )
                }
            }
            synchronized(lock) {
                if (isCurrentPrefetchLocked(requestGeneration)) prefetchJob = null
            }
        }
        synchronized(lock) {
            if (isCurrentPrefetchLocked(requestGeneration)) {
                prefetchJob = job
            } else {
                job.cancel()
            }
        }
    }

    fun prefetchFirstRecommendation(
        seedUri: String,
        recommendation: LastfmTrack,
        artworkUri: String?
    ) {
        prefetchRecommendations(
            seedUri = seedUri,
            recommendations = listOf(recommendation),
            artworkByKey = mapOf(recommendationKey(recommendation) to artworkUri)
        )
    }

    fun startPreview(
        seedUri: String,
        recommendations: List<LastfmTrack>,
        startIndex: Int,
        artworkByKey: Map<String, String?>,
        deferUntilCurrentEnds: Boolean = false
    ) {
        if (recommendations.isEmpty()) return
        val controller = musicController ?: run {
            synchronized(lock) {
                _state.value = withSaveState(
                    RecommendationPreviewState(
                        seedUri = seedUri,
                        recommendations = recommendations,
                        discoverMode = _state.value.discoverMode,
                        failures = mapOf(
                            recommendationKey(
                                recommendations[startIndex.coerceIn(0, recommendations.lastIndex)]
                            ) to "Player is not ready"
                        )
                    )
                )
            }
            return
        }

        val firstIndex = startIndex.coerceIn(0, recommendations.lastIndex)
        val firstKey = recommendationKey(recommendations[firstIndex])
        val oldFiles: List<File>
        val clearOldPlayback: Boolean
        val sessionGeneration: Long
        val restoreOriginalQueue: Boolean
        val retainedAssets: Map<String, PreviewAsset>
        val retainedCurrentPreview: PreviewAsset?
        val currentTrack = controller.currentTrack.value
        val currentUri = currentTrack?.uri
        val currentIsPreview = synchronized(lock) { currentUri in keyByUri }
        val deferPlayback = shouldDeferDiscoverHandoff(
            requested = deferUntilCurrentEnds,
            currentTrack = controller.currentTrack.value,
            seedUri = seedUri
        ) || (deferUntilCurrentEnds && currentIsPreview)
        if (deferUntilCurrentEnds && !deferPlayback) return
        val discoverSession = synchronized(lock) { _state.value.discoverMode }
        val physicalQueueSnapshot = if (!currentIsPreview) {
            val queue = controller.getQueue().filterNot { it.isTransient }
            val currentIndex = queue.indexOfFirst { it.uri == currentUri }
            if (currentIndex >= 0) {
                queue
            } else {
                listOfNotNull(currentTrack?.takeUnless { it.isTransient }) +
                    queueContinuationAfterCurrent(controller, currentUri)
            }
        } else {
            emptyList()
        }
        val physicalCurrentIndex = physicalQueueSnapshot.indexOfFirst { it.uri == currentUri }
        val nextQueueTracks = if (currentIsPreview) {
            synchronized(lock) { continuationTracks }
        } else {
            physicalQueueSnapshot.drop((physicalCurrentIndex + 1).coerceAtLeast(0))
        }
        val sessionRecommendations = recommendationsForSession(
            recommendations = recommendations,
            startIndex = firstIndex,
            discoverMode = discoverSession
        )
        synchronized(lock) {
            if (!currentIsPreview && physicalCurrentIndex >= 0) {
                physicalQueueBeforePreview = physicalQueueSnapshot
                physicalCurrentIndexBeforePreview = physicalCurrentIndex
            }
            clearOldPlayback = controller.currentTrack.value?.uri in keyByUri &&
                !deferPlayback && physicalQueueBeforePreview.isEmpty()
            restoreOriginalQueue = currentIsPreview && !deferPlayback
            retainedCurrentPreview = if (deferPlayback && currentIsPreview) {
                currentUri?.let { uri ->
                    keyByUri[uri]?.let(assetsByKey::remove)
                }
            } else {
                null
            }
            val retainPrefetchedAssets = prefetchSeedUri == seedUri || currentIsPreview
            val retainedKeys = retainPreparedRecommendationKeys(
                preparedKeys = assetsByKey.keys.toSet() +
                    if (retainPrefetchedAssets) prefetchedAssetsByKey.keys else emptySet(),
                recommendations = recommendations
            )
            val retainedByKey = linkedMapOf<String, PreviewAsset>()
            fun retainPreparedAssets(source: Map<String, PreviewAsset>) {
                source.forEach { (key, asset) ->
                    if (key in retainedKeys && asset.file.isFile) {
                        retainedByKey.putIfAbsent(key, asset)
                    }
                }
            }
            // A tap can restart from any item in the temporary queue. Keep all
            // already downloaded items in the new window instead of retaining
            // only the tapped item and downloading its successors again.
            retainPreparedAssets(assetsByKey)
            if (retainPrefetchedAssets) {
                retainPreparedAssets(prefetchedAssetsByKey)
            }
            retainedAssets = retainedByKey
            generation++
            sessionGeneration = generation
            sessionJob?.cancel()
            // Keep a still-running prefetch for this seed alive: its
            // in-flight download must not be cancelled and restarted — the
            // session loop adopts completed prefetch assets and waits for
            // the in-flight one. Only a prefetch for a different seed is
            // cancelled here.
            if (!retainPrefetchedAssets) {
                prefetchGeneration++
                prefetchJob?.cancel()
                prefetchJob = null
            }
            val retainedFiles = retainedAssets.values.map { it.file }
                .plus(retainedCurrentPreview?.file)
                .toSet()
            oldFiles = (assetsByKey.values.map { it.file } +
                prefetchedAssetsByKey.values.map { it.file })
                .filterNot(filesBeingPromoted::contains)
                .filterNot(retainedFiles::contains)
            assetsByKey.clear()
            keyByUri.clear()
            observedPreviewTrack = false
            previewPlaybackStarted = false
            preparationComplete = false
            recoveringPlaybackError = false
            forcedAdvanceUri = null
            forcedAdvanceInProgress = false
            deferredInitialPlayback = deferPlayback
            queuedPreviewTailUri = if (deferPlayback) currentUri ?: seedUri else null
            if (!deferPlayback) advanceToDeferredRecommendation = false
            endAfterCurrentRecommendation = false
            discoverExtensionPending = false
            discoverPreparingKeys.clear()
            if (!currentIsPreview) {
                initialNormalTrackUri = currentTrack?.takeUnless { it.isTransient }?.uri
            }
            baselineController = controller
            baselinePlayRequestRevision = controller.playRequestRevision.value
            continuationTracks = nextQueueTracks
            _state.value = withSaveState(
                RecommendationPreviewState(
                    active = true,
                    seedUri = seedUri,
                    recommendations = recommendations.toList(),
                    discoverMode = _state.value.discoverMode,
                    selectedKey = firstKey,
                    currentKey = retainedCurrentPreview?.let { recommendationKey(it.recommendation) },
                    readyKeys = retainedAssets.keys.toSet()
                        .let { keys ->
                            retainedCurrentPreview?.let { keys + recommendationKey(it.recommendation) }
                                ?: keys
                        }
                )
            )
            retainedCurrentPreview?.let { asset ->
                val key = recommendationKey(asset.recommendation)
                assetsByKey[key] = asset
                keyByUri[asset.mediaTrack.uri] = key
            }
            retainedAssets.forEach { (key, asset) ->
                assetsByKey[key] = asset
            }
        }

        if (clearOldPlayback) controller.clearPlaybackQueue()
        deleteFiles(oldFiles)

        sessionJob = appScope.launch {
            startupCleanup.await()
            var previousPlayableTrack: MediaTrack? = null
            var preparedTrackCount = 0
            var offset = 0
            while (isCurrent(sessionGeneration)) {
                if (!isCurrent(sessionGeneration)) return@launch
                val recommendation = if (discoverSession) {
                    synchronized(lock) {
                        _state.value.recommendations.getOrNull(firstIndex + offset)
                    }
                } else {
                    sessionRecommendations.getOrNull(offset)
                }
                if (recommendation == null) {
                    synchronized(lock) {
                        if (generation == sessionGeneration) preparationComplete = true
                    }
                    finishEmptyPreviewSession(sessionGeneration)
                    val sessionStillActive = synchronized(lock) {
                        generation == sessionGeneration && _state.value.active
                    }
                    if (!discoverSession || !sessionStillActive) break
                    // Discover pages are appended while the current preview
                    // keeps playing. Stay alive at the end of the loaded page
                    // instead of ending the session before the next page is
                    // available.
                    delay(PREFETCH_POLL_INTERVAL_MS)
                    continue
                }
                val currentOffset = offset++
                if (!discoverSession && currentOffset > 0 &&
                    !synchronized(lock) { previewPlaybackStarted }
                ) {
                    continue
                }
                val key = recommendationKey(recommendation)
                previousPlayableTrack?.let { previous ->
                    updateState(sessionGeneration) { state ->
                        state.copy(waitingKeys = state.waitingKeys + key)
                    }
                    if (shouldWaitForNextPreview(preparedTrackCount) &&
                        !awaitRecommendationWindow(
                            sessionGeneration = sessionGeneration,
                            recommendations = synchronized(lock) {
                                if (discoverSession) _state.value.recommendations
                                else sessionRecommendations
                            },
                            nextRecommendationIndex = if (discoverSession) {
                                firstIndex + currentOffset
                            } else {
                                currentOffset
                            }
                        )
                    ) {
                        return@launch
                    }
                    updateState(sessionGeneration) { state ->
                        state.copy(waitingKeys = state.waitingKeys - key)
                    }
                }
                val adopted = synchronized(lock) {
                    assetsByKey[key]?.takeIf { it.file.isFile }
                        ?: prefetchedAssetsByKey[key]?.takeIf { it.file.isFile }?.also { asset ->
                            prefetchedAssetsByKey.remove(key)
                            assetsByKey[key] = asset
                            keyByUri[asset.mediaTrack.uri] = key
                        }
                }
                val mediaTrack: MediaTrack
                if (adopted != null) {
                    mediaTrack = adopted.mediaTrack
                    updateState(sessionGeneration) { state ->
                        state.copy(
                            resolvingKeys = state.resolvingKeys - key,
                            preparingKeys = state.preparingKeys - key,
                            readyKeys = state.readyKeys + key
                        )
                    }
                } else {
                    val prefetchInFlight = synchronized(lock) {
                        key in _state.value.prefetchingKeys ||
                            key in _state.value.prefetchQueuedKeys
                    }
                    val adoptedFromPrefetch = if (prefetchInFlight) {
                        awaitPrefetchAdoption(sessionGeneration, key)
                    } else {
                        null
                    }
                    if (adoptedFromPrefetch != null) {
                        mediaTrack = adoptedFromPrefetch.mediaTrack
                        updateState(sessionGeneration) { state ->
                            state.copy(
                                resolvingKeys = state.resolvingKeys - key,
                                preparingKeys = state.preparingKeys - key,
                                readyKeys = state.readyKeys + key
                            )
                        }
                    } else {
                        updateState(sessionGeneration) { state ->
                            state.copy(
                                resolvingKeys = state.resolvingKeys + key,
                                failures = state.failures - key
                            )
                        }
                        val resolved = runTimedResult(
                            timeoutMs = resolveTimeoutMs,
                            timeoutMessage = "Finding a playable source timed out. Tap to retry."
                        ) {
                            resolver(recommendation)
                        }
                        if (!isCurrent(sessionGeneration)) return@launch
                        if (resolved.isFailure) {
                            markFailure(sessionGeneration, key, resolved.errorMessage())
                            markDiscoverPreparationComplete(key)
                            continue
                        }
                        val stream = resolved.getOrThrow()
                        updateState(sessionGeneration) { state ->
                            state.copy(
                                resolvingKeys = state.resolvingKeys - key,
                                preparingKeys = state.preparingKeys + key
                            )
                        }
                        val extension = extensionForMimeType(stream.mimeType)
                        val target = File(
                            previewDirectory,
                            "$sessionGeneration-${firstIndex + currentOffset}-${safeName(recommendation)}.$extension"
                        )
                        val artworkUri = artworkByKey[key]?.takeUnless { it.isNullOrBlank() }
                            ?: stream.thumbnailUrl.takeUnless { it.isBlank() }
                        val downloaded = runTimedResult(
                            timeoutMs = downloadTimeoutMs,
                            timeoutMessage = "Preview download timed out. Tap to retry."
                        ) {
                            previewDownloader(
                                stream,
                                recommendation,
                                artworkUri,
                                target
                            )
                        }
                        if (!isCurrent(sessionGeneration)) {
                            target.delete()
                            return@launch
                        }
                        if (downloaded.isFailure) {
                            markFailure(sessionGeneration, key, downloaded.errorMessage())
                            markDiscoverPreparationComplete(key)
                            continue
                        }

                        mediaTrack = downloaded.getOrThrow()
                        val asset = PreviewAsset(
                            recommendation = recommendation,
                            file = target,
                            sourceUrl = stream.sourceUrl,
                            thumbnailUrl = stream.thumbnailUrl.ifBlank { artworkUri.orEmpty() },
                            mediaTrack = mediaTrack
                        )
                        synchronized(lock) {
                            if (generation != sessionGeneration) {
                                target.delete()
                                return@launch
                            }
                            assetsByKey[key] = asset
                            keyByUri[mediaTrack.uri] = key
                            _state.value = _state.value.copy(
                                preparingKeys = _state.value.preparingKeys - key,
                                readyKeys = _state.value.readyKeys + key
                            )
                        }
                    }
                }

                synchronized(lock) {
                    if (generation == sessionGeneration) {
                        keyByUri[mediaTrack.uri] = key
                    }
                }

                if (discoverSession || currentOffset == 0) {
                    if (!enqueuePreparedTrack(
                            sessionGeneration = sessionGeneration,
                            mediaTrack = mediaTrack
                        )
                    ) {
                        if (isCurrent(sessionGeneration)) {
                            markFailure(sessionGeneration, key, "Player is not ready")
                            markDiscoverPreparationComplete(key)
                        }
                        return@launch
                    }
                }
                markDiscoverPreparationComplete(key)
                previousPlayableTrack = mediaTrack
                preparedTrackCount++
            }

            synchronized(lock) {
                if (generation == sessionGeneration) preparationComplete = true
            }
            if (restoreOriginalQueue && !synchronized(lock) { previewPlaybackStarted }) {
                restoreQueueContinuation(sessionGeneration)
            } else if (!synchronized(lock) { deferredInitialPlayback }) {
                appendQueueContinuation(sessionGeneration)
            }
            finishEmptyPreviewSession(sessionGeneration)
        }
    }

    /**
     * Handles Next while a recommendation preview is active. In normal
     * (non-Discover) mode the transport buttons navigate the surrounding
     * physical library: Next leaves the preview session and plays the
     * physical item after the captured seed, mirroring [skipToPrevious].
     * Discover mode routes Next through the queued recommendations, waking
     * the preparation loop when the next item is not prepared yet.
     */
    fun skipToNext(): Boolean {
        val controller = musicController ?: return false
        val currentTrack = controller.currentTrack.value ?: return false
        val discoverMode: Boolean
        val active: Boolean
        val currentKey: String?
        synchronized(lock) {
            discoverMode = _state.value.discoverMode
            active = _state.value.active
            currentKey = if (active) keyByUri[currentTrack.uri] else null
        }
        if (!active) return false
        if (!discoverMode) {
            if (currentKey == null) return false
            val physicalContext = synchronized(lock) {
                if (physicalQueueBeforePreview.isEmpty() ||
                    physicalCurrentIndexBeforePreview !in physicalQueueBeforePreview.indices
                ) {
                    null
                } else {
                    PreviewPlaybackContext(
                        tracks = physicalQueueBeforePreview,
                        startIndex = (physicalCurrentIndexBeforePreview + 1)
                            .coerceAtMost(physicalQueueBeforePreview.lastIndex)
                    )
                }
            }
            if (physicalContext != null) {
                val moved = controller.play(physicalContext.tracks, physicalContext.startIndex)
                if (moved) finishForPlaybackIntervention()
                return moved
            }
            val next = controller.getNextPlaybackItem() ?: return false
            if (synchronized(lock) { keyByUri.containsKey(next.uri) }) return false
            return controller.skipToNextPlaybackItem()
        }
        if (currentKey == null) {
            val waitingForDiscoverHandoff = synchronized(lock) {
                isDiscoverHandoffPending(
                    discoverMode = _state.value.discoverMode,
                    currentTrack = currentTrack,
                    deferredInitialPlayback = deferredInitialPlayback,
                    initialNormalTrackUri = initialNormalTrackUri,
                    discoverRequestPending = discoverRequestJob != null,
                    discoverStartedUri = discoverStartedUri
                )
            }
            if (!waitingForDiscoverHandoff) return false

            val nextIsQueuedPreview = controller.getNextPlaybackItem()?.let { next ->
                synchronized(lock) { keyByUri.containsKey(next.uri) }
            } == true
            if (nextIsQueuedPreview) {
                controller.skipToNextPlaybackItem()
                return true
            }
            // The deferred block is still being prepared (Last.fm fetch or
            // the first preview download can take tens of seconds): arm the
            // auto-advance on the first press, but never swallow repeated
            // presses — they fall through to the player's own skip so the
            // media keys move as soon as the block is enqueued instead of
            // feeling dead while downloads are pending.
            val alreadyArmed = synchronized(lock) {
                if (advanceToDeferredRecommendation) {
                    true
                } else {
                    advanceToDeferredRecommendation = true
                    false
                }
            }
            return !alreadyArmed
        }

        val nextIsQueuedPreview = controller.getNextPlaybackItem()?.let { next ->
            synchronized(lock) { keyByUri.containsKey(next.uri) }
        } == true
        if (nextIsQueuedPreview) {
            controller.skipToNextPlaybackItem()
            return true
        }

        val recommendations = synchronized(lock) { _state.value.recommendations }
        val currentIndex = recommendations.indexOfFirst {
            recommendationKey(it) == currentKey
        }
        if (currentIndex < 0 || currentIndex >= recommendations.lastIndex) return false

        val seedUri = synchronized(lock) { _state.value.seedUri }
        if (seedUri != null) {
            startPreview(
                seedUri = seedUri,
                recommendations = recommendations,
                startIndex = currentIndex + 1,
                artworkByKey = emptyMap()
            )
            return true
        }

        synchronized(lock) {
            forcedAdvanceUri = currentTrack.uri
            forcedAdvanceInProgress = true
            val skippedAsset = assetsByKey.remove(currentKey)
            keyByUri.remove(currentTrack.uri)
            _state.value = _state.value.copy(
                readyKeys = _state.value.readyKeys - currentKey,
                currentKey = null
            )
            skippedAsset?.file?.let { deleteFiles(listOf(it)) }
        }
        if (!controller.removeCurrentFromPlaybackContext()) {
            synchronized(lock) {
                forcedAdvanceUri = null
                forcedAdvanceInProgress = false
            }
            return false
        }
        return true
    }

    /** Handles Previous inside a recommendation context when possible. */
    fun skipToPrevious(): Boolean {
        val controller = musicController ?: return false
        val currentTrack = controller.currentTrack.value ?: return false
        val discoverMode: Boolean
        val active: Boolean
        val currentKey: String?
        synchronized(lock) {
            discoverMode = _state.value.discoverMode
            active = _state.value.active
            currentKey = if (active) keyByUri[currentTrack.uri] else null
        }
        if (!active || currentKey == null) return false

        if (!discoverMode) {
            val physicalContext = synchronized(lock) {
                if (physicalQueueBeforePreview.isEmpty() ||
                    physicalCurrentIndexBeforePreview !in physicalQueueBeforePreview.indices
                ) {
                    null
                } else {
                    PreviewPlaybackContext(
                        tracks = physicalQueueBeforePreview,
                        startIndex = physicalCurrentIndexBeforePreview
                    )
                }
            } ?: return false
            val moved = controller.play(physicalContext.tracks, physicalContext.startIndex)
            if (moved) finishForPlaybackIntervention()
            return moved
        }

        val previous = controller.getPreviousPlaybackItem()
        if (previous != null) return controller.skipToPreviousPlaybackItem()
        if (!discoverMode) return false

        val recommendations: List<LastfmTrack>
        val seedUri: String?
        val currentRecommendationIndex: Int
        synchronized(lock) {
            recommendations = _state.value.recommendations
            seedUri = _state.value.seedUri
            currentRecommendationIndex = recommendations.indexOfFirst {
                recommendationKey(it) == currentKey
            }
        }
        if (currentRecommendationIndex <= 0 || seedUri == null) return false

        startPreview(
            seedUri = seedUri,
            recommendations = recommendations,
            startIndex = currentRecommendationIndex - 1,
            artworkByKey = emptyMap()
        )
        return true
    }

    suspend fun saveToPlaylist(
        recommendation: LastfmTrack,
        playlistId: Long,
        artworkUri: String?
    ): Result<RecommendationSaveOutcome> {
        val key = recommendationKey(recommendation)
        val reserved = synchronized(lock) {
            if (key in savingKeys || key in queuedJobByKey) {
                false
            } else {
                savingKeys += key
                saveFailures -= key
                _state.value = _state.value.copy(failures = _state.value.failures - key)
                publishSaveState()
                true
            }
        }
        if (!reserved) {
            return Result.failure(IllegalStateException("This recommendation is already being saved"))
        }

        var promotedFile: File? = null
        return try {
            val savedUri = synchronized(lock) { savedTrackUris[key] }
            if (savedUri != null) {
                playlistRepository.addTrackToPlaylist(playlistId, savedUri)
                finishImmediateSave(key)
                return Result.success(RecommendationSaveOutcome.AddedToPlaylist(savedUri))
            }

            val asset = synchronized(lock) { assetsByKey[key] }
            if (asset != null && asset.file.isFile) {
                promotedFile = asset.file
                synchronized(lock) { filesBeingPromoted += asset.file }
                val track = downloadRepository.promotePreview(
                    previewFile = asset.file,
                    sourceUrl = asset.sourceUrl,
                    title = recommendation.title,
                    artist = recommendation.artist,
                    playlistId = playlistId,
                    artworkUrl = asset.thumbnailUrl.ifBlank { artworkUri.orEmpty() },
                    artworkPath = asset.mediaTrack.artworkUri
                )
                synchronized(lock) {
                    savedTrackUris[key] = track.uri
                    savingKeys -= key
                    publishSaveState()
                }
                return Result.success(RecommendationSaveOutcome.Saved(track))
            }

            val resolved = durableResolver(recommendation).getOrThrow()
            val jobId = downloadRepository.enqueueDownload(
                sourceUrl = resolved.sourceUrl,
                title = recommendation.title,
                artist = recommendation.artist,
                playlistId = playlistId,
                thumbnailUrl = resolved.thumbnailUrl.ifBlank { artworkUri.orEmpty() }
            )
            synchronized(lock) {
                savingKeys -= key
                queuedJobByKey[key] = jobId
                publishSaveState()
            }
            observeQueuedDownload(key, jobId)
            Result.success(RecommendationSaveOutcome.Queued(jobId))
        } catch (cancelled: CancellationException) {
            failSave(key, "Save cancelled")
            throw cancelled
        } catch (error: Exception) {
            failSave(key, error.message ?: "Could not save recommendation")
            Result.failure(error)
        } finally {
            promotedFile?.let { file ->
                val deleteAfterPromotion = synchronized(lock) {
                    filesBeingPromoted -= file
                    assetsByKey.values.none { it.file == file }
                }
                if (deleteAfterPromotion) deleteFiles(listOf(file))
            }
        }
    }

    suspend fun saveCurrentToPlaylist(
        playlistId: Long,
        previewUri: String
    ): Result<RecommendationSaveOutcome> {
        val currentKey = synchronized(lock) { keyByUri[previewUri] }
            ?: return Result.failure(
                IllegalStateException("This temporary preview is no longer available")
            )
        val asset = synchronized(lock) { assetsByKey[currentKey] }
            ?: return Result.failure(IllegalStateException("The temporary preview is not ready"))
        return saveToPlaylist(
            recommendation = asset.recommendation,
            playlistId = playlistId,
            artworkUri = asset.mediaTrack.artworkUri
        )
    }

    private fun finishForPlaybackIntervention(restoreNormalQueue: Boolean = false) {
        val files: List<File>
        val previewUris: Set<String>
        val normalQueue: List<MediaTrack>
        val normalQueueStartIndex: Int
        val controller: MusicController?
        synchronized(lock) {
            val previousPreviewState = _state.value
            generation++
            sessionJob?.cancel()
            sessionJob = null
            controller = musicController
            val keepDeferredNormalPlayback = restoreNormalQueue &&
                deferredInitialPlayback &&
                controller?.currentTrack?.value?.uri == initialNormalTrackUri
            val hasPhysicalQueue = restoreNormalQueue &&
                !keepDeferredNormalPlayback &&
                physicalQueueBeforePreview.isNotEmpty()
            normalQueue = if (restoreNormalQueue && !keepDeferredNormalPlayback) {
                physicalQueueBeforePreview.ifEmpty { continuationTracks }
            } else {
                emptyList()
            }
            normalQueueStartIndex = if (hasPhysicalQueue) {
                // Resume at the physical track that is currently playing (the
                // natural handoff already advanced into the library); when the
                // session ends while a temporary recommendation is current
                // (Discover turned off after the current song), continue the
                // library at the seed's successor in normal order.
                val currentUri = controller?.currentTrack?.value?.uri
                val currentIndex = currentUri?.let { uri ->
                    normalQueue.indexOfFirst { it.uri == uri }.takeIf { it >= 0 }
                }
                currentIndex ?: (physicalCurrentIndexBeforePreview + 1)
                    .coerceIn(0, normalQueue.lastIndex)
            } else {
                0
            }
            // When the session ends with a physical-context restore (Discover
            // toggled off, natural handoff), keep the still-cached session
            // assets belonging to the current recommendation list so neither
            // the passive prefetch nor the next Discover session re-downloads
            // them, and their checkmarks persist.
            val retainedForPrefetch: Map<String, PreviewAsset>
            val previousSeedUri = previousPreviewState.seedUri
            val previousRecommendations = previousPreviewState.recommendations
            retainedForPrefetch = if (restoreNormalQueue && previousSeedUri != null) {
                val allowedKeys = previousRecommendations
                    .map(::recommendationKey)
                    .toSet()
                val kept = linkedMapOf<String, PreviewAsset>()
                assetsByKey.forEach { (key, asset) ->
                    if (key in allowedKeys && asset.file.isFile) kept[key] = asset
                }
                prefetchedAssetsByKey.clear()
                prefetchedAssetsByKey.putAll(kept)
                prefetchSeedUri = previousSeedUri.takeIf { kept.isNotEmpty() }
                kept
            } else {
                emptyMap()
            }
            files = assetsByKey.values.map { it.file }
                .filterNot(filesBeingPromoted::contains)
                .filterNot { file -> retainedForPrefetch.values.any { it.file == file } }
            previewUris = keyByUri.keys.toSet()
            assetsByKey.clear()
            keyByUri.clear()
            observedPreviewTrack = false
            previewPlaybackStarted = false
            preparationComplete = false
            recoveringPlaybackError = false
            initialNormalTrackUri = null
            baselineController = null
            continuationTracks = emptyList()
            forcedAdvanceUri = null
            forcedAdvanceInProgress = false
            deferredInitialPlayback = false
            queuedPreviewTailUri = null
            advanceToDeferredRecommendation = false
            endAfterCurrentRecommendation = false
            discoverExtensionPending = false
            discoverPreparingKeys.clear()
            physicalQueueBeforePreview = emptyList()
            physicalCurrentIndexBeforePreview = -1
            _state.value = withSaveState(
                RecommendationPreviewState(
                    seedUri = previousPreviewState.seedUri,
                    recommendations = previousPreviewState.recommendations,
                    discoverMode = _state.value.discoverMode,
                    prefetchedKeys = retainedForPrefetch.keys.toSet()
                )
            )
        }
        appScope.launch(Dispatchers.Main.immediate) {
            controller?.removeTransientItemsFromPlaybackContext(previewUris)
            if (restoreNormalQueue && normalQueue.isNotEmpty()) {
                controller?.play(normalQueue, normalQueueStartIndex)
            }
            withContext(Dispatchers.IO) { files.forEach { it.delete() } }
        }
    }

    private suspend fun enqueuePreparedTrack(
        sessionGeneration: Long,
        mediaTrack: MediaTrack
    ): Boolean {
        while (isCurrent(sessionGeneration)) {
            val controller = awaitController(sessionGeneration) ?: return false
            val firstTrack = synchronized(lock) { !previewPlaybackStarted }
            val deferPlayback = synchronized(lock) { deferredInitialPlayback }
            val insertionAnchor = synchronized(lock) { queuedPreviewTailUri }
            val discoverPreview = synchronized(lock) { _state.value.discoverMode }
            val dispatchResult = withContext(Dispatchers.Main.immediate) {
                if (!isCurrent(sessionGeneration) || musicController !== controller ||
                    !controller.isConnected.value
                ) {
                    PreviewDispatchResult.RETRY
                } else if (normalPlaybackSupersededPreview(controller)) {
                    PreviewDispatchResult.INTERVENED
                } else {
                    val revisionBefore = controller.queueRevision.value
                    val accepted = if (deferPlayback) {
                        // The deferred Discover block is inserted silently: an
                        // already-ended/idle player must not start the first
                        // recommendation by itself — only the natural end of
                        // the current song hands playback over to the block.
                        controller.insertIntoPlaybackContext(
                            track = mediaTrack,
                            afterUri = insertionAnchor,
                            insertBeforeCurrentIfAnchorMissing = true,
                            resumeWhenIdle = false
                        )
                    } else if (firstTrack) {
                        controller.playSequential(mediaTrack)
                    } else if (discoverPreview && insertionAnchor != null) {
                        controller.insertIntoPlaybackContext(
                            track = mediaTrack,
                            afterUri = insertionAnchor
                        )
                    } else {
                        controller.appendToPlaybackContext(mediaTrack)
                    }
                    if (firstTrack) {
                        synchronized(lock) {
                            baselineController = controller
                            baselinePlayRequestRevision = controller.playRequestRevision.value
                        }
                    }
                    if (accepted && controller.queueRevision.value != revisionBefore) {
                        val advanceNow = synchronized(lock) {
                            deferPlayback && firstTrack && advanceToDeferredRecommendation
                        }
                        if (advanceNow) {
                            controller.skipToNext()
                            synchronized(lock) {
                                advanceToDeferredRecommendation = false
                            }
                        }
                        PreviewDispatchResult.SUCCESS
                    } else {
                        PreviewDispatchResult.RETRY
                    }
                }
            }
            when (dispatchResult) {
                PreviewDispatchResult.SUCCESS -> {
                    synchronized(lock) {
                        if (generation != sessionGeneration) return false
                        if (firstTrack) previewPlaybackStarted = true
                        if (deferredInitialPlayback || discoverPreview) {
                            queuedPreviewTailUri = mediaTrack.uri
                        }
                    }
                    return true
                }
                PreviewDispatchResult.INTERVENED -> {
                    finishForPlaybackIntervention()
                    return false
                }
                PreviewDispatchResult.RETRY -> delay(CONTROLLER_WAIT_INTERVAL_MS)
            }
        }
        return false
    }

    /**
     * Waits (bounded) for the passive prefetch to finish the currently
     * in-flight/queued item and adopts it, so toggling a session on never
     * cancels and restarts the download. Returns null when the prefetch
     * failed or abandoned the item or the deadline passed — the caller then
     * resolves it itself.
     */
    private suspend fun awaitPrefetchAdoption(
        sessionGeneration: Long,
        key: String
    ): PreviewAsset? {
        val deadline = System.currentTimeMillis() + PREFETCH_ADOPT_TIMEOUT_MS
        while (isCurrent(sessionGeneration) && System.currentTimeMillis() < deadline) {
            val adopted = synchronized(lock) {
                val asset = prefetchedAssetsByKey[key]?.takeIf { it.file.isFile }
                if (asset != null) {
                    prefetchedAssetsByKey.remove(key)
                    assetsByKey[key] = asset
                    keyByUri[asset.mediaTrack.uri] = key
                }
                asset
            }
            if (adopted != null) return adopted
            val stillPending = synchronized(lock) {
                key in _state.value.prefetchingKeys ||
                    key in _state.value.prefetchQueuedKeys
            }
            if (!stillPending) return null
            delay(PREFETCH_POLL_INTERVAL_MS)
        }
        return null
    }

    private suspend fun awaitRecommendationWindow(
        sessionGeneration: Long,
        recommendations: List<LastfmTrack>,
        nextRecommendationIndex: Int
    ): Boolean {
        val minimumCurrentIndex = minimumCurrentIndexForRecommendation(nextRecommendationIndex)
        while (isCurrent(sessionGeneration)) {
            val forcedAdvance = synchronized(lock) {
                forcedAdvanceInProgress
            }
            if (forcedAdvance) return true
            val controller = awaitController(sessionGeneration) ?: return false
            val currentTrack = controller.currentTrack.value
            val currentKey = currentTrack?.uri?.let { uri ->
                synchronized(lock) { keyByUri[uri] }
            }
            val currentIndex = currentKey?.let { key ->
                recommendations.indexOfFirst { recommendationKey(it) == key }
            } ?: -1
            if (currentIndex >= minimumCurrentIndex) return true
            if (currentTrack == null && synchronized(lock) { previewPlaybackStarted }) {
                // The current preview can end while the next item is still
                // being prepared; inserting it will resume the player.
                return true
            }

            when {
                currentTrack != null && !currentTrack.isTransient -> {
                    val expectedInitialTrack =
                        currentTrack.uri == initialNormalTrackUri &&
                        controller.playRequestRevision.value == baselinePlayRequestRevision
                    if (!expectedInitialTrack) {
                        finishForPlaybackIntervention()
                        return false
                    }
                }
            }
            delay(PREFETCH_POLL_INTERVAL_MS)
        }
        return false
    }

    private fun queueContinuationAfterCurrent(
        controller: MusicController,
        currentUri: String?
    ): List<MediaTrack> {
        controller.getQueueAfterCurrent()?.let { queue ->
            return queue.filterNot { it.isTransient }
        }
        controller.lastDispatchedRequest?.let { request ->
            if (request.items.getOrNull(request.startIndex)?.uri == currentUri) {
                return request.items
                    .drop((request.startIndex + 1).coerceAtMost(request.items.size))
                    .filterNot { it.isTransient }
            }
        }
        return controller.pendingRequest?.let { request ->
            request.items.drop((request.startIndex + 1).coerceAtMost(request.items.size))
                .filterNot { it.isTransient }
        }.orEmpty()
    }

    private suspend fun appendQueueContinuation(sessionGeneration: Long) {
        // Only a short handoff window is appended behind the preview block:
        // the first physical track that plays ends the session, and
        // finishForPlaybackIntervention rebuilds the full physical queue in a
        // single setMediaItems call. Appending the entire library one
        // addMediaItem at a time floods the session command pipeline and the
        // single-threaded artwork enricher, delaying later dispatches by
        // seconds.
        val tracks = synchronized(lock) {
            if (generation == sessionGeneration && previewPlaybackStarted) {
                continuationTracks.take(CONTINUATION_APPEND_WINDOW)
            } else {
                emptyList()
            }
        }
        if (tracks.isEmpty()) return
        val controller = awaitController(sessionGeneration) ?: return
        withContext(Dispatchers.Main.immediate) {
            if (!isCurrent(sessionGeneration) || musicController !== controller ||
                !controller.isConnected.value || normalPlaybackSupersededPreview(controller)
            ) {
                return@withContext
            }
            tracks.forEach { track ->
                if (!enqueuePreparedTrack(sessionGeneration, track)) return@withContext
            }
        }
    }

    private suspend fun restoreQueueContinuation(sessionGeneration: Long) {
        val tracks = synchronized(lock) {
            if (generation == sessionGeneration) continuationTracks else emptyList()
        }
        val physicalContext = synchronized(lock) {
            if (generation != sessionGeneration ||
                physicalQueueBeforePreview.isEmpty() ||
                physicalCurrentIndexBeforePreview !in physicalQueueBeforePreview.indices
            ) {
                null
            } else {
                PreviewPlaybackContext(
                    tracks = physicalQueueBeforePreview,
                    startIndex = (physicalCurrentIndexBeforePreview + 1)
                        .coerceAtMost(physicalQueueBeforePreview.lastIndex)
                )
            }
        }
        if (physicalContext == null && tracks.isEmpty()) return
        val controller = awaitController(sessionGeneration) ?: return
        withContext(Dispatchers.Main.immediate) {
            if (!isCurrent(sessionGeneration) || musicController !== controller ||
                !controller.isConnected.value || normalPlaybackSupersededPreview(controller)
            ) {
                return@withContext
            }
            if (physicalContext != null) {
                controller.play(physicalContext.tracks, physicalContext.startIndex)
            } else {
                controller.play(tracks)
            }
        }
    }

    private fun normalPlaybackSupersededPreview(controller: MusicController): Boolean =
        synchronized(lock) {
            isNormalPlaybackSupersedingPreview(
                newerPlayRequest = baselineController === controller &&
                    controller.playRequestRevision.value != baselinePlayRequestRevision,
                currentTrack = controller.currentTrack.value,
                initialNormalTrackUri = initialNormalTrackUri,
                deferredInitialPlayback = deferredInitialPlayback,
                observedPreviewTrack = observedPreviewTrack,
                previewPlaybackStarted = previewPlaybackStarted
            )
        }

    private suspend fun recoverFromPlaybackError(
        controller: MusicController,
        key: String,
        message: String
    ) {
        val sessionGeneration: Long
        val failedFile: File?
        val failedUri: String?
        synchronized(lock) {
            if (!_state.value.active || recoveringPlaybackError || musicController !== controller) {
                return
            }
            recoveringPlaybackError = true
            sessionGeneration = generation
            val failedAsset = assetsByKey.remove(key)
            failedFile = failedAsset?.file
            failedUri = failedAsset?.mediaTrack?.uri
            keyByUri.entries.removeAll { it.value == key }
            _state.value = _state.value.copy(
                readyKeys = _state.value.readyKeys - key,
                currentKey = null,
                failures = _state.value.failures + (key to message)
            )
        }

        val (removed, awaitingTransition) = withContext(Dispatchers.Main.immediate) {
            val failedItemIsCurrent = controller.currentTrack.value?.uri == failedUri
            val success = musicController === controller &&
                (!failedItemIsCurrent || controller.removeCurrentFromPlaybackContext())
            success to (success && failedItemIsCurrent)
        }
        synchronized(lock) {
            if (generation == sessionGeneration && !awaitingTransition) {
                recoveringPlaybackError = false
            }
        }
        if (removed) {
            failedFile?.let { deleteFiles(listOf(it)) }
            finishEmptyPreviewSession(sessionGeneration)
        } else if (isCurrent(sessionGeneration)) {
            finishForPlaybackIntervention()
        }
    }

    private fun finishEmptyPreviewSession(sessionGeneration: Long) {
        synchronized(lock) {
            if (generation != sessionGeneration || !preparationComplete ||
                assetsByKey.isNotEmpty()
            ) {
                return
            }
            previewPlaybackStarted = false
            observedPreviewTrack = false
            recoveringPlaybackError = false
            deferredInitialPlayback = false
            queuedPreviewTailUri = null
            advanceToDeferredRecommendation = false
            discoverExtensionPending = false
            discoverPreparingKeys.clear()
            physicalQueueBeforePreview = emptyList()
            physicalCurrentIndexBeforePreview = -1
            _state.value = _state.value.copy(
                active = false,
                seedUri = null,
                currentKey = null,
                selectedKey = null
            )
        }
    }

    private fun finishImmediateSave(key: String) {
        synchronized(lock) {
            savingKeys -= key
            publishSaveState()
        }
    }

    private fun failSave(key: String, message: String) {
        synchronized(lock) {
            savingKeys -= key
            queuedJobByKey -= key
            saveFailures[key] = message
            publishSaveState()
        }
    }

    private fun observeQueuedDownload(key: String, jobId: Long) {
        appScope.launch {
            val job = downloadRepository.getAllDownloads()
                .map { jobs -> jobs.firstOrNull { it.id == jobId } }
                .first { candidate ->
                    candidate == null || candidate.state in TERMINAL_DOWNLOAD_STATES
                }
            val savedUri = if (job?.state == DownloadState.COMPLETED) {
                job.localUri.takeUnless { it.isBlank() }
            } else {
                null
            }
            synchronized(lock) {
                if (queuedJobByKey[key] != jobId) return@synchronized
                queuedJobByKey -= key
                if (savedUri != null) {
                    savedTrackUris[key] = savedUri
                    saveFailures -= key
                } else {
                    saveFailures[key] = job?.errorMessage?.ifBlank { "Download failed" }
                        ?: "Download removed"
                }
                publishSaveState()
            }
        }
    }

    private fun publishSaveState() {
        _state.value = withSaveState(_state.value)
    }

    private fun withSaveState(
        previewState: RecommendationPreviewState
    ): RecommendationPreviewState = previewState.copy(
        permanentKeys = savedTrackUris.keys.toSet(),
        savingKeys = savingKeys.toSet(),
        queuedKeys = queuedJobByKey.keys.toSet(),
        saveFailures = saveFailures.toMap()
    )

    private fun markFailure(sessionGeneration: Long, key: String, message: String) {
        updateState(sessionGeneration) { state ->
            state.copy(
                waitingKeys = state.waitingKeys - key,
                resolvingKeys = state.resolvingKeys - key,
                preparingKeys = state.preparingKeys - key,
                failures = state.failures + (key to message)
            )
        }
    }

    private fun markDiscoverPreparationComplete(key: String) {
        val shouldRecheckEndedPlayback: Boolean
        synchronized(lock) {
            val removed = discoverPreparingKeys.remove(key)
            shouldRecheckEndedPlayback = removed && discoverPreparingKeys.isEmpty()
        }
        if (shouldRecheckEndedPlayback) {
            setDiscoverExtensionPending(false)
        }
    }

    private suspend fun <T> runTimedResult(
        timeoutMs: Long,
        timeoutMessage: String,
        operation: suspend () -> Result<T>
    ): Result<T> = try {
        withTimeout(timeoutMs) { operation() }
    } catch (_: TimeoutCancellationException) {
        Result.failure(IOException(timeoutMessage))
    } catch (cancelled: CancellationException) {
        throw cancelled
    } catch (error: Exception) {
        Result.failure(error)
    }

    private fun updateState(
        sessionGeneration: Long,
        update: (RecommendationPreviewState) -> RecommendationPreviewState
    ) {
        synchronized(lock) {
            if (generation == sessionGeneration) _state.value = update(_state.value)
        }
    }

    private fun markPrefetchFailure(
        requestGeneration: Long,
        key: String,
        message: String
    ) {
        synchronized(lock) {
            if (!isCurrentPrefetchLocked(requestGeneration)) return
            _state.value = withSaveState(
                _state.value.copy(
                    prefetchingKeys = _state.value.prefetchingKeys - key,
                    prefetchedKeys = _state.value.prefetchedKeys - key,
                    failures = _state.value.failures + (key to message)
                )
            )
        }
    }

    private fun isCurrentPrefetch(requestGeneration: Long): Boolean = synchronized(lock) {
        isCurrentPrefetchLocked(requestGeneration)
    }

    private fun isCurrentPrefetchLocked(requestGeneration: Long): Boolean =
        prefetchGeneration == requestGeneration && prefetchSeedUri != null

    private fun cancelPrefetch() {
        val files: List<File>
        synchronized(lock) {
            if (prefetchSeedUri == null && prefetchJob == null &&
                prefetchedAssetsByKey.isEmpty()
            ) {
                return
            }
            prefetchGeneration++
            prefetchJob?.cancel()
            prefetchJob = null
            files = prefetchedAssetsByKey.values.map { it.file }
                .filterNot(filesBeingPromoted::contains)
            prefetchedAssetsByKey.clear()
            prefetchSeedUri = null
            if (!_state.value.active) {
                _state.value = withSaveState(
                    _state.value.copy(
                        seedUri = null,
                        recommendations = emptyList(),
                        prefetchingKeys = emptySet(),
                        prefetchQueuedKeys = emptySet(),
                        prefetchedKeys = emptySet()
                    )
                )
            }
        }
        deleteFiles(files)
    }

    private fun isCurrent(sessionGeneration: Long): Boolean = synchronized(lock) {
        generation == sessionGeneration
    }

    private suspend fun awaitController(sessionGeneration: Long): MusicController? {
        while (isCurrent(sessionGeneration)) {
            if (!isCurrent(sessionGeneration)) return null
            synchronized(lock) { musicController }
                ?.takeIf { it.isConnected.value }
                ?.let { return it }
            delay(CONTROLLER_WAIT_INTERVAL_MS)
        }
        return null
    }

    private fun deleteFiles(files: List<File>) {
        if (files.isEmpty()) return
        appScope.launch(Dispatchers.IO) { files.forEach { it.delete() } }
    }

    companion object {
        private const val CONTROLLER_WAIT_INTERVAL_MS = 100L
        private const val PREFETCH_POLL_INTERVAL_MS = 250L
        internal const val PREVIEW_RESOLVE_TIMEOUT_MS = 15_000L
        internal const val PREVIEW_DOWNLOAD_TIMEOUT_MS = 30_000L
        private const val METADATA_TIMEOUT_MS = 5_000L
        private const val MAX_PREVIEW_SOURCE_DURATION_SECONDS = 30 * 60L
        private const val MAX_PREVIEW_BYTES = 24L * 1024L * 1024L
        internal const val IMMEDIATE_PREVIEW_AHEAD_COUNT = 5
        internal const val PREFETCH_RECOMMENDATION_COUNT = 5
        private const val CONTINUATION_APPEND_WINDOW = 3
        private const val PREFETCH_ADOPT_TIMEOUT_MS = 60_000L
        internal const val RECOMMENDATION_PAGE_SIZE = 25
        internal const val PREVIEW_TOO_LARGE_MESSAGE =
            "This source is too large for a quick preview. Use Download to playlist to save the full song."
        private val TERMINAL_DOWNLOAD_STATES = setOf(
            DownloadState.COMPLETED,
            DownloadState.FAILED,
            DownloadState.CANCELLED
        )
        fun recommendationKey(track: LastfmTrack): String =
            normalizedRecommendationKey(track)

        internal fun hasReachedPrefetchThreshold(
            progressMs: Long,
            durationMs: Long
        ): Boolean {
            if (durationMs <= 0L) return false
            val thresholdMs = durationMs - durationMs / 4L
            return progressMs.coerceAtLeast(0L) >= thresholdMs
        }

        internal fun shouldWaitForNextPreview(preparedTrackCount: Int): Boolean =
            preparedTrackCount > IMMEDIATE_PREVIEW_AHEAD_COUNT

        internal fun recommendationsForSession(
            recommendations: List<LastfmTrack>,
            startIndex: Int,
            discoverMode: Boolean
        ): List<LastfmTrack> {
            if (recommendations.isEmpty()) return emptyList()
            val firstIndex = startIndex.coerceIn(0, recommendations.lastIndex)
            return if (discoverMode) {
                recommendations.drop(firstIndex)
            } else {
                (listOf(recommendations[firstIndex]) +
                    recommendations.drop(firstIndex + 1).take(PREFETCH_RECOMMENDATION_COUNT)
                ).distinctBy(::recommendationKey)
            }
        }

        internal fun retainPreparedRecommendationKeys(
            preparedKeys: Set<String>,
            recommendations: List<LastfmTrack>
        ): Set<String> = preparedKeys.intersect(recommendations.map(::recommendationKey).toSet())

        /**
         * True when normal (physical) playback has taken over from a preview
         * session. A transient recommendation still playing is part of the
         * session itself and never counts as intervention, so a restarted
         * preview session is not torn down before its first dispatch.
         */
        internal fun isNormalPlaybackSupersedingPreview(
            newerPlayRequest: Boolean,
            currentTrack: MediaTrack?,
            initialNormalTrackUri: String?,
            deferredInitialPlayback: Boolean,
            observedPreviewTrack: Boolean,
            previewPlaybackStarted: Boolean
        ): Boolean {
            val currentNormalTrack = currentTrack?.takeUnless { it.isTransient }
            val waitingForDeferredHandoff = deferredInitialPlayback &&
                !observedPreviewTrack && currentNormalTrack?.uri == initialNormalTrackUri
            return newerPlayRequest || if (waitingForDeferredHandoff) {
                false
            } else if (previewPlaybackStarted) {
                currentNormalTrack != null
            } else if (currentTrack?.isTransient == true) {
                false
            } else {
                currentNormalTrack?.uri != initialNormalTrackUri
            }
        }

        internal fun shouldDeferDiscoverHandoff(
            requested: Boolean,
            currentTrack: MediaTrack?,
            seedUri: String
        ): Boolean = requested && currentTrack?.let {
            it.uri == seedUri && !it.isTransient
        } == true

        internal fun isDiscoverHandoffPending(
            discoverMode: Boolean,
            currentTrack: MediaTrack?,
            deferredInitialPlayback: Boolean,
            initialNormalTrackUri: String?,
            discoverRequestPending: Boolean,
            discoverStartedUri: String?
        ): Boolean = discoverMode && currentTrack?.let { track ->
            !track.isTransient &&
                ((deferredInitialPlayback && track.uri == initialNormalTrackUri) ||
                    (discoverRequestPending && discoverStartedUri == track.uri))
        } == true

        /** The current item must advance before a sixth item beyond it is prepared. */
        internal fun minimumCurrentIndexForRecommendation(
            nextRecommendationIndex: Int
        ): Int = (nextRecommendationIndex - IMMEDIATE_PREVIEW_AHEAD_COUNT).coerceAtLeast(0)

        internal fun shouldContinueAfterPlaybackGap(
            observedPreviousTrack: Boolean,
            currentTrack: MediaTrack?
        ): Boolean = observedPreviousTrack && currentTrack == null

        internal fun continuationAfterCurrent(
            queue: List<MediaTrack>,
            currentUri: String?
        ): List<MediaTrack>? {
            val currentIndex = currentUri?.let { uri -> queue.indexOfFirst { it.uri == uri } } ?: -1
            if (currentIndex < 0) return null
            return queue.drop(currentIndex + 1).filterNot { it.isTransient }
        }

        private val client = OkHttpClient.Builder()
            .connectTimeout(30, TimeUnit.SECONDS)
            .readTimeout(60, TimeUnit.SECONDS)
            .callTimeout(PREVIEW_DOWNLOAD_TIMEOUT_MS, TimeUnit.MILLISECONDS)
            .followRedirects(true)
            .followSslRedirects(true)
            .connectionSpecs(listOf(ConnectionSpec.MODERN_TLS, ConnectionSpec.COMPATIBLE_TLS))
            .build()

        private val previewResolverExecutor = Executors.newFixedThreadPool(2) { runnable ->
            Thread(runnable, "luno-preview-resolver").apply { isDaemon = true }
        }
        private val durableResolverExecutor = Executors.newFixedThreadPool(2) { runnable ->
            Thread(runnable, "luno-durable-resolver").apply { isDaemon = true }
        }
        private val metadataExecutor = Executors.newSingleThreadExecutor { runnable ->
            Thread(runnable, "luno-preview-metadata").apply { isDaemon = true }
        }

        private suspend fun resolveRecommendation(
            recommendation: LastfmTrack,
            previewMode: Boolean
        ): Result<ResolvedRecommendation> {
            val operation = {
                resolveRecommendationBlocking(recommendation, previewMode)
            }
            return if (previewMode) {
                withExecutorTimeout(
                    executor = previewResolverExecutor,
                    timeoutMs = PREVIEW_RESOLVE_TIMEOUT_MS,
                    block = operation
                ) ?: Result.failure(
                    IOException("Finding a playable source timed out. Tap to retry.")
                )
            } else {
                withExecutor(durableResolverExecutor, operation)
            }
        }

        private fun resolveRecommendationBlocking(
            recommendation: LastfmTrack,
            previewMode: Boolean
        ): Result<ResolvedRecommendation> = runCatching {
            val search = WebSearchService.searchYouTube(
                "${recommendation.artist} ${recommendation.title}",
                limit = 5
            )
            val candidates = (search as? ExtractionResult.Success)?.data.orEmpty()
            val result = candidates.firstOrNull { candidate ->
                candidate.duration in 1..MAX_PREVIEW_SOURCE_DURATION_SECONDS
            } ?: candidates.firstOrNull { candidate -> candidate.duration <= 0L }
                ?: throw IOException(
                    (search as? ExtractionResult.Error)?.message
                        ?: "Could not find a song-length version on YouTube"
                )
            val audio = WebSearchService.getAudioStreamUrl(
                videoId = result.videoId,
                previewMode = previewMode
            )
            val stream = (audio as? ExtractionResult.Success)?.data
                ?: throw IOException(
                    (audio as? ExtractionResult.Error)?.message
                        ?: "Could not prepare this song"
                )
            ResolvedRecommendation(
                sourceUrl = stream.url,
                thumbnailUrl = result.thumbnailUrl.ifBlank {
                    WebSearchService.thumbnailUrlForVideoId(result.videoId)
                },
                mimeType = stream.mimeType
            )
        }

        private suspend fun downloadPreview(
            context: Context,
            resolved: ResolvedRecommendation,
            recommendation: LastfmTrack,
            artworkUri: String?,
            target: File
        ): Result<MediaTrack> = withContext(Dispatchers.IO) {
            target.parentFile?.mkdirs()
            val partial = File(target.parentFile, "${target.name}.part")
            partial.delete()
            try {
                val request = Request.Builder()
                    .url(resolved.sourceUrl.replaceFirst("http://", "https://"))
                    .header(
                        "User-Agent",
                        "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 " +
                            "(KHTML, like Gecko) Chrome/125.0 Mobile Safari/537.36"
                    )
                    .header("Referer", "https://www.youtube.com")
                    .header("Origin", "https://www.youtube.com")
                    .header("Range", "bytes=0-")
                    .build()
                client.newCall(request).execute().use { response ->
                    if (response.code != 200 && response.code != 206) {
                        throw IOException("Preview download failed (HTTP ${response.code})")
                    }
                    val body = response.body ?: throw IOException("Preview response was empty")
                    val contentLength = body.contentLength()
                    if (contentLength > MAX_PREVIEW_BYTES) {
                        throw IOException(PREVIEW_TOO_LARGE_MESSAGE)
                    }
                    FileOutputStream(partial).use { output ->
                        val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
                        var totalBytes = 0L
                        body.byteStream().use { input ->
                            while (true) {
                                currentCoroutineContext().ensureActive()
                                val read = input.read(buffer)
                                if (read == -1) break
                                totalBytes += read
                                if (totalBytes > MAX_PREVIEW_BYTES) {
                                    throw IOException(PREVIEW_TOO_LARGE_MESSAGE)
                                }
                                output.write(buffer, 0, read)
                            }
                        }
                    }
                }
                if (!partial.renameTo(target)) {
                    partial.copyTo(target, overwrite = true)
                    partial.delete()
                }
                val durationMs = withExecutorTimeout(
                    executor = metadataExecutor,
                    timeoutMs = METADATA_TIMEOUT_MS
                ) {
                    extractDuration(target)
                } ?: 0L
                if (durationMs <= 0L) {
                    throw IOException("Downloaded audio could not be played")
                }
                val localArtworkUri = resolveArtworkUri(
                    context = context,
                    preferredUri = artworkUri,
                    fallbackUri = resolved.thumbnailUrl
                )
                Result.success(
                    MediaTrack(
                        uri = Uri.fromFile(target).toString(),
                        title = recommendation.title,
                        artist = recommendation.artist,
                        durationMs = durationMs,
                        artworkUri = localArtworkUri,
                        playbackSource = PlaybackSource(
                            category = "Recommendations",
                            name = "Recommended for this song"
                        ),
                        isTransient = true
                    )
                )
            } catch (cancelled: CancellationException) {
                partial.delete()
                target.delete()
                throw cancelled
            } catch (error: Exception) {
                partial.delete()
                target.delete()
                Result.failure(error)
            }
        }

        private fun resolveArtworkUri(
            context: Context,
            preferredUri: String?,
            fallbackUri: String?
        ): String? {
            listOf(preferredUri, fallbackUri)
                .mapNotNull { it?.trim()?.takeIf(String::isNotBlank) }
                .distinct()
                .forEach { candidate ->
                    val parsed = Uri.parse(candidate)
                    if (parsed.scheme == null) {
                        val path = File(candidate)
                        if (ArtworkStorage.hasUsableArtwork(path.absolutePath)) {
                            return Uri.fromFile(path).toString()
                        }
                    } else if (parsed.scheme.equals("file", ignoreCase = true) &&
                        ArtworkStorage.hasUsableArtwork(parsed.path)
                    ) {
                        return candidate
                    } else if (parsed.scheme.equals("content", ignoreCase = true)) {
                        return candidate
                    }

                    if (parsed.scheme.equals("http", ignoreCase = true) ||
                        parsed.scheme.equals("https", ignoreCase = true)
                    ) {
                        val request = Request.Builder()
                            .url(candidate.replaceFirst("http://", "https://"))
                            .header("User-Agent", "Luno/1.0 (Android)")
                            .build()
                        val cachedPath = runCatching {
                            client.newCall(request).execute().use { response ->
                                if (!response.isSuccessful) return@use null
                                response.body?.bytes()
                                    ?.takeIf { it.isNotEmpty() }
                                    ?.let { ArtworkStorage.saveImageBytes(context, it) }
                            }
                        }.getOrNull()
                        if (!cachedPath.isNullOrBlank()) {
                            return Uri.fromFile(File(cachedPath)).toString()
                        }
                    }
                }
            return null
        }

        private fun extractDuration(file: File): Long {
            val retriever = MediaMetadataRetriever()
            return try {
                retriever.setDataSource(file.absolutePath)
                retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION)
                    ?.toLongOrNull()?.coerceAtLeast(0L) ?: 0L
            } catch (_: Exception) {
                0L
            } finally {
                runCatching { retriever.release() }
            }
        }

        internal fun extensionForMimeType(mimeType: String): String = when {
            mimeType.contains("opus", ignoreCase = true) ||
                mimeType.contains("webm", ignoreCase = true) -> "opus"
            mimeType.contains("mp4", ignoreCase = true) ||
                mimeType.contains("m4a", ignoreCase = true) -> "m4a"
            mimeType.contains("mpeg", ignoreCase = true) ||
                mimeType.contains("mp3", ignoreCase = true) -> "mp3"
            mimeType.contains("ogg", ignoreCase = true) -> "ogg"
            else -> "audio"
        }

        private fun safeName(track: LastfmTrack): String =
            "${track.artist}-${track.title}"
                .replace(Regex("[^a-zA-Z0-9_-]"), "_")
                .take(80)
                .ifBlank { "preview" }

        private fun <T> Result<T>.errorMessage(): String =
            exceptionOrNull()?.message ?: "Could not prepare this recommendation"

        private suspend fun <T> withExecutorTimeout(
            executor: java.util.concurrent.ExecutorService,
            timeoutMs: Long,
            block: () -> T
        ): T? = withTimeoutOrNull(timeoutMs) { withExecutor(executor, block) }

        private suspend fun <T> withExecutor(
            executor: java.util.concurrent.ExecutorService,
            block: () -> T
        ): T = suspendCancellableCoroutine { continuation ->
            try {
                val future = executor.submit {
                    if (!continuation.isActive) return@submit
                    val result = try {
                        Result.success(block())
                    } catch (error: Throwable) {
                        Result.failure(error)
                    }
                    runCatching { continuation.resumeWith(result) }
                }
                continuation.invokeOnCancellation { future.cancel(true) }
                if (!continuation.isActive) future.cancel(true)
            } catch (error: RejectedExecutionException) {
                continuation.resumeWith(Result.failure(error))
            }
        }
    }
}
