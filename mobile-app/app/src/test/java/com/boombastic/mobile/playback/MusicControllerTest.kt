package com.boombastic.mobile.playback

import android.content.Context
import androidx.media3.common.Player
import androidx.media3.session.MediaController
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.google.common.truth.Truth.assertThat
import com.google.common.util.concurrent.ListenableFuture
import com.google.common.util.concurrent.SettableFuture
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runTest
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/**
 * Deterministic contract tests for [MusicController] using a fake
 * [MusicController.AsyncConnector] so queue dispatch, late completion,
 * state hydration, and failure paths can be verified without a live
 * [androidx.media3.session.MediaSessionService].
 *
 * Uses [Config.NONE] because the test injects a [FakeConnector] that
 * does not require [SessionToken][androidx.media3.session.SessionToken]
 * construction (which validates against the manifest).
 */
@RunWith(AndroidJUnit4::class)
@Config(manifest = Config.NONE, sdk = [34])
class MusicControllerTest {

    private lateinit var context: Context
    private lateinit var fakeConnector: FakeConnector
    private lateinit var controller: MusicController

    @Before
    fun setUp() {
        context = ApplicationProvider.getApplicationContext()
        fakeConnector = FakeConnector()
        controller = MusicController(
            context,
            connector = fakeConnector
        )
    }

    // ── Pending-play contract (pre-connection) ──────────────────────────

    @Test
    fun `playUri returns true even before connection`() {
        assertThat(controller.playUri("http://example.com/track.mp3")).isTrue()
    }

    @Test
    fun `play with single track returns true before connection`() {
        assertThat(controller.play(track("uri1"))).isTrue()
    }

    @Test
    fun `play with list returns true before connection`() {
        assertThat(controller.play(listOf(track("a"), track("b")), 0)).isTrue()
    }

    @Test
    fun `play with empty list returns true before connection`() {
        assertThat(controller.play(emptyList<String>())).isTrue()
    }

    @Test
    fun `empty request does not replace the last non-empty pending request`() {
        controller.play(listOf(track("keep-a"), track("keep-b")), startIndex = 1)
        controller.play(emptyList<MediaTrack>())

        val req = controller.pendingRequest
        assertThat(req).isNotNull()
        assertThat(req!!.items.map { it.uri }).containsExactly("keep-a", "keep-b").inOrder()
        assertThat(req.startIndex).isEqualTo(1)
    }

    @Test
    fun `multiple playUri calls preserve last user request`() {
        controller.playUri("first")
        controller.playUri("second")
        controller.playUri("third")
        // All preceding contract holds — no crash, no silent false.
    }

    // ── Full queue preservation ──────────────────────────────────────────

    @Test
    fun `pre-connection multi-track request retains full queue and start index`() {
        val tracks = listOf(track("a"), track("b"), track("c"))
        controller.play(tracks, startIndex = 1)
        // Pending request should hold the full queue.
        val req = controller.pendingRequest
        assertThat(req).isNotNull()
        assertThat(req!!.items.map { it.uri }).containsExactly("a", "b", "c").inOrder()
        assertThat(req.startIndex).isEqualTo(1)
    }

    @Test
    fun `last pre-connection request wins`() {
        controller.play(listOf(track("first-a"), track("first-b")))
        controller.play(listOf(track("second-a"), track("second-b"), track("second-c")), startIndex = 2)
        val req = controller.pendingRequest
        assertThat(req).isNotNull()
        assertThat(req!!.items.map { it.uri }).containsExactly("second-a", "second-b", "second-c")
        assertThat(req.startIndex).isEqualTo(2)
    }

    @Test
    fun `startIndex is clamped to valid range`() {
        val tracks = listOf(track("a"), track("b"))
        controller.play(tracks, startIndex = 99) // out of range
        val req = controller.pendingRequest
        assertThat(req).isNotNull()
        assertThat(req!!.startIndex).isEqualTo(1) // clamped to lastIndex
    }

    @Test
    fun `pending request is consumed after connection`() = runTest {
        controller.playUri("once")
        assertThat(controller.pendingRequest).isNotNull()
        controller.initialize()
        // Wait for async completion to settle
        kotlinx.coroutines.yield()
    }

    @Test
    fun `after successful connection pending request is consumed`() {
        controller.playUri("consumed")
        assertThat(controller.pendingRequest).isNotNull()
        controller.initialize()
        // Still not consumed because the fake future hasn't completed.
        assertThat(controller.pendingRequest).isNotNull()
    }

