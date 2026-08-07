package com.luno.mobile.playback

import android.content.Context
import androidx.room.Room
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.work.testing.WorkManagerTestInitHelper
import com.google.common.truth.Truth.assertThat
import com.luno.mobile.data.db.AppDatabase
import com.luno.mobile.data.db.entity.DownloadState
import com.luno.mobile.data.db.entity.Playlist
import com.luno.mobile.data.discovery.LastfmTrack
import com.luno.mobile.data.repository.DownloadRepository
import com.luno.mobile.data.repository.PlaylistRepository
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

@RunWith(AndroidJUnit4::class)
@Config(manifest = Config.NONE, sdk = [34])
class RecommendationPreviewManagerTest {

    private lateinit var context: Context
    private lateinit var database: AppDatabase
    private lateinit var downloadRepository: DownloadRepository
    private lateinit var appScope: CoroutineScope

    @Before
    fun setUp() {
        context = ApplicationProvider.getApplicationContext()
        WorkManagerTestInitHelper.initializeTestWorkManager(context)
        database = Room.inMemoryDatabaseBuilder(context, AppDatabase::class.java).build()
        downloadRepository = DownloadRepository(
            downloadJobDao = database.downloadJobDao(),
            context = context,
            playlistDao = database.playlistDao(),
            trackDao = database.trackDao(),
            database = database
        )
        appScope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
    }

    @After
    fun tearDown() {
        appScope.cancel()
        database.close()
        File(context.cacheDir, DownloadRepository.PREVIEW_DOWNLOAD_DIR).deleteRecursively()
    }

    @Test
    fun recommendationKey_isStableAcrossCaseAndOuterWhitespace() {
        val first = LastfmTrack("  Artist Name ", " Song Title ", 0.9)
        val second = LastfmTrack("artist name", "song title", 0.8)

        assertThat(RecommendationPreviewManager.recommendationKey(first))
            .isEqualTo(RecommendationPreviewManager.recommendationKey(second))
    }

    @Test
    fun recommendationKey_collapsesPunctuationAccentsAndUnicodeSpacing() {
        val first = LastfmTrack("Taco", "Puttin' on the Ritz", 0.9)
        val duplicate = LastfmTrack("  tacó ", "Puttin\u00a0on the Ritz", 0.8)

        assertThat(RecommendationPreviewManager.recommendationKey(first))
            .isEqualTo(RecommendationPreviewManager.recommendationKey(duplicate))
    }

    @Test
    fun extensionForMimeType_mapsPlayableAudioFormats() {
        assertThat(RecommendationPreviewManager.extensionForMimeType("audio/mp4; codecs=mp4a"))
            .isEqualTo("m4a")
        assertThat(RecommendationPreviewManager.extensionForMimeType("audio/webm; codecs=opus"))
            .isEqualTo("opus")
        assertThat(RecommendationPreviewManager.extensionForMimeType("audio/mpeg"))
            .isEqualTo("mp3")
    }

    @Test
    fun prefetchThreshold_startsAtExactlySeventyFivePercent() {
        assertThat(
            RecommendationPreviewManager.hasReachedPrefetchThreshold(
                progressMs = 749,
                durationMs = 1_000
            )
        ).isFalse()
        assertThat(
            RecommendationPreviewManager.hasReachedPrefetchThreshold(
                progressMs = 750,
                durationMs = 1_000
            )
        ).isTrue()
        assertThat(
            RecommendationPreviewManager.hasReachedPrefetchThreshold(
                progressMs = 1_000,
                durationMs = 0
            )
        ).isFalse()
    }

    @Test
    fun nextFivePreviews_prepareImmediatelyBeforeTheRollingWindowGate() {
        assertThat(RecommendationPreviewManager.shouldWaitForNextPreview(0)).isFalse()
        assertThat(RecommendationPreviewManager.shouldWaitForNextPreview(1)).isFalse()
        assertThat(
            RecommendationPreviewManager.shouldWaitForNextPreview(
                RecommendationPreviewManager.IMMEDIATE_PREVIEW_AHEAD_COUNT
            )
        ).isFalse()
        assertThat(
            RecommendationPreviewManager.shouldWaitForNextPreview(
                RecommendationPreviewManager.IMMEDIATE_PREVIEW_AHEAD_COUNT + 1
            )
        ).isTrue()
    }

