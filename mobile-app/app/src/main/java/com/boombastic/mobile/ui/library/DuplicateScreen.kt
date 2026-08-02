package com.boombastic.mobile.ui.library

import android.widget.Toast
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.boombastic.mobile.BoomBasticApp
import com.boombastic.mobile.data.db.entity.Track
import com.boombastic.mobile.data.repository.DuplicateGroup
import com.boombastic.mobile.data.repository.findDuplicateGroups
import com.boombastic.mobile.ui.components.TrackRowCard
import com.boombastic.mobile.ui.components.BulkSelectionToolbar
import com.boombastic.mobile.ui.theme.AccentGreen
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText
import com.boombastic.mobile.ui.theme.SurfaceDark
import kotlinx.coroutines.launch

@Composable
fun DuplicateScreen(onBack: () -> Unit) {
    val context = LocalContext.current
    val app = context.applicationContext as BoomBasticApp
    val scope = rememberCoroutineScope()
    val allTracks by app.libraryData.tracks.collectAsState()
    val libraryLoaded by app.libraryData.loaded.collectAsState()
    val groups = remember(allTracks) { findDuplicateGroups(allTracks) }
    var selectedUris by remember { mutableStateOf<Set<String>>(emptySet()) }
    var showRemoveConfirm by remember { mutableStateOf(false) }

    if (!libraryLoaded) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            CircularProgressIndicator(color = AccentGreen)
        }
        return
    }

    val duplicateTracks = groups.flatMap { it.tracks }
    val visibleSelectedUris = selectedUris.intersect(duplicateTracks.map { it.uri }.toSet())

    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        verticalArrangement = Arrangement.spacedBy(Dimens.paddingSmall),
        contentPadding = PaddingValues(bottom = Dimens.paddingLarge)
    ) {
        item {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = Dimens.paddingSmall, vertical = Dimens.paddingMedium),
                verticalAlignment = Alignment.CenterVertically
            ) {
                IconButton(onClick = onBack) {
                    Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back", tint = PrimaryText)
                }
                Text(
                    text = "Duplicate songs",
                    style = MaterialTheme.typography.headlineSmall,
                    color = PrimaryText,
                    fontWeight = FontWeight.Bold,
                    modifier = Modifier.weight(1f)
                )
            }
        }
        item {
            Text(
                text = if (groups.isEmpty()) {
                    "No duplicate songs found in your library."
                } else {
                    "${groups.size} duplicate groups · ${duplicateTracks.size} songs"
                },
                style = MaterialTheme.typography.bodyMedium,
                color = SecondaryText,
                modifier = Modifier.padding(horizontal = Dimens.paddingLarge)
            )
        }
        if (groups.isNotEmpty()) {
            item {
                if (visibleSelectedUris.isNotEmpty()) {
                    BulkSelectionToolbar(
                        selectedTracks = duplicateTracks.filter { it.uri in visibleSelectedUris },
                        selectedPlaylists = emptyList(),
                        allSelected = visibleSelectedUris.size == duplicateTracks.size,
                        onSelectAll = { selectAll ->
                            selectedUris = if (selectAll) {
                                duplicateTracks.map { it.uri }.toSet()
                            } else {
                                emptySet()
                            }
                        },
                        onDismiss = { selectedUris = emptySet() },
                        onAddToPlaylist = { playlist, trackUris ->
                            scope.launch {
                                app.playlistRepository.addTracksToPlaylist(playlist.id, trackUris)
                                Toast.makeText(context, "Added to ${playlist.name}", Toast.LENGTH_SHORT).show()
                            }
                            selectedUris = emptySet()
                        },
                        onCreatePlaylist = { name, description, trackUris ->
                            scope.launch {
                                app.playlistRepository.createPlaylist(name, description).onSuccess { playlist ->
                                    app.playlistRepository.addTracksToPlaylist(playlist.id, trackUris)
                                    Toast.makeText(context, "Created ${playlist.name}", Toast.LENGTH_SHORT).show()
                                }
                            }
                            selectedUris = emptySet()
                        },
                        onRemoveTracks = { uris ->
                            scope.launch { uris.forEach { app.libraryRepository.deleteTrack(it) } }
                            selectedUris = emptySet()
                        },
                        modifier = Modifier.padding(horizontal = Dimens.paddingLarge)
                    )
                } else {
                    TextButton(
                        onClick = { selectedUris = duplicateTracks.map { it.uri }.toSet() }
                    ) {
                        Text("Select all", color = AccentGreen)
                    }
                }
            }
            groups.forEach { group ->
                item(key = "header-${group.key}") { DuplicateGroupHeader(group) }
                items(
                    group.tracks,
                    key = { it.uri },
                    contentType = { "duplicate-track" }
                ) { track ->
                    TrackRowCard(
                        track = track,
                        selected = track.uri in visibleSelectedUris,
                        onClick = {
                            selectedUris = if (track.uri in selectedUris) {
                                selectedUris - track.uri
                            } else {
                                selectedUris + track.uri
                            }
                        },
                        onMenuClick = { /* Removal is handled by the selection toolbar. */ },
                        onLongClick = {
                            selectedUris = selectedUris + track.uri
                        },
                        modifier = Modifier.padding(horizontal = Dimens.paddingLarge)
                    )
                }
            }
        }
    }

    if (showRemoveConfirm) {
        AlertDialog(
            onDismissRequest = { showRemoveConfirm = false },
            containerColor = SurfaceDark,
            titleContentColor = PrimaryText,
            textContentColor = SecondaryText,
            title = { Text("Remove selected songs?") },
            text = {
                Text(
                    "${visibleSelectedUris.size} song(s) will be removed from the library and playlists. " +
                        "The audio files will stay on your device."
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    val uris = visibleSelectedUris.toList()
                    scope.launch {
                        uris.forEach { app.libraryRepository.deleteTrack(it) }
                        Toast.makeText(context, "Removed ${uris.size} song(s)", Toast.LENGTH_SHORT).show()
                    }
                    selectedUris = emptySet()
                    showRemoveConfirm = false
                }) {
                    Text("Remove", color = AccentGreen)
                }
            },
            dismissButton = {
                TextButton(onClick = { showRemoveConfirm = false }) {
                    Text("Cancel", color = SecondaryText)
                }
            }
        )
    }

}

@Composable
private fun DuplicateGroupHeader(group: DuplicateGroup) {
    val first = group.tracks.first()
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = Dimens.paddingLarge, vertical = Dimens.paddingMedium)
    ) {
        Text(
            text = first.title,
            style = MaterialTheme.typography.titleMedium,
            color = PrimaryText,
            fontWeight = FontWeight.Bold
        )
        Text(
            text = "${first.artist.ifBlank { "Unknown artist" }} · ${group.tracks.size} copies",
            style = MaterialTheme.typography.bodySmall,
            color = SecondaryText
        )
    }
}
