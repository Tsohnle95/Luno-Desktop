package com.boombastic.mobile.data.db

import com.boombastic.mobile.data.db.entity.Playlist
import com.boombastic.mobile.data.db.entity.PlaylistTrack
import com.boombastic.mobile.data.db.entity.Track
import com.google.common.truth.Truth.assertThat
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Test

class PlaylistDaoTest : AppDatabaseTest() {

    @Test
    fun createAndRetrievePlaylist() = runBlocking {
        val playlist = Playlist(name = "My Favorites", description = "My favorite tracks")
        val id = playlistDao.insertPlaylist(playlist)

        val retrieved = playlistDao.getPlaylist(id)
        assertThat(retrieved).isNotNull()
        assertThat(retrieved!!.name).isEqualTo("My Favorites")
        assertThat(retrieved.description).isEqualTo("My favorite tracks")
    }

    @Test
    fun getPlaylistsFlow_returnsAll() = runBlocking {
        playlistDao.insertPlaylist(Playlist(name = "Playlist A"))
        playlistDao.insertPlaylist(Playlist(name = "Playlist B"))

        val playlists = playlistDao.getAllPlaylists().first()
        assertThat(playlists).hasSize(2)
    }

    @Test
    fun deletePlaylist_removesIt() = runBlocking {
        val id = playlistDao.insertPlaylist(Playlist(name = "Delete Me"))
        assertThat(playlistDao.getPlaylist(id)).isNotNull()

        playlistDao.deletePlaylistById(id)
        assertThat(playlistDao.getPlaylist(id)).isNull()
    }

    @Test
    fun addTrackToPlaylist_increasesTrackCount() = runBlocking {
        val track = Track(uri = "content://test/1", title = "Song")
        trackDao.insertTrack(track)

        val playlistId = playlistDao.insertPlaylist(Playlist(name = "Test"))
        playlistDao.addTrackToPlaylist(
            PlaylistTrack(playlistId = playlistId, trackUri = "content://test/1", sortOrder = 0)
        )

        val count = playlistDao.trackCount(playlistId)
        assertThat(count).isEqualTo(1)
    }

    @Test
    fun removeTrackFromPlaylist_decreasesCount() = runBlocking {
        val track = Track(uri = "content://test/1", title = "Song")
        trackDao.insertTrack(track)

        val playlistId = playlistDao.insertPlaylist(Playlist(name = "Test"))
        playlistDao.addTrackToPlaylist(
            PlaylistTrack(playlistId = playlistId, trackUri = "content://test/1", sortOrder = 0)
        )
        assertThat(playlistDao.trackCount(playlistId)).isEqualTo(1)

        playlistDao.removeTrackFromPlaylist(playlistId, "content://test/1")
        assertThat(playlistDao.trackCount(playlistId)).isEqualTo(0)
    }

    @Test
    fun getPlaylistWithTracks_includesTracks() = runBlocking {
        val track = Track(uri = "content://test/1", title = "Song", artist = "Artist")
        trackDao.insertTrack(track)

        val playlistId = playlistDao.insertPlaylist(Playlist(name = "Test"))
        playlistDao.addTrackToPlaylist(
            PlaylistTrack(playlistId = playlistId, trackUri = "content://test/1", sortOrder = 0)
        )

        val result = playlistDao.getPlaylistWithTracks(playlistId)
        assertThat(result).isNotNull()
        assertThat(result!!.playlist.name).isEqualTo("Test")
        assertThat(result.tracks).hasSize(1)
        assertThat(result.tracks[0].title).isEqualTo("Song")
    }

    @Test
    fun maxSortOrder_returnsHighestOrder() = runBlocking {
        val track1 = Track(uri = "content://test/1", title = "Song 1")
        val track2 = Track(uri = "content://test/2", title = "Song 2")
        trackDao.insertTracks(listOf(track1, track2))

        val playlistId = playlistDao.insertPlaylist(Playlist(name = "Test"))
        playlistDao.addTrackToPlaylist(
            PlaylistTrack(playlistId = playlistId, trackUri = "content://test/1", sortOrder = 0)
        )
        playlistDao.addTrackToPlaylist(
            PlaylistTrack(playlistId = playlistId, trackUri = "content://test/2", sortOrder = 5)
        )

        val maxOrder = playlistDao.maxSortOrder(playlistId)
        assertThat(maxOrder).isEqualTo(5)
    }

    @Test
    fun playlistTrackCascade_removesTracksOnPlaylistDelete() = runBlocking {
        val track = Track(uri = "content://test/1", title = "Song")
        trackDao.insertTrack(track)

        val playlistId = playlistDao.insertPlaylist(Playlist(name = "ToDelete"))
        playlistDao.addTrackToPlaylist(
            PlaylistTrack(playlistId = playlistId, trackUri = "content://test/1", sortOrder = 0)
        )
        assertThat(playlistDao.trackCount(playlistId)).isEqualTo(1)

        playlistDao.deletePlaylistById(playlistId)
        assertThat(playlistDao.trackCount(playlistId)).isEqualTo(0)
    }
}
