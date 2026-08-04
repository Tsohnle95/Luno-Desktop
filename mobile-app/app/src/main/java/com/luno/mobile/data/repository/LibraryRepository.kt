package com.luno.mobile.data.repository

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import com.luno.mobile.data.artwork.ArtworkFetchService
import com.luno.mobile.data.artwork.ArtworkStorage
import com.luno.mobile.data.db.dao.TrackDao
import com.luno.mobile.data.db.dao.DownloadJobDao
import com.luno.mobile.data.db.entity.Track
import kotlin.coroutines.resume
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withPermit
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import java.io.BufferedReader
import java.io.InputStreamReader
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.ExecutorService
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.SynchronousQueue
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

class LibraryRepository(
    private val context: Context,
    private val trackDao: TrackDao,
    private val playlistDao: com.luno.mobile.data.db.dao.PlaylistDao? = null,
    private val uriPermissionPersister: UriPermissionPersister =
        UriPermissionPersister.Default(context),
    private val artworkFetchService: ArtworkFetchService = ArtworkFetchService(),
    private val remoteArtwork: suspend (Track) -> String? = { track ->
        artworkFetchService.fetchAndSave(context, track.artist, track.title)
    },
    private val thumbnailArtwork: suspend (String) -> String? = { url ->
        artworkFetchService.fetchAndSaveImage(context, url)
    },
    private val downloadJobDao: DownloadJobDao? = null
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
                        withAbandonableTimeout(GRANT_TIMEOUT_MS) {
                            uriPermissionPersister.persistReadPermission(uri)
                        }
                    }.isSuccess
                    if (!persisted) counters.persistFailures++
                    importTreeFolder(uri, "", folderDisplayName(uri), counters, 0) { _, _, _ -> }
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
                    runCatching {
                        withAbandonableTimeout(GRANT_TIMEOUT_MS) {
                            uriPermissionPersister.persistReadPermission(uri)
                        }
                    }
                    importDocumentFolder(uri, folderDisplayName(uri), counters, 0)
                }
                else -> {
                    val result = importAudioUri(uri, counters.existingDocIds)
                    if (result.isSuccess) {
                        counters.incrementImported()
                    } else {
                        val error = result.exceptionOrNull()
                        if (error is ImportException && error.error == ImportError.DUPLICATE) {
                            counters.incrementDuplicates()
                        } else {
                            counters.incrementErrors()
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
            // silently; report it so the UI can warn the user.  The call is
            // a blocking Binder transaction that can hang under provider
            // load, so it runs with the abandonable timeout (it happens
            // before the first progress tick — an unbounded hang here
            // would read as a stall).
            val persisted = runCatching {
                withAbandonableTimeout(GRANT_TIMEOUT_MS) {
                    uriPermissionPersister.persistReadPermission(treeUri)
                }
            }.isSuccess
            if (!persisted) counters.persistFailures++
            importTreeFolder(treeUri, "", UNSORTED_PLAYLIST, counters, 0, onProgress)
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
     *
     * Recursion safety: some providers answer a folder-document query with
     * the folder's **own row** (the fallback in [queryTreeChildren]), and
     * for an empty folder that is the only row — without a self-row guard
     * the import would recurse into the same folder forever and crash the
     * app with a StackOverflowError mid-import.  Rows whose document id
     * equals the folder being listed are skipped, the recursion depth is
     * capped, and a Throwable in one folder (e.g. a provider that returns
     * itself as its own child anyway) is contained so it can never kill
     * the whole import or the app.
     */
    private suspend fun importTreeFolder(
        treeUri: Uri,
        folderDocumentId: String,
        playlistName: String,
        counters: ImportCounters,
        depth: Int,
        onProgress: (imported: Int, duplicates: Int, errors: Int) -> Unit
    ) {
        if (depth > MAX_FOLDER_DEPTH) return
        // Progress tick FIRST — before any Room work or listing: every call
        // below is timeout-bounded, so a tick at folder entry guarantees
        // the stall watchdog sees activity at least every ~50s no matter
        // which provider/Room call happens to hang.
        onProgress(counters.imported, counters.duplicates, counters.errors)
        val playlistId = withTimeoutOrNull(ROOM_TIMEOUT_MS) { playlistIdFor(playlistName) }
        counters.seedSortOrder(
            playlistId,
            withTimeoutOrNull(ROOM_TIMEOUT_MS) { playlistId?.let { playlistDao?.maxSortOrder(it) } }
        )
        val children = queryTreeChildren(treeUri, folderDocumentId)
        if (children == null) {
            // The folder could not be listed even after retries — report
            // it instead of silently skipping its files (a silent skip is
            // exactly what produced the random 1000–1800-duplicate counts
            // with 0 errors).
            counters.incrementErrors()
            onProgress(counters.imported, counters.duplicates, counters.errors)
            return
        }

        // Process the folder's children with bounded parallelism: a single
        // bad file must not stall a 4k-song import, and 4 workers make a
        // full import 3-4x faster.  The semaphore serializes only the
        // file work — directory recursion runs without a permit, so a
        // parent can never hold a permit its own children are waiting for
        // (that would deadlock the whole import at the final counts).
        val semaphore = Semaphore(PARALLELISM)
        coroutineScope {
            children.map { (documentId, name, mime) ->
                async {
                    try {
                        // A folder must never recurse into itself: when a
                        // provider returns the folder's own row as a
                        // "child", skip it or we loop until SOE.
                        if (documentId == folderDocumentId) return@async
                        if (mime == DocumentsContract.Document.MIME_TYPE_DIR) {
                            importTreeFolder(treeUri, documentId, name, counters, depth + 1, onProgress)
                        } else if (isAudio(mime, name)) {
                            semaphore.withPermit {
                                handleAudioChild(
                                    childUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, documentId),
                                    playlistId = playlistId,
                                    counters = counters,
                                    onProgress = onProgress
                                )
                            }
                        }
                    } catch (e: CancellationException) {
                        throw e
                    } catch (_: Throwable) {
                        // One bad folder must never crash the app
                        // mid-import — but it must be counted, never
                        // silently dropped.
                        counters.incrementErrors()
                        onProgress(counters.imported, counters.duplicates, counters.errors)
                    }
                }
            }.awaitAll()
        }
    }

    /**
     * Recursively imports a folder picked as a plain **document** URI via
     * `ACTION_OPEN_DOCUMENT` (folders selected in the file picker).
     * Children are only readable when the provider extends the grant to
     * descendants; otherwise they surface as import errors instead of
     * bogus folder-as-track imports.  Same self-row guard + depth cap as
     * the tree walk.
     */
    private suspend fun importDocumentFolder(
        folderUri: Uri,
        playlistName: String,
        counters: ImportCounters,
        depth: Int
    ) {
        if (depth > MAX_FOLDER_DEPTH) return
        val playlistId = withTimeoutOrNull(ROOM_TIMEOUT_MS) { playlistIdFor(playlistName) }
        counters.seedSortOrder(
            playlistId,
            withTimeoutOrNull(ROOM_TIMEOUT_MS) { playlistId?.let { playlistDao?.maxSortOrder(it) } }
        )
        val children = queryDocumentChildren(folderUri)
        if (children == null) {
            // Unlistable folder — report it instead of silently skipping.
            counters.incrementErrors()
            return
        }

        val semaphore = Semaphore(PARALLELISM)
        coroutineScope {
            children.map { (childUri, name, mime) ->
                async {
                    try {
                        if (childUri == folderUri) return@async
                        if (mime == DocumentsContract.Document.MIME_TYPE_DIR) {
                            importDocumentFolder(childUri, name, counters, depth + 1)
                        } else if (isAudio(mime, name)) {
                            semaphore.withPermit {
                                handleAudioChild(
                                    childUri = childUri,
                                    playlistId = playlistId,
                                    counters = counters,
                                    onProgress = null
                                )
                            }
                        }
                    } catch (e: CancellationException) {
                        throw e
                    } catch (_: Throwable) {
                        // One bad folder must never crash the app
                        // mid-import — but it must be counted, never
                        // silently dropped.
                        counters.incrementErrors()
                    }
                }
            }.awaitAll()
        }
    }

    /**
     * Imports one audio [childUri] and books it in [counters]: dedupe,
     * metadata extraction, track insert, playlist membership when
     * [playlistId] is non-null.
     *
     * The whole pipeline is wrapped in a hard timeout: the Room DAO calls
     * (dedupe, insert, playlist membership) are cancellable suspends, so a
     * wedged SQLite/SD card or a backed-up Room executor can delay a file
     * forever without any progress tick firing — a stuck file must fail
     * as an error, never freeze the import or trip the stall watchdog.
     */
    private suspend fun handleAudioChild(
        childUri: Uri,
        playlistId: Long?,
        counters: ImportCounters,
        onProgress: ((imported: Int, duplicates: Int, errors: Int) -> Unit)?
    ) {
        val result = withTimeoutOrNull(FILE_WORK_TIMEOUT_MS) {
            val r = importTrackFromGrantedUri(childUri, counters.existingDocIds)
            if (r.isSuccess && playlistId != null) {
                runCatching {
                    val sortOrder = counters.nextSortOrder(playlistId) ?: 0
                    playlistDao?.addTrackToPlaylist(
                        com.luno.mobile.data.db.entity.PlaylistTrack(
                            playlistId = playlistId,
                            trackUri = r.getOrThrow().uri,
                            sortOrder = sortOrder
                        )
                    )
                }
            }
            r
        } ?: Result.failure(
            ImportException("Timed out importing file", ImportError.IO_ERROR)
        )
        when {
            result.isSuccess -> {
                counters.incrementImported()
            }
            result.exceptionOrNull() is ImportException &&
                (result.exceptionOrNull() as ImportException).error == ImportError.DUPLICATE -> {
                counters.incrementDuplicates()
            }
            else -> counters.incrementErrors()
        }
        // A callback failure must never cancel the whole import (it would
        // silently kill the appScope coroutine and freeze the progress UI).
        runCatching {
            onProgress?.invoke(counters.imported, counters.duplicates, counters.errors)
        }
    }

    private suspend fun playlistIdFor(folderName: String): Long? {
        val dao = playlistDao ?: return null
        return try {
            dao.getPlaylistByName(folderName)?.id
                ?: dao.insertPlaylist(
                    com.luno.mobile.data.db.entity.Playlist(name = folderName)
                )
        } catch (e: CancellationException) {
            // Never swallow cancellation — a cancelled import must unwind.
            throw e
        } catch (_: Throwable) {
            null
        }
    }

    /**
     * Lists `(documentId, displayName, mimeType)` for [folderDocumentId]
     * under [treeUri].  Returns `null` when the folder could NOT be listed
     * at all (every retry timed out / was rejected) — the caller reports
     * that as an import error instead of silently skipping the folder's
     * files.  An empty list means the folder is genuinely empty (both
     * query paths answered successfully with no rows).
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
    private suspend fun queryTreeChildren(
        treeUri: Uri,
        folderDocumentId: String
    ): List<Triple<String, String, String>>? {
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
        // The canonical children query gets the full timeout.  The fallback
        // (folder-document query) gets a short one: it only exists for
        // providers that answer instantly instead of supporting
        // `.../children` (Samsung-style) — a provider that HANGS on it is
        // wasting the import.  If the primary path times out or is
        // rejected, the fallback is still tried before giving up.
        val answered = booleanArrayOf(false, false)
        for ((index, queryUri) in queryUris.withIndex()) {
            val timeoutMs = if (index == 0) QUERY_TIMEOUT_MS else FALLBACK_QUERY_TIMEOUT_MS
            val rows = queryTreeRows(queryUri, timeoutMs)
            if (rows == null) continue // unlistable — try the fallback path
            answered[index] = true
            if (rows.isNotEmpty()) return rows
        }
        if (answered.all { it }) {
            // Both query paths answered successfully with no rows:
            // genuinely empty folder.
            return emptyList()
        }
        // Some path never answered.  An empty answer from ONE path is not
        // proof of an empty folder: on Samsung-style providers the
        // canonical `.../children` query always answers empty (it is
        // unsupported), so the fallback is the authoritative listing —
        // and a provider whose canonical listing is unlistable may be
        // hiding rows the fallback could not reach.  The folder could NOT
        // be verified as empty, so report it as unlistable instead of
        // silently dropping its files (the silent-skip bug that produced
        // random partial counts with 0 errors).
        return null
    }

    /**
     * Queries [queryUri] with retries.  Returns the rows, or `null` when
     * every attempt timed out or was rejected.
     *
     * A single timeout/rejection means the provider was slow or the
     * extraction executor was momentarily busy — neither proves the folder
     * is empty.  Treating a failed listing as "empty" silently dropped
     * whole folders from the import (the random 1000–1800-duplicate
     * results with 0 errors).  Retrying recovers folders whose provider
     * answers slowly, and the caller reports a folder that stays
     * unlistable as an error instead of hiding it.
     */
    private suspend fun queryTreeRows(
        queryUri: Uri,
        timeoutMs: Long
    ): List<Triple<String, String, String>>? {
        repeat(QUERY_RETRIES) {
            val rows = withListingTimeout(timeoutMs) { queryTreeRowsBlocking(queryUri) }
            if (rows != null) return rows
        }
        return null
    }

    private fun queryTreeRowsBlocking(
        queryUri: Uri
    ): List<Triple<String, String, String>>? {
        val children = mutableListOf<Triple<String, String, String>>()
        val resolver = context.contentResolver
        val projection = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE
        )
        return try {
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
            children
        } catch (_: Throwable) {
            // The query FAILED (rejected URI, TransactionTooLargeException
            // on an enormous folder, provider crash...) — that is
            // "unlistable", NEVER "empty".  Returning null lets the caller
            // retry and finally report the folder as an error instead of
            // silently dropping its files.
            null
        }
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
     *
     * Returns `null` when neither form answered even after retries (the
     * caller reports the folder as an error instead of silently skipping
     * its files).
     */
    private suspend fun queryDocumentChildren(
        folderUri: Uri
    ): List<Triple<Uri, String, String>>? {
        val authority = folderUri.authority ?: return emptyList()
        val folderId = DocumentsContract.getDocumentId(folderUri)

        val viaChildren = queryDocumentRows(
            DocumentsContract.buildChildDocumentsUri(authority, folderId)
        ) { id ->
            // A folder must never list itself as its own child.
            if (id == folderId) null else DocumentsContract.buildDocumentUri(authority, id)
        }
        if (viaChildren != null) {
            if (viaChildren.isNotEmpty()) return viaChildren
            // Answered empty: try the fallback form (Samsung-style folders
            // return children on the folder-document query).
        } else {
            // `.../children` never answered — try the folder-document form.
        }

        // Fallback: querying the folder document itself returns its
        // children on some providers; the folder's own row is excluded.
        val selfId = folderId
        val viaSelf = queryDocumentRows(folderUri) { id ->
            if (id == selfId) {
                null
            } else {
                DocumentsContract.buildDocumentUri(authority, id)
            }
        }
        if (viaSelf != null) return viaSelf
        // Neither form answered — unlistable.
        return null
    }

    /**
     * Queries [queryUri] for `(documentId, displayName, mimeType)` rows and
     * maps them to document URIs via [toUri]; rows that map to `null` are
     * skipped.  Returns the rows, or `null` when every retry timed out /
     * was rejected (the caller reports the folder as an error instead of
     * treating it as empty).
     */
    private suspend fun queryDocumentRows(
        queryUri: Uri,
        toUri: (String) -> Uri?
    ): List<Triple<Uri, String, String>>? {
        repeat(QUERY_RETRIES) {
            val rows = withListingTimeout(QUERY_TIMEOUT_MS) {
                queryDocumentRowsBlocking(queryUri, toUri)
            }
            if (rows != null) return rows
        }
        return null
    }

    private fun queryDocumentRowsBlocking(
        queryUri: Uri,
        toUri: (String) -> Uri?
    ): List<Triple<Uri, String, String>>? {
        val rows = mutableListOf<Triple<Uri, String, String>>()
        val resolver = context.contentResolver
        val projection = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE
        )
        return try {
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
            rows
        } catch (_: Throwable) {
            // The query FAILED — "unlistable", never "empty" (see
            // [queryTreeRowsBlocking]).
            null
        }
    }

    /**
     * True when [uri] is a directory document.  A file never has children,
     * so a non-empty children listing is the definitive test; folders
     * whose children cannot be listed fall back to the document's own MIME
     * type.  Tree URIs are always folders.
     */
    private suspend fun isFolderDocument(uri: Uri): Boolean {
        if (DocumentsContract.isTreeUri(uri)) return true
        if (queryDocumentChildren(uri)?.isNotEmpty() == true) return true
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
        // Bounded: a wedged DB read at import start must not freeze the
        // whole import before a single progress tick fires.  On timeout
        // the run continues with an empty id set (URI-string dedupe in
        // `trackDao.exists` still guards re-imports of the same flow).
        withTimeoutOrNull(DOCIDS_TIMEOUT_MS) {
            buildSet {
                trackDao.getAllTracksOnce().forEach { track ->
                    documentIdOf(track.uri)?.let { add(it) }
                }
            }
        } ?: emptySet()
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

    /**
     * Extracts track metadata with a hard timeout.  Metadata/artwork
     * extraction runs on a dedicated cached-thread pool (not the shared
     * IO dispatcher): `MediaMetadataRetriever.setDataSource` and some
     * provider queries can block **forever** on a corrupt file, and a
     * permanently blocked thread must not freeze a whole serial import.
     * When the timeout fires the stuck thread is abandoned (it may never
     * finish) and a filename-based fallback track is imported instead.
     */
    private suspend fun extractMetadata(uri: Uri): Track =
        withAbandonableTimeout(EXTRACTION_TIMEOUT_MS) { extractMetadataBlocking(uri) }
            ?: Track(
                uri = uri.toString(),
                title = "Unknown Track",
                artist = "Unknown Artist",
                addedAt = System.currentTimeMillis()
            )

    /**
     * Runs [block] on [executor] with a hard timeout and returns `null`
     * when it does not finish in time.  A blocked thread is abandoned (it
     * may never finish — `MediaMetadataRetriever` and some
     * DocumentsProvider queries can block forever on corrupt files), so
     * the import can never be frozen by a single file or query.  A block
     * failure resumes the caller with the exception immediately (never
     * burning the full timeout window on every retry).
     */
    private suspend fun <T> withExecutorTimeout(
        executor: ExecutorService,
        timeoutMs: Long,
        block: () -> T
    ): T? =
        withTimeoutOrNull(timeoutMs) {
            suspendCancellableCoroutine { continuation ->
                try {
                    executor.execute {
                        if (!continuation.isActive) return@execute
                        val result = try {
                            Result.success(block())
                        } catch (e: Throwable) {
                            Result.failure(e)
                        }
                        runCatching { continuation.resumeWith(result) }
                    }
                } catch (e: RejectedExecutionException) {
                    continuation.resume(null)
                }
            }
        }

    private suspend fun <T> withAbandonableTimeout(timeoutMs: Long, block: () -> T): T? =
        withExecutorTimeout(extractionExecutor, timeoutMs, block)

    private suspend fun <T> withListingTimeout(timeoutMs: Long, block: () -> T): T? =
        withExecutorTimeout(listingExecutor, timeoutMs, block)

    private fun extractMetadataBlocking(uri: Uri): Track {
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
        val retriever = android.media.MediaMetadataRetriever()
        return try {
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
        } finally {
            // release() must ALWAYS run: a leaked native retriever leaks
            // file descriptors and native memory — over a large import
            // that accumulates until the app is killed.
            runCatching { retriever.release() }
        }
    }

    fun getAllTracks() = trackDao.getAllTracks()

    fun searchTracks(query: String) = trackDao.searchTracks(query)

    suspend fun getTrack(uri: String) = trackDao.getTrack(uri)

    suspend fun deleteTrack(uri: String) = trackDao.deleteTrack(uri)

    /** Records global song popularity and, when applicable, playlist popularity. */
    suspend fun recordPlayback(uri: String, playlistId: Long? = null) {
        trackDao.incrementPlayCount(uri)
        playlistId?.let { playlistDao?.incrementPlayCount(it) }
    }

    /**
     * Desktop "Fetch missing album art for entire library"
     * (`views/settings.py` `_on_fetch_artwork`), adapted for Android:
     * every track whose cached artwork is missing (never extracted, or
     * the cache file was deleted) is re-scanned for **embedded** artwork
     * via [ArtworkStorage] against its persisted source (SAF grant /
     * local file).  Completed downloads use their stored video thumbnail URL
     * first, then the Deezer lookup fallback for tracks without a thumbnail.
     *
     * The extraction call is the same one used at import time
     * ([importAudioUri] → [ArtworkStorage.saveEmbeddedArtwork]), so any
     * source the library accepted at import can be re-extracted here.
     * If the file has no embedded art, a Deezer cover is downloaded as a
     * fallback, matching the desktop feature.  A stale or corrupt cache file
     * is treated as missing even when its old path still exists.
     *
     * [onProgress] receives `(scanned, total, updated)` after every track,
     * so the UI can render a live progress strip (a 4000-track library
     * takes a while).  [extract] is an injectable seam for tests.
     *
     * Returns the number of tracks whose artwork was (re)filled.
     */
    suspend fun fetchMissingArtwork(
        onProgress: (scanned: Int, total: Int, updated: Int) -> Unit = { _, _, _ -> },
        extract: (Uri) -> String? = { uri -> ArtworkStorage.saveEmbeddedArtwork(context, uri) }
    ): Int = withContext(Dispatchers.IO) {
        val tracks = trackDao.getAllTracksOnce()
        val thumbnailByTrackUri = downloadJobDao
            ?.getCompletedDownloadsOnce()
            ?.asSequence()
            ?.filter { it.thumbnailUrl.isNotBlank() && it.localUri.isNotBlank() }
            ?.associate { it.localUri to it.thumbnailUrl }
            .orEmpty()
        var updated = 0
        tracks.forEachIndexed { index, track ->
            val cached = track.albumArtPath
            val missing = !ArtworkStorage.hasUsableArtwork(cached)
            if (missing) {
                val embeddedPath = try {
                    extract(Uri.parse(track.uri))
                } catch (e: CancellationException) {
                    throw e
                } catch (_: Throwable) {
                    null
                }
                val thumbnailPath = thumbnailByTrackUri[track.uri]?.let { url ->
                    try {
                        thumbnailArtwork(url)
                    } catch (e: CancellationException) {
                        throw e
                    } catch (_: Throwable) {
                        null
                    }
                }
                val path = embeddedPath?.takeUnless { it.isBlank() }
                    ?: thumbnailPath?.takeUnless { it.isBlank() }
                    ?: try {
                        remoteArtwork(track)
                    } catch (e: CancellationException) {
                        throw e
                    } catch (_: Throwable) {
                        null
                    }
                if (!path.isNullOrBlank()) {
                    trackDao.updateTrack(track.copy(albumArtPath = path))
                    updated++
                }
            }
            onProgress(index + 1, tracks.size, updated)
        }
        updated
    }

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

    /**
     * Thread-safe counters shared by the parallel folder import workers:
     * atomic counts, a concurrent document-id set, and per-playlist
     * monotonically increasing sort orders (seeded from the DB once per
     * folder so parallel inserts never collide on `maxSortOrder + 1`).
     */
    private class ImportCounters {
        private val importedAtomic = AtomicInteger()
        private val duplicatesAtomic = AtomicInteger()
        private val errorsAtomic = AtomicInteger()

        var persistFailures = 0

        val imported: Int get() = importedAtomic.get()
        val duplicates: Int get() = duplicatesAtomic.get()
        val errors: Int get() = errorsAtomic.get()

        fun incrementImported() = importedAtomic.incrementAndGet()
        fun incrementDuplicates() = duplicatesAtomic.incrementAndGet()
        fun incrementErrors() = errorsAtomic.incrementAndGet()

        /** Document ids of all known tracks (preloaded + grown as the run imports). */
        val existingDocIds: MutableSet<String> = ConcurrentHashMap.newKeySet()

        private val nextSortOrders = ConcurrentHashMap<Long, AtomicInteger>()

        fun seedSortOrder(playlistId: Long?, currentMax: Int?) {
            if (playlistId == null) return
            nextSortOrders.putIfAbsent(playlistId, AtomicInteger((currentMax ?: 0) + 1))
        }

        fun nextSortOrder(playlistId: Long?): Int? {
            if (playlistId == null) return null
            return nextSortOrders[playlistId]?.getAndIncrement()
        }
    }

    companion object {
        /** Desktop convention: files at the music root belong to "Unsorted". */
        const val UNSORTED_PLAYLIST = "Unsorted"

        /** Bounded parallelism for folder imports (one bad file can't stall
         *  a whole 4k-song import, and the run is 3-4x faster). */
        private const val PARALLELISM = 4

        /** Hard cap per file for metadata/artwork extraction; on timeout the
         *  stuck thread is abandoned and a filename-based track is imported. */
        private const val EXTRACTION_TIMEOUT_MS = 20_000L

        /** Hard cap per DocumentsProvider query (folder listings); on timeout
         *  the folder is treated as empty rather than hanging the import. */
        private const val QUERY_TIMEOUT_MS = 15_000L

        /** Listing attempts per folder — a timeout/rejection means the
         *  provider was slow, NOT that the folder is empty; retrying
         *  recovers folders that answer slowly. */
        private const val QUERY_RETRIES = 3

        /** Short cap for the fallback folder-document query — it only
         *  exists for providers that answer it instantly, so a hang here
         *  must not compound into a stalled import. */
        private const val FALLBACK_QUERY_TIMEOUT_MS = 4_000L

        /** Cap per Room DAO call inside the import (cancellable suspends —
         *  a wedged SQLite/SD card or backed-up executor cannot freeze the
         *  import anymore). */
        private const val ROOM_TIMEOUT_MS = 10_000L

        /** Cap for one file's whole import pipeline (dedupe + insert +
         *  playlist membership) — a stuck file becomes an error, never a
         *  stall. */
        private const val FILE_WORK_TIMEOUT_MS = 30_000L

        /** Cap for the pre-run document-id preload. */
        private const val DOCIDS_TIMEOUT_MS = 30_000L

        /** Cap for persisting the URI grant (a blocking Binder call that
         *  must not freeze the import before the first progress tick). */
        private const val GRANT_TIMEOUT_MS = 15_000L

        /** Safety cap for the folder recursion (a provider that returns a
         *  folder as its own child must not recurse into a crash). */
        private const val MAX_FOLDER_DEPTH = 24

        /** Upper bound for [extractionExecutor]; abandoned threads (blocked
         *  on corrupt files) count against it and never return. */
        private const val MAX_EXTRACTION_THREADS = 32

        /** Fixed size of the folder-listing pool — listings are short
         *  (ms) and queued, so 2 threads never bottleneck a 4k-song
         *  import. */
        private const val MAX_LISTING_THREADS = 2

        /**
         * Dedicated cached-thread pool for blocking metadata extraction.
         * Threads that hang on corrupt files are abandoned (they may never
         * finish, so they are never returned to the pool) — the pool is
         * bounded so abandoned threads can never exhaust the device's
         * native thread budget and crash the app; once every thread is
         * wedged, further extraction calls fall back immediately.
         */
        private val extractionExecutor: ExecutorService = ThreadPoolExecutor(
            0,
            MAX_EXTRACTION_THREADS,
            60L,
            TimeUnit.SECONDS,
            SynchronousQueue(),
            { runnable -> Thread(runnable, "track-extraction").apply { isDaemon = true } }
        )

        /**
         * Dedicated fixed pool for DocumentsProvider folder-listing
         * queries, kept SEPARATE from [extractionExecutor] on purpose.
         * Listings used to share the extraction pool: once the 32
         * extraction threads were busy (or wedged) on files, every
         * listing submitted via the `SynchronousQueue` was REJECTED —
         * a whole folder import starved out ~25 of 59 folders and each
         * one reported an error (previously they were silently skipped
         * as "empty").  This pool is fixed-size with an unbounded queue,
         * so listings are never rejected and never compete with file
         * work; the abandonable timeout still protects against a
         * provider that hangs on a listing.
         */
        private val listingExecutor: ExecutorService = ThreadPoolExecutor(
            MAX_LISTING_THREADS,
            MAX_LISTING_THREADS,
            60L,
            TimeUnit.SECONDS,
            LinkedBlockingQueue(),
            { runnable -> Thread(runnable, "folder-listing").apply { isDaemon = true } }
        )

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
