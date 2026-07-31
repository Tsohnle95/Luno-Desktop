package com.boombastic.mobile.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MoreVert
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.boombastic.mobile.data.db.entity.Playlist
import com.boombastic.mobile.ui.theme.AccentGreen
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SurfaceDark

/**
 * Spotify-style playlist card for 2-column grids: first-track artwork
 * thumbnail on the left, playlist name beside it, green 3-dot options on
 * the right, all on a dark gray surface that stands out against the black
 * screen background.
 *
 * [onSync] / [onUrlChanged] / [onDelete] control which items appear in the
 * 3-dot menu; passing only [onClick] yields a plain tappable card (used on
 * Home for quick-action playlists).
 */
@Composable
fun PlaylistCard(
    playlist: Playlist,
    thumbnailUri: String?,
    onClick: () -> Unit,
    onSync: (() -> Unit)? = null,
    onUrlChanged: ((String) -> Unit)? = null,
    onDelete: (() -> Unit)? = null,
    modifier: Modifier = Modifier
) {
    var showMenu by remember { mutableStateOf(false) }

    Row(
        modifier = modifier
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
                .clickable(onClick = onClick)
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
