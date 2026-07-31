package com.boombastic.mobile.ui.components

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
import androidx.compose.material.icons.filled.PlaylistAdd
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
import com.boombastic.mobile.BoomBasticApp
import com.boombastic.mobile.data.db.entity.Playlist
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText
import com.boombastic.mobile.ui.theme.SurfaceDark

/** Bottom sheet listing all playlists; [onPick] is called with the choice. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PlaylistPickerSheet(
    onPick: (Playlist) -> Unit,
    onDismiss: () -> Unit
) {
    val app = LocalContext.current.applicationContext as BoomBasticApp
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
                text = "Add to playlist",
                style = MaterialTheme.typography.headlineMedium,
                color = PrimaryText
            )
            Spacer(modifier = Modifier.height(Dimens.paddingMedium))

            if (playlists.isEmpty()) {
                Text(
                    text = "No playlists yet. Create one in Your Library.",
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
                            imageVector = Icons.Filled.PlaylistAdd,
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
