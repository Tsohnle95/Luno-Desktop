package com.luno.mobile.data.db

import com.luno.mobile.data.db.entity.Track
import com.google.common.truth.Truth.assertThat
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Test

class TrackDaoTest : AppDatabaseTest() {

    @Test
    fun insertAndRetrieveTrack() = runBlocking {
        val track = Track(
            uri = "content://test/1",
            title = "Test Song",
            artist = "Test Artist",
            album = "Test Album",
            durationMs = 300000L
        )
        trackDao.insertTrack(track)

        val retrieved = trackDao.getTrack("content://test/1")
        assertThat(retrieved).isNotNull()
        assertThat(retrieved!!.title).isEqualTo("Test Song")
        assertThat(retrieved.artist).isEqualTo("Test Artist")
    }

    @Test
    fun getTracksFlow_returnsInsertedTracks() = runBlocking {
        val track1 = Track(
            uri = "content://test/1",
            title = "Alpha",
            artist = "Artist A"
        )
        val track2 = Track(
            uri = "content://test/2",
            title = "Beta",
            artist = "Artist B"
        )
        trackDao.insertTracks(listOf(track1, track2))

        val tracks = trackDao.getAllTracks().first()
        assertThat(tracks).hasSize(2)
    }

    @Test
    fun searchTracks_returnsMatchingResults() = runBlocking {
        val track1 = Track(
            uri = "content://test/1",
            title = "Hello World",
            artist = "Artist A"
        )
        val track2 = Track(
            uri = "content://test/2",
            title = "Goodbye World",
            artist = "Artist B"
        )
        trackDao.insertTracks(listOf(track1, track2))

        val results = trackDao.searchTracks("Hello").first()
        assertThat(results).hasSize(1)
        assertThat(results[0].title).isEqualTo("Hello World")
    }

    @Test
    fun searchTracks_byArtist_returnsResults() = runBlocking {
        val track1 = Track(
            uri = "content://test/1",
            title = "Song One",
            artist = "Special Artist"
        )
        val track2 = Track(
            uri = "content://test/2",
            title = "Song Two",
            artist = "Other Artist"
        )
        trackDao.insertTracks(listOf(track1, track2))

        val results = trackDao.searchTracks("Special").first()
        assertThat(results).hasSize(1)
        assertThat(results[0].artist).isEqualTo("Special Artist")
    }

    @Test
    fun searchTracks_emptyQuery_returnsAll() = runBlocking {
        val track1 = Track(uri = "content://test/1", title = "Song A")
        val track2 = Track(uri = "content://test/2", title = "Song B")
        trackDao.insertTracks(listOf(track1, track2))

        val results = trackDao.searchTracks("").first()
        assertThat(results).hasSize(2)
    }

    @Test
    fun searchTracks_noMatch_returnsEmpty() = runBlocking {
        val track1 = Track(uri = "content://test/1", title = "Song A")
        trackDao.insertTrack(track1)

        val results = trackDao.searchTracks("NonExistent").first()
        assertThat(results).isEmpty()
    }

    @Test
    fun deduplicate_skipsExistingUri() = runBlocking {
        val track = Track(
            uri = "content://test/unique",
            title = "Original"
        )
        trackDao.insertTrack(track)

        val exists = trackDao.exists("content://test/unique")
        assertThat(exists).isTrue()

        val notExists = trackDao.exists("content://test/nonexistent")
        assertThat(notExists).isFalse()
    }

    @Test
    fun deleteTrack_removesFromDatabase() = runBlocking {
        val track = Track(uri = "content://test/delete", title = "Delete Me")
        trackDao.insertTrack(track)
        assertThat(trackDao.exists("content://test/delete")).isTrue()

        trackDao.deleteTrack("content://test/delete")
        assertThat(trackDao.exists("content://test/delete")).isFalse()
    }

    @Test
    fun incrementPlayCount_recordsLocalPopularity() = runBlocking {
        trackDao.insertTrack(Track(uri = "content://test/popular", title = "Popular"))

        trackDao.incrementPlayCount("content://test/popular")
        trackDao.incrementPlayCount("content://test/popular")

        assertThat(trackDao.getTrack("content://test/popular")!!.playCount).isEqualTo(2)
    }

    @Test
    fun setFavorite_persistsFavoriteFlag() = runBlocking {
        trackDao.insertTrack(Track(uri = "content://test/favorite", title = "Favorite"))

        trackDao.setFavorite("content://test/favorite", true)
        assertThat(trackDao.getTrack("content://test/favorite")!!.isFavorite).isTrue()

        trackDao.setFavorite("content://test/favorite", false)
        assertThat(trackDao.getTrack("content://test/favorite")!!.isFavorite).isFalse()
    }

    @Test
    fun trackCount_returnsCorrectCount() = runBlocking {
        assertThat(trackDao.trackCount()).isEqualTo(0)

        trackDao.insertTrack(Track(uri = "content://test/1", title = "T1"))
        assertThat(trackDao.trackCount()).isEqualTo(1)

        trackDao.insertTrack(Track(uri = "content://test/2", title = "T2"))
        assertThat(trackDao.trackCount()).isEqualTo(2)
    }

    @Test
    fun albumArtPath_roundTripsAndDefaultsToNull() = runBlocking {
        val withArt = Track(
            uri = "content://test/art",
            title = "With Art",
            albumArtPath = "/data/user/0/com.luno.mobile/files/artwork/abc.jpg"
        )
        trackDao.insertTrack(withArt)

        val retrieved = trackDao.getTrack("content://test/art")
        assertThat(retrieved!!.albumArtPath)
            .isEqualTo("/data/user/0/com.luno.mobile/files/artwork/abc.jpg")
        assertThat(retrieved.albumArtUri())
            .isEqualTo("file:///data/user/0/com.luno.mobile/files/artwork/abc.jpg")

        trackDao.insertTrack(Track(uri = "content://test/noart", title = "No Art"))
        val plain = trackDao.getTrack("content://test/noart")
        assertThat(plain!!.albumArtPath).isNull()
        assertThat(plain.albumArtUri()).isNull()
    }

    @Test
    fun updateTrack_persistsAlbumArtPathForFetchArtwork() = runBlocking {
        trackDao.insertTrack(Track(uri = "content://test/fetchart", title = "Fetch Art"))

        trackDao.updateTrack(
            trackDao.getTrack("content://test/fetchart")!!.copy(
                albumArtPath = "/data/user/0/com.luno.mobile/files/artwork/fetched.jpg"
            )
        )

        val updated = trackDao.getTrack("content://test/fetchart")
        assertThat(updated!!.albumArtPath)
            .isEqualTo("/data/user/0/com.luno.mobile/files/artwork/fetched.jpg")
        // Update never loses the other columns.
        assertThat(updated.title).isEqualTo("Fetch Art")
        assertThat(updated.uri).isEqualTo("content://test/fetchart")
    }
}
