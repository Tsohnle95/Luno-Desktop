package com.luno.mobile.data.db.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import androidx.room.Update
import com.luno.mobile.data.db.entity.DownloadJob
import com.luno.mobile.data.db.entity.DownloadState
import kotlinx.coroutines.flow.Flow

@Dao
interface DownloadJobDao {
    @Query("SELECT * FROM download_jobs ORDER BY addedAt DESC")
    fun getAllDownloads(): Flow<List<DownloadJob>>

    @Query("SELECT * FROM download_jobs WHERE state = :state ORDER BY addedAt DESC")
    fun getDownloadsByState(state: DownloadState): Flow<List<DownloadJob>>

    @Query("SELECT * FROM download_jobs WHERE id = :id")
    suspend fun getDownload(id: Long): DownloadJob?

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insertDownload(job: DownloadJob): Long

    @Update
    suspend fun updateDownload(job: DownloadJob)

    @Query("UPDATE download_jobs SET state = :state, progress = :progress WHERE id = :id")
    suspend fun updateProgress(id: Long, state: DownloadState, progress: Int)

    @Query("UPDATE download_jobs SET state = :state, localUri = :localUri, completedAt = :completedAt, progress = 100 WHERE id = :id")
    suspend fun markCompleted(id: Long, state: DownloadState, localUri: String, completedAt: Long)

    @Query("UPDATE download_jobs SET state = :state, errorMessage = :errorMessage WHERE id = :id")
    suspend fun markFailed(id: Long, state: DownloadState, errorMessage: String)

    @Query("DELETE FROM download_jobs WHERE id = :id")
    suspend fun deleteDownload(id: Long)

    @Query("DELETE FROM download_jobs WHERE state = :state")
    suspend fun deleteDownloadsByState(state: DownloadState)

    @Query("SELECT COUNT(*) FROM download_jobs WHERE state = :state")
    suspend fun countByState(state: DownloadState): Int

    @Query("SELECT * FROM download_jobs WHERE state IN ('QUEUED', 'DOWNLOADING')")
    suspend fun getActiveDownloadsOnce(): List<DownloadJob>

    @Query("SELECT * FROM download_jobs WHERE state = 'COMPLETED' AND localUri != ''")
    suspend fun getCompletedDownloadsOnce(): List<DownloadJob>

    @Query("SELECT * FROM download_jobs WHERE state IN ('QUEUED', 'DOWNLOADING') AND playlistId = :playlistId")
    suspend fun getActiveDownloadsForPlaylistOnce(playlistId: Long): List<DownloadJob>

    @Query("SELECT COUNT(*) FROM download_jobs WHERE sourceUrl LIKE '%' || :videoId || '%' AND state IN ('QUEUED', 'DOWNLOADING')")
    suspend fun countByVideoQuery(videoId: String): Int
}
