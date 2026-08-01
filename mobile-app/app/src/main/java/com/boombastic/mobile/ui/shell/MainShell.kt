package com.boombastic.mobile.ui.shell

import android.Manifest
import android.os.Build
import android.os.SystemClock
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.BugReport
import androidx.compose.material.icons.filled.DeleteSweep
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.History
import androidx.compose.material.icons.filled.Image
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Key
import androidx.compose.material.icons.filled.SystemUpdate
import androidx.compose.material.icons.filled.UploadFile
import androidx.compose.material3.CircularProgressIndicator
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
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.vectorResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import com.boombastic.mobile.R
import com.boombastic.mobile.playback.MediaTrack
import com.boombastic.mobile.playback.MusicController
import com.boombastic.mobile.playback.NotificationPermissionPolicy
import com.boombastic.mobile.ui.components.MiniPlayer
import com.boombastic.mobile.ui.create.CreatePlaylistSheet
import com.boombastic.mobile.ui.discover.LastfmKeyDialog
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
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** How long the green-loader transition mask stays fully visible (the
 *  "black screen" moment with the spinner). */
private const val TRANSITION_MASK_HOLD_MS = 300L

/** How long the transition mask takes to fade away (revealing the screen). */
private const val TRANSITION_MASK_FADE_MS = 400

/** Hard cap for the mask while waiting on first-launch data — beyond this
 *  the reveal happens even if the library is still loading. */
private const val TRANSITION_MASK_MAX_MS = 6_000L

