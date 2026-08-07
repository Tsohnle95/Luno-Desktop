package com.luno.mobile.playback

import android.content.Context
import android.media.MediaMetadataRetriever
import android.net.Uri
import com.luno.mobile.data.db.entity.DownloadState
import com.luno.mobile.data.db.entity.Track
import com.luno.mobile.data.discovery.LastfmTrack
import com.luno.mobile.data.repository.DownloadRepository
import com.luno.mobile.data.repository.PlaylistRepository
import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.util.Locale
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
    val resolvingKeys: Set<String> = emptySet(),
    val preparingKeys: Set<String> = emptySet(),
    val readyKeys: Set<String> = emptySet(),
    val failures: Map<String, String> = emptyMap(),
    val currentKey: String? = null,
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
    private val previewDownloader: suspend (
        ResolvedRecommendation,
        LastfmTrack,
        String?,
        File
    ) -> Result<MediaTrack> = { resolved, recommendation, artworkUri, target ->
        downloadPreview(resolved, recommendation, artworkUri, target)
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
                    val transition = synchronized(lock) {
                        if (!_state.value.active || musicController !== controller ||
                            !controller.isConnected.value
                        ) {
                            return@collect
                        }
                        val key = track?.uri?.let(keyByUri::get)
                        val recoveryTransition = recoveringPlaybackError
                        if (recoveryTransition) recoveringPlaybackError = false
                        when {
                            key != null -> {
                                observedPreviewTrack = true
                                _state.value = _state.value.copy(currentKey = key)
                                false
                            }
                            observedPreviewTrack && track != null -> true
                            recoveryTransition -> false
                            observedPreviewTrack -> true
                            else -> false
                        }
                    }
                    if (transition) finishForPlaybackIntervention()
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

    fun startPreview(
        seedUri: String,
        recommendations: List<LastfmTrack>,
        startIndex: Int,
        artworkByKey: Map<String, String?>
    ) {
        if (recommendations.isEmpty()) return
        val controller = musicController ?: run {
            synchronized(lock) {
                _state.value = withSaveState(
                    RecommendationPreviewState(
                        seedUri = seedUri,
                        recommendations = recommendations,
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

        val oldFiles: List<File>
        val clearOldPlayback: Boolean
        val sessionGeneration: Long
        synchronized(lock) {
            clearOldPlayback = controller.currentTrack.value?.uri in keyByUri
            generation++
            sessionGeneration = generation
            sessionJob?.cancel()
            oldFiles = assetsByKey.values.map { it.file }
                .filterNot(filesBeingPromoted::contains)
            assetsByKey.clear()
            keyByUri.clear()
            observedPreviewTrack = false
            previewPlaybackStarted = false
            preparationComplete = false
            recoveringPlaybackError = false
            initialNormalTrackUri = controller.currentTrack.value
                ?.takeUnless { it.isTransient }
                ?.uri
            baselineController = controller
            baselinePlayRequestRevision = controller.playRequestRevision.value
            _state.value = withSaveState(
                RecommendationPreviewState(
                    active = true,
                    seedUri = seedUri,
                    recommendations = recommendations.toList()
                )
            )
        }

        if (clearOldPlayback) controller.clearPlaybackQueue()
        deleteFiles(oldFiles)

        val firstIndex = startIndex.coerceIn(0, recommendations.lastIndex)
        sessionJob = appScope.launch {
            startupCleanup.await()
            for ((offset, recommendation) in recommendations.drop(firstIndex).withIndex()) {
                if (!isCurrent(sessionGeneration)) return@launch
                val key = recommendationKey(recommendation)
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
                    "$sessionGeneration-${firstIndex + offset}-${safeName(recommendation)}.$extension"
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
                    continue
                }

                val mediaTrack = downloaded.getOrThrow()
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

                if (!enqueuePreparedTrack(sessionGeneration, mediaTrack)) {
                    if (isCurrent(sessionGeneration)) {
                        markFailure(sessionGeneration, key, "Player is not ready")
                    }
                    return@launch
                }
            }

            synchronized(lock) {
                if (generation == sessionGeneration) preparationComplete = true
            }
            finishEmptyPreviewSession(sessionGeneration)
        }
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
                    artworkUrl = asset.thumbnailUrl.ifBlank { artworkUri.orEmpty() }
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

    private fun finishForPlaybackIntervention() {
        val files: List<File>
        val previewUris: Set<String>
        val controller: MusicController?
        synchronized(lock) {
            generation++
            sessionJob?.cancel()
            sessionJob = null
            controller = musicController
            files = assetsByKey.values.map { it.file }
                .filterNot(filesBeingPromoted::contains)
            previewUris = keyByUri.keys.toSet()
            assetsByKey.clear()
            keyByUri.clear()
            observedPreviewTrack = false
            previewPlaybackStarted = false
            preparationComplete = false
            recoveringPlaybackError = false
            initialNormalTrackUri = null
            baselineController = null
            _state.value = withSaveState(RecommendationPreviewState())
        }
        appScope.launch(Dispatchers.Main.immediate) {
            controller?.removeTransientItemsFromPlaybackContext(previewUris)
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
            val dispatchResult = withContext(Dispatchers.Main.immediate) {
                if (!isCurrent(sessionGeneration) || musicController !== controller ||
                    !controller.isConnected.value
                ) {
                    PreviewDispatchResult.RETRY
                } else if (normalPlaybackSupersededPreview(controller)) {
                    PreviewDispatchResult.INTERVENED
                } else {
                    val revisionBefore = controller.queueRevision.value
                    val accepted = if (firstTrack) {
                        controller.playSequential(mediaTrack)
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

    private fun normalPlaybackSupersededPreview(controller: MusicController): Boolean =
        synchronized(lock) {
            val newerPlayRequest = baselineController === controller &&
                controller.playRequestRevision.value != baselinePlayRequestRevision
            val currentNormalTrack = controller.currentTrack.value?.takeUnless { it.isTransient }
            newerPlayRequest || if (previewPlaybackStarted) {
                currentNormalTrack != null
            } else {
                currentNormalTrack?.uri != initialNormalTrackUri
            }
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
            _state.value = _state.value.copy(
                active = false,
                seedUri = null,
                currentKey = null
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
                resolvingKeys = state.resolvingKeys - key,
                preparingKeys = state.preparingKeys - key,
                failures = state.failures + (key to message)
            )
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
        internal const val PREVIEW_RESOLVE_TIMEOUT_MS = 15_000L
        internal const val PREVIEW_DOWNLOAD_TIMEOUT_MS = 30_000L
        private const val METADATA_TIMEOUT_MS = 5_000L
        private const val MAX_PREVIEW_SOURCE_DURATION_SECONDS = 30 * 60L
        private const val MAX_PREVIEW_BYTES = 24L * 1024L * 1024L
        private val TERMINAL_DOWNLOAD_STATES = setOf(
            DownloadState.COMPLETED,
            DownloadState.FAILED,
            DownloadState.CANCELLED
        )
        fun recommendationKey(track: LastfmTrack): String =
            "${track.artist.trim().lowercase(Locale.ROOT)}|" +
                track.title.trim().lowercase(Locale.ROOT)

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
                thumbnailUrl = result.thumbnailUrl,
                mimeType = stream.mimeType
            )
        }

        private suspend fun downloadPreview(
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
                        throw IOException("This source is too large for a quick preview")
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
                                    throw IOException("This source is too large for a quick preview")
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
                Result.success(
                    MediaTrack(
                        uri = Uri.fromFile(target).toString(),
                        title = recommendation.title,
                        artist = recommendation.artist,
                        durationMs = durationMs,
                        artworkUri = artworkUri,
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
