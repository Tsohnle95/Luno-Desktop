package com.boombastic.mobile.ui.search

import android.net.Uri
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
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
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowForward
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Clear
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.ExpandLess
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material.icons.filled.Link
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.UploadFile
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.boombastic.mobile.BoomBasticApp
import com.boombastic.mobile.data.db.entity.DownloadJob
import com.boombastic.mobile.data.db.entity.DownloadState
import com.boombastic.mobile.data.db.entity.Track
import com.boombastic.mobile.playback.ExtractionResult
import com.boombastic.mobile.playback.WebSearchResult
import com.boombastic.mobile.playback.WebSearchService
import com.boombastic.mobile.ui.components.ArtworkImage
import com.boombastic.mobile.ui.components.PlaylistPickerSheet
import com.boombastic.mobile.ui.components.TrackActionsSheet
import com.boombastic.mobile.ui.components.TrackRowCard
import com.boombastic.mobile.ui.theme.AccentGreen
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText
import com.boombastic.mobile.ui.theme.SurfaceDark
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File

private val DownloadPanelPadding = 16.dp
private val DownloadControlSpacing = 12.dp
private val DownloadFieldSpacing = 8.dp

/**
 * The download hub. It helps the user bring audio into the local library,
 * inspect completed downloads, assign them to playlists, and manage the
 * background queue without turning the screen into a second full library.
 */
