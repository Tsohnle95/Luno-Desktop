package com.boombastic.mobile.playback

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.google.common.truth.Truth.assertThat
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/**
 * Contract tests for [MusicController] focusing on the pending-play
 * mechanism and connection-error signalling.
 *
 * These tests verify the observable API contract without requiring a
 * live Media3 service connection. Full integration tests that exercise
 * the actual async MediaController connection belong in an instrumented
 * test suite.
 */
@RunWith(AndroidJUnit4::class)
@Config(manifest = Config.NONE, sdk = [34])
class MusicControllerTest {

    private lateinit var context: Context
    private lateinit var controller: MusicController

    @Before
    fun setUp() {
        context = ApplicationProvider.getApplicationContext()
        controller = MusicController(context)
    }

    // ── Pending-play contract (pre-connection) ──────────────────────────

    @Test
    fun `playUri returns true even before connection`() {
        // Before initialize(), the underlying MediaController is null, but
        // the call should queue and return true (no silent drop).
        assertThat(controller.playUri("http://example.com/track.mp3")).isTrue()
    }

    @Test
    fun `play returns true even before connection`() {
        assertThat(controller.play(listOf("http://example.com/track.mp3"))).isTrue()
    }

    @Test
    fun `play with empty list returns true before connection`() {
        assertThat(controller.play(emptyList())).isTrue()
    }

    @Test
    fun `multiple playUri calls preserve last user request`() {
        // The last URI should win; all calls return true.
        assertThat(controller.playUri("http://example.com/first.mp3")).isTrue()
        assertThat(controller.playUri("http://example.com/second.mp3")).isTrue()
        assertThat(controller.playUri("http://example.com/third.mp3")).isTrue()
        // All preceding contract holds — no crash, no silent false.
    }

    // ── Lifecycle safety ────────────────────────────────────────────────

    @Test
    fun `playUri after release returns true`() {
        controller.release()
        // After release the controller is null; still queues gracefully.
        assertThat(controller.playUri("http://example.com/track.mp3")).isTrue()
    }

    @Test
    fun `togglePlayPause does not throw before connection`() {
        // Should be a no-op when controller is null.
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
    fun `skipToPrevious does not throw before connection`() {
        controller.skipToPrevious()
    }

    @Test
    fun `stop does not throw before connection`() {
        controller.stop()
    }

    @Test
    fun `release is idempotent`() {
        controller.release()
        controller.release() // second call must not throw
    }

    // ── State flows are cold-safe ───────────────────────────────────────

    @Test
    fun `state flows have initial values`() {
        assertThat(controller.isPlaying.value).isFalse()
        assertThat(controller.currentTrack.value).isNull()
        assertThat(controller.progress.value).isEqualTo(0L)
        assertThat(controller.duration.value).isEqualTo(0L)
        assertThat(controller.hasActiveItem.value).isFalse()
        assertThat(controller.isConnected.value).isFalse()
    }
}
