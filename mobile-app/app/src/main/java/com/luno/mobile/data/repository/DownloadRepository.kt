package com.luno.mobile.data.repository

import android.content.Context
import android.provider.DocumentsContract
import android.util.Log
import androidx.room.withTransaction
import androidx.work.BackoffPolicy
import androidx.work.Constraints
import androidx.work.Data
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import com.luno.mobile.data.artwork.ArtworkStorage
import com.luno.mobile.data.db.AppDatabase
import com.luno.mobile.data.db.dao.DownloadJobDao
import com.luno.mobile.data.db.dao.PlaylistDao
import com.luno.mobile.data.db.dao.TrackDao
import com.luno.mobile.data.db.entity.DownloadJob
import com.luno.mobile.data.db.entity.DownloadState
import com.luno.mobile.data.db.entity.Playlist
import com.luno.mobile.data.db.entity.PlaylistTrack
import com.luno.mobile.data.db.entity.Track
import com.luno.mobile.playback.DownloadWorker
import com.luno.mobile.playback.PlaylistSyncWorker
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import android.net.Uri
import java.io.File
import java.util.UUID
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.NonCancellable
import okhttp3.OkHttpClient
import okhttp3.Request

class DownloadRepository(
    private val downloadJobDao: DownloadJobDao,
    private val context: Context,
    private val playlistDao: PlaylistDao? = null,
    private val trackDao: TrackDao? = null,
    private val database: AppDatabase? = null
) {
    private val unsortedMutex = Mutex()

    companion object {
        const val TAG = "DownloadRepository"
        const val UNSORTED_PLAYLIST_NAME = "Unsorted"
        const val PREVIEW_DOWNLOAD_DIR = "recommendation_previews"
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
        thumbnailUrl: String = "",
        playlistIds: List<Long> = emptyList()
    ): Long {
        Log.d(TAG, "enqueueDownload: $title by $artist")
        Log.d(TAG, "Source URL (truncated): ${sourceUrl.take(120)}")

        val resolvedPlaylistId = playlistId ?: ensureUnsortedPlaylistId()
        val destinationPlaylistIds = buildList {
            resolvedPlaylistId?.let(::add)
            addAll(playlistIds)
        }.distinct()

        val job = DownloadJob(
            sourceUrl = sourceUrl,
            title = title,
            artist = artist,
            state = DownloadState.QUEUED,
            addedAt = System.currentTimeMillis(),
            playlistId = resolvedPlaylistId,
            thumbnailUrl = thumbnailUrl,
            playlistIdsCsv = destinationPlaylistIds.joinToString(",")
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

    private suspend fun ensureUnsortedPlaylistId(): Long? {
        val dao = playlistDao ?: return null
        return unsortedMutex.withLock {
            dao.getPlaylistByName(UNSORTED_PLAYLIST_NAME)?.id
                ?: dao.insertPlaylist(Playlist(name = UNSORTED_PLAYLIST_NAME))
        }
    }

    /** Repairs completed downloads created before playlist-less routing existed. */
    suspend fun repairUnsortedMemberships(trackDao: TrackDao) {
        val dao = playlistDao ?: return
        val unassignedTracks = buildList {
            for (job in downloadJobDao.getCompletedDownloadsOnce()) {
                val track = trackDao.getTrack(job.localUri) ?: continue
                if (!dao.isTrackInAnyPlaylist(track.uri)) add(track)
            }
        }
        if (unassignedTracks.isEmpty()) return

        val unsortedId = ensureUnsortedPlaylistId() ?: return
        var nextOrder = dao.maxSortOrder(unsortedId) + 1
        unassignedTracks.forEach { track ->
            dao.addTrackToPlaylist(
                com.luno.mobile.data.db.entity.PlaylistTrack(
                    playlistId = unsortedId,
                    trackUri = track.uri,
                    sortOrder = nextOrder++
                )
            )
        }
    }

    /**
     * Promotes a temporary recommendation preview into the durable library
     * without downloading the audio a second time. The caller must choose an
     * explicit playlist; preview promotion never falls back to Unsorted.
     */
    suspend fun promotePreview(
        previewFile: File,
        sourceUrl: String,
        title: String,
        artist: String,
        playlistId: Long,
        artworkUrl: String = ""
    ): Track = withContext(Dispatchers.IO + NonCancellable) {
        val tracks = trackDao ?: throw IllegalStateException("Download library is unavailable")
        val playlists = playlistDao ?: throw IllegalStateException("Playlist library is unavailable")
        val appDatabase = database
            ?: throw IllegalStateException("Transactional download library is unavailable")
        if (playlists.getPlaylist(playlistId) == null) {
            throw IllegalArgumentException("The selected playlist no longer exists")
        }

        val previewRoot = File(context.cacheDir, PREVIEW_DOWNLOAD_DIR).canonicalFile
        val source = previewFile.canonicalFile
        if (!source.isFile || !source.toPath().startsWith(previewRoot.toPath())) {
            throw IllegalArgumentException("The temporary preview is no longer available")
        }

        val downloadRoot = File(context.filesDir, DownloadWorker.KEY_DOWNLOAD_DIR).apply {
            mkdirs()
        }
        val extension = source.extension.takeIf { it.isNotBlank() } ?: "audio"
        val uniqueName = "${safeDownloadName(artist, title)}-${UUID.randomUUID()}"
        val stagedFile = File(downloadRoot, "$uniqueName.$extension")
        var destinationUri: Uri? = null
        var artworkDestinationUri: Uri? = null
        var databaseCommitted = false

        try {
            source.copyTo(stagedFile, overwrite = false)
            val durationMs = extractDuration(stagedFile)
            var albumArtPath = ArtworkStorage.saveEmbeddedArtworkFromPath(
                context,
                stagedFile.absolutePath
            )
            if (albumArtPath == null && artworkUrl.isNotBlank()) {
                albumArtPath = fetchArtwork(artworkUrl)
            }

            val mimeType = mimeTypeForExtension(extension)
            destinationUri = MusicFolderRepository(context).copyFileToSelectedFolder(
                sourceFile = stagedFile,
                displayName = stagedFile.name,
                mimeType = mimeType,
                reuseExisting = false
            )
            if (destinationUri != null && albumArtPath != null) {
                File(albumArtPath).takeIf { it.isFile }?.let { artworkFile ->
                    artworkDestinationUri = runCatching {
                        MusicFolderRepository(context).copyFileToSelectedFolder(
                            sourceFile = artworkFile,
                            displayName = "${stagedFile.nameWithoutExtension}.jpg",
                            mimeType = "image/jpeg",
                            reuseExisting = false
                        )
                    }.getOrNull()
                }
            }

            val trackUri = destinationUri?.toString() ?: stagedFile.toURI().toString()
            val now = System.currentTimeMillis()
            val track = Track(
                uri = trackUri,
                title = title,
                artist = artist,
                durationMs = durationMs,
                albumArtPath = albumArtPath,
                addedAt = now
            )
            appDatabase.withTransaction {
                tracks.insertTrack(track)
                playlists.addTrackToPlaylist(
                    PlaylistTrack(
                        playlistId = playlistId,
                        trackUri = trackUri,
                        sortOrder = playlists.maxSortOrder(playlistId) + 1
                    )
                )
                downloadJobDao.insertDownload(
                    DownloadJob(
                        sourceUrl = sourceUrl,
                        title = title,
                        artist = artist,
                        state = DownloadState.COMPLETED,
                        progress = 100,
                        localUri = trackUri,
                        addedAt = now,
                        completedAt = now,
                        playlistId = playlistId,
                        thumbnailUrl = artworkUrl,
                        playlistIdsCsv = playlistId.toString()
                    )
                )
            }
            databaseCommitted = true
            if (destinationUri != null) stagedFile.delete()
            track
        } catch (error: Throwable) {
            stagedFile.delete()
            if (!databaseCommitted) {
                listOfNotNull(artworkDestinationUri, destinationUri).forEach { uri ->
                    runCatching {
                        DocumentsContract.deleteDocument(context.contentResolver, uri)
                    }
                }
            }
            throw error
        }
    }

    /** Copies app-private downloaded songs into the selected Music folder. */
    suspend fun syncAppSongsToMusicFolder(): SyncSongsResult = withContext(Dispatchers.IO) {
        val trackDao = trackDao ?: return@withContext SyncSongsResult(
            error = "Download library is unavailable"
        )
        if (MusicFolderRepository(context).loadTreeUri().isNullOrBlank()) {
            return@withContext SyncSongsResult(error = "Choose a Music folder first")
        }

        val downloadRoot = File(context.filesDir, DownloadWorker.KEY_DOWNLOAD_DIR)
            .canonicalFile
        val memberships = playlistDao?.getAllPlaylistTracksOnce().orEmpty()
        val completedJobs = downloadJobDao.getCompletedDownloadsOnce()
        var synced = 0
        var skipped = 0
        var failed = 0

        trackDao.getAllTracksOnce()
            .filter { track ->
                val uri = Uri.parse(track.uri)
                val file = if (uri.scheme == "file") uri.path?.let(::File) else null
                file != null && file.canonicalFile.toPath().startsWith(downloadRoot.toPath())
            }
            .forEach { track ->
                val sourceFile = Uri.parse(track.uri).path?.let(::File)
                if (sourceFile == null || !sourceFile.isFile) {
                    skipped++
                    return@forEach
                }

                try {
                    val destinationUri = MusicFolderRepository(context)
                        .copyFileToSelectedFolder(
                            sourceFile = sourceFile,
                            displayName = sourceFile.name,
                            mimeType = mimeTypeForExtension(sourceFile.extension)
                        ) ?: throw java.io.IOException("Choose a Music folder first")
                    track.albumArtPath
                        ?.let(::File)
                        ?.takeIf { it.isFile }
                        ?.let { artworkFile ->
                            MusicFolderRepository(context).copyFileToSelectedFolder(
                                sourceFile = artworkFile,
                                displayName = "${sourceFile.nameWithoutExtension}.jpg",
                                mimeType = "image/jpeg"
                            )
                        }
                    val newUri = destinationUri.toString()
                    val trackMemberships = memberships.filter { it.trackUri == track.uri }

                    trackDao.deleteTrack(track.uri)
                    trackDao.insertTrack(track.copy(uri = newUri))
                    playlistDao?.addTracksToPlaylist(
                        trackMemberships.map { it.copy(trackUri = newUri) }
                    )
                    completedJobs
                        .filter { it.localUri == track.uri }
                        .forEach { job ->
                            downloadJobDao.updateDownload(job.copy(localUri = newUri))
                        }
                    sourceFile.delete()
                    synced++
                } catch (_: Exception) {
                    failed++
                }
            }

        SyncSongsResult(synced = synced, skipped = skipped, failed = failed)
    }

    private fun mimeTypeForExtension(extension: String): String = when (extension.lowercase().removePrefix(".")) {
        "mp3" -> "audio/mpeg"
        "m4a" -> "audio/mp4"
        "opus" -> "audio/opus"
        "ogg" -> "audio/ogg"
        else -> "application/octet-stream"
    }

    private fun safeDownloadName(artist: String, title: String): String = "$artist - $title"
        .replace(Regex("[^a-zA-Z0-9_\\- ]"), "")
        .trim()
        .take(120)
        .ifBlank { "recommendation" }

    private fun extractDuration(file: File): Long {
        val retriever = android.media.MediaMetadataRetriever()
        return try {
            retriever.setDataSource(file.absolutePath)
            retriever.extractMetadata(
                android.media.MediaMetadataRetriever.METADATA_KEY_DURATION
            )?.toLongOrNull()?.coerceAtLeast(0L) ?: 0L
        } catch (_: Exception) {
            0L
        } finally {
            runCatching { retriever.release() }
        }
    }

    private fun fetchArtwork(url: String): String? {
        return try {
            val request = Request.Builder()
                .url(url)
                .header("User-Agent", "Luno/1.0 (Android)")
                .build()
            OkHttpClient().newCall(request).execute().use { response ->
                if (!response.isSuccessful) return@use null
                response.body?.bytes()
                    ?.takeIf { it.isNotEmpty() }
                    ?.let { ArtworkStorage.saveImageBytes(context, it) }
            }
        } catch (_: Exception) {
            null
        }
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

data class SyncSongsResult(
    val synced: Int = 0,
    val skipped: Int = 0,
    val failed: Int = 0,
    val error: String? = null
)