private data class BottomNavItem(
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
    // Drawer settings state: artwork-fetch busy flag + clear-history confirm.
    var fetchingArtwork by remember { mutableStateOf(false) }
    var showClearHistoryConfirm by remember { mutableStateOf(false) }
    var showErrorLog by remember { mutableStateOf(false) }
    var showLastfmKeyDialog by remember { mutableStateOf(false) }
    val hasActiveItem by musicController.hasActiveItem.collectAsState()
    val currentBackStack by navController.currentBackStackEntryAsState()
    val currentRoute = currentBackStack?.destination?.route
    val snackbarHostState = remember { SnackbarHostState() }
    val drawerState = rememberDrawerState(initialValue = DrawerValue.Closed)
    val scope = rememberCoroutineScope()

    // Green-loader transition mask.  It is set to `true` BEFORE every
    // navigation (see the call sites below) so the black layer with the
    // spinner is already opaque when the new screen composes — the screen
    // can never flash in early.  The release waits for a deliberate
    // "black screen" moment AND for the library data to be loaded, so the
    // reveal always lands on a fully rendered screen (no loading gate
    // popping in after the spinner).
    var transitionMask by remember { mutableStateOf(false) }
    val app = context.applicationContext as com.boombastic.mobile.BoomBasticApp
    val libraryLoaded by app.libraryData.loaded.collectAsState()
    LaunchedEffect(transitionMask) {
        if (!transitionMask) return@LaunchedEffect
        val startedAt = SystemClock.elapsedRealtime()
        while (
            SystemClock.elapsedRealtime() - startedAt < TRANSITION_MASK_HOLD_MS ||
            (!libraryLoaded && SystemClock.elapsedRealtime() - startedAt < TRANSITION_MASK_MAX_MS)
        ) {
            delay(16)
        }
        transitionMask = false
    }

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

    // Music-folder destination picker (Settings drawer) — sets the folder
    // and imports it desktop-style (subfolders become playlists).  Live
    // progress is shown in the shell (progress strip under the header);
    // re-imports of a large folder take a while, so silence looks broken.
    // The import itself, its stall watchdog and the strip's final
    // "Import complete!" state are owned by MusicFolderImportManager
    // (process-lifetime), so the strip can never sit frozen at the final
    // counts.
    val importManager = remember { app.musicFolderImportManager }
    val importStatus by importManager.status.collectAsState()
    val folderImportLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.OpenDocumentTree()
    ) { uri: android.net.Uri? ->
        if (uri != null) {
            importManager.start(uri)
        }
    }

    // Single ActivityResult permission launcher — survives recomposition.
    val notificationPermissionLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.RequestPermission()
    ) { /* grant result intentionally ignored — playback already dispatched */ }

    // Stable onPlay callback used by SearchScreen, LibraryScreen and
    // HomeScreen.  Accepts the FULL playback context (ordered track list +
    // start index) so next/previous/shuffle work relative to where the
    // track was picked from (search results, all songs, carousel, ...).
    val onPlay: (List<MediaTrack>, Int) -> Unit = remember(policy, notificationPermissionLauncher, musicController) {
        { tracks: List<MediaTrack>, startIndex: Int ->
            if (tracks.isNotEmpty()) {
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
                musicController.play(tracks, startIndex)
            }
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
                        transitionMask = true
                        navController.navigate(Routes.DOWNLOADS) {
                            launchSingleTop = true
                        }
                    }
                )
                DrawerItem(
                    icon = Icons.Filled.Folder,
                    label = "Music folder",
                    onClick = {
                        scope.launch { drawerState.close() }
                        folderImportLauncher.launch(null)
                    }
                )
                DrawerItem(
                    icon = Icons.Filled.Key,
                    label = "Last.fm API key",
                    onClick = {
                        scope.launch { drawerState.close() }
                        showLastfmKeyDialog = true
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
                    icon = Icons.Filled.Image,
                    label = if (fetchingArtwork) "Fetching artwork…" else "Fetch missing artwork",
                    onClick = {
                        if (fetchingArtwork) return@DrawerItem
                        scope.launch { drawerState.close() }
                        fetchingArtwork = true
                        app.appScope.launch {
                            val updated = app.libraryRepository.fetchMissingArtwork()
                            // appScope runs on Dispatchers.Default — every UI
                            // touch (Compose state + toast) must return to the
                            // main thread first (toasting off the main thread
                            // crashes: "Can't toast on a thread that has not
                            // called Looper.prepare()").
                            withContext(Dispatchers.Main) {
                                fetchingArtwork = false
                                val message = if (updated > 0) {
                                    "Artwork fetched for $updated track(s)"
                                } else {
                                    "All tracks already have artwork"
                                }
                                Toast.makeText(context, message, Toast.LENGTH_SHORT).show()
                            }
                        }
                    }
                )
                DrawerItem(
                    icon = Icons.Filled.History,
                    label = "Recently played",
                    onClick = {
                        scope.launch { drawerState.close() }
                        transitionMask = true
                        navController.navigate(Routes.RECENTS) {
                            launchSingleTop = true
                        }
                    }
                )
                DrawerItem(
                    icon = Icons.Filled.DeleteSweep,
                    label = "Clear recent history",
                    onClick = {
                        scope.launch { drawerState.close() }
                        showClearHistoryConfirm = true
                    }
                )
                DrawerItem(
                    icon = Icons.Filled.Info,
                    label = "About",
                    onClick = {
                        scope.launch { drawerState.close() }
                        Toast.makeText(context, "Luno v0.1.0", Toast.LENGTH_SHORT).show()
                    }
                )
                DrawerItem(
                    icon = Icons.Filled.BugReport,
                    label = "Error log",
                    onClick = {
                        scope.launch { drawerState.close() }
                        showErrorLog = true
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
                                    onMiniPlayerTap = {
                                        transitionMask = true
                                        navController.navigate(Routes.FULL_PLAYER)
                                    }
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
                                            // Mask FIRST: the black layer
                                            // is opaque before the new
                                            // screen composes, so it never
                                            // flashes in early.
                                            transitionMask = true
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
                Column(
                    modifier = Modifier
                        .fillMaxSize()
                        .padding(innerPadding)
                ) {
                    // App header: "Luno" with the small green bar to its
                    // right (desktop sidebar logo style).  Hidden on the
                    // full player, which is truly full-screen.  The title
                    // itself is a button: tapping it opens the Settings
                    // drawer, like the Home green-circle icon.
                    if (currentRoute != Routes.FULL_PLAYER) {
                        AppHeader(onClick = { scope.launch { drawerState.open() } })
                    }
                    // Live music-folder import progress (Settings drawer
                    // flow) — re-imports of large folders take a while, so
                    // show that work is happening.  The bar is a custom
                    // smooth sweep: m3 1.2.1's built-in indeterminate
                    // indicator snaps back at its loop point.  When the
                    // import finishes the green text flips to "Import
                    // complete!" and auto-clears after a couple of seconds
                    // (the manager owns that lifecycle).
                    importStatus?.let { status ->
                        Column(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(horizontal = Dimens.paddingLarge)
                        ) {
                            when (status) {
                                is FolderImportStatus.Importing -> {
                                    SmoothProgressBar(
                                        modifier = Modifier
                                            .fillMaxWidth()
                                            .height(Dimens.progressBarHeight)
                                    )
                                    Text(
                                        text = "Importing music folder… ${status.imported} added, " +
                                            "${status.duplicates} duplicates, ${status.errors} errors",
                                        color = AccentGreen,
                                        style = MaterialTheme.typography.bodySmall,
                                        modifier = Modifier.padding(vertical = Dimens.paddingSmall)
                                    )
                                }
                                is FolderImportStatus.Finished -> {
                                    Text(
                                        text = when {
                                            status.failed ->
                                                "Import failed — please try again" +
                                                    status.errorMessage?.let { " ($it)" }.orEmpty()
                                            status.stalled ->
                                                "Import is taking longer than expected… still working"
                                            else ->
                                                "Import complete! ${status.imported} added, " +
                                                    "${status.duplicates} duplicates, " +
                                                    "${status.errors} errors${status.persistWarning.orEmpty()}"
                                        },
                                        color = if (status.failed || status.stalled) {
                                            MaterialTheme.colorScheme.error
                                        } else {
                                            AccentGreen
                                        },
                                        style = MaterialTheme.typography.bodySmall,
                                        modifier = Modifier.padding(vertical = Dimens.paddingSmall)
                                    )
                                }
                            }
                        }
                    }
                    Box(
                        modifier = Modifier
                            .fillMaxWidth()
                            .weight(1f)
                    ) {
                        BoomBasticNavHost(
                            navController = navController,
                            musicController = musicController,
                            modifier = Modifier.fillMaxSize(),
                            onCreatePlaylist = { showCreateSheet = true },
                            onPlay = onPlay,
                            onNavigate = { transitionMask = true }
                        )
                        // Green-loader transition mask (see above): covers
                        // the content area while the new screen fades in
                        // underneath, then fades away to reveal it.
                        TransitionMask(visible = transitionMask)
                    }
                }
            }

            // Create Playlist modal
            if (showCreateSheet) {
                CreatePlaylistSheet(
                    onDismiss = { showCreateSheet = false }
                )
            }

            // Clear recent-history confirm (Settings drawer) — desktop
            // "Clear History" action; in-session history only.
            if (showClearHistoryConfirm) {
                androidx.compose.material3.AlertDialog(
                    onDismissRequest = { showClearHistoryConfirm = false },
                    containerColor = SurfaceDark,
                    titleContentColor = PrimaryText,
                    textContentColor = SecondaryText,
                    title = { Text("Clear recent history?") },
                    text = { Text("Recently played tracks will be removed from Home. This cannot be undone.") },
                    confirmButton = {
                        androidx.compose.material3.TextButton(onClick = {
                            musicController.clearRecentlyPlayed()
                            showClearHistoryConfirm = false
                            Toast.makeText(context, "Recent history cleared", Toast.LENGTH_SHORT).show()
                        }) {
                            Text("Clear", color = AccentGreen)
                        }
                    },
                    dismissButton = {
                        androidx.compose.material3.TextButton(onClick = { showClearHistoryConfirm = false }) {
                            Text("Cancel", color = SecondaryText)
                        }
                    }
                )
            }

            // Last.fm API key (Settings drawer + Discover) — stored
            // encrypted via DiscoveryRepository; never logged or exported.
            if (showLastfmKeyDialog) {
                val currentKey by app.discoveryRepository.apiKey.collectAsState()
                LastfmKeyDialog(
                    currentKey = currentKey,
                    onSave = { key ->
                        app.discoveryRepository.setApiKey(key)
                        showLastfmKeyDialog = false
                    },
                    onClear = {
                        app.discoveryRepository.clearApiKey()
                        showLastfmKeyDialog = false
                    },
                    onDismiss = { showLastfmKeyDialog = false }
                )
            }

            // Error log (Settings drawer) — shows the last captured crash
            // stack from crash_log.txt, so a crash can be reported without
            // logcat (desktop error_log-view parity).
            if (showErrorLog) {
                val crashLog = remember {
                    val file = File(context.filesDir, "crash_log.txt")
                    if (file.exists()) file.readText() else ""
                }
                androidx.compose.material3.AlertDialog(
                    onDismissRequest = { showErrorLog = false },
                    containerColor = SurfaceDark,
                    titleContentColor = PrimaryText,
                    textContentColor = PrimaryText,
                    title = { Text("Error log") },
                    text = {
                        Text(
                            text = crashLog.ifBlank { "No crashes logged yet." },
                            style = MaterialTheme.typography.bodySmall,
                            color = SecondaryText,
                            modifier = Modifier
                                .fillMaxWidth()
                                .heightIn(max = 360.dp)
                                .verticalScroll(rememberScrollState())
                        )
                    },
                    confirmButton = {
                        androidx.compose.material3.TextButton(onClick = { showErrorLog = false }) {
                            Text("Close", color = AccentGreen)
                        }
                    }
                )
            }
        }
    }
}

