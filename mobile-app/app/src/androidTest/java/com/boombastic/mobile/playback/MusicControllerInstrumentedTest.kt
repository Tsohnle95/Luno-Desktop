package com.boombastic.mobile.playback

import android.app.Notification
import android.app.NotificationManager
import android.content.ComponentName
import android.content.Context
import android.net.Uri
import android.os.Build
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.session.MediaController
import androidx.media3.session.MediaSession
import androidx.media3.session.SessionToken
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.boombastic.mobile.MainActivity
import com.google.common.truth.Truth.assertThat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import org.junit.After
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.ByteArrayOutputStream
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * Instrumented smoke tests for [MusicController] that verify the real
 * [MediaController] connection to [MusicService], playback dispatch,
 * error propagation, notification posting, and lifecycle survival.
 *
 * These tests require an emulator or device with API 34+ and are
 * excluded from the `testDebugUnitTest` JVM suite.
 */
@RunWith(AndroidJUnit4::class)
class MusicControllerInstrumentedTest {

    private lateinit var context: Context
    private lateinit var controller: MusicController
    private val testScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    @Before
    fun setUp() {
        context = ApplicationProvider.getApplicationContext()
        controller = MusicController(context)
    }

    @After
    fun tearDown() {
        controller.release()
        testScope.cancel()
    }

    @Test
    fun connectAndObserveState() {
        val connectedLatch = CountDownLatch(1)
        controller.initialize { connectedLatch.countDown() }

        val connected = connectedLatch.await(5, TimeUnit.SECONDS)
        assertThat(connected).isTrue()
        assertThat(controller.isConnected.value).isTrue()
        assertThat(controller.isPlaying.value).isFalse()
    }

    @Test
    fun playUriConnectsAndUpdatesState() {
        val connectedLatch = CountDownLatch(1)
        controller.initialize { connectedLatch.countDown() }
        assertThat(connectedLatch.await(5, TimeUnit.SECONDS)).isTrue()

        controller.playUri("http://localhost/test.mp3")
        assertThat(controller.isConnected.value).isTrue()
    }

    @Test
    fun connectionErrorEmitsOnBadService() {
        val badConnector = MusicController.AsyncConnector { ctx ->
            val token = SessionToken(ctx, ComponentName("com.boombastic.nonexistent", "NoService"))
            MediaController.Builder(ctx, token).buildAsync()
        }
        val badController = MusicController(context, connector = badConnector)

        val errorLatch = CountDownLatch(1)
        val job = testScope.launch {
            badController.connectionError.collect { errorLatch.countDown() }
        }

        badController.initialize()
        val receivedError = errorLatch.await(5, TimeUnit.SECONDS)
        assertThat(receivedError).isTrue()
        job.cancel()
        badController.release()
    }

    // ── Required coverage: playback, notification, activity recreation ─────

    @Test
    fun playbackActuallyStarts() {
        // Local ExoPlayer + MediaSession test double for deterministic playback.
        val player = ExoPlayer.Builder(context).build()
        val session = MediaSession.Builder(context, player).build()
        val localConnector = MusicController.AsyncConnector { ctx ->
            MediaController.Builder(ctx, session.token).buildAsync()
        }
        val testController = MusicController(context, connector = localConnector)

        try {
            val connectedLatch = CountDownLatch(1)
            testController.initialize { connectedLatch.countDown() }
            assertThat(connectedLatch.await(5, TimeUnit.SECONDS)).isTrue()
            assertThat(testController.isConnected.value).isTrue()

            // Create a valid WAV file in the app cache directory.
            val audioFile = createMinimalWavFile(context.cacheDir, "playback_test.wav")
            val uri = Uri.fromFile(audioFile)

            // Subscribe to isPlaying BEFORE playUri so the transition is never
            // missed.  Use a collectorReady latch to guarantee ordering.
            val collectorReady = CountDownLatch(1)
            val playingLatch = CountDownLatch(1)
            val job = testScope.launch {
                testController.isPlaying.collect { playing ->
                    collectorReady.countDown()
                    if (playing) playingLatch.countDown()
                }
            }
            assertThat(collectorReady.await(5, TimeUnit.SECONDS)).isTrue()

            // Now play — the collector is already registered.
            testController.playUri(uri.toString())

            assertThat(playingLatch.await(15, TimeUnit.SECONDS)).isTrue()
            assertThat(testController.isPlaying.value).isTrue()
            job.cancel()
        } finally {
            testController.release()
            session.release()
            player.release()
        }
    }

    @Test
    fun notificationBehavior() {
        // Grant POST_NOTIFICATIONS on API 33+ so the notification is visible.
        if (Build.VERSION.SDK_INT >= 33) {
            try {
                val instrumentation = InstrumentationRegistry.getInstrumentation()
                instrumentation.uiAutomation.executeShellCommand(
                    "pm grant ${context.packageName} android.permission.POST_NOTIFICATIONS"
                )
            } catch (_: Exception) {
                // Non-fatal — media notifications are exempt from the runtime
                // permission on compliant devices.
            }
        }

        val connectedLatch = CountDownLatch(1)
        controller.initialize { connectedLatch.countDown() }
        assertThat(connectedLatch.await(5, TimeUnit.SECONDS)).isTrue()

        val audioFile = createMinimalWavFile(context.cacheDir, "notification_test.wav")
        val uri = Uri.fromFile(audioFile)

        // Subscribe to isPlaying before playUri to capture the transition.
        val collectorReady = CountDownLatch(1)
        val playingLatch = CountDownLatch(1)
        val job = testScope.launch {
            controller.isPlaying.collect { playing ->
                collectorReady.countDown()
                if (playing) playingLatch.countDown()
            }
        }
        assertThat(collectorReady.await(5, TimeUnit.SECONDS)).isTrue()

        controller.playUri(uri.toString())

        // Wait for playback to actually start, then inspect notifications.
        assertThat(playingLatch.await(15, TimeUnit.SECONDS)).isTrue()
        job.cancel()

        val notificationManager =
            context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        val active = notificationManager.activeNotifications
        val mediaNotification = active.firstOrNull { n ->
            n.notification.category == Notification.CATEGORY_TRANSPORT ||
                n.notification.category == Notification.CATEGORY_SERVICE
        }
        assertThat(mediaNotification).isNotNull()
    }

