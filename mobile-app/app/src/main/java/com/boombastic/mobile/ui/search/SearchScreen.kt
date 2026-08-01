package com.boombastic.mobile.ui.search

import android.net.Uri
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Clear
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.ExpandLess
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material.icons.filled.FolderOpen
import androidx.compose.material.icons.filled.LibraryMusic
import androidx.compose.material.icons.filled.Link
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.UploadFile
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.boombastic.mobile.BoomBasticApp
import com.boombastic.mobile.data.db.dao.PlaylistWithTracks
import com.boombastic.mobile.data.db.entity.DownloadJob
import com.boombastic.mobile.data.db.entity.DownloadState
import com.boombastic.mobile.data.db.entity.Track
import com.boombastic.mobile.playback.ExtractionResult
import com.boombastic.mobile.playback.MediaTrack
import com.boombastic.mobile.playback.MusicController
import com.boombastic.mobile.playback.WebSearchResult
import com.boombastic.mobile.playback.WebSearchService
import com.boombastic.mobile.ui.components.ArtworkCollage
import com.boombastic.mobile.ui.components.ArtworkImage
import com.boombastic.mobile.ui.components.TrackActionsSheet
import com.boombastic.mobile.ui.shell.FolderImportStatus
import com.boombastic.mobile.ui.theme.AccentGreen
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText
import com.boombastic.mobile.ui.theme.SurfaceElevated
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Spotify-inspired search hub: a bold "Search" header, pill-shaped search
 * bar, chip-style tab switcher (Library / Web Search), and — while idle —
 * a "Browse all" grid of gradient tiles (playlists + import actions).
 * Web results render with YouTube thumbnails.
 */
@Composable
fun SearchScreen(
    musicController: MusicController,
    onPlay: (List<MediaTrack>, Int) -> Unit = { _, _ -> },
    onOpenPlaylist: (Long) -> Unit = {}
) {
    val context = LocalContext.current
    val app = context.applicationContext as BoomBasticApp
    val scope = rememberCoroutineScope()

    var tabIndex by rememberSaveable { mutableIntStateOf(0) }
    var query by rememberSaveable { mutableStateOf("") }
    val searchResults by app.libraryRepository.searchTracks(query).collectAsState(initial = emptyList())

    val importLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.OpenMultipleDocuments()
    ) { uris: List<Uri> ->
        if (uris.isNotEmpty()) {
            // appScope: the import must survive leaving this screen.
            app.appScope.launch {
                app.libraryRepository.importMultipleUris(uris)
            }
        }
    }

    // Desktop-style folder import: subfolders become playlists (folder name
    // = playlist name), files at the root land in "Unsorted".  The chosen
    // folder is persisted as the app's music-folder destination.  The
    // import job, its stall watchdog and the "Import complete!" strip
    // lifecycle are owned by MusicFolderImportManager (process-lifetime,
    // shared with the Settings-drawer launcher), so the strip always
    // resolves instead of sitting at the final counts.
    val importManager = remember {
        (context.applicationContext as BoomBasticApp).musicFolderImportManager
    }
    val importStatus by importManager.status.collectAsState()
    val folderImportLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.OpenDocumentTree()
    ) { uri: Uri? ->
        if (uri != null) {
            importManager.start(uri)
        }
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = Dimens.paddingLarge)
    ) {
        Text(
            text = "Search",
            style = MaterialTheme.typography.headlineLarge,
            fontWeight = FontWeight.Bold,
            color = PrimaryText,
            modifier = Modifier.padding(
                top = Dimens.paddingLarge,
                bottom = Dimens.paddingMedium
            )
        )

        // Chip-style switcher (selected chip = accent green, Spotify's
        // filter-chip look) instead of the old underline TabRow.
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.Center
        ) {
            SearchTabChip(
                text = "Library",
                selected = tabIndex == 0,
                onClick = { tabIndex = 0 }
            )
            Spacer(modifier = Modifier.width(Dimens.paddingSmall))
            SearchTabChip(
                text = "Web Search",
                selected = tabIndex == 1,
                onClick = { tabIndex = 1 }
            )
        }

        Spacer(modifier = Modifier.height(Dimens.paddingMedium))

        when (tabIndex) {
            0 -> LibrarySearchContent(
                query = query,
                onQueryChange = { query = it },
                displayTracks = if (query.isBlank()) emptyList() else searchResults,
                onPlay = onPlay,
                onOpenPlaylist = onOpenPlaylist,
                // `*/*` (not `audio/*`): with `audio/*` the system picker
                // hides folders from selection, so "Select all" only ever
                // returned the loose songs at the root and the playlist
                // folders were never imported.  `importMultipleUris` recurses
                // into picked folders itself.
                onImport = { importLauncher.launch(arrayOf("*/*")) },
                onImportFolder = { folderImportLauncher.launch(null) },
                importStatus = importStatus
            )
            1 -> WebSearchContent(
                downloadRepository = app.downloadRepository,
                scope = scope
            )
        }
    }
}

