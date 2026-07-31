package com.boombastic.mobile.data.db.entity

import android.net.Uri
import androidx.room.Entity
import androidx.room.PrimaryKey
import java.io.File

@Entity(tableName = "tracks")
data class Track(
    @PrimaryKey val uri: String,
    val title: String,
    val artist: String = "",
    val album: String = "",
    val durationMs: Long = 0L,
    val albumArtPath: String? = null,
    val addedAt: Long = System.currentTimeMillis()
) {
    /** `file://` URI of the cached artwork, or `null` when unavailable. */
    fun albumArtUri(): String? =
        albumArtPath?.let { Uri.fromFile(File(it)).toString() }
}
