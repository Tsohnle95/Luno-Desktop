package com.boombastic.mobile.ui.shell

import android.content.Context
import androidx.room.Room
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.boombastic.mobile.data.db.AppDatabase
import com.boombastic.mobile.data.repository.LibraryRepository
import com.google.common.truth.Truth.assertThat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/**
 * Pins the ArtworkFetchManager state machine: start publishes an
 * immediate Progress state, per-track progress flows through, the sweep
 * resolves to Finished (with the updated count), auto-clears after the
 * display window, and a second start while active is ignored.
 *
 * The injected fetch function replaces the real repository sweep; the
 * repository itself is a real in-memory instance (its sweep never runs).
 */
@RunWith(AndroidJUnit4::class)
@Config(manifest = Config.NONE, sdk = [34])
class ArtworkFetchManagerTest {

    private lateinit var context: Context
    private lateinit var repository: LibraryRepository
    private var fetchInvocations = 0

    @Before
    fun setUp() {
        context = ApplicationProvider.getApplicationContext()
        val database = Room.inMemoryDatabaseBuilder(context, AppDatabase::class.java).build()
        repository = LibraryRepository(
            context = context,
            trackDao = database.trackDao(),
            playlistDao = database.playlistDao(),
            uriPermissionPersister = LibraryRepository.UriPermissionPersister { }
        )
        fetchInvocations = 0
    }

    private fun manager(
        scope: CoroutineScope,
        fetch: suspend (onProgress: (Int, Int, Int) -> Unit) -> Int
    ) = ArtworkFetchManager(
        appScope = scope,
        libraryRepository = repository,
        context = context,
        fetchArtwork = fetch
    )

    @Test
    fun start_publishesProgress_thenFinished_thenAutoClears() = runTest {
        val scope = CoroutineScope(StandardTestDispatcher(testScheduler) + Job())
        val manager = manager(scope) { onProgress ->
            fetchInvocations++
            onProgress(1, 3, 0)
            onProgress(2, 3, 1)
            onProgress(3, 3, 2)
            2
        }

        manager.start()
        // Synchronously published before any coroutine runs.
        assertThat(manager.status.value).isEqualTo(ArtworkFetchStatus.Progress(0, 0, 0))

        runCurrent()
        assertThat(manager.status.value).isEqualTo(ArtworkFetchStatus.Finished(2))
        assertThat(fetchInvocations).isEqualTo(1)

        // Auto-clear after the display window.
        advanceTimeBy(5_000)
        runCurrent()
        assertThat(manager.status.value).isNull()
    }

    @Test
    fun start_whileRunning_isIgnored() = runTest {
        val scope = CoroutineScope(StandardTestDispatcher(testScheduler) + Job())
        val manager = manager(scope) { onProgress ->
            fetchInvocations++
            onProgress(1, 1, 1)
            1
        }

        manager.start()
        manager.start() // second tap while the first sweep is active

        runCurrent()
        assertThat(fetchInvocations).isEqualTo(1)
        assertThat(manager.status.value).isEqualTo(ArtworkFetchStatus.Finished(1))

        advanceUntilIdle()
        assertThat(manager.status.value).isNull()

        // After completion a new start runs again.
        manager.start()
        runCurrent()
        assertThat(fetchInvocations).isEqualTo(2)
    }

    @Test
    fun failedFetch_resolvesToFailedFinishedAndClears() = runTest {
        val scope = CoroutineScope(StandardTestDispatcher(testScheduler) + Job())
        val manager = manager(scope) { throw IllegalStateException("boom") }

        manager.start()
        runCurrent()
        val status = manager.status.value as ArtworkFetchStatus.Finished
        assertThat(status.failed).isTrue()
        assertThat(status.errorMessage).isEqualTo("boom")

        advanceTimeBy(5_000)
        runCurrent()
        assertThat(manager.status.value).isNull()
    }
}