/** Spotify filter-chip: accent-green pill when selected, dark pill otherwise. */
@Composable
private fun SearchTabChip(
    text: String,
    selected: Boolean,
    onClick: () -> Unit
) {
    Text(
        text = text,
        modifier = Modifier
            .clip(RoundedCornerShape(20.dp))
            .background(if (selected) AccentGreen else SurfaceElevated)
            .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = onClick
            )
            .padding(horizontal = Dimens.paddingXLarge, vertical = Dimens.paddingSmall),
        style = MaterialTheme.typography.labelLarge,
        color = if (selected) Color.Black else PrimaryText
    )
}

@Composable
private fun LibrarySearchContent(
    query: String,
    onQueryChange: (String) -> Unit,
    displayTracks: List<Track>,
    onPlay: (List<MediaTrack>, Int) -> Unit,
    onOpenPlaylist: (Long) -> Unit,
    onImport: () -> Unit,
    onImportFolder: () -> Unit,
    importStatus: FolderImportStatus?
) {
    val app = (LocalContext.current.applicationContext as BoomBasticApp)
    val playlists by app.libraryData.playlists.collectAsState()

    Column(modifier = Modifier.fillMaxSize()) {
        SearchPill(
            value = query,
            onValueChange = onQueryChange,
            placeholder = "What do you want to play?"
        )

        // Live import progress strip — flips to "Import complete!" (green)
        // when the folder import finishes, then auto-clears after a couple
        // of seconds (the manager owns that lifecycle).
        importStatus?.let { status ->
            when (status) {
                is FolderImportStatus.Importing -> Text(
                    text = "Importing music folder… ${status.imported} added, " +
                        "${status.duplicates} duplicates, ${status.errors} errors",
                    color = AccentGreen,
                    style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.padding(top = Dimens.paddingSmall)
                )
                is FolderImportStatus.Finished -> Text(
                    text = when {
                        status.failed ->
                            "Import failed — please try again" +
                                status.errorMessage?.let { " ($it)" }.orEmpty()
                        status.stalled -> "Import is taking longer than expected… still working"
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
                    modifier = Modifier.padding(top = Dimens.paddingSmall)
                )
            }
        }

        if (query.isBlank()) {
            BrowseAllGrid(
                playlists = playlists,
                onOpenPlaylist = onOpenPlaylist,
                onImport = onImport,
                onImportFolder = onImportFolder
            )
        } else if (displayTracks.isEmpty()) {
            Box(
                modifier = Modifier.fillMaxSize(),
                contentAlignment = Alignment.Center
            ) {
                Text(
                    text = "No results for \"$query\"",
                    color = SecondaryText,
                    style = MaterialTheme.typography.bodyMedium
                )
            }
        } else {
            // Track actions sheet — hoisted out of the LazyColumn: composing
            // a ModalBottomSheet inside a lazy item makes it scroll with the
            // list.
            var actionsTrack by remember { mutableStateOf<Track?>(null) }
            // The full result set as the playback context: next/previous on
            // the full player walk the search results, matching the desktop.
            val mediaTracks = remember(displayTracks) {
                displayTracks.map {
                    MediaTrack(
                        uri = it.uri,
                        title = it.title,
                        artist = it.artist,
                        album = it.album,
                        durationMs = it.durationMs,
                        artworkUri = it.albumArtUri()
                    )
                }
            }
            LazyColumn(
                verticalArrangement = Arrangement.spacedBy(2.dp),
                contentPadding = PaddingValues(bottom = Dimens.paddingLarge)
            ) {
                items(displayTracks, key = { it.uri }) { track ->
                    TrackRow(
                        track = track,
                        onClick = {
                            val index = displayTracks.indexOfFirst { it.uri == track.uri }
                            onPlay(mediaTracks, index.coerceAtLeast(0))
                        },
                        onLongPress = { actionsTrack = track }
                    )
                }
            }
            actionsTrack?.let { track ->
                TrackActionsSheet(
                    track = track,
                    onDismiss = { actionsTrack = null }
                )
            }
        }
    }
}

/** Spotify "Browse all": a 2-column grid of colorful gradient tiles. */
@Composable
private fun BrowseAllGrid(
    playlists: List<PlaylistWithTracks>,
    onOpenPlaylist: (Long) -> Unit,
    onImport: () -> Unit,
    onImportFolder: () -> Unit
) {
    LazyVerticalGrid(
        columns = GridCells.Fixed(2),
        modifier = Modifier.fillMaxSize(),
        horizontalArrangement = Arrangement.spacedBy(Dimens.paddingMedium),
        verticalArrangement = Arrangement.spacedBy(Dimens.paddingMedium),
        contentPadding = PaddingValues(bottom = Dimens.paddingXLarge)
    ) {
        item(span = { GridItemSpan(maxLineSpan) }) {
            Text(
                text = "Browse all",
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Bold,
                color = PrimaryText,
                modifier = Modifier.padding(
                    top = Dimens.paddingSmall,
                    bottom = Dimens.paddingSmall
                )
            )
        }

        item(key = "import_songs") {
            BrowseActionTile(
                label = "Import songs",
                icon = Icons.Filled.LibraryMusic,
                gradient = listOf(Color(0xFF1ED760), Color(0xFF0E8F43)),
                onClick = onImport
            )
        }
        item(key = "import_folder") {
            BrowseActionTile(
                label = "Import folder",
                icon = Icons.Filled.FolderOpen,
                gradient = listOf(Color(0xFF7358FF), Color(0xFF4527A0)),
                onClick = onImportFolder
            )
        }

        if (playlists.isEmpty()) {
            item(span = { GridItemSpan(maxLineSpan) }) {
                Text(
                    text = "No playlists yet — import songs to get started.",
                    color = SecondaryText,
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.padding(top = Dimens.paddingMedium)
                )
            }
        } else {
            items(playlists, key = { it.playlist.id }) { playlistWithTracks ->
                PlaylistBrowseTile(
                    playlistWithTracks = playlistWithTracks,
                    onClick = { onOpenPlaylist(playlistWithTracks.playlist.id) }
                )
            }
        }
    }
}

/** Playlist tile: 2x2 collage art with a dark scrim + bold name (Spotify style). */
@Composable
private fun PlaylistBrowseTile(
    playlistWithTracks: PlaylistWithTracks,
    onClick: () -> Unit
) {
    Box(
        modifier = Modifier
            .aspectRatio(1f)
            .clip(RoundedCornerShape(Dimens.cornerLarge))
            .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = onClick
            )
    ) {
        ArtworkCollage(
            tracks = playlistWithTracks.tracks,
            modifier = Modifier.fillMaxSize(),
            placeholderIconSize = 28.dp,
            decodeSizePx = 256,
            shape = RoundedCornerShape(0.dp)
        )
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .align(Alignment.BottomCenter)
                .background(
                    Brush.verticalGradient(
                        listOf(Color.Transparent, Color.Black.copy(alpha = 0.72f))
                    )
                )
                .padding(horizontal = Dimens.paddingSmall, vertical = Dimens.paddingMedium)
        ) {
            Text(
                text = playlistWithTracks.playlist.name,
                style = MaterialTheme.typography.titleSmall,
                fontWeight = FontWeight.Bold,
                color = Color.White,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
        }
    }
}