/**
 * Indeterminate progress bar with a perfectly smooth left-to-right sweep.
 * The phase is a pure function of wall-clock time (`withFrameNanos`), so
 * dropped frames during heavy scanning shift the bar forward instead of
 * making it jump — a repeating tween can visibly skip at its loop point
 * when frames are missed.  The bar is fully off-screen at both ends of
 * the loop, so the wrap-around is invisible.
 */
@Composable
private fun SmoothProgressBar(
    modifier: Modifier = Modifier,
    color: Color = AccentGreen,
    trackColor: Color = SurfaceDark
) {
    val sweepMillis = 1100f
    var phase by remember { mutableStateOf(0f) }
    LaunchedEffect(Unit) {
        var lastNanos = 0L
        while (true) {
            withFrameNanos { nanos ->
                if (lastNanos != 0L) {
                    phase = ((nanos / 1_000_000f) / sweepMillis) % 1f
                }
                lastNanos = nanos
            }
        }
    }
    Canvas(modifier = modifier) {
        drawRect(color = trackColor)
        val barWidth = size.width * 0.3f
        val x = phase * (size.width + barWidth) - barWidth
        drawRoundRect(
            color = color,
            topLeft = Offset(x, 0f),
            size = Size(barWidth, size.height),
            cornerRadius = CornerRadius(size.height / 2f, size.height / 2f)
        )
    }
}

