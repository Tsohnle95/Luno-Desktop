package com.boombastic.mobile

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import androidx.work.Configuration
import com.boombastic.mobile.data.db.AppDatabase
import com.boombastic.mobile.data.repository.DownloadRepository
import com.boombastic.mobile.data.repository.LibraryRepository
import com.boombastic.mobile.data.repository.PlaylistRepository
import com.boombastic.mobile.playback.NewPipeDownloader
import org.schabi.newpipe.extractor.NewPipe
import java.net.CookieManager
import java.net.CookiePolicy
import java.net.CookieHandler

class BoomBasticApp : Application(), Configuration.Provider {

    lateinit var database: AppDatabase
        private set
    lateinit var libraryRepository: LibraryRepository
        private set
    lateinit var playlistRepository: PlaylistRepository
        private set
    lateinit var downloadRepository: DownloadRepository
        private set

    override fun onCreate() {
        super.onCreate()
        database = AppDatabase.getInstance(this)
        libraryRepository = LibraryRepository(this, database.trackDao())
        playlistRepository = PlaylistRepository(database.playlistDao(), database.trackDao())
        downloadRepository = DownloadRepository(database.downloadJobDao(), this)

        createDownloadNotificationChannel()
        CookieHandler.setDefault(CookieManager(null, CookiePolicy.ACCEPT_ALL))
        NewPipe.init(NewPipeDownloader())
    }

    override val workManagerConfiguration: Configuration
        get() = Configuration.Builder()
            .setMinimumLoggingLevel(android.util.Log.INFO)
            .build()

    private fun createDownloadNotificationChannel() {
        val channel = NotificationChannel(
            "download_channel",
            "Downloads",
            NotificationManager.IMPORTANCE_LOW
        ).apply {
            description = "Ongoing download progress"
            setShowBadge(false)
        }
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(channel)
    }
}