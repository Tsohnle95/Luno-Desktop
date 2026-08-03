package com.luno.mobile.ui.components

import com.luno.mobile.data.db.dao.PlaylistWithTracks
import com.luno.mobile.data.db.entity.Playlist
import com.luno.mobile.data.db.entity.Track
import com.google.common.truth.Truth.assertThat
import org.junit.Test

class SortChipTest {

    @Test
    fun `recent sort is labeled recently added`() {
        assertThat(TrackSortMode.RECENT.label).isEqualTo("Recently added")
    }

    @Test
    fun `recent sort puts newest songs first`() {
        val tracks = listOf(
            Track(uri = "old", title = "Old", addedAt = 100L),
            Track(uri = "new", title = "New", addedAt = 200L)
        )

        assertThat(tracks.sortedByMode(TrackSortMode.RECENT).map { it.uri })
            .containsExactly("new", "old")
            .inOrder()
    }

    @Test
    fun `recent sort puts newest playlists first`() {
        val playlists = listOf(
            playlistWithTracks(id = 1L, name = "Old", createdAt = 100L),
            playlistWithTracks(id = 2L, name = "New", createdAt = 200L)
        )

        assertThat(playlists.sortedPlaylistsByMode(TrackSortMode.RECENT).map { it.playlist.id })
            .containsExactly(2L, 1L)
            .inOrder()
    }

    private fun playlistWithTracks(
        id: Long,
        name: String,
        createdAt: Long
    ): PlaylistWithTracks = PlaylistWithTracks(
        playlist = Playlist(id = id, name = name, createdAt = createdAt),
        tracks = emptyList()
    )
}
