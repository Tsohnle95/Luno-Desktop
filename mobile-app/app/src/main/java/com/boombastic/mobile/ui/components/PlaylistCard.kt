package com.boombastic.mobile.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.RadioButtonUnchecked
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.boombastic.mobile.data.db.entity.Playlist
import com.boombastic.mobile.data.db.entity.Track
import com.boombastic.mobile.ui.theme.AccentGreen
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText

/**
 * Spotify-style playlist card: a 2x2 four-artwork collage (first four
 * songs) as the thumbnail on the left, playlist name beside it, green
 * 3-dot options on the right, flat on the screen background.
 *
 * [onSync] / [onUrlChanged] / [onDelete] control which items appear in the
 * 3-dot menu; passing only [onClick] yields a plain tappable card.
 */
@Composable
@OptIn(ExperimentalFoundationApi::class)
fun PlaylistCard(
    playlist: Playlist,
    tracks: List<Track>,
    onClick: () -> Unit,
    onSync: (() -> Unit)? = null,
    onStopSync: (() -> Unit)? = null,
    onUrlChanged: ((String) -> Unit)? = null,
    onClearPlaylist: (() -> Unit)? = null,
    onDelete: (() -> Unit)? = null,
    onExport: (() -> Unit)? = null,
    modifier: Modifier = Modifier,
    selected: Boolean? = null,
    onLongClick: (() -> Unit)? = null
) {
    var showMenu by remember { mutableStateOf(false) }

    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(Dimens.paddingSmall)
            .then(
                if (onLongClick != null) {
                    Modifier.combinedClickable(
                        interactionSource = remember { MutableInteractionSource() },
                        indication = null,
                        onClick = onClick,
                        onLongClick = onLongClick
                    )
                } else {
                    Modifier.clickable(
                        interactionSource = remember { MutableInteractionSource() },
                        indication = null,
                        onClick = onClick
                    )
                }
            ),
        verticalAlignment = Alignment.CenterVertically
    ) {
        if (selected != null) {
            Icon(
                imageVector = if (selected) {
                    Icons.Filled.CheckCircle
                } else {
                    Icons.Filled.RadioButtonUnchecked
                },
                contentDescription = if (selected) "Selected" else "Not selected",
                tint = if (selected) AccentGreen else SecondaryText,
                modifier = Modifier.size(Dimens.iconSize)
            )
            Spacer(modifier = Modifier.width(Dimens.paddingSmall))
        }

        ArtworkCollage(
            tracks = tracks,
            modifier = Modifier.size(Dimens.albumArtSmall),
            placeholderIconSize = 14.dp,
            decodeSizePx = 192
        )

        Spacer(modifier = Modifier.width(Dimens.paddingSmall))

        Column(
            modifier = Modifier
                .weight(1f)
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
                if (onSync != null && playlist.playlistUrl.isNotBlank()) {
                    DropdownMenuItem(
                        text = { Text("Sync playlist", color = PrimaryText) },
                        onClick = {
                            showMenu = false
                            onSync()
                        }
                    )
                }
                if (onStopSync != null) {
                    DropdownMenuItem(
                        text = { Text("Stop sync", color = PrimaryText) },
                        onClick = {
                            showMenu = false
                            onStopSync()
                        }
                    )
                }
                if (onUrlChanged != null) {
                    DropdownMenuItem(
                        text = {
                            Text(
                                if (playlist.playlistUrl.isNotBlank()) "Edit YouTube URL" else "Set YouTube URL",
                                color = PrimaryText
                            )
                        },
                        onClick = {
                            showMenu = false
                            onUrlChanged(playlist.playlistUrl)
                        }
                    )
                }
                if (onClearPlaylist != null && tracks.isNotEmpty()) {
                    DropdownMenuItem(
                        text = { Text("Clear playlist", color = PrimaryText) },
                        onClick = {
                            showMenu = false
                            onClearPlaylist()
                        }
                    )
                }
                if (onExport != null) {
                    DropdownMenuItem(
                        text = { Text("Export playlist", color = PrimaryText) },
                        onClick = {
                            showMenu = false
                            onExport()
                        }
                    )
                }
                if (onDelete != null) {
                    DropdownMenuItem(
                        text = { Text("Delete playlist", color = PrimaryText) },
                        onClick = {
                            showMenu = false
                            onDelete()
                        }
                    )
                }
            }
        }
    }
}
