package com.boombastic.mobile.data.db

import android.content.Context
import androidx.room.Database
import androidx.room.Room
import androidx.room.RoomDatabase
import com.boombastic.mobile.data.db.dao.PlaylistDao
import com.boombastic.mobile.data.db.dao.TrackDao
import com.boombastic.mobile.data.db.entity.Playlist
import com.boombastic.mobile.data.db.entity.PlaylistTrack
import com.boombastic.mobile.data.db.entity.Track

@Database(
    entities = [Track::class, Playlist::class, PlaylistTrack::class],
    version = 1,
    exportSchema = true
)
abstract class AppDatabase : RoomDatabase() {
    abstract fun trackDao(): TrackDao
    abstract fun playlistDao(): PlaylistDao

    companion object {
        private const val DB_NAME = "boombastic.db"

        @Volatile
        private var INSTANCE: AppDatabase? = null

        fun getInstance(context: Context): AppDatabase {
            return INSTANCE ?: synchronized(this) {
                INSTANCE ?: buildDatabase(context).also { INSTANCE = it }
            }
        }

        private fun buildDatabase(context: Context): AppDatabase {
            return Room.databaseBuilder(
                context.applicationContext,
                AppDatabase::class.java,
                DB_NAME
            ).build()
        }
    }
}