/** Action tile: Spotify genre-tile look (vivid gradient, label + icon). */
@Composable
private fun BrowseActionTile(
    label: String,
    icon: ImageVector,
    gradient: List<Color>,
    onClick: () -> Unit
) {
    Box(
        modifier = Modifier
            .aspectRatio(1f)
            .clip(RoundedCornerShape(Dimens.cornerLarge))
            .background(Brush.verticalGradient(gradient))
            .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = onClick
            )
    ) {
        Text(
            text = label,
            style = MaterialTheme.typography.titleSmall,
            fontWeight = FontWeight.Bold,
            color = Color.White,
            modifier = Modifier
                .align(Alignment.BottomStart)
                .padding(Dimens.paddingMedium)
        )
        Icon(
            imageVector = icon,
            contentDescription = null,
            tint = Color.White,
            modifier = Modifier
                .align(Alignment.BottomEnd)
                .padding(Dimens.paddingMedium)
                .size(Dimens.iconSizeLarge)
        )
    }
}

/** The shared Spotify-style pill search bar. */
@Composable
private fun SearchPill(
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String
) {
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        modifier = Modifier.fillMaxWidth(),
        placeholder = { Text(text = placeholder, color = SecondaryText) },
        leadingIcon = {
            Icon(
                imageVector = Icons.Filled.Search,
                contentDescription = "Search",
                tint = SecondaryText
            )
        },
        trailingIcon = {
            if (value.isNotBlank()) {
                IconButton(onClick = { onValueChange("") }) {
                    Icon(
                        imageVector = Icons.Filled.Clear,
                        contentDescription = "Clear search",
                        tint = SecondaryText
                    )
                }
            }
        },
        singleLine = true,
        shape = RoundedCornerShape(24.dp),
        colors = OutlinedTextFieldDefaults.colors(
            focusedTextColor = PrimaryText,
            unfocusedTextColor = PrimaryText,
            cursorColor = AccentGreen,
            focusedBorderColor = AccentGreen,
            unfocusedBorderColor = SurfaceElevated,
            focusedContainerColor = SurfaceElevated,
            unfocusedContainerColor = SurfaceElevated
        )
    )
}