    @Test
    fun activityRecreation() {
        // Use CountDownLatch outside onActivity so the main thread is never
        // blocked by polling.
        val scenario = ActivityScenario.launch(MainActivity::class.java)

        // Wait for initial connection.
        val connectedLatch1 = CountDownLatch(1)
        scenario.onActivity { activity ->
            testScope.launch {
                activity.musicController.isConnected.first { it }
                connectedLatch1.countDown()
            }
        }
        assertThat(connectedLatch1.await(8, TimeUnit.SECONDS)).isTrue()

        // Recreate the activity (onDestroy → onCreate).
        scenario.recreate()

        // Wait for the new controller to connect.
        val connectedLatch2 = CountDownLatch(1)
        scenario.onActivity { activity ->
            testScope.launch {
                activity.musicController.isConnected.first { it }
                connectedLatch2.countDown()
            }
        }
        assertThat(connectedLatch2.await(8, TimeUnit.SECONDS)).isTrue()

        scenario.close()
    }

    @Test
    fun preConnectionQueueIsInstalledOnceWithRequestedStartIndex() {
        val player = ExoPlayer.Builder(context).build()
        val session = MediaSession.Builder(context, player).build()
        val localConnector = MusicController.AsyncConnector { ctx ->
            MediaController.Builder(ctx, session.token).buildAsync()
        }
        val testController = MusicController(context, connector = localConnector)

        try {
            val tracks = (1..3).map { index ->
                val file = createMinimalWavFile(context.cacheDir, "queue_$index.wav")
                MediaTrack(
                    uri = Uri.fromFile(file).toString(),
                    title = "Track $index",
                    durationMs = 2_000L
                )
            }
            testController.play(tracks, startIndex = 1)

            val connectedLatch = CountDownLatch(1)
            testController.initialize { connectedLatch.countDown() }
            assertThat(connectedLatch.await(5, TimeUnit.SECONDS)).isTrue()

            var installed = false
            repeat(20) {
                if (player.mediaItemCount == 3 && player.currentMediaItemIndex == 1) {
                    installed = true
                    return@repeat
                }
                Thread.sleep(100)
            }

            assertThat(installed).isTrue()
            assertThat(player.mediaItemCount).isEqualTo(3)
            assertThat((0 until player.mediaItemCount).map {
                player.getMediaItemAt(it).mediaId
            }).containsExactlyElementsIn(tracks.map { it.uri }).inOrder()
            assertThat(player.currentMediaItemIndex).isEqualTo(1)
        } finally {
            testController.release()
            session.release()
            player.release()
        }
    }

    // ── Test helpers ───────────────────────────────────────────────────────

    companion object {
        /**
         * Creates a minimal but valid 16-bit mono 44.1 kHz WAV file with
         * silent samples.  ExoPlayer can prepare and play this file,
         * transitioning to [isPlaying][androidx.media3.common.Player.isPlaying]
         * immediately without audio output.
         */
        fun createMinimalWavFile(dir: File, name: String): File {
            val file = File(dir, name)
            val sampleRate = 44100
            val bitsPerSample = 16
            val numChannels = 1
            val numSamples = sampleRate * 2 // 2 seconds of silence
            val byteRate = sampleRate * numChannels * (bitsPerSample / 8)
            val blockAlign = numChannels * (bitsPerSample / 8)
            val dataSize = numSamples * numChannels * (bitsPerSample / 8)
            val fileSize = 36 + dataSize

            val bos = ByteArrayOutputStream()
            bos.write("RIFF".toByteArray())
            bos.write(intToBytesLE(fileSize, 4))
            bos.write("WAVE".toByteArray())
            bos.write("fmt ".toByteArray())
            bos.write(intToBytesLE(16, 4))   // chunk size
            bos.write(intToBytesLE(1, 2))    // PCM
            bos.write(intToBytesLE(numChannels, 2))
            bos.write(intToBytesLE(sampleRate, 4))
            bos.write(intToBytesLE(byteRate, 4))
            bos.write(intToBytesLE(blockAlign, 2))
            bos.write(intToBytesLE(bitsPerSample, 2))
            bos.write("data".toByteArray())
            bos.write(intToBytesLE(dataSize, 4))
            repeat(dataSize) { bos.write(0) }

            file.writeBytes(bos.toByteArray())
            return file
        }

        private fun intToBytesLE(value: Int, numBytes: Int): ByteArray {
            val bytes = ByteArray(numBytes)
            for (i in 0 until numBytes) {
                bytes[i] = (value shr (i * 8)).toByte()
            }
            return bytes
        }
    }
}