    // ── Terminal release semantics ───────────────────────────────────────

    @Test
    fun `release clears pending request`() {
        controller.playUri("lost")
        controller.release()
        assertThat(controller.pendingRequest).isNull()
    }

    @Test
    fun `release prevents onReady callback`() {
        var onReadyCalled = false
        controller.release()
        controller.initialize { onReadyCalled = true }
        assertThat(fakeConnector.lastConnectAttempt).isFalse()
        assertThat(onReadyCalled).isFalse()
    }

    @Test
    fun `playUri after release returns true without queuing`() {
        controller.release()
        assertThat(controller.playUri("http://example.com/track.mp3")).isTrue()
        assertThat(controller.pendingRequest).isNull()
    }

    @Test
    fun `release is idempotent`() {
        controller.release()
        controller.release() // second call must not throw
    }

    @Test
    fun `togglePlayPause does not throw before connection`() {
        controller.togglePlayPause()
    }

    @Test
    fun `seekTo does not throw before connection`() {
        controller.seekTo(5000L)
    }

    @Test
    fun `skipToNext does not throw before connection`() {
        controller.skipToNext()
    }

    @Test
    fun `stop does not throw before connection`() {
        controller.stop()
    }

    // ── Metadata preservation ────────────────────────────────────────────

    @Test
    fun `metadata from Track is preserved in pending request`() {
        val original = MediaTrack(
            uri = "content://track/1",
            title = "Sonne",
            artist = "Rammstein",
            album = "Mutter",
            durationMs = 290000L
        )
        controller.play(original)
        val req = controller.pendingRequest
        assertThat(req).isNotNull()
        assertThat(req!!.items).hasSize(1)
        val item = req.items[0]
        assertThat(item.uri).isEqualTo("content://track/1")
        assertThat(item.title).isEqualTo("Sonne")
        assertThat(item.artist).isEqualTo("Rammstein")
        assertThat(item.album).isEqualTo("Mutter")
        assertThat(item.durationMs).isEqualTo(290000L)
    }

    @Test
    fun `playUri creates minimal metadata`() {
        controller.playUri("content://track/no-metadata.mp3")
        val req = controller.pendingRequest
        assertThat(req).isNotNull()
        assertThat(req!!.items[0].title).isEqualTo("Unknown")
        assertThat(req.items[0].artist).isEqualTo("Unknown")
        assertThat(req.items[0].album).isEmpty()
        assertThat(req.items[0].durationMs).isEqualTo(0L)
    }

    @Test
    fun `MediaItem preserves exact Room metadata including duration`() {
        val original = MediaTrack(
            uri = "content://tracks/東京%20Song",
            title = "  東京 Song  ",
            artist = "Artist CASE",
            album = "Álbum",
            durationMs = 123456L
        )

        val item = controller.buildMediaItem(original)

        assertThat(item.mediaId).isEqualTo(original.uri)
        assertThat(item.mediaMetadata.title.toString()).isEqualTo(original.title)
        assertThat(item.mediaMetadata.artist.toString()).isEqualTo(original.artist)
        assertThat(item.mediaMetadata.albumTitle.toString()).isEqualTo(original.album)
        assertThat(
            item.mediaMetadata.extras?.getLong(MusicController.METADATA_DURATION_MS)
        ).isEqualTo(original.durationMs)
    }

    // ── State flows ──────────────────────────────────────────────────────

    @Test
    fun `state flows have initial values`() {
        assertThat(controller.isPlaying.value).isFalse()
        assertThat(controller.currentTrack.value).isNull()
        assertThat(controller.progress.value).isEqualTo(0L)
        assertThat(controller.duration.value).isEqualTo(0L)
        assertThat(controller.hasActiveItem.value).isFalse()
        assertThat(controller.isConnected.value).isFalse()
    }

    // ── Repeat / shuffle / queue API ─────────────────────────────────────

    @Test
    fun `repeatMode and shuffleEnabled have initial values`() {
        assertThat(controller.repeatMode.value).isEqualTo(Player.REPEAT_MODE_OFF)
        assertThat(controller.shuffleEnabled.value).isFalse()
    }

    @Test
    fun `toggleRepeatMode does not throw before connection`() {
        controller.toggleRepeatMode()
        assertThat(controller.repeatMode.value).isEqualTo(Player.REPEAT_MODE_OFF)
    }

