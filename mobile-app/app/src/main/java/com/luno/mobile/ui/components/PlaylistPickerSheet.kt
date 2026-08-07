package com.luno.mobile.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.PlaylistAdd
import androidx.compose.material.icons.filled.Clear
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.luno.mobile.LunoApp
import com.luno.mobile.data.db.entity.Playlist
import com.luno.mobile.ui.theme.AccentGreen
import com.luno.mobile.ui.theme.Dimens
import com.luno.mobile.ui.theme.PrimaryText
import com.luno.mobile.ui.theme.SecondaryText
import com.luno.mobile.ui.theme.SurfaceDark
import com.luno.mobile.ui.theme.SurfaceElevated

/** Bottom sheet listing all playlists; [onPick] is called with the choice. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PlaylistPickerSheet(
    title: String = "Add to playlist",
    onPick: (Playlist) -> Unit,
    onDismiss: () -> Unit,
    onCreateNew: (() -> Unit)? = null,
    showRecentlySaved: Boolean = false
) {
    val app = LocalContext.current.applicationContext as LunoApp
    val playlists by app.playlistRepository.getAllPlaylists().collectAsState(initial = emptyList())
    val recentlySavedIds by app.recentlySavedPlaylistIds.collectAsState()
    var searchQuery by rememberSaveable { mutableStateOf("") }
    val filteredPlaylists = playlists.filter { playlist ->
        playlist.name.contains(searchQuery.trim(), ignoreCase = true)
    }
    val recentlySavedPlaylists = if (showRecentlySaved && searchQuery.isBlank()) {
        recentlySavedIds.mapNotNull { id -> playlists.firstOrNull { it.id == id } }
    } else {
        emptyList()
    }
    val remainingPlaylists = filteredPlaylists.filterNot { playlist ->
        playlist.id in recentlySavedPlaylists.map { it.id }
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        containerColor = SurfaceDark
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .fillMaxHeight(0.9f)
                .padding(horizontal = Dimens.paddingLarge)
        ) {
            Text(
                text = title,
                style = MaterialTheme.typography.headlineMedium,
                color = PrimaryText
            )
            Spacer(modifier = Modifier.height(Dimens.paddingMedium))

            OutlinedTextField(
                value = searchQuery,
                onValueChange = { searchQuery = it },
                modifier = Modifier.fillMaxWidth(),
                placeholder = { Text("Search playlists", color = SecondaryText) },
                leadingIcon = {
                    Icon(
                        imageVector = Icons.Filled.Search,
                        contentDescription = null,
                        tint = SecondaryText
                    )
                },
                trailingIcon = if (searchQuery.isNotEmpty()) {
                    {
                        IconButton(onClick = { searchQuery = "" }) {
                            Icon(
                                imageVector = Icons.Filled.Clear,
                                contentDescription = "Clear playlist search",
                                tint = SecondaryText
                            )
                        }
                    }
                } else {
                    null
                },
                singleLine = true,
                shape = RoundedCornerShape(Dimens.cornerSmall),
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
            Spacer(modifier = Modifier.height(Dimens.paddingMedium))

            if (onCreateNew != null) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(Dimens.cornerMedium))
                        .clickable { onCreateNew() }
                        .padding(vertical = Dimens.paddingMedium, horizontal = Dimens.paddingSmall),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Icon(
                        imageVector = Icons.AutoMirrored.Filled.PlaylistAdd,
                        contentDescription = null,
                        tint = AccentGreen,
                        modifier = Modifier.size(Dimens.iconSize)
                    )
                    Spacer(modifier = Modifier.width(Dimens.paddingLarge))
                    Text(
                        text = "Create new playlist",
                        style = MaterialTheme.typography.bodyLarge,
                        color = AccentGreen
                    )
                }
            }

            LazyColumn(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f),
                contentPadding = PaddingValues(bottom = Dimens.paddingXLarge)
            ) {
                if (playlists.isEmpty()) {
                    item {
                        Text(
                            text = if (onCreateNew != null) {
                                "No playlists yet. Create a destination above."
                            } else {
                                "No playlists yet. Create one in Your Library."
                            },
                            style = MaterialTheme.typography.bodyMedium,
                            color = SecondaryText,
                            modifier = Modifier.padding(vertical = Dimens.paddingMedium)
                        )
                    }
                } else if (filteredPlaylists.isEmpty()) {
                    item {
                        Text(
                            text = "No playlists match \"${searchQuery.trim()}\".",
                            style = MaterialTheme.typography.bodyMedium,
                            color = SecondaryText,
                            modifier = Modifier.padding(vertical = Dimens.paddingMedium)
                        )
                    }
                } else {
                    if (recentlySavedPlaylists.isNotEmpty()) {
                        item(key = "recently-saved-heading") {
                            PlaylistPickerSectionHeading("Recently saved")
                        }
                        items(
                            items = recentlySavedPlaylists,
                            key = { "recent-${it.id}" }
                        ) { playlist ->
                            PlaylistPickerRow(playlist = playlist, onPick = onPick)
                        }
                        if (remainingPlaylists.isNotEmpty()) {
                            item(key = "all-playlists-heading") {
                                PlaylistPickerSectionHeading("All playlists")
                            }
                        }
                    }
                    items(
                        items = if (recentlySavedPlaylists.isEmpty()) {
                            filteredPlaylists
                        } else {
                            remainingPlaylists
                        },
                        key = { "playlist-${it.id}" }
                    ) { playlist ->
                        PlaylistPickerRow(playlist = playlist, onPick = onPick)
                    }
                }
            }

            Spacer(modifier = Modifier.height(Dimens.paddingSmall))
        }
    }
}

@Composable
private fun PlaylistPickerSectionHeading(text: String) {
    Text(
        text = text,
        style = MaterialTheme.typography.labelLarge,
        color = AccentGreen,
        modifier = Modifier.padding(
            top = Dimens.paddingMedium,
            bottom = Dimens.paddingSmall,
            start = Dimens.paddingSmall
        )
    )
}

@Composable
private fun PlaylistPickerRow(
    playlist: Playlist,
    onPick: (Playlist) -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(Dimens.cornerMedium))
            .clickable { onPick(playlist) }
            .padding(vertical = Dimens.paddingMedium, horizontal = Dimens.paddingSmall),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Icon(
            imageVector = Icons.AutoMirrored.Filled.PlaylistAdd,
            contentDescription = null,
            tint = PrimaryText,
            modifier = Modifier.size(Dimens.iconSize)
        )
        Spacer(modifier = Modifier.width(Dimens.paddingLarge))
        Text(
            text = playlist.name,
            style = MaterialTheme.typography.bodyLarge,
            color = PrimaryText
        )
    }
}
