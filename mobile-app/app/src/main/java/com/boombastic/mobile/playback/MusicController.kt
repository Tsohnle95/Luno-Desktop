package com.boombastic.mobile.playback

import android.content.ComponentName
import android.content.Context
import android.os.Bundle
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Player
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import com.google.common.util.concurrent.ListenableFuture
import com.google.common.util.concurrent.MoreExecutors
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/**
 * Lifecycle-safe controller that connects to the Media3 service.
 *
 * ## Lifecycle
 *
 * Each instance is **single-use**: call [initialize] once, then [release] when
 * done.  After [release] the instance is **terminal** — no new connection can
 * be started and pending requests are discarded.  Legacy Boolean-returning
 * methods remain source-compatible but are no-ops (they return `true` without
 * queuing or starting playback).
 *
 * ## Pending-play semantics
 *
 * If [play] or [playUri] is called before the underlying [MediaController]
 * is connected, the request is serialised as a single immutable
 * [PlaybackRequest] (last-user-request wins).  When the controller connects
 * (via [initialize]), the most recent request executes exactly once.
 *
 * ## Thread safety
 *
 * All public methods are designed to be called from the main application
 * thread.  Connection, listener, and state transitions are serialised through
 * the main-thread dispatcher.
 */
class MusicController @JvmOverloads constructor(
    private val context: Context,
    internal val connector: AsyncConnector = AsyncConnector.Default
) {

    // ── Exceptions / errors ──────────────────────────────────────────────

    /** Thrown when the connection to [MusicService] fails. */
    class ConnectionException(message: String) : Exception(message)

    /** Carries a playback error message safe for user-facing display. */
    data class PlaybackError(val message: String)

    // ── State flows — exposed to UI ───────────────────────────────────────

    private val _isPlaying = MutableStateFlow(false)
    val isPlaying: StateFlow<Boolean> = _isPlaying.asStateFlow()

    private val _currentTrack = MutableStateFlow<MediaTrack?>(null)
    val currentTrack: StateFlow<MediaTrack?> = _currentTrack.asStateFlow()

    private val _progress = MutableStateFlow(0L)
    val progress: StateFlow<Long> = _progress.asStateFlow()

    private val _duration = MutableStateFlow(0L)
    val duration: StateFlow<Long> = _duration.asStateFlow()

    private val _hasActiveItem = MutableStateFlow(false)
    val hasActiveItem: StateFlow<Boolean> = _hasActiveItem.asStateFlow()

    private val _repeatMode = MutableStateFlow(Player.REPEAT_MODE_OFF)
    val repeatMode: StateFlow<Int> = _repeatMode.asStateFlow()

    private val _shuffleEnabled = MutableStateFlow(false)
    val shuffleEnabled: StateFlow<Boolean> = _shuffleEnabled.asStateFlow()

    private val _isConnected = MutableStateFlow(false)
    val isConnected: StateFlow<Boolean> = _isConnected.asStateFlow()

    /** Emits once on permanent connection failure. */
    private val _connectionError = MutableSharedFlow<ConnectionException>(extraBufferCapacity = 1)
    val connectionError: SharedFlow<ConnectionException> = _connectionError.asSharedFlow()

    /** Emits non‑sensitive user‑facing error messages from the player. */
    private val _playbackError = MutableSharedFlow<PlaybackError>(extraBufferCapacity = 1)
    val playbackError: SharedFlow<PlaybackError> = _playbackError.asSharedFlow()

    // ── Internal state ───────────────────────────────────────────────────

    private var controller: MediaController? = null
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var progressUpdater: Job? = null

    /** Monotonically increasing generation; incremented on every [release]. */
    private var generation = 0

    /** Once true, this instance will never connect again. */
    @Volatile private var released = false

    /**
     * Single immutable pending request.  Replaced atomically on each
     * pre‑connection [play] / [playUri] call (last‑user‑request wins).
     * `null` means no pending request.
     */
    @Volatile internal var pendingRequest: PlaybackRequest? = null
        private set

    /** Test-only hook: records every [PlaybackRequest] dispatched by [playOnController]. */
    @Volatile internal var lastDispatchedRequest: PlaybackRequest? = null
        private set

    // ── Player listener ──────────────────────────────────────────────────

    private val listener = object : Player.Listener {
        override fun onIsPlayingChanged(isPlaying: Boolean) {
            _isPlaying.value = isPlaying
            _hasActiveItem.value = isPlaying || (controller?.mediaItemCount ?: 0) > 0
            if (isPlaying) startProgressUpdates() else stopProgressUpdates()
        }

        override fun onMediaItemTransition(mediaItem: MediaItem?, reason: Int) {
            val metadata = mediaItem?.mediaMetadata
            _currentTrack.value = metadata?.let {
                val sessionDuration = controller?.duration ?: 0L
                MediaTrack(
                    uri = mediaItem.mediaId,
                    title = it.title?.toString() ?: "Unknown",
                    artist = it.artist?.toString() ?: "Unknown",
                    album = it.albumTitle?.toString() ?: "",
                    durationMs = metadataDuration(it, sessionDuration),
                    artworkUri = it.artworkUri?.toString()
                )
            }
            _hasActiveItem.value = mediaItem != null
            _duration.value = _currentTrack.value?.durationMs ?: 0L
            _progress.value = 0L
        }

        override fun onPlaybackStateChanged(playbackState: Int) {
            if (playbackState == Player.STATE_READY) {
                _duration.value = controller?.duration?.coerceAtLeast(0L) ?: 0L
            }
        }

        override fun onRepeatModeChanged(repeatMode: Int) {
            _repeatMode.value = repeatMode
        }

        override fun onShuffleModeEnabledChanged(shuffleModeEnabled: Boolean) {
            _shuffleEnabled.value = shuffleModeEnabled
        }

        override fun onPlayerError(error: androidx.media3.common.PlaybackException) {
            _playbackError.tryEmit(
                PlaybackError(sanitizeErrorMessage(error.message ?: "Playback error"))
            )
        }
    }

    // ── Public API ───────────────────────────────────────────────────────

    /**
     * Initialises the async connection to [MusicService].
     *
     * @param onReady Optional callback invoked on the main thread after the
     *   controller is connected **and** any pending play has been dispatched.
     */
    fun initialize(onReady: () -> Unit = {}) {
        if (released) return
        val currentGen = generation
        val controllerFuture = try {
            connector.connect(context)
        } catch (e: Exception) {
            reportConnectionError(
                sanitizeErrorMessage("Failed to connect: ${e.message}")
            )
            return
        }

        controllerFuture.addListener({
            // Stale future guard: release was called or a new generation started.
            if (released || generation != currentGen) {
                releaseStaleController(controllerFuture)
                return@addListener
            }
            try {
                val ctrl = controllerFuture.get()
                if (ctrl != null) {
                    controller = ctrl
                    ctrl.addListener(listener)
                    hydrateState(ctrl)
                    executePendingPlay()
                    onReady()
                } else {
                    reportConnectionError("Controller future returned null")
                }
            } catch (e: Exception) {
                reportConnectionError(
                    sanitizeErrorMessage("Failed to connect: ${e.message}")
                )
            }
        }, MoreExecutors.directExecutor())
    }

    /**
     * Plays a single track with full metadata.
     *
     * @return `true` — dispatched immediately or queued.  After [release]
     *   returns `true` without queuing (harmless no‑op).
     */
    fun play(track: MediaTrack): Boolean {
        return play(listOf(track), 0)
    }

    /**
     * Plays the given list of tracks, starting at [startIndex].
     * Only the most recent call to any play method is preserved
     * (last‑user‑request wins).
     *
     * @return `true` — see [play].
     */
    fun play(tracks: List<MediaTrack>, startIndex: Int = 0): Boolean {
        if (released) return true
        if (tracks.isEmpty()) return true
        val request = PlaybackRequest(
            items = tracks.toList(),
            startIndex = startIndex.coerceIn(0, tracks.lastIndex)
        )
        val ctrl = controller
        if (ctrl != null) {
            return playOnController(ctrl, request)
        }
        pendingRequest = request
        return true
    }

    /**
     * Plays the given URI.  Metadata will show "Unknown" / "" until the
     * track transitions and Media3 resolves it.
     *
     * @return `true` — dispatched, queued, or (after [release]) silently ignored.
     */
    fun playUri(uri: String): Boolean {
        return play(MediaTrack(uri = uri))
    }

    /**
     * Plays the given list of URIs.  Minimal metadata.
     *
     * @return `true` — see [playUri].
     */
    @JvmName("playUris")
    fun play(uris: List<String>, startIndex: Int = 0): Boolean {
        val tracks = uris.map { MediaTrack(uri = it) }
        return play(tracks, startIndex)
    }

    fun togglePlayPause() {
        if (released) return
        controller?.let {
            if (it.isPlaying) it.pause() else it.play()
        }
    }

    fun seekTo(positionMs: Long) {
        if (released) return
        controller?.seekTo(positionMs)
    }

    fun skipToNext() {
        if (released) return
        controller?.seekToNextMediaItem()
    }

    fun skipToPrevious() {
        if (released) return
        controller?.seekToPreviousMediaItem()
    }

    /**
     * Cycles repeat mode OFF → ALL → ONE → OFF on the connected player.
     * No-op before connection.
     */
    fun toggleRepeatMode() {
        if (released) return
        controller?.let { ctrl ->
            ctrl.repeatMode = when (ctrl.repeatMode) {
                Player.REPEAT_MODE_OFF -> Player.REPEAT_MODE_ALL
                Player.REPEAT_MODE_ALL -> Player.REPEAT_MODE_ONE
                else -> Player.REPEAT_MODE_OFF
            }
        }
    }

    /**
     * Toggles ExoPlayer shuffle mode on the connected player.  No-op before
     * connection.
     */
    fun toggleShuffle() {
        if (released) return
        controller?.let { ctrl ->
            ctrl.shuffleModeEnabled = !ctrl.shuffleModeEnabled
        }
    }

    /**
     * Sets ExoPlayer shuffle mode explicitly on the connected player.
     * No-op before connection.
     */
    fun setShuffle(enabled: Boolean) {
        if (released) return
        controller?.shuffleModeEnabled = enabled
    }

    /**
     * Snapshot of the current playback queue (in playback order) derived from
     * the connected player's media items.  Empty before connection.
     */
    fun getQueue(): List<MediaTrack> {
        val ctrl = controller ?: return emptyList()
        return (0 until ctrl.mediaItemCount).map { index ->
            val item = ctrl.getMediaItemAt(index)
            val meta = item.mediaMetadata
            MediaTrack(
                uri = item.mediaId,
                title = meta.title?.toString() ?: "Unknown",
                artist = meta.artist?.toString() ?: "Unknown",
                album = meta.albumTitle?.toString() ?: "",
                durationMs = metadataDuration(meta, -1L),
                artworkUri = meta.artworkUri?.toString(),
            )
        }
    }

    /**
     * Inserts the track immediately after the currently playing item
     * ("Play next").  No-op before connection.
     */
    fun playNext(track: MediaTrack) {
        if (released) return
        val ctrl = controller ?: return
        val insertAt = if (ctrl.currentMediaItemIndex == C.INDEX_UNSET) {
            ctrl.mediaItemCount
        } else {
            (ctrl.currentMediaItemIndex + 1).coerceAtMost(ctrl.mediaItemCount)
        }
        ctrl.addMediaItem(insertAt, buildMediaItem(track))
    }

    /**
     * Appends the track to the end of the playback queue.  No-op before
     * connection.
     */
    fun addToQueue(track: MediaTrack) {
        if (released) return
        controller?.addMediaItem(buildMediaItem(track))
    }

    /**
     * Moves the queue item at [fromIndex] so it plays at [toIndex].
     * Indexes are clamped to the current queue bounds; no-op before
     * connection or when both indexes are equal.
     */
    fun moveQueueItem(fromIndex: Int, toIndex: Int) {
        if (released) return
        val ctrl = controller ?: return
        if (ctrl.mediaItemCount == 0) return
        val from = fromIndex.coerceIn(0, ctrl.mediaItemCount - 1)
        val to = toIndex.coerceIn(0, ctrl.mediaItemCount - 1)
        if (from == to) return
        ctrl.moveMediaItem(from, to)
    }

    fun stop() {
        if (released) return
        controller?.stop()
        _isPlaying.value = false
        stopProgressUpdates()
    }

    /**
     * Releases the underlying Media3 controller and marks this instance as
     * **terminal**.  Idempotent — subsequent calls are no‑ops.
     *
     * After release:
     * - [play], [playUri] return `true` silently without queuing.
     * - Any late‑arriving future is released immediately.
     * - No listener callbacks or state changes occur.
     */
    fun release() {
        if (released) return
        released = true
        generation++
        pendingRequest = null
        stopProgressUpdates()
        scope.cancel()
        _isConnected.value = false
        controller?.removeListener(listener)
        controller?.release()
        controller = null
    }

    // ── Private helpers ──────────────────────────────────────────────────

    private fun playOnController(ctrl: MediaController, request: PlaybackRequest): Boolean {
        lastDispatchedRequest = request
        val items = request.items.map(::buildMediaItem)
        ctrl.apply {
            stop()
            clearMediaItems()
            addMediaItems(items)
            prepare()
            seekToDefaultPosition(request.startIndex.coerceIn(0, items.lastIndex))
            play()
        }
        startProgressUpdates()
        return true
    }

    /**
     * Synchronises all state flows from the connected controller.
     * Called after every (re)connection so UI state is immediately accurate.
     */
    private fun hydrateState(ctrl: MediaController) {
        _isConnected.value = true
        _isPlaying.value = ctrl.isPlaying
        _hasActiveItem.value = ctrl.mediaItemCount > 0
        _repeatMode.value = ctrl.repeatMode
        _shuffleEnabled.value = ctrl.shuffleModeEnabled

        val currentMediaItem = ctrl.currentMediaItem
        if (currentMediaItem != null) {
            val meta = currentMediaItem.mediaMetadata
            val hydratedDuration = metadataDuration(meta, ctrl.duration)
            _currentTrack.value = MediaTrack(
                uri = currentMediaItem.mediaId,
                title = meta.title?.toString() ?: "Unknown",
                artist = meta.artist?.toString() ?: "Unknown",
                album = meta.albumTitle?.toString() ?: "",
                durationMs = hydratedDuration,
                artworkUri = meta.artworkUri?.toString()
            )
            _duration.value = hydratedDuration
            _progress.value = ctrl.currentPosition.coerceAtLeast(0L)
        }

        if (ctrl.isPlaying) startProgressUpdates()
    }

    /**
     * Executes a queued pending request exactly once.
     */
    private fun executePendingPlay() {
        val req = pendingRequest ?: return
        pendingRequest = null // consume — guarantees exactly-once dispatch
        val ctrl = controller ?: return
        playOnController(ctrl, req)
    }

    /**
     * Stale-future guard: if the connection future completed after release
     * or a new generation was created, release any delivered controller
     * immediately and swallow the result.
     */
    private fun releaseStaleController(future: ListenableFuture<MediaController>) {
        try {
            future.get()?.release()
        } catch (_: Exception) {
            // Stale future — nothing to clean up.
        }
    }

    private fun reportConnectionError(msg: String) {
        _connectionError.tryEmit(ConnectionException(msg))
    }

    /**
     * Strips sensitive content (URIs, paths, stacks) from error messages
     * so they are safe for user‑facing display.
     */
    internal fun sanitizeErrorMessage(raw: String): String {
        // Keep only the first line and strip anything that looks like a URI or path.
        val firstLine = raw.lines().firstOrNull() ?: raw
        return firstLine
            .replace(Regex("\\b[a-zA-Z][a-zA-Z0-9+.-]*://\\S+"), "[link]")
            .replace(Regex("/\\S+/"), "")
            .trim()
            .take(200)
            .ifEmpty { "An unexpected error occurred" }
    }

    /** Builds the exact metadata-bearing item dispatched to Media3. */
    internal fun buildMediaItem(track: MediaTrack): MediaItem {
        val extras = Bundle().apply {
            putLong(METADATA_DURATION_MS, track.durationMs)
        }
        val metadataBuilder = MediaMetadata.Builder()
            .setTitle(track.title)
            .setArtist(track.artist)
            .setAlbumTitle(track.album)
            .setExtras(extras)
        track.artworkUri?.let { metadataBuilder.setArtworkUri(android.net.Uri.parse(it)) }
        return MediaItem.Builder()
            .setMediaId(track.uri)
            .setUri(track.uri)
            .setMediaMetadata(metadataBuilder.build())
            .build()
    }

    private fun metadataDuration(metadata: MediaMetadata, sessionDurationMs: Long): Long {
        if (sessionDurationMs >= 0L) return sessionDurationMs
        val extras = metadata.extras
        return if (extras?.containsKey(METADATA_DURATION_MS) == true) {
            extras.getLong(METADATA_DURATION_MS).coerceAtLeast(0L)
        } else {
            0L
        }
    }

    private fun startProgressUpdates() {
        if (progressUpdater?.isActive == true) return
        progressUpdater = scope.launch {
            while (isActive) {
                _progress.value = controller?.currentPosition?.coerceAtLeast(0L) ?: 0L
                delay(PROGRESS_UPDATE_INTERVAL_MS)
            }
        }
    }

    private fun stopProgressUpdates() {
        progressUpdater?.cancel()
        progressUpdater = null
    }

    // ── Async connector (injectable seam for testing) ────────────────────

    fun interface AsyncConnector {
        fun connect(context: Context): ListenableFuture<MediaController>

        companion object {
            val Default = AsyncConnector { ctx ->
                val token = SessionToken(ctx, ComponentName(ctx, MusicService::class.java))
                MediaController.Builder(ctx, token).buildAsync()
            }
        }
    }

    companion object {
        private const val PROGRESS_UPDATE_INTERVAL_MS = 250L
        internal const val METADATA_DURATION_MS =
            "com.boombastic.mobile.playback.DURATION_MS"
    }
}

// ── Data types ───────────────────────────────────────────────────────────

/**
 * Immutable request representing a complete ordered playback list with a
 * clamped start index.  Created by [MusicController.play] and
 * [MusicController.playUri] before or after connection.
 */
data class PlaybackRequest(
    val items: List<MediaTrack>,
    val startIndex: Int = 0
)

/**
 * Metadata for a playable audio item.
 *
 * When originating from a Room [Track][com.boombastic.mobile.data.db.entity.Track],
 * title, artist, album, and durationMs contain the exact values supplied by
 * the database, including original Unicode, case, and whitespace.
 *
 * [artworkUri] is a `file://` URI pointing at cached embedded artwork
 * (see [com.boombastic.mobile.data.artwork.ArtworkStorage]); `null` when
 * the item has no artwork.
 */
data class MediaTrack(
    val uri: String,
    val title: String = "Unknown",
    val artist: String = "Unknown",
    val album: String = "",
    val durationMs: Long = 0L,
    val artworkUri: String? = null
)
