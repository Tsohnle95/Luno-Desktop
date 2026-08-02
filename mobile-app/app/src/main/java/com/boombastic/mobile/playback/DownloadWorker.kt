package com.boombastic.mobile.playback

import android.content.Context
import android.util.Log
import androidx.work.CoroutineWorker
import androidx.work.ForegroundInfo
import androidx.work.WorkerParameters
import com.boombastic.mobile.R
import com.boombastic.mobile.data.artwork.ArtworkStorage
import com.boombastic.mobile.data.db.AppDatabase
import com.boombastic.mobile.data.db.entity.DownloadState
import com.boombastic.mobile.data.db.entity.Playlist
import com.boombastic.mobile.data.db.entity.Track
import com.boombastic.mobile.data.repository.DownloadRepository
import okhttp3.ConnectionSpec
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.util.concurrent.TimeUnit

class DownloadWorker(
    private val context: Context,
    params: WorkerParameters
) : CoroutineWorker(context, params) {

    companion object {
        const val TAG = "DownloadWorker"
        const val KEY_DOWNLOAD_JOB_ID = "download_job_id"
        const val KEY_THUMBNAIL_URL = "thumbnail_url"
        const val KEY_DOWNLOAD_DIR = "downloads"
        /** Common tag for every download work — enables bulk cancel (Stop All). */
        const val TAG_DOWNLOAD = "download"
        private const val NOTIFICATION_ID_BASE = 1000
    }

    private val client = OkHttpClient.Builder()
        .connectTimeout(30, TimeUnit.SECONDS)
        .readTimeout(60, TimeUnit.SECONDS)
        .followRedirects(true)
        .followSslRedirects(true)
        .connectionSpecs(listOf(ConnectionSpec.MODERN_TLS, ConnectionSpec.COMPATIBLE_TLS))
        .build()

    override suspend fun doWork(): Result {
        val jobId = inputData.getLong(KEY_DOWNLOAD_JOB_ID, -1L)
        if (jobId == -1L) {
            Log.e(TAG, "doWork: no jobId in inputData")
            return Result.failure()
        }

        val db = AppDatabase.getInstance(context)
        val jobDao = db.downloadJobDao()
        val trackDao = db.trackDao()
        val playlistDao = db.playlistDao()

        val job = jobDao.getDownload(jobId) ?: run {
            Log.e(TAG, "doWork: job $jobId not found")
            return Result.failure()
        }

        val srcUrl = if (job.sourceUrl.startsWith("http://")) {
            job.sourceUrl.replaceFirst("http://", "https://")
        } else job.sourceUrl

        Log.d(TAG, "Downloading $jobId: ${job.title}")

        jobDao.updateProgress(jobId, DownloadState.DOWNLOADING, 0)
        setForeground(createForegroundInfo(job.title))

        val request = Request.Builder()
            .url(srcUrl)
            .header("User-Agent", "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.6422.165 Mobile Safari/537.36")
            .header("Referer", "https://www.youtube.com")
            .header("Origin", "https://www.youtube.com")
            .header("Range", "bytes=0-")
            .build()

        try {
            val response = client.newCall(request).execute()

            val protocol = response.protocol
            Log.d(TAG, "Protocol: $protocol, code: ${response.code}")

            if (response.code != 200 && response.code != 206) {
                // Redacted diagnostics: response headers can carry signed stream
                // URLs (e.g. Location after a redirect) — never log full headers.
                Log.w(TAG, "HTTP ${response.code} from ${response.request.url.host}")
                val contentType = response.header("Content-Type")
                val contentRange = response.header("Content-Range")
                if (contentType != null) Log.w(TAG, "  Content-Type: $contentType")
                if (contentRange != null) Log.w(TAG, "  Content-Range: $contentRange")
                response.close()
                jobDao.markFailed(jobId, DownloadState.FAILED, downloadErrorForCode(response.code))
                return Result.failure()
            }

            val body = response.body
            if (body == null) {
                response.close()
                jobDao.markFailed(jobId, DownloadState.FAILED, "Empty response body")
                return Result.failure()
            }

            val contentLength = body.contentLength()
            Log.d(TAG, "Content-Length: $contentLength, protocol: $protocol")
            val inputStream = body.byteStream()

            val downloadDir = File(context.filesDir, KEY_DOWNLOAD_DIR)
            downloadDir.mkdirs()

            val safeFileName = "${job.artist} - ${job.title}"
                .replace(Regex("[^a-zA-Z0-9_\\- ]"), "")
                .trim()
                .ifBlank { "download_$jobId" }

            val contentType = body.contentType()?.toString() ?: ""
            val ext = when {
                contentType.contains("opus") || contentType.contains("webm") -> ".opus"
                contentType.contains("mp4") || contentType.contains("m4a") -> ".m4a"
                contentType.contains("mp3") || contentType.contains("mpeg") -> ".mp3"
                contentType.contains("ogg") -> ".ogg"
                else -> ".audio"
            }
            val file = File(downloadDir, "$safeFileName$ext")
            Log.d(TAG, "Saving as: $ext (from contentType: $contentType)")

            var totalBytes = 0L
            val buffer = ByteArray(8192)
            var bytesRead: Int

            FileOutputStream(file).use { outputStream ->
                while (inputStream.read(buffer).also { bytesRead = it } != -1) {
                    if (isStopped) {
                        Log.d(TAG, "Cancelled job $jobId")
                        file.delete()
                        response.close()
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

            response.close()
            Log.d(TAG, "Downloaded $totalBytes bytes to ${file.absolutePath}")

            val durationMs = try {
                val retriever = android.media.MediaMetadataRetriever()
                retriever.setDataSource(file.absolutePath)
                val dur = retriever.extractMetadata(
                    android.media.MediaMetadataRetriever.METADATA_KEY_DURATION
                )?.toLongOrNull() ?: 0L
                retriever.release()
                dur
            } catch (e: Exception) {
                Log.w(TAG, "Duration extraction failed", e)
                0L
            }

            // YouTube audio streams carry no embedded album art, so fall
            // back to the video thumbnail when one was captured at enqueue
            // time (search results / playlist sync).
            var albumArtPath = ArtworkStorage.saveEmbeddedArtworkFromPath(
                context, file.absolutePath
            )
            if (albumArtPath == null) {
                val thumbnailUrl = inputData.getString(KEY_THUMBNAIL_URL)
                if (!thumbnailUrl.isNullOrBlank()) {
                    albumArtPath = fetchThumbnail(thumbnailUrl)
                }
            }

            val track = Track(
                uri = file.toURI().toString(),
                title = job.title,
                artist = job.artist,
                durationMs = durationMs,
                albumArtPath = albumArtPath,
                addedAt = System.currentTimeMillis()
            )
            trackDao.insertTrack(track)

            // Every download belongs to a playlist. Legacy jobs created before
            // Unsorted routing get repaired here before completion.
            val targetPlaylistIds = buildList {
                job.playlistId?.let(::add)
                job.playlistIdsCsv
                    .split(',')
                    .mapNotNull { it.trim().toLongOrNull() }
                    .forEach { if (it !in this) add(it) }
            }.ifEmpty {
                listOf(ensureUnsortedPlaylistId(playlistDao))
            }
            targetPlaylistIds.forEach { playlistId ->
                val sortOrder = playlistDao.maxSortOrder(playlistId) + 1
                playlistDao.addTrackToPlaylist(
                    com.boombastic.mobile.data.db.entity.PlaylistTrack(
                        playlistId = playlistId,
                        trackUri = track.uri,
                        sortOrder = sortOrder
                    )
                )
            }

            jobDao.markCompleted(jobId, DownloadState.COMPLETED, file.toURI().toString(), System.currentTimeMillis())
            Log.d(TAG, "Job $jobId complete")
            return Result.success()
        } catch (e: IOException) {
            Log.e(TAG, "Download failed for job $jobId", e)
            jobDao.markFailed(jobId, DownloadState.FAILED, "${e::class.simpleName}: ${e.message}")
            return if (runAttemptCount < 3) Result.retry() else Result.failure()
        }
    }

    private suspend fun ensureUnsortedPlaylistId(
        playlistDao: com.boombastic.mobile.data.db.dao.PlaylistDao
    ): Long {
        return playlistDao.getPlaylistByName(DownloadRepository.UNSORTED_PLAYLIST_NAME)?.id
            ?: playlistDao.insertPlaylist(Playlist(name = DownloadRepository.UNSORTED_PLAYLIST_NAME))
    }

    /**
     * Downloads a thumbnail image (YouTube i.ytimg.com) and caches it via
     * [ArtworkStorage].  Returns the artwork file path or `null` on failure.
     */
    private fun fetchThumbnail(url: String): String? {
        return try {
            val request = Request.Builder()
                .url(url)
                .header("User-Agent", "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36")
                .build()
            client.newCall(request).execute().use { response ->
                if (response.code != 200) {
                    null
                } else {
                    val bytes = response.body?.bytes()
                    if (bytes == null || bytes.isEmpty()) {
                        null
                    } else {
                        ArtworkStorage.saveImageBytes(context, bytes)
                    }
                }
            }
        } catch (e: Exception) {
            Log.w(TAG, "Thumbnail fetch failed for $url", e)
            null
        }
    }

    private fun downloadErrorForCode(code: Int): String = when (code) {
        400 -> "HTTP 400 - Stream request invalid (expired or wrong format; re-extract and retry)"
        403 -> "HTTP 403 - Stream authorization denied (re-extract to get a fresh URL)"
        404 -> "HTTP 404 - Stream URL no longer available"
        429 -> "HTTP 429 - Rate limited; try again later"
        503 -> "HTTP 503 - Server temporarily unavailable; try again later"
        else -> "HTTP $code - Download failed"
    }

    private fun createForegroundInfo(title: String): ForegroundInfo {
        val notification = android.app.Notification.Builder(context, "download_channel")
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
}