@Composable
fun SearchScreen(
    onOpenDownloads: () -> Unit = {}
) {
    val context = LocalContext.current
    val app = context.applicationContext as BoomBasticApp
    val scope = rememberCoroutineScope()
    val downloadRepository = app.downloadRepository
    val downloads by downloadRepository.getAllDownloads().collectAsState(initial = emptyList())
    val allTracks by app.libraryData.tracks.collectAsState()
    val downloadsRootUri = remember(context) {
        File(context.filesDir, "downloads").toURI().toString()
    }
    val downloadedTracks = remember(allTracks, downloadsRootUri) {
        allTracks
            .filter { it.uri.startsWith(downloadsRootUri) }
            .sortedByDescending { it.addedAt }
    }

    var searchQuery by rememberSaveable { mutableStateOf("") }
    var webResults by remember { mutableStateOf<List<WebSearchResult>>(emptyList()) }
    var isSearching by remember { mutableStateOf(false) }
    var errorMessage by remember { mutableStateOf("") }
    var showUrlInput by rememberSaveable { mutableStateOf(false) }
    var urlInput by rememberSaveable { mutableStateOf("") }
    var urlTitle by rememberSaveable { mutableStateOf("") }
    var urlArtist by rememberSaveable { mutableStateOf("") }
    var isQueuingUrl by remember { mutableStateOf(false) }
    var showCsvImport by rememberSaveable { mutableStateOf(false) }
    var csvImporting by remember { mutableStateOf(false) }
    var csvStatus by rememberSaveable { mutableStateOf("") }
    var downloadingVideoIds by remember { mutableStateOf(setOf<String>()) }
    var jobIdByVideoId by remember { mutableStateOf<Map<String, Long>>(emptyMap()) }
    var downloadedExpanded by rememberSaveable { mutableStateOf(false) }
    var downloadedSelectionMode by rememberSaveable { mutableStateOf(false) }
    var selectedDownloadedUris by remember { mutableStateOf(setOf<String>()) }
    var actionsTrack by remember { mutableStateOf<Track?>(null) }
    var playlistTracksToAdd by remember { mutableStateOf<List<Track>?>(null) }
    var playlistResultToQueue by remember { mutableStateOf<WebSearchResult?>(null) }

    fun search() {
        val query = searchQuery.trim()
        if (query.isBlank() || isSearching) return

        isSearching = true
        webResults = emptyList()
        errorMessage = ""
        scope.launch {
            try {
                when (val result = withContext(Dispatchers.IO) {
                    WebSearchService.searchYouTube(query)
                }) {
                    is ExtractionResult.Success -> webResults = result.data
                    is ExtractionResult.Error -> errorMessage = result.message
                }
            } catch (e: Exception) {
                errorMessage = "Search failed: ${e.message ?: "Try again."}"
            } finally {
                isSearching = false
            }
        }
    }

    fun queueSearchResult(result: WebSearchResult, playlistId: Long? = null) {
        val videoId = result.videoId
        if (videoId in downloadingVideoIds) return
        downloadingVideoIds = downloadingVideoIds + videoId
        scope.launch {
            try {
                val audioResult = withContext(Dispatchers.IO) {
                    WebSearchService.getAudioStreamUrl(videoId)
                }
                when (audioResult) {
                    is ExtractionResult.Success -> {
                        val titleParts = result.title.split(" - ", limit = 2)
                        val artist = if (titleParts.size > 1) titleParts[0].trim() else result.artist
                        val title = if (titleParts.size > 1) titleParts[1].trim() else result.title
                        val jobId = downloadRepository.enqueueDownload(
                            sourceUrl = audioResult.data.url,
                            title = title,
                            artist = artist,
                            playlistId = playlistId,
                            thumbnailUrl = result.thumbnailUrl
                        )
                        jobIdByVideoId = jobIdByVideoId + (videoId to jobId)
                        Toast.makeText(
                            context,
                            if (playlistId == null) "Download queued: $title"
                            else "Download queued for playlist: $title",
                            Toast.LENGTH_SHORT
                        ).show()
                    }
                    is ExtractionResult.Error -> {
                        errorMessage = "Could not prepare this download: ${audioResult.message}"
                    }
                }
            } catch (e: Exception) {
                errorMessage = "Download failed to start: ${e.message ?: "Try again."}"
            } finally {
                downloadingVideoIds = downloadingVideoIds - videoId
            }
        }
    }

    fun queueCsvRows(rows: List<Pair<String, String>>) {
        if (csvImporting) return

        scope.launch {
            csvImporting = true
            var queued = 0
            var skipped = 0
            try {
                rows.forEachIndexed { index, (artist, title) ->
                    csvStatus = "Finding ${index + 1} of ${rows.size}: $title"
                    val query = "$artist $title"
                    val searchResult = withContext(Dispatchers.IO) {
                        WebSearchService.searchYouTube(query, limit = 1)
                    }
                    if (searchResult !is ExtractionResult.Success || searchResult.data.isEmpty()) {
                        skipped++
                        return@forEachIndexed
                    }

                    val result = searchResult.data.first()
                    val audioResult = withContext(Dispatchers.IO) {
                        WebSearchService.getAudioStreamUrl(result.videoId)
                    }
                    if (audioResult is ExtractionResult.Success) {
                        downloadRepository.enqueueDownload(
                            sourceUrl = audioResult.data.url,
                            title = title,
                            artist = artist,
                            thumbnailUrl = result.thumbnailUrl
                        )
                        queued++
                    } else {
                        skipped++
                    }
                }
                csvStatus = "Queued $queued of ${rows.size} tracks" +
                    if (skipped > 0) " ($skipped could not be found)" else ""
                if (queued > 0) {
                    Toast.makeText(context, "$queued downloads queued", Toast.LENGTH_SHORT).show()
                }
            } catch (e: Exception) {
                csvStatus = "Batch download stopped: ${e.message ?: "Try again."}"
            } finally {
                csvImporting = false
            }
        }
    }

    val jobForResult: (WebSearchResult) -> DownloadJob? = { result ->
        jobIdByVideoId[result.videoId]?.let { id -> downloads.firstOrNull { it.id == id } }
    }

    val activeDownloads = downloads.count {
        it.state == DownloadState.QUEUED || it.state == DownloadState.DOWNLOADING
    }

    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        verticalArrangement = Arrangement.spacedBy(0.dp),
        contentPadding = PaddingValues(
            start = Dimens.paddingLarge,
            end = Dimens.paddingLarge,
            top = Dimens.paddingLarge,
            bottom = 32.dp
        )
    ) {
        item(key = "header") {
            DownloadHeader(
                query = searchQuery,
                onQueryChange = {
                    searchQuery = it
                    errorMessage = ""
                    if (it.isBlank()) webResults = emptyList()
                },
                onSearch = ::search,
                isSearching = isSearching,
                activeDownloads = activeDownloads,
                onOpenDownloads = onOpenDownloads
            )
        }

        item(key = "downloaded-header") {
            DownloadOptionRow(
                icon = Icons.Filled.Download,
                title = "Downloaded songs",
                description = if (downloadedTracks.isEmpty()) {
                    "No downloaded songs yet"
                } else {
                    "${downloadedTracks.size} songs in this app's private Downloads folder"
                },
                expanded = downloadedExpanded,
                onClick = { downloadedExpanded = !downloadedExpanded }
            )
        }

        if (downloadedExpanded) {
            item(key = "downloaded-actions") {
                DownloadedSongsActions(
                    trackCount = downloadedTracks.size,
                    selectedCount = selectedDownloadedUris.size,
                    selectionMode = downloadedSelectionMode,
                    onToggleSelectionMode = {
                        downloadedSelectionMode = !downloadedSelectionMode
                        if (downloadedSelectionMode) {
                            selectedDownloadedUris = emptySet()
                        } else {
                            selectedDownloadedUris = emptySet()
                        }
                    },
                    onSelectAll = {
                        selectedDownloadedUris = downloadedTracks.map { it.uri }.toSet()
                    },
                    onAddToPlaylist = {
                        playlistTracksToAdd = downloadedTracks.filter {
                            it.uri in selectedDownloadedUris
                        }
                    }
                )
            }
            if (downloadedTracks.isEmpty()) {
                item(key = "downloaded-empty") {
                    Text(
                        text = "Completed downloads will appear here.",
                        style = MaterialTheme.typography.bodySmall,
                        color = SecondaryText,
                        modifier = Modifier.padding(vertical = 8.dp)
                    )
                }
            } else {
                items(downloadedTracks, key = { "downloaded-${it.uri}" }) { track ->
                    TrackRowCard(
                        track = track,
                        onClick = {
                            if (downloadedSelectionMode) {
                                selectedDownloadedUris = if (track.uri in selectedDownloadedUris) {
                                    selectedDownloadedUris - track.uri
                                } else {
                                    selectedDownloadedUris + track.uri
                                }
                            }
                        },
                        onMenuClick = { actionsTrack = track },
                        onLongClick = { actionsTrack = track },
                        selected = if (downloadedSelectionMode) {
                            track.uri in selectedDownloadedUris
                        } else {
                            null
                        }
                    )
                    HorizontalDivider(color = Color.White.copy(alpha = 0.08f))
                }
            }
        }

        if (errorMessage.isNotBlank()) {
            item(key = "error") {
                MessageBox(text = errorMessage, isError = true)
            }
        }

        if (isSearching) {
            item(key = "searching") {
                SearchLoadingState()
            }
        } else if (webResults.isNotEmpty()) {
            item(key = "results-heading") {
                ResultsHeader(query = searchQuery, count = webResults.size)
            }
            items(webResults, key = { it.videoId }) { result ->
                val job = jobForResult(result)
                WebResultRow(
                    result = result,
                    job = job,
                    extractingAudio = result.videoId in downloadingVideoIds,
                    onDownload = { queueSearchResult(result) },
                    onSaveToPlaylist = {
                        val job = jobForResult(result)
                        if (job?.state == DownloadState.COMPLETED) {
                            val track = allTracks.firstOrNull { it.uri == job.localUri }
                            if (track == null) {
                                Toast.makeText(context, "Downloaded track is still loading", Toast.LENGTH_SHORT).show()
                            } else {
                                playlistTracksToAdd = listOf(track)
                            }
                        } else if (job == null) {
                            playlistResultToQueue = result
                        } else {
                            Toast.makeText(context, "Wait for this download to finish first", Toast.LENGTH_SHORT).show()
                        }
                    },
                    onCancel = { jobId ->
                        scope.launch { downloadRepository.cancelDownload(jobId) }
                    },
                    onRetry = { jobId ->
                        scope.launch { downloadRepository.retryDownload(jobId) }
                    }
                )
            }
        } else if (searchQuery.isNotBlank() && errorMessage.isBlank()) {
            item(key = "no-results") {
                MessageBox(
                    text = "No matches yet. Try adding the artist or a different title."
                )
            }
        }

        item(key = "source-options") {
            SourceOptions(
                showUrlInput = showUrlInput,
                showCsvImport = showCsvImport,
                onUrlClick = {
                    val next = !showUrlInput
                    showUrlInput = next
                    if (next) showCsvImport = false
                },
                onCsvClick = {
                    val next = !showCsvImport
                    showCsvImport = next
                    if (next) showUrlInput = false
                }
            )
        }

        if (showUrlInput) {
            item(key = "direct-url-form") {
                UrlDownloadSection(
                    urlInput = urlInput,
                    onUrlChange = {
                        urlInput = it
                        errorMessage = ""
                    },
                    trackTitle = urlTitle,
                    onTitleChange = { urlTitle = it },
                    trackArtist = urlArtist,
                    onArtistChange = { urlArtist = it },
                    isQueuing = isQueuingUrl,
                    onDownload = {
                        if (urlInput.isBlank() || isQueuingUrl) return@UrlDownloadSection
                        isQueuingUrl = true
                        scope.launch {
                            try {
                                downloadRepository.enqueueDownload(
                                    sourceUrl = urlInput.trim(),
                                    title = urlTitle.trim().ifBlank { "Direct audio download" },
                                    artist = urlArtist.trim()
                                )
                                urlInput = ""
                                urlTitle = ""
                                urlArtist = ""
                                Toast.makeText(context, "Download queued", Toast.LENGTH_SHORT).show()
                            } catch (e: Exception) {
                                errorMessage = "Could not queue this link: ${e.message ?: "Try again."}"
                            } finally {
                                isQueuingUrl = false
                            }
                        }
                    }
                )
            }
        }

        if (showCsvImport) {
            item(key = "csv-form") {
                CsvImportSection(
                    isImporting = csvImporting,
                    status = csvStatus,
                    onTracksLoaded = ::queueCsvRows
                )
            }
        }
    }

    playlistTracksToAdd?.let { tracks ->
        PlaylistPickerSheet(
            onPick = { playlist ->
                playlistTracksToAdd = null
                scope.launch {
                    tracks.forEach { track ->
                        app.playlistRepository.addTrackToPlaylist(playlist.id, track.uri)
                    }
                    Toast.makeText(
                        context,
                        if (tracks.size == 1) {
                            "Added to ${playlist.name}"
                        } else {
                            "Added ${tracks.size} songs to ${playlist.name}"
                        },
                        Toast.LENGTH_SHORT
                    ).show()
                }
            },
            onDismiss = { playlistTracksToAdd = null }
        )
    }

    playlistResultToQueue?.let { result ->
        PlaylistPickerSheet(
            onPick = { playlist ->
                playlistResultToQueue = null
                queueSearchResult(result, playlist.id)
            },
            onDismiss = { playlistResultToQueue = null }
        )
    }

    actionsTrack?.let { track ->
        TrackActionsSheet(
            track = track,
            onDismiss = { actionsTrack = null }
        )
    }
}

