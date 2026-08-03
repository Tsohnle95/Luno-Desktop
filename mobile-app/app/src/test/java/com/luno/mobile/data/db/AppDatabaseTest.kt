package com.luno.mobile.data.db

import android.content.Context
import androidx.room.Room
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.luno.mobile.data.db.dao.PlaylistDao
import com.luno.mobile.data.db.dao.TrackDao
import org.junit.After
import org.junit.Before
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

@RunWith(AndroidJUnit4::class)
@Config(manifest = Config.NONE, sdk = [34])
abstract class AppDatabaseTest {

    protected lateinit var database: AppDatabase
    protected lateinit var trackDao: TrackDao
    protected lateinit var playlistDao: PlaylistDao

    @Before
    fun initDb() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        database = Room.inMemoryDatabaseBuilder(
            context,
            AppDatabase::class.java
        ).build()
        trackDao = database.trackDao()
        playlistDao = database.playlistDao()
    }

    @After
    fun closeDb() {
        database.close()
    }
}
