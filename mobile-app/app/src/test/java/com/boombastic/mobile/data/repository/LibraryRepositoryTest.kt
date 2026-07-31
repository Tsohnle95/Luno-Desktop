package com.boombastic.mobile.data.repository

import android.content.ContentProvider
import android.content.ContentValues
import android.content.Context
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.provider.DocumentsContract
import androidx.room.Room
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.boombastic.mobile.data.db.AppDatabase
import com.google.common.truth.Truth.assertThat
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.annotation.Config
import java.io.File
import java.io.FileNotFoundException

/**
 * Regression tests for SAF import recursion.
 *
 * The old implementation built child document URIs from a child document
 * URI, producing nested `tree/.../document/.../document/...` URIs (and
 * queried document URIs expecting children).  These tests pin the correct
 * behavior: tree children are enumerated via `.../children` and every
 * descendant URI is built from the original tree root, so subfolder
 * contents are imported into playlists named after their folders.
 */
@RunWith(AndroidJUnit4::class)
@Config(manifest = Config.NONE, sdk = [34])
class LibraryRepositoryTest {

    private lateinit var database: AppDatabase
    private lateinit var repository: LibraryRepository

    @Before
    fun setUp() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        database = Room.inMemoryDatabaseBuilder(context, AppDatabase::class.java).build()
        repository = LibraryRepository(
            context = context,
            trackDao = database.trackDao(),
            playlistDao = database.playlistDao(),
            uriPermissionPersister = LibraryRepository.UriPermissionPersister { }
        )
        FakeDocumentsProvider.samsungStyle = false
        FakeDocumentsProvider.withEmptyFolder = false
        FakeDocumentsProvider.selfAsChild = false
        Robolectric.setupContentProvider(
            FakeDocumentsProvider::class.java,
            FakeDocumentsProvider.AUTHORITY
        )
    }

    @After
    fun tearDown() {
        database.close()
    }

    private fun treeRootUri(): Uri =
        Uri.parse("content://${FakeDocumentsProvider.AUTHORITY}/tree/root")

    private suspend fun playlistNames(): List<String> =
        database.playlistDao().getAllPlaylists().first().map { it.name }

    private suspend fun trackTitles(): List<String> =
        database.trackDao().getAllTracks().first().map { it.title }

    @Test
    fun importLibraryTree_importsNestedFolderContentsIntoPlaylists() = runBlocking<Unit> {
        val result = repository.importLibraryTree(treeRootUri())

        assertThat(result.imported).isEqualTo(5)
        assertThat(result.duplicates).isEqualTo(0)
        assertThat(result.errors).isEqualTo(0)

        assertThat(trackTitles()).containsExactly(
            "album1", "song1", "song2", "deep1", "only"
        )

        // Root file lands in "Unsorted"; each folder becomes a playlist.
        assertThat(playlistNames()).containsExactly(
            "Unsorted", "Folder A", "Folder B", "Sub"
        )
        val unsorted = database.playlistDao().getPlaylistByName("Unsorted")!!
        val folderA = database.playlistDao().getPlaylistByName("Folder A")!!
        val folderB = database.playlistDao().getPlaylistByName("Folder B")!!
        val sub = database.playlistDao().getPlaylistByName("Sub")!!
        assertThat(database.playlistDao().trackCount(unsorted.id)).isEqualTo(1)
        assertThat(database.playlistDao().trackCount(folderA.id)).isEqualTo(2)
        assertThat(database.playlistDao().trackCount(folderB.id)).isEqualTo(1)
        assertThat(database.playlistDao().trackCount(sub.id)).isEqualTo(1)
    }

    @Test
    fun importLibraryTree_secondRunReportsAllDuplicates() = runBlocking<Unit> {
        repository.importLibraryTree(treeRootUri())
        val result = repository.importLibraryTree(treeRootUri())

        assertThat(result.imported).isEqualTo(0)
        assertThat(result.duplicates).isEqualTo(5)
        assertThat(result.errors).isEqualTo(0)
    }

    @Test
    fun pickerFileThenTreeImport_dedupesByDocumentId() = runBlocking<Unit> {
        // The same physical file has different URI strings per flow:
        // `…/document/<id>` (picker) vs `…/tree/<id>/document/<id>` (tree).
        // A picker import followed by a folder import must NOT create a
        // second Track row — the folder run counts it as a duplicate.
        val fileUri = Uri.parse(
            "content://${FakeDocumentsProvider.AUTHORITY}/document/root%2Falbum1.mp3"
        )
        repository.importMultipleUris(listOf(fileUri))

        val result = repository.importLibraryTree(treeRootUri())

        assertThat(result.imported).isEqualTo(4)
        assertThat(result.duplicates).isEqualTo(1)
        assertThat(result.errors).isEqualTo(0)
        assertThat(trackTitles()).containsExactly(
            "album1", "song1", "song2", "deep1", "only"
        )
        // album1 was already a track, so it is not re-added to "Unsorted".
        val unsorted = database.playlistDao().getPlaylistByName("Unsorted")!!
        assertThat(database.playlistDao().trackCount(unsorted.id)).isEqualTo(0)
    }

    @Test
    fun treeImportThenPickerFile_dedupesByDocumentId() = runBlocking<Unit> {
        // Reverse direction: folder import first, then the same file picked
        // directly — the picker run must report it as a duplicate.
        repository.importLibraryTree(treeRootUri())

        val fileUri = Uri.parse(
            "content://${FakeDocumentsProvider.AUTHORITY}/document/root%2Falbum1.mp3"
        )
        val result = repository.importMultipleUris(listOf(fileUri))

        assertThat(result.imported).isEqualTo(0)
        assertThat(result.duplicates).isEqualTo(1)
        assertThat(result.errors).isEqualTo(0)
        assertThat(trackTitles()).containsExactly(
            "album1", "song1", "song2", "deep1", "only"
        )
    }

    @Test
    fun importLibraryTree_samsungStyleTree_importsViaDocQueryFallbacks() = runBlocking<Unit> {
        // Samsung-style provider: `.../children` tree queries are
        // unsupported; children come from querying the folder document
        // (the bare tree URI for the root).  The whole tree — root songs
        // plus every nested playlist folder — must still be imported.
        FakeDocumentsProvider.samsungStyle = true
        val result = repository.importLibraryTree(treeRootUri())

        assertThat(result.imported).isEqualTo(5)
        assertThat(result.duplicates).isEqualTo(0)
        assertThat(result.errors).isEqualTo(0)

        assertThat(trackTitles()).containsExactly(
            "album1", "song1", "song2", "deep1", "only"
        )
        assertThat(playlistNames()).containsExactly(
            "Unsorted", "Folder A", "Folder B", "Sub"
        )
    }

    @Test
    fun importLibraryTree_emptySubfolder_doesNotRecurseForever() = runBlocking<Unit> {
        // AOSP-style provider behavior: an EMPTY subfolder's `.../children`
        // query returns an empty cursor, so queryTreeChildren falls back to
        // a folder-document query — which returns the folder's OWN row.
        // Without the self-row guard the import would recurse into the same
        // folder forever and crash the app with a StackOverflowError (the
        // "Luno closed" bug).  The import must terminate instead.
        FakeDocumentsProvider.withEmptyFolder = true
        val result = withTimeout(10_000) { repository.importLibraryTree(treeRootUri()) }

        assertThat(result.imported).isEqualTo(5)
        assertThat(result.duplicates).isEqualTo(0)
        assertThat(result.errors).isEqualTo(0)
        assertThat(trackTitles()).containsExactly(
            "album1", "song1", "song2", "deep1", "only"
        )
        // The empty folder still becomes a (memberless) playlist.
        assertThat(playlistNames()).containsExactly(
            "Unsorted", "Folder A", "Folder B", "Sub", "Empty"
        )
    }

    @Test
    fun importLibraryTree_providerReturnsFolderAsOwnChild_terminates() = runBlocking<Unit> {
        // A misbehaving provider lists a folder as its own child; the
        // import must skip self-rows instead of recursing until the stack
        // blows up.  (The tree root recurses once — its self-row has a
        // different id than the `""` root — then every level is filtered.)
        // Counters may be inflated by the concurrent double-pass, but the
        // DB rows dedupe by URI and the import must terminate.
        FakeDocumentsProvider.selfAsChild = true
        val result = withTimeout(10_000) { repository.importLibraryTree(treeRootUri()) }

        assertThat(result.errors).isEqualTo(0)
        assertThat(trackTitles()).containsExactly(
            "album1", "song1", "song2", "deep1", "only"
        )
    }

    @Test
    fun importMultipleUris_folderSelected_importsContentsAsPlaylist() = runBlocking<Unit> {
        val folderUri = Uri.parse(
            "content://${FakeDocumentsProvider.AUTHORITY}/document/root%2FFolder%20A"
        )
        val result = repository.importMultipleUris(listOf(folderUri))

        assertThat(result.imported).isEqualTo(3)
        assertThat(result.duplicates).isEqualTo(0)
        assertThat(result.errors).isEqualTo(0)

        assertThat(trackTitles()).containsExactly("song1", "song2", "deep1")
        assertThat(playlistNames()).containsExactly("Folder A", "Sub")
    }

    @Test
    fun importMultipleUris_filesImportWithoutPlaylist() = runBlocking<Unit> {
        val fileUri = Uri.parse(
            "content://${FakeDocumentsProvider.AUTHORITY}/document/root%2Falbum1.mp3"
        )
        val result = repository.importMultipleUris(listOf(fileUri))

        assertThat(result.imported).isEqualTo(1)
        assertThat(result.errors).isEqualTo(0)
        assertThat(trackTitles()).containsExactly("album1")
        assertThat(playlistNames()).isEmpty()
    }

    @Test
    fun importMultipleUris_nonAudioDocumentIsAnErrorNotATrack() = runBlocking<Unit> {
        // The picker launches with `*/*` (so playlist folders can be
        // selected), which surfaces non-audio files too — they must be
        // rejected, never imported as bogus tracks.
        val txtUri = Uri.parse(
            "content://${FakeDocumentsProvider.AUTHORITY}/document/root%2Fnotes.txt"
        )
        val result = repository.importMultipleUris(listOf(txtUri))

        assertThat(result.imported).isEqualTo(0)
        assertThat(result.duplicates).isEqualTo(0)
        assertThat(result.errors).isEqualTo(1)
        assertThat(trackTitles()).isEmpty()
    }

    @Test
    fun importMultipleUris_samsungStyleFolder_importsContentsAsPlaylist() = runBlocking<Unit> {
        // Samsung My Files: querying a folder document returns its
        // children and `document/<id>/children` is unsupported — the
        // folder must still be recognized and imported recursively.
        FakeDocumentsProvider.samsungStyle = true
        val folderUri = Uri.parse(
            "content://${FakeDocumentsProvider.AUTHORITY}/document/root%2FFolder%20A"
        )
        val result = repository.importMultipleUris(listOf(folderUri))

        assertThat(result.imported).isEqualTo(3)
        assertThat(result.errors).isEqualTo(0)
        assertThat(trackTitles()).containsExactly("song1", "song2", "deep1")
        assertThat(playlistNames()).containsExactly("Folder A", "Sub")
    }

    @Test
    fun importMultipleUris_treeUri_importsRootContentsAsPlaylist() = runBlocking<Unit> {
        val result = repository.importMultipleUris(listOf(treeRootUri()))

        assertThat(result.imported).isEqualTo(5)
        assertThat(result.errors).isEqualTo(0)
        assertThat(playlistNames()).containsExactly("root", "Folder A", "Folder B", "Sub")
    }

    /**
     * Minimal in-memory SAF provider emulating an external-storage
     * DocumentsProvider: querying a document URI returns the document
     * itself; `.../children` returns its children; the bare tree URI
     * returns the tree root's children.
     */
    class FakeDocumentsProvider : ContentProvider() {

        private data class FakeDoc(
            val id: String,
            val name: String,
            val mime: String,
            val children: List<String> = emptyList()
        )

        private val docs: Map<String, FakeDoc> = listOf(
            FakeDoc(ROOT_ID, "Music", DIR, listOf("root/album1.mp3", "root/Folder A", "root/Folder B")),
            FakeDoc("root/album1.mp3", "album1.mp3", AUDIO),
            FakeDoc("root/Folder A", "Folder A", DIR, listOf("root/Folder A/song1.mp3", "root/Folder A/song2.mp3", "root/Folder A/Sub")),
            FakeDoc("root/Folder A/song1.mp3", "song1.mp3", AUDIO),
            FakeDoc("root/Folder A/song2.mp3", "song2.mp3", AUDIO),
            FakeDoc("root/Folder A/Sub", "Sub", DIR, listOf("root/Folder A/Sub/deep1.flac")),
            FakeDoc("root/Folder A/Sub/deep1.flac", "deep1.flac", AUDIO),
            FakeDoc("root/Folder B", "Folder B", DIR, listOf("root/Folder B/only.mp3")),
            FakeDoc("root/Folder B/only.mp3", "only.mp3", AUDIO),
            FakeDoc("root/Empty", "Empty", DIR)
        ).associateBy { it.id }

        override fun onCreate(): Boolean = true

        override fun query(
            uri: Uri,
            projection: Array<out String>?,
            selection: String?,
            selectionArgs: Array<out String>?,
            sortOrder: String?
        ): Cursor {
            val segments = uri.pathSegments ?: return emptyCursor(projection)
            return when {
                // Bare tree URI → the tree root document itself (AOSP
                // behavior: the bare tree URI never lists children).
                // Samsung-style: querying the folder document returns its
                // children, so the bare tree URI lists the root's children.
                segments.size == 2 && segments[0] == "tree" ->
                    if (samsungStyle) childrenCursor(ROOT_ID, projection)
                    else documentCursor(ROOT_ID, projection)
                // tree/<treeId>/document/<docId> → the document itself
                // (AOSP), or the folder's children Samsung-style (files
                // return their own row even Samsung-style).
                segments.size == 4 && segments[0] == "tree" && segments[2] == "document" ->
                    if (samsungStyle && docs[segments[3]]?.mime == DIR) {
                        childrenCursor(segments[3], projection)
                    } else {
                        documentCursor(segments[3], projection)
                    }
                // tree/<treeId>/document/<docId>/children → the folder's
                // children (unsupported Samsung-style).
                segments.size == 5 && segments[0] == "tree" &&
                    segments[2] == "document" && segments[4] == "children" ->
                    if (samsungStyle) emptyCursor(projection)
                    else childrenCursor(segments[3], projection)
                // document/<docId> → the document itself (or its children,
                // Samsung-style, when the document is a folder).
                segments.size == 2 && segments[0] == "document" ->
                    if (samsungStyle && docs[segments[1]]?.mime == DIR) {
                        childrenCursor(segments[1], projection)
                    } else {
                        documentCursor(segments[1], projection)
                    }
                // document/<docId>/children → the folder's children
                // (unsupported Samsung-style).
                segments.size == 3 && segments[0] == "document" && segments[2] == "children" ->
                    if (samsungStyle) emptyCursor(projection) else childrenCursor(segments[1], projection)
                else -> emptyCursor(projection)
            }
        }

        override fun getType(uri: Uri): String = docs[documentIdOf(uri)]?.mime ?: "application/octet-stream"

        override fun insert(uri: Uri, values: ContentValues?): Uri =
            throw UnsupportedOperationException()

        override fun delete(uri: Uri, selection: String?, selectionArgs: Array<out String>?): Int =
            throw UnsupportedOperationException()

        override fun update(
            uri: Uri,
            values: ContentValues?,
            selection: String?,
            selectionArgs: Array<out String>?
        ): Int = throw UnsupportedOperationException()

        override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
            val file = File(System.getProperty("java.io.tmpdir"), "fake-audio.bin")
            file.writeBytes(ByteArray(4))
            return ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)
        }

        private fun documentIdOf(uri: Uri): String {
            val segments = uri.pathSegments ?: return ""
            return if (segments.size >= 4 && segments[0] == "tree") {
                segments[3]
            } else {
                segments.getOrNull(1) ?: ""
            }
        }

        private fun documentCursor(documentId: String, projection: Array<out String>?): Cursor {
            val doc = docs[documentId] ?: return emptyCursor(projection)
            return matrixCursor(
                projection,
                listOf(rowOf(doc.id, doc.name, doc.mime))
            )
        }

        private fun childrenCursor(documentId: String, projection: Array<out String>?): Cursor {
            val parent = docs[documentId] ?: return emptyCursor(projection)
            val children = parent.children.toMutableList()
            if (withEmptyFolder && documentId == ROOT_ID) children.add(EMPTY_ID)
            if (selfAsChild) children.add(0, documentId)
            return matrixCursor(
                projection,
                children.mapNotNull { childId ->
                    docs[childId]?.let { rowOf(it.id, it.name, it.mime) }
                }
            )
        }

        private fun rowOf(id: String, name: String, mime: String): Map<String, Any?> = mapOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID to id,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME to name,
            DocumentsContract.Document.COLUMN_MIME_TYPE to mime,
            DocumentsContract.Document.COLUMN_SIZE to 0L
        )

        private fun matrixCursor(
            projection: Array<out String>?,
            rows: List<Map<String, Any?>>
        ): Cursor {
            val columns = projection ?: arrayOf(
                DocumentsContract.Document.COLUMN_DOCUMENT_ID,
                DocumentsContract.Document.COLUMN_DISPLAY_NAME,
                DocumentsContract.Document.COLUMN_MIME_TYPE,
                DocumentsContract.Document.COLUMN_SIZE
            )
            val cursor = MatrixCursor(columns)
            for (row in rows) {
                cursor.addRow(columns.map { row[it] })
            }
            return cursor
        }

        private fun emptyCursor(projection: Array<out String>?): Cursor =
            MatrixCursor(projection ?: arrayOf(DocumentsContract.Document.COLUMN_DOCUMENT_ID))

        companion object {
            const val AUTHORITY = "com.boombastic.test.externalstorage.documents"
            const val ROOT_ID = "root"
            private const val EMPTY_ID = "root/Empty"
            private const val DIR = DocumentsContract.Document.MIME_TYPE_DIR
            private const val AUDIO = "audio/mpeg"

            /** Samsung-style provider: folder doc queries return children. */
            var samsungStyle = false

            /** Also list an empty subfolder at the root (fallback-query
             *  self-row recursion regression). */
            var withEmptyFolder = false

            /** Each folder lists itself as its own first child (self-row
             *  guard regression). */
            var selfAsChild = false
        }
    }
}