@Composable
private fun DownloadHeader(
    query: String,
    onQueryChange: (String) -> Unit,
    onSearch: () -> Unit,
    isSearching: Boolean,
    activeDownloads: Int,
    onOpenDownloads: () -> Unit
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(bottom = 8.dp)
    ) {
        Text(
            text = "Download music",
            style = MaterialTheme.typography.headlineMedium,
            fontWeight = FontWeight.Bold,
            color = PrimaryText
        )
        Text(
            text = "Search YouTube, paste an audio link, or import an Exportify playlist. Downloaded files stay in this app's private Downloads folder, appear in Your Library, and can finish in the background.",
            style = MaterialTheme.typography.bodyMedium,
            color = SecondaryText,
            modifier = Modifier.padding(top = 4.dp)
        )

        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(top = 16.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            OutlinedTextField(
                value = query,
                onValueChange = onQueryChange,
                modifier = Modifier.weight(1f),
                placeholder = { Text("Search for music to add", color = SecondaryText) },
                leadingIcon = {
                    Icon(
                        imageVector = Icons.Filled.Search,
                        contentDescription = null,
                        tint = SecondaryText
                    )
                },
                trailingIcon = {
                    if (query.isNotBlank()) {
                        IconButton(onClick = { onQueryChange("") }) {
                            Icon(
                                imageVector = Icons.Filled.Clear,
                                contentDescription = "Clear search",
                                tint = SecondaryText
                            )
                        }
                    }
                },
                singleLine = true,
                shape = RoundedCornerShape(14.dp),
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                keyboardActions = KeyboardActions(onSearch = { onSearch() }),
                colors = downloadFieldColors()
            )
            Spacer(modifier = Modifier.width(8.dp))
            val searchShape = RoundedCornerShape(14.dp)
            IconButton(
                onClick = onSearch,
                enabled = query.isNotBlank() && !isSearching,
                modifier = Modifier
                    .size(52.dp)
                    .clip(searchShape)
                    .background(
                        if (query.isNotBlank() && !isSearching) AccentGreen else SurfaceDark
                    )
                    .border(
                        1.dp,
                        if (query.isNotBlank() && !isSearching) {
                            AccentGreen
                        } else {
                            Color.White.copy(alpha = 0.08f)
                        },
                        searchShape
                    )
            ) {
                if (isSearching) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(Dimens.iconSizeSmall),
                        color = AccentGreen,
                        strokeWidth = 2.dp
                    )
                } else {
                    Icon(
                        imageVector = Icons.Filled.Search,
                        contentDescription = "Search for music",
                        tint = if (query.isNotBlank()) Color.Black else SecondaryText,
                        modifier = Modifier.size(Dimens.iconSizeSmall)
                    )
                }
            }
        }

        if (activeDownloads > 0) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = 8.dp)
                    .clickable(
                        interactionSource = remember { MutableInteractionSource() },
                        indication = null,
                        onClick = onOpenDownloads
                    )
                    .padding(vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                CircularProgressIndicator(
                    modifier = Modifier.size(15.dp),
                    color = AccentGreen,
                    strokeWidth = 2.dp
                )
                Spacer(modifier = Modifier.width(8.dp))
                Text(
                    text = if (activeDownloads == 1) {
                        "1 download in progress"
                    } else {
                        "$activeDownloads downloads in progress"
                    },
                    style = MaterialTheme.typography.bodySmall,
                    color = SecondaryText,
                    modifier = Modifier.weight(1f)
                )
                Text(
                    text = "View queue",
                    style = MaterialTheme.typography.labelLarge,
                    color = AccentGreen
                )
                Spacer(modifier = Modifier.width(4.dp))
                Icon(
                    imageVector = Icons.AutoMirrored.Filled.ArrowForward,
                    contentDescription = "Open downloads",
                    tint = AccentGreen,
                    modifier = Modifier.size(17.dp)
                )
            }
        }
    }
}

