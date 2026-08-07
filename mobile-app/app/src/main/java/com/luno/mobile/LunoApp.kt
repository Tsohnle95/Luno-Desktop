package com.luno.mobile

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import android.util.Log
import androidx.work.Configuration
import com.luno.mobile.data.db.AppDatabase
import com.luno.mobile.data.db.entity.Track
import com.luno.mobile.data.discovery.LastfmResult
import com.luno.mobile.data.repository.DiscoveryRepository
import com.luno.mobile.data.repository.DownloadRepository
import com.luno.mobile.data.repository.LibraryData
import com.luno.mobile.data.repository.LibraryRepository
import com.luno.mobile.data.repository.LibraryTransferRepository
import com.luno.mobile.data.repository.MusicFolderRepository
import com.luno.mobile.data.repository.PlaylistRepository
import com.luno.mobile.data.discovery.RecommendationArtworkService
import com.luno.mobile.playback.NewPipeDownloader
import com.luno.mobile.playback.RecommendationPreviewManager
import com.luno.mobile.ui.shell.ArtworkFetchManager
import com.luno.mobile.ui.shell.MusicFolderImportManager
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import org.schabi.newpipe.extractor.NewPipe
import java.io.File
import java.net.CookieManager
import java.net.CookiePolicy
import java.net.CookieHandler

class LunoApp : Application(), Configuration.Provider {

    companion object {
        private const val RECENT_PLAYLIST_PREFS = "playlist_picker"
        private const val RECENT_PLAYLIST_IDS = "recently_saved_ids"
        private const val RECENT_PLAYLIST_LIMIT = 3
    }

    lateinit var database: AppDatabase
        private set
    lateinit var libraryRepository: LibraryRepository
        private set
    lateinit var playlistRepository: PlaylistRepository
        private set
    lateinit var downloadRepository: DownloadRepository
        private set
    lateinit var libraryTransferRepository: LibraryTransferRepository
        private set
    lateinit var musicFolderRepository: MusicFolderRepository
        private set
    lateinit var discoveryRepository: DiscoveryRepository
        private set
    lateinit var recommendationArtworkService: RecommendationArtworkService
        private set
    lateinit var musicFolderImportManager: MusicFolderImportManager
        private set
    lateinit var artworkFetchManager: ArtworkFetchManager
        private set
    lateinit var recommendationPreviewManager: RecommendationPreviewManager
        private set
    lateinit var libraryData: LibraryData
        private set

    /** The exact Home-session sample shown by the Made for You carousel. */
    private val madeForYouTracksState = MutableStateFlow<List<Track>>(emptyList())
    val madeForYouTracks: StateFlow<List<Track>> = madeForYouTracksState

    private val recentlySavedPlaylistPreferences by lazy {
        getSharedPreferences(RECENT_PLAYLIST_PREFS, MODE_PRIVATE)
    }
    private val recentlySavedPlaylistLock = Any()
    private val recentlySavedPlaylistIdsState by lazy {
        MutableStateFlow(loadRecentlySavedPlaylistIds())
    }
    val recentlySavedPlaylistIds: StateFlow<List<Long>>
        get() = recentlySavedPlaylistIdsState.asStateFlow()

    /**
     * Process-lifetime scope for long-running work (library imports) that
     * must survive navigation away from the launching screen — a
     * `rememberCoroutineScope` dies with its composable, which is exactly
     * how folder imports used to stop partway.
     */
    val appScope: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    fun setMadeForYouTracks(tracks: List<Track>) {
        madeForYouTracksState.value = tracks
    }

    /** Records a successful playlist destination, newest first, capped at three. */
    fun rememberRecentlySavedPlaylist(playlistId: Long) {
        if (playlistId <= 0L) return
        synchronized(recentlySavedPlaylistLock) {
            val updated = (listOf(playlistId) + recentlySavedPlaylistIdsState.value
                .filterNot { it == playlistId })
                .take(RECENT_PLAYLIST_LIMIT)
            recentlySavedPlaylistIdsState.value = updated
            recentlySavedPlaylistPreferences.edit()
                .putString(RECENT_PLAYLIST_IDS, updated.joinToString(","))
                .apply()
        }
    }

    private fun loadRecentlySavedPlaylistIds(): List<Long> =
        getSharedPreferences(RECENT_PLAYLIST_PREFS, MODE_PRIVATE)
            .getString(RECENT_PLAYLIST_IDS, "")
            .orEmpty()
            .split(',')
            .mapNotNull { it.toLongOrNull() }
            .distinct()
            .take(RECENT_PLAYLIST_LIMIT)

    override fun onCreate() {
        super.onCreate()
        database = AppDatabase.getInstance(this)
        libraryRepository = LibraryRepository(
            context = this,
            trackDao = database.trackDao(),
            playlistDao = database.playlistDao(),
            downloadJobDao = database.downloadJobDao()
        )
        playlistRepository = PlaylistRepository(database.playlistDao(), database.trackDao())
        downloadRepository = DownloadRepository(
            downloadJobDao = database.downloadJobDao(),
            context = this,
            playlistDao = database.playlistDao(),
            trackDao = database.trackDao(),
            database = database
        )
        libraryTransferRepository = LibraryTransferRepository(database, downloadRepository)
        musicFolderRepository = MusicFolderRepository(this)
        discoveryRepository = DiscoveryRepository(this)
        recommendationArtworkService = RecommendationArtworkService()
        musicFolderImportManager = MusicFolderImportManager(
            appScope = appScope,
            libraryRepository = libraryRepository,
            musicFolderRepository = musicFolderRepository,
            context = this
        )
        artworkFetchManager = ArtworkFetchManager(
            appScope = appScope,
            libraryRepository = libraryRepository,
            context = this
        )
        recommendationPreviewManager = RecommendationPreviewManager(
            context = this,
            appScope = appScope,
            downloadRepository = downloadRepository,
            playlistRepository = playlistRepository,
            discoverRecommendations = { track ->
                when (val result = discoveryRepository.getSimilar(
                    artist = track.artist,
                    title = track.title,
                    limit = 1_000,
                    libraryTracks = database.trackDao().getAllTracksOnce()
                )) {
                    is LastfmResult.Success -> Result.success(result.tracks)
                    is LastfmResult.Failure -> Result.failure(
                        IllegalStateException(result.message)
                    )
                }
            }
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
        appScope.launch {
            downloadRepository.repairUnsortedMemberships(database.trackDao())
        }
        appScope.launch {
            downloadRepository.repairMissingDownloadedArtwork()
        }

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
