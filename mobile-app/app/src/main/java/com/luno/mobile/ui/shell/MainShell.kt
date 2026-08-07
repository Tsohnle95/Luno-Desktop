package com.luno.mobile.ui.shell

import android.Manifest
import android.content.Intent
import android.net.Uri
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
import androidx.compose.foundation.layout.fillMaxHeight
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
import androidx.compose.material.icons.filled.ExpandLess
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.History
import androidx.compose.material.icons.filled.Image
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Key
import androidx.compose.material.icons.filled.SystemUpdate
import androidx.compose.material.icons.filled.Sync
import androidx.compose.material.icons.filled.UploadFile
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
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
import androidx.compose.material3.TextButton
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
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.vectorResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import com.luno.mobile.BuildConfig
import com.luno.mobile.R
import com.luno.mobile.data.update.GitHubReleaseService
import com.luno.mobile.data.update.ReleaseCheckResult
import com.luno.mobile.data.update.ApkInstallResult
import com.luno.mobile.data.update.ApkInstaller
import com.luno.mobile.data.export.LibraryManifest
import com.luno.mobile.playback.MediaTrack
import com.luno.mobile.playback.MusicController
import com.luno.mobile.playback.NotificationPermissionPolicy
import com.luno.mobile.ui.components.MiniPlayer
import com.luno.mobile.ui.create.CreatePlaylistSheet
import com.luno.mobile.ui.discover.LastfmKeyDialog
import com.luno.mobile.ui.navigation.LunoNavHost
import com.luno.mobile.ui.navigation.Routes
import com.luno.mobile.ui.theme.AccentGreen
import com.luno.mobile.ui.theme.AppBackgroundGreen
import com.luno.mobile.ui.theme.Dimens
import com.luno.mobile.ui.theme.NavBarSurface
import com.luno.mobile.ui.theme.NavBarUnselected
import com.luno.mobile.ui.theme.PrimaryBackground
import com.luno.mobile.ui.theme.PrimaryText
import com.luno.mobile.ui.theme.SecondaryText
import com.luno.mobile.ui.theme.SurfaceDark
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
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
    BottomNavItem("Download", R.drawable.ic_download, Routes.SEARCH),
    BottomNavItem("Your Library", R.drawable.ic_library, Routes.LIBRARY),
    BottomNavItem("Discover", R.drawable.ic_discover, Routes.DISCOVER)
)

private sealed interface UpdateDialogState {
    data object Checking : UpdateDialogState
    data class Result(val value: ReleaseCheckResult) : UpdateDialogState
}