@Composable
private fun SearchLoadingState() {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        CircularProgressIndicator(
            modifier = Modifier.size(Dimens.iconSizeSmall),
            color = AccentGreen,
            strokeWidth = 2.dp
        )
        Spacer(modifier = Modifier.width(Dimens.paddingSmall))
        Text("Searching…", color = SecondaryText, style = MaterialTheme.typography.bodyMedium)
    }
}

@Composable
private fun ResultsHeader(query: String, count: Int) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(top = 8.dp, bottom = 4.dp)
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Text(
                text = "Music to add",
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Bold,
                color = PrimaryText,
                modifier = Modifier.weight(1f)
            )
            Text(
                text = "$count matches",
                style = MaterialTheme.typography.labelMedium,
                color = SecondaryText
            )
        }
        Text(
            text = "For \"${query.trim()}\"",
            style = MaterialTheme.typography.bodySmall,
            color = SecondaryText,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.padding(top = 3.dp)
        )
    }
}

@Composable
private fun SourceOptions(
    showUrlInput: Boolean,
    showCsvImport: Boolean,
    onUrlClick: () -> Unit,
    onCsvClick: () -> Unit
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(top = 24.dp, bottom = 8.dp)
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Text(
                text = "More ways to add",
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Bold,
                color = PrimaryText,
                modifier = Modifier.weight(1f)
            )
            Text(
                text = "Optional",
                style = MaterialTheme.typography.labelMedium,
                color = SecondaryText
            )
        }
        Spacer(modifier = Modifier.height(8.dp))
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(14.dp))
                .background(SurfaceDark)
                .padding(horizontal = 14.dp)
        ) {
            DownloadOptionRow(
                icon = Icons.Filled.Link,
                title = "Direct audio link",
                description = "Add one audio file from a URL",
                expanded = showUrlInput,
                onClick = onUrlClick
            )
            HorizontalDivider(color = Color.White.copy(alpha = 0.08f))
            DownloadOptionRow(
                icon = Icons.Filled.UploadFile,
                title = "Exportify playlist",
                description = "Match and queue an exported playlist",
                expanded = showCsvImport,
                onClick = onCsvClick
            )
        }
    }
}

