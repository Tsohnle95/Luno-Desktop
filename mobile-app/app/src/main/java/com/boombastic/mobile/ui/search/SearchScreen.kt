package com.boombastic.mobile.ui.search

import android.net.Uri
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Tab
import androidx.compose.material3.TabRow
import androidx.compose.material3.TabRowDefaults
import androidx.compose.material3.TabRowDefaults.tabIndicatorOffset
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.boombastic.mobile.BoomBasticApp
import com.boombastic.mobile.data.db.entity.DownloadJob
import com.boombastic.mobile.data.db.entity.DownloadState
import com.boombastic.mobile.data.db.entity.Track
import com.boombastic.mobile.playback.MediaTrack
import com.boombastic.mobile.playback.MusicController
import com.boombastic.mobile.playback.ExtractionResult
import com.boombastic.mobile.playback.WebSearchResult
import com.boombastic.mobile.playback.WebSearchService
import com.boombastic.mobile.ui.components.TrackActionsSheet
import com.boombastic.mobile.ui.theme.AccentGreen
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText
import com.boombastic.mobile.ui.theme.SurfaceDark
import com.boombastic.mobile.ui.theme.SurfaceElevated
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Live counters shown while a folder (desktop-style) import is running. */
private data class FolderImportState(
    val imported: Int,
    val duplicates: Int,
    val errors: Int
)

@Composable
fun SearchScreen(
    musicController: MusicController,
    onPlay: (MediaTrack) -> Unit = {}
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
            scope.launch {
                app.libraryRepository.importMultipleUris(uris)
            }
        }
    }

    // Desktop-style folder import: subfolders become playlists (folder name
    // = playlist name), files at the root land in "Unsorted".  The chosen
    // folder is persisted as the app's music-folder destination.
    var folderImportState by remember { mutableStateOf<FolderImportState?>(null) }
    val folderImportLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.OpenDocumentTree()
    ) { uri: Uri? ->
        if (uri != null) {
            folderImportState = FolderImportState(0, 0, 0)
            scope.launch {
                app.musicFolderRepository.saveTreeUri(uri)
                val result = app.libraryRepository.importLibraryTree(
                    treeUri = uri,
                    onProgress = { imported, duplicates, errors ->
                        folderImportState = FolderImportState(imported, duplicates, errors)
                    }
                )
                Toast.makeText(
                    context,
                    "Imported ${result.imported} songs " +
                        "(${result.duplicates} duplicates, ${result.errors} errors)",
                    Toast.LENGTH_LONG
                ).show()
            }
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
            color = PrimaryText,
            modifier = Modifier.padding(
                top = Dimens.paddingLarge,
                bottom = Dimens.paddingMedium
            )
        )

        TabRow(
            selectedTabIndex = tabIndex,
            containerColor = SurfaceDark,
            contentColor = AccentGreen,
            indicator = { tabPositions ->
                if (tabIndex < tabPositions.size) {
                    TabRowDefaults.SecondaryIndicator(
                        modifier = Modifier.tabIndicatorOffset(tabPositions[tabIndex]),
                        color = AccentGreen
                    )
                }
            }
        ) {
            Tab(
                selected = tabIndex == 0,
                onClick = { tabIndex = 0 },
                text = { Text("Library", color = if (tabIndex == 0) AccentGreen else SecondaryText) }
            )
            Tab(
                selected = tabIndex == 1,
                onClick = { tabIndex = 1 },
                text = { Text("Web Search", color = if (tabIndex == 1) AccentGreen else SecondaryText) }
            )
        }

        Spacer(modifier = Modifier.height(Dimens.paddingMedium))

        when (tabIndex) {
            0 -> LibrarySearchContent(
                query = query,
                onQueryChange = { query = it },
                displayTracks = if (query.isBlank()) emptyList() else searchResults,
                onPlay = onPlay,
                // `*/*` (not `audio/*`): with `audio/*` the system picker
                // hides folders from selection, so "Select all" only ever
                // returned the loose songs at the root and the playlist
                // folders were never imported.  `importMultipleUris` recurses
                // into picked folders itself.
                onImport = { importLauncher.launch(arrayOf("*/*")) },
                onImportFolder = { folderImportLauncher.launch(null) },
                folderImportState = folderImportState
            )
            1 -> WebSearchContent(
                downloadRepository = app.downloadRepository,
                scope = scope
            )
        }
    }
}

