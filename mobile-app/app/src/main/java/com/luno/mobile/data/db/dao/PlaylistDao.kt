package com.luno.mobile.data.db.dao

import androidx.room.Dao
import androidx.room.Delete
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import androidx.room.Relation
import androidx.room.Transaction
import androidx.room.Update
import com.luno.mobile.data.db.entity.Playlist
import com.luno.mobile.data.db.entity.PlaylistTrack
import com.luno.mobile.data.db.entity.Track
import kotlinx.coroutines.flow.Flow

data class PlaylistWithTracks(
    @androidx.room.Embedded val playlist: Playlist,
    @Relation(
        parentColumn = "id",
        entityColumn = "uri",
        associateBy = androidx.room.Junction(
            value = PlaylistTrack::class,
            parentColumn = "playlistId",
            entityColumn = "trackUri"
        )
    )
    val tracks: List<Track>
)

@Dao
interface PlaylistDao {
    @Query("SELECT * FROM playlists ORDER BY name ASC")
    fun getAllPlaylists(): Flow<List<Playlist>>

    @Query("SELECT * FROM playlists ORDER BY name COLLATE NOCASE ASC")
    suspend fun getAllPlaylistsOnce(): List<Playlist>

    @Query("SELECT * FROM playlist_tracks ORDER BY playlistId ASC, sortOrder ASC")
    suspend fun getAllPlaylistTracksOnce(): List<PlaylistTrack>

    @Query("SELECT * FROM playlists WHERE id = :id")
    suspend fun getPlaylist(id: Long): Playlist?

    @Query("UPDATE playlists SET playCount = playCount + 1 WHERE id = :playlistId")
    suspend fun incrementPlayCount(playlistId: Long)

    @Query("SELECT * FROM playlists WHERE name = :name LIMIT 1")
    suspend fun getPlaylistByName(name: String): Playlist?

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insertPlaylist(playlist: Playlist): Long

    @Update
    suspend fun updatePlaylist(playlist: Playlist)

    @Delete
    suspend fun deletePlaylist(playlist: Playlist)

    @Query("DELETE FROM playlists WHERE id = :id")
    suspend fun deletePlaylistById(id: Long)

    @Transaction
    @Query("SELECT * FROM playlists WHERE id = :id")
    suspend fun getPlaylistWithTracks(id: Long): PlaylistWithTracks?

    @Transaction
    @Query("SELECT * FROM playlists ORDER BY name ASC")
    fun getAllPlaylistsWithTracks(): Flow<List<PlaylistWithTracks>>

    @Insert(onConflict = OnConflictStrategy.IGNORE)
    suspend fun addTrackToPlaylist(playlistTrack: PlaylistTrack)

    @Insert(onConflict = OnConflictStrategy.IGNORE)
    suspend fun addTracksToPlaylist(playlistTracks: List<PlaylistTrack>)

    @Query("DELETE FROM playlist_tracks WHERE playlistId = :playlistId AND trackUri = :trackUri")
    suspend fun removeTrackFromPlaylist(playlistId: Long, trackUri: String)

    @Query("DELETE FROM playlist_tracks WHERE playlistId = :playlistId")
    suspend fun clearPlaylist(playlistId: Long)

    @Query("SELECT COUNT(*) FROM playlist_tracks WHERE playlistId = :playlistId")
    suspend fun trackCount(playlistId: Long): Int

    @Query("SELECT COALESCE(MAX(sortOrder), 0) FROM playlist_tracks WHERE playlistId = :playlistId")
    suspend fun maxSortOrder(playlistId: Long): Int

    @Query("SELECT EXISTS(SELECT 1 FROM playlist_tracks WHERE trackUri = :trackUri)")
    suspend fun isTrackInAnyPlaylist(trackUri: String): Boolean

    @Query("SELECT * FROM playlists WHERE playlistUrl != '' AND playlistUrl IS NOT NULL")
    fun getPlaylistsWithUrls(): Flow<List<Playlist>>

    @Query("SELECT * FROM playlists WHERE playlistUrl != '' AND playlistUrl IS NOT NULL")
    suspend fun getPlaylistsWithUrlsOnce(): List<Playlist>
}
