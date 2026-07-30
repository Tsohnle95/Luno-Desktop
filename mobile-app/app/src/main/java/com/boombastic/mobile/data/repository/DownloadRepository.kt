package com.boombastic.mobile.data.repository

import android.content.Context
import androidx.work.BackoffPolicy
import androidx.work.Constraints
import androidx.work.Data
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import com.boombastic.mobile.data.db.dao.DownloadJobDao
import com.boombastic.mobile.data.db.entity.DownloadJob
import com.boombastic.mobile.data.db.entity.DownloadState
import com.boombastic.mobile.playback.DownloadWorker
import com.boombastic.mobile.playback.PlaylistSyncWorker
import kotlinx.coroutines.flow.Flow
import java.util.concurrent.TimeUnit

class DownloadRepository(
    private val downloadJobDao: DownloadJobDao,
    private val context: Context
) {
    fun getAllDownloads(): Flow<List<DownloadJob>> = downloadJobDao.getAllDownloads()

    fun getDownloadsByState(state: DownloadState): Flow<List<DownloadJob>> =
        downloadJobDao.getDownloadsByState(state)

    suspend fun getDownload(id: Long): DownloadJob? = downloadJobDao.getDownload(id)

    suspend fun enqueueDownload(
        sourceUrl: String,
        title: String,
        artist: String = "",
        playlistId: Long? = null
    ): Long {
        val job = DownloadJob(
            sourceUrl = sourceUrl,
            title = title,
            artist = artist,
            state = DownloadState.QUEUED,
            addedAt = System.currentTimeMillis(),
            playlistId = playlistId
        )
        val jobId = downloadJobDao.insertDownload(job)

        val inputData = Data.Builder()
            .putLong(DownloadWorker.KEY_DOWNLOAD_JOB_ID, jobId)
            .build()

        val workRequest = OneTimeWorkRequestBuilder<DownloadWorker>()
            .setInputData(inputData)
            .setConstraints(
                Constraints.Builder()
                    .setRequiredNetworkType(NetworkType.CONNECTED)
                    .build()
            )
            .setBackoffCriteria(
                BackoffPolicy.EXPONENTIAL,
                30,
                TimeUnit.SECONDS
            )
            .addTag("download_$jobId")
            .build()

        val workManagerId = workRequest.id.toString()
        downloadJobDao.updateDownload(
            job.copy(workManagerId = workManagerId)
        )

        WorkManager.getInstance(context)
            .enqueueUniqueWork(
                "download_$jobId",
                ExistingWorkPolicy.REPLACE,
                workRequest
            )

        return jobId
    }

    suspend fun retryDownload(id: Long) {
        val job = downloadJobDao.getDownload(id) ?: return
        downloadJobDao.updateDownload(
            job.copy(
                state = DownloadState.QUEUED,
                progress = 0,
                errorMessage = ""
            )
        )

        val inputData = Data.Builder()
            .putLong(DownloadWorker.KEY_DOWNLOAD_JOB_ID, id)
            .build()

        val workRequest = OneTimeWorkRequestBuilder<DownloadWorker>()
            .setInputData(inputData)
            .setConstraints(
                Constraints.Builder()
                    .setRequiredNetworkType(NetworkType.CONNECTED)
                    .build()
            )
            .setBackoffCriteria(
                BackoffPolicy.EXPONENTIAL,
                30,
                TimeUnit.SECONDS
            )
            .addTag("download_$id")
            .build()

        downloadJobDao.updateDownload(job.copy(workManagerId = workRequest.id.toString()))

        WorkManager.getInstance(context)
            .enqueueUniqueWork(
                "download_$id",
                ExistingWorkPolicy.REPLACE,
                workRequest
            )
    }

    suspend fun cancelDownload(id: Long) {
        val job = downloadJobDao.getDownload(id) ?: return
        if (job.workManagerId.isNotBlank()) {
            WorkManager.getInstance(context).cancelWorkById(
                java.util.UUID.fromString(job.workManagerId)
            )
        }
        downloadJobDao.updateDownload(
            job.copy(state = DownloadState.CANCELLED)
        )
    }

    suspend fun clearCompleted() {
        downloadJobDao.deleteDownloadsByState(DownloadState.COMPLETED)
    }

    suspend fun deleteDownload(id: Long) {
        val job = downloadJobDao.getDownload(id)
        if (job != null && job.workManagerId.isNotBlank()) {
            WorkManager.getInstance(context).cancelWorkById(
                java.util.UUID.fromString(job.workManagerId)
            )
        }
        downloadJobDao.deleteDownload(id)
    }

    suspend fun syncPlaylist(playlistId: Long, playlistName: String, playlistUrl: String) {
        val inputData = Data.Builder()
            .putLong(PlaylistSyncWorker.KEY_PLAYLIST_ID, playlistId)
            .putString(PlaylistSyncWorker.KEY_PLAYLIST_NAME, playlistName)
            .putString(PlaylistSyncWorker.KEY_PLAYLIST_URL, playlistUrl)
            .build()

        val workRequest = OneTimeWorkRequestBuilder<PlaylistSyncWorker>()
            .setInputData(inputData)
            .setConstraints(
                Constraints.Builder()
                    .setRequiredNetworkType(NetworkType.CONNECTED)
                    .build()
            )
            .setBackoffCriteria(
                BackoffPolicy.EXPONENTIAL,
                30,
                TimeUnit.SECONDS
            )
            .addTag("playlist_sync_$playlistId")
            .build()

        WorkManager.getInstance(context)
            .enqueueUniqueWork(
                "playlist_sync_$playlistId",
                ExistingWorkPolicy.REPLACE,
                workRequest
            )
    }
}