package com.boombastic.mobile.data.db.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import com.boombastic.mobile.data.db.entity.Track
import kotlinx.coroutines.flow.Flow

@Dao
interface TrackDao {
    @Query("SELECT * FROM tracks ORDER BY addedAt DESC")
    fun getAllTracks(): Flow<List<Track>>

    @Query("SELECT * FROM tracks WHERE uri = :uri")
    suspend fun getTrack(uri: String): Track?

    @Query("SELECT * FROM tracks WHERE title LIKE '%' || :query || '%' OR artist LIKE '%' || :query || '%' OR album LIKE '%' || :query || '%' ORDER BY title ASC")
    fun searchTracks(query: String): Flow<List<Track>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insertTrack(track: Track)

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insertTracks(tracks: List<Track>)

    @Query("DELETE FROM tracks WHERE uri = :uri")
    suspend fun deleteTrack(uri: String)

    @Query("SELECT COUNT(*) FROM tracks")
    suspend fun trackCount(): Int

    @Query("SELECT EXISTS(SELECT 1 FROM tracks WHERE uri = :uri)")
    suspend fun exists(uri: String): Boolean
}
