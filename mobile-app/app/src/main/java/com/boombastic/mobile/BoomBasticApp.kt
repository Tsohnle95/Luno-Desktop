package com.boombastic.mobile

import android.app.Application
import com.boombastic.mobile.data.db.AppDatabase
import com.boombastic.mobile.data.repository.LibraryRepository
import com.boombastic.mobile.data.repository.PlaylistRepository

class BoomBasticApp : Application() {

    lateinit var database: AppDatabase
        private set
    lateinit var libraryRepository: LibraryRepository
        private set
    lateinit var playlistRepository: PlaylistRepository
        private set

    override fun onCreate() {
        super.onCreate()
        database = AppDatabase.getInstance(this)
        libraryRepository = LibraryRepository(this, database.trackDao())
        playlistRepository = PlaylistRepository(database.playlistDao(), database.trackDao())
    }
}
