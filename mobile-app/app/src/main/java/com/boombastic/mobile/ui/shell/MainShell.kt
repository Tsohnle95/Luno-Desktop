package com.boombastic.mobile.ui.shell

import android.Manifest
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.vectorResource
import androidx.compose.ui.unit.dp
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import com.boombastic.mobile.R
import com.boombastic.mobile.playback.MediaTrack
import com.boombastic.mobile.playback.MusicController
import com.boombastic.mobile.playback.NotificationPermissionPolicy
import com.boombastic.mobile.ui.components.MiniPlayer
import com.boombastic.mobile.ui.create.CreatePlaylistSheet
import com.boombastic.mobile.ui.navigation.BoomBasticNavHost
import com.boombastic.mobile.ui.navigation.Routes
import com.boombastic.mobile.ui.theme.AccentGreen
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.NavBarSurface
import com.boombastic.mobile.ui.theme.NavBarUnselected
import com.boombastic.mobile.ui.theme.PrimaryBackground

data class BottomNavItem(
    val label: String,
    val icon: Int,
    val route: String
)

private val bottomNavItems = listOf(
    BottomNavItem("Home", R.drawable.ic_home, Routes.HOME),
    BottomNavItem("Search", R.drawable.ic_search, Routes.SEARCH),
    BottomNavItem("Your Library", R.drawable.ic_library, Routes.LIBRARY),
    BottomNavItem("Discover", R.drawable.ic_discover, Routes.DISCOVER)
)

@Composable
fun MainShell(musicController: MusicController) {
    val context = LocalContext.current
    val navController = rememberNavController()
    var showCreateSheet by rememberSaveable { mutableStateOf(false) }
    val hasActiveItem by musicController.hasActiveItem.collectAsState()
    val currentBackStack by navController.currentBackStackEntryAsState()
    val currentRoute = currentBackStack?.destination?.route
    val snackbarHostState = remember { SnackbarHostState() }

    // ── Error Snackbar collection ───────────────────────────────────────────
    LaunchedEffect(Unit) {
        musicController.connectionError.collect { error ->
            snackbarHostState.showSnackbar(
                message = error.message ?: "Connection failed"
            )
        }
    }

    LaunchedEffect(Unit) {
        musicController.playbackError.collect { error ->
            snackbarHostState.showSnackbar(
                message = error.message
            )
        }
    }

    // ── Centralised one-shot notification-prompt policy ──────────────────────
    val policy = remember { NotificationPermissionPolicy.create(context) }

    // Single ActivityResult permission launcher — survives recomposition.
    val notificationPermissionLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.RequestPermission()
    ) { /* grant result intentionally ignored — playback already dispatched */ }

    // Stable onPlay callback used by SearchScreen and LibraryScreen.
    // Accepts full MediaTrack metadata so MediaItems carry accurate data.
    val onPlay: (MediaTrack) -> Unit = remember(policy, notificationPermissionLauncher, musicController) {
        { track: MediaTrack ->
            if (policy.shouldPrompt(
                    sdkInt = Build.VERSION.SDK_INT,
                    isGranted = policy.isGranted(context)
                )
            ) {
                policy.recordPromptAttempted()
                notificationPermissionLauncher.launch(Manifest.permission.POST_NOTIFICATIONS)
            }
            // Playback proceeds regardless of permission state — per Android docs
            // media-session notifications are exempt from POST_NOTIFICATIONS.
            musicController.play(track)
        }
    }

    Box(modifier = Modifier.fillMaxSize()) {
        Scaffold(
            containerColor = PrimaryBackground,
            snackbarHost = { SnackbarHost(snackbarHostState) },
            bottomBar = {
                Column {
                    // Mini player
                    AnimatedVisibility(
                        visible = hasActiveItem,
                        enter = slideInVertically(initialOffsetY = { it }),
                        exit = slideOutVertically(targetOffsetY = { it })
                    ) {
                        MiniPlayer(musicController = musicController)
                    }

                    // Bottom navigation
                    NavigationBar(
                        containerColor = NavBarSurface,
                        tonalElevation = 0.dp
                    ) {
                        bottomNavItems.forEach { item ->
                            val selected = currentRoute == item.route
                            NavigationBarItem(
                                selected = selected,
                                onClick = {
                                    if (item.route == currentRoute) return@NavigationBarItem
                                    navController.navigate(item.route) {
                                        popUpTo(navController.graph.startDestinationId) {
                                            saveState = true
                                        }
                                        launchSingleTop = true
                                        restoreState = true
                                    }
                                },
                                icon = {
                                    Icon(
                                        imageVector = ImageVector.vectorResource(id = item.icon),
                                        contentDescription = item.label,
                                        modifier = Modifier.padding(Dimens.paddingSmall)
                                    )
                                },
                                label = {
                                    Text(
                                        text = item.label,
                                        style = MaterialTheme.typography.labelSmall
                                    )
                                },
                                colors = NavigationBarItemDefaults.colors(
                                    selectedIconColor = AccentGreen,
                                    selectedTextColor = AccentGreen,
                                    unselectedIconColor = NavBarUnselected,
                                    unselectedTextColor = NavBarUnselected,
                                    indicatorColor = NavBarSurface
                                )
                            )
                        }

                        // Create action item (does not navigate)
                        NavigationBarItem(
                            selected = false,
                            onClick = { showCreateSheet = true },
                            icon = {
                                Icon(
                                    imageVector = ImageVector.vectorResource(id = R.drawable.ic_create),
                                    contentDescription = "Create",
                                    modifier = Modifier.padding(Dimens.paddingSmall)
                                )
                            },
                            label = {
                                Text(
                                    text = "Create",
                                    style = MaterialTheme.typography.labelSmall
                                )
                            },
                            colors = NavigationBarItemDefaults.colors(
                                unselectedIconColor = NavBarUnselected,
                                unselectedTextColor = NavBarUnselected,
                                indicatorColor = NavBarSurface
                            )
                        )
                    }
                }
            }
        ) { innerPadding ->
            BoomBasticNavHost(
                navController = navController,
                musicController = musicController,
                modifier = Modifier.padding(innerPadding),
                onCreatePlaylist = { showCreateSheet = true },
                onPlay = onPlay
            )
        }

        // Create Playlist modal
        if (showCreateSheet) {
            CreatePlaylistSheet(
                onDismiss = { showCreateSheet = false }
            )
        }
    }
}
