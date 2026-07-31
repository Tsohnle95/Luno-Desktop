package com.boombastic.mobile.data.repository

import android.content.Context
import android.util.Log
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
    companion object {
        const val TAG = "DownloadRepository"
    }
    fun getAllDownloads(): Flow<List<DownloadJob>> = downloadJobDao.getAllDownloads()

    fun getDownloadsByState(state: DownloadState): Flow<List<DownloadJob>> =
        downloadJobDao.getDownloadsByState(state)

    suspend fun getDownload(id: Long): DownloadJob? = downloadJobDao.getDownload(id)

    suspend fun enqueueDownload(
        sourceUrl: String,
        title: String,
        artist: String = "",
        playlistId: Long? = null,
        thumbnailUrl: String = ""
    ): Long {
        Log.d(TAG, "enqueueDownload: $title by $artist")
        Log.d(TAG, "Source URL (truncated): ${sourceUrl.take(120)}")

        val job = DownloadJob(
            sourceUrl = sourceUrl,
            title = title,
            artist = artist,
            state = DownloadState.QUEUED,
            addedAt = System.currentTimeMillis(),
            playlistId = playlistId,
            thumbnailUrl = thumbnailUrl
        )
        val jobId = downloadJobDao.insertDownload(job)
        Log.d(TAG, "Inserted download job with id=$jobId")

        val inputData = Data.Builder()
            .putLong(DownloadWorker.KEY_DOWNLOAD_JOB_ID, jobId)
            .putString(DownloadWorker.KEY_THUMBNAIL_URL, thumbnailUrl)
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
            .addTag(DownloadWorker.TAG_DOWNLOAD)
            .addTag("download_$jobId")
            .build()

        val workManagerId = workRequest.id.toString()
        downloadJobDao.updateDownload(
            job.copy(id = jobId, workManagerId = workManagerId)
        )

        WorkManager.getInstance(context)
            .enqueueUniqueWork(
                "download_$jobId",
                ExistingWorkPolicy.REPLACE,
                workRequest
            )

        Log.d(TAG, "Enqueued WorkManager work for download job $jobId (workId=$workManagerId)")
        return jobId
    }

    suspend fun retryDownload(id: Long) {
        val job = downloadJobDao.getDownload(id) ?: return
        val updatedJob = job.copy(
            state = DownloadState.QUEUED,
            progress = 0,
            errorMessage = ""
        )
        downloadJobDao.updateDownload(updatedJob)

        val inputData = Data.Builder()
            .putLong(DownloadWorker.KEY_DOWNLOAD_JOB_ID, id)
            .putString(DownloadWorker.KEY_THUMBNAIL_URL, job.thumbnailUrl)
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
            .addTag(DownloadWorker.TAG_DOWNLOAD)
            .addTag("download_$id")
            .build()

        downloadJobDao.updateDownload(updatedJob.copy(workManagerId = workRequest.id.toString()))

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

    /**
     * Halts every active/pending download job AND every running playlist
     * sync (whose worker keeps spawning new downloads).  Cancels all work
     * sharing the common tags and marks active jobs CANCELLED.
     */
    suspend fun stopAllActive() {
        val wm = WorkManager.getInstance(context)
        wm.cancelAllWorkByTag(DownloadWorker.TAG_DOWNLOAD)
        wm.cancelAllWorkByTag(PlaylistSyncWorker.TAG_PLAYLIST_SYNC)
        for (job in downloadJobDao.getActiveDownloadsOnce()) {
            downloadJobDao.updateDownload(job.copy(state = DownloadState.CANCELLED))
        }
    }

    /**
     * Stops syncing one playlist: cancels its [PlaylistSyncWorker] (via the
     * `playlist_sync_$playlistId` tag) and cancels every active download job
     * belonging to that playlist.
     */
    suspend fun cancelPlaylistSync(playlistId: Long) {
        WorkManager.getInstance(context).cancelAllWorkByTag("playlist_sync_$playlistId")
        for (job in downloadJobDao.getActiveDownloadsForPlaylistOnce(playlistId)) {
            if (job.workManagerId.isNotBlank()) {
                runCatching {
                    WorkManager.getInstance(context).cancelWorkById(
                        java.util.UUID.fromString(job.workManagerId)
                    )
                }
            }
            downloadJobDao.updateDownload(job.copy(state = DownloadState.CANCELLED))
        }
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
            .addTag(PlaylistSyncWorker.TAG_PLAYLIST_SYNC)
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