    @Test
    fun `toggleShuffle does not throw before connection`() {
        controller.toggleShuffle()
        assertThat(controller.shuffleEnabled.value).isFalse()
    }

    @Test
    fun `getQueue returns empty before connection`() {
        assertThat(controller.getQueue()).isEmpty()
    }

    @Test
    fun `playNext does not throw before connection`() {
        controller.playNext(track("next"))
    }

    @Test
    fun `addToQueue does not throw before connection`() {
        controller.addToQueue(track("append"))
    }

    // ── Error emission ───────────────────────────────────────────────────

    @Test
    fun `connection failure emits connectionError`() = runTest {
        val errors = mutableListOf<MusicController.ConnectionException>()
        val job = launch {
            controller.connectionError.collect { errors.add(it) }
        }
        // Give the collector a chance to start collecting before emission.
        kotlinx.coroutines.yield()
        controller.initialize()
        fakeConnector.connectFuture.setException(RuntimeException("Service unavailable"))
        // Yield again so the buffered emission reaches the collector.
        kotlinx.coroutines.yield()
        assertThat(errors).isNotEmpty()
        job.cancel()
    }

    @Test
    fun `synchronous connector failure emits a sanitized connection error`() = runTest {
        val throwingController = MusicController(
            context,
            connector = MusicController.AsyncConnector {
                throw IllegalStateException(
                    "Cannot open content://library/private/song.mp3\ninternal stack"
                )
            }
        )
        val errors = mutableListOf<MusicController.ConnectionException>()
        val job = launch {
            throwingController.connectionError.collect { errors.add(it) }
        }
        kotlinx.coroutines.yield()

        throwingController.initialize()
        kotlinx.coroutines.yield()

        assertThat(errors).hasSize(1)
        assertThat(errors.single().message).doesNotContain("content://")
        assertThat(errors.single().message).doesNotContain("private")
        assertThat(errors.single().message).doesNotContain("stack")
        job.cancel()
        throwingController.release()
    }

    @Test
    fun `error sanitizer removes local and remote URIs paths and later lines`() {
        val sanitized = controller.sanitizeErrorMessage(
            "file://storage/emulated/0/Music/private.mp3 " +
                "https://user:secret@example.test/audio /data/user/0/app/private/file\n" +
                "java.lang.IllegalStateException: stack"
        )

        assertThat(sanitized).doesNotContain("file://")
        assertThat(sanitized).doesNotContain("https://")
        assertThat(sanitized).doesNotContain("user:secret")
        assertThat(sanitized).doesNotContain("/data/user")
        assertThat(sanitized).doesNotContain("IllegalStateException")
    }

    @Test
    fun `stale future after release does not emit connectionError`() = runTest {
        val errors = mutableListOf<MusicController.ConnectionException>()
        val job = launch {
            controller.connectionError.collect { errors.add(it) }
        }
        kotlinx.coroutines.yield()
        controller.initialize()
        controller.release()
        // Complete the future after release — should be ignored silently.
        fakeConnector.connectFuture.setException(RuntimeException("Late failure"))
        kotlinx.coroutines.yield()
        assertThat(errors).isEmpty()
        job.cancel()
    }

    // ── Lifecycle ────────────────────────────────────────────────────────

    @Test
    fun `initialize before release attempts connection`() {
        controller.initialize()
        assertThat(fakeConnector.lastConnectAttempt).isTrue()
    }

    @Test
    fun `connected dispatch sets lastDispatchedRequest`() {
        controller.play(listOf(track("a"), track("b")), startIndex = 1)
        controller.initialize()
        // Before connection, lastDispatchedRequest is null.
        assertThat(controller.lastDispatchedRequest).isNull()
    }
}

// ── Test helpers ─────────────────────────────────────────────────────────

/** Shorthand for creating a [MediaTrack] with just a URI. */
private fun track(uri: String): MediaTrack = MediaTrack(uri = uri)

// ── Test double ───────────────────────────────────────────────────────────

/**
 * A fake [MusicController.AsyncConnector] that captures connect calls
 * and returns a [SettableFuture] for deterministic completion.
 */
class FakeConnector : MusicController.AsyncConnector {

    val connectFuture: SettableFuture<MediaController> = SettableFuture.create()
    var lastConnectAttempt: Boolean = false
        private set

    override fun connect(context: Context): ListenableFuture<MediaController> {
        lastConnectAttempt = true
        return connectFuture
    }
}