@Composable
private fun LibrarySearchContent(
    query: String,
    onQueryChange: (String) -> Unit,
    displayTracks: List<Track>,
    onPlay: (MediaTrack) -> Unit,
    onImport: () -> Unit,
    onImportFolder: () -> Unit,
    folderImportState: FolderImportState?
) {
    OutlinedTextField(
        value = query,
        onValueChange = onQueryChange,
        modifier = Modifier.fillMaxWidth(),
        placeholder = {
            Text(text = "Search your library...", color = SecondaryText)
        },
        leadingIcon = {
            Icon(
                imageVector = Icons.Default.Search,
                contentDescription = "Search",
                tint = SecondaryText
            )
        },
        singleLine = true,
        colors = OutlinedTextFieldDefaults.colors(
            focusedTextColor = PrimaryText,
            unfocusedTextColor = PrimaryText,
            cursorColor = AccentGreen,
            focusedBorderColor = AccentGreen,
            unfocusedBorderColor = SurfaceElevated,
            focusedContainerColor = SurfaceDark,
            unfocusedContainerColor = SurfaceDark
        )
    )

    Button(
        onClick = onImport,
        modifier = Modifier.padding(vertical = Dimens.paddingMedium),
        colors = ButtonDefaults.buttonColors(containerColor = AccentGreen)
    ) {
        Text(text = "Import songs or playlist folders", color = PrimaryText)
    }

    OutlinedButton(
        onClick = onImportFolder,
        modifier = Modifier.fillMaxWidth(),
        colors = ButtonDefaults.outlinedButtonColors(contentColor = AccentGreen)
    ) {
        Text("Import music folder (playlists by folder)")
    }

    folderImportState?.let { state ->
        Text(
            text = if (state.errors > 0) {
                "Importing... ${state.imported} added, ${state.duplicates} duplicates, ${state.errors} errors"
            } else {
                "Importing... ${state.imported} added, ${state.duplicates} duplicates"
            },
            color = AccentGreen,
            style = MaterialTheme.typography.bodySmall,
            modifier = Modifier.padding(top = Dimens.paddingSmall)
        )
    }

    if (query.isBlank()) {
        Box(
            modifier = Modifier.fillMaxSize(),
            contentAlignment = Alignment.Center
        ) {
            Text(
                text = "Songs appear here when you search.",
                color = SecondaryText,
                style = MaterialTheme.typography.bodyMedium
            )
        }
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
        // Track actions sheet — hoisted out of the LazyColumn: composing a
        // ModalBottomSheet inside a lazy item makes it scroll with the list.
        var actionsTrack by remember { mutableStateOf<Track?>(null) }
        LazyColumn(
            verticalArrangement = Arrangement.spacedBy(2.dp),
            contentPadding = PaddingValues(bottom = Dimens.paddingLarge)
        ) {
            items(displayTracks, key = { it.uri }) { track ->
                TrackRow(
                    track = track,
                    onClick = {
                        onPlay(
                            MediaTrack(
                                uri = track.uri,
                                title = track.title,
                                artist = track.artist,
                                album = track.album,
                                durationMs = track.durationMs,
                                artworkUri = track.albumArtUri()
                            )
                        )
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
                colors = OutlinedTextFieldDefaults.colors(
                    focusedTextColor = PrimaryText,
                    unfocusedTextColor = PrimaryText,
                    cursorColor = AccentGreen,
                    focusedBorderColor = AccentGreen,
                    unfocusedBorderColor = SurfaceElevated,
                    focusedContainerColor = SurfaceDark,
                    unfocusedContainerColor = SurfaceDark
                )
            )
            Spacer(modifier = Modifier.width(Dimens.paddingSmall))
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
                colors = ButtonDefaults.buttonColors(containerColor = AccentGreen)
            ) {
                Text("Search", color = PrimaryText)
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
                    .background(androidx.compose.ui.graphics.Color(0x33FF0000))
                    .padding(horizontal = Dimens.paddingSmall, vertical = Dimens.paddingSmall)
            )
        }

        // Toggle to show URL input
        Spacer(modifier = Modifier.height(Dimens.paddingSmall))
        TextButton(
            onClick = { showUrlInput = !showUrlInput },
            modifier = Modifier.fillMaxWidth()
        ) {
            Text(
                text = if (showUrlInput) "Hide URL input" else "Or paste a direct audio URL",
                color = AccentGreen,
                style = MaterialTheme.typography.labelLarge
            )
        }

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
        TextButton(
            onClick = { showCsvImport = !showCsvImport },
            modifier = Modifier.fillMaxWidth()
        ) {
            Text(
                text = if (showCsvImport) "Hide CSV import" else "Import Spotify CSV (Exportify)",
                color = AccentGreen,
                style = MaterialTheme.typography.labelLarge
            )
        }

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
                Text("Searching...", color = SecondaryText)
            }
        } else if (webResults.isNotEmpty()) {
            // Show results
            Text(
                text = "YouTube results (${visibleResults.size})",
                style = MaterialTheme.typography.titleSmall,
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
            colors = OutlinedTextFieldDefaults.colors(
                focusedTextColor = PrimaryText,
                unfocusedTextColor = PrimaryText,
                cursorColor = AccentGreen,
                focusedBorderColor = AccentGreen,
                unfocusedBorderColor = SurfaceElevated,
                focusedContainerColor = SurfaceDark,
                unfocusedContainerColor = SurfaceDark
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
                colors = OutlinedTextFieldDefaults.colors(
                    focusedTextColor = PrimaryText,
                    unfocusedTextColor = PrimaryText,
                    cursorColor = AccentGreen,
                    focusedBorderColor = AccentGreen,
                    unfocusedBorderColor = SurfaceElevated,
                    focusedContainerColor = SurfaceDark,
                    unfocusedContainerColor = SurfaceDark
                )
            )
            Spacer(modifier = Modifier.width(Dimens.paddingSmall))
            OutlinedTextField(
                value = trackArtist,
                onValueChange = onArtistChange,
                modifier = Modifier.weight(1f),
                placeholder = { Text("Artist", color = SecondaryText) },
                singleLine = true,
                colors = OutlinedTextFieldDefaults.colors(
                    focusedTextColor = PrimaryText,
                    unfocusedTextColor = PrimaryText,
                    cursorColor = AccentGreen,
                    focusedBorderColor = AccentGreen,
                    unfocusedBorderColor = SurfaceElevated,
                    focusedContainerColor = SurfaceDark,
                    unfocusedContainerColor = SurfaceDark
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
                androidx.compose.material3.CircularProgressIndicator(
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
                androidx.compose.material3.CircularProgressIndicator(
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

@Composable
private fun TextButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit
) {
    androidx.compose.material3.TextButton(
        onClick = onClick,
        modifier = modifier
    ) {
        content()
    }
}