/** Expandable row (URL input / CSV import toggles). */
@Composable
private fun ToggleRow(
    icon: ImageVector,
    text: String,
    expanded: Boolean,
    onClick: () -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = onClick
            )
            .padding(vertical = Dimens.paddingMedium),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            tint = AccentGreen,
            modifier = Modifier.size(Dimens.iconSize)
        )
        Spacer(modifier = Modifier.width(Dimens.paddingSmall))
        Text(
            text = text,
            style = MaterialTheme.typography.labelLarge,
            color = AccentGreen
        )
        Spacer(modifier = Modifier.weight(1f))
        Icon(
            imageVector = if (expanded) Icons.Filled.ExpandLess else Icons.Filled.ExpandMore,
            contentDescription = null,
            tint = SecondaryText,
            modifier = Modifier.size(Dimens.iconSizeSmall)
        )
    }
}

@Composable
private fun WebSearchContent(
    downloadRepository: com.boombastic.mobile.data.repository.DownloadRepository,
    scope: kotlinx.coroutines.CoroutineScope
) {
    var searchQuery by rememberSaveable { mutableStateOf("") }
    var webResults by remember { mutableStateOf<List<WebSearchResult>>(emptyList()) }
    var isSearching by remember { mutableStateOf(false) }
    var showUrlInput by remember { mutableStateOf(false) }
    var urlInput by rememberSaveable { mutableStateOf("") }
    var urlTitle by rememberSaveable { mutableStateOf("") }
    var urlArtist by rememberSaveable { mutableStateOf("") }
    val downloads by downloadRepository.getAllDownloads().collectAsState(initial = emptyList())
    var errorMsg by remember { mutableStateOf("") }
    val ctx = androidx.compose.ui.platform.LocalContext.current
    var downloadingVideoIds by remember { mutableStateOf(setOf<String>()) }

    // videoId → job id, set at enqueue time so rows can show live
    // progress/checkmark/cancel for the exact job they created.
    var jobIdByVideoId by remember { mutableStateOf<Map<String, Long>>(emptyMap()) }

    // Completed results show a green checkmark for a moment, then vanish.
    var dismissedVideoIds by remember { mutableStateOf(setOf<String>()) }
    LaunchedEffect(downloads) {
        for (job in downloads) {
            if (job.state != DownloadState.COMPLETED) continue
            val videoId = jobIdByVideoId.entries.firstOrNull { it.value == job.id }?.key
                ?: continue
            if (videoId in dismissedVideoIds) continue
            delay(1500)
            dismissedVideoIds = dismissedVideoIds + videoId
        }
    }
    val visibleResults = webResults.filter { it.videoId !in dismissedVideoIds }

    fun jobForResult(result: WebSearchResult): DownloadJob? =
        jobIdByVideoId[result.videoId]?.let { id ->
            downloads.firstOrNull { it.id == id }
        }

    Column(modifier = Modifier.fillMaxSize()) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically
        ) {
            OutlinedTextField(
                value = searchQuery,
                onValueChange = { searchQuery = it; errorMsg = "" },
                modifier = Modifier.weight(1f),
                placeholder = { Text("Search YouTube...", color = SecondaryText) },
                leadingIcon = {
                    Icon(Icons.Default.Search, contentDescription = null, tint = SecondaryText)
                },
                singleLine = true,
                shape = RoundedCornerShape(24.dp),
                colors = OutlinedTextFieldDefaults.colors(
                    focusedTextColor = PrimaryText,
                    unfocusedTextColor = PrimaryText,
                    cursorColor = AccentGreen,
                    focusedBorderColor = AccentGreen,
                    unfocusedBorderColor = SurfaceElevated,
                    focusedContainerColor = SurfaceElevated,
                    unfocusedContainerColor = SurfaceElevated
                )
            )
            Spacer(modifier = Modifier.width(Dimens.paddingSmall))
            // Circular green search button (Spotify's round action look).
            Button(
                onClick = {
                    if (searchQuery.isBlank()) return@Button
                    isSearching = true
                    webResults = emptyList()
                    errorMsg = ""
                    scope.launch {
                        val result = withContext(Dispatchers.IO) {
                            WebSearchService.searchYouTube(searchQuery.trim())
                        }
                        when (result) {
                            is ExtractionResult.Success -> {
                                webResults = result.data
                                errorMsg = ""
                            }
                            is ExtractionResult.Error -> {
                                webResults = emptyList()
                                errorMsg = result.message
                            }
                        }
                        isSearching = false
                    }
                },
                enabled = searchQuery.isNotBlank() && !isSearching,
                colors = ButtonDefaults.buttonColors(containerColor = AccentGreen),
                modifier = Modifier.size(Dimens.touchTargetMin),
                shape = CircleShape
            ) {
                Icon(
                    imageVector = Icons.Filled.Search,
                    contentDescription = "Search YouTube",
                    tint = Color.Black,
                    modifier = Modifier.size(Dimens.iconSize)
                )
            }
        }

        if (errorMsg.isNotBlank()) {
            Text(
                text = errorMsg,
                color = MaterialTheme.colorScheme.error,
                style = MaterialTheme.typography.bodyMedium,
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = Dimens.paddingSmall, bottom = Dimens.paddingSmall)
                    .background(Color(0x33FF0000))
                    .padding(horizontal = Dimens.paddingSmall, vertical = Dimens.paddingSmall)
            )
        }

        // Toggle to show URL input
        ToggleRow(
            icon = Icons.Filled.Link,
            text = if (showUrlInput) "Hide URL input" else "Or paste a direct audio URL",
            expanded = showUrlInput
        ) { showUrlInput = !showUrlInput }

        if (showUrlInput) {
            UrlDownloadSection(
                urlInput = urlInput,
                onUrlChange = { urlInput = it; errorMsg = "" },
                trackTitle = urlTitle,
                onTitleChange = { urlTitle = it },
                trackArtist = urlArtist,
                onArtistChange = { urlArtist = it },
                onDownload = {
                    if (urlInput.isNotBlank()) {
                        scope.launch {
                            downloadRepository.enqueueDownload(
                                sourceUrl = urlInput.trim(),
                                title = urlTitle.trim().ifBlank { "Download" },
                                artist = urlArtist.trim()
                            )
                            urlInput = ""
                            urlTitle = ""
                            urlArtist = ""
                            Toast.makeText(ctx, "Download queued", Toast.LENGTH_SHORT).show()
                        }
                    }
                }
            )
        }

        // CSV import
        var showCsvImport by rememberSaveable { mutableStateOf(false) }
        ToggleRow(
            icon = Icons.Filled.UploadFile,
            text = if (showCsvImport) "Hide CSV import" else "Import Spotify CSV (Exportify)",
            expanded = showCsvImport
        ) { showCsvImport = !showCsvImport }

        if (showCsvImport) {
            CsvImportSection(
                onTracksLoaded = { rows ->
                    scope.launch {
                        for ((artist, title) in rows) {
                            val query = "$artist $title"
                            val encoded = java.net.URLEncoder.encode(query, "UTF-8")
                            val searchResult = withContext(kotlinx.coroutines.Dispatchers.IO) {
                                WebSearchService.searchYouTube(query, limit = 1)
                            }
                            if (searchResult is ExtractionResult.Success && searchResult.data.isNotEmpty()) {
                                val r = searchResult.data.first()
                                val audioResult = withContext(kotlinx.coroutines.Dispatchers.IO) {
                                    WebSearchService.getAudioStreamUrl(r.videoId)
                                }
                                if (audioResult is ExtractionResult.Success) {
                                    downloadRepository.enqueueDownload(
                                        sourceUrl = audioResult.data.url,
                                        title = title,
                                        artist = artist,
                                        thumbnailUrl = r.thumbnailUrl
                                    )
                                }
                            }
                        }
                    }
                }
            )
        }

        Spacer(modifier = Modifier.height(Dimens.paddingSmall))

        // Search results
        if (isSearching) {
            Box(
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth(),
                contentAlignment = Alignment.Center
            ) {
                CircularProgressIndicator(color = AccentGreen)
            }
        } else if (webResults.isNotEmpty()) {
            // Show results
            Text(
                text = "YouTube results (${visibleResults.size})",
                style = MaterialTheme.typography.titleSmall,
                fontWeight = FontWeight.Bold,
                color = PrimaryText,
                modifier = Modifier.padding(bottom = Dimens.paddingSmall)
            )
            LazyColumn(
                modifier = Modifier.weight(1f),
                verticalArrangement = Arrangement.spacedBy(2.dp),
                contentPadding = PaddingValues(bottom = Dimens.paddingSmall)
            ) {
                items(visibleResults, key = { it.videoId }) { result ->
                    val job = jobForResult(result)
                    WebResultRow(
                        result = result,
                        job = job,
                        onDownload = {
                            val videoId = result.videoId
                            if (videoId in downloadingVideoIds) return@WebResultRow
                            downloadingVideoIds = downloadingVideoIds + videoId
                            scope.launch {
                                try {
                                    val audioResult = withContext(Dispatchers.IO) {
                                        WebSearchService.getAudioStreamUrl(result.videoId)
                                    }
                                    when (audioResult) {
                                        is ExtractionResult.Success -> {
                                            val titleParts = result.title.split(" - ", limit = 2)
                                            val artist = if (titleParts.size > 1) titleParts[0].trim() else result.artist
                                            val trackTitle = if (titleParts.size > 1) titleParts[1].trim() else result.title
                                            val jobId = downloadRepository.enqueueDownload(
                                                sourceUrl = audioResult.data.url,
                                                title = trackTitle,
                                                artist = artist,
                                                thumbnailUrl = result.thumbnailUrl
                                            )
                                            jobIdByVideoId = jobIdByVideoId + (videoId to jobId)
                                            Toast.makeText(ctx, "Download queued: $trackTitle", Toast.LENGTH_SHORT).show()
                                        }
                                        is ExtractionResult.Error -> {
                                            errorMsg = "Extraction failed: ${audioResult.message}"
                                        }
                                    }
                                } catch (e: Exception) {
                                    errorMsg = "Error: ${e.message}"
                                } finally {
                                    downloadingVideoIds = downloadingVideoIds - videoId
                                }
                            }
                        },
                        onCancel = { jobId ->
                            scope.launch { downloadRepository.cancelDownload(jobId) }
                        },
                        onRetry = { jobId ->
                            scope.launch { downloadRepository.retryDownload(jobId) }
                        },
                        extractingAudio = result.videoId in downloadingVideoIds
                    )
                }
            }
        } else if (searchQuery.isNotBlank() && !isSearching && webResults.isEmpty() && errorMsg.isBlank()) {
            Text(
                text = "No results found for \"$searchQuery\". Check network or try a different query.",
                color = MaterialTheme.colorScheme.error,
                style = MaterialTheme.typography.bodyMedium,
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth()
                    .padding(vertical = Dimens.paddingLarge)
            )
        }

        if (webResults.isEmpty() && !isSearching && searchQuery.isBlank()) {
            Box(
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth(),
                contentAlignment = Alignment.Center
            ) {
                Text(
                    text = "Search YouTube for songs, then download them to your library.",
                    color = SecondaryText,
                    style = MaterialTheme.typography.bodyMedium
                )
            }
        }
    }
}

