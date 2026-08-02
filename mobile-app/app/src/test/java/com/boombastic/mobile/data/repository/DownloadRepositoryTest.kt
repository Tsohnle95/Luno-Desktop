package com.boombastic.mobile.data.repository

import android.content.Context
import androidx.room.Room
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.work.WorkManager
import androidx.work.testing.WorkManagerTestInitHelper
import com.boombastic.mobile.data.db.AppDatabase
import com.boombastic.mobile.data.db.entity.DownloadJob
import com.boombastic.mobile.data.db.entity.DownloadState
import com.boombastic.mobile.data.db.entity.Track
import com.google.common.truth.Truth.assertThat
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

@RunWith(AndroidJUnit4::class)
@Config(manifest = Config.NONE, sdk = [34])
class DownloadRepositoryTest {

    private lateinit var database: AppDatabase
    private lateinit var repository: DownloadRepository
    private lateinit var context: Context

    @Before
    fun setUp() {
        context = ApplicationProvider.getApplicationContext()
        WorkManagerTestInitHelper.initializeTestWorkManager(context)
        database = Room.inMemoryDatabaseBuilder(context, AppDatabase::class.java).build()
        repository = DownloadRepository(
            downloadJobDao = database.downloadJobDao(),
            context = context,
            playlistDao = database.playlistDao()
        )
    }

    @After
    fun tearDown() {
        database.close()
    }

    @Test
    fun enqueueDownload_insertsQueuedJob() = runBlocking {
        val id = repository.enqueueDownload(
            sourceUrl = "https://example.com/audio.mp3",
            title = "Test Song",
            artist = "Test Artist"
        )

        val job = database.downloadJobDao().getDownload(id)
        assertThat(job).isNotNull()
        assertThat(job!!.state).isEqualTo(DownloadState.QUEUED)
        assertThat(job.sourceUrl).isEqualTo("https://example.com/audio.mp3")
        assertThat(job.title).isEqualTo("Test Song")
        assertThat(job.artist).isEqualTo("Test Artist")
        assertThat(job.thumbnailUrl).isEmpty()
        assertThat(job.playlistId).isNotNull()
        assertThat(database.playlistDao().getPlaylist(job.playlistId!!)?.name)
            .isEqualTo(DownloadRepository.UNSORTED_PLAYLIST_NAME)
    }

    @Test
    fun enqueueDownload_persistsThumbnailUrl() = runBlocking {
        val id = repository.enqueueDownload(
            sourceUrl = "https://example.com/audio.m4a",
            title = "Thumb Track",
            thumbnailUrl = "https://i.ytimg.com/vi/abc123/mqdefault.jpg"
        )

        val job = database.downloadJobDao().getDownload(id)
        assertThat(job).isNotNull()
        assertThat(job!!.thumbnailUrl)
            .isEqualTo("https://i.ytimg.com/vi/abc123/mqdefault.jpg")
    }

    @Test
    fun enqueueDownload_persistsPlaylistAssignment() = runBlocking {
        val id = repository.enqueueDownload(
            sourceUrl = "https://example.com/assigned.mp3",
            title = "Assigned Track",
            playlistId = 42L
        )

        assertThat(database.downloadJobDao().getDownload(id)!!.playlistId).isEqualTo(42L)
    }

    @Test
    fun enqueueDownload_reusesUnsortedPlaylist() = runBlocking {
        repository.enqueueDownload("https://example.com/one.mp3", "One")
        repository.enqueueDownload("https://example.com/two.mp3", "Two")

        assertThat(database.playlistDao().getPlaylistByName(DownloadRepository.UNSORTED_PLAYLIST_NAME))
            .isNotNull()
        assertThat(database.playlistDao().getAllPlaylistsWithTracks().first())
            .hasSize(1)
    }

    @Test
    fun repairUnsortedMemberships_assignsLegacyCompletedDownload() = runBlocking {
        val localUri = "file:///data/data/app/files/downloads/legacy.mp3"
        database.downloadJobDao().insertDownload(
            DownloadJob(
                sourceUrl = "https://example.com/legacy.mp3",
                title = "Legacy",
                state = DownloadState.COMPLETED,
                localUri = localUri
            )
        )
        database.trackDao().insertTrack(Track(uri = localUri, title = "Legacy"))

        repository.repairUnsortedMemberships(database.trackDao())

        val unsorted = database.playlistDao()
            .getPlaylistByName(DownloadRepository.UNSORTED_PLAYLIST_NAME)
        assertThat(unsorted).isNotNull()
        assertThat(database.playlistDao().getPlaylistWithTracks(unsorted!!.id)!!.tracks.map { it.uri })
            .containsExactly(localUri)
        Unit
    }

    @Test
    fun getAllDownloads_returnsInsertedJobs() = runBlocking {
        repository.enqueueDownload("https://example.com/1.mp3", "Track A")
        repository.enqueueDownload("https://example.com/2.mp3", "Track B")

        val all = repository.getAllDownloads().first()
        assertThat(all).hasSize(2)
    }

    @Test
    fun getDownloadsByState_filtersCorrectly() = runBlocking {
        val id = repository.enqueueDownload("https://example.com/1.mp3", "Track A")
        database.downloadJobDao().markCompleted(id, DownloadState.COMPLETED, "file:///test.mp3", System.currentTimeMillis())

        val active = repository.getDownloadsByState(DownloadState.QUEUED).first()
        assertThat(active).hasSize(0)

        val completed = repository.getDownloadsByState(DownloadState.COMPLETED).first()
        assertThat(completed).hasSize(1)
    }

    @Test
    fun cancelDownload_marksAsCancelled() = runBlocking {
        val id = repository.enqueueDownload("https://example.com/1.mp3", "Track A")

        repository.cancelDownload(id)

        val job = database.downloadJobDao().getDownload(id)
        assertThat(job!!.state).isEqualTo(DownloadState.CANCELLED)
    }

    @Test
    fun deleteDownload_removesJob() = runBlocking {
        val id = repository.enqueueDownload("https://example.com/1.mp3", "Track A")

        repository.deleteDownload(id)

        val job = database.downloadJobDao().getDownload(id)
        assertThat(job).isNull()
    }

    @Test
    fun getDownload_returnsNullForMissing() = runBlocking {
        val job = repository.getDownload(999L)
        assertThat(job).isNull()
    }
}
