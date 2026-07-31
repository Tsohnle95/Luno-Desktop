package com.boombastic.mobile.data.repository

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import com.boombastic.mobile.data.artwork.ArtworkStorage
import com.boombastic.mobile.data.db.dao.TrackDao
import com.boombastic.mobile.data.db.entity.Track
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.BufferedReader
import java.io.InputStreamReader

class LibraryRepository(
    private val context: Context,
    private val trackDao: TrackDao,
    private val playlistDao: com.boombastic.mobile.data.db.dao.PlaylistDao? = null,
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

            // A folder is never a track — recurse into it instead of
            // importing a bogus "track" named after the folder.
            if (isDirectoryDocument(uri)) {
                return@withContext Result.failure(
                    ImportException("Selected a folder, not a track", ImportError.IO_ERROR)
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

    /**
     * Import for URIs already covered by a persisted **tree** grant
     * (descendants of an imported music folder).  No per-file
     * `takePersistableUriPermission` — that only works on the tree root
     * itself and throws for child document URIs.
     */
    private suspend fun importTrackFromGrantedUri(uri: Uri): Result<Track> =
        withContext(Dispatchers.IO) {
            try {
                if (trackDao.exists(uri.toString())) {
                    return@withContext Result.failure(
                        ImportException("Track already imported", ImportError.DUPLICATE)
                    )
                }
                val track = extractMetadata(uri)
                trackDao.insertTrack(track)
                Result.success(track)
            } catch (e: Exception) {
                Result.failure(
                    ImportException("Failed to import: ${e.message}", ImportError.IO_ERROR)
                )
            }
        }

    suspend fun importMultipleUris(uris: List<Uri>): ImportResult = withContext(Dispatchers.IO) {
        val counters = ImportCounters()

        for (uri in uris) {
            if (isDirectoryDocument(uri)) {
                // A folder selected in the file picker (some pickers, e.g.
                // Samsung's, allow selecting folders): import its audio
                // contents desktop-style (folder name = playlist name)
                // instead of treating the folder as a single track.
                val name = documentDisplayName(uri)?.takeIf { it.isNotBlank() }
                    ?: uri.lastPathSegment?.substringAfterLast('/')
                    ?: UNSORTED_PLAYLIST
                importDocumentFolder(uri, name, counters)
            } else {
                val result = importAudioUri(uri)
                if (result.isSuccess) {
                    counters.imported++
                } else {
                    val error = result.exceptionOrNull()
                    if (error is ImportException && error.error == ImportError.DUPLICATE) {
                        counters.duplicates++
                    } else {
                        counters.errors++
                    }
                }
            }
        }

        ImportResult(counters.imported, counters.duplicates, counters.errors)
    }

    /**
     * Desktop-style library import: pick a music root folder (SAF tree)
     * and every audio file is imported into a playlist named after its
     * folder; files directly in the root land in the "Unsorted" playlist.
     * Mirrors desktop `scan_library` semantics (subdirs = playlists, root =
     * Unsorted).
     *
     * [onProgress] receives `(imported, duplicates, errors)` after each file.
     */
    suspend fun importLibraryTree(
        treeUri: Uri,
        onProgress: (imported: Int, duplicates: Int, errors: Int) -> Unit = { _, _, _ -> }
    ): ImportResult {
        val counters = ImportCounters()

        withContext(Dispatchers.IO) {
            runCatching {
                uriPermissionPersister.persistReadPermission(treeUri)
            }
            importTreeFolder(treeUri, "", UNSORTED_PLAYLIST, counters, onProgress)
        }

        return ImportResult(counters.imported, counters.duplicates, counters.errors)
    }

    /**
     * Recursively imports a folder of a SAF **tree** ([treeUri] is the
     * original root from the picker).  [folderDocumentId] is the folder's
     * document id; `""` means the tree root itself.
     *
     * Descendant URIs are always built from the tree root plus a document
     * id via `buildDocumentUriUsingTree`/`buildChildDocumentsUriUsingTree`.
     * Building them from a child document URI instead produces nested
     * `tree/.../document/.../document/...` URIs that the DocumentsProvider
     * cannot resolve (it only parses the first document segment), and
     * querying a document URI returns the document itself, not its
     * children — so subfolder contents were silently skipped.
     */
    private suspend fun importTreeFolder(
        treeUri: Uri,
        folderDocumentId: String,
        playlistName: String,
        counters: ImportCounters,
        onProgress: (imported: Int, duplicates: Int, errors: Int) -> Unit
    ) {
        val playlistId = playlistIdFor(playlistName)
        for ((documentId, name, mime) in queryTreeChildren(treeUri, folderDocumentId)) {
            if (mime == DocumentsContract.Document.MIME_TYPE_DIR) {
                importTreeFolder(treeUri, documentId, name, counters, onProgress)
            } else if (isAudio(mime, name)) {
                handleAudioChild(
                    childUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, documentId),
                    playlistId = playlistId,
                    counters = counters,
                    onProgress = onProgress
                )
            }
        }
    }

    /**
     * Recursively imports a folder picked as a plain **document** URI via
     * `ACTION_OPEN_DOCUMENT` (folders selected in the file picker).
     * Children are only readable when the provider extends the grant to
     * descendants; otherwise they surface as import errors instead of
     * bogus folder-as-track imports.
     */
    private suspend fun importDocumentFolder(
        folderUri: Uri,
        playlistName: String,
        counters: ImportCounters
    ) {
        val playlistId = playlistIdFor(playlistName)
        for ((childUri, name, mime) in queryDocumentChildren(folderUri)) {
            if (mime == DocumentsContract.Document.MIME_TYPE_DIR) {
                importDocumentFolder(childUri, name, counters)
            } else if (isAudio(mime, name)) {
                handleAudioChild(
                    childUri = childUri,
                    playlistId = playlistId,
                    counters = counters,
                    onProgress = null
                )
            }
        }
    }

    /**
     * Imports one audio [childUri] and books it in [counters]: dedupe,
     * metadata extraction, track insert, playlist membership when
     * [playlistId] is non-null.
     */
    private suspend fun handleAudioChild(
        childUri: Uri,
        playlistId: Long?,
        counters: ImportCounters,
        onProgress: ((imported: Int, duplicates: Int, errors: Int) -> Unit)?
    ) {
        val result = importTrackFromGrantedUri(childUri)
        when {
            result.isSuccess -> {
                val track = result.getOrThrow()
                if (playlistId != null) {
                    runCatching {
                        val sortOrder = playlistDao?.maxSortOrder(playlistId)?.plus(1) ?: 0
                        playlistDao?.addTrackToPlaylist(
                            com.boombastic.mobile.data.db.entity.PlaylistTrack(
                                playlistId = playlistId,
                                trackUri = track.uri,
                                sortOrder = sortOrder
                            )
                        )
                    }
                }
                counters.imported++
            }
            result.exceptionOrNull() is ImportException &&
                (result.exceptionOrNull() as ImportException).error == ImportError.DUPLICATE -> {
                counters.duplicates++
            }
            else -> counters.errors++
        }
        onProgress?.invoke(counters.imported, counters.duplicates, counters.errors)
    }

    private suspend fun playlistIdFor(folderName: String): Long? {
        val dao = playlistDao ?: return null
        return runCatching {
            dao.getPlaylistByName(folderName)?.id
                ?: dao.insertPlaylist(
                    com.boombastic.mobile.data.db.entity.Playlist(name = folderName)
                )
        }.getOrNull()
    }

    /**
     * Lists `(documentId, displayName, mimeType)` for [folderDocumentId]
     * under [treeUri].  The bare tree URI lists the root's children
     * directly; deeper folders are queried via
     * `buildChildDocumentsUriUsingTree` (`.../children`).
     */
    private fun queryTreeChildren(
        treeUri: Uri,
        folderDocumentId: String
    ): List<Triple<String, String, String>> {
        val children = mutableListOf<Triple<String, String, String>>()
        val resolver = context.contentResolver
        val projection = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE
        )
        runCatching {
            val queryUri = if (folderDocumentId.isEmpty()) {
                treeUri
            } else {
                DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, folderDocumentId)
            }
            resolver.query(queryUri, projection, null, null, null)?.use { cursor ->
                val idIndex = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
                val nameIndex = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
                val mimeIndex = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_MIME_TYPE)
                while (cursor.moveToNext()) {
                    val id = cursor.getString(idIndex)
                    val name = cursor.getString(nameIndex)
                    val mime = cursor.getString(mimeIndex)
                    if (id != null) {
                        children.add(Triple(id, name ?: "", mime ?: ""))
                    }
                }
            }
        }
        return children
    }

    /**
     * Lists `(documentUri, displayName, mimeType)` for a folder picked as
     * a plain document URI.  Children are addressed as
     * `content://<authority>/document/<childId>` — document ids carry the
     * full path, so this never nests.
     */
    private fun queryDocumentChildren(folderUri: Uri): List<Triple<Uri, String, String>> {
        val children = mutableListOf<Triple<Uri, String, String>>()
        val resolver = context.contentResolver
        val projection = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE
        )
        runCatching {
            val childrenUri = DocumentsContract.buildChildDocumentsUri(
                folderUri.authority,
                DocumentsContract.getDocumentId(folderUri)
            )
            resolver.query(childrenUri, projection, null, null, null)?.use { cursor ->
                val idIndex = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
                val nameIndex = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
                val mimeIndex = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_MIME_TYPE)
                while (cursor.moveToNext()) {
                    val id = cursor.getString(idIndex)
                    val name = cursor.getString(nameIndex)
                    val mime = cursor.getString(mimeIndex)
                    if (id != null) {
                        children.add(
                            Triple(
                                DocumentsContract.buildDocumentUri(folderUri.authority, id),
                                name ?: "",
                                mime ?: ""
                            )
                        )
                    }
                }
            }
        }
        return children
    }

    /**
     * True when [uri] is a directory document (a folder selected in a
     * file picker).  Querying a document URI returns the document's own
     * row; a directory is identified by the directory MIME type.
     */
    private fun isDirectoryDocument(uri: Uri): Boolean = runCatching {
        val projection = arrayOf(DocumentsContract.Document.COLUMN_MIME_TYPE)
        context.contentResolver.query(uri, projection, null, null, null)?.use { cursor ->
            cursor.moveToFirst() &&
                cursor.getString(0) == DocumentsContract.Document.MIME_TYPE_DIR
        } ?: false
    }.getOrDefault(false)

    private fun documentDisplayName(uri: Uri): String? = runCatching {
        val projection = arrayOf(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
        context.contentResolver.query(uri, projection, null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) cursor.getString(0) else null
        }
    }.getOrNull()

    private fun isAudio(mime: String, name: String): Boolean {
        if (mime.startsWith("audio/")) return true
        val ext = name.substringAfterLast('.', "").lowercase()
        return ext in AUDIO_EXTENSIONS
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
            val albumArtPath = ArtworkStorage.saveEmbeddedArtwork(context, uri)
            retriever.release()

            if (!extractedTitle.isNullOrBlank()) title = extractedTitle
            if (!extractedArtist.isNullOrBlank()) artist = extractedArtist
            if (!extractedDuration.isNullOrBlank()) {
                durationMs = extractedDuration.toLongOrNull() ?: durationMs
            }

            Track(
                uri = uri.toString(),
                title = title,
                artist = artist,
                durationMs = durationMs,
                albumArtPath = albumArtPath,
                addedAt = System.currentTimeMillis()
            )
        } catch (_: Exception) {
            // Metadata extraction failed, use file-name-based fallback
            Track(
                uri = uri.toString(),
                title = title,
                artist = artist,
                durationMs = durationMs,
                addedAt = System.currentTimeMillis()
            )
        }
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

    /** Mutable counters threaded through recursive folder imports. */
    private class ImportCounters(
        var imported: Int = 0,
        var duplicates: Int = 0,
        var errors: Int = 0
    )

    companion object {
        /** Desktop convention: files at the music root belong to "Unsorted". */
        const val UNSORTED_PLAYLIST = "Unsorted"

        private val AUDIO_EXTENSIONS = setOf(
            "mp3", "wav", "flac", "ogg", "m4a", "aac", "wma", "opus", "aiff", "alac"
        )
    }
}

data class ImportException(
    override val message: String,
    val error: ImportError
) : Exception(message)

enum class ImportError {
    DUPLICATE,
    IO_ERROR
}
