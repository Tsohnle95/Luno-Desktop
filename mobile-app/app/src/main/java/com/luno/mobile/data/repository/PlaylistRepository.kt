package com.luno.mobile.data.repository

import com.luno.mobile.data.db.dao.PlaylistDao
import com.luno.mobile.data.db.dao.TrackDao
import com.luno.mobile.data.db.entity.Playlist
import com.luno.mobile.data.db.entity.PlaylistTrack
import kotlinx.coroutines.flow.Flow

class PlaylistRepository(
    private val playlistDao: PlaylistDao,
    private val trackDao: TrackDao
) {
    fun getAllPlaylists() = playlistDao.getAllPlaylists()

    fun getAllPlaylistsWithTracks() = playlistDao.getAllPlaylistsWithTracks()

    fun getPlaylistsWithUrls(): Flow<List<Playlist>> = playlistDao.getPlaylistsWithUrls()

    suspend fun getPlaylistsWithUrlsOnce(): List<Playlist> = playlistDao.getPlaylistsWithUrlsOnce()

    suspend fun getPlaylist(id: Long) = playlistDao.getPlaylist(id)

    suspend fun createPlaylist(name: String, description: String = "", playlistUrl: String = ""): Result<Playlist> {
        if (name.isBlank()) {
            return Result.failure(IllegalArgumentException("Playlist name cannot be empty"))
        }
        val id = playlistDao.insertPlaylist(
            Playlist(name = name.trim(), description = description.trim(), playlistUrl = playlistUrl.trim())
        )
        val playlist = playlistDao.getPlaylist(id)
            ?: return Result.failure(IllegalStateException("Playlist was not created after insert"))
        return Result.success(playlist)
    }

    suspend fun updatePlaylistUrl(id: Long, url: String) {
        val pl = playlistDao.getPlaylist(id) ?: return
        playlistDao.updatePlaylist(pl.copy(playlistUrl = url.trim()))
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

    /** Adds a batch in one Room transaction and ignores duplicate membership. */
    suspend fun addTracksToPlaylist(playlistId: Long, trackUris: Collection<String>) {
        val distinctUris = trackUris.distinct()
        if (distinctUris.isEmpty()) return
        val firstOrder = (playlistDao.maxSortOrder(playlistId) ?: -1) + 1
        playlistDao.addTracksToPlaylist(
            distinctUris.mapIndexed { index, uri ->
                PlaylistTrack(
                    playlistId = playlistId,
                    trackUri = uri,
                    sortOrder = firstOrder + index
                )
            }
        )
    }

    suspend fun removeTrackFromPlaylist(playlistId: Long, trackUri: String) {
        playlistDao.removeTrackFromPlaylist(playlistId, trackUri)
    }

    /**
     * Removes every track from the playlist.  Metadata-only — the songs
     * stay in the library and the audio files stay on the device.
     */
    suspend fun clearPlaylist(playlistId: Long) {
        playlistDao.clearPlaylist(playlistId)
    }

    suspend fun getPlaylistWithTracks(id: Long) = playlistDao.getPlaylistWithTracks(id)
}
