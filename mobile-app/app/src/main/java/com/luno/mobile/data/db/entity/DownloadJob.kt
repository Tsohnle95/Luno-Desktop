package com.luno.mobile.data.db.entity

import androidx.room.Entity
import androidx.room.PrimaryKey

enum class DownloadState {
    QUEUED,
    DOWNLOADING,
    COMPLETED,
    FAILED,
    CANCELLED
}

@Entity(tableName = "download_jobs")
data class DownloadJob(
    @PrimaryKey(autoGenerate = true) val id: Long = 0,
    val sourceUrl: String,
    val title: String,
    val artist: String = "",
    val state: DownloadState = DownloadState.QUEUED,
    val progress: Int = 0,
    val localUri: String = "",
    val errorMessage: String = "",
    val addedAt: Long = System.currentTimeMillis(),
    val completedAt: Long = 0L,
    val playlistId: Long? = null,
    val workManagerId: String = "",
    val thumbnailUrl: String = "",
    /** Additional destination playlist ids for one imported download. */
    val playlistIdsCsv: String = "",
    /** Original YouTube videoId for durable cross-process job lookup. */
    val videoId: String = ""
)
