package com.boombastic.mobile.ui.library

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
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.boombastic.mobile.BoomBasticApp
import com.boombastic.mobile.data.db.dao.PlaylistWithTracks
import com.boombastic.mobile.data.db.entity.Playlist
import com.boombastic.mobile.data.db.entity.Track
import com.boombastic.mobile.playback.MediaTrack
import com.boombastic.mobile.playback.MusicController
import com.boombastic.mobile.ui.components.ArtworkImage
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
    val playlistsWithTracks by app.playlistRepository.getAllPlaylistsWithTracks()
        .collectAsState(initial = emptyList())
    var showNewPlaylist by remember { mutableStateOf(false) }

    LazyVerticalGrid(
        columns = GridCells.Fixed(2),
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = Dimens.paddingLarge),
        verticalArrangement = Arrangement.spacedBy(Dimens.paddingMedium),
        horizontalArrangement = Arrangement.spacedBy(Dimens.paddingMedium),
        contentPadding = PaddingValues(vertical = Dimens.paddingLarge)
    ) {
        item(span = { GridItemSpan(maxLineSpan) }) {
            Text(
                text = "Your Library",
                style = MaterialTheme.typography.headlineLarge,
                color = PrimaryText,
                modifier = Modifier.padding(bottom = Dimens.paddingMedium)
            )
        }

        // New playlist
        item(span = { GridItemSpan(maxLineSpan) }) {
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
            item(span = { GridItemSpan(maxLineSpan) }) {
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

        // Playlists section — two cards per horizontal block
        item(span = { GridItemSpan(maxLineSpan) }) {
            SectionHeader(title = "Playlists (${playlistsWithTracks.size})")
        }

        if (playlistsWithTracks.isEmpty()) {
            item(span = { GridItemSpan(maxLineSpan) }) {
                Text(
                    text = "No playlists yet. Create one above.",
                    color = SecondaryText,
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.padding(vertical = Dimens.paddingMedium)
                )
            }
        } else {
            items(playlistsWithTracks, key = { it.playlist.id }) { playlistWithTracks ->
                PlaylistCard(
                    playlist = playlistWithTracks.playlist,
                    thumbnailUri = playlistWithTracks.tracks.firstOrNull()?.albumArtUri(),
                    onSync = {
                        if (playlistWithTracks.playlist.playlistUrl.isNotBlank()) {
                            scope.launch {
                                app.downloadRepository.syncPlaylist(
                                    playlistId = playlistWithTracks.playlist.id,
                                    playlistName = playlistWithTracks.playlist.name,
                                    playlistUrl = playlistWithTracks.playlist.playlistUrl
                                )
                            }
                        }
                    },
                    onUrlChanged = { url ->
                        scope.launch {
                            app.playlistRepository.updatePlaylistUrl(playlistWithTracks.playlist.id, url)
                        }
                    },
                    onDelete = {
                        scope.launch {
                            app.playlistRepository.deletePlaylist(playlistWithTracks.playlist.id)
                        }
                    }
                )
            }
        }

        // Tracks section
        item(span = { GridItemSpan(maxLineSpan) }) {
            SectionHeader(title = "Tracks (${allTracks.size})")
        }

        if (allTracks.isEmpty()) {
            item(span = { GridItemSpan(maxLineSpan) }) {
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

/**
 * Playlist card for the 2-column grid: thumbnail on the left, name to the
 * right of it, green 3-dot options on the right, all on a dark gray
 * surface that stands out against the black screen background.
 */
@Composable
private fun PlaylistCard(
    playlist: Playlist,
    thumbnailUri: String?,
    onSync: () -> Unit,
    onUrlChanged: (String) -> Unit,
    onDelete: () -> Unit
) {
    var showMenu by remember { mutableStateOf(false) }
    var showUrlDialog by rememberSaveable(playlist.id) { mutableStateOf(false) }
    var showDeleteDialog by rememberSaveable(playlist.id) { mutableStateOf(false) }

    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(SurfaceDark, RoundedCornerShape(Dimens.cornerMedium))
            .padding(Dimens.paddingSmall),
        verticalAlignment = Alignment.CenterVertically
    ) {
        ArtworkImage(
            artworkUri = thumbnailUri,
            modifier = Modifier
                .size(Dimens.albumArtSmall)
                .clip(RoundedCornerShape(Dimens.cornerSmall)),
            placeholderIconSize = 20.dp
        )

        Spacer(modifier = Modifier.width(Dimens.paddingSmall))

        Column(
            modifier = Modifier
                .weight(1f)
                .clickable { showMenu = true }
        ) {
            Text(
                text = playlist.name,
                style = MaterialTheme.typography.titleSmall,
                color = PrimaryText,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis
            )
        }

        // Green 3-dot options
        Box {
            IconButton(onClick = { showMenu = true }) {
                Icon(
                    imageVector = Icons.Default.MoreVert,
                    contentDescription = "Playlist options",
                    tint = AccentGreen,
                    modifier = Modifier.size(Dimens.iconSize)
                )
            }
            DropdownMenu(
                expanded = showMenu,
                onDismissRequest = { showMenu = false }
            ) {
                if (playlist.playlistUrl.isNotBlank()) {
                    DropdownMenuItem(
                        text = { Text("Sync playlist", color = PrimaryText) },
                        onClick = {
                            showMenu = false
                            onSync()
                        }
                    )
                }
                DropdownMenuItem(
                    text = {
                        Text(
                            if (playlist.playlistUrl.isNotBlank()) "Edit YouTube URL" else "Set YouTube URL",
                            color = PrimaryText
                        )
                    },
                    onClick = {
                        showMenu = false
                        showUrlDialog = true
                    }
                )
                DropdownMenuItem(
                    text = { Text("Delete playlist", color = PrimaryText) },
                    onClick = {
                        showMenu = false
                        showDeleteDialog = true
                    }
                )
            }
        }
    }

    if (showUrlDialog) {
        PlaylistUrlDialog(
            initialUrl = playlist.playlistUrl,
            onDismiss = { showUrlDialog = false },
            onSave = { url ->
                onUrlChanged(url)
                showUrlDialog = false
            }
        )
    }

    if (showDeleteDialog) {
        AlertDialog(
            onDismissRequest = { showDeleteDialog = false },
            containerColor = SurfaceDark,
            titleContentColor = PrimaryText,
            textContentColor = SecondaryText,
            title = { Text("Delete playlist?") },
            text = { Text("\"${playlist.name}\" will be removed. Tracks are not deleted.") },
            confirmButton = {
                TextButton(onClick = {
                    showDeleteDialog = false
                    onDelete()
                }) {
                    Text("Delete", color = AccentGreen)
                }
            },
            dismissButton = {
                TextButton(onClick = { showDeleteDialog = false }) {
                    Text("Cancel", color = SecondaryText)
                }
            }
        )
    }
}

@Composable
private fun PlaylistUrlDialog(
    initialUrl: String,
    onDismiss: () -> Unit,
    onSave: (String) -> Unit
) {
    var urlInput by rememberSaveable { mutableStateOf(initialUrl) }

    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = SurfaceDark,
        titleContentColor = PrimaryText,
        textContentColor = PrimaryText,
        title = { Text("YouTube playlist URL") },
        text = {
            OutlinedTextField(
                value = urlInput,
                onValueChange = { urlInput = it },
                modifier = Modifier.fillMaxWidth(),
                placeholder = { Text("https://www.youtube.com/playlist?list=...", color = SecondaryText) },
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
        },
        confirmButton = {
            TextButton(onClick = { onSave(urlInput.trim()) }) {
                Text("Save", color = AccentGreen)
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text("Cancel", color = SecondaryText)
            }
        }
    )
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