@Composable
private fun UrlDownloadSection(
    urlInput: String,
    onUrlChange: (String) -> Unit,
    trackTitle: String,
    onTitleChange: (String) -> Unit,
    trackArtist: String,
    onArtistChange: (String) -> Unit,
    onDownload: () -> Unit
) {
    Column(modifier = Modifier.padding(vertical = Dimens.paddingSmall)) {
        OutlinedTextField(
            value = urlInput,
            onValueChange = onUrlChange,
            modifier = Modifier.fillMaxWidth(),
            placeholder = { Text("https://...", color = SecondaryText) },
            singleLine = true,
            shape = RoundedCornerShape(16.dp),
            colors = OutlinedTextFieldDefaults.colors(
                focusedTextColor = PrimaryText,
                unfocusedTextColor = PrimaryText,
                cursorColor = AccentGreen,
                focusedBorderColor = AccentGreen,
                unfocusedBorderColor = SurfaceElevated,
                focusedContainerColor = SurfaceElevated,
                unfocusedContainerColor = SurfaceElevated
            )
        )
        Spacer(modifier = Modifier.height(Dimens.paddingSmall))
        Row(modifier = Modifier.fillMaxWidth()) {
            OutlinedTextField(
                value = trackTitle,
                onValueChange = onTitleChange,
                modifier = Modifier.weight(1f),
                placeholder = { Text("Title", color = SecondaryText) },
                singleLine = true,
                shape = RoundedCornerShape(16.dp),
                colors = OutlinedTextFieldDefaults.colors(
                    focusedTextColor = PrimaryText,
                    unfocusedTextColor = PrimaryText,
                    cursorColor = AccentGreen,
                    focusedBorderColor = AccentGreen,
                    unfocusedBorderColor = SurfaceElevated,
                    focusedContainerColor = SurfaceElevated,
                    unfocusedContainerColor = SurfaceElevated
                )
            )
            Spacer(modifier = Modifier.width(Dimens.paddingSmall))
            OutlinedTextField(
                value = trackArtist,
                onValueChange = onArtistChange,
                modifier = Modifier.weight(1f),
                placeholder = { Text("Artist", color = SecondaryText) },
                singleLine = true,
                shape = RoundedCornerShape(16.dp),
                colors = OutlinedTextFieldDefaults.colors(
                    focusedTextColor = PrimaryText,
                    unfocusedTextColor = PrimaryText,
                    cursorColor = AccentGreen,
                    focusedBorderColor = AccentGreen,
                    unfocusedBorderColor = SurfaceElevated,
                    focusedContainerColor = SurfaceElevated,
                    unfocusedContainerColor = SurfaceElevated
                )
            )
        }
        Spacer(modifier = Modifier.height(Dimens.paddingSmall))
        Button(
            onClick = onDownload,
            enabled = urlInput.isNotBlank(),
            colors = ButtonDefaults.buttonColors(containerColor = AccentGreen),
            modifier = Modifier.fillMaxWidth()
        ) {
            Icon(Icons.Default.Download, contentDescription = null, modifier = Modifier.size(Dimens.iconSize))
            Spacer(modifier = Modifier.width(Dimens.paddingSmall))
            Text("Start Download")
        }
    }
}

