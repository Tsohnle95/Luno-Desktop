package com.boombastic.mobile.data.repository

import com.boombastic.mobile.data.db.dao.PlaylistDao
import com.boombastic.mobile.data.db.dao.TrackDao
import com.boombastic.mobile.data.db.entity.Playlist
import com.boombastic.mobile.data.db.entity.PlaylistTrack
import kotlinx.coroutines.flow.Flow

class PlaylistRepository(
    private val playlistDao: PlaylistDao,
    private val trackDao: TrackDao
) {
    fun getAllPlaylists() = playlistDao.getAllPlaylists()

    fun getAllPlaylistsWithTracks() = playlistDao.getAllPlaylistsWithTracks()

    suspend fun getPlaylist(id: Long) = playlistDao.getPlaylist(id)

    suspend fun createPlaylist(name: String, description: String = ""): Result<Playlist> {
        if (name.isBlank()) {
            return Result.failure(IllegalArgumentException("Playlist name cannot be empty"))
        }
        val id = playlistDao.insertPlaylist(
            Playlist(name = name.trim(), description = description.trim())
        )
        val playlist = playlistDao.getPlaylist(id)
            ?: return Result.failure(IllegalStateException("Playlist was not created after insert"))
        return Result.success(playlist)
    }

    suspend fun deletePlaylist(id: Long) = playlistDao.deletePlaylistById(id)

    suspend fun addTrackToPlaylist(playlistId: Long, trackUri: String) {
        val maxOrder = playlistDao.maxSortOrder(playlistId) ?: -1
        playlistDao.addTrackToPlaylist(
            PlaylistTrack(
                playlistId = playlistId,
                trackUri = trackUri,
                sortOrder = maxOrder + 1
            )
        )
    }

    suspend fun removeTrackFromPlaylist(playlistId: Long, trackUri: String) {
        playlistDao.removeTrackFromPlaylist(playlistId, trackUri)
    }

    suspend fun getPlaylistWithTracks(id: Long) = playlistDao.getPlaylistWithTracks(id)
}
