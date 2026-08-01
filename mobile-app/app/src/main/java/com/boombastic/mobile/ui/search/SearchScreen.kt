package com.boombastic.mobile.ui.search

import android.net.Uri
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Clear
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.ExpandLess
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material.icons.filled.Info
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
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.boombastic.mobile.BoomBasticApp
import com.boombastic.mobile.data.db.entity.DownloadJob
import com.boombastic.mobile.data.db.entity.DownloadState
import com.boombastic.mobile.playback.ExtractionResult
import com.boombastic.mobile.playback.WebSearchResult
import com.boombastic.mobile.playback.WebSearchService
import com.boombastic.mobile.ui.components.ArtworkImage
import com.boombastic.mobile.ui.theme.AccentGreen
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText
import com.boombastic.mobile.ui.theme.SurfaceDark
import com.boombastic.mobile.ui.theme.SurfaceElevated
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

private val DownloadSectionSpacing = 16.dp
private val DownloadCardPadding = 20.dp
private val DownloadControlSpacing = 16.dp
private val DownloadFieldSpacing = 12.dp

/**
 * The download hub. This screen deliberately does not render local tracks,
 * playlists, playback controls, or library import actions. Its only job is
 * helping the user find audio and put it in the background download queue.
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

    LazyColumn(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = Dimens.paddingLarge),
        verticalArrangement = Arrangement.spacedBy(DownloadSectionSpacing),
        contentPadding = PaddingValues(
            top = DownloadCardPadding,
            bottom = 32.dp
        )
    ) {
        item(key = "header") {
            DownloadHeader()
        }

        item(key = "intro") {
            DownloadIntroCard()
        }

        item(key = "search-heading") {
            SectionHeading(
                title = "Find music to download",
                subtitle = "Search YouTube for a song, artist, or album.",
                modifier = Modifier.padding(top = Dimens.paddingSmall)
            )
        }

        item(key = "youtube-search") {
            YouTubeSearchPanel(
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

        if (errorMessage.isNotBlank()) {
            item(key = "error") {
                MessageBox(text = errorMessage, isError = true)
            }
        }

        if (isSearching) {
            item(key = "searching") {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(vertical = Dimens.paddingMedium),
                    horizontalArrangement = Arrangement.Center,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(Dimens.iconSizeSmall),
                        color = AccentGreen,
                        strokeWidth = 2.dp
                    )
                    Spacer(modifier = Modifier.width(Dimens.paddingSmall))
                    Text("Searching for downloadable audio…", color = SecondaryText)
                }
            }
        } else if (webResults.isNotEmpty()) {
            item(key = "results-heading") {
                SectionHeading(
                    title = "Choose a download",
                    subtitle = "${webResults.size} YouTube results. Nothing is saved until you tap Download.",
                    modifier = Modifier.padding(top = Dimens.paddingSmall)
                )
            }
            items(webResults, key = { it.videoId }) { result ->
                val job = jobForResult(result)
                WebResultRow(
                    result = result,
                    job = job,
                    extractingAudio = result.videoId in downloadingVideoIds,
                    onDownload = {
                        val videoId = result.videoId
                        if (videoId in downloadingVideoIds) return@WebResultRow
                        downloadingVideoIds = downloadingVideoIds + videoId
                        scope.launch {
                            try {
                                val audioResult = withContext(Dispatchers.IO) {
                                    WebSearchService.getAudioStreamUrl(videoId)
                                }
                                when (audioResult) {
                                    is ExtractionResult.Success -> {
                                        val titleParts = result.title.split(" - ", limit = 2)
                                        val artist = if (titleParts.size > 1) {
                                            titleParts[0].trim()
                                        } else {
                                            result.artist
                                        }
                                        val title = if (titleParts.size > 1) {
                                            titleParts[1].trim()
                                        } else {
                                            result.title
                                        }
                                        val jobId = downloadRepository.enqueueDownload(
                                            sourceUrl = audioResult.data.url,
                                            title = title,
                                            artist = artist,
                                            thumbnailUrl = result.thumbnailUrl
                                        )
                                        jobIdByVideoId = jobIdByVideoId + (videoId to jobId)
                                        Toast.makeText(
                                            context,
                                            "Download queued: $title",
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
                    text = "No downloadable results found for \"${searchQuery.trim()}\". Try an artist and title, or use a direct audio link below."
                )
            }
        } else if (searchQuery.isBlank()) {
            item(key = "empty-search") {
                EmptyDownloadState()
            }
        }

        item(key = "other-options-heading") {
            SectionHeading(
                title = "Other ways to download",
                subtitle = "Use one of these when you already have a source or a list of tracks.",
                modifier = Modifier.padding(top = Dimens.paddingSmall)
            )
        }

        item(key = "direct-url-option") {
            DownloadOptionCard(
                icon = Icons.Filled.Link,
                title = "Paste a direct audio link",
                description = "Download an MP3, M4A, OGG, or other audio file from a URL.",
                expanded = showUrlInput,
                onClick = { showUrlInput = !showUrlInput }
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

        item(key = "csv-option") {
            DownloadOptionCard(
                icon = Icons.Filled.UploadFile,
                title = "Batch download from CSV",
                description = "Choose an Exportify CSV; each artist and title is searched and queued.",
                expanded = showCsvImport,
                onClick = { showCsvImport = !showCsvImport }
            )
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

        item(key = "queue") {
            DownloadQueueCard(
                downloads = downloads,
                onOpenDownloads = onOpenDownloads
            )
        }

        item(key = "how-it-works") {
            HowItWorksCard()
        }

        item(key = "note") {
            Text(
                text = "Only download audio you have permission to save. Downloads run in the background when a network connection is available and appear in Your Library when complete.",
                style = MaterialTheme.typography.bodySmall,
                color = SecondaryText,
                modifier = Modifier.padding(top = Dimens.paddingSmall)
            )
        }
    }
}

@Composable
private fun DownloadHeader() {
    Column {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = "Download music",
                    style = MaterialTheme.typography.headlineLarge,
                    fontWeight = FontWeight.Bold,
                    color = PrimaryText
                )
                Spacer(modifier = Modifier.height(Dimens.paddingMedium))
                Text(
                    text = "Find audio online and save it for offline listening.",
                    style = MaterialTheme.typography.bodyLarge,
                    color = SecondaryText
                )
            }
            Box(
                modifier = Modifier
                    .size(56.dp)
                    .clip(RoundedCornerShape(Dimens.cornerLarge))
                    .background(AccentGreen),
                contentAlignment = Alignment.Center
            ) {
                Icon(
                    imageVector = Icons.Filled.Download,
                    contentDescription = null,
                    tint = Color.Black,
                    modifier = Modifier.size(Dimens.iconSizeLarge)
                )
            }
        }
        Text(
            text = "This screen is for finding and downloading music, not browsing the songs already on your device.",
            style = MaterialTheme.typography.bodySmall,
            color = SecondaryText,
            modifier = Modifier.padding(top = DownloadControlSpacing)
        )
    }
}

@Composable
private fun DownloadIntroCard() {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(Dimens.cornerLarge))
            .background(SurfaceDark)
            .padding(DownloadCardPadding)
    ) {
        Row(verticalAlignment = Alignment.Top) {
            Icon(
                imageVector = Icons.Filled.Info,
                contentDescription = null,
                tint = AccentGreen,
                modifier = Modifier.size(Dimens.iconSize)
            )
            Spacer(modifier = Modifier.width(DownloadControlSpacing))
            Column {
                Text(
                    text = "How to use this page",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold,
                    color = PrimaryText
                )
                Spacer(modifier = Modifier.height(Dimens.paddingMedium))
                Text(
                    text = "Search for music, review the results, and tap Download on the exact recording you want. Every download is added to the queue before it is saved.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = SecondaryText
                )
            }
        }
    }
}

@Composable
private fun SectionHeading(
    title: String,
    subtitle: String,
    modifier: Modifier = Modifier
) {
    Column(modifier = modifier) {
        Text(
            text = title,
            style = MaterialTheme.typography.titleLarge,
            fontWeight = FontWeight.SemiBold,
            color = PrimaryText
        )
        Text(
            text = subtitle,
            style = MaterialTheme.typography.bodySmall,
            color = SecondaryText,
            modifier = Modifier.padding(top = Dimens.paddingSmall)
        )
    }
}

@Composable
private fun YouTubeSearchPanel(
    query: String,
    onQueryChange: (String) -> Unit,
    onSearch: () -> Unit,
    isSearching: Boolean
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(Dimens.cornerLarge))
            .background(SurfaceDark)
            .padding(DownloadCardPadding)
    ) {
        OutlinedTextField(
            value = query,
            onValueChange = onQueryChange,
            modifier = Modifier.fillMaxWidth(),
            label = { Text("Song, artist, or album") },
            placeholder = { Text("e.g. Daft Punk One More Time", color = SecondaryText) },
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
            shape = RoundedCornerShape(16.dp),
            colors = OutlinedTextFieldDefaults.colors(
                focusedTextColor = PrimaryText,
                unfocusedTextColor = PrimaryText,
                focusedLabelColor = AccentGreen,
                unfocusedLabelColor = SecondaryText,
                cursorColor = AccentGreen,
                focusedBorderColor = AccentGreen,
                unfocusedBorderColor = SurfaceElevated,
                focusedContainerColor = SurfaceElevated,
                unfocusedContainerColor = SurfaceElevated
            )
        )
        Spacer(modifier = Modifier.height(DownloadControlSpacing))
        Button(
            onClick = onSearch,
            enabled = query.isNotBlank() && !isSearching,
            colors = ButtonDefaults.buttonColors(
                containerColor = AccentGreen,
                contentColor = Color.Black
            ),
            modifier = Modifier
                .fillMaxWidth()
                .height(48.dp),
            shape = RoundedCornerShape(Dimens.cornerMedium)
        ) {
            Icon(
                imageVector = Icons.Filled.Search,
                contentDescription = null,
                modifier = Modifier.size(Dimens.iconSizeSmall)
            )
            Spacer(modifier = Modifier.width(Dimens.paddingSmall))
            Text("Search YouTube", fontWeight = FontWeight.SemiBold)
        }
        Text(
            text = "YouTube is used to find a downloadable source. Searching does not start a download.",
            style = MaterialTheme.typography.bodySmall,
            color = SecondaryText,
            modifier = Modifier.padding(top = DownloadFieldSpacing)
        )
    }
}

@Composable
private fun EmptyDownloadState() {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(Dimens.cornerMedium))
            .background(SurfaceDark)
            .padding(DownloadCardPadding)
    ) {
        Column {
            Text(
                text = "Ready when you are",
                style = MaterialTheme.typography.titleMedium,
                color = PrimaryText
            )
            Text(
                text = "Start with a search above, or open one of the other download options below.",
                style = MaterialTheme.typography.bodyMedium,
                color = SecondaryText,
                modifier = Modifier.padding(top = Dimens.paddingMedium)
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
private fun DownloadOptionCard(
    icon: ImageVector,
    title: String,
    description: String,
    expanded: Boolean,
    onClick: () -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(Dimens.cornerLarge))
            .background(SurfaceDark)
            .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = onClick
            )
            .padding(Dimens.paddingLarge),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Box(
            modifier = Modifier
                .size(40.dp)
                .clip(RoundedCornerShape(Dimens.cornerMedium))
                .background(AccentGreen.copy(alpha = 0.16f)),
            contentAlignment = Alignment.Center
        ) {
            Icon(
                imageVector = icon,
                contentDescription = null,
                tint = AccentGreen,
                modifier = Modifier.size(Dimens.iconSize)
            )
        }
        Spacer(modifier = Modifier.width(Dimens.paddingMedium))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = title,
                style = MaterialTheme.typography.titleMedium,
                color = PrimaryText
            )
            Text(
                text = description,
                style = MaterialTheme.typography.bodySmall,
                color = SecondaryText,
                modifier = Modifier.padding(top = 2.dp)
            )
        }
        Spacer(modifier = Modifier.width(Dimens.paddingSmall))
        Icon(
            imageVector = if (expanded) Icons.Filled.ExpandLess else Icons.Filled.ExpandMore,
            contentDescription = if (expanded) "Hide $title" else "Show $title",
            tint = SecondaryText,
            modifier = Modifier.size(Dimens.iconSize)
        )
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
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(Dimens.cornerMedium))
            .background(SurfaceElevated)
            .padding(DownloadCardPadding)
    ) {
        Text(
            text = "Use a direct audio-file URL. For YouTube pages, use the search above instead.",
            style = MaterialTheme.typography.bodySmall,
            color = SecondaryText
        )
        Spacer(modifier = Modifier.height(DownloadFieldSpacing))
        OutlinedTextField(
            value = urlInput,
            onValueChange = onUrlChange,
            modifier = Modifier.fillMaxWidth(),
            label = { Text("Audio URL") },
            placeholder = { Text("https://example.com/track.mp3", color = SecondaryText) },
            singleLine = true,
            shape = RoundedCornerShape(12.dp),
            colors = downloadFieldColors()
        )
        Spacer(modifier = Modifier.height(DownloadFieldSpacing))
        OutlinedTextField(
            value = trackTitle,
            onValueChange = onTitleChange,
            modifier = Modifier.fillMaxWidth(),
            label = { Text("Title (optional)") },
            singleLine = true,
            shape = RoundedCornerShape(12.dp),
            colors = downloadFieldColors()
        )
        Spacer(modifier = Modifier.height(DownloadFieldSpacing))
        OutlinedTextField(
            value = trackArtist,
            onValueChange = onArtistChange,
            modifier = Modifier.fillMaxWidth(),
            label = { Text("Artist (optional)") },
            singleLine = true,
            shape = RoundedCornerShape(12.dp),
            colors = downloadFieldColors()
        )
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
                text = if (isQueuing) "Adding to queue…" else "Add to download queue",
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
            .clip(RoundedCornerShape(Dimens.cornerMedium))
            .background(SurfaceElevated)
            .padding(DownloadCardPadding)
    ) {
        Text(
            text = "Select an Exportify CSV. Each row is searched on YouTube and added to the download queue; large files may take a while.",
            style = MaterialTheme.typography.bodySmall,
            color = SecondaryText
        )
        Spacer(modifier = Modifier.height(DownloadControlSpacing))
        Button(
            onClick = { launcher.launch(arrayOf("text/*", "*/*")) },
            enabled = !isImporting,
            colors = ButtonDefaults.buttonColors(
                containerColor = AccentGreen,
                contentColor = Color.Black
            ),
            modifier = Modifier.fillMaxWidth(),
            shape = RoundedCornerShape(Dimens.cornerMedium)
        ) {
            Icon(
                imageVector = Icons.Filled.UploadFile,
                contentDescription = null,
                modifier = Modifier.size(Dimens.iconSizeSmall)
            )
            Spacer(modifier = Modifier.width(Dimens.paddingSmall))
            Text(if (isImporting) "Finding tracks…" else "Select CSV file")
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
private fun DownloadQueueCard(
    downloads: List<DownloadJob>,
    onOpenDownloads: () -> Unit
) {
    val active = downloads.count {
        it.state == DownloadState.QUEUED || it.state == DownloadState.DOWNLOADING
    }
    val completed = downloads.count { it.state == DownloadState.COMPLETED }
    val failed = downloads.count { it.state == DownloadState.FAILED }

    Box(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(Dimens.cornerLarge))
            .background(SurfaceDark)
            .padding(DownloadCardPadding)
    ) {
        Column {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    imageVector = Icons.Filled.Download,
                    contentDescription = null,
                    tint = AccentGreen,
                    modifier = Modifier.size(Dimens.iconSize)
                )
                Spacer(modifier = Modifier.width(Dimens.paddingSmall))
                Text(
                    text = "Download queue",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold,
                    color = PrimaryText,
                    modifier = Modifier.weight(1f)
                )
                if (active > 0) {
                    Text(
                        text = "$active active",
                        style = MaterialTheme.typography.labelMedium,
                        color = AccentGreen
                    )
                }
            }
            Text(
                text = if (downloads.isEmpty()) {
                    "Nothing is queued yet. Downloads you start above will appear here."
                } else {
                    "$completed saved  •  $active active  •  $failed need attention"
                },
                style = MaterialTheme.typography.bodySmall,
                color = SecondaryText,
                modifier = Modifier.padding(top = DownloadFieldSpacing)
            )
            Spacer(modifier = Modifier.height(DownloadControlSpacing))
            OutlinedButton(
                onClick = onOpenDownloads,
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(Dimens.cornerMedium),
                colors = androidx.compose.material3.ButtonDefaults.outlinedButtonColors(
                    contentColor = AccentGreen
                )
            ) {
                Text("Manage download queue")
            }
        }
    }
}