@Composable
private fun WebResultRow(
    result: WebSearchResult,
    job: DownloadJob?,
    onDownload: () -> Unit,
    onCancel: (Long) -> Unit,
    onRetry: (Long) -> Unit,
    extractingAudio: Boolean = false
) {
    val durationStr = formatDuration(result.duration)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = Dimens.paddingSmall, horizontal = Dimens.paddingSmall),
        verticalAlignment = Alignment.CenterVertically
    ) {
        // YouTube thumbnail (48dp, rounded) — same row size as library rows.
        ArtworkImage(
            artworkUri = result.thumbnailUrl,
            modifier = Modifier
                .size(Dimens.albumArtSmall)
                .clip(RoundedCornerShape(Dimens.cornerSmall)),
            placeholderIconSize = 20.dp,
            decodeSizePx = 128
        )
        Spacer(modifier = Modifier.width(Dimens.paddingMedium))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = result.title,
                style = MaterialTheme.typography.titleSmall,
                color = PrimaryText,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
            Row {
                Text(
                    text = result.artist,
                    style = MaterialTheme.typography.bodySmall,
                    color = SecondaryText,
                    maxLines = 1,
                    modifier = Modifier.weight(1f)
                )
                Text(
                    text = durationStr,
                    style = MaterialTheme.typography.labelSmall,
                    color = SecondaryText
                )
            }
        }

        Spacer(modifier = Modifier.width(Dimens.paddingSmall))

        val state = job?.state
        when {
            state == DownloadState.COMPLETED -> {
                // Green checkmark — row disappears shortly after
                Icon(
                    Icons.Default.CheckCircle,
                    contentDescription = "Downloaded",
                    tint = AccentGreen,
                    modifier = Modifier.size(Dimens.iconSize)
                )
            }
            state == DownloadState.QUEUED || state == DownloadState.DOWNLOADING -> {
                // Green circular progress — tap to cancel this download
                CircularProgressIndicator(
                    modifier = Modifier
                        .size(Dimens.iconSize)
                        .clickable { job?.let { onCancel(it.id) } },
                    color = AccentGreen,
                    strokeWidth = 2.dp
                )
            }
            state == DownloadState.FAILED -> {
                IconButton(onClick = { job?.let { onRetry(it.id) } }) {
                    Icon(
                        Icons.Default.Refresh,
                        contentDescription = "Retry download",
                        tint = AccentGreen,
                        modifier = Modifier.size(Dimens.iconSize)
                    )
                }
            }
            extractingAudio -> {
                CircularProgressIndicator(
                    modifier = Modifier.size(Dimens.iconSize),
                    color = AccentGreen,
                    strokeWidth = 2.dp
                )
            }
            else -> {
                IconButton(onClick = onDownload) {
                    Icon(
                        Icons.Default.Download,
                        contentDescription = "Download",
                        tint = AccentGreen
                    )
                }
            }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
fun TrackRow(
    track: Track,
    onClick: () -> Unit,
    onLongPress: () -> Unit = {}
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .combinedClickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = onClick,
                onLongClick = onLongPress
            )
            .padding(vertical = Dimens.paddingSmall, horizontal = Dimens.paddingSmall),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Column(
            modifier = Modifier.weight(1f)
        ) {
            Text(
                text = track.title,
                style = MaterialTheme.typography.titleSmall,
                color = PrimaryText,
                maxLines = 1
            )
            Text(
                text = track.artist,
                style = MaterialTheme.typography.bodySmall,
                color = SecondaryText,
                maxLines = 1
            )
        }
    }
}

