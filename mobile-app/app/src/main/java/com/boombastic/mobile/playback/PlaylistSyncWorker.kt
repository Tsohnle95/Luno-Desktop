package com.boombastic.mobile.playback

import android.content.Context
import android.util.Log
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
import com.boombastic.mobile.R
import com.boombastic.mobile.data.db.AppDatabase
import com.boombastic.mobile.data.db.entity.DownloadState
import java.util.concurrent.TimeUnit

class PlaylistSyncWorker(
    private val context: Context,
    params: WorkerParameters
) : CoroutineWorker(context, params) {

    companion object {
        const val TAG = "PlaylistSyncWorker"
        const val KEY_PLAYLIST_ID = "playlist_id"
        const val KEY_PLAYLIST_NAME = "playlist_name"
        const val KEY_PLAYLIST_URL = "playlist_url"
        /** Common tag for every playlist-sync work — enables bulk cancel. */
        const val TAG_PLAYLIST_SYNC = "playlist_sync"
    }

    override suspend fun doWork(): Result {
        val playlistId = inputData.getLong(KEY_PLAYLIST_ID, -1L)
        val playlistName = inputData.getString(KEY_PLAYLIST_NAME) ?: "Playlist"
        val playlistUrl = inputData.getString(KEY_PLAYLIST_URL) ?: ""

        if (playlistId == -1L || playlistUrl.isBlank()) {
            Log.e(TAG, "Invalid input data: id=$playlistId url=$playlistUrl")
            return Result.failure()
        }

        val db = AppDatabase.getInstance(context)
        val jobDao = db.downloadJobDao()
        val trackDao = db.trackDao()
        val playlistDao = db.playlistDao()

        setForeground(createForegroundInfo(playlistName))

        val result = WebSearchService.getPlaylistVideos(playlistUrl)
        when (result) {
            is ExtractionResult.Error -> {
                Log.w(TAG, "Playlist sync failed: ${result.message}")
                return Result.failure()
            }
            is ExtractionResult.Success -> {
                val name = result.data.first
                val videos = result.data.second

                if (videos.isEmpty()) return Result.success()

                val existingTracks = trackDao.getAllTracksOnce()

                var newCount = 0
                var errorCount = 0
                for (video in videos) {
                    if (isStopped) break

                    val existing = existingTracks.firstOrNull {
                        it.title.contains(video.title, ignoreCase = true) ||
                        it.uri.contains(video.videoId, ignoreCase = true)
                    }
                    if (existing != null) {
                        // Track already in the library — make sure it is in
                        // this playlist (idempotent via IGNORE conflict).
                        val sortOrder = playlistDao.maxSortOrder(playlistId) + 1
                        playlistDao.addTrackToPlaylist(
                            com.boombastic.mobile.data.db.entity.PlaylistTrack(
                                playlistId = playlistId,
                                trackUri = existing.uri,
                                sortOrder = sortOrder
                            )
                        )
                        continue
                    }

                    val queuedCount = jobDao.countByVideoQuery(video.videoId)
                    if (queuedCount > 0) continue

                    val audioResult = WebSearchService.getAudioStreamUrl(video.videoId)
                    when (audioResult) {
                        is ExtractionResult.Error -> {
                            Log.w(TAG, "Skipping ${video.title}: ${audioResult.message}")
                            errorCount++
                            continue
                        }
                        is ExtractionResult.Success -> {
                            val trackTitleParts = video.title.split(" - ", limit = 2)
                            val artist = if (trackTitleParts.size > 1) trackTitleParts[0].trim() else video.artist
                            val trackTitle = if (trackTitleParts.size > 1) trackTitleParts[1].trim() else video.title

                            val job = com.boombastic.mobile.data.db.entity.DownloadJob(
                                sourceUrl = audioResult.data.url,
                                title = trackTitle,
                                artist = artist,
                                state = DownloadState.QUEUED,
                                playlistId = playlistId,
                                thumbnailUrl = video.thumbnailUrl,
                                addedAt = System.currentTimeMillis()
                            )
                            val jobId = jobDao.insertDownload(job)

                            val inputData = Data.Builder()
                                .putLong(DownloadWorker.KEY_DOWNLOAD_JOB_ID, jobId)
                                .putString(DownloadWorker.KEY_THUMBNAIL_URL, video.thumbnailUrl)
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
                                .addTag("playlist_sync_$playlistId")
                                .build()

                            jobDao.updateDownload(job.copy(id = jobId, workManagerId = workRequest.id.toString()))

                            WorkManager.getInstance(context)
                                .enqueueUniqueWork(
                                    "download_$jobId",
                                    ExistingWorkPolicy.REPLACE,
                                    workRequest
                                )

                            newCount++
                        }
                    }
                }

                Log.d(TAG, "Playlist sync complete: $newCount new, $errorCount errors")
                return Result.success()
            }
        }
    }

    private fun createForegroundInfo(playlistName: String): ForegroundInfo {
        val notification = android.app.Notification.Builder(
            context,
            "download_channel"
        )
            .setContentTitle("Syncing playlist")
            .setContentText(playlistName)
            .setSmallIcon(R.drawable.ic_download)
            .setOngoing(true)
            .build()

        return ForegroundInfo(
            2000 + inputData.getLong(KEY_PLAYLIST_ID, -1L).toInt(),
            notification
        )
    }
}
