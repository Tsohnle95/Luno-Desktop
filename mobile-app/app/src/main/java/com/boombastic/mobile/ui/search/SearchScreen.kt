package com.boombastic.mobile.ui.search

import android.net.Uri
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
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
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Download
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
import com.boombastic.mobile.ui.theme.AccentGreen
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText
import com.boombastic.mobile.ui.theme.SurfaceDark
import com.boombastic.mobile.ui.theme.SurfaceElevated
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

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
    val allTracks by app.libraryRepository.getAllTracks().collectAsState(initial = emptyList())

    val importLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.OpenMultipleDocuments()
    ) { uris: List<Uri> ->
        if (uris.isNotEmpty()) {
            scope.launch {
                app.libraryRepository.importMultipleUris(uris)
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
                displayTracks = if (query.isBlank()) allTracks else searchResults,
                onPlay = onPlay,
                onImport = { importLauncher.launch(arrayOf("audio/*")) }
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
    onImport: () -> Unit
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
        Text(text = "Import audio files", color = PrimaryText)
    }

    if (displayTracks.isEmpty()) {
        Box(
            modifier = Modifier.fillMaxSize(),
            contentAlignment = Alignment.Center
        ) {
            Text(
                text = if (query.isBlank()) "Import audio to see your tracks here"
                else "No results for \"$query\"",
                color = SecondaryText,
                style = MaterialTheme.typography.bodyMedium
            )
        }
    } else {
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
                    }
                )
            }
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
                text = "YouTube results (${webResults.size})",
                style = MaterialTheme.typography.titleSmall,
                color = PrimaryText,
                modifier = Modifier.padding(bottom = Dimens.paddingSmall)
            )
            LazyColumn(
                modifier = Modifier.weight(1f),
                verticalArrangement = Arrangement.spacedBy(2.dp),
                contentPadding = PaddingValues(bottom = Dimens.paddingSmall)
            ) {
                items(webResults, key = { it.videoId }) { result ->
                    WebResultRow(
                        result = result,
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
                                            downloadRepository.enqueueDownload(
                                                sourceUrl = audioResult.data.url,
                                                title = trackTitle,
                                                artist = artist,
                                                thumbnailUrl = result.thumbnailUrl
                                            )
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
                        isDownloading = downloads.any { it.title == result.title && it.state == DownloadState.DOWNLOADING },
                        isDone = downloads.any { it.title == result.title && it.state == DownloadState.COMPLETED },
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

        // Download history
        if (downloads.isNotEmpty()) {
            Text(
                text = "Recent Downloads",
                style = MaterialTheme.typography.titleSmall,
                color = PrimaryText,
                modifier = Modifier.padding(bottom = Dimens.paddingSmall)
            )
            LazyColumn(
                modifier = Modifier.weight(1f),
                verticalArrangement = Arrangement.spacedBy(2.dp),
                contentPadding = PaddingValues(bottom = Dimens.paddingLarge)
            ) {
                items(downloads, key = { it.id }) { job ->
                    DownloadJobRow(
                        job = job,
                        onCancel = { scope.launch { downloadRepository.cancelDownload(job.id) } },
                        onRetry = { scope.launch { downloadRepository.retryDownload(job.id) } },
                        onDelete = { scope.launch { downloadRepository.deleteDownload(job.id) } }
                    )
                }
            }
        }

        if (webResults.isEmpty() && !isSearching && searchQuery.isBlank()) {
            Box(
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth(),
                contentAlignment = Alignment.Center
            ) {
                Text(
                    text = if (downloads.isEmpty()) "Search YouTube for songs, then download them to your library."
                    else "",
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
    onDownload: () -> Unit,
    isDownloading: Boolean,
    isDone: Boolean,
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

        when {
            isDone -> {
                Icon(
                    Icons.Default.Close,
                    contentDescription = "Downloaded",
                    tint = AccentGreen,
                    modifier = Modifier.size(Dimens.iconSize)
                )
            }
            isDownloading -> {
                Icon(
                    Icons.Default.Download,
                    contentDescription = "Downloading",
                    tint = AccentGreen,
                    modifier = Modifier.size(Dimens.iconSize)
                )
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

@Composable
private fun DownloadJobRow(
    job: DownloadJob,
    onCancel: () -> Unit,
    onRetry: () -> Unit,
    onDelete: () -> Unit
) {
    val stateColor = when (job.state) {
        DownloadState.QUEUED -> SecondaryText
        DownloadState.DOWNLOADING -> AccentGreen
        DownloadState.COMPLETED -> AccentGreen
        DownloadState.FAILED -> MaterialTheme.colorScheme.error
        DownloadState.CANCELLED -> SecondaryText
    }

    val stateLabel = when (job.state) {
        DownloadState.QUEUED -> "Queued"
        DownloadState.DOWNLOADING -> "Downloading ${job.progress}%"
        DownloadState.COMPLETED -> "Completed"
        DownloadState.FAILED -> job.errorMessage.ifBlank { "Failed" }
        DownloadState.CANCELLED -> "Cancelled"
    }

    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = Dimens.paddingSmall, horizontal = Dimens.paddingSmall),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = job.title,
                style = MaterialTheme.typography.titleSmall,
                color = PrimaryText,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
            if (job.artist.isNotBlank()) {
                Text(
                    text = job.artist,
                    style = MaterialTheme.typography.bodySmall,
                    color = SecondaryText,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis
                )
            }
            Text(
                text = stateLabel,
                style = MaterialTheme.typography.labelSmall,
                color = stateColor
            )
        }

        when (job.state) {
            DownloadState.QUEUED, DownloadState.DOWNLOADING -> {
                IconButton(onClick = onCancel) {
                    Icon(Icons.Default.Close, contentDescription = "Cancel", tint = SecondaryText)
                }
            }
            DownloadState.FAILED -> {
                OutlinedButton(
                    onClick = onRetry,
                    colors = ButtonDefaults.outlinedButtonColors(contentColor = AccentGreen)
                ) { Text("Retry") }
                Spacer(modifier = Modifier.width(Dimens.paddingSmall))
                IconButton(onClick = onDelete) {
                    Icon(Icons.Default.Close, contentDescription = "Delete", tint = SecondaryText)
                }
            }
            DownloadState.COMPLETED, DownloadState.CANCELLED -> {
                IconButton(onClick = onDelete) {
                    Icon(Icons.Default.Close, contentDescription = "Dismiss", tint = SecondaryText)
                }
            }
        }
    }
}

@Composable
fun TrackRow(
    track: Track,
    onClick: () -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
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