private fun formatDuration(seconds: Long): String {
    val mins = seconds / 60
    val secs = seconds % 60
    return "%d:%02d".format(mins, secs)
}

@Composable
private fun CsvImportSection(
    onTracksLoaded: (List<Pair<String, String>>) -> Unit
) {
    val context = androidx.compose.ui.platform.LocalContext.current
    val scope = rememberCoroutineScope()
    val launcher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.OpenDocument()
    ) { uri: Uri? ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            val rows = withContext(Dispatchers.IO) {
                try {
                    val reader = java.io.BufferedReader(
                        java.io.InputStreamReader(context.contentResolver.openInputStream(uri))
                    )
                    val lines = reader.readLines()
                    reader.close()
                    // Skip header, parse CSV
                    val trackRows = mutableListOf<Pair<String, String>>()
                    for (line in lines.drop(1)) {
                        // Exportify format: "Track Name","Artist Name(s)","Album Name","...
                        val parts = line.split("\",\"")
                        if (parts.size >= 2) {
                            val title = parts[0].trim('"', ' ')
                            val artist = parts[1].trim('"', ' ')
                            if (title.isNotBlank() && artist.isNotBlank()) {
                                trackRows.add(Pair(artist, title))
                            }
                        }
                    }
                    trackRows
                } catch (e: Exception) {
                    emptyList()
                }
            }
            if (rows.isNotEmpty()) {
                onTracksLoaded(rows)
            }
        }
    }

    Column(modifier = Modifier.padding(vertical = Dimens.paddingSmall)) {
        Text(
            text = "Select an Exportify CSV file to batch-import from Spotify.",
            style = MaterialTheme.typography.bodySmall,
            color = SecondaryText,
            modifier = Modifier.padding(bottom = Dimens.paddingSmall)
        )
        Button(
            onClick = { launcher.launch(arrayOf("text/*", "*/*")) },
            colors = ButtonDefaults.buttonColors(containerColor = AccentGreen),
            modifier = Modifier.fillMaxWidth()
        ) {
            Text("Select CSV File", color = PrimaryText)
        }
    }
}
