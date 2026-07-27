package com.boombastic.mobile.playback

import android.content.ComponentName
import android.content.Context
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
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
 * Must be initialized before use and released when done.
 *
 * ## Pending-play semantics
 *
 * If [playUri] or [play] is called before the underlying [MediaController]
 * is connected, the request is queued as a **pending play**. When the
 * controller connects (via [initialize]), the most recent pending request
 * (last-user-request wins) executes exactly once. This prevents silent
 * drops when the user taps a track before the async connection completes.
 *
 * If the controller connection fails, [connectionError] emits a
 * [ConnectionException] with a descriptive message.
 */
class MusicController(private val context: Context) {

    /**
     * Thrown when the connection to [MusicService] fails and a pending play
     * cannot be delivered. Callers can surface this as a recoverable error.
     */
    class ConnectionException(message: String) : Exception(message)

    private var controller: MediaController? = null

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

    private val _isConnected = MutableStateFlow(false)
    val isConnected: StateFlow<Boolean> = _isConnected.asStateFlow()

    /** Emits once if controller connection fails permanently. */
    private val _connectionError = MutableSharedFlow<ConnectionException>(extraBufferCapacity = 1)
    val connectionError: SharedFlow<ConnectionException> = _connectionError.asSharedFlow()

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var progressUpdater: Job? = null

    /** Most recent URI queued while disconnected; last-user-request wins. */
    @Volatile private var pendingUri: String? = null

    /** Whether a pending play has already been consumed after connection. */
    private var pendingPlayConsumed = false

    private val listener = object : Player.Listener {
        override fun onIsPlayingChanged(isPlaying: Boolean) {
            _isPlaying.value = isPlaying
            _hasActiveItem.value = isPlaying || (controller?.mediaItemCount ?: 0) > 0
            if (isPlaying) {
                startProgressUpdates()
            } else {
                stopProgressUpdates()
            }
        }

        override fun onMediaItemTransition(mediaItem: MediaItem?, reason: Int) {
            val metadata = mediaItem?.mediaMetadata
            _currentTrack.value = metadata?.let {
                MediaTrack(
                    uri = mediaItem.mediaId,
                    title = it.title?.toString() ?: "Unknown",
                    artist = it.artist?.toString() ?: "Unknown"
                )
            }
            _hasActiveItem.value = mediaItem != null
            _duration.value = controller?.duration ?: 0L
            _progress.value = 0L
        }

        override fun onPlaybackStateChanged(playbackState: Int) {
            if (playbackState == Player.STATE_READY) {
                _duration.value = controller?.duration ?: 0L
            }
        }
    }

    /**
     * Initializes the async connection to [MusicService].
     *
     * @param onReady Optional callback invoked on the main thread when the
     *   controller is fully connected and any pending play has been dispatched.
     *   Called **after** pending-play execution, so callers can observe the
     *   resulting player state.
     */
    fun initialize(onReady: () -> Unit = {}) {
        val sessionToken = SessionToken(
            context,
            ComponentName(context, MusicService::class.java)
        )

        val controllerFuture = MediaController.Builder(context, sessionToken)
            .buildAsync()

        controllerFuture.addListener({
            try {
                val ctrl = controllerFuture.get()
                if (ctrl != null) {
                    controller = ctrl
                    ctrl.addListener(listener)
                    _hasActiveItem.value = ctrl.mediaItemCount > 0
                    _isPlaying.value = ctrl.isPlaying
                    _isConnected.value = true
                    // Flush any pending play (last-user-request wins).
                    executePendingPlay()
                    onReady()
                } else {
                    reportConnectionError("Controller future returned null")
                }
            } catch (e: Exception) {
                reportConnectionError("Failed to connect: ${e.message}", e)
            }
        }, MoreExecutors.directExecutor())
    }

