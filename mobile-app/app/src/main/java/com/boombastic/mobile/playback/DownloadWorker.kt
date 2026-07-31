package com.boombastic.mobile.playback

import android.content.Context
import android.util.Log
import androidx.work.CoroutineWorker
import androidx.work.ForegroundInfo
import androidx.work.WorkerParameters
import com.boombastic.mobile.R
import com.boombastic.mobile.data.db.AppDatabase
import com.boombastic.mobile.data.db.entity.DownloadState
import com.boombastic.mobile.data.db.entity.Track
import java.io.File
import java.io.FileOutputStream
import java.net.HttpURLConnection
import java.net.URL

class DownloadWorker(
    private val context: Context,
    params: WorkerParameters
) : CoroutineWorker(context, params) {

    override suspend fun doWork(): Result {
        val jobId = inputData.getLong(KEY_DOWNLOAD_JOB_ID, -1L)
        if (jobId == -1L) {
            Log.e(TAG, "doWork: no jobId in inputData")
            return Result.failure()
        }

        val db = AppDatabase.getInstance(context)
        val jobDao = db.downloadJobDao()
        val trackDao = db.trackDao()

        val job = jobDao.getDownload(jobId)
        if (job == null) {
            Log.e(TAG, "doWork: job $jobId not found in database")
            return Result.failure()
        }

        Log.d(TAG, "Starting download job $jobId: ${job.title} by ${job.artist}")
        Log.d(TAG, "Source URL (truncated): ${job.sourceUrl.take(120)}")

        jobDao.updateProgress(jobId, DownloadState.DOWNLOADING, 0)
        setForeground(createForegroundInfo(job.title))

        return try {
            val url = URL(job.sourceUrl)
            val connection = url.openConnection() as HttpURLConnection
            connection.connectTimeout = 30000
            connection.readTimeout = 30000
            connection.setRequestProperty("User-Agent", "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.6422.165 Mobile Safari/537.36")
            connection.setRequestProperty("Referer", "https://www.youtube.com")
            connection.connect()

            val responseCode = connection.responseCode
            Log.d(TAG, "HTTP response code: $responseCode for job $jobId")
            if (responseCode != 200) {
                val errorMsg = "HTTP $responseCode downloading ${job.title}"
                Log.w(TAG, errorMsg)
                jobDao.markFailed(jobId, DownloadState.FAILED, errorMsg)
                return Result.failure()
            }

            val contentLength = connection.contentLengthLong
            Log.d(TAG, "Content-Length: $contentLength for job $jobId")
            val inputStream = connection.inputStream

            val downloadDir = File(context.filesDir, KEY_DOWNLOAD_DIR)
            downloadDir.mkdirs()

            val safeFileName = "${job.artist} - ${job.title}"
                .replace(Regex("[^a-zA-Z0-9_\\- ]"), "")
                .trim()
                .ifBlank { "download_$jobId" }
            val file = File(downloadDir, "$safeFileName.mp3")
            var bytesRead: Int
            val buffer = ByteArray(8192)
            var totalBytes = 0L

            FileOutputStream(file).use { outputStream ->
                while (inputStream.read(buffer).also { bytesRead = it } != -1) {
                    if (isStopped) {
                        Log.d(TAG, "Download job $jobId cancelled by user")
                        file.delete()
                        jobDao.markFailed(jobId, DownloadState.FAILED, "Cancelled")
                        return Result.failure()
                    }
                    outputStream.write(buffer, 0, bytesRead)
                    totalBytes += bytesRead
                    if (contentLength > 0) {
                        val progress = ((totalBytes * 100) / contentLength).toInt()
                        jobDao.updateProgress(jobId, DownloadState.DOWNLOADING, progress)
                    }
                }
            }

            inputStream.close()
            connection.disconnect()
            Log.d(TAG, "Downloaded $totalBytes bytes for job $jobId to ${file.absolutePath}")

            val durationMs = try {
                val retriever = android.media.MediaMetadataRetriever()
                retriever.setDataSource(file.absolutePath)
                val durStr = retriever.extractMetadata(
                    android.media.MediaMetadataRetriever.METADATA_KEY_DURATION
                )
                retriever.release()
                val d = durStr?.toLongOrNull() ?: 0L
                Log.d(TAG, "Extracted duration: ${d}ms for job $jobId")
                d
            } catch (e: Exception) {
                Log.w(TAG, "Failed to extract duration for job $jobId", e)
                0L
            }

            val track = Track(
                uri = file.toURI().toString(),
                title = job.title,
                artist = job.artist,
                durationMs = durationMs,
                addedAt = System.currentTimeMillis()
            )
            trackDao.insertTrack(track)
            Log.d(TAG, "Track inserted for job $jobId: ${track.title}")

            jobDao.markCompleted(
                jobId,
                DownloadState.COMPLETED,
                file.toURI().toString(),
                System.currentTimeMillis()
            )

            Log.d(TAG, "Download job $jobId completed successfully")
            Result.success()
        } catch (e: Exception) {
            Log.e(TAG, "Download job $jobId failed", e)
            jobDao.markFailed(jobId, DownloadState.FAILED, "${e::class.simpleName}: ${e.message}")
            if (runAttemptCount < 3) {
                Log.d(TAG, "Will retry job $jobId (attempt ${runAttemptCount + 1})")
                Result.retry()
            } else {
                Log.w(TAG, "Job $jobId exhausted retries")
                Result.failure()
            }
        }
    }

    private fun createForegroundInfo(title: String): ForegroundInfo {
        val notification = android.app.Notification.Builder(
            context,
            "download_channel"
        )
            .setContentTitle("Downloading")
            .setContentText(title)
            .setSmallIcon(R.drawable.ic_download)
            .setOngoing(true)
            .build()

        return ForegroundInfo(
            NOTIFICATION_ID_BASE + inputData.getLong(KEY_DOWNLOAD_JOB_ID, -1L).toInt(),
            notification
        )
    }

    companion object {
        const val TAG = "DownloadWorker"
        const val KEY_DOWNLOAD_JOB_ID = "download_job_id"
        const val KEY_DOWNLOAD_DIR = "downloads"
        private const val NOTIFICATION_ID_BASE = 1000
    }
}