@Composable
private fun MessageBox(
    text: String,
    isError: Boolean = false
) {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(Dimens.cornerMedium))
            .background(
                if (isError) {
                    MaterialTheme.colorScheme.error.copy(alpha = 0.14f)
                } else {
                    SurfaceDark
                }
            )
            .padding(Dimens.paddingMedium)
    ) {
        Text(
            text = text,
            style = MaterialTheme.typography.bodyMedium,
            color = if (isError) MaterialTheme.colorScheme.error else SecondaryText
        )
    }
}

@Composable
private fun DownloadOptionRow(
    icon: ImageVector,
    title: String,
    description: String,
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
            .padding(vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Box(
            modifier = Modifier
                .size(36.dp)
                .clip(RoundedCornerShape(10.dp))
                .background(AccentGreen.copy(alpha = if (expanded) 0.2f else 0.12f)),
            contentAlignment = Alignment.Center
        ) {
            Icon(
                imageVector = icon,
                contentDescription = null,
                tint = AccentGreen,
                modifier = Modifier.size(19.dp)
            )
        }
        Spacer(modifier = Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = title,
                style = MaterialTheme.typography.titleSmall,
                color = PrimaryText,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
            Text(
                text = description,
                style = MaterialTheme.typography.bodySmall,
                color = SecondaryText,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.padding(top = 2.dp)
            )
        }
        Spacer(modifier = Modifier.width(8.dp))
        Icon(
            imageVector = if (expanded) Icons.Filled.ExpandLess else Icons.Filled.ExpandMore,
            contentDescription = if (expanded) "Hide $title" else "Show $title",
            tint = if (expanded) AccentGreen else SecondaryText,
            modifier = Modifier.size(20.dp)
        )
    }
}

