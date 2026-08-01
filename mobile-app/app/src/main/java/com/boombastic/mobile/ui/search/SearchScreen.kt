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
import androidx.compose.ui.graphics.Brush
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

private val DownloadSectionSpacing = 14.dp
private val DownloadCardPadding = 18.dp
private val DownloadControlSpacing = 14.dp
private val DownloadFieldSpacing = 10.dp

/**
 * The download hub. This screen deliberately does not render local tracks,
 * playlists, playback controls, or library import actions. Its only job is
 * helping the user find audio, add it to the background queue, and keep the
 * first interaction focused on the music rather than the implementation.
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

    val activeDownloads = downloads.count {
        it.state == DownloadState.QUEUED || it.state == DownloadState.DOWNLOADING
    }

    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        verticalArrangement = Arrangement.spacedBy(DownloadSectionSpacing),
        contentPadding = PaddingValues(
            start = Dimens.paddingLarge,
            end = Dimens.paddingLarge,
            top = DownloadCardPadding,
            bottom = 32.dp
        )
    ) {
        item(key = "hero") {
            DownloadHero(
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

        item(key = "download-flow") {
            DownloadFlow()
        }
    }
}

@Composable
private fun DownloadHero(
    query: String,
    onQueryChange: (String) -> Unit,
    onSearch: () -> Unit,
    isSearching: Boolean,
    activeDownloads: Int,
    onOpenDownloads: () -> Unit
) {
    val heroShape = RoundedCornerShape(28.dp)
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(heroShape)
            .background(
                Brush.linearGradient(
                    colors = listOf(
                        Color(0xFF284A34),
                        Color(0xFF1E2C24),
                        SurfaceDark
                    )
                )
            )
            .border(1.dp, AccentGreen.copy(alpha = 0.22f), heroShape)
            .padding(DownloadCardPadding)
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = "Add music",
                    style = MaterialTheme.typography.headlineLarge,
                    fontWeight = FontWeight.Bold,
                    color = PrimaryText
                )
                Text(
                    text = "Find it. Keep it.",
                    style = MaterialTheme.typography.bodyLarge,
                    color = Color(0xFFD2E6D7),
                    modifier = Modifier.padding(top = 4.dp)
                )
            }
            DownloadHeroArt()
        }

        Spacer(modifier = Modifier.height(DownloadCardPadding))
        Text(
            text = "Find a song",
            style = MaterialTheme.typography.labelLarge,
            color = Color(0xFFD2E6D7)
        )
        Spacer(modifier = Modifier.height(DownloadFieldSpacing))
        OutlinedTextField(
            value = query,
            onValueChange = onQueryChange,
            modifier = Modifier.fillMaxWidth(),
            placeholder = { Text("Song, artist, or album", color = SecondaryText) },
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
                cursorColor = AccentGreen,
                focusedBorderColor = AccentGreen,
                unfocusedBorderColor = Color.White.copy(alpha = 0.12f),
                focusedContainerColor = Color.Black.copy(alpha = 0.22f),
                unfocusedContainerColor = Color.Black.copy(alpha = 0.22f)
            )
        )
        Spacer(modifier = Modifier.height(DownloadFieldSpacing))
        Button(
            onClick = onSearch,
            enabled = query.isNotBlank() && !isSearching,
            colors = ButtonDefaults.buttonColors(
                containerColor = AccentGreen,
                contentColor = Color.Black
            ),
            modifier = Modifier
                .fillMaxWidth()
                .height(52.dp),
            shape = RoundedCornerShape(16.dp)
        ) {
            Icon(
                imageVector = Icons.Filled.Search,
                contentDescription = null,
                modifier = Modifier.size(Dimens.iconSizeSmall)
            )
            Spacer(modifier = Modifier.width(Dimens.paddingSmall))
            Text("Find music", fontWeight = FontWeight.SemiBold)
        }

        if (activeDownloads > 0) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = DownloadControlSpacing)
                    .clip(RoundedCornerShape(12.dp))
                    .clickable(
                        interactionSource = remember { MutableInteractionSource() },
                        indication = null,
                        onClick = onOpenDownloads
                    )
                    .background(Color.Black.copy(alpha = 0.18f))
                    .padding(horizontal = 12.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                CircularProgressIndicator(
                    modifier = Modifier.size(16.dp),
                    color = AccentGreen,
                    strokeWidth = 2.dp
                )
                Spacer(modifier = Modifier.width(10.dp))
                Text(
                    text = if (activeDownloads == 1) "1 download on the way" else "$activeDownloads downloads on the way",
                    style = MaterialTheme.typography.labelMedium,
                    color = PrimaryText,
                    modifier = Modifier.weight(1f)
                )
                Icon(
                    imageVector = Icons.AutoMirrored.Filled.ArrowForward,
                    contentDescription = "Open downloads",
                    tint = AccentGreen,
                    modifier = Modifier.size(18.dp)
                )
            }
        }
    }
}

@Composable
private fun DownloadHeroArt() {
    Box(
        modifier = Modifier
            .size(72.dp)
            .clip(RoundedCornerShape(24.dp))
            .background(AccentGreen),
        contentAlignment = Alignment.Center
    ) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(4.dp)
        ) {
            Icon(
                imageVector = Icons.Filled.Download,
                contentDescription = null,
                tint = Color.Black,
                modifier = Modifier.size(28.dp)
            )
            Row(
                horizontalArrangement = Arrangement.spacedBy(3.dp),
                verticalAlignment = Alignment.Bottom
            ) {
                listOf(7.dp, 12.dp, 17.dp, 10.dp).forEach { barHeight ->
                    Box(
                        modifier = Modifier
                            .width(3.dp)
                            .height(barHeight)
                            .clip(RoundedCornerShape(2.dp))
                            .background(Color.Black.copy(alpha = 0.72f))
                    )
                }
            }
        }
    }
}

@Composable
private fun SearchLoadingState() {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(Dimens.cornerMedium))
            .background(SurfaceDark)
            .padding(horizontal = 14.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        CircularProgressIndicator(
            modifier = Modifier.size(Dimens.iconSizeSmall),
            color = AccentGreen,
            strokeWidth = 2.dp
        )
        Spacer(modifier = Modifier.width(Dimens.paddingSmall))
        Text("Looking for matches…", color = SecondaryText, style = MaterialTheme.typography.bodyMedium)
    }
}

@Composable
private fun ResultsHeader(query: String, count: Int) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(top = Dimens.paddingSmall),
        verticalAlignment = Alignment.Bottom
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = "Choose a recording",
                style = MaterialTheme.typography.titleLarge,
                fontWeight = FontWeight.SemiBold,
                color = PrimaryText
            )
            Text(
                text = "For \"${query.trim()}\"",
                style = MaterialTheme.typography.bodySmall,
                color = SecondaryText,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.padding(top = 3.dp)
            )
        }
        Box(
            modifier = Modifier
                .clip(RoundedCornerShape(10.dp))
                .background(AccentGreen.copy(alpha = 0.14f))
                .padding(horizontal = 9.dp, vertical = 6.dp)
        ) {
            Text(
                text = "$count found",
                style = MaterialTheme.typography.labelMedium,
                color = AccentGreen
            )
        }
    }
}

@Composable
private fun SourceOptions(
    showUrlInput: Boolean,
    showCsvImport: Boolean,
    onUrlClick: () -> Unit,
    onCsvClick: () -> Unit
) {
    Column(modifier = Modifier.padding(top = Dimens.paddingSmall)) {
        Text(
            text = "Have the music already?",
            style = MaterialTheme.typography.titleMedium,
            color = PrimaryText
        )
        Text(
            text = "Two quick ways to bring it in",
            style = MaterialTheme.typography.bodySmall,
            color = SecondaryText,
            modifier = Modifier.padding(top = 3.dp)
        )
        Spacer(modifier = Modifier.height(Dimens.paddingMedium))
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(10.dp)
        ) {
            DownloadOptionCard(
                modifier = Modifier.weight(1f),
                icon = Icons.Filled.Link,
                title = "Paste a link",
                description = "Direct audio",
                expanded = showUrlInput,
                onClick = onUrlClick
            )
            DownloadOptionCard(
                modifier = Modifier.weight(1f),
                icon = Icons.Filled.UploadFile,
                title = "Import a list",
                description = "Playlist file",
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
private fun DownloadOptionCard(
    modifier: Modifier,
    icon: ImageVector,
    title: String,
    description: String,
    expanded: Boolean,
    onClick: () -> Unit
) {
    Column(
        modifier = Modifier
            .then(modifier)
            .height(112.dp)
            .clip(RoundedCornerShape(18.dp))
            .background(if (expanded) SurfaceElevated else SurfaceDark)
            .border(
                width = 1.dp,
                color = if (expanded) AccentGreen.copy(alpha = 0.62f) else Color.White.copy(alpha = 0.06f),
                shape = RoundedCornerShape(18.dp)
            )
            .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = onClick
            )
            .padding(14.dp)
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Box(
                modifier = Modifier
                    .size(36.dp)
                    .clip(RoundedCornerShape(12.dp))
                    .background(AccentGreen.copy(alpha = 0.15f)),
                contentAlignment = Alignment.Center
            ) {
                Icon(
                    imageVector = icon,
                    contentDescription = null,
                    tint = AccentGreen,
                    modifier = Modifier.size(20.dp)
                )
            }
            Spacer(modifier = Modifier.weight(1f))
            Icon(
                imageVector = if (expanded) Icons.Filled.ExpandLess else Icons.Filled.ExpandMore,
                contentDescription = if (expanded) "Hide $title" else "Show $title",
                tint = SecondaryText,
                modifier = Modifier.size(20.dp)
            )
        }
        Spacer(modifier = Modifier.weight(1f))
        Text(
            text = title,
            style = MaterialTheme.typography.titleSmall,
            color = PrimaryText,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis
        )
        Text(
            text = description,
            style = MaterialTheme.typography.labelSmall,
            color = SecondaryText,
            modifier = Modifier.padding(top = 2.dp)
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
    var showDetails by rememberSaveable { mutableStateOf(false) }
    val panelShape = RoundedCornerShape(18.dp)
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(panelShape)
            .background(SurfaceDark)
            .border(1.dp, Color.White.copy(alpha = 0.06f), panelShape)
            .padding(DownloadCardPadding)
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(
                imageVector = Icons.Filled.Link,
                contentDescription = null,
                tint = AccentGreen,
                modifier = Modifier.size(20.dp)
            )
            Spacer(modifier = Modifier.width(Dimens.paddingSmall))
            Text(
                text = "Paste a link",
                style = MaterialTheme.typography.titleMedium,
                color = PrimaryText
            )
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
                text = if (isQueuing) "Adding…" else "Add to downloads",
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
            .clip(RoundedCornerShape(18.dp))
            .background(SurfaceDark)
            .border(1.dp, Color.White.copy(alpha = 0.06f), RoundedCornerShape(18.dp))
            .padding(DownloadCardPadding)
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(
                imageVector = Icons.Filled.UploadFile,
                contentDescription = null,
                tint = AccentGreen,
                modifier = Modifier.size(20.dp)
            )
            Spacer(modifier = Modifier.width(Dimens.paddingSmall))
            Column {
                Text(
                    text = "Import a playlist",
                    style = MaterialTheme.typography.titleMedium,
                    color = PrimaryText
                )
                Text(
                    text = "Bring in a playlist file",
                    style = MaterialTheme.typography.bodySmall,
                    color = SecondaryText,
                    modifier = Modifier.padding(top = 2.dp)
                )
            }
        }
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
            Text(if (isImporting) "Finding songs…" else "Choose file")
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
private fun DownloadFlow() {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(18.dp))
            .background(SurfaceDark)
            .padding(DownloadCardPadding)
    ) {
        Text(
            text = "Find it. Queue it. Play it.",
            style = MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.SemiBold,
            color = PrimaryText
        )
        Spacer(modifier = Modifier.height(DownloadControlSpacing))
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically
        ) {
            FlowStep(
                modifier = Modifier.weight(1f),
                icon = Icons.Filled.Search,
                label = "Find"
            )
            FlowConnector()
            FlowStep(
                modifier = Modifier.weight(1f),
                icon = Icons.Filled.Download,
                label = "Queue"
            )
            FlowConnector()
            FlowStep(
                modifier = Modifier.weight(1f),
                icon = Icons.Filled.CheckCircle,
                label = "Listen"
            )
        }
    }
}

@Composable
private fun FlowStep(
    modifier: Modifier,
    icon: ImageVector,
    label: String
) {
    Column(
        modifier = modifier,
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Box(
            modifier = Modifier
                .size(38.dp)
                .clip(RoundedCornerShape(13.dp))
                .background(AccentGreen.copy(alpha = 0.15f)),
            contentAlignment = Alignment.Center
        ) {
            Icon(
                imageVector = icon,
                contentDescription = null,
                tint = AccentGreen,
                modifier = Modifier.size(20.dp)
            )
        }
        Text(
            text = label,
            style = MaterialTheme.typography.labelMedium,
            color = SecondaryText,
            modifier = Modifier.padding(top = 6.dp)
        )
    }
}

@Composable
private fun FlowConnector() {
    Box(
        modifier = Modifier
            .width(24.dp)
            .height(1.dp)
            .background(Color.White.copy(alpha = 0.14f))
    )
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
            if (duration.isNotBlank()) {
                Text(
                    text = duration,
                    style = MaterialTheme.typography.labelSmall,
                    color = SecondaryText,
                    modifier = Modifier.padding(top = 2.dp)
                )
            }
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
            Box(
                modifier = Modifier
                    .size(48.dp)
                    .clip(RoundedCornerShape(16.dp))
                    .background(AccentGreen.copy(alpha = 0.14f)),
                contentAlignment = Alignment.Center
            ) {
                Icon(
                    imageVector = Icons.Filled.CheckCircle,
                    contentDescription = "Downloaded",
                    tint = AccentGreen,
                    modifier = Modifier.size(24.dp)
                )
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
            Box(
                modifier = Modifier
                    .size(48.dp)
                    .clip(RoundedCornerShape(16.dp))
                    .background(AccentGreen)
                    .clickable(
                        interactionSource = remember { MutableInteractionSource() },
                        indication = null,
                        onClick = onDownload
                    ),
                contentAlignment = Alignment.Center
            ) {
                Icon(
                    imageVector = Icons.Filled.Download,
                    contentDescription = "Download ${job?.title ?: "song"}",
                    tint = Color.Black,
                    modifier = Modifier.size(22.dp)
                )
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
