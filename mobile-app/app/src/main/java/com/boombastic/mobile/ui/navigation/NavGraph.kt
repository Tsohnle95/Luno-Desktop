package com.boombastic.mobile.ui.navigation

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import com.boombastic.mobile.playback.MediaTrack
import com.boombastic.mobile.playback.MusicController
import com.boombastic.mobile.ui.discover.DiscoverScreen
import com.boombastic.mobile.ui.downloads.DownloadsScreen
import com.boombastic.mobile.ui.home.HomeScreen
import com.boombastic.mobile.ui.library.LibraryScreen
import com.boombastic.mobile.ui.player.FullPlayerScreen
import com.boombastic.mobile.ui.search.SearchScreen

object Routes {
    const val HOME = "home"
    const val SEARCH = "search"
    const val LIBRARY = "library"
    const val DISCOVER = "discover"
    const val DOWNLOADS = "downloads"
    const val FULL_PLAYER = "full_player"
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
        modifier = modifier
    ) {
        composable(Routes.HOME) {
            HomeScreen(musicController = musicController)
        }
        composable(Routes.SEARCH) {
            SearchScreen(musicController = musicController, onPlay = onPlay)
        }
        composable(Routes.LIBRARY) {
            LibraryScreen(musicController = musicController, onPlay = onPlay)
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
    }
}
