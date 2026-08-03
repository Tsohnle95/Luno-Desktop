package com.luno.mobile.ui.components

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
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
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

/** Bottom sheet listing all playlists; [onPick] is called with the choice. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PlaylistPickerSheet(
    title: String = "Add to playlist",
    onPick: (Playlist) -> Unit,
    onDismiss: () -> Unit,
    onCreateNew: (() -> Unit)? = null
) {
    val app = LocalContext.current.applicationContext as LunoApp
    val playlists by app.playlistRepository.getAllPlaylists().collectAsState(initial = emptyList())

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
                text = title,
                style = MaterialTheme.typography.headlineMedium,
                color = PrimaryText
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

            if (playlists.isEmpty()) {
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
            } else {
                playlists.forEach { playlist ->
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
            }

            Spacer(modifier = Modifier.height(Dimens.paddingXLarge))
        }
    }
}