    /**
     * Plays the given URI. If the controller is not yet connected, the URI
     * is stored as the pending play and will execute when the connection
     * completes. Only the **most recent** call is preserved (last-user-request
     * semantics).
     *
     * @return `true` – the request is either dispatched immediately or queued.
     *   Callers can rely on the return value always being `true` as of 2026;
     *   the return type is kept for binary compatibility.
     */
    fun playUri(uri: String): Boolean {
        val ctrl = controller
        if (ctrl != null) {
            return playOnController(ctrl, uri)
        }
        // Queue until connected; last-user-request wins.
        pendingUri = uri
        pendingPlayConsumed = false
        return true
    }

    /**
     * Plays the given list of URIs, starting at [startIndex].
     * If the controller is not yet connected, the request is stored as
     * pending. Only the most recent call to [play] or [playUri] is preserved.
     *
     * @return `true` – the request is either dispatched immediately or queued.
     */
    fun play(uris: List<String>, startIndex: Int = 0): Boolean {
        val ctrl = controller
        if (ctrl != null) {
            return playOnController(ctrl, uris, startIndex)
        }
        // Queue the first URI (the full playlist could be stored, but for
        // simplicity we capture the effective start URI).
        if (uris.isNotEmpty()) {
            pendingUri = uris[startIndex.coerceIn(0, uris.lastIndex)]
            pendingPlayConsumed = false
        }
        return true
    }

    fun togglePlayPause() {
        controller?.let {
            if (it.isPlaying) it.pause() else it.play()
        }
    }

    fun seekTo(positionMs: Long) {
        controller?.seekTo(positionMs)
    }

    fun skipToNext() {
        controller?.seekToNextMediaItem()
    }

    fun skipToPrevious() {
        controller?.seekToPreviousMediaItem()
    }

    fun stop() {
        controller?.stop()
        _isPlaying.value = false
        stopProgressUpdates()
    }

    fun release() {
        stopProgressUpdates()
        scope.cancel()
        _isConnected.value = false
        controller?.removeListener(listener)
        controller?.release()
        controller = null
        pendingUri = null
        pendingPlayConsumed = false
    }

    // ── Private helpers ──────────────────────────────────────────────────

    private fun playOnController(ctrl: MediaController, uri: String): Boolean {
        val mediaItem = MediaItem.Builder()
            .setMediaId(uri)
            .setUri(uri)
            .build()
        ctrl.apply {
            stop()
            clearMediaItems()
            addMediaItem(mediaItem)
            prepare()
            play()
        }
        startProgressUpdates()
        return true
    }

    private fun playOnController(ctrl: MediaController, uris: List<String>, startIndex: Int): Boolean {
        val items = uris.map { uri ->
            MediaItem.Builder()
                .setMediaId(uri)
                .setUri(uri)
                .build()
        }
        ctrl.apply {
            stop()
            clearMediaItems()
            addMediaItems(items)
            prepare()
            seekToDefaultPosition(startIndex)
            play()
        }
        startProgressUpdates()
        return true
    }

    /**
     * Executes the queued pending play, if any.  Idempotent — will only
     * fire once after connection.
     */
    private fun executePendingPlay() {
        val uri = pendingUri
        if (uri != null && !pendingPlayConsumed) {
            pendingPlayConsumed = true
            val ctrl = controller ?: return
            playOnController(ctrl, uri)
        }
    }

    private fun reportConnectionError(msg: String, cause: Throwable? = null) {
        _connectionError.tryEmit(ConnectionException(msg))
    }

    private fun startProgressUpdates() {
        if (progressUpdater?.isActive == true) return
        progressUpdater = scope.launch {
            while (isActive) {
                _progress.value = controller?.currentPosition ?: 0L
                delay(PROGRESS_UPDATE_INTERVAL_MS)
            }
        }
    }

    private fun stopProgressUpdates() {
        progressUpdater?.cancel()
        progressUpdater = null
    }

    companion object {
        private const val PROGRESS_UPDATE_INTERVAL_MS = 250L
    }
}

data class MediaTrack(
    val uri: String,
    val title: String,
    val artist: String
)
