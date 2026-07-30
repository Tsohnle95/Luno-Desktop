package com.boombastic.mobile.ui.search

import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.boombastic.mobile.BoomBasticApp
import com.boombastic.mobile.data.db.entity.Track
import com.boombastic.mobile.playback.MediaTrack
import com.boombastic.mobile.playback.MusicController
import com.boombastic.mobile.ui.theme.AccentGreen
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText
import com.boombastic.mobile.ui.theme.SurfaceDark
import com.boombastic.mobile.ui.theme.SurfaceElevated
import kotlinx.coroutines.launch

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SearchScreen(
    musicController: MusicController,
    onPlay: (MediaTrack) -> Unit = {}
) {
    val context = LocalContext.current
    val app = context.applicationContext as BoomBasticApp
    val scope = rememberCoroutineScope()

    var query by rememberSaveable { mutableStateOf("") }
    val searchResults by app.libraryRepository.searchTracks(query).collectAsState(initial = emptyList())
    val allTracks by app.libraryRepository.getAllTracks().collectAsState(initial = emptyList())

    // SAF launcher for opening documents — NOT notification permission
    val importLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.OpenMultipleDocuments()
    ) { uris: List<Uri> ->
        if (uris.isNotEmpty()) {
            scope.launch {
                val result = app.libraryRepository.importMultipleUris(uris)
                // Result logged but not shown as toast to keep it clean
            }
        }
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = Dimens.paddingLarge)
    ) {
        // Header
        Text(
            text = "Search",
            style = androidx.compose.material3.MaterialTheme.typography.headlineLarge,
            color = PrimaryText,
            modifier = Modifier.padding(
                top = Dimens.paddingLarge,
                bottom = Dimens.paddingMedium
            )
        )

        // Search field
        OutlinedTextField(
            value = query,
            onValueChange = { query = it },
            modifier = Modifier.fillMaxWidth(),
            placeholder = {
                Text(
                    text = "Search your library...",
                    color = SecondaryText
                )
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

        // Import button area
        androidx.compose.material3.TextButton(
            onClick = {
                importLauncher.launch(arrayOf("audio/*"))
            },
            modifier = Modifier.padding(vertical = Dimens.paddingMedium)
        ) {
            Text(
                text = "Import audio files",
                color = AccentGreen,
                style = androidx.compose.material3.MaterialTheme.typography.labelLarge
            )
        }

        // Results
        val displayTracks = if (query.isBlank()) allTracks else searchResults

        if (displayTracks.isEmpty()) {
            Box(
                modifier = Modifier.fillMaxSize(),
                contentAlignment = Alignment.Center
            ) {
                Text(
                    text = if (query.isBlank()) "Import audio to see your tracks here"
                    else "No results for \"$query\"",
                    color = SecondaryText,
                    style = androidx.compose.material3.MaterialTheme.typography.bodyMedium
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
                                    durationMs = track.durationMs
                                )
                            )
                        }
                    )
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
                style = androidx.compose.material3.MaterialTheme.typography.titleSmall,
                color = PrimaryText,
                maxLines = 1
            )
            Text(
                text = track.artist,
                style = androidx.compose.material3.MaterialTheme.typography.bodySmall,
                color = SecondaryText,
                maxLines = 1
            )
        }
    }
}
