package com.luno.mobile.ui.search

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
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
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
import androidx.compose.material3.LinearProgressIndicator
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
import com.luno.mobile.LunoApp
import com.luno.mobile.data.artwork.ArtworkStorage
import com.luno.mobile.data.db.entity.DownloadJob
import com.luno.mobile.data.db.entity.DownloadState
import com.luno.mobile.data.db.entity.Playlist
import com.luno.mobile.data.db.entity.Track
import com.luno.mobile.playback.ExtractionResult
import com.luno.mobile.playback.WebSearchResult
import com.luno.mobile.playback.WebSearchService
import com.luno.mobile.ui.components.ArtworkImage
import com.luno.mobile.ui.components.BulkSelectionToolbar
import com.luno.mobile.ui.components.PlaylistPickerSheet
import com.luno.mobile.ui.components.TrackActionsSheet
import com.luno.mobile.ui.components.TrackRowCard
import com.luno.mobile.ui.create.CreatePlaylistSheet
import com.luno.mobile.ui.theme.AccentGreen
import com.luno.mobile.ui.theme.Dimens
import com.luno.mobile.ui.theme.PrimaryText
import com.luno.mobile.ui.theme.SecondaryText
import com.luno.mobile.ui.theme.SurfaceDark
import com.luno.mobile.ui.theme.SurfaceElevated
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File

private val DownloadFieldSpacing = 8.dp

private const val DownloadTabSearch = "search"
private const val DownloadTabPlaylist = "playlist"
private const val DownloadTabDirect = "direct"