@Composable
private fun DownloadedSongsActions(
    trackCount: Int,
    selectedCount: Int,
    selectionMode: Boolean,
    onToggleSelectionMode: () -> Unit,
    onSelectAll: () -> Unit,
    onAddToPlaylist: () -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Text(
            text = if (selectionMode) "$selectedCount selected" else "$trackCount songs",
            style = MaterialTheme.typography.labelMedium,
            color = SecondaryText,
            modifier = Modifier.weight(1f)
        )
        if (selectionMode) {
            TextButton(
                onClick = onSelectAll,
                enabled = trackCount > 0 && selectedCount < trackCount,
                contentPadding = PaddingValues(horizontal = 6.dp)
            ) {
                Text("Select all", color = AccentGreen)
            }
            TextButton(
                onClick = onAddToPlaylist,
                enabled = selectedCount > 0,
                contentPadding = PaddingValues(horizontal = 6.dp)
            ) {
                Text("Add to playlist", color = AccentGreen)
            }
        }
        TextButton(
            onClick = onToggleSelectionMode,
            contentPadding = PaddingValues(horizontal = 6.dp)
        ) {
            Text(if (selectionMode) "Done" else "Select", color = AccentGreen)
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
    isQueuing: Boolean,
    onDownload: () -> Unit
) {
    var showDetails by rememberSaveable { mutableStateOf(false) }
    val panelShape = RoundedCornerShape(18.dp)
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(panelShape)
            .background(SurfaceDark)
            .border(1.dp, Color.White.copy(alpha = 0.06f), panelShape)
            .padding(DownloadPanelPadding)
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(
                imageVector = Icons.Filled.Link,
                contentDescription = null,
                tint = AccentGreen,
                modifier = Modifier.size(20.dp)
            )
            Spacer(modifier = Modifier.width(Dimens.paddingSmall))
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = "Direct audio link",
                    style = MaterialTheme.typography.titleMedium,
                    color = PrimaryText
                )
                Text(
                    text = "Save one audio file on this device and add it to Your Library.",
                    style = MaterialTheme.typography.bodySmall,
                    color = SecondaryText,
                    modifier = Modifier.padding(top = 2.dp)
                )
            }
        }
        Spacer(modifier = Modifier.height(DownloadFieldSpacing))
        OutlinedTextField(
            value = urlInput,
            onValueChange = onUrlChange,
            modifier = Modifier.fillMaxWidth(),
            placeholder = { Text("Paste an audio link", color = SecondaryText) },
            singleLine = true,
            shape = RoundedCornerShape(12.dp),
            colors = downloadFieldColors()
        )
        TextButton(
            onClick = { showDetails = !showDetails },
            contentPadding = PaddingValues(horizontal = 0.dp, vertical = 2.dp)
        ) {
            Text(
                text = if (showDetails) "Hide details" else "Add details",
                color = AccentGreen,
                style = MaterialTheme.typography.labelLarge
            )
            Icon(
                imageVector = if (showDetails) Icons.Filled.ExpandLess else Icons.Filled.ExpandMore,
                contentDescription = null,
                tint = AccentGreen,
                modifier = Modifier.size(18.dp)
            )
        }
        if (showDetails) {
            OutlinedTextField(
                value = trackTitle,
                onValueChange = onTitleChange,
                modifier = Modifier.fillMaxWidth(),
                label = { Text("Title") },
                singleLine = true,
                shape = RoundedCornerShape(12.dp),
                colors = downloadFieldColors()
            )
            Spacer(modifier = Modifier.height(DownloadFieldSpacing))
            OutlinedTextField(
                value = trackArtist,
                onValueChange = onArtistChange,
                modifier = Modifier.fillMaxWidth(),
                label = { Text("Artist") },
                singleLine = true,
                shape = RoundedCornerShape(12.dp),
                colors = downloadFieldColors()
            )
        }
        Spacer(modifier = Modifier.height(DownloadControlSpacing))
        Button(
            onClick = onDownload,
            enabled = urlInput.isNotBlank() && !isQueuing,
            colors = ButtonDefaults.buttonColors(
                containerColor = AccentGreen,
                contentColor = Color.Black
            ),
            modifier = Modifier.fillMaxWidth(),
            shape = RoundedCornerShape(Dimens.cornerMedium)
        ) {
            if (isQueuing) {
                CircularProgressIndicator(
                    modifier = Modifier.size(Dimens.iconSizeSmall),
                    color = Color.Black,
                    strokeWidth = 2.dp
                )
            } else {
                Icon(
                    imageVector = Icons.Filled.Download,
                    contentDescription = null,
                    modifier = Modifier.size(Dimens.iconSizeSmall)
                )
            }
            Spacer(modifier = Modifier.width(Dimens.paddingSmall))
            Text(
                text = if (isQueuing) "Adding…" else "Queue link",
                fontWeight = FontWeight.SemiBold
            )
        }
    }
}