@Composable
fun MainShell(
    musicController: MusicController,
    notificationOpenRequests: StateFlow<Long> = MutableStateFlow(0L)
) {
    val context = LocalContext.current
    val navController = rememberNavController()
    var showCreateSheet by rememberSaveable { mutableStateOf(false) }
    // Drawer settings state: clear-history confirm + error log dialog.
    var showClearHistoryConfirm by remember { mutableStateOf(false) }
    var showErrorLog by remember { mutableStateOf(false) }
    var showLastfmKeyDialog by remember { mutableStateOf(false) }
    var updateDialogState by remember { mutableStateOf<UpdateDialogState?>(null) }
    var showSideloadWarning by remember { mutableStateOf(false) }
    var pendingReleaseUrl by remember { mutableStateOf<String?>(null) }
    var pendingManifestExport by remember { mutableStateOf<LibraryManifest?>(null) }
    var manifestExportBusy by remember { mutableStateOf(false) }
    var expandedSettingsSection by rememberSaveable { mutableStateOf<String?>(null) }
    val hasActiveItem by musicController.hasActiveItem.collectAsState()
    val currentBackStackState = navController.currentBackStackEntryAsState()
    val currentBackStack by currentBackStackState
    val currentRoute = currentBackStack?.destination?.route
    val snackbarHostState = remember { SnackbarHostState() }
    val drawerState = rememberDrawerState(initialValue = DrawerValue.Closed)
    val scope = rememberCoroutineScope()
    val releaseService = remember { GitHubReleaseService() }
    val updatePreferences = remember {
        context.getSharedPreferences("github_releases", android.content.Context.MODE_PRIVATE)
    }

    fun openReleaseUrl(url: String) {
        val uri = runCatching { Uri.parse(url) }.getOrNull()
        if (uri?.scheme != "https" || uri.host.isNullOrBlank()) {
            Toast.makeText(context, "Release link is not valid", Toast.LENGTH_SHORT).show()
            return
        }
        runCatching {
            context.startActivity(Intent(Intent.ACTION_VIEW, uri))
        }.onFailure {
            Toast.makeText(context, "No browser available", Toast.LENGTH_SHORT).show()
        }
    }

    fun downloadAndInstallApk(url: String) {
        updateDialogState = null
        scope.launch {
            Toast.makeText(context, "Downloading update…", Toast.LENGTH_SHORT).show()
            when (val result = withContext(Dispatchers.IO) {
                ApkInstaller.downloadAndOpenInstaller(context, url)
            }) {
                ApkInstallResult.InstallerOpened -> Unit
                ApkInstallResult.UnknownSourcesPermissionRequired -> {
                    Toast.makeText(
                        context,
                        "Allow Luno to install unknown apps, then tap Check for updates again.",
                        Toast.LENGTH_LONG
                    ).show()
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                        context.startActivity(Intent(android.provider.Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES).apply {
                            data = Uri.parse("package:${context.packageName}")
                        })
                    }
                }
                is ApkInstallResult.Failure -> Toast.makeText(
                    context,
                    result.message,
                    Toast.LENGTH_LONG
                ).show()
            }
        }
    }

    fun openReleaseWithWarning(url: String) {
        if (updatePreferences.getBoolean("sideload_warning_shown", false)) {
            downloadAndInstallApk(url)
        } else {
            pendingReleaseUrl = url
            showSideloadWarning = true
        }
    }

    fun checkForUpdates() {
        updateDialogState = UpdateDialogState.Checking
        scope.launch {
            val result = withContext(Dispatchers.IO) {
                releaseService.checkForUpdate(BuildConfig.VERSION_NAME)
            }
            updateDialogState = UpdateDialogState.Result(result)
        }
    }

    // Green-loader transition mask. It is set to `true` BEFORE every
    // navigation (see the call sites below) so the black layer with the
    // spinner is already opaque when the new screen composes — the screen
    // can never flash in early. Discover also reports its Last.fm request
    // through this state so its page is not revealed with a second loader.
    var transitionMask by remember { mutableStateOf(false) }
    val app = context.applicationContext as com.luno.mobile.LunoApp
    fun skipToNext() {
        if (!app.recommendationPreviewManager.skipToNext()) {
            musicController.skipToNext()
        }
    }
    fun skipToPrevious() {
        if (!app.recommendationPreviewManager.skipToPrevious()) {
            musicController.skipToPrevious()
        }
    }
    val libraryLoadedState = app.libraryData.loaded.collectAsState()
    val discoverLoadingState = remember { mutableStateOf(false) }
    LaunchedEffect(notificationOpenRequests) {
        var handledRequest = 0L
        notificationOpenRequests.collect { requestId ->
            if (requestId == 0L || requestId == handledRequest) return@collect
            handledRequest = requestId
            transitionMask = true
            navController.navigate(Routes.FULL_PLAYER) {
                launchSingleTop = true
            }
        }
    }
    LaunchedEffect(transitionMask) {
        if (!transitionMask) return@LaunchedEffect
        val startedAt = SystemClock.elapsedRealtime()
        while (SystemClock.elapsedRealtime() - startedAt < TRANSITION_MASK_HOLD_MS) {
            delay(16)
        }
        while (true) {
            val elapsed = SystemClock.elapsedRealtime() - startedAt
            val route = currentBackStackState.value?.destination?.route
            val libraryReady = libraryLoadedState.value
            val discoverReady = route != Routes.DISCOVER || !discoverLoadingState.value
            val pastNonDiscoverCap = route != Routes.DISCOVER && elapsed >= TRANSITION_MASK_MAX_MS
            if ((libraryReady && discoverReady) || pastNonDiscoverCap) break
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
    // Missing-artwork sweep (Settings drawer) — live "Fetching missing
    // artwork… N/M" strip under the header, owned by ArtworkFetchManager.
    val artworkStatus by app.artworkFetchManager.status.collectAsState()
    val folderImportLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.OpenDocumentTree()
    ) { uri: android.net.Uri? ->
        if (uri != null) {
            importManager.start(uri)
        }
    }

    val manifestExportLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.CreateDocument("application/json")
    ) { uri: Uri? ->
        val manifest = pendingManifestExport
        if (uri == null || manifest == null) {
            pendingManifestExport = null
            manifestExportBusy = false
        } else {
            scope.launch {
                try {
                    val json = app.libraryTransferRepository.encode(manifest)
                    withContext(Dispatchers.IO) {
                        context.contentResolver.openOutputStream(uri)?.use { output ->
                            output.write(json.toByteArray(Charsets.UTF_8))
                        } ?: error("Could not open the selected file")
                    }
                    Toast.makeText(
                        context,
                        "Exported ${manifest.tracks.size} track reference(s)",
                        Toast.LENGTH_SHORT
                    ).show()
                } catch (error: Exception) {
                    Toast.makeText(
                        context,
                        "Export failed: ${error.message ?: "Could not write file"}",
                        Toast.LENGTH_LONG
                    ).show()
                } finally {
                    pendingManifestExport = null
                    manifestExportBusy = false
                }
            }
        }
    }

    fun exportTracks(trackUris: List<String>) {
        if (manifestExportBusy || trackUris.isEmpty()) return
        scope.launch {
            manifestExportBusy = true
            try {
                pendingManifestExport = app.libraryTransferRepository
                    .buildSelectedTracksManifest(trackUris)
                manifestExportLauncher.launch("luno-selected.json")
            } catch (error: Exception) {
                manifestExportBusy = false
                Toast.makeText(
                    context,
                    "Export failed: ${error.message ?: "Could not build manifest"}",
                    Toast.LENGTH_LONG
                ).show()
            }
        }
    }

    fun exportPlaylists(playlistIds: List<Long>) {
        if (manifestExportBusy || playlistIds.isEmpty()) return
        scope.launch {
            manifestExportBusy = true
            try {
                pendingManifestExport = app.libraryTransferRepository
                    .buildPlaylistsManifest(playlistIds)
                manifestExportLauncher.launch("luno-playlists.json")
            } catch (error: Exception) {
                manifestExportBusy = false
                Toast.makeText(
                    context,
                    "Export failed: ${error.message ?: "Could not build manifest"}",
                    Toast.LENGTH_LONG
                ).show()
            }
        }
    }

    // Single ActivityResult permission launcher — survives recomposition.
    val notificationPermissionLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.RequestPermission()
    ) { /* grant result intentionally ignored — playback already dispatched */ }

    // Stable playback callback used by LibraryScreen and HomeScreen.
    // Accepts the FULL playback context (ordered track list +
    // start index) so next/previous/shuffle work relative to where the
    // track was picked from (all songs, carousel, playlist, ...).
    val onPlay: (List<MediaTrack>, Int, Boolean) -> Unit = remember(policy, notificationPermissionLauncher, musicController) {
        { tracks: List<MediaTrack>, startIndex: Int, shuffle: Boolean ->
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
                if (shuffle) {
                    musicController.playShuffled(tracks)
                } else {
                    musicController.play(tracks, startIndex)
                }
            }
        }
    }

    @Composable
    fun BottomNavigationBar() {
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
                        if (item.route == Routes.DISCOVER) {
                            // The request starts as soon as Discover composes. Set
                            // this before navigation so a fast recomposition cannot
                            // release the mask before the request reports loading.
                            discoverLoadingState.value = true
                        }
                        // Mask FIRST: the black layer is opaque before the new
                        // screen composes, so it never flashes in early.
                        transitionMask = true
                        if (item.route == Routes.HOME) {
                            // Home always returns to the Home screen, even from
                            // drawer/deep routes (Downloads, playlist detail, etc.).
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
                            // Download renders at 28dp because its glyph is
                            // optically smaller than the other tabs.
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

    ModalNavigationDrawer(
        drawerState = drawerState,
        drawerContent = {
            ModalDrawerSheet(
                drawerContainerColor = SurfaceDark,
                modifier = Modifier
                    .fillMaxWidth(0.8f)
                    .fillMaxHeight()
                    .verticalScroll(rememberScrollState())
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
                SettingsAccordion(
                    title = "Library",
                    icon = Icons.Filled.Folder,
                    expanded = expandedSettingsSection == "library",
                    onToggle = {
                        expandedSettingsSection = if (expandedSettingsSection == "library") {
                            null
                        } else {
                            "library"
                        }
                    }
                ) {
                    DrawerItem(
                        icon = Icons.Filled.Folder,
                        label = "Music folder",
                        onClick = {
                            scope.launch { drawerState.close() }
                            folderImportLauncher.launch(null)
                        }
                    )
                    DrawerItem(
                        icon = Icons.Filled.Sync,
                        label = "Sync app songs to music folder",
                        onClick = {
                            scope.launch { drawerState.close() }
                            scope.launch {
                                val result = app.downloadRepository.syncAppSongsToMusicFolder()
                                val message = result.error ?: buildString {
                                    append("Synced ${result.synced} songs to the music folder")
                                    if (result.skipped > 0) append("; ${result.skipped} skipped")
                                    if (result.failed > 0) append("; ${result.failed} failed")
                                }
                                Toast.makeText(
                                    context,
                                    message,
                                    if (result.error == null) Toast.LENGTH_SHORT else Toast.LENGTH_LONG
                                ).show()
                            }
                        }
                    )
                    DrawerItem(
                        icon = Icons.Filled.DeleteSweep,
                        label = "Duplicate checker",
                        onClick = {
                            scope.launch { drawerState.close() }
                            transitionMask = true
                            navController.navigate(Routes.DUPLICATES) {
                                launchSingleTop = true
                            }
                        }
                    )
                    DrawerItem(
                        icon = Icons.Filled.Image,
                        label = if (artworkStatus is ArtworkFetchStatus.Progress) {
                            "Fetching artwork…"
                        } else {
                            "Fetch missing artwork"
                        },
                        onClick = {
                            scope.launch { drawerState.close() }
                            app.artworkFetchManager.start()
                        }
                    )
                }
                SettingsAccordion(
                    title = "Downloads",
                    icon = Icons.Filled.Download,
                    expanded = expandedSettingsSection == "downloads",
                    onToggle = {
                        expandedSettingsSection = if (expandedSettingsSection == "downloads") {
                            null
                        } else {
                            "downloads"
                        }
                    }
                ) {
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
                        icon = Icons.Filled.UploadFile,
                        label = "Export / Import",
                        onClick = {
                            scope.launch { drawerState.close() }
                            transitionMask = true
                            navController.navigate(Routes.EXPORT_IMPORT) {
                                launchSingleTop = true
                            }
                        }
                    )
                }
                SettingsAccordion(
                    title = "History & discovery",
                    icon = Icons.Filled.History,
                    expanded = expandedSettingsSection == "history",
                    onToggle = {
                        expandedSettingsSection = if (expandedSettingsSection == "history") {
                            null
                        } else {
                            "history"
                        }
                    }
                ) {
                    DrawerItem(
                        icon = Icons.Filled.Key,
                        label = "Last.fm API key",
                        onClick = {
                            scope.launch { drawerState.close() }
                            showLastfmKeyDialog = true
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
                }
                SettingsAccordion(
                    title = "App",
                    icon = Icons.Filled.Info,
                    expanded = expandedSettingsSection == "app",
                    onToggle = {
                        expandedSettingsSection = if (expandedSettingsSection == "app") {
                            null
                        } else {
                            "app"
                        }
                    }
                ) {
                    DrawerItem(
                        icon = Icons.Filled.Info,
                        label = "About",
                        onClick = {
                            scope.launch { drawerState.close() }
                            Toast.makeText(context, "Luno v${BuildConfig.VERSION_NAME}", Toast.LENGTH_SHORT).show()
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
                            checkForUpdates()
                        }
                    )
                }
            }
        }
    ) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(PrimaryBackground)
                .appBackgroundWash()
        ) {
            Scaffold(
                // Let the shell wash show through the screen content while
                // navigation, drawers, and modal surfaces keep their own
                // opaque contrast-safe colors.
                containerColor = Color.Transparent,
                snackbarHost = { SnackbarHost(snackbarHostState) },
                bottomBar = {
                    // Full player is truly full-screen — hide mini player + nav bar.
                    if (currentRoute != Routes.FULL_PLAYER) {
                        if (currentRoute == Routes.HOME) {
                            BottomNavigationBar()
                        } else {
                            Column {
                                AnimatedVisibility(
                                    visible = hasActiveItem,
                                    enter = slideInVertically(initialOffsetY = { it }),
                                    exit = slideOutVertically(targetOffsetY = { it })
                                ) {
                                    MiniPlayer(
                                        musicController = musicController,
                                        onNext = ::skipToNext,
                                        onPrevious = ::skipToPrevious,
                                        onMiniPlayerTap = {
                                            transitionMask = true
                                            navController.navigate(Routes.FULL_PLAYER)
                                        }
                                    )
                                }
                                BottomNavigationBar()
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
                    // Live "Fetch missing artwork" progress (Settings
                    // drawer) — the sweep over a large library takes
                    // minutes, so show that work is happening: smooth sweep
                    // bar + "Fetching missing artwork… N/M (K found)", then
                    // the final message ("Artwork fetched for N track(s)" /
                    // "No missing artwork found" / failure) which the
                    // manager auto-clears after a few seconds.
                    when (val artwork = artworkStatus) {
                        is ArtworkFetchStatus.Progress -> Column(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(horizontal = Dimens.paddingLarge)
                        ) {
                            SmoothProgressBar(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .height(Dimens.progressBarHeight)
                            )
                            Text(
                                text = "Fetching missing artwork… ${artwork.scanned}/${artwork.total}" +
                                    if (artwork.updated > 0) " (${artwork.updated} found)" else "",
                                color = AccentGreen,
                                style = MaterialTheme.typography.bodySmall,
                                modifier = Modifier.padding(vertical = Dimens.paddingSmall)
                            )
                        }
                        is ArtworkFetchStatus.Finished -> Text(
                            text = when {
                                artwork.failed ->
                                    "Artwork fetch failed — please try again" +
                                        artwork.errorMessage?.let { " ($it)" }.orEmpty()
                                artwork.updated > 0 -> "Artwork fetched for ${artwork.updated} track(s)"
                                else -> "No missing artwork found"
                            },
                            color = if (artwork.failed) {
                                MaterialTheme.colorScheme.error
                            } else {
                                AccentGreen
                            },
                            style = MaterialTheme.typography.bodySmall,
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(horizontal = Dimens.paddingLarge, vertical = Dimens.paddingSmall)
                        )
                        null -> Unit
                    }

                    Box(
                        modifier = Modifier
                            .fillMaxWidth()
                            .weight(1f)
                    ) {
                        LunoNavHost(
                            navController = navController,
                            musicController = musicController,
                            modifier = Modifier.fillMaxSize(),
                            onCreatePlaylist = { showCreateSheet = true },
                            onPlay = onPlay,
                            onNavigate = { transitionMask = true },
                            onExportTracks = ::exportTracks,
                            onExportPlaylists = ::exportPlaylists,
                            onDiscoverLoadingChanged = { loading ->
                                discoverLoadingState.value = loading
                                if (loading && navController.currentDestination?.route == Routes.DISCOVER) {
                                    // Refreshes use the same single shell spinner as tab changes.
                                    transitionMask = true
                                }
                            }
                        )
                        // Green-loader transition mask (see above): covers
                        // the content area while the new screen fades in
                        // underneath, then fades away to reveal it.
                        TransitionMask(visible = transitionMask)
                        if (currentRoute == Routes.HOME) {
                            HomeMiniPlayerOverlay(
                                visible = hasActiveItem,
                                musicController = musicController,
                                onNext = ::skipToNext,
                                onPrevious = ::skipToPrevious,
                                onMiniPlayerTap = {
                                    transitionMask = true
                                    navController.navigate(Routes.FULL_PLAYER)
                                }
                            )
                        }
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
            // "Clear History" action; clears persisted history.
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

            // GitHub Releases update check. The APK is downloaded into private
            // cache storage and handed to Android's package installer.
            when (val state = updateDialogState) {
                UpdateDialogState.Checking -> AlertDialog(
                    onDismissRequest = { updateDialogState = null },
                    containerColor = SurfaceDark,
                    titleContentColor = PrimaryText,
                    textContentColor = SecondaryText,
                    title = { Text("Checking for updates") },
                    text = {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            CircularProgressIndicator(
                                modifier = Modifier.size(22.dp),
                                color = AccentGreen,
                                strokeWidth = 2.dp
                            )
                            Text(
                                text = "Checking GitHub Releases…",
                                color = SecondaryText,
                                modifier = Modifier.padding(start = Dimens.paddingMedium)
                            )
                        }
                    },
                    confirmButton = {
                        TextButton(onClick = { updateDialogState = null }) {
                            Text("Cancel", color = SecondaryText)
                        }
                    }
                )
                is UpdateDialogState.Result -> UpdateResultDialog(
                    result = state.value,
                    onDismiss = { updateDialogState = null },
                    onRetry = ::checkForUpdates,
                    onOpenRelease = ::openReleaseUrl,
                    onDownloadApk = ::openReleaseWithWarning
                )
                null -> Unit
            }

            if (showSideloadWarning) {
                AlertDialog(
                    onDismissRequest = {
                        showSideloadWarning = false
                        pendingReleaseUrl = null
                    },
                    containerColor = SurfaceDark,
                    titleContentColor = PrimaryText,
                    textContentColor = SecondaryText,
                    title = { Text("Install outside Google Play?") },
                    text = {
                        Text(
                            "This APK comes from GitHub, not Google Play. Android may ask you to allow " +
                                "your browser to install unknown apps. Only continue if you trust this release.",
                            color = SecondaryText
                        )
                    },
                    confirmButton = {
                        Button(
                            onClick = {
                                updatePreferences.edit()
                                    .putBoolean("sideload_warning_shown", true)
                                    .commit()
                                val url = pendingReleaseUrl
                                showSideloadWarning = false
                                pendingReleaseUrl = null
                                if (url != null) downloadAndInstallApk(url)
                            },
                            colors = ButtonDefaults.buttonColors(
                                containerColor = AccentGreen,
                                contentColor = PrimaryBackground
                            )
                        ) {
                            Text("Continue")
                        }
                    },
                    dismissButton = {
                        TextButton(onClick = {
                            showSideloadWarning = false
                            pendingReleaseUrl = null
                        }) {
                            Text("Cancel", color = SecondaryText)
                        }
                    }
                )
            }
        }
    }
}

@Composable
private fun HomeMiniPlayerOverlay(
    visible: Boolean,
    musicController: MusicController,
    onNext: () -> Unit,
    onPrevious: () -> Unit,
    onMiniPlayerTap: () -> Unit
) {
    // Keep the player in the content layer rather than the Scaffold bottom
    // bar. Its appearance never changes the measured Home viewport, and the
    // full-size overlay remains touchable above the navigation bar.
    Box(
        modifier = Modifier.fillMaxSize(),
        contentAlignment = Alignment.BottomCenter
    ) {
        AnimatedVisibility(
            visible = visible,
            enter = slideInVertically(initialOffsetY = { it }),
            exit = slideOutVertically(targetOffsetY = { it })
        ) {
            MiniPlayer(
                musicController = musicController,
                onNext = onNext,
                onPrevious = onPrevious,
                onMiniPlayerTap = onMiniPlayerTap
            )
        }
    }
}

@Composable
private fun UpdateResultDialog(
    result: ReleaseCheckResult,
    onDismiss: () -> Unit,
    onRetry: () -> Unit,
    onOpenRelease: (String) -> Unit,
    onDownloadApk: (String) -> Unit
) {
    when (result) {
        is ReleaseCheckResult.UpToDate -> AlertDialog(
            onDismissRequest = onDismiss,
            containerColor = SurfaceDark,
            titleContentColor = PrimaryText,
            textContentColor = SecondaryText,
            title = { Text("You're up to date") },
            text = { Text("Luno ${result.currentVersion} is the latest GitHub Release.") },
            confirmButton = {
                TextButton(onClick = onDismiss) { Text("Close", color = AccentGreen) }
            }
        )
        is ReleaseCheckResult.Failure -> AlertDialog(
            onDismissRequest = onDismiss,
            containerColor = SurfaceDark,
            titleContentColor = PrimaryText,
            textContentColor = SecondaryText,
            title = { Text("Could not check for updates") },
            text = { Text(result.message) },
            confirmButton = {
                TextButton(onClick = onRetry) { Text("Retry", color = AccentGreen) }
            },
            dismissButton = {
                TextButton(onClick = onDismiss) { Text("Close", color = SecondaryText) }
            }
        )
        is ReleaseCheckResult.UpdateAvailable -> {
            val release = result.release
            AlertDialog(
                onDismissRequest = onDismiss,
                containerColor = SurfaceDark,
                titleContentColor = PrimaryText,
                textContentColor = SecondaryText,
                title = { Text("Update available") },
                text = {
                    Column {
                        Text(
                            text = "${release.name} (${release.tagName})",
                            color = PrimaryText,
                            fontWeight = FontWeight.Bold
                        )
                        Spacer(modifier = Modifier.height(Dimens.paddingSmall))
                        Text(
                            text = release.notes.ifBlank { "No release notes were provided." },
                            color = SecondaryText,
                            modifier = Modifier
                                .fillMaxWidth()
                                .heightIn(max = 280.dp)
                                .verticalScroll(rememberScrollState())
                        )
                    }
                },
                confirmButton = {
                    Row {
                        TextButton(onClick = { onOpenRelease(release.releaseUrl) }) {
                            Text("View release", color = SecondaryText)
                        }
                        release.apkUrl?.let { apkUrl ->
                            Button(
                                onClick = { onDownloadApk(apkUrl) },
                                colors = ButtonDefaults.buttonColors(
                                    containerColor = AccentGreen,
                                    contentColor = PrimaryBackground
                                )
                            ) {
                                Text("Download APK")
                            }
                        }
                    }
                },
                dismissButton = {
                    TextButton(onClick = onDismiss) { Text("Close", color = SecondaryText) }
                }
            )
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
 * Blob-based background wash: the header keeps its green gradient while
 * separate, soft-edged glows leave gaps for the black base to show through.
 */
private fun Modifier.appBackgroundWash(): Modifier = drawWithCache {
    val headerGradient = Brush.linearGradient(
        colorStops = arrayOf(
            0.0f to AccentGreen.copy(alpha = 0.08f),
            0.26f to AppBackgroundGreen.copy(alpha = 0.48f),
            0.68f to AppBackgroundGreen.copy(alpha = 0.18f),
            1.0f to Color.Transparent
        ),
        start = Offset(size.width * 0.42f, 0f),
        end = Offset(size.width * 0.42f, size.height * 0.27f)
    )

    fun blob(center: Offset, radius: Float, centerAlpha: Float, bodyAlpha: Float): Brush =
        Brush.radialGradient(
            colorStops = arrayOf(
                0.0f to AccentGreen.copy(alpha = centerAlpha),
                0.28f to AppBackgroundGreen.copy(alpha = bodyAlpha),
                0.70f to AppBackgroundGreen.copy(alpha = bodyAlpha * 0.42f),
                1.0f to Color.Transparent
            ),
            center = center,
            radius = radius
        )

    val blobs = listOf(
        blob(
            center = Offset(size.width * -0.12f, size.height * 0.36f),
            radius = size.minDimension * 0.44f,
            centerAlpha = 0.08f,
            bodyAlpha = 0.43f
        ),
        blob(
            center = Offset(size.width * 1.12f, size.height * 0.36f),
            radius = size.minDimension * 0.41f,
            centerAlpha = 0.11f,
            bodyAlpha = 0.47f
        ),
        blob(
            center = Offset(size.width * -0.08f, size.height * 0.82f),
            radius = size.minDimension * 0.46f,
            centerAlpha = 0.09f,
            bodyAlpha = 0.46f
        ),
        blob(
            center = Offset(size.width * 1.08f, size.height * 0.82f),
            radius = size.minDimension * 0.44f,
            centerAlpha = 0.10f,
            bodyAlpha = 0.45f
        )
    )

    onDrawBehind {
        blobs.forEach { drawRect(it) }
        // Keep the green fade behind the Luno title and status-bar area.
        drawRect(headerGradient)
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
                .background(PrimaryBackground)
                .appBackgroundWash(),
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
private fun SettingsAccordion(
    title: String,
    icon: ImageVector,
    expanded: Boolean,
    onToggle: () -> Unit,
    content: @Composable () -> Unit
) {
    Column(modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(Dimens.cornerMedium))
                .clickable(
                    interactionSource = remember { MutableInteractionSource() },
                    indication = null,
                    onClick = onToggle
                )
                .padding(
                    horizontal = Dimens.paddingLarge,
                    vertical = Dimens.paddingMedium
                ),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Icon(
                imageVector = icon,
                contentDescription = null,
                tint = AccentGreen,
                modifier = Modifier.size(Dimens.iconSize)
            )
            Spacer(modifier = Modifier.width(Dimens.paddingLarge))
            Text(
                text = title,
                style = MaterialTheme.typography.titleMedium,
                color = PrimaryText,
                modifier = Modifier.weight(1f)
            )
            Icon(
                imageVector = if (expanded) Icons.Filled.ExpandLess else Icons.Filled.ExpandMore,
                contentDescription = if (expanded) "Collapse $title" else "Expand $title",
                tint = SecondaryText
            )
        }
        AnimatedVisibility(visible = expanded) {
            Column(modifier = Modifier.padding(start = Dimens.paddingMedium)) {
                content()
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
