package com.boombastic.mobile.playback

import android.content.Context
import androidx.work.CoroutineWorker
import androidx.work.ForegroundInfo
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.Data
import androidx.work.ExistingWorkPolicy
import androidx.work.Constraints
import androidx.work.NetworkType
import androidx.work.BackoffPolicy
import com.boombastic.mobile.data.db.AppDatabase
import com.boombastic.mobile.data.db.entity.DownloadState
import java.util.concurrent.TimeUnit

class PlaylistSyncWorker(
    private val context: Context,
    params: WorkerParameters
) : CoroutineWorker(context, params) {

    companion object {
        const val KEY_PLAYLIST_ID = "playlist_id"
        const val KEY_PLAYLIST_NAME = "playlist_name"
        const val KEY_PLAYLIST_URL = "playlist_url"
    }

    override suspend fun doWork(): Result {
        val playlistId = inputData.getLong(KEY_PLAYLIST_ID, -1L)
        val playlistName = inputData.getString(KEY_PLAYLIST_NAME) ?: "Playlist"
        val playlistUrl = inputData.getString(KEY_PLAYLIST_URL) ?: ""

        if (playlistId == -1L || playlistUrl.isBlank()) return Result.failure()

        val db = AppDatabase.getInstance(context)
        val jobDao = db.downloadJobDao()
        val playlistDao = db.playlistDao()
        val trackDao = db.trackDao()

        setForeground(createForegroundInfo(playlistName))

        val result = WebSearchService.getPlaylistVideos(playlistUrl)
        val name = result.first
        val videos = result.second

        if (videos.isEmpty()) return Result.failure()

        // Get all existing track URIs to avoid re-downloading
        val existingTracks = trackDao.getAllTracksOnce()

        var newCount = 0
        for (video in videos) {
            if (isStopped) break

            // Check if this video is already in the library
            val exists = existingTracks.any {
                it.title.contains(video.title, ignoreCase = true) ||
                it.uri.contains(video.videoId, ignoreCase = true)
            }
            if (exists) continue

            // Check if there's already a queued/downloading job for this video
            val queuedCount = jobDao.countByVideoQuery(video.videoId)
            if (queuedCount > 0) continue

            // Extract audio URL
            val audioUrl = WebSearchService.getAudioStreamUrl(video.videoId)
            if (audioUrl == null) continue

            // Create a download job for this track
            val trackTitleParts = video.title.split(" - ", limit = 2)
            val artist = if (trackTitleParts.size > 1) trackTitleParts[0].trim() else video.artist
            val trackTitle = if (trackTitleParts.size > 1) trackTitleParts[1].trim() else video.title

            val job = com.boombastic.mobile.data.db.entity.DownloadJob(
                sourceUrl = audioUrl,
                title = trackTitle,
                artist = artist,
                state = DownloadState.QUEUED,
                playlistId = playlistId,
                addedAt = System.currentTimeMillis()
            )
            val jobId = jobDao.insertDownload(job)

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
                .addTag("playlist_sync_$playlistId")
                .build()

            jobDao.updateDownload(job.copy(workManagerId = workRequest.id.toString()))

            WorkManager.getInstance(context)
                .enqueueUniqueWork(
                    "download_$jobId",
                    ExistingWorkPolicy.REPLACE,
                    workRequest
                )

            newCount++
        }

        return if (newCount > 0) Result.success() else Result.success()
    }

    private fun createForegroundInfo(playlistName: String): ForegroundInfo {
        val notification = android.app.Notification.Builder(
            context,
            "download_channel"
        )
            .setContentTitle("Syncing playlist")
            .setContentText(playlistName)
            .setSmallIcon(android.R.drawable.stat_sys_download)
            .setOngoing(true)
            .build()

        return ForegroundInfo(
            applicationContext.getString(android.R.string.ok).hashCode(),
            notification
        )
    }
}