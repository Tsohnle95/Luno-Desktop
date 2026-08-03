package com.luno.mobile.ui.navigation

import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.navigation.NavHostController
import androidx.navigation.NavType
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.navArgument
import com.luno.mobile.LunoApp
import com.luno.mobile.playback.MediaTrack
import com.luno.mobile.playback.MusicController
import com.luno.mobile.ui.discover.DiscoverScreen
import com.luno.mobile.ui.downloads.DownloadsScreen
import com.luno.mobile.ui.export.ExportImportScreen
import com.luno.mobile.ui.home.HomeScreen
import com.luno.mobile.ui.library.LibraryScreen
import com.luno.mobile.ui.library.DuplicateScreen
import com.luno.mobile.ui.library.PlaylistDetailScreen
import com.luno.mobile.ui.player.FullPlayerScreen
import com.luno.mobile.ui.player.RecentsScreen
import com.luno.mobile.ui.search.SearchScreen

object Routes {
    const val HOME = "home"
    const val SEARCH = "search"
    const val LIBRARY = "library"
    const val DISCOVER = "discover"
    const val DOWNLOADS = "downloads"
    const val RECENTS = "recents"
    const val DUPLICATES = "duplicates"
    const val FULL_PLAYER = "full_player"
    const val MADE_FOR_YOU = "made_for_you"
    const val EXPORT_IMPORT = "export_import"

    const val PLAYLIST_DETAIL = "playlist/{playlistId}"
    fun playlistDetail(playlistId: Long) = "playlist/$playlistId"
}

@Composable
fun LunoNavHost(
    navController: NavHostController,
    musicController: MusicController,
    modifier: Modifier = Modifier,
    onCreatePlaylist: () -> Unit,
    onPlay: (List<MediaTrack>, Int, Boolean) -> Unit = { _, _, _ -> },
    onNavigate: () -> Unit = {},
    onDiscoverLoadingChanged: (Boolean) -> Unit = {},
    onExportTracks: (List<String>) -> Unit = {},
    onExportPlaylists: (List<Long>) -> Unit = {}
) {
    val app = LocalContext.current.applicationContext as LunoApp
    val madeForYouTracks by app.madeForYouTracks.collectAsState()

    NavHost(
        navController = navController,
        startDestination = Routes.HOME,
        modifier = modifier,
        // Instant screen swaps: the incoming screen appears fully-formed
        // UNDER the shell's green-loader transition mask, which then fades
        // away on its own — a screen-side fade would run simultaneously
        // with the mask reveal and read as jumpy/fidgety.
        enterTransition = { EnterTransition.None },
        exitTransition = { ExitTransition.None },
        popEnterTransition = { EnterTransition.None },
        popExitTransition = { ExitTransition.None }
    ) {
        composable(Routes.HOME) {
            HomeScreen(
                musicController = musicController,
                onPlay = onPlay,
                onOpenPlaylist = { playlistId ->
                    // Mask-first: the shell's transition mask covers the
                    // content area before the new screen composes.
                    onNavigate()
                    navController.navigate(Routes.playlistDetail(playlistId))
                },
                onOpenMadeForYou = {
                    onNavigate()
                    navController.navigate(Routes.MADE_FOR_YOU)
                }
            )
        }
        composable(Routes.MADE_FOR_YOU) {
            PlaylistDetailScreen(
                playlistId = -1L,
                musicController = musicController,
                onBack = {
                    onNavigate()
                    navController.navigateUp()
                },
                virtualName = "Made for you playlist",
                virtualDescription = "A fresh mix of 50 songs from your library.",
                virtualTracks = madeForYouTracks
            )
        }
        composable(Routes.SEARCH) {
            SearchScreen(
                onOpenDownloads = {
                    onNavigate()
                    navController.navigate(Routes.DOWNLOADS) {
                        launchSingleTop = true
                    }
                }
            )
        }
        composable(Routes.EXPORT_IMPORT) {
            ExportImportScreen(
                onBack = {
                    onNavigate()
                    navController.navigateUp()
                }
            )
        }
        composable(Routes.LIBRARY) {
            LibraryScreen(
                musicController = musicController,
                onPlay = onPlay,
                onExportTracks = onExportTracks,
                onExportPlaylists = onExportPlaylists,
                onOpenPlaylist = { playlistId ->
                    onNavigate()
                    navController.navigate(Routes.playlistDetail(playlistId))
                }
            )
        }
        composable(Routes.DISCOVER) {
            DiscoverScreen(
                musicController = musicController,
                onLoadingChanged = onDiscoverLoadingChanged
            )
        }
        composable(Routes.DOWNLOADS) {
            DownloadsScreen()
        }
        composable(Routes.RECENTS) {
            RecentsScreen(
                musicController = musicController,
                onBack = {
                    onNavigate()
                    navController.navigateUp()
                }
            )
        }
        composable(Routes.DUPLICATES) {
            DuplicateScreen(
                onBack = {
                    onNavigate()
                    navController.navigateUp()
                }
            )
        }
        composable(Routes.FULL_PLAYER) {
            FullPlayerScreen(
                musicController = musicController,
                onBack = {
                    onNavigate()
                    navController.navigateUp()
                }
            )
        }
        composable(
            route = Routes.PLAYLIST_DETAIL,
            arguments = listOf(navArgument("playlistId") { type = NavType.LongType })
        ) { backStackEntry ->
            val playlistId = backStackEntry.arguments?.getLong("playlistId") ?: -1L
            PlaylistDetailScreen(
                playlistId = playlistId,
                musicController = musicController,
                onBack = {
                    onNavigate()
                    navController.navigateUp()
                },
                onExportPlaylist = { id -> onExportPlaylists(listOf(id)) },
                onExportTracks = onExportTracks
            )
        }
    }
}
