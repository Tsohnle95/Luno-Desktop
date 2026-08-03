package com.luno.mobile.data.db

import android.content.Context
import androidx.room.Database
import androidx.room.Room
import androidx.room.RoomDatabase
import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase
import com.luno.mobile.data.db.dao.DownloadJobDao
import com.luno.mobile.data.db.dao.PlaylistDao
import com.luno.mobile.data.db.dao.TrackDao
import com.luno.mobile.data.db.entity.DownloadJob
import com.luno.mobile.data.db.entity.Playlist
import com.luno.mobile.data.db.entity.PlaylistTrack
import com.luno.mobile.data.db.entity.Track

@Database(
    entities = [Track::class, Playlist::class, PlaylistTrack::class, DownloadJob::class],
    version = 8,
    exportSchema = true
)
abstract class AppDatabase : RoomDatabase() {
    abstract fun trackDao(): TrackDao
    abstract fun playlistDao(): PlaylistDao
    abstract fun downloadJobDao(): DownloadJobDao

    companion object {
        private const val DB_NAME = "luno.db"

        private val MIGRATION_1_2 = object : Migration(1, 2) {
            override fun migrate(db: SupportSQLiteDatabase) {
                db.execSQL("""
                    CREATE TABLE IF NOT EXISTS `download_jobs` (
                        `id` INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                        `sourceUrl` TEXT NOT NULL,
                        `title` TEXT NOT NULL,
                        `artist` TEXT NOT NULL DEFAULT '',
                        `state` TEXT NOT NULL DEFAULT 'QUEUED',
                        `progress` INTEGER NOT NULL DEFAULT 0,
                        `localUri` TEXT NOT NULL DEFAULT '',
                        `errorMessage` TEXT NOT NULL DEFAULT '',
                        `addedAt` INTEGER NOT NULL,
                        `completedAt` INTEGER NOT NULL DEFAULT 0,
                        `playlistId` INTEGER,
                        `workManagerId` TEXT NOT NULL DEFAULT ''
                    )
                """.trimIndent())
            }
        }

        private val MIGRATION_2_3 = object : Migration(2, 3) {
            override fun migrate(db: SupportSQLiteDatabase) {
                db.execSQL("ALTER TABLE playlists ADD COLUMN playlistUrl TEXT NOT NULL DEFAULT ''")
            }
        }

        private val MIGRATION_3_4 = object : Migration(3, 4) {
            override fun migrate(db: SupportSQLiteDatabase) {
                db.execSQL("ALTER TABLE tracks ADD COLUMN albumArtPath TEXT")
            }
        }

        private val MIGRATION_4_5 = object : Migration(4, 5) {
            override fun migrate(db: SupportSQLiteDatabase) {
                db.execSQL(
                    "ALTER TABLE download_jobs ADD COLUMN thumbnailUrl TEXT NOT NULL DEFAULT ''"
                )
            }
        }

        private val MIGRATION_5_6 = object : Migration(5, 6) {
            override fun migrate(db: SupportSQLiteDatabase) {
                db.execSQL("ALTER TABLE tracks ADD COLUMN playCount INTEGER NOT NULL DEFAULT 0")
            }
        }

        private val MIGRATION_6_7 = object : Migration(6, 7) {
            override fun migrate(db: SupportSQLiteDatabase) {
                db.execSQL("ALTER TABLE playlists ADD COLUMN playCount INTEGER NOT NULL DEFAULT 0")
            }
        }

        private val MIGRATION_7_8 = object : Migration(7, 8) {
            override fun migrate(db: SupportSQLiteDatabase) {
                db.execSQL("ALTER TABLE download_jobs ADD COLUMN playlistIdsCsv TEXT NOT NULL DEFAULT ''")
            }
        }

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
            ).addMigrations(
                MIGRATION_1_2,
                MIGRATION_2_3,
                MIGRATION_3_4,
                MIGRATION_4_5,
                MIGRATION_5_6,
                MIGRATION_6_7,
                MIGRATION_7_8
            )
                .build()
        }
    }
}