@Composable
private fun HowItWorksCard() {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(Dimens.cornerLarge))
            .background(SurfaceDark)
            .padding(DownloadCardPadding)
    ) {
        Column {
            Text(
                text = "How downloading works",
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
                color = PrimaryText
            )
            Spacer(modifier = Modifier.height(DownloadControlSpacing))
            Column(verticalArrangement = Arrangement.spacedBy(DownloadControlSpacing)) {
                DownloadStep(
                    number = "1",
                    title = "Find a source",
                    description = "Search YouTube or provide a direct audio link."
                )
                DownloadStep(
                    number = "2",
                    title = "Choose Download",
                    description = "The audio is prepared and placed in the background queue."
                )
                DownloadStep(
                    number = "3",
                    title = "Listen offline",
                    description = "When complete, the track is added to Your Library."
                )
            }
        }
    }
}

@Composable
private fun DownloadStep(
    number: String,
    title: String,
    description: String
) {
    Row(
        verticalAlignment = Alignment.Top
    ) {
        Box(
            modifier = Modifier
                .size(28.dp)
                .clip(RoundedCornerShape(14.dp))
                .background(AccentGreen),
            contentAlignment = Alignment.Center
        ) {
            Text(
                text = number,
                style = MaterialTheme.typography.labelMedium,
                fontWeight = FontWeight.Bold,
                color = Color.Black
            )
        }
        Spacer(modifier = Modifier.width(Dimens.paddingMedium))
        Column {
            Text(
                text = title,
                style = MaterialTheme.typography.titleSmall,
                color = PrimaryText
            )
            Text(
                text = description,
                style = MaterialTheme.typography.bodySmall,
                color = SecondaryText
            )
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
    extractingAudio: Boolean
) {
    val duration = formatDuration(result.duration)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(Dimens.cornerMedium))
            .background(SurfaceDark)
            .padding(Dimens.paddingSmall),
        verticalAlignment = Alignment.CenterVertically
    ) {
        ArtworkImage(
            artworkUri = result.thumbnailUrl,
            modifier = Modifier
                .size(56.dp)
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
                maxLines = 2,
                overflow = TextOverflow.Ellipsis
            )
            Text(
                text = result.artist,
                style = MaterialTheme.typography.bodySmall,
                color = SecondaryText,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.padding(top = 2.dp)
            )
            Text(
                text = listOfNotNull(result.source, duration.takeIf { it.isNotBlank() })
                    .joinToString("  •  "),
                style = MaterialTheme.typography.labelSmall,
                color = SecondaryText,
                modifier = Modifier.padding(top = 2.dp)
            )
        }
        Spacer(modifier = Modifier.width(Dimens.paddingSmall))
        ResultAction(
            job = job,
            extractingAudio = extractingAudio,
            onDownload = onDownload,
            onCancel = onCancel,
            onRetry = onRetry
        )
    }
}

