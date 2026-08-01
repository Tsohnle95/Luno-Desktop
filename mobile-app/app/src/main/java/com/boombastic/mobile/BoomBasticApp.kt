package com.boombastic.mobile

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import android.util.Log
import androidx.work.Configuration
import com.boombastic.mobile.data.db.AppDatabase
import com.boombastic.mobile.data.repository.DownloadRepository
import com.boombastic.mobile.data.repository.LibraryData
import com.boombastic.mobile.data.repository.LibraryRepository
import com.boombastic.mobile.data.repository.MusicFolderRepository
import com.boombastic.mobile.data.repository.PlaylistRepository
import com.boombastic.mobile.playback.NewPipeDownloader
import com.boombastic.mobile.ui.shell.MusicFolderImportManager
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.schabi.newpipe.extractor.NewPipe
import java.io.File
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
    lateinit var musicFolderRepository: MusicFolderRepository
        private set
    lateinit var musicFolderImportManager: MusicFolderImportManager
        private set
    lateinit var libraryData: LibraryData
        private set

    /**
     * Process-lifetime scope for long-running work (library imports) that
     * must survive navigation away from the launching screen — a
     * `rememberCoroutineScope` dies with its composable, which is exactly
     * how folder imports used to stop partway.
     */
    val appScope: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    override fun onCreate() {
        super.onCreate()
        database = AppDatabase.getInstance(this)
        libraryRepository = LibraryRepository(this, database.trackDao(), database.playlistDao())
        playlistRepository = PlaylistRepository(database.playlistDao(), database.trackDao())
        downloadRepository = DownloadRepository(database.downloadJobDao(), this)
        musicFolderRepository = MusicFolderRepository(this)
        musicFolderImportManager = MusicFolderImportManager(
            appScope = appScope,
            libraryRepository = libraryRepository,
            musicFolderRepository = musicFolderRepository,
            context = this
        )
        // Warm the library data eagerly at startup so every screen renders
        // its full content in the same frame as the navigation transition
        // (no per-tab-switch query latency, no loading-spinner pop-in).
        libraryData = LibraryData(
            appScope = appScope,
            trackDao = database.trackDao(),
            playlistDao = database.playlistDao(),
            downloadJobDao = database.downloadJobDao()
        )

        createDownloadNotificationChannel()
        CookieHandler.setDefault(CookieManager(null, CookiePolicy.ACCEPT_ALL))
        NewPipe.init(NewPipeDownloader())
        installCrashCapture()
    }

    /**
     * Captures any uncaught crash to `filesDir/crash_log.txt` (readable from
     * the Settings drawer's "Error log" item) before the process dies, so a
     * reproducible crash can be reported without logcat.  The previous
     * default handler always runs afterwards.
     */
    private fun installCrashCapture() {
        val defaultHandler = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { thread, throwable ->
            runCatching {
                val logFile = File(filesDir, "crash_log.txt")
                val stack = Log.getStackTraceString(throwable)
                logFile.appendText(
                    "=== ${System.currentTimeMillis()} — ${thread.name} ===\n$stack\n\n"
                )
            }
            defaultHandler?.uncaughtException(thread, throwable)
        }
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