    @Test
    fun discoverHandoff_defersOnlyForTheCurrentNormalSeed() {
        val normal = MediaTrack(uri = "seed", isTransient = false)
        val transient = normal.copy(isTransient = true)

        assertThat(
            RecommendationPreviewManager.shouldDeferDiscoverHandoff(
                requested = true,
                currentTrack = normal,
                seedUri = "seed"
            )
        ).isTrue()
        assertThat(
            RecommendationPreviewManager.shouldDeferDiscoverHandoff(
                requested = true,
                currentTrack = transient,
                seedUri = "seed"
            )
        ).isFalse()
        assertThat(
            RecommendationPreviewManager.shouldDeferDiscoverHandoff(
                requested = true,
                currentTrack = normal,
                seedUri = "other"
            )
        ).isFalse()
    }

    @Test
    fun nextDuringDiscoverHandoff_isInterceptedBeforeTheFirstPreviewStarts() {
        val normal = MediaTrack(uri = "seed")

        assertThat(
            RecommendationPreviewManager.isDiscoverHandoffPending(
                discoverMode = true,
                currentTrack = normal,
                deferredInitialPlayback = true,
                initialNormalTrackUri = "seed",
                discoverRequestPending = false,
                discoverStartedUri = null
            )
        ).isTrue()
        assertThat(
            RecommendationPreviewManager.isDiscoverHandoffPending(
                discoverMode = true,
                currentTrack = normal,
                deferredInitialPlayback = false,
                initialNormalTrackUri = null,
                discoverRequestPending = true,
                discoverStartedUri = "seed"
            )
        ).isTrue()
        assertThat(
            RecommendationPreviewManager.isDiscoverHandoffPending(
                discoverMode = true,
                currentTrack = normal.copy(isTransient = true),
                deferredInitialPlayback = true,
                initialNormalTrackUri = "seed",
                discoverRequestPending = false,
                discoverStartedUri = null
            )
        ).isFalse()
    }

    @Test
    fun recommendationWindow_keepsFiveItemsAheadOfCurrentRecommendation() {
        assertThat(
            RecommendationPreviewManager.minimumCurrentIndexForRecommendation(6)
        ).isEqualTo(1)
        assertThat(
            RecommendationPreviewManager.minimumCurrentIndexForRecommendation(10)
        ).isEqualTo(5)
        assertThat(
            RecommendationPreviewManager.minimumCurrentIndexForRecommendation(3)
        ).isEqualTo(0)
    }

    @Test
    fun emptyPlaybackContext_afterObservedPreview_keepsPreparationAlive() {
        assertThat(
            RecommendationPreviewManager.shouldContinueAfterPlaybackGap(
                observedPreviousTrack = true,
                currentTrack = null
            )
        ).isTrue()
        assertThat(
            RecommendationPreviewManager.shouldContinueAfterPlaybackGap(
                observedPreviousTrack = false,
                currentTrack = null
            )
        ).isFalse()
        assertThat(
            RecommendationPreviewManager.shouldContinueAfterPlaybackGap(
                observedPreviousTrack = true,
                currentTrack = MediaTrack(uri = "preview")
            )
        ).isFalse()
    }

    @Test
    fun discoverMode_toggleIsProcessState() {
        val manager = RecommendationPreviewManager(
            context = context,
            appScope = appScope,
            downloadRepository = downloadRepository,
            playlistRepository = PlaylistRepository(
                database.playlistDao(),
                database.trackDao()
            )
        )

        manager.setDiscoverMode(true)
        assertThat(manager.state.value.discoverMode).isTrue()

        manager.setDiscoverMode(false)
        assertThat(manager.state.value.discoverMode).isFalse()
    }

    @Test
    fun continuationAfterCurrent_keepsOnlyLaterNonPreviewQueueItems() {
        val queue = listOf(
            MediaTrack(uri = "current"),
            MediaTrack(uri = "next"),
            MediaTrack(uri = "temporary", isTransient = true),
            MediaTrack(uri = "later")
        )

        assertThat(
            RecommendationPreviewManager.continuationAfterCurrent(queue, "current")
                ?.map { it.uri }
        ).containsExactly("next", "later").inOrder()
    }

    @Test
    fun unpreparedSave_isQueuedOnceAndBecomesPermanentOnlyAfterCompletion() = runBlocking {
        val playlistId = database.playlistDao().insertPlaylist(Playlist(name = "Saved previews"))
        val recommendation = LastfmTrack("Artist", "Song", 0.9)
        var previewResolverCalls = 0
        var durableResolverCalls = 0
        val manager = RecommendationPreviewManager(
            context = context,
            appScope = appScope,
            downloadRepository = downloadRepository,
            playlistRepository = PlaylistRepository(
                database.playlistDao(),
                database.trackDao()
            ),
            resolver = {
                previewResolverCalls++
                Result.failure(AssertionError("Preview resolver must not handle durable saves"))
            },
            durableResolver = {
                durableResolverCalls++
                Result.success(
                    ResolvedRecommendation(
                        sourceUrl = "https://example.com/audio.m4a",
                        thumbnailUrl = "https://example.com/art.jpg",
                        mimeType = "audio/mp4"
                    )
                )
            }
        )
        val key = RecommendationPreviewManager.recommendationKey(recommendation)

        val firstSave = manager.saveToPlaylist(recommendation, playlistId, null)
        val duplicateSave = manager.saveToPlaylist(recommendation, playlistId, null)

        assertThat(firstSave.getOrThrow()).isInstanceOf(RecommendationSaveOutcome.Queued::class.java)
        assertThat(duplicateSave.isFailure).isTrue()
        assertThat(previewResolverCalls).isEqualTo(0)
        assertThat(durableResolverCalls).isEqualTo(1)
        assertThat(manager.state.value.queuedKeys).containsExactly(key)
        assertThat(manager.state.value.permanentKeys).doesNotContain(key)

        val jobId = (firstSave.getOrThrow() as RecommendationSaveOutcome.Queued).jobId
        database.downloadJobDao().markCompleted(
            jobId,
            DownloadState.COMPLETED,
            "file:///durable/song.m4a",
            System.currentTimeMillis()
        )
        withTimeout(5_000) {
            manager.state.first { key in it.permanentKeys }
        }

        assertThat(manager.state.value.queuedKeys).doesNotContain(key)
        assertThat(manager.state.value.saveFailures).doesNotContainKey(key)
    }

    @Test
    fun previewResolveTimeout_clearsSpinnerAndShowsRetryableFailure() = runBlocking {
        val recommendation = LastfmTrack("Slow Artist", "Slow Song", 0.9)
        val controller = MusicController(context)
        val manager = RecommendationPreviewManager(
            context = context,
            appScope = appScope,
            downloadRepository = downloadRepository,
            playlistRepository = PlaylistRepository(
                database.playlistDao(),
                database.trackDao()
            ),
            resolver = {
                delay(1_000)
                Result.success(
                    ResolvedRecommendation(
                        sourceUrl = "https://example.com/audio.m4a",
                        thumbnailUrl = "",
                        mimeType = "audio/mp4"
                    )
                )
            },
            resolveTimeoutMs = 25
        )
        manager.attach(controller)

        manager.startPreview(
            seedUri = "file:///seed.m4a",
            recommendations = listOf(recommendation),
            startIndex = 0,
            artworkByKey = emptyMap()
        )

        val key = RecommendationPreviewManager.recommendationKey(recommendation)
        val failedState = withTimeout(5_000) {
            manager.state.first { key in it.failures }
        }
        assertThat(failedState.resolvingKeys).doesNotContain(key)
        assertThat(failedState.preparingKeys).doesNotContain(key)
        assertThat(failedState.failures.getValue(key)).contains("timed out")
        withTimeout(5_000) { manager.state.first { !it.active } }
        controller.release()
        Unit
    }

    @Test
    fun previewDownloadTimeout_clearsSpinnerAndShowsRetryableFailure() = runBlocking {
        val recommendation = LastfmTrack("Slow Artist", "Slow Download", 0.9)
        val controller = MusicController(context)
        val manager = RecommendationPreviewManager(
            context = context,
            appScope = appScope,
            downloadRepository = downloadRepository,
            playlistRepository = PlaylistRepository(
                database.playlistDao(),
                database.trackDao()
            ),
            resolver = {
                Result.success(
                    ResolvedRecommendation(
                        sourceUrl = "https://example.com/audio.m4a",
                        thumbnailUrl = "",
                        mimeType = "audio/mp4"
                    )
                )
            },
            previewDownloader = { _, _, _, _ ->
                delay(1_000)
                Result.failure(AssertionError("Timed-out download must not complete"))
            },
            downloadTimeoutMs = 25
        )
        manager.attach(controller)

        manager.startPreview(
            seedUri = "file:///seed.m4a",
            recommendations = listOf(recommendation),
            startIndex = 0,
            artworkByKey = emptyMap()
        )

        val key = RecommendationPreviewManager.recommendationKey(recommendation)
        val failedState = withTimeout(5_000) {
            manager.state.first { key in it.failures }
        }
        assertThat(failedState.resolvingKeys).doesNotContain(key)
        assertThat(failedState.preparingKeys).doesNotContain(key)
        assertThat(failedState.failures.getValue(key)).contains("download timed out")
        withTimeout(5_000) { manager.state.first { !it.active } }
        controller.release()
        Unit
    }
}
