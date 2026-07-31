package com.boombastic.mobile.ui.shell

import android.Manifest
import android.os.Build
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.SystemUpdate
import androidx.compose.material.icons.filled.UploadFile
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalDrawerSheet
import androidx.compose.material3.ModalNavigationDrawer
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.rememberDrawerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
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
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText
import com.boombastic.mobile.ui.theme.SurfaceDark
import kotlinx.coroutines.launch

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
    val drawerState = rememberDrawerState(initialValue = DrawerValue.Closed)
    val scope = rememberCoroutineScope()

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

    ModalNavigationDrawer(
        drawerState = drawerState,
        drawerContent = {
            ModalDrawerSheet(
                drawerContainerColor = SurfaceDark,
                modifier = Modifier.fillMaxWidth(0.8f)
            ) {
                // Drawer header
                Text(
                    text = "Settings",
                    style = MaterialTheme.typography.headlineSmall,
                    color = PrimaryText,
                    modifier = Modifier.padding(
                        start = Dimens.paddingLarge,
                        top = Dimens.paddingXLarge,
                        bottom = Dimens.paddingLarge
                    )
                )

                // Local-function drawer — never contains cloud accounts.
                DrawerItem(
                    icon = Icons.Filled.Download,
                    label = "Downloads",
                    onClick = {
                        scope.launch { drawerState.close() }
                        navController.navigate(Routes.DOWNLOADS) {
                            launchSingleTop = true
                        }
                    }
                )
                DrawerItem(
                    icon = Icons.Filled.UploadFile,
                    label = "Export / Import",
                    onClick = {
                        scope.launch { drawerState.close() }
                        Toast.makeText(context, "Export / Import coming soon", Toast.LENGTH_SHORT).show()
                    }
                )
                DrawerItem(
                    icon = Icons.Filled.Info,
                    label = "About",
                    onClick = {
                        scope.launch { drawerState.close() }
                        Toast.makeText(context, "BoomBastic v0.1.0", Toast.LENGTH_SHORT).show()
                    }
                )
                DrawerItem(
                    icon = Icons.Filled.SystemUpdate,
                    label = "Check for updates",
                    onClick = {
                        scope.launch { drawerState.close() }
                        Toast.makeText(context, "Update check coming soon", Toast.LENGTH_SHORT).show()
                    }
                )
            }
        }
    ) {
        Box(modifier = Modifier.fillMaxSize()) {
            Scaffold(
                containerColor = PrimaryBackground,
                snackbarHost = { SnackbarHost(snackbarHostState) },
                bottomBar = {
                    // Full player is truly full-screen — hide mini player + nav bar.
                    if (currentRoute != Routes.FULL_PLAYER) {
                        Column {
                            // Mini player
                            AnimatedVisibility(
                                visible = hasActiveItem,
                                enter = slideInVertically(initialOffsetY = { it }),
                                exit = slideOutVertically(targetOffsetY = { it })
                            ) {
                                MiniPlayer(
                                    musicController = musicController,
                                    onMiniPlayerTap = { navController.navigate(Routes.FULL_PLAYER) }
                                )
                            }

                            // Bottom navigation: Home / Search / Library /
                            // Discover / Create.  Downloads and other local
                            // functions live in the options drawer.
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
                                            if (item.route == Routes.HOME) {
                                                // Home always returns to the Home screen,
                                                // even from drawer/deep routes (Downloads,
                                                // playlist detail, etc.).
                                                val popped = navController.popBackStack(
                                                    Routes.HOME,
                                                    inclusive = false
                                                )
                                                if (!popped) {
                                                    navController.navigate(Routes.HOME) {
                                                        launchSingleTop = true
                                                    }
                                                }
                                            } else {
                                                navController.navigate(item.route) {
                                                    popUpTo(navController.graph.startDestinationId) {
                                                        saveState = true
                                                    }
                                                    launchSingleTop = true
                                                    restoreState = true
                                                }
                                            }
                                        },
                                        icon = {
                                            Icon(
                                                imageVector = ImageVector.vectorResource(id = item.icon),
                                                contentDescription = item.label,
                                                // Search (and Create below) render at 28dp — their
                                                // glyphs are optically smaller than the other tabs.
                                                modifier = Modifier.size(
                                                    if (item.route == Routes.SEARCH) {
                                                        Dimens.iconSizeMedium
                                                    } else {
                                                        Dimens.bottomNavIconSize
                                                    }
                                                )
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
                                            modifier = Modifier.size(Dimens.iconSizeLarge)
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
                }
            ) { innerPadding ->
                BoomBasticNavHost(
                    navController = navController,
                    musicController = musicController,
                    modifier = Modifier.padding(innerPadding),
                    onCreatePlaylist = { showCreateSheet = true },
                    onPlay = onPlay,
                    onOpenOptions = { scope.launch { drawerState.open() } }
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
}

@Composable
private fun DrawerItem(
    icon: ImageVector,
    label: String,
    onClick: () -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(Dimens.cornerMedium))
            .clickable(onClick = onClick)
            .padding(vertical = Dimens.paddingMedium, horizontal = Dimens.paddingLarge),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            tint = SecondaryText,
            modifier = Modifier.size(Dimens.iconSize)
        )
        Spacer(modifier = Modifier.width(Dimens.paddingLarge))
        Text(
            text = label,
            style = MaterialTheme.typography.bodyLarge,
            color = PrimaryText
        )
    }
}
