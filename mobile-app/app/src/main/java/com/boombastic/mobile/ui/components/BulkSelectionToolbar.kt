package com.boombastic.mobile.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.PlaylistAdd
import androidx.compose.material.icons.filled.UploadFile
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.boombastic.mobile.data.db.dao.PlaylistWithTracks
import com.boombastic.mobile.data.db.entity.Playlist
import com.boombastic.mobile.data.db.entity.Track
import com.boombastic.mobile.ui.theme.AccentGreen
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText
import com.boombastic.mobile.ui.theme.SurfaceDark
import com.boombastic.mobile.ui.theme.SurfaceElevated

/** Selection toolbar with a visible select-all checkbox and anchored actions menu. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun BulkSelectionToolbar(
    selectedTracks: List<Track>,
    selectedPlaylists: List<PlaylistWithTracks>,
    allSelected: Boolean,
    onSelectAll: (Boolean) -> Unit,
    onDismiss: () -> Unit,
    onAddToPlaylist: (Playlist, List<String>) -> Unit,
    onCreatePlaylist: (String, String, List<String>) -> Unit,
    onExportTracks: ((List<String>) -> Unit)? = null,
    onExportPlaylists: ((List<Long>) -> Unit)? = null,
    onRemoveTracks: ((List<String>) -> Unit)? = null,
    onDeletePlaylists: ((List<Long>) -> Unit)? = null,
    modifier: Modifier = Modifier
) {
    var showMenu by remember { mutableStateOf(false) }
    var showPlaylistPicker by remember { mutableStateOf(false) }
    var showCreateDialog by remember { mutableStateOf(false) }
    val trackUris = remember(selectedTracks, selectedPlaylists) {
        buildList {
            addAll(selectedTracks.map { it.uri })
            selectedPlaylists.forEach { addAll(it.tracks.map { track -> track.uri }) }
        }.distinct()
    }

    Row(
        modifier = modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Checkbox(
            checked = allSelected,
            onCheckedChange = { checked -> onSelectAll(checked) },
            colors = androidx.compose.material3.CheckboxDefaults.colors(
                checkedColor = AccentGreen,
                checkmarkColor = SurfaceDark,
                uncheckedColor = SecondaryText
            )
        )
        Text(
            text = "Select all",
            style = MaterialTheme.typography.labelLarge,
            color = PrimaryText,
            modifier = Modifier
                .clickable(onClick = { onSelectAll(!allSelected) })
                .padding(end = Dimens.paddingSmall)
        )
        Text(
            text = "${selectedTracks.size + selectedPlaylists.size} selected",
            style = MaterialTheme.typography.titleMedium,
            color = AccentGreen,
            modifier = Modifier.weight(1f)
        )
        Box {
            TextButton(onClick = { showMenu = true }) {
                Text("Options", color = AccentGreen)
            }
            DropdownMenu(
                expanded = showMenu,
                onDismissRequest = { showMenu = false }
            ) {
                DropdownMenuItem(
                    text = { Text("Add to playlist", color = PrimaryText) },
                    enabled = trackUris.isNotEmpty(),
                    leadingIcon = {
                        Icon(Icons.Filled.PlaylistAdd, contentDescription = null)
                    },
                    onClick = {
                        showMenu = false
                        showPlaylistPicker = true
                    }
                )
                DropdownMenuItem(
                    text = { Text("Create new playlist", color = PrimaryText) },
                    enabled = trackUris.isNotEmpty(),
                    leadingIcon = { Icon(Icons.Filled.Add, contentDescription = null) },
                    onClick = {
                        showMenu = false
                        showCreateDialog = true
                    }
                )
                if (onExportTracks != null && selectedTracks.isNotEmpty()) {
                    DropdownMenuItem(
                        text = { Text("Export selected songs", color = PrimaryText) },
                        leadingIcon = { Icon(Icons.Filled.UploadFile, contentDescription = null) },
                        onClick = {
                            showMenu = false
                            onExportTracks(selectedTracks.map { it.uri })
                        }
                    )
                }
                if (onExportPlaylists != null && selectedPlaylists.isNotEmpty()) {
                    DropdownMenuItem(
                        text = { Text("Export selected playlists", color = PrimaryText) },
                        leadingIcon = { Icon(Icons.Filled.UploadFile, contentDescription = null) },
                        onClick = {
                            showMenu = false
                            onExportPlaylists(selectedPlaylists.map { it.playlist.id })
                        }
                    )
                }
                if (onRemoveTracks != null && selectedTracks.isNotEmpty()) {
                    DropdownMenuItem(
                        text = { Text("Remove selected songs", color = PrimaryText) },
                        leadingIcon = { Icon(Icons.Filled.Delete, contentDescription = null) },
                        onClick = {
                            showMenu = false
                            onRemoveTracks(selectedTracks.map { it.uri })
                        }
                    )
                }
                if (onDeletePlaylists != null && selectedPlaylists.isNotEmpty()) {
                    DropdownMenuItem(
                        text = { Text("Delete selected playlists", color = PrimaryText) },
                        leadingIcon = { Icon(Icons.Filled.Delete, contentDescription = null) },
                        onClick = {
                            showMenu = false
                            onDeletePlaylists(selectedPlaylists.map { it.playlist.id })
                        }
                    )
                }
                DropdownMenuItem(
                    text = { Text("Done", color = PrimaryText) },
                    onClick = {
                        showMenu = false
                        onDismiss()
                    }
                )
            }
        }
        TextButton(onClick = onDismiss) {
            Text("Done", color = SecondaryText)
        }
    }

    if (showPlaylistPicker) {
        PlaylistPickerSheet(
            onPick = { playlist ->
                onAddToPlaylist(playlist, trackUris)
                showPlaylistPicker = false
            },
            onDismiss = { showPlaylistPicker = false }
        )
    }
    if (showCreateDialog) {
        BulkCreatePlaylistDialog(
            trackCount = trackUris.size,
            onDismiss = { showCreateDialog = false },
            onCreate = { name, description ->
                onCreatePlaylist(name, description, trackUris)
                showCreateDialog = false
            }
        )
    }
}

@Composable
private fun BulkCreatePlaylistDialog(
    trackCount: Int,
    onDismiss: () -> Unit,
    onCreate: (String, String) -> Unit
) {
    var name by rememberSaveable { mutableStateOf("") }
    var description by rememberSaveable { mutableStateOf("") }

    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = SurfaceDark,
        titleContentColor = PrimaryText,
        textContentColor = PrimaryText,
        title = { Text("Create new playlist") },
        text = {
            Column {
                Text(
                    text = "Add $trackCount selected songs to this playlist.",
                    color = SecondaryText
                )
                Spacer(modifier = Modifier.height(Dimens.paddingMedium))
                OutlinedTextField(
                    value = name,
                    onValueChange = { name = it },
                    modifier = Modifier.fillMaxWidth(),
                    placeholder = { Text("Playlist name", color = SecondaryText) },
                    singleLine = true,
                    colors = bulkFieldColors()
                )
                Spacer(modifier = Modifier.height(Dimens.paddingSmall))
                OutlinedTextField(
                    value = description,
                    onValueChange = { description = it },
                    modifier = Modifier.fillMaxWidth(),
                    placeholder = { Text("Description (optional)", color = SecondaryText) },
                    singleLine = true,
                    colors = bulkFieldColors()
                )
            }
        },
        confirmButton = {
            TextButton(
                onClick = { onCreate(name.trim(), description.trim()) },
                enabled = name.isNotBlank()
            ) {
                Text("Create", color = AccentGreen)
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
private fun bulkFieldColors() = OutlinedTextFieldDefaults.colors(
    focusedTextColor = PrimaryText,
    unfocusedTextColor = PrimaryText,
    cursorColor = AccentGreen,
    focusedBorderColor = AccentGreen,
    unfocusedBorderColor = SurfaceElevated,
    focusedContainerColor = SurfaceDark,
    unfocusedContainerColor = SurfaceDark
)