@Composable
private fun CsvImportSection(
    isImporting: Boolean,
    status: String,
    onTracksLoaded: (List<Pair<String, String>>) -> Unit
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val launcher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.OpenDocument()
    ) { uri: Uri? ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            val rows = withContext(Dispatchers.IO) {
                try {
                    context.contentResolver.openInputStream(uri)?.use { input ->
                        input.bufferedReader().useLines { lines ->
                            lines.drop(1).mapNotNull { line ->
                                val parts = line.split("\",\"")
                                if (parts.size < 2) return@mapNotNull null
                                val title = parts[0].trim('"', ' ')
                                val artist = parts[1].trim('"', ' ')
                                if (title.isBlank() || artist.isBlank()) {
                                    null
                                } else {
                                    artist to title
                                }
                            }.toList()
                        }
                    } ?: emptyList()
                } catch (_: Exception) {
                    emptyList()
                }
            }
            if (rows.isEmpty()) {
                Toast.makeText(context, "No tracks found in that CSV", Toast.LENGTH_SHORT).show()
            } else {
                onTracksLoaded(rows)
            }
        }
    }

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .background(SurfaceDark)
            .border(1.dp, Color.White.copy(alpha = 0.06f), RoundedCornerShape(14.dp))
            .padding(DownloadPanelPadding)
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(
                imageVector = Icons.Filled.UploadFile,
                contentDescription = null,
                tint = AccentGreen,
                modifier = Modifier.size(20.dp)
            )
            Spacer(modifier = Modifier.width(Dimens.paddingSmall))
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = "Exportify playlist",
                    style = MaterialTheme.typography.titleMedium,
                    color = PrimaryText
                )
                Text(
                    text = "Match a Spotify CSV export and queue its tracks into this local library.",
                    style = MaterialTheme.typography.bodySmall,
                    color = SecondaryText,
                    modifier = Modifier.padding(top = 2.dp)
                )
            }
        }
        Spacer(modifier = Modifier.height(DownloadControlSpacing))
        OutlinedButton(
            onClick = { launcher.launch(arrayOf("text/*", "*/*")) },
            enabled = !isImporting,
            colors = ButtonDefaults.outlinedButtonColors(contentColor = AccentGreen),
            modifier = Modifier.fillMaxWidth(),
            shape = RoundedCornerShape(Dimens.cornerMedium)
        ) {
            Icon(
                imageVector = Icons.Filled.UploadFile,
                contentDescription = null,
                modifier = Modifier.size(Dimens.iconSizeSmall)
            )
            Spacer(modifier = Modifier.width(Dimens.paddingSmall))
            if (isImporting) {
                CircularProgressIndicator(
                    modifier = Modifier.size(Dimens.iconSizeSmall),
                    color = AccentGreen,
                    strokeWidth = 2.dp
                )
                Spacer(modifier = Modifier.width(Dimens.paddingSmall))
            }
            Text(if (isImporting) "Finding tracks…" else "Choose CSV file")
        }
        if (isImporting || status.isNotBlank()) {
            Text(
                text = status,
                style = MaterialTheme.typography.bodySmall,
                color = if (isImporting) AccentGreen else SecondaryText,
                modifier = Modifier.padding(top = DownloadFieldSpacing)
            )
        }
    }
}

@Composable
private fun WebResultRow(
    result: WebSearchResult,
    job: DownloadJob?,
    onDownload: () -> Unit,
    onSaveToPlaylist: () -> Unit,
    onCancel: (Long) -> Unit,
    onRetry: (Long) -> Unit,
    extractingAudio: Boolean
) {
    val duration = formatDuration(result.duration)
    val metadata = listOfNotNull(
        result.artist.takeIf { it.isNotBlank() },
        duration.takeIf { it.isNotBlank() }
    ).joinToString(" • ")

    Column(modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            ArtworkImage(
                artworkUri = result.thumbnailUrl,
                modifier = Modifier
                    .size(60.dp)
                    .clip(RoundedCornerShape(10.dp)),
                placeholderIconSize = 20.dp,
                decodeSizePx = 128
            )
            Spacer(modifier = Modifier.width(12.dp))
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = result.title,
                    style = MaterialTheme.typography.titleSmall,
                    color = PrimaryText,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis
                )
                if (metadata.isNotBlank()) {
                    Text(
                        text = metadata,
                        style = MaterialTheme.typography.bodySmall,
                        color = SecondaryText,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.padding(top = 3.dp)
                    )
                }
                val error = job?.takeIf { it.state == DownloadState.FAILED }
                    ?.errorMessage
                    ?.takeIf { it.isNotBlank() }
                if (error != null) {
                    Text(
                        text = error,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.error,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.padding(top = 3.dp)
                    )
                }
            }
            Spacer(modifier = Modifier.width(8.dp))
            ResultAction(
                job = job,
                extractingAudio = extractingAudio,
                onDownload = onDownload,
                onSaveToPlaylist = onSaveToPlaylist,
                onCancel = onCancel,
                onRetry = onRetry
            )
        }
        HorizontalDivider(color = Color.White.copy(alpha = 0.08f))
    }
}

