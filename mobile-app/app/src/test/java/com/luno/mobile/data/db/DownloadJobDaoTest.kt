package com.luno.mobile.data.db

import com.luno.mobile.data.db.entity.DownloadJob
import com.luno.mobile.data.db.entity.DownloadState
import com.google.common.truth.Truth.assertThat
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Test

class DownloadJobDaoTest : AppDatabaseTest() {

    private val jobDao get() = database.downloadJobDao()

    @Test
    fun insertAndGetDownload() = runBlocking {
        val job = DownloadJob(
            sourceUrl = "https://example.com/audio.mp3",
            title = "Test Song",
            artist = "Test Artist"
        )
        val id = jobDao.insertDownload(job)

        val retrieved = jobDao.getDownload(id)
        assertThat(retrieved).isNotNull()
        assertThat(retrieved!!.sourceUrl).isEqualTo("https://example.com/audio.mp3")
        assertThat(retrieved.title).isEqualTo("Test Song")
        assertThat(retrieved.artist).isEqualTo("Test Artist")
        assertThat(retrieved.state).isEqualTo(DownloadState.QUEUED)
    }

    @Test
    fun getAllDownloads_returnsAllInOrder() = runBlocking {
        val job1 = DownloadJob(sourceUrl = "https://example.com/1.mp3", title = "A", addedAt = 1000)
        val job2 = DownloadJob(sourceUrl = "https://example.com/2.mp3", title = "B", addedAt = 2000)
        jobDao.insertDownload(job1)
        jobDao.insertDownload(job2)

        val all = jobDao.getAllDownloads().first()
        assertThat(all).hasSize(2)
        assertThat(all[0].title).isEqualTo("B")
        assertThat(all[1].title).isEqualTo("A")
    }

    @Test
    fun updateProgress_updatesState() = runBlocking {
        val id = jobDao.insertDownload(
            DownloadJob(sourceUrl = "https://example.com/audio.mp3", title = "T")
        )

        jobDao.updateProgress(id, DownloadState.DOWNLOADING, 50)

        val updated = jobDao.getDownload(id)
        assertThat(updated!!.state).isEqualTo(DownloadState.DOWNLOADING)
        assertThat(updated.progress).isEqualTo(50)
    }

    @Test
    fun markCompleted_setsStateAndUri() = runBlocking {
        val id = jobDao.insertDownload(
            DownloadJob(sourceUrl = "https://example.com/audio.mp3", title = "T")
        )
        val now = System.currentTimeMillis()

        jobDao.markCompleted(id, DownloadState.COMPLETED, "file:///test.mp3", now)

        val updated = jobDao.getDownload(id)
        assertThat(updated!!.state).isEqualTo(DownloadState.COMPLETED)
        assertThat(updated.localUri).isEqualTo("file:///test.mp3")
        assertThat(updated.completedAt).isEqualTo(now)
        assertThat(updated.progress).isEqualTo(100)
    }

    @Test
    fun markFailed_setsStateAndMessage() = runBlocking {
        val id = jobDao.insertDownload(
            DownloadJob(sourceUrl = "https://example.com/audio.mp3", title = "T")
        )

        jobDao.markFailed(id, DownloadState.FAILED, "Network error")

        val updated = jobDao.getDownload(id)
        assertThat(updated!!.state).isEqualTo(DownloadState.FAILED)
        assertThat(updated.errorMessage).isEqualTo("Network error")
    }

    @Test
    fun getDownloadsByState_filtersCorrectly() = runBlocking {
        jobDao.insertDownload(
            DownloadJob(sourceUrl = "https://example.com/1.mp3", title = "A", state = DownloadState.COMPLETED)
        )
        jobDao.insertDownload(
            DownloadJob(sourceUrl = "https://example.com/2.mp3", title = "B", state = DownloadState.FAILED)
        )

        val completed = jobDao.getDownloadsByState(DownloadState.COMPLETED).first()
        assertThat(completed).hasSize(1)
        assertThat(completed[0].title).isEqualTo("A")

        val failed = jobDao.getDownloadsByState(DownloadState.FAILED).first()
        assertThat(failed).hasSize(1)
        assertThat(failed[0].title).isEqualTo("B")
    }

    @Test
    fun deleteDownload_removesFromDatabase() = runBlocking {
        val id = jobDao.insertDownload(
            DownloadJob(sourceUrl = "https://example.com/audio.mp3", title = "T")
        )
        assertThat(jobDao.getDownload(id)).isNotNull()

        jobDao.deleteDownload(id)
        assertThat(jobDao.getDownload(id)).isNull()
    }

    @Test
    fun deleteDownloadsByState_removesOnlyMatching() = runBlocking {
        jobDao.insertDownload(
            DownloadJob(sourceUrl = "https://example.com/1.mp3", title = "A", state = DownloadState.COMPLETED)
        )
        jobDao.insertDownload(
            DownloadJob(sourceUrl = "https://example.com/2.mp3", title = "B", state = DownloadState.QUEUED)
        )

        jobDao.deleteDownloadsByState(DownloadState.COMPLETED)

        val all = jobDao.getAllDownloads().first()
        assertThat(all).hasSize(1)
        assertThat(all[0].state).isEqualTo(DownloadState.QUEUED)
    }

    @Test
    fun countByState_returnsCorrectCount() = runBlocking {
        assertThat(jobDao.countByState(DownloadState.QUEUED)).isEqualTo(0)

        jobDao.insertDownload(
            DownloadJob(sourceUrl = "https://example.com/1.mp3", title = "A", state = DownloadState.QUEUED)
        )
        assertThat(jobDao.countByState(DownloadState.QUEUED)).isEqualTo(1)

        jobDao.insertDownload(
            DownloadJob(sourceUrl = "https://example.com/2.mp3", title = "B", state = DownloadState.QUEUED)
        )
        assertThat(jobDao.countByState(DownloadState.QUEUED)).isEqualTo(2)
    }

    @Test
    fun getActiveDownloadsOnce_returnsOnlyActiveJobs() = runBlocking {
        val active1 = jobDao.insertDownload(
            DownloadJob(sourceUrl = "https://example.com/1.mp3", title = "A", state = DownloadState.QUEUED)
        )
        val active2 = jobDao.insertDownload(
            DownloadJob(sourceUrl = "https://example.com/2.mp3", title = "B", state = DownloadState.DOWNLOADING)
        )
        jobDao.insertDownload(
            DownloadJob(sourceUrl = "https://example.com/3.mp3", title = "C", state = DownloadState.COMPLETED)
        )

        val active = jobDao.getActiveDownloadsOnce()
        assertThat(active.map { it.id }).containsExactly(active1, active2).inOrder()
    }

    @Test
    fun getActiveDownloadsForPlaylistOnce_filtersByPlaylist() = runBlocking {
        val p1 = jobDao.insertDownload(
            DownloadJob(
                sourceUrl = "https://example.com/1.mp3",
                title = "P1",
                state = DownloadState.DOWNLOADING,
                playlistId = 7L
            )
        )
        jobDao.insertDownload(
            DownloadJob(
                sourceUrl = "https://example.com/2.mp3",
                title = "P1 done",
                state = DownloadState.COMPLETED,
                playlistId = 7L
            )
        )
        jobDao.insertDownload(
            DownloadJob(
                sourceUrl = "https://example.com/3.mp3",
                title = "Other",
                state = DownloadState.DOWNLOADING,
                playlistId = 8L
            )
        )

        val active = jobDao.getActiveDownloadsForPlaylistOnce(7L)
        assertThat(active.map { it.id }).containsExactly(p1).inOrder()
    }
}