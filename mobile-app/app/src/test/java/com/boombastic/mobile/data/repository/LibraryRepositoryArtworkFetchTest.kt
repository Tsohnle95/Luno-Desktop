package com.boombastic.mobile.data.repository

import android.content.Context
import android.net.Uri
import androidx.room.Room
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.boombastic.mobile.data.db.AppDatabase
import com.boombastic.mobile.data.db.entity.Track
import com.google.common.truth.Truth.assertThat
import java.io.File
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/**
 * Pins the "Fetch missing artwork" sweep (desktop settings parity):
 * missing-artwork detection (null / blank / deleted cache file), refill
 * via the extractor (the production extractor is the same
 * MediaMetadataRetriever path proven at import time), persistence through
 * the DAO, and progress reporting — so a sweep over a 4000-track library
 * is verifiable without a device.
 */
@RunWith(AndroidJUnit4::class)
@Config(manifest = Config.NONE, sdk = [34])
class LibraryRepositoryArtworkFetchTest {

    private lateinit var context: Context
    private lateinit var database: AppDatabase
    private lateinit var repository: LibraryRepository

    @Before
    fun setUp() {
        context = ApplicationProvider.getApplicationContext()
        database = Room.inMemoryDatabaseBuilder(context, AppDatabase::class.java).build()
        repository = LibraryRepository(
            context = context,
            trackDao = database.trackDao(),
            playlistDao = database.playlistDao(),
            uriPermissionPersister = LibraryRepository.UriPermissionPersister { }
        )
    }

    @After
    fun tearDown() {
        database.close()
    }

    private suspend fun insertTrack(uri: String, albumArtPath: String? = null) {
        database.trackDao().insertTrack(
            Track(uri = uri, title = "Title", artist = "Artist", albumArtPath = albumArtPath)
        )
    }

    /** Extractor that hands every URI a deterministic path (no real decode). */
    private fun fakeExtractor(): Pair<(Uri) -> String?, MutableList<Uri>> {
        val called = mutableListOf<Uri>()
        val extract: (Uri) -> String? = { uri ->
            called += uri
            "/artwork/${uri.lastPathSegment}.jpg"
        }
        return extract to called
    }

    @Test
    fun missingArtwork_isExtractedAndPersisted() = runBlocking {
        insertTrack("content://media/external/audio/1")
        val (extract, _) = fakeExtractor()

        val updated = repository.fetchMissingArtwork(extract = extract)

        assertThat(updated).isEqualTo(1)
        val stored = database.trackDao().getTrack("content://media/external/audio/1")!!
        assertThat(stored.albumArtPath).isEqualTo("/artwork/1.jpg")
    }

    @Test
    fun existingValidArtwork_isNotRescanned() = runBlocking {
        val existing = File(context.filesDir, "artwork/existing.jpg").apply {
            parentFile?.mkdirs()
            writeBytes(byteArrayOf(1, 2, 3))
        }
        insertTrack("content://media/external/audio/2", albumArtPath = existing.absolutePath)
        val (extract, called) = fakeExtractor()

        val updated = repository.fetchMissingArtwork(extract = extract)

        assertThat(updated).isEqualTo(0)
        assertThat(called).isEmpty()
    }

    @Test
    fun deletedCacheFile_isRefilled() = runBlocking {
        // Path recorded but the cache file is gone (cache cleared on disk).
        insertTrack("content://media/external/audio/3", albumArtPath = "/nonexistent/art/3.jpg")
        val (extract, _) = fakeExtractor()

        val updated = repository.fetchMissingArtwork(extract = extract)

        assertThat(updated).isEqualTo(1)
        assertThat(database.trackDao().getTrack("content://media/external/audio/3")!!.albumArtPath)
            .isEqualTo("/artwork/3.jpg")
    }

    @Test
    fun blankArtworkPath_isTreatedAsMissing() = runBlocking {
        insertTrack("content://media/external/audio/4", albumArtPath = "   ")
        val (extract, _) = fakeExtractor()

        val updated = repository.fetchMissingArtwork(extract = extract)

        assertThat(updated).isEqualTo(1)
    }

    @Test
    fun failedExtraction_isSkipped() = runBlocking {
        insertTrack("content://media/external/audio/5")
        val updated = repository.fetchMissingArtwork(extract = { null })

        assertThat(updated).isEqualTo(0)
        assertThat(database.trackDao().getTrack("content://media/external/audio/5")!!.albumArtPath)
            .isNull()
    }

    @Test
    fun fileUris_arePassedToTheExtractorAsParsed() {
        runBlocking {
            insertTrack("file:///data/app/files/downloads/song.m4a")
            val (extract, called) = fakeExtractor()

            repository.fetchMissingArtwork(extract = extract)

            assertThat(called).containsExactly(Uri.parse("file:///data/app/files/downloads/song.m4a"))
        }
    }

    @Test
    fun progress_reportsScannedTotalAndUpdatedPerTrack() = runBlocking {
        insertTrack("content://media/external/audio/1")
        insertTrack("content://media/external/audio/2")
        val existing = File(context.filesDir, "artwork/existing2.jpg").apply {
            parentFile?.mkdirs()
            writeBytes(byteArrayOf(1, 2, 3))
        }
        insertTrack("content://media/external/audio/3", albumArtPath = existing.absolutePath)
        val (extract, _) = fakeExtractor()
        val progress = mutableListOf<Triple<Int, Int, Int>>()

        repository.fetchMissingArtwork(onProgress = { scanned, total, updated ->
            progress += Triple(scanned, total, updated)
        }, extract = extract)

        assertThat(progress).containsExactly(
            Triple(1, 3, 1), // track 1 missing → filled
            Triple(2, 3, 2), // track 2 missing → filled
            Triple(3, 3, 2)  // track 3 has valid artwork → skipped
        ).inOrder()
    }

    @Test
    fun emptyLibrary_finishesWithoutProgress() = runBlocking {
        val progress = mutableListOf<Triple<Int, Int, Int>>()
        val updated = repository.fetchMissingArtwork(onProgress = { s, t, u ->
            progress += Triple(s, t, u)
        }, extract = { null })

        assertThat(updated).isEqualTo(0)
        assertThat(progress).isEmpty()
    }
}