@Composable
private fun ResultAction(
    job: DownloadJob?,
    extractingAudio: Boolean,
    onDownload: () -> Unit,
    onSaveToPlaylist: () -> Unit,
    onCancel: (Long) -> Unit,
    onRetry: (Long) -> Unit
) {
    when {
        job?.state == DownloadState.COMPLETED -> {
            Column(
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(2.dp)
            ) {
                Icon(
                    imageVector = Icons.Filled.CheckCircle,
                    contentDescription = "Downloaded",
                    tint = AccentGreen,
                    modifier = Modifier.size(22.dp)
                )
                Text(
                    text = "Saved",
                    style = MaterialTheme.typography.labelSmall,
                    color = AccentGreen
                )
                TextButton(
                    onClick = onSaveToPlaylist,
                    contentPadding = PaddingValues(horizontal = 4.dp, vertical = 0.dp)
                ) {
                    Text("Save to playlist", color = AccentGreen, maxLines = 1)
                }
            }
        }
        job?.state == DownloadState.QUEUED || job?.state == DownloadState.DOWNLOADING -> {
            Column(
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(2.dp)
            ) {
                Text(
                    text = if (job.state == DownloadState.DOWNLOADING) "${job.progress}%" else "Waiting",
                    style = MaterialTheme.typography.labelSmall,
                    color = AccentGreen
                )
                TextButton(
                    onClick = { onCancel(job.id) },
                    contentPadding = PaddingValues(horizontal = 4.dp, vertical = 0.dp)
                ) {
                    Text("Stop", color = SecondaryText)
                }
            }
        }
        job?.state == DownloadState.FAILED -> {
            OutlinedButton(
                onClick = { onRetry(job.id) },
                contentPadding = PaddingValues(horizontal = 10.dp, vertical = 0.dp),
                colors = ButtonDefaults.outlinedButtonColors(contentColor = AccentGreen),
                shape = RoundedCornerShape(Dimens.cornerMedium)
            ) {
                Icon(
                    imageVector = Icons.Filled.Refresh,
                    contentDescription = null,
                    modifier = Modifier.size(Dimens.iconSizeSmall)
                )
                Spacer(modifier = Modifier.width(4.dp))
                Text("Retry")
            }
        }
        extractingAudio -> {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                CircularProgressIndicator(
                    modifier = Modifier.size(Dimens.iconSizeSmall),
                    color = AccentGreen,
                    strokeWidth = 2.dp
                )
                Text(
                    text = "Getting ready",
                    style = MaterialTheme.typography.labelSmall,
                    color = SecondaryText
                )
            }
        }
        else -> {
            val actionShape = RoundedCornerShape(12.dp)
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                IconButton(
                    onClick = onDownload,
                    modifier = Modifier
                        .size(44.dp)
                        .clip(actionShape)
                        .background(AccentGreen.copy(alpha = 0.14f))
                        .border(1.dp, AccentGreen.copy(alpha = 0.38f), actionShape)
                ) {
                    Icon(
                        imageVector = Icons.Filled.Download,
                        contentDescription = "Download ${job?.title ?: "song"}",
                        tint = AccentGreen,
                        modifier = Modifier.size(20.dp)
                    )
                }
                TextButton(
                    onClick = onSaveToPlaylist,
                    contentPadding = PaddingValues(horizontal = 4.dp, vertical = 0.dp)
                ) {
                    Text("Save to playlist", color = AccentGreen, maxLines = 1)
                }
            }
        }
    }
}

@Composable
private fun downloadFieldColors() = OutlinedTextFieldDefaults.colors(
    focusedTextColor = PrimaryText,
    unfocusedTextColor = PrimaryText,
    focusedLabelColor = AccentGreen,
    unfocusedLabelColor = SecondaryText,
    cursorColor = AccentGreen,
    focusedBorderColor = AccentGreen,
    unfocusedBorderColor = Color.White.copy(alpha = 0.12f),
    focusedContainerColor = SurfaceDark,
    unfocusedContainerColor = SurfaceDark
)

private fun formatDuration(seconds: Long): String {
    if (seconds <= 0) return ""
    val mins = seconds / 60
    val secs = seconds % 60
    return "%d:%02d".format(mins, secs)
}
