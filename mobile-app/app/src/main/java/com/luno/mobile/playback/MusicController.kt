package com.luno.mobile.playback

import android.content.ComponentName
import android.content.Context
import android.os.Bundle
import android.util.Log
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
import kotlin.random.Random

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
    internal val connector: AsyncConnector = AsyncConnector.Default,
    private val onTrackPlayed: (String, Long?) -> Unit = { _, _ -> },
    private val recentlyPlayedStore: RecentlyPlayedStore =
        SharedPreferencesRecentlyPlayedStore(context)
) {

    // ── Exceptions / errors ──────────────────────────────────────────────

    /** Thrown when the connection to [MusicService] fails. */
    class ConnectionException(message: String) : Exception(message)

    /** Carries a playback error message safe for user-facing display. */
    data class PlaybackError(
        val message: String,
        val isMediaItemFailure: Boolean = false,
        val mediaUri: String? = null
    )

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

    private val _queueRevision = MutableStateFlow(0L)
    val queueRevision: StateFlow<Long> = _queueRevision.asStateFlow()

    private val _playRequestRevision = MutableStateFlow(0L)
    internal val playRequestRevision: StateFlow<Long> = _playRequestRevision.asStateFlow()

    /** Most recent first (max 100), restored from app-private storage at startup. */
    private val _recentlyPlayed = MutableStateFlow(recentlyPlayedStore.load())
    val recentlyPlayed: StateFlow<List<MediaTrack>> = _recentlyPlayed.asStateFlow()

    private val _isConnected = MutableStateFlow(false)
    val isConnected: StateFlow<Boolean> = _isConnected.asStateFlow()

    /** Media3 playback state (IDLE/READY/ENDED…), kept in sync with the player. */
    private val _playbackState = MutableStateFlow(Player.STATE_IDLE)
    val playbackState: StateFlow<Int> = _playbackState.asStateFlow()

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

    /**
     * URIs of tracks the user manually queued ("Play next" / "Add to queue").
     *
     * Mirrors the desktop `user_queue_count` model (`engine.py`): manually
     * queued items play **before** the rest of the playback context.  A
     * URI is removed when its item plays (or is skipped past), so later
     * insertions still land right after the remaining manual items.
     *
     * Main-thread only (all public API runs on the main thread).
     */
    private val manualQueueUris = mutableSetOf<String>()

    // ── Player listener ──────────────────────────────────────────────────

    private val listener = object : Player.Listener {
        override fun onIsPlayingChanged(isPlaying: Boolean) {
            try {
                _isPlaying.value = isPlaying
                _hasActiveItem.value = isPlaying || (controller?.mediaItemCount ?: 0) > 0
                if (isPlaying) startProgressUpdates() else stopProgressUpdates()
            } catch (e: Exception) {
                Log.e(TAG, "listener onIsPlayingChanged failed: ${e.message}")
            }
        }

        override fun onMediaItemTransition(mediaItem: MediaItem?, reason: Int) {
            try {
                val metadata = mediaItem?.mediaMetadata
                // The item we left just played (or was skipped past): it is no
                // longer "up next", so drop it from the manual-queue set.
                _currentTrack.value?.let { left -> manualQueueUris.remove(left.uri) }
                _currentTrack.value = metadata?.let {
                    val sessionDuration = controller?.duration ?: 0L
                    MediaTrack(
                        uri = mediaItem.mediaId,
                        title = it.title?.toString() ?: "Unknown",
                        artist = it.artist?.toString() ?: "Unknown",
                        album = it.albumTitle?.toString() ?: "",
                        durationMs = metadataDuration(it, sessionDuration),
                        artworkUri = it.artworkUri?.toString(),
                        isTransient = it.extras?.getBoolean(METADATA_TRANSIENT) == true
                    )
                }
                _hasActiveItem.value = mediaItem != null
                _duration.value = _currentTrack.value?.durationMs ?: 0L
                _progress.value = 0L
                _currentTrack.value?.let { track ->
                    if (!track.isTransient) {
                        recordRecentlyPlayed(track)
                        val playlistId = mediaItem?.mediaMetadata?.extras
                            ?.takeIf { it.containsKey(METADATA_PLAYLIST_ID) }
                            ?.getLong(METADATA_PLAYLIST_ID)
                        runCatching { onTrackPlayed(track.uri, playlistId) }
                    }
                }
            } catch (e: Exception) {
                Log.e(TAG, "listener onMediaItemTransition failed: ${e.message}")
            }
        }

        override fun onPlaybackStateChanged(playbackState: Int) {
            try {
                _playbackState.value = playbackState
                if (playbackState == Player.STATE_READY) {
                    _duration.value = controller?.duration?.coerceAtLeast(0L) ?: 0L
                }
            } catch (e: Exception) {
                Log.e(TAG, "listener onPlaybackStateChanged failed: ${e.message}")
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
                PlaybackError(
                    message = sanitizeErrorMessage(error.message ?: "Playback error"),
                    isMediaItemFailure = true,
                    mediaUri = _currentTrack.value?.uri
                )
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
    fun play(track: MediaTrack, playlistId: Long? = null): Boolean {
        return play(listOf(track), 0, playlistId)
    }

    /**
     * Plays the given list of tracks, starting at [startIndex].
     * Only the most recent call to any play method is preserved
     * (last‑user‑request wins).
     *
     * @return `true` — see [play].
     */
    fun play(
        tracks: List<MediaTrack>,
        startIndex: Int = 0,
        playlistId: Long? = null
    ): Boolean {
        if (released) return true
        if (tracks.isEmpty()) return true
        val request = PlaybackRequest(
            items = tracks.toList(),
            startIndex = startIndex.coerceIn(0, tracks.lastIndex),
            shuffle = null,
            playlistId = playlistId
        )
        return dispatchPlayback(request)
    }

    /** Plays a context in shuffle mode from a random item. */
    fun playShuffled(tracks: List<MediaTrack>, playlistId: Long? = null): Boolean {
        if (released) return true
        if (tracks.isEmpty()) return true
        val request = PlaybackRequest(
            items = tracks.toList(),
            startIndex = if (tracks.size == 1) 0 else Random.nextInt(tracks.size),
            shuffle = true,
            playlistId = playlistId
        )
        return dispatchPlayback(request)
    }

    /** Replaces playback with a finite, ordered context (shuffle/repeat off). */
    fun playSequential(track: MediaTrack): Boolean {
        if (released) return true
        return dispatchPlayback(
            PlaybackRequest(
                items = listOf(track),
                startIndex = 0,
                shuffle = false,
                repeatMode = Player.REPEAT_MODE_OFF
            )
        )
    }

    private fun dispatchPlayback(request: PlaybackRequest): Boolean {
        _playRequestRevision.value = _playRequestRevision.value + 1L
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
        safePlayerCommand("playPause") {
            controller?.let { if (it.isPlaying) it.pause() else it.play() }
        }
    }

    fun seekTo(positionMs: Long) {
        if (released) return
        safePlayerCommand("seek") {
            controller?.let { ctrl ->
                val target = positionMs.coerceAtLeast(0L).let { requested ->
                    val playerDuration = ctrl.duration
                    if (playerDuration > 0L) {
                        requested.coerceAtMost(playerDuration)
                    } else {
                        requested
                    }
                }
                ctrl.seekTo(target)
                // Reflect a completed seek immediately. The normal position
                // poll will reconcile this value with Media3 on its next tick.
                _progress.value = target
            }
        }
    }

    fun skipToNext() {
        if (released) return
        safePlayerCommand("next") { controller?.seekToNextMediaItem() }
    }

    fun skipToPrevious() {
        if (released) return
        safePlayerCommand("previous") { controller?.seekToPreviousMediaItem() }
    }

    /** Moves to the previous item in the current playback order immediately. */
    fun skipToPreviousPlaybackItem(): Boolean {
        if (released) return false
        var moved = false
        safePlayerCommand("previousPlaybackItem") {
            val ctrl = controller ?: return@safePlayerCommand
            val targetIndex = playbackNeighborIndex(ctrl, direction = -1)
                ?: return@safePlayerCommand
            ctrl.seekToDefaultPosition(targetIndex)
            moved = true
        }
        return moved
    }

    /** Moves to the next item in the current playback order immediately. */
    fun skipToNextPlaybackItem(): Boolean {
        if (released) return false
        var moved = false
        safePlayerCommand("nextPlaybackItem") {
            val ctrl = controller ?: return@safePlayerCommand
            val targetIndex = playbackNeighborIndex(ctrl, direction = 1)
                ?: return@safePlayerCommand
            ctrl.seekToDefaultPosition(targetIndex)
            moved = true
        }
        return moved
    }

    /**
     * Cycles repeat mode OFF → ALL → ONE → OFF on the connected player.
     * No-op before connection.
     */
    fun toggleRepeatMode() {
        if (released) return
        safePlayerCommand("repeatMode") {
            controller?.let { ctrl ->
                ctrl.repeatMode = when (ctrl.repeatMode) {
                    Player.REPEAT_MODE_OFF -> Player.REPEAT_MODE_ALL
                    Player.REPEAT_MODE_ALL -> Player.REPEAT_MODE_ONE
                    else -> Player.REPEAT_MODE_OFF
                }
            }
        }
    }

    /** Toggles shuffle and immediately starts a different random queue item. */
    fun toggleShuffle() {
        if (released) return
        safePlayerCommand("shuffle") {
            controller?.let { ctrl ->
                val enabled = !ctrl.shuffleModeEnabled
                ctrl.shuffleModeEnabled = enabled
                if (enabled) randomizeCurrentItem(ctrl)
                _shuffleEnabled.value = enabled
            }
        }
    }

    /**
     * Sets ExoPlayer shuffle mode explicitly on the connected player.
     * No-op before connection.
     */
    fun setShuffle(enabled: Boolean) {
        if (released) return
        safePlayerCommand("shuffle") {
            controller?.let { ctrl ->
                val wasEnabled = ctrl.shuffleModeEnabled
                ctrl.shuffleModeEnabled = enabled
                if (enabled && !wasEnabled) randomizeCurrentItem(ctrl)
                _shuffleEnabled.value = enabled
            }
        }
    }

    /** Sets one repeat mode explicitly; used by finite temporary queues. */
    fun setRepeatMode(mode: Int) {
        if (released) return
        val acceptedMode = when (mode) {
            Player.REPEAT_MODE_ONE, Player.REPEAT_MODE_ALL -> mode
            else -> Player.REPEAT_MODE_OFF
        }
        safePlayerCommand("repeatMode") {
            controller?.let { ctrl ->
                ctrl.repeatMode = acceptedMode
                _repeatMode.value = acceptedMode
            }
        }
    }

    /**
     * Snapshot of the current playback queue (in playback order) derived from
     * the connected player's media items.  Empty before connection — and
     * empty (never a crash) if the player's cached timeline is momentarily
     * inconsistent while it processes a queue change.
     */
    fun getQueue(): List<MediaTrack> {
        val ctrl = controller ?: return emptyList()
        return runCatching {
            playbackOrderIndices(ctrl).map { index ->
                mediaTrackFromItem(ctrl.getMediaItemAt(index))
            }
        }.getOrElse {
            Log.e(TAG, "getQueue failed: ${it.message}")
            emptyList()
        }
    }

    /**
     * Returns the exact playback-order continuation after the current item.
     * `null` means the current item/timeline could not be identified; an empty
     * list means the current item is known and has no following items.
     */
    fun getQueueAfterCurrent(): List<MediaTrack>? {
        val ctrl = controller ?: return null
        return runCatching {
            val playbackIndices = playbackOrderIndices(ctrl)
            val currentRawIndex = ctrl.currentMediaItemIndex
            val currentPlaybackIndex = playbackIndices.indexOf(currentRawIndex)
            if (currentPlaybackIndex < 0) return@runCatching null
            playbackIndices.drop(currentPlaybackIndex + 1).map { index ->
                mediaTrackFromItem(ctrl.getMediaItemAt(index))
            }
        }.getOrElse {
            Log.e(TAG, "getQueueAfterCurrent failed: ${it.message}")
            null
        }
    }

    /** Returns the adjacent item without materializing the complete queue. */
    fun getNextPlaybackItem(): MediaTrack? = getPlaybackNeighbor(direction = 1)

    /** Returns the adjacent item without materializing the complete queue. */
    fun getPreviousPlaybackItem(): MediaTrack? = getPlaybackNeighbor(direction = -1)

    /**
     * Inserts the track immediately after the currently playing item
     * ("Play next", desktop `_ctx_play_next` semantics: the item plays
     * before anything previously queued).  No-op before connection.
     */
    fun playNext(track: MediaTrack) {
        if (released) return
        safePlayerCommand("playNext") {
            val ctrl = controller ?: return@safePlayerCommand
            val currentIndex = ctrl.currentMediaItemIndex
            val insertAt = if (currentIndex == C.INDEX_UNSET) {
                0
            } else {
                (currentIndex + 1).coerceAtMost(ctrl.mediaItemCount)
            }
            // Single inserted items are always enriched (the notification
            // needs their artwork the moment they play).
            ctrl.addMediaItem(insertAt, buildMediaItem(track, enrichArtwork = true))
            manualQueueUris.add(track.uri)
            bumpQueueRevision()
        }
    }

    /**
     * Appends the track to the end of the user-queued ("Up Next") items so
     * it plays right after the items already queued manually and **before**
     * the rest of the playback context — the desktop
     * `queue_idx + 1 + user_queue_count` model (`engine.py`).  No-op before
     * connection.
     */
    fun addToQueue(track: MediaTrack) {
        if (released) return
        safePlayerCommand("addToQueue") {
            val ctrl = controller ?: return@safePlayerCommand
            val currentIndex = ctrl.currentMediaItemIndex
            val insertAt = if (currentIndex == C.INDEX_UNSET) {
                ctrl.mediaItemCount
            } else {
                (currentIndex + 1 + manualItemsAfterCurrent(ctrl)).coerceAtMost(ctrl.mediaItemCount)
            }
            ctrl.addMediaItem(insertAt, buildMediaItem(track, enrichArtwork = true))
            manualQueueUris.add(track.uri)
            bumpQueueRevision()
        }
    }

    /**
     * Appends to the end of the current playback context without marking the
     * item as a manually queued "Up Next" track. If the previous last item
     * already ended while this item was downloading, playback resumes here.
     */
    fun appendToPlaybackContext(track: MediaTrack): Boolean {
        if (released) return false
        val ctrl = controller ?: return false
        var appended = false
        safePlayerCommand("appendPlaybackContext") {
            val resumePlayback = ctrl.mediaItemCount == 0 ||
                ctrl.playbackState == Player.STATE_ENDED ||
                ctrl.playbackState == Player.STATE_IDLE
            ctrl.addMediaItem(buildMediaItem(track, enrichArtwork = true))
            if (resumePlayback) {
                ctrl.seekToDefaultPosition(ctrl.mediaItemCount - 1)
                ctrl.prepare()
                ctrl.play()
            }
            bumpQueueRevision()
            appended = true
        }
        return appended
    }

    /**
     * Inserts a temporary context item immediately after [afterUri]. When the
     * anchor is unavailable, the item is inserted after the current item, or
     * before it when [insertBeforeCurrentIfAnchorMissing] is set. The
     * insertion does not replace playback or mark the item as user-queued.
     * With [resumeWhenIdle] = false (the deferred Discover handoff), an
     * ended/idle player is left alone — the inserted block never starts
     * playback by itself.
     */
    fun insertIntoPlaybackContext(
        track: MediaTrack,
        afterUri: String? = null,
        insertBeforeCurrentIfAnchorMissing: Boolean = false,
        resumeWhenIdle: Boolean = true
    ): Boolean {
        if (released) return false
        val ctrl = controller ?: return false
        var inserted = false
        safePlayerCommand("insertPlaybackContext") {
            val playbackIndices = playbackOrderIndices(ctrl)
            val anchorRawIndex = afterUri
                ?.let { uri ->
                    playbackIndices.firstOrNull { index ->
                        ctrl.getMediaItemAt(index).mediaId == uri
                    }
                }
            val currentRawIndex = ctrl.currentMediaItemIndex
            val insertAt = when {
                anchorRawIndex != null -> anchorRawIndex + 1
                insertBeforeCurrentIfAnchorMissing &&
                    currentRawIndex in 0 until ctrl.mediaItemCount -> currentRawIndex
                currentRawIndex in 0 until ctrl.mediaItemCount -> currentRawIndex + 1
                else -> ctrl.mediaItemCount
            }.coerceIn(0, ctrl.mediaItemCount)
            val resumePlayback = resumeWhenIdle && (
                ctrl.mediaItemCount == 0 ||
                    ctrl.playbackState == Player.STATE_ENDED ||
                    ctrl.playbackState == Player.STATE_IDLE
                )
            ctrl.addMediaItem(insertAt, buildMediaItem(track, enrichArtwork = true))
            if (resumePlayback) {
                ctrl.seekToDefaultPosition(insertAt.coerceAtMost(ctrl.mediaItemCount - 1))
                ctrl.prepare()
                ctrl.play()
            }
            bumpQueueRevision()
            inserted = true
        }
        return inserted
    }

    /** Removes a failed current item and resumes the next queued item, if any. */
    fun removeCurrentFromPlaybackContext(): Boolean {
        if (released) return false
        val ctrl = controller ?: return false
        var removed = false
        safePlayerCommand("removeCurrentPlaybackItem") {
            val currentIndex = ctrl.currentMediaItemIndex
            if (currentIndex == C.INDEX_UNSET || currentIndex !in 0 until ctrl.mediaItemCount) {
                return@safePlayerCommand
            }
            val hasFollowingItem = currentIndex < ctrl.mediaItemCount - 1
            ctrl.removeMediaItems(0, currentIndex + 1)
            if (hasFollowingItem && ctrl.mediaItemCount > 0 && !ctrl.isPlaying) {
                ctrl.prepare()
                ctrl.play()
            }
            bumpQueueRevision()
            removed = true
        }
        return removed
    }

    /** Removes matching stale temporary items without disturbing the user's normal queue. */
    fun removeTransientItemsFromPlaybackContext(uris: Set<String>): Boolean {
        if (released) return false
        if (uris.isEmpty()) return false
        val ctrl = controller ?: return false
        var removed = false
        safePlayerCommand("removeTransientPlaybackItems") {
            val transientItems = (ctrl.mediaItemCount - 1 downTo 0).mapNotNull { index ->
                val item = ctrl.getMediaItemAt(index)
                if (item.mediaId in uris &&
                    item.mediaMetadata.extras?.getBoolean(METADATA_TRANSIENT) == true
                ) {
                    index to item.mediaId
                } else {
                    null
                }
            }
            if (transientItems.isEmpty()) return@safePlayerCommand
            transientItems.forEach { (index, _) -> ctrl.removeMediaItem(index) }
            manualQueueUris.removeAll(transientItems.map { it.second }.toSet())
            bumpQueueRevision()
            removed = true
        }
        return removed
    }

    /** Stops playback and removes every queued item, including stale temp URIs. */
    fun clearPlaybackQueue() {
        if (released) return
        pendingRequest = null
        safePlayerCommand("clearPlaybackQueue") {
            controller?.let { ctrl ->
                ctrl.stop()
                ctrl.clearMediaItems()
            }
            manualQueueUris.clear()
            _isPlaying.value = false
            _hasActiveItem.value = false
            _currentTrack.value = null
            _progress.value = 0L
            _duration.value = 0L
            bumpQueueRevision()
        }
        stopProgressUpdates()
    }

    /**
     * Moves the queue item at [fromIndex] so it plays at [toIndex].
     * Indexes are clamped to the current queue bounds; no-op before
     * connection or when both indexes are equal.
     *
     * A manual reorder resets the Up-Next accounting (the desktop
     * reorder adjusts `user_queue_count` in place; with a flat queue the
     * clean equivalent is to forget which items were manually queued).
     */
    fun moveQueueItem(fromIndex: Int, toIndex: Int) {
        if (released) return
        safePlayerCommand("moveQueueItem") {
            val ctrl = controller ?: return@safePlayerCommand
            if (ctrl.mediaItemCount == 0) return@safePlayerCommand
            val from = fromIndex.coerceIn(0, ctrl.mediaItemCount - 1)
            val to = toIndex.coerceIn(0, ctrl.mediaItemCount - 1)
            if (from == to) return@safePlayerCommand
            val playbackIndices = playbackOrderIndices(ctrl)
            val rawFrom = playbackIndices.getOrNull(from) ?: return@safePlayerCommand
            val rawTo = playbackIndices.getOrNull(to) ?: return@safePlayerCommand
            ctrl.moveMediaItem(rawFrom, rawTo)
            manualQueueUris.clear()
            bumpQueueRevision()
        }
    }

    /**
     * Clears the persisted recently-played history (desktop
     * "Clear History" action in `views/recent.py`).
     */
    fun clearRecentlyPlayed() {
        _recentlyPlayed.value = emptyList()
        recentlyPlayedStore.clear()
    }

    fun stop() {
        if (released) return
        safePlayerCommand("stop") { controller?.stop() }
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
        runCatching {
            controller?.removeListener(listener)
            controller?.release()
        }
        controller = null
    }

    // ── Private helpers ──────────────────────────────────────────────────

    private fun playOnController(ctrl: MediaController, request: PlaybackRequest): Boolean {
        lastDispatchedRequest = request
        // Enrich (notification/lock-screen artworkData) only the items the
        // listener actually needs: the play-start item plus a small window
        // around it for linear skips.  Enriching a whole multi-thousand
        // track queue reads every artwork file into memory at once (~170MB
        // for this library) and gets the process LMK/OOM-killed.
        val items = request.items.mapIndexed { index, track ->
            buildMediaItem(
                track,
                enrichArtwork = kotlin.math.abs(index - request.startIndex) <= ENRICH_WINDOW,
                playlistId = request.playlistId
            )
        }
        // A new playback context resets the Up-Next accounting (desktop:
        // `play_with_context` resets `user_queue_count` to 0).
        manualQueueUris.clear()
        safePlayerCommand("play") {
            request.shuffle?.let { ctrl.shuffleModeEnabled = it }
            request.repeatMode?.let { ctrl.repeatMode = it }
            // Atomic queue replacement: a single `setMediaItems` call (with
            // position reset) replaces the old stop+clear+add sequence.  A
            // burst of separate timeline commands leaves MediaController's
            // cached timeline/position transiently inconsistent with the
            // session (the crash class of androidx/media#86), and shuffle
            // toggles right after the burst widen that window.
            val startIndex = request.startIndex.coerceIn(0, items.lastIndex)
            ctrl.setMediaItems(items, startIndex, C.TIME_UNSET)
            ctrl.prepare()
            ctrl.play()
            _shuffleEnabled.value = ctrl.shuffleModeEnabled
            _repeatMode.value = ctrl.repeatMode
            bumpQueueRevision()
        }
        startProgressUpdates()
        return true
    }

    /**
     * Executes a MediaController command defensively: a Media3 race
     * (controller cached state briefly out of sync with the session during
     * a queue/shuffle change) must degrade to a logged, user-visible
     * Snackbar — never a hard crash.
     */
    private inline fun safePlayerCommand(
        operation: String,
        block: () -> Unit
    ) {
        try {
            block()
        } catch (e: Exception) {
            Log.e(TAG, "Player command '$operation' failed: ${e.message}")
            _playbackError.tryEmit(
                PlaybackError(sanitizeErrorMessage("$operation failed: ${e.message}"))
            )
        }
    }

    /**
     * Synchronises all state flows from the connected controller.
     * Called after every (re)connection so UI state is immediately accurate.
     */
    private fun hydrateState(ctrl: MediaController) {
        _isConnected.value = true
        _isPlaying.value = ctrl.isPlaying
        _playbackState.value = ctrl.playbackState
        _hasActiveItem.value = ctrl.mediaItemCount > 0
        _repeatMode.value = ctrl.repeatMode
        _shuffleEnabled.value = ctrl.shuffleModeEnabled
        bumpQueueRevision()

        val currentMediaItem = ctrl.currentMediaItem
        if (currentMediaItem != null) {
            val meta = currentMediaItem.mediaMetadata
            val hydratedDuration = metadataDuration(meta, ctrl.duration)
            val hydrated = MediaTrack(
                uri = currentMediaItem.mediaId,
                title = meta.title?.toString() ?: "Unknown",
                artist = meta.artist?.toString() ?: "Unknown",
                album = meta.albumTitle?.toString() ?: "",
                durationMs = hydratedDuration,
                artworkUri = meta.artworkUri?.toString(),
                isTransient = meta.extras?.getBoolean(METADATA_TRANSIENT) == true
            )
            _currentTrack.value = hydrated
            _duration.value = hydratedDuration
            _progress.value = ctrl.currentPosition.coerceAtLeast(0L)
            if (!hydrated.isTransient) recordRecentlyPlayed(hydrated)
        }

        ctrl.playerError?.let { error ->
            _playbackError.tryEmit(
                PlaybackError(
                    message = sanitizeErrorMessage(error.message ?: "Playback error"),
                    isMediaItemFailure = true,
                    mediaUri = currentMediaItem?.mediaId
                )
            )
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

    /**
     * Number of manually queued ("Up Next") items still sitting after the
     * current one — the desktop `user_queue_count`.  Later manual
     * insertions land right after them, so they play before the context.
     */
    private fun manualItemsAfterCurrent(ctrl: MediaController): Int {
        if (manualQueueUris.isEmpty()) return 0
        val currentIndex = ctrl.currentMediaItemIndex
        if (currentIndex == C.INDEX_UNSET) return manualQueueUris.size
        var count = 0
        for (i in (currentIndex + 1) until ctrl.mediaItemCount) {
            if (ctrl.getMediaItemAt(i).mediaId in manualQueueUris) count++
        }
        return count
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
    internal fun buildMediaItem(
        track: MediaTrack,
        enrichArtwork: Boolean = false,
        playlistId: Long? = null
    ): MediaItem {
        val extras = Bundle().apply {
            putLong(METADATA_DURATION_MS, track.durationMs)
            playlistId?.let { putLong(METADATA_PLAYLIST_ID, it) }
            if (track.isTransient) putBoolean(METADATA_TRANSIENT, true)
            // The session's ArtworkEnrichingCallback only loads artworkData
            // for flagged items — keeps notification/lock-screen artwork
            // without reading the whole library into memory per play.
            if (enrichArtwork) putBoolean(METADATA_ENRICH_ARTWORK, true)
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

    private fun randomizeCurrentItem(ctrl: MediaController) {
        if (ctrl.mediaItemCount <= 1) {
            if (ctrl.mediaItemCount == 1) ctrl.play()
            return
        }
        val current = ctrl.currentMediaItemIndex
        val randomIndex = if (current in 0 until ctrl.mediaItemCount) {
            // Pick uniformly from the N-1 items other than the current one.
            val randomOffset = Random.nextInt(ctrl.mediaItemCount - 1)
            randomIndexExcludingCurrent(ctrl.mediaItemCount, current, randomOffset)
        } else {
            Random.nextInt(ctrl.mediaItemCount)
        }
        ctrl.seekToDefaultPosition(randomIndex)
        ctrl.play()
    }

    private fun playbackOrderIndices(ctrl: MediaController): List<Int> {
        val timeline = ctrl.currentTimeline
        if (timeline.isEmpty) return emptyList()
        val indices = ArrayList<Int>(timeline.windowCount)
        var index = timeline.getFirstWindowIndex(ctrl.shuffleModeEnabled)
        while (index != C.INDEX_UNSET && indices.size < timeline.windowCount) {
            indices += index
            index = timeline.getNextWindowIndex(
                index,
                Player.REPEAT_MODE_OFF,
                ctrl.shuffleModeEnabled
            )
        }
        return indices
    }

    private fun playbackNeighborIndex(ctrl: MediaController, direction: Int): Int? {
        val timeline = ctrl.currentTimeline
        val currentIndex = ctrl.currentMediaItemIndex
        if (timeline.isEmpty || currentIndex == C.INDEX_UNSET) return null
        return if (direction < 0) {
            timeline.getPreviousWindowIndex(
                currentIndex,
                Player.REPEAT_MODE_OFF,
                ctrl.shuffleModeEnabled
            )
        } else {
            timeline.getNextWindowIndex(
                currentIndex,
                Player.REPEAT_MODE_OFF,
                ctrl.shuffleModeEnabled
            )
        }.takeUnless { it == C.INDEX_UNSET }
    }

    private fun getPlaybackNeighbor(direction: Int): MediaTrack? {
        val ctrl = controller ?: return null
        return runCatching {
            playbackNeighborIndex(ctrl, direction)
                ?.let(ctrl::getMediaItemAt)
                ?.let(::mediaTrackFromItem)
        }.getOrNull()
    }

    private fun mediaTrackFromItem(item: MediaItem): MediaTrack {
        val metadata = item.mediaMetadata
        return MediaTrack(
            uri = item.mediaId,
            title = metadata.title?.toString() ?: "Unknown",
            artist = metadata.artist?.toString() ?: "Unknown",
            album = metadata.albumTitle?.toString() ?: "",
            durationMs = metadataDuration(metadata, -1L),
            artworkUri = metadata.artworkUri?.toString(),
            isTransient = metadata.extras?.getBoolean(METADATA_TRANSIENT) == true
        )
    }

    private fun bumpQueueRevision() {
        _queueRevision.value++
    }

    /**
     * Records a track in the recently-played history, most recent first,
     * capped at 100 entries.  Matches the desktop convention
     * (`engine.py play_current`): a replay is moved to the front, not
     * duplicated — consecutive or not.
     */
    private fun recordRecentlyPlayed(track: MediaTrack) {
        val current = _recentlyPlayed.value
        val withoutTrack = current.filterNot { it.uri == track.uri }
        val updated = (listOf(track) + withoutTrack).take(MAX_RECENTLY_PLAYED)
        _recentlyPlayed.value = updated
        recentlyPlayedStore.save(updated)
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
        private const val MAX_RECENTLY_PLAYED = 100
        /** How many items around the play-start get notification artwork
         *  (see [playOnController]). */
        private const val ENRICH_WINDOW = 3
        internal const val METADATA_DURATION_MS =
            "com.luno.mobile.playback.DURATION_MS"
        internal const val METADATA_ENRICH_ARTWORK =
            "com.luno.mobile.playback.ENRICH_ARTWORK"
        internal const val METADATA_PLAYLIST_ID =
            "com.luno.mobile.playback.PLAYLIST_ID"
        internal const val METADATA_TRANSIENT =
            "com.luno.mobile.playback.TRANSIENT"
        private const val TAG = "MusicController"
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
    val startIndex: Int = 0,
    val shuffle: Boolean? = null,
    val playlistId: Long? = null,
    val repeatMode: Int? = null
)

/**
 * Metadata for a playable audio item.
 *
 * When originating from a Room [Track][com.luno.mobile.data.db.entity.Track],
 * title, artist, album, and durationMs contain the exact values supplied by
 * the database, including original Unicode, case, and whitespace.
 *
 * [artworkUri] is a `file://` URI pointing at cached embedded artwork
 * (see [com.luno.mobile.data.artwork.ArtworkStorage]); `null` when
 * the item has no artwork.
 */
data class MediaTrack(
    val uri: String,
    val title: String = "Unknown",
    val artist: String = "Unknown",
    val album: String = "",
    val durationMs: Long = 0L,
    val artworkUri: String? = null,
    /** Temporary previews are playable but never persisted to history/counts. */
    val isTransient: Boolean = false
)

/** Maps a uniform offset in the N-1 eligible items to the original item index. */
internal fun randomIndexExcludingCurrent(
    itemCount: Int,
    currentIndex: Int,
    randomOffset: Int
): Int {
    require(itemCount > 1) { "At least two items are required" }
    require(currentIndex in 0 until itemCount) { "Current index is out of range" }
    require(randomOffset in 0 until (itemCount - 1)) { "Random offset is out of range" }
    return if (randomOffset >= currentIndex) randomOffset + 1 else randomOffset
}

internal fun playbackNeighborIndex(
    playbackIndices: List<Int>,
    currentRawIndex: Int,
    direction: Int
): Int? {
    require(direction == -1 || direction == 1) { "Direction must be -1 or 1" }
    val currentPosition = playbackIndices.indexOf(currentRawIndex)
    return playbackIndices.getOrNull(currentPosition + direction)
}
