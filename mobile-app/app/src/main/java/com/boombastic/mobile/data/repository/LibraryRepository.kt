package com.boombastic.mobile.data.repository

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.OpenableColumns
import com.boombastic.mobile.data.db.dao.TrackDao
import com.boombastic.mobile.data.db.entity.Track
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.BufferedReader
import java.io.InputStreamReader

class LibraryRepository(
    private val context: Context,
    private val trackDao: TrackDao,
    private val uriPermissionPersister: UriPermissionPersister =
        UriPermissionPersister.Default(context)
) {
    suspend fun importAudioUri(uri: Uri): Result<Track> = withContext(Dispatchers.IO) {
        try {
            // Check dedupe
            if (trackDao.exists(uri.toString())) {
                return@withContext Result.failure(
                    ImportException("Track already imported", ImportError.DUPLICATE)
                )
            }

            // Persist read URI permission using the injectable persister
            uriPermissionPersister.persistReadPermission(uri)

            // Extract metadata safely off main thread
            val track = extractMetadata(uri)
            trackDao.insertTrack(track)
            Result.success(track)
        } catch (e: Exception) {
            Result.failure(
                ImportException("Failed to import: ${e.message}", ImportError.IO_ERROR)
            )
        }
    }

    suspend fun importMultipleUris(uris: List<Uri>): ImportResult {
        var imported = 0
        var duplicates = 0
        var errors = 0

        for (uri in uris) {
            val result = importAudioUri(uri)
            if (result.isSuccess) {
                imported++
            } else {
                val error = result.exceptionOrNull()
                if (error is ImportException && error.error == ImportError.DUPLICATE) {
                    duplicates++
                } else {
                    errors++
                }
            }
        }
        return ImportResult(imported, duplicates, errors)
    }

    private suspend fun extractMetadata(uri: Uri): Track = withContext(Dispatchers.IO) {
        val cursor = context.contentResolver.query(uri, null, null, null, null)
        var title = "Unknown Track"
        var artist = "Unknown Artist"
        var durationMs = 0L

        cursor?.use { c ->
            if (c.moveToFirst()) {
                val nameIndex = c.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                if (nameIndex >= 0) {
                    val fileName = c.getString(nameIndex) ?: "Unknown"
                    title = fileName.removeSuffix(".mp3")
                        .removeSuffix(".wav")
                        .removeSuffix(".flac")
                        .removeSuffix(".ogg")
                        .removeSuffix(".m4a")
                        .removeSuffix(".aac")
                        .removeSuffix(".wma")
                    // Try to split "Artist - Title" pattern
                    val parts = title.split(" - ", limit = 2)
                    if (parts.size == 2) {
                        artist = parts[0].trim()
                        title = parts[1].trim()
                    }
                }
                // Try to get duration from documents provider if available
                val durationIndex = c.getColumnIndex("duration")
                if (durationIndex >= 0) {
                    durationMs = c.getLong(durationIndex)
                }
            }
        }

        // Try MediaMetadataRetriever for more accurate metadata
        try {
            val retriever = android.media.MediaMetadataRetriever()
            retriever.setDataSource(context, uri)
            val extractedTitle = retriever.extractMetadata(
                android.media.MediaMetadataRetriever.METADATA_KEY_TITLE
            )
            val extractedArtist = retriever.extractMetadata(
                android.media.MediaMetadataRetriever.METADATA_KEY_ARTIST
            )
            val extractedDuration = retriever.extractMetadata(
                android.media.MediaMetadataRetriever.METADATA_KEY_DURATION
            )
            retriever.release()

            if (!extractedTitle.isNullOrBlank()) title = extractedTitle
            if (!extractedArtist.isNullOrBlank()) artist = extractedArtist
            if (!extractedDuration.isNullOrBlank()) {
                durationMs = extractedDuration.toLongOrNull() ?: durationMs
            }
        } catch (_: Exception) {
            // Metadata extraction failed, use file-name-based fallback
        }

        Track(
            uri = uri.toString(),
            title = title,
            artist = artist,
            durationMs = durationMs,
            addedAt = System.currentTimeMillis()
        )
    }

    fun getAllTracks() = trackDao.getAllTracks()

    fun searchTracks(query: String) = trackDao.searchTracks(query)

    suspend fun getTrack(uri: String) = trackDao.getTrack(uri)

    suspend fun deleteTrack(uri: String) = trackDao.deleteTrack(uri)

    /**
     * Injectable seam for URI-permission persistence.
     *
     * The [Default] implementation calls
     * [android.content.ContentResolver.takePersistableUriPermission],
     * which is the production SAF requirement.  Tests may inject a fake
     * that records the call without requiring a real URI permission grant.
     */
    fun interface UriPermissionPersister {
        fun persistReadPermission(uri: Uri)

        companion object {
            fun Default(context: Context): UriPermissionPersister =
                UriPermissionPersister { uri ->
                    val takeFlags = Intent.FLAG_GRANT_READ_URI_PERMISSION
                    context.contentResolver.takePersistableUriPermission(uri, takeFlags)
                }
        }
    }

    data class ImportResult(
        val imported: Int,
        val duplicates: Int,
        val errors: Int
    )
}

data class ImportException(
    override val message: String,
    val error: ImportError
) : Exception(message)

enum class ImportError {
    DUPLICATE,
    IO_ERROR
}