private val DownloadTabs = listOf(
    DownloadTabSearch to "Search",
    DownloadTabPlaylist to "YT / CSV",
    DownloadTabDirect to "Direct URL"
)

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
    val app = context.applicationContext as LunoApp
    val scope = rememberCoroutineScope()
    val downloadRepository = app.downloadRepository
    val downloads by downloadRepository.getAllDownloads().collectAsState(initial = emptyList())
    val allTracks by app.libraryData.tracks.collectAsState()
    val downloadsRootUri = remember(context) {
        File(context.filesDir, "downloads").toURI().toString()
    }
    val completedDownloadUris = remember(downloads) {
        downloads
            .asSequence()
            .filter { it.state == DownloadState.COMPLETED && it.localUri.isNotBlank() }
            .map { it.localUri }
            .toSet()
    }
    val downloadedTracks = remember(allTracks, downloadsRootUri, completedDownloadUris) {
        allTracks
            .filter { it.uri.startsWith(downloadsRootUri) || it.uri in completedDownloadUris }
            .sortedByDescending { it.addedAt }
    }

    var searchQuery by rememberSaveable { mutableStateOf("") }
    var webResults by remember { mutableStateOf<List<WebSearchResult>>(emptyList()) }
    var isSearching by remember { mutableStateOf(false) }
    var errorMessage by remember { mutableStateOf("") }
    var urlInput by rememberSaveable { mutableStateOf("") }
    var urlTitle by rememberSaveable { mutableStateOf("") }
    var urlArtist by rememberSaveable { mutableStateOf("") }
    var isQueuingUrl by remember { mutableStateOf(false) }
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
    var youtubePlaylistUrl by rememberSaveable { mutableStateOf("") }
    var playlistImportStatus by rememberSaveable { mutableStateOf("") }
    var playlistImporting by remember { mutableStateOf(false) }
    var importDestination by remember { mutableStateOf<Playlist?>(null) }
    var showImportDestinationPicker by remember { mutableStateOf(false) }
    var showCreateImportPlaylist by remember { mutableStateOf(false) }
    var downloadTab by rememberSaveable { mutableStateOf(DownloadTabSearch) }

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

    fun queuePlaylistImport(destination: Playlist) {
        val sourceUrl = youtubePlaylistUrl.trim()
        if (sourceUrl.isBlank() || playlistImporting) return
        if (WebSearchService.extractPlaylistId(sourceUrl) == null) {
            playlistImportStatus = "Paste a YouTube playlist URL containing a list= parameter."
            return
        }

        playlistImporting = true
        playlistImportStatus = "Preparing ${destination.name}…"
        scope.launch {
            try {
                // Keep the source attached to the destination so it can be
                // synced again later from the Downloads manager.
                app.playlistRepository.updatePlaylistUrl(destination.id, sourceUrl)
                app.downloadRepository.syncPlaylist(
                    playlistId = destination.id,
                    playlistName = destination.name,
                    playlistUrl = sourceUrl
                )
                playlistImportStatus = "Playlist import queued for ${destination.name}."
                Toast.makeText(
                    context,
                    "Playlist import queued for ${destination.name}",
                    Toast.LENGTH_SHORT
                ).show()
            } catch (e: Exception) {
                playlistImportStatus = "Could not start playlist import: ${e.message ?: "Try again."}"
            } finally {
                playlistImporting = false
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
                activeDownloads = activeDownloads,
                onOpenDownloads = onOpenDownloads
            )
        }

        item(key = "tabs") {
            DownloadTabBar(
                selectedTab = downloadTab,
                onTabSelected = {
                    downloadTab = it
                    errorMessage = ""
                }
            )
        }

        if (activeDownloads > 0) {
            item(key = "download-progress") {
                LinearProgressIndicator(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(2.dp),
                    color = AccentGreen,
                    trackColor = SurfaceDark
                )
            }
        }

        if (errorMessage.isNotBlank()) {
            item(key = "error") {
                MessageBox(text = errorMessage, isError = true)
            }
        }

        when (downloadTab) {
            DownloadTabSearch -> {
                item(key = "search-source") {
                    SearchSourcePanel(
                        query = searchQuery,
                        onQueryChange = {
                            searchQuery = it
                            errorMessage = ""
                            if (it.isBlank()) webResults = emptyList()
                        },
                        onSearch = ::search,
                        isSearching = isSearching
                    )
                }
                if (isSearching) {
                    item(key = "searching") {
                        SearchLoadingState()
                    }
                } else if (webResults.isNotEmpty()) {
                    item(key = "results-heading") {
                        ResultsHeader(query = searchQuery, count = webResults.size)
                    }
                    items(
                        webResults,
                        key = { it.videoId },
                        contentType = { "web-result" }
                    ) { result ->
                        val job = jobForResult(result)
                        WebResultRow(
                            result = result,
                            job = job,
                            extractingAudio = result.videoId in downloadingVideoIds,
                            onDownload = { queueSearchResult(result) },
                            onSaveToPlaylist = {
                                val completedJob = jobForResult(result)
                                if (completedJob?.state == DownloadState.COMPLETED) {
                                    val track = allTracks.firstOrNull { it.uri == completedJob.localUri }
                                    if (track == null) {
                                        Toast.makeText(context, "Downloaded track is still loading", Toast.LENGTH_SHORT).show()
                                    } else {
                                        playlistTracksToAdd = listOf(track)
                                    }
                                } else if (completedJob == null) {
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

                if (!isSearching && searchQuery.isBlank() && webResults.isEmpty() && errorMessage.isBlank()) {
                    item(key = "start-here") {
                        DownloadStartHint()
                    }
                }
            }
            DownloadTabPlaylist -> {
                item(key = "youtube-playlist-import") {
                    PlaylistImportSection(
                        urlInput = youtubePlaylistUrl,
                        destinationName = importDestination?.name,
                        isImporting = playlistImporting,
                        status = playlistImportStatus,
                        onUrlChange = {
                            youtubePlaylistUrl = it
                            playlistImportStatus = ""
                        },
                        onChooseDestination = { showImportDestinationPicker = true },
                        onImport = { importDestination?.let(::queuePlaylistImport) }
                    )
                }
                item(key = "csv-form") {
                    CsvImportSection(
                        isImporting = csvImporting,
                        status = csvStatus,
                        onTracksLoaded = ::queueCsvRows
                    )
                }
            }
            DownloadTabDirect -> {
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
        }

        item(key = "downloaded-header") {
            DownloadOptionRow(
                icon = Icons.Filled.Download,
                title = "Downloaded songs",
                description = if (downloadedTracks.isEmpty()) {
                    "Completed downloads will appear here"
                } else {
                    "${downloadedTracks.size} songs saved on this device"
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
                    selectedTracks = downloadedTracks.filter { it.uri in selectedDownloadedUris },
                    selectionMode = downloadedSelectionMode,
                    onToggleSelectionMode = {
                        downloadedSelectionMode = !downloadedSelectionMode
                        selectedDownloadedUris = emptySet()
                    },
                    onSelectAll = { selectAll ->
                        selectedDownloadedUris = if (selectAll) {
                            downloadedTracks.map { it.uri }.toSet()
                        } else {
                            emptySet()
                        }
                    },
                    onDismissSelection = {
                        downloadedSelectionMode = false
                        selectedDownloadedUris = emptySet()
                    },
                    onAddToPlaylist = { playlist, trackUris ->
                        scope.launch {
                            app.playlistRepository.addTracksToPlaylist(playlist.id, trackUris)
                            Toast.makeText(context, "Added to ${playlist.name}", Toast.LENGTH_SHORT).show()
                        }
                        downloadedSelectionMode = false
                        selectedDownloadedUris = emptySet()
                    },
                    onCreatePlaylist = { name, description, trackUris ->
                        scope.launch {
                            app.playlistRepository.createPlaylist(name, description).onSuccess { playlist ->
                                app.playlistRepository.addTracksToPlaylist(playlist.id, trackUris)
                                Toast.makeText(context, "Created ${playlist.name}", Toast.LENGTH_SHORT).show()
                            }
                        }
                        downloadedSelectionMode = false
                        selectedDownloadedUris = emptySet()
                    },
                    onRemoveTracks = { uris ->
                        scope.launch { uris.forEach { app.libraryRepository.deleteTrack(it) } }
                        downloadedSelectionMode = false
                        selectedDownloadedUris = emptySet()
                    }
                )
            }
            if (downloadedTracks.isEmpty()) {
                item(key = "downloaded-empty") {
                    Text(
                        text = "Start a search above and your finished songs will collect here.",
                        style = MaterialTheme.typography.bodySmall,
                        color = SecondaryText,
                        modifier = Modifier.padding(vertical = 8.dp)
                    )
                }
            } else {
                items(
                    downloadedTracks,
                    key = { "downloaded-${it.uri}" },
                    contentType = { "downloaded-track" }
                ) { track ->
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
                        artworkUri = track.albumArtPath
                            ?.takeIf(ArtworkStorage::hasUsableArtwork)
                            ?.let { track.albumArtUri() }
                            ?: downloads
                                .firstOrNull { it.localUri == track.uri }
                                ?.thumbnailUrl
                                ?.takeIf { it.isNotBlank() },
                        onLongClick = {
                            downloadedSelectionMode = true
                            selectedDownloadedUris = selectedDownloadedUris + track.uri
                        },
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

    if (showImportDestinationPicker) {
        PlaylistPickerSheet(
            title = "Choose import destination",
            onPick = { playlist ->
                importDestination = playlist
                showImportDestinationPicker = false
                playlistImportStatus = "Ready to import into ${playlist.name}."
            },
            onCreateNew = {
                showImportDestinationPicker = false
                showCreateImportPlaylist = true
            },
            onDismiss = { showImportDestinationPicker = false }
        )
    }

    if (showCreateImportPlaylist) {
        CreatePlaylistSheet(
            onDismiss = { showCreateImportPlaylist = false },
            onCreated = { playlist ->
                importDestination = playlist
                playlistImportStatus = "Ready to import into ${playlist.name}."
            }
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
    activeDownloads: Int,
    onOpenDownloads: () -> Unit
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(bottom = 8.dp)
    ) {
        Text(
            text = "Downloader",
            style = MaterialTheme.typography.headlineLarge,
            fontWeight = FontWeight.Bold,
            color = PrimaryText
        )
        Text(
            text = "Search and queue music.",
            style = MaterialTheme.typography.bodySmall,
            color = SecondaryText,
            modifier = Modifier.padding(top = 2.dp)
        )

        if (activeDownloads > 0) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = 12.dp)
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
private fun DownloadTabBar(
    selectedTab: String,
    onTabSelected: (String) -> Unit
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(bottom = 10.dp)
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .horizontalScroll(rememberScrollState()),
            horizontalArrangement = Arrangement.spacedBy(4.dp)
        ) {
            DownloadTabs.forEach { (tabId, label) ->
                val selected = tabId == selectedTab
                Text(
                    text = label,
                    style = MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.Bold,
                    color = if (selected) PrimaryText else SecondaryText,
                    modifier = Modifier
                        .clip(RoundedCornerShape(Dimens.cornerSmall))
                        .background(if (selected) SurfaceDark else Color.Transparent)
                        .clickable(
                            interactionSource = remember { MutableInteractionSource() },
                            indication = null,
                            onClick = { onTabSelected(tabId) }
                        )
                        .padding(horizontal = 14.dp, vertical = 9.dp)
                )
            }
        }
        HorizontalDivider(color = Color.White.copy(alpha = 0.08f))
    }
}

@Composable
private fun SearchSourcePanel(
    query: String,
    onQueryChange: (String) -> Unit,
    onSearch: () -> Unit,
    isSearching: Boolean
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(bottom = 8.dp)
    ) {
        Text(
            text = "Search YouTube for individual songs.",
            style = MaterialTheme.typography.bodySmall,
            color = SecondaryText
        )
        Spacer(modifier = Modifier.height(8.dp))
        DownloaderInputRow(
            value = query,
            onValueChange = onQueryChange,
            placeholder = "Artist, song, or album",
            leadingIcon = Icons.Filled.Search,
            actionLabel = "SEARCH",
            actionEnabled = query.isNotBlank() && !isSearching,
            actionHighlighted = isSearching,
            onAction = onSearch,
            onClear = { onQueryChange("") },
            onEditorAction = onSearch
        )
    }
}

@Composable
private fun DownloaderInputRow(
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String,
    leadingIcon: ImageVector,
    actionLabel: String,
    actionEnabled: Boolean,
    actionProgress: Boolean = false,
    actionHighlighted: Boolean = actionProgress,
    onAction: () -> Unit,
    onClear: (() -> Unit)? = null,
    onEditorAction: (() -> Unit)? = null
) {
    val rowShape = RoundedCornerShape(Dimens.cornerSmall)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(48.dp)
            .clip(rowShape)
            .background(SurfaceDark)
            .border(1.dp, Color.White.copy(alpha = 0.12f), rowShape),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Icon(
            imageVector = leadingIcon,
            contentDescription = null,
            tint = SecondaryText,
            modifier = Modifier
                .padding(start = 12.dp)
                .size(19.dp)
        )
        BasicTextField(
            value = value,
            onValueChange = onValueChange,
            modifier = Modifier
                .weight(1f)
                .padding(horizontal = 10.dp),
            textStyle = MaterialTheme.typography.bodyMedium.copy(color = PrimaryText),
            singleLine = true,
            keyboardOptions = KeyboardOptions(
                imeAction = if (onEditorAction == null) ImeAction.Default else ImeAction.Search
            ),
            keyboardActions = KeyboardActions(onSearch = { onEditorAction?.invoke() }),
            decorationBox = { innerTextField ->
                Box(contentAlignment = Alignment.CenterStart) {
                    if (value.isBlank()) {
                        Text(placeholder, color = SecondaryText, maxLines = 1)
                    }
                    innerTextField()
                }
            }
        )
        if (onClear != null && value.isNotBlank()) {
            IconButton(onClick = onClear, modifier = Modifier.size(36.dp)) {
                Icon(
                    imageVector = Icons.Filled.Clear,
                    contentDescription = "Clear input",
                    tint = SecondaryText,
                    modifier = Modifier.size(18.dp)
                )
            }
        }
        Box(
            modifier = Modifier
                .height(48.dp)
                .width(82.dp)
                .background(
                    if (actionEnabled || actionHighlighted) {
                        AccentGreen
                    } else {
                        SurfaceElevated
                    }
                )
                .clickable(
                    enabled = actionEnabled,
                    onClick = onAction
                ),
            contentAlignment = Alignment.Center
        ) {
            if (actionProgress) {
                CircularProgressIndicator(
                    modifier = Modifier.size(16.dp),
                    color = Color.Black,
                    strokeWidth = 2.dp
                )
            } else {
                Text(
                    text = actionLabel,
                    style = MaterialTheme.typography.labelSmall,
                    fontWeight = FontWeight.Bold,
                    color = if (actionEnabled || actionHighlighted) Color.Black else SecondaryText
                )
            }
        }
    }
}

@Composable
private fun PlaylistImportSection(
    urlInput: String,
    destinationName: String?,
    isImporting: Boolean,
    status: String,
    onUrlChange: (String) -> Unit,
    onChooseDestination: () -> Unit,
    onImport: () -> Unit
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(bottom = 14.dp)
    ) {
        Text(
            text = "YouTube playlist",
            style = MaterialTheme.typography.titleMedium,
            color = PrimaryText,
            fontWeight = FontWeight.Bold
        )
        Text(
            text = "Queue a playlist and keep its source attached for later syncs.",
            style = MaterialTheme.typography.bodySmall,
            color = SecondaryText,
            modifier = Modifier.padding(top = 2.dp)
        )

        Spacer(modifier = Modifier.height(8.dp))
        DownloaderInputRow(
            value = urlInput,
            onValueChange = onUrlChange,
            placeholder = "https://youtube.com/playlist?list=…",
            leadingIcon = Icons.Filled.Link,
            actionLabel = "QUEUE",
            actionEnabled = urlInput.isNotBlank() && destinationName != null && !isImporting,
            actionProgress = isImporting,
            onAction = onImport
        )

        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(top = 8.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Text(
                text = "SAVE TO",
                style = MaterialTheme.typography.labelSmall,
                fontWeight = FontWeight.Bold,
                color = SecondaryText
            )
            TextButton(
                onClick = onChooseDestination,
                contentPadding = PaddingValues(horizontal = 8.dp, vertical = 2.dp)
            ) {
                Text(
                    text = destinationName ?: "Choose playlist",
                    color = if (destinationName == null) AccentGreen else PrimaryText,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis
                )
            }
        }
        Text(
            text = "The playlist URL is saved on the destination.",
            style = MaterialTheme.typography.labelSmall,
            color = SecondaryText
        )
        if (status.isNotBlank()) {
            Text(
                text = status,
                style = MaterialTheme.typography.bodySmall,
                color = if (status.startsWith("Could not") || status.startsWith("Paste")) {
                    MaterialTheme.colorScheme.error
                } else {
                    AccentGreen
                },
                modifier = Modifier.padding(top = DownloadFieldSpacing)
            )
        }
        HorizontalDivider(
            modifier = Modifier.padding(top = 14.dp),
            color = Color.White.copy(alpha = 0.08f)
        )
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
                text = "Search results",
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
private fun DownloadStartHint() {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(top = 12.dp, bottom = 4.dp)
    ) {
        Text(
            text = "Search results appear here.",
            style = MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.Bold,
            color = PrimaryText
        )
        Text(
            text = "Use the tabs above for playlists, CSV imports, or a direct audio URL.",
            style = MaterialTheme.typography.bodySmall,
            color = SecondaryText,
            modifier = Modifier.padding(top = 4.dp)
        )
    }
}

@Composable
private fun MessageBox(
    text: String,
    isError: Boolean = false
) {
    Text(
        text = text,
        style = MaterialTheme.typography.bodySmall,
        color = if (isError) MaterialTheme.colorScheme.error else SecondaryText,
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 8.dp)
    )
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
    selectedTracks: List<Track>,
    selectionMode: Boolean,
    onToggleSelectionMode: () -> Unit,
    onSelectAll: (Boolean) -> Unit,
    onDismissSelection: () -> Unit,
    onAddToPlaylist: (com.luno.mobile.data.db.entity.Playlist, List<String>) -> Unit,
    onCreatePlaylist: (String, String, List<String>) -> Unit,
    onRemoveTracks: (List<String>) -> Unit
) {
    if (selectionMode) {
        BulkSelectionToolbar(
            selectedTracks = selectedTracks,
            selectedPlaylists = emptyList(),
            allSelected = trackCount > 0 && selectedCount == trackCount,
            onSelectAll = onSelectAll,
            onDismiss = onDismissSelection,
            onAddToPlaylist = onAddToPlaylist,
            onCreatePlaylist = onCreatePlaylist,
            onRemoveTracks = onRemoveTracks
        )
        return
    }
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
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(bottom = 14.dp)
    ) {
        Text(
            text = "Direct audio URL",
            style = MaterialTheme.typography.titleMedium,
            color = PrimaryText,
            fontWeight = FontWeight.Bold
        )
        Text(
            text = "Paste a link to an audio file and queue it directly.",
            style = MaterialTheme.typography.bodySmall,
            color = SecondaryText,
            modifier = Modifier.padding(top = 2.dp)
        )
        Spacer(modifier = Modifier.height(8.dp))
        DownloaderInputRow(
            value = urlInput,
            onValueChange = onUrlChange,
            placeholder = "Paste an audio link",
            leadingIcon = Icons.Filled.Link,
            actionLabel = "QUEUE",
            actionEnabled = urlInput.isNotBlank() && !isQueuing,
            actionProgress = isQueuing,
            onAction = onDownload
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
                shape = RoundedCornerShape(Dimens.cornerSmall),
                colors = downloadFieldColors()
            )
            Spacer(modifier = Modifier.height(DownloadFieldSpacing))
            OutlinedTextField(
                value = trackArtist,
                onValueChange = onArtistChange,
                modifier = Modifier.fillMaxWidth(),
                label = { Text("Artist") },
                singleLine = true,
                shape = RoundedCornerShape(Dimens.cornerSmall),
                colors = downloadFieldColors()
            )
        }
        HorizontalDivider(
            modifier = Modifier.padding(top = 14.dp),
            color = Color.White.copy(alpha = 0.08f)
        )
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
            .padding(bottom = 14.dp)
    ) {
        Text(
            text = "Exportify CSV",
            style = MaterialTheme.typography.titleMedium,
            color = PrimaryText,
            fontWeight = FontWeight.Bold
        )
        Text(
            text = "Load a Spotify export and queue its tracks one by one.",
            style = MaterialTheme.typography.bodySmall,
            color = SecondaryText,
            modifier = Modifier.padding(top = 2.dp)
        )
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(top = 4.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Text(
                text = "SOURCE",
                style = MaterialTheme.typography.labelSmall,
                fontWeight = FontWeight.Bold,
                color = SecondaryText,
                modifier = Modifier.weight(1f)
            )
            TextButton(
                onClick = { launcher.launch(arrayOf("text/*", "*/*")) },
                enabled = !isImporting,
                contentPadding = PaddingValues(horizontal = 8.dp, vertical = 2.dp)
            ) {
                if (isImporting) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(Dimens.iconSizeSmall),
                        color = AccentGreen,
                        strokeWidth = 2.dp
                    )
                    Spacer(modifier = Modifier.width(6.dp))
                } else {
                    Icon(
                        imageVector = Icons.Filled.UploadFile,
                        contentDescription = null,
                        modifier = Modifier.size(Dimens.iconSizeSmall)
                    )
                    Spacer(modifier = Modifier.width(6.dp))
                }
                Text(if (isImporting) "FINDING…" else "LOAD CSV")
            }
        }
        if (isImporting || status.isNotBlank()) {
            Text(
                text = status,
                style = MaterialTheme.typography.bodySmall,
                color = if (isImporting) AccentGreen else SecondaryText,
                modifier = Modifier.padding(top = DownloadFieldSpacing)
            )
        }
        HorizontalDivider(
            modifier = Modifier.padding(top = 14.dp),
            color = Color.White.copy(alpha = 0.08f)
        )
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
                Button(
                    onClick = onDownload,
                    contentPadding = PaddingValues(horizontal = 10.dp, vertical = 0.dp),
                    colors = ButtonDefaults.buttonColors(
                        containerColor = AccentGreen,
                        contentColor = Color.Black
                    ),
                    shape = actionShape,
                    modifier = Modifier.height(40.dp)
                ) {
                    Icon(
                        imageVector = Icons.Filled.Download,
                        contentDescription = null,
                        modifier = Modifier.size(20.dp)
                    )
                    Spacer(modifier = Modifier.width(4.dp))
                    Text("Download", fontWeight = FontWeight.SemiBold)
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