/**
 * Green-loader transition mask: a solid full-area layer with the centered
 * spinner.  It appears instantly on a route change, holds for a deliberate
 * "black screen" moment while the new screen (swapped in instantly by the
 * NavHost) sits fully rendered underneath, and then fades away smoothly
 * (FastOutSlowIn) to reveal it — the switch reads as one calm motion
 * instead of a rush of overlapping fades.  Kept in its own composable so
 * the plain (non-scope-extension) AnimatedVisibility resolves correctly
 * inside the nested Box.
 */
@Composable
private fun TransitionMask(visible: Boolean) {
    AnimatedVisibility(
        visible = visible,
        // No enter animation: the layer must be fully opaque in the very
        // first frame it appears, or the screen underneath flashes through.
        enter = EnterTransition.None,
        exit = fadeOut(
            animationSpec = tween(
                TRANSITION_MASK_FADE_MS,
                easing = FastOutSlowInEasing
            )
        )
    ) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(PrimaryBackground),
            contentAlignment = Alignment.Center
        ) {
            CircularProgressIndicator(color = AccentGreen)
        }
    }
}

/**
 * Desktop-style app header: bold "Luno" wordmark with the small green
 * rounded bar (the desktop's green "▮") immediately to its right,
 * vertically centered with the title text.  The whole title is tappable
 * ([onClick]) and opens the Settings drawer.
 */
@Composable
private fun AppHeader(onClick: () -> Unit) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .clip(RoundedCornerShape(Dimens.cornerMedium))
            .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = onClick
            )
            .padding(
                start = Dimens.paddingLarge,
                top = Dimens.paddingMedium,
                bottom = Dimens.paddingSmall,
                end = Dimens.paddingMedium
            )
    ) {
        Text(
            text = "Luno",
            style = MaterialTheme.typography.headlineLarge,
            fontWeight = FontWeight.Bold,
            color = PrimaryText
        )
        Spacer(modifier = Modifier.width(6.dp))
        Box(
            modifier = Modifier
                .size(width = 6.dp, height = 16.dp)
                .clip(RoundedCornerShape(2.dp))
                .background(AccentGreen)
        )
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
            .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = onClick
            )
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
