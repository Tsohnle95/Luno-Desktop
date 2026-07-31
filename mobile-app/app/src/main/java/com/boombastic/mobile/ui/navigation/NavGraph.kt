package com.boombastic.mobile.ui.navigation

import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.navigation.NavHostController
import androidx.navigation.NavType
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.navArgument
import com.boombastic.mobile.playback.MediaTrack
import com.boombastic.mobile.playback.MusicController
import com.boombastic.mobile.ui.discover.DiscoverScreen
import com.boombastic.mobile.ui.downloads.DownloadsScreen
import com.boombastic.mobile.ui.home.HomeScreen
import com.boombastic.mobile.ui.library.LibraryScreen
import com.boombastic.mobile.ui.library.PlaylistDetailScreen
import com.boombastic.mobile.ui.player.FullPlayerScreen
import com.boombastic.mobile.ui.search.SearchScreen

object Routes {
    const val HOME = "home"
    const val SEARCH = "search"
    const val LIBRARY = "library"
    const val DISCOVER = "discover"
    const val DOWNLOADS = "downloads"
    const val FULL_PLAYER = "full_player"

    const val PLAYLIST_DETAIL = "playlist/{playlistId}"
    fun playlistDetail(playlistId: Long) = "playlist/$playlistId"
}

@Composable
fun BoomBasticNavHost(
    navController: NavHostController,
    musicController: MusicController,
    modifier: Modifier = Modifier,
    onCreatePlaylist: () -> Unit,
    onPlay: (MediaTrack) -> Unit = {}
) {
    NavHost(
        navController = navController,
        startDestination = Routes.HOME,
        modifier = modifier,
        // Quick fades: short durations with minimal overlap keep tab
        // switches snappy — long 350ms cross-fades over heavy screens
        // (4k-track library) felt laggy.  Enter is eased, exit is linear
        // so the outgoing screen fades out while the new one fades in.
        enterTransition = { fadeIn(animationSpec = tween(150, easing = FastOutSlowInEasing)) },
        exitTransition = { fadeOut(animationSpec = tween(100, easing = LinearEasing)) },
        popEnterTransition = { fadeIn(animationSpec = tween(150, easing = FastOutSlowInEasing)) },
        popExitTransition = { fadeOut(animationSpec = tween(100, easing = LinearEasing)) }
    ) {
        composable(Routes.HOME) {
            HomeScreen(
                musicController = musicController,
                onPlay = onPlay,
                onOpenPlaylist = { playlistId ->
                    navController.navigate(Routes.playlistDetail(playlistId))
                }
            )
        }
        composable(Routes.SEARCH) {
            SearchScreen(musicController = musicController, onPlay = onPlay)
        }
        composable(Routes.LIBRARY) {
            LibraryScreen(
                musicController = musicController,
                onPlay = onPlay,
                onOpenPlaylist = { playlistId ->
                    navController.navigate(Routes.playlistDetail(playlistId))
                }
            )
        }
        composable(Routes.DISCOVER) {
            DiscoverScreen()
        }
        composable(Routes.DOWNLOADS) {
            DownloadsScreen()
        }
        composable(Routes.FULL_PLAYER) {
            FullPlayerScreen(
                musicController = musicController,
                onBack = { navController.navigateUp() }
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
                onBack = { navController.navigateUp() }
            )
        }
    }
}
