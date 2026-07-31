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
    /**
     * Imports a single audio document.
     *
     * [existingDocIds] — when provided — holds the document ids of every
     * track already in the library (and of tracks added earlier in this
     * run).  Dedupe then matches by **document id** as well as by exact
     * URI string, because the same physical file has different URI
     * strings depending on how it was imported: the picker returns
     * `…/document/<id>` while folder (tree) imports build
     * `…/tree/<treeId>/document/<id>`.  Without this, re-importing the
     * same song through the other flow creates a second Track row.
     */
    suspend fun importAudioUri(
        uri: Uri,
        existingDocIds: MutableSet<String>? = null
    ): Result<Track> = withContext(Dispatchers.IO) {
        try {
            // Check dedupe
            if (trackDao.exists(uri.toString()) || isDocumentIdDuplicate(uri, existingDocIds)) {
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

            // A non-audio document is never a track either.  The multi-file
            // picker launches with `*/*` (so playlist folders can be
            // selected), which also surfaces non-audio files — reject them
            // instead of importing bogus tracks.
            if (!isAudio(documentMimeType(uri) ?: "", documentDisplayName(uri) ?: "")) {
                return@withContext Result.failure(
                    ImportException("Not an audio file", ImportError.IO_ERROR)
                )
            }

            // Persist read URI permission using the injectable persister
            uriPermissionPersister.persistReadPermission(uri)

            // Extract metadata safely off main thread
            val track = extractMetadata(uri)
            trackDao.insertTrack(track)
            documentIdOf(uri.toString())?.let { existingDocIds?.add(it) }
            Result.success(track)
        } catch (e: Throwable) {
            // `Throwable` (not just `Exception`): an OutOfMemoryError while
            // decoding huge embedded artwork must become one failed file,
            // never a silently dead import.
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
    private suspend fun importTrackFromGrantedUri(
        uri: Uri,
        existingDocIds: MutableSet<String>?
    ): Result<Track> =
        withContext(Dispatchers.IO) {
            try {
                if (trackDao.exists(uri.toString()) || isDocumentIdDuplicate(uri, existingDocIds)) {
                    return@withContext Result.failure(
                        ImportException("Track already imported", ImportError.DUPLICATE)
                    )
                }
                val track = extractMetadata(uri)
                trackDao.insertTrack(track)
                documentIdOf(uri.toString())?.let { existingDocIds?.add(it) }
                Result.success(track)
            } catch (e: Throwable) {
                // `Throwable`: one bad file must never kill the whole import.
                Result.failure(
                    ImportException("Failed to import: ${e.message}", ImportError.IO_ERROR)
                )
            }
        }

    suspend fun importMultipleUris(uris: List<Uri>): ImportResult = withContext(Dispatchers.IO) {
        val counters = ImportCounters()
        counters.existingDocIds += loadExistingDocumentIds()

        for (uri in uris) {
            when {
                // Some pickers (e.g. Samsung's) return selected folders as
                // tree URIs — import them like an `ACTION_OPEN_DOCUMENT_TREE`
                // root (folder name = playlist name).
                DocumentsContract.isTreeUri(uri) -> {
                    val persisted = runCatching {
                        uriPermissionPersister.persistReadPermission(uri)
                    }.isSuccess
                    if (!persisted) counters.persistFailures++
                    importTreeFolder(uri, "", folderDisplayName(uri), counters) { _, _, _ -> }
                }
                // A folder selected in the file picker: import its audio
                // contents desktop-style (folder name = playlist name)
                // instead of treating the folder as a single track.
                isFolderDocument(uri) -> {
                    // Persist the folder grant (best effort): playback of
                    // its songs after an app restart needs the grant; a
                    // provider that extends it to descendants keeps them
                    // readable.  Some providers don't support persistence —
                    // then the songs still import and play this session.
                    runCatching { uriPermissionPersister.persistReadPermission(uri) }
                    importDocumentFolder(uri, folderDisplayName(uri), counters)
                }
                else -> {
                    val result = importAudioUri(uri, counters.existingDocIds)
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
        }

        ImportResult(counters.imported, counters.duplicates, counters.errors, counters.persistFailures)
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
        counters.existingDocIds += loadExistingDocumentIds()

        withContext(Dispatchers.IO) {
            // Persisting the root grant is what keeps every imported song
            // playable after an app restart — never swallow the failure
            // silently; report it so the UI can warn the user.
            val persisted = runCatching {
                uriPermissionPersister.persistReadPermission(treeUri)
            }.isSuccess
            if (!persisted) counters.persistFailures++
            importTreeFolder(treeUri, "", UNSORTED_PLAYLIST, counters, onProgress)
        }

        return ImportResult(counters.imported, counters.duplicates, counters.errors, counters.persistFailures)
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
        val result = importTrackFromGrantedUri(childUri, counters.existingDocIds)
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
     * under [treeUri].
     *
     * Root listing: the canonical children URI for the tree root is
     * `buildChildDocumentsUriUsingTree(treeUri, getTreeDocumentId(treeUri))`.
     * Querying the **bare tree URI** returns the tree root document's own
     * row on standard (AOSP-style) providers — never its children — so the
     * root's contents must come from the canonical `.../children` URI.
     * Samsung-style providers that don't answer `.../children` return
     * children when the folder document itself is queried, so the bare
     * tree URI is kept as a fallback for the root and
     * `buildDocumentUriUsingTree` (a folder-document query) as the
     * fallback for deeper folders.
     */
    private fun queryTreeChildren(
        treeUri: Uri,
        folderDocumentId: String
    ): List<Triple<String, String, String>> {
        val queryUris = if (folderDocumentId.isEmpty()) {
            val rootChildrenUri = runCatching {
                DocumentsContract.buildChildDocumentsUriUsingTree(
                    treeUri,
                    DocumentsContract.getTreeDocumentId(treeUri)
                )
            }.getOrNull() ?: treeUri
            listOf(rootChildrenUri, treeUri)
        } else {
            listOf(
                DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, folderDocumentId),
                DocumentsContract.buildDocumentUriUsingTree(treeUri, folderDocumentId)
            )
        }
        for (queryUri in queryUris) {
            val children = queryTreeRows(queryUri)
            if (children.isNotEmpty()) return children
        }
        return emptyList()
    }

    private fun queryTreeRows(
        queryUri: Uri
    ): List<Triple<String, String, String>> {
        val children = mutableListOf<Triple<String, String, String>>()
        val resolver = context.contentResolver
        val projection = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE
        )
        runCatching {
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
     *
     * Providers disagree on how to enumerate a folder document's children:
     * standard ones answer `document/<id>/children`, while some (e.g.
     * Samsung My Files) return the children when the folder document itself
     * is queried (the folder's own row — same document id — is not a
     * child).  Both forms are tried.
     */
    private fun queryDocumentChildren(folderUri: Uri): List<Triple<Uri, String, String>> {
        val authority = folderUri.authority ?: return emptyList()
        val folderId = DocumentsContract.getDocumentId(folderUri)

        val viaChildren = queryDocumentRows(
            DocumentsContract.buildChildDocumentsUri(authority, folderId)
        ) { id ->
            DocumentsContract.buildDocumentUri(authority, id)
        }
        if (viaChildren.isNotEmpty()) return viaChildren

        // Fallback: querying the folder document itself returns its
        // children on some providers; the folder's own row is excluded.
        val selfId = folderId
        return queryDocumentRows(folderUri) { id ->
            if (id == selfId) {
                null
            } else {
                DocumentsContract.buildDocumentUri(authority, id)
            }
        }
    }

    /**
     * Queries [queryUri] for `(documentId, displayName, mimeType)` rows and
     * maps them to document URIs via [toUri]; rows that map to `null` are
     * skipped.  Returns an empty list when the query fails or has no rows.
     */
    private fun queryDocumentRows(
        queryUri: Uri,
        toUri: (String) -> Uri?
    ): List<Triple<Uri, String, String>> {
        val rows = mutableListOf<Triple<Uri, String, String>>()
        val resolver = context.contentResolver
        val projection = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE
        )
        runCatching {
            resolver.query(queryUri, projection, null, null, null)?.use { cursor ->
                val idIndex = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
                val nameIndex = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
                val mimeIndex = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_MIME_TYPE)
                while (cursor.moveToNext()) {
                    val id = cursor.getString(idIndex)
                    val name = cursor.getString(nameIndex)
                    val mime = cursor.getString(mimeIndex)
                    if (id != null) {
                        val docUri = toUri(id)
                        if (docUri != null) {
                            rows.add(Triple(docUri, name ?: "", mime ?: ""))
                        }
                    }
                }
            }
        }
        return rows
    }

    /**
     * True when [uri] is a directory document.  A file never has children,
     * so a non-empty children listing is the definitive test; folders
     * whose children cannot be listed fall back to the document's own MIME
     * type.  Tree URIs are always folders.
     */
    private fun isFolderDocument(uri: Uri): Boolean {
        if (DocumentsContract.isTreeUri(uri)) return true
        if (queryDocumentChildren(uri).isNotEmpty()) return true
        return isDirectoryDocument(uri)
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

    private fun documentMimeType(uri: Uri): String? = runCatching {
        val projection = arrayOf(DocumentsContract.Document.COLUMN_MIME_TYPE)
        context.contentResolver.query(uri, projection, null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) cursor.getString(0) else null
        }
    }.getOrNull()

    /**
     * Best-effort folder name for a picker-returned folder URI.
     *
     * External-storage providers encode the folder path in the URI's last
     * path segment, which is always correct; Samsung-style providers
     * additionally return a folder's *children* when its document is
     * queried, so the queried display name would be the first song's name.
     * For other providers the queried display name is preferred (e.g. the
     * Downloads provider, whose document ids are opaque).
     */
    private fun folderDisplayName(uri: Uri): String {
        val pathName = uri.lastPathSegment?.substringAfterLast('/')?.takeIf { it.isNotBlank() }
        val name = if (uri.authority?.contains("externalstorage") == true) {
            pathName ?: documentDisplayName(uri)
        } else {
            documentDisplayName(uri) ?: pathName
        }
        return name?.takeIf { it.isNotBlank() } ?: UNSORTED_PLAYLIST
    }

    /**
     * Document ids (`authority + "/" + documentId`) of every track already
     * in the library — built once per import run so cross-flow re-imports
     * (picker `document/…` vs folder `tree/…/document/…` URIs for the same
     * physical file) are detected without an O(n²) per-file scan.
     */
    private suspend fun loadExistingDocumentIds(): Set<String> = withContext(Dispatchers.IO) {
        buildSet {
            trackDao.getAllTracksOnce().forEach { track ->
                documentIdOf(track.uri)?.let { add(it) }
            }
        }
    }

    private fun isDocumentIdDuplicate(uri: Uri, existingDocIds: Set<String>?): Boolean {
        val id = documentIdOf(uri.toString()) ?: return false
        return id in existingDocIds.orEmpty()
    }

    /**
     * Stable identity for a SAF document across URI forms: `authority` +
     * the document id, for both `…/document/<id>` (picker) and
     * `…/tree/<treeId>/document/<id>` (tree grant) URIs — the two forms of
     * the same physical file.  Returns null for non-SAF URIs (e.g. `file://`
     * downloads), which can never collide across flows.
     */
    private fun documentIdOf(uriString: String): String? = runCatching {
        val uri = Uri.parse(uriString)
        val segments = uri.pathSegments ?: return null
        val documentId = when {
            segments.size >= 2 && segments[0] == "document" -> segments[1]
            segments.size >= 4 && segments[0] == "tree" && segments[2] == "document" -> segments[3]
            else -> return null
        }
        uri.authority?.let { "$it/$documentId" }
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
        val errors: Int,
        /** Tree/folder grants the provider refused to persist — those songs
         *  may stop playing after an app restart. */
        val persistFailures: Int = 0
    )

    /** Mutable counters threaded through recursive folder imports. */
    private class ImportCounters(
        var imported: Int = 0,
        var duplicates: Int = 0,
        var errors: Int = 0,
        var persistFailures: Int = 0
    ) {
        /** Document ids of all known tracks (preloaded + grown as the run imports). */
        val existingDocIds: MutableSet<String> = mutableSetOf()
    }

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
