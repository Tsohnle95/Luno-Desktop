package com.boombastic.mobile.data.repository

import android.content.Context
import androidx.room.Room
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.work.WorkManager
import androidx.work.testing.WorkManagerTestInitHelper
import com.boombastic.mobile.data.db.AppDatabase
import com.boombastic.mobile.data.db.entity.DownloadState
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
        repository = DownloadRepository(database.downloadJobDao(), context)
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