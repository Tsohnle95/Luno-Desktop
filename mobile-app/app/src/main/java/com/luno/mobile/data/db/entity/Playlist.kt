package com.luno.mobile.data.db.entity

import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(tableName = "playlists")
data class Playlist(
    @PrimaryKey(autoGenerate = true) val id: Long = 0,
    val name: String,
    val description: String = "",
    val playlistUrl: String = "",
    val playCount: Int = 0,
    val createdAt: Long = System.currentTimeMillis()
)

/** Reserved IDs for playlists that are derived from library metadata. */
object SystemPlaylists {
    // -1 is already used by the Made-for-you virtual playlist route.
    const val FAVORITES_ID = -2L
    const val FAVORITES_NAME = "Favorites"
}
