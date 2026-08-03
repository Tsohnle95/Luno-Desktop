package com.luno.mobile.data.repository

import android.content.Context
import androidx.room.Room
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.luno.mobile.data.db.AppDatabase
import com.google.common.truth.Truth.assertThat
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

@RunWith(AndroidJUnit4::class)
@Config(manifest = Config.NONE, sdk = [34])
class PlaylistRepositoryTest {

    private lateinit var database: AppDatabase
    private lateinit var repository: PlaylistRepository

    @Before
    fun setUp() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        database = Room.inMemoryDatabaseBuilder(context, AppDatabase::class.java).build()
        repository = PlaylistRepository(database.playlistDao(), database.trackDao())
    }

    @After
    fun tearDown() {
        database.close()
    }

    @Test
    fun createPlaylist_blankName_returnsFailure() = runBlocking {
        val result = repository.createPlaylist("")
        assertThat(result.isFailure).isTrue()
        assertThat(result.exceptionOrNull()).isInstanceOf(IllegalArgumentException::class.java)
    }

    @Test
    fun createPlaylist_validName_persists() = runBlocking {
        val result = repository.createPlaylist("My Playlist")
        assertThat(result.isSuccess).isTrue()
        val playlist = result.getOrNull()!!
        assertThat(playlist.name).isEqualTo("My Playlist")
        assertThat(playlist.id).isGreaterThan(0)
    }

    @Test
    fun createPlaylist_trimsName() = runBlocking {
        val result = repository.createPlaylist("  My List  ")
        assertThat(result.isSuccess).isTrue()
        assertThat(result.getOrNull()!!.name).isEqualTo("My List")
    }

    @Test
    fun getAllPlaylists_returnsCreatedPlaylists() = runBlocking {
        repository.createPlaylist("A")
        repository.createPlaylist("B")

        val playlists = repository.getAllPlaylists().first()
        assertThat(playlists).hasSize(2)
    }

    @Test
    fun deletePlaylist_removesIt() = runBlocking {
        val result = repository.createPlaylist("Delete Me")
        val id = result.getOrNull()!!.id

        repository.deletePlaylist(id)
        val playlists = repository.getAllPlaylists().first()
        assertThat(playlists).isEmpty()
    }

    @Test
    fun clearPlaylist_removesTracksButKeepsPlaylistAndTracks() = runBlocking {
        val playlist = repository.createPlaylist("Clear Me").getOrNull()!!
        val track = com.luno.mobile.data.db.entity.Track(
            uri = "content://test/1",
            title = "Song"
        )
        database.trackDao().insertTrack(track)
        repository.addTrackToPlaylist(playlist.id, track.uri)

        repository.clearPlaylist(playlist.id)

        val withTracks = repository.getPlaylistWithTracks(playlist.id)
        assertThat(withTracks).isNotNull()
        assertThat(withTracks!!.tracks).isEmpty()
        // Playlist itself and the track both survive (metadata-only removal).
        assertThat(repository.getAllPlaylists().first()).hasSize(1)
        assertThat(database.trackDao().getAllTracksOnce()).hasSize(1)
    }

    @Test
    fun addTracksToPlaylist_batchesAndDeduplicatesMembership() = runBlocking {
        val tracks = (1..3).map { index ->
            com.luno.mobile.data.db.entity.Track(
                uri = "content://test/$index",
                title = "Song $index"
            )
        }
        database.trackDao().insertTracks(tracks)
        val playlist = repository.createPlaylist("Batch").getOrThrow()

        repository.addTracksToPlaylist(
            playlist.id,
            listOf("content://test/1", "content://test/2", "content://test/1")
        )
        repository.addTracksToPlaylist(playlist.id, listOf("content://test/2", "content://test/3"))

        assertThat(repository.getPlaylistWithTracks(playlist.id)!!.tracks.map { it.uri })
            .containsExactly("content://test/1", "content://test/2", "content://test/3")
            .inOrder()
    }
}