@Composable
private fun ResultAction(
    job: DownloadJob?,
    extractingAudio: Boolean,
    onDownload: () -> Unit,
    onCancel: (Long) -> Unit,
    onRetry: (Long) -> Unit
) {
    when {
        job?.state == DownloadState.COMPLETED -> {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                Icon(
                    imageVector = Icons.Filled.CheckCircle,
                    contentDescription = "Downloaded",
                    tint = AccentGreen,
                    modifier = Modifier.size(Dimens.iconSize)
                )
                Text(
                    text = "Downloaded",
                    style = MaterialTheme.typography.labelSmall,
                    color = AccentGreen
                )
            }
        }
        job?.state == DownloadState.QUEUED || job?.state == DownloadState.DOWNLOADING -> {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                Text(
                    text = if (job.state == DownloadState.DOWNLOADING) {
                        "${job.progress}%"
                    } else {
                        "Queued"
                    },
                    style = MaterialTheme.typography.labelSmall,
                    color = AccentGreen
                )
                TextButton(
                    onClick = { onCancel(job.id) },
                    contentPadding = PaddingValues(horizontal = 4.dp, vertical = 0.dp)
                ) {
                    Text("Cancel", color = SecondaryText)
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
                    text = "Preparing",
                    style = MaterialTheme.typography.labelSmall,
                    color = SecondaryText
                )
            }
        }
        else -> {
            Button(
                onClick = onDownload,
                colors = ButtonDefaults.buttonColors(
                    containerColor = AccentGreen,
                    contentColor = Color.Black
                ),
                contentPadding = PaddingValues(horizontal = 10.dp, vertical = 0.dp),
                shape = RoundedCornerShape(Dimens.cornerMedium)
            ) {
                Icon(
                    imageVector = Icons.Filled.Download,
                    contentDescription = null,
                    modifier = Modifier.size(Dimens.iconSizeSmall)
                )
                Spacer(modifier = Modifier.width(4.dp))
                Text("Download", fontWeight = FontWeight.SemiBold)
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
    unfocusedBorderColor = SurfaceDark,
    focusedContainerColor = SurfaceDark,
    unfocusedContainerColor = SurfaceDark
)

private fun formatDuration(seconds: Long): String {
    if (seconds <= 0) return ""
    val mins = seconds / 60
    val secs = seconds % 60
    return "%d:%02d".format(mins, secs)
}
