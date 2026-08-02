package com.boombastic.mobile.data.repository

import android.content.Context
import androidx.room.Room
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.boombastic.mobile.data.db.AppDatabase
import com.boombastic.mobile.data.db.entity.Playlist
import com.boombastic.mobile.data.db.entity.PlaylistTrack
import com.boombastic.mobile.data.db.entity.Track
import com.google.common.truth.Truth.assertThat
import androidx.work.testing.WorkManagerTestInitHelper
import org.junit.After
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import kotlinx.coroutines.runBlocking

@RunWith(AndroidJUnit4::class)
@Config(manifest = Config.NONE, sdk = [34])
class LibraryTransferRepositoryTest {
    private lateinit var database: AppDatabase
    private lateinit var repository: LibraryTransferRepository
    private lateinit var context: Context

    @Before
    fun setUp() {
        context = ApplicationProvider.getApplicationContext()
        WorkManagerTestInitHelper.initializeTestWorkManager(context)
        database = Room.inMemoryDatabaseBuilder(context, AppDatabase::class.java).build()
        repository = LibraryTransferRepository(
            database,
            DownloadRepository(database.downloadJobDao(), context, database.playlistDao())
        )
    }

    @After
    fun tearDown() {
        database.close()
    }

    @Test
    fun fullExportPreservesPlaylistRelationOrderAndUnassignedTracks() = runBlocking<Unit> {
        val first = Track(uri = "file:///first.mp3", title = "First", artist = "Artist")
        val second = Track(uri = "file:///second.mp3", title = "Second", artist = "Artist")
        val unassigned = Track(uri = "file:///unassigned.mp3", title = "Loose", artist = "Artist")
        database.trackDao().insertTracks(listOf(first, second, unassigned))
        val playlistId = database.playlistDao().insertPlaylist(Playlist(name = "Favorites"))
        database.playlistDao().addTrackToPlaylist(PlaylistTrack(playlistId, second.uri, 10))
        database.playlistDao().addTrackToPlaylist(PlaylistTrack(playlistId, first.uri, 2))

        val manifest = repository.buildFullLibraryManifest()
        val playlist = manifest.playlists.single()
        val titleByRef = manifest.tracks.associateBy { it.ref }.mapValues { it.value.title }

        assertThat(playlist.trackRefs.map { titleByRef[it] }).containsExactly("First", "Second").inOrder()
        assertThat(manifest.unassignedTrackRefs.map { titleByRef[it] }).containsExactly("Loose")
    }

    @Test
    fun selectedExportDoesNotExposeLocalUri() = runBlocking<Unit> {
        val track = Track(uri = "content://private/path/song.mp3", title = "Song", artist = "Artist")
        database.trackDao().insertTrack(track)

        val json = repository.encode(repository.buildSelectedTracksManifest(listOf(track.uri)))

        assertThat(json).doesNotContain(track.uri)
        assertThat(json).contains("selected_tracks")
    }
}
