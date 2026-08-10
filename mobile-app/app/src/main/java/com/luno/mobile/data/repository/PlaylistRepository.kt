package com.luno.mobile.data.repository

import android.net.Uri
import com.luno.mobile.data.db.dao.PlaylistDao
import com.luno.mobile.data.db.dao.TrackDao
import com.luno.mobile.data.db.entity.Playlist
import com.luno.mobile.data.db.entity.PlaylistTrack
import com.luno.mobile.data.db.entity.Track
import com.luno.mobile.playback.MediaTrack
import kotlinx.coroutines.flow.Flow
import java.io.File

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
        require(playlistId > 0L) { "Playlist does not exist" }
        require(playlistDao.getPlaylist(playlistId) != null) {
            "The selected playlist no longer exists"
        }
        require(trackUri.isNotBlank() && trackDao.exists(trackUri)) {
            "This song is not in the library yet"
        }
        val maxOrder = playlistDao.maxSortOrder(playlistId) ?: -1
        playlistDao.addTrackToPlaylist(
            PlaylistTrack(
                playlistId = playlistId,
                trackUri = trackUri,
                sortOrder = maxOrder + 1
            )
        )
    }

    /**
     * Adds the item currently represented by the player. Normal library
     * tracks already exist in Room; a non-transient queue item may not (for
     * example, a direct URI). Materialize its metadata first so the junction
     * insert cannot fail its Track foreign key and the song is searchable in
     * the destination playlist immediately.
     */
    suspend fun addMediaTrackToPlaylist(playlistId: Long, track: MediaTrack): Result<Unit> =
        runCatching {
            require(playlistId > 0L) { "Playlist does not exist" }
            require(playlistDao.getPlaylist(playlistId) != null) {
                "The selected playlist no longer exists"
            }
            if (trackDao.getTrack(track.uri) == null) {
                require(track.uri.isNotBlank()) { "This song has no playable source" }
                val artworkPath = track.artworkUri?.let { artworkUri ->
                    runCatching { Uri.parse(artworkUri).path }.getOrNull()
                        ?.takeIf { File(it).isFile }
                }
                trackDao.insertTrack(
                    Track(
                        uri = track.uri,
                        title = track.title.trim().ifBlank { "Unknown Track" },
                        artist = track.artist.trim(),
                        album = track.album.trim(),
                        durationMs = track.durationMs.coerceAtLeast(0L),
                        albumArtPath = artworkPath
                    )
                )
            }
            addTrackToPlaylist(playlistId, track.uri)
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
