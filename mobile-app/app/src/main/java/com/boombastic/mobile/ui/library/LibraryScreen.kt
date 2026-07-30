package com.boombastic.mobile.ui.library

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.Sync
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
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
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.boombastic.mobile.BoomBasticApp
import com.boombastic.mobile.data.db.entity.Playlist
import com.boombastic.mobile.data.db.entity.Track
import com.boombastic.mobile.playback.MediaTrack
import com.boombastic.mobile.playback.MusicController
import com.boombastic.mobile.ui.home.SectionHeader
import com.boombastic.mobile.ui.theme.AccentGreen
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText
import com.boombastic.mobile.ui.theme.SurfaceDark
import com.boombastic.mobile.ui.theme.SurfaceElevated
import kotlinx.coroutines.launch

@Composable
fun LibraryScreen(
    musicController: MusicController,
    onPlay: (MediaTrack) -> Unit = {}
) {
    val context = LocalContext.current
    val app = context.applicationContext as BoomBasticApp
    val scope = rememberCoroutineScope()
    val allTracks by app.libraryRepository.getAllTracks().collectAsState(initial = emptyList())
    val playlists by app.playlistRepository.getAllPlaylists().collectAsState(initial = emptyList())
    var showNewPlaylist by remember { mutableStateOf(false) }

    LazyColumn(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = Dimens.paddingLarge),
        verticalArrangement = Arrangement.spacedBy(Dimens.paddingMedium),
        contentPadding = PaddingValues(vertical = Dimens.paddingLarge)
    ) {
        item {
            Text(
                text = "Your Library",
                style = MaterialTheme.typography.headlineLarge,
                color = PrimaryText,
                modifier = Modifier.padding(bottom = Dimens.paddingMedium)
            )
        }

        // New playlist
        item {
            Button(
                onClick = { showNewPlaylist = !showNewPlaylist },
                colors = ButtonDefaults.buttonColors(containerColor = AccentGreen)
            ) {
                Icon(Icons.Default.Add, contentDescription = null, modifier = Modifier.size(Dimens.iconSizeSmall))
                Spacer(modifier = Modifier.width(Dimens.paddingSmall))
                Text("New Playlist")
            }
        }

        if (showNewPlaylist) {
            item {
                NewPlaylistForm(
                    onCreate = { name, desc, url ->
                        scope.launch {
                            app.playlistRepository.createPlaylist(name, desc, url)
                            showNewPlaylist = false
                        }
                    }
                )
            }
        }

        // Playlists section
        item {
            SectionHeader(title = "Playlists (${playlists.size})")
        }

        if (playlists.isEmpty()) {
            item {
                Text(
                    text = "No playlists yet. Create one above.",
                    color = SecondaryText,
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.padding(vertical = Dimens.paddingMedium)
                )
            }
        } else {
            items(playlists, key = { it.id }) { playlist ->
                PlaylistCard(
                    playlist = playlist,
                    onSync = {
                        if (playlist.playlistUrl.isNotBlank()) {
                            scope.launch {
                                app.downloadRepository.syncPlaylist(
                                    playlistId = playlist.id,
                                    playlistName = playlist.name,
                                    playlistUrl = playlist.playlistUrl
                                )
                            }
                        }
                    },
                    onUrlChanged = { url ->
                        scope.launch {
                            app.playlistRepository.updatePlaylistUrl(playlist.id, url)
                        }
                    }
                )
            }
        }

        // Tracks section
        item {
            SectionHeader(title = "Tracks (${allTracks.size})")
        }

        if (allTracks.isEmpty()) {
            item {
                Text(
                    text = "Import audio files or search the web from the Search tab.",
                    color = SecondaryText,
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.padding(vertical = Dimens.paddingMedium)
                )
            }
        } else {
            items(allTracks, key = { it.uri }) { track ->
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

@Composable
private fun NewPlaylistForm(
    onCreate: (String, String, String) -> Unit
) {
    var name by rememberSaveable { mutableStateOf("") }
    var desc by rememberSaveable { mutableStateOf("") }
    var url by rememberSaveable { mutableStateOf("") }

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = Dimens.paddingSmall)
    ) {
        OutlinedTextField(
            value = name,
            onValueChange = { name = it },
            modifier = Modifier.fillMaxWidth(),
            placeholder = { Text("Playlist name", color = SecondaryText) },
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
        OutlinedTextField(
            value = desc,
            onValueChange = { desc = it },
            modifier = Modifier.fillMaxWidth(),
            placeholder = { Text("Description (optional)", color = SecondaryText) },
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
        OutlinedTextField(
            value = url,
            onValueChange = { url = it },
            modifier = Modifier.fillMaxWidth(),
            placeholder = { Text("YouTube playlist URL (optional)", color = SecondaryText) },
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
        Button(
            onClick = { if (name.isNotBlank()) onCreate(name.trim(), desc.trim(), url.trim()) },
            enabled = name.isNotBlank(),
            colors = ButtonDefaults.buttonColors(containerColor = AccentGreen),
            modifier = Modifier.fillMaxWidth()
        ) {
            Text("Create")
        }
    }
}

@Composable
private fun PlaylistCard(
    playlist: Playlist,
    onSync: () -> Unit,
    onUrlChanged: (String) -> Unit
) {
    var editingUrl by rememberSaveable(playlist.id) { mutableStateOf(false) }
    var urlInput by rememberSaveable(playlist.id) { mutableStateOf(playlist.playlistUrl) }

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = Dimens.paddingSmall)
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.fillMaxWidth()
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = playlist.name,
                    style = MaterialTheme.typography.titleSmall,
                    color = PrimaryText
                )
                if (playlist.description.isNotBlank()) {
                    Text(
                        text = playlist.description,
                        style = MaterialTheme.typography.bodySmall,
                        color = SecondaryText,
                        maxLines = 1
                    )
                }
            }

            if (playlist.playlistUrl.isNotBlank()) {
                IconButton(onClick = onSync) {
                    Icon(
                        Icons.Default.Sync,
                        contentDescription = "Sync playlist",
                        tint = AccentGreen,
                        modifier = Modifier.size(Dimens.iconSize)
                    )
                }
                IconButton(onClick = { editingUrl = !editingUrl }) {
                    Icon(
                        Icons.Default.Download,
                        contentDescription = "Edit URL",
                        tint = SecondaryText,
                        modifier = Modifier.size(Dimens.iconSize)
                    )
                }
            } else {
                IconButton(onClick = { editingUrl = !editingUrl }) {
                    Icon(
                        Icons.Default.Download,
                        contentDescription = "Set playlist URL",
                        tint = SecondaryText,
                        modifier = Modifier.size(Dimens.iconSize)
                    )
                }
            }
        }

        if (editingUrl) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth()
            ) {
                OutlinedTextField(
                    value = urlInput,
                    onValueChange = { urlInput = it },
                    modifier = Modifier.weight(1f),
                    placeholder = { Text("YouTube playlist URL", color = SecondaryText) },
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
                        onUrlChanged(urlInput.trim())
                        editingUrl = false
                    },
                    colors = ButtonDefaults.buttonColors(containerColor = AccentGreen)
                ) { Text("Save") }
            }
        }
    }
}

@Composable
private fun TrackRow(
    track: Track,
    onClick: () -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(vertical = Dimens.paddingSmall),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Column(modifier = Modifier.weight(1f)) {
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