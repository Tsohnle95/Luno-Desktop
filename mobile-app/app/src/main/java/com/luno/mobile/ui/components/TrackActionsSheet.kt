package com.luno.mobile.ui.components

import android.widget.Toast
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.PlaylistAdd
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.luno.mobile.LunoApp
import com.luno.mobile.data.db.entity.Track
import com.luno.mobile.ui.theme.AccentGreen
import com.luno.mobile.ui.theme.Dimens
import com.luno.mobile.ui.theme.PrimaryText
import com.luno.mobile.ui.theme.SecondaryText
import com.luno.mobile.ui.theme.SurfaceDark
import com.luno.mobile.ui.theme.SurfaceElevated
import kotlinx.coroutines.launch

/**
 * Long-press action sheet for a track: "Add to playlist" (nested playlist
 * picker) and "Delete from library" (confirm dialog, cascades out of all
 * playlists).  [onDeleted] lets the host remove the row from its list.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TrackActionsSheet(
    track: Track,
    onDismiss: () -> Unit,
    onDeleted: () -> Unit = {}
) {
    val context = LocalContext.current
    val app = context.applicationContext as LunoApp
    val scope = rememberCoroutineScope()
    var showPlaylistPicker by remember { mutableStateOf(false) }
    var showDeleteConfirm by remember { mutableStateOf(false) }
    var showNewPlaylistDialog by remember { mutableStateOf(false) }

    if (showNewPlaylistDialog) {
        NewPlaylistForTrackDialog(
            trackTitle = track.title,
            onDismiss = { showNewPlaylistDialog = false },
            onCreate = { name, description ->
                scope.launch {
                    app.playlistRepository.createPlaylist(name, description)
                        .onSuccess { playlist ->
                            app.playlistRepository.addTrackToPlaylist(
                                playlistId = playlist.id,
                                trackUri = track.uri
                            )
                            Toast.makeText(
                                context,
                                "Created \"${playlist.name}\"",
                                Toast.LENGTH_SHORT
                            ).show()
                        }
                }
                showNewPlaylistDialog = false
                onDismiss()
            }
        )
    } else if (showPlaylistPicker) {
        PlaylistPickerSheet(
            onPick = { playlist ->
                scope.launch {
                    app.playlistRepository.addTrackToPlaylist(
                        playlistId = playlist.id,
                        trackUri = track.uri
                    )
                }
                Toast.makeText(
                    context,
                    "Added to ${playlist.name}",
                    Toast.LENGTH_SHORT
                ).show()
                showPlaylistPicker = false
                onDismiss()
            },
            onDismiss = { showPlaylistPicker = false },
            onCreateNew = {
                showPlaylistPicker = false
                showNewPlaylistDialog = true
            }
        )
    } else if (showDeleteConfirm) {
        AlertDialog(
            onDismissRequest = { showDeleteConfirm = false },
            containerColor = SurfaceDark,
            titleContentColor = PrimaryText,
            textContentColor = SecondaryText,
            title = { Text("Remove from library?") },
            text = { Text("\"${track.title}\" will be removed from your library and all playlists. The audio file stays on your phone.") },
            confirmButton = {
                TextButton(onClick = {
                    scope.launch {
                        app.libraryRepository.deleteTrack(track.uri)
                    }
                    showDeleteConfirm = false
                    onDeleted()
                    onDismiss()
                }) {
                    Text("Remove", color = AccentGreen)
                }
            },
            dismissButton = {
                TextButton(onClick = { showDeleteConfirm = false }) {
                    Text("Cancel", color = SecondaryText)
                }
            }
        )
    } else {
        ModalBottomSheet(
            onDismissRequest = onDismiss,
            containerColor = SurfaceDark
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = Dimens.paddingLarge)
            ) {
                Text(
                    text = track.title,
                    style = MaterialTheme.typography.titleLarge,
                    color = PrimaryText,
                    maxLines = 1
                )
                Text(
                    text = track.artist,
                    style = MaterialTheme.typography.bodyMedium,
                    color = SecondaryText,
                    maxLines = 1
                )
                Spacer(modifier = Modifier.height(Dimens.paddingMedium))

                TrackActionRow(
                    icon = Icons.AutoMirrored.Filled.PlaylistAdd,
                    label = "Add to playlist",
                    onClick = { showPlaylistPicker = true }
                )
                TrackActionRow(
                    icon = Icons.Filled.Delete,
                    label = "Remove from library",
                    onClick = { showDeleteConfirm = true }
                )

                Spacer(modifier = Modifier.height(Dimens.paddingXLarge))
            }
        }
    }
}

@Composable
private fun TrackActionRow(
    icon: ImageVector,
    label: String,
    onClick: () -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(Dimens.cornerMedium))
            .clickable(onClick = onClick)
            .padding(vertical = Dimens.paddingMedium, horizontal = Dimens.paddingSmall),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            tint = PrimaryText,
            modifier = Modifier.size(Dimens.iconSize)
        )
        Spacer(modifier = Modifier.width(Dimens.paddingLarge))
        Text(
            text = label,
            style = MaterialTheme.typography.bodyLarge,
            color = PrimaryText
        )
    }
}

/** Creates a new playlist (and adds the long-pressed track to it). */
@Composable
private fun NewPlaylistForTrackDialog(
    trackTitle: String,
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
        title = { Text("New playlist") },
        text = {
            Column {
                Text(
                    text = "Add \"$trackTitle\" to a new playlist.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = SecondaryText
                )
                Spacer(modifier = Modifier.height(Dimens.paddingMedium))
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
                    value = description,
                    onValueChange = { description = it },
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
