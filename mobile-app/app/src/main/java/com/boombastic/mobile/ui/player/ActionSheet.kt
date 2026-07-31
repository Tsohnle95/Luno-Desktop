package com.boombastic.mobile.ui.player

import android.content.Intent
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
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.PlaylistAdd
import androidx.compose.material.icons.filled.QueueMusic
import androidx.compose.material.icons.filled.Share
import androidx.compose.material.icons.filled.SkipNext
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.boombastic.mobile.BoomBasticApp
import com.boombastic.mobile.playback.MusicController
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText
import com.boombastic.mobile.ui.theme.SurfaceDark
import kotlinx.coroutines.launch

/**
 * Action sheet for the current track: Add to playlist (nested playlist
 * picker), Play next, Add to queue, Go to artist (placeholder toast), and
 * Share (Android Sharesheet).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PlayerActionSheet(
    musicController: MusicController,
    onDismiss: () -> Unit
) {
    val context = LocalContext.current
    val app = context.applicationContext as BoomBasticApp
    val scope = rememberCoroutineScope()
    val currentTrack by musicController.currentTrack.collectAsState()
    var showPlaylistPicker by remember { mutableStateOf(false) }

    if (showPlaylistPicker) {
        val playlists by app.playlistRepository.getAllPlaylists().collectAsState(initial = emptyList())
        ModalBottomSheet(
            onDismissRequest = { showPlaylistPicker = false },
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
                        text = "No playlists yet.",
                        style = MaterialTheme.typography.bodyMedium,
                        color = SecondaryText,
                        modifier = Modifier.padding(vertical = Dimens.paddingMedium)
                    )
                } else {
                    playlists.forEach { playlist ->
                        ActionSheetRow(
                            icon = Icons.Filled.PlaylistAdd,
                            label = playlist.name,
                            onClick = {
                                val track = currentTrack
                                if (track != null) {
                                    scope.launch {
                                        app.playlistRepository.addTrackToPlaylist(
                                            playlistId = playlist.id,
                                            trackUri = track.uri
                                        )
                                    }
                                }
                                onDismiss()
                            }
                        )
                    }
                }

                Spacer(modifier = Modifier.height(Dimens.paddingXLarge))
            }
        }
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
                ActionSheetRow(
                    icon = Icons.Filled.PlaylistAdd,
                    label = "Add to playlist",
                    onClick = { showPlaylistPicker = true }
                )
                ActionSheetRow(
                    icon = Icons.Filled.SkipNext,
                    label = "Play next",
                    onClick = {
                        currentTrack?.let { musicController.playNext(it) }
                        onDismiss()
                    }
                )
                ActionSheetRow(
                    icon = Icons.Filled.QueueMusic,
                    label = "Add to queue",
                    onClick = {
                        currentTrack?.let { musicController.addToQueue(it) }
                        onDismiss()
                    }
                )
                ActionSheetRow(
                    icon = Icons.Filled.Person,
                    label = "Go to artist",
                    onClick = {
                        Toast.makeText(
                            context,
                            "Go to artist coming soon",
                            Toast.LENGTH_SHORT
                        ).show()
                        onDismiss()
                    }
                )
                ActionSheetRow(
                    icon = Icons.Filled.Share,
                    label = "Share",
                    onClick = {
                        val track = currentTrack
                        if (track != null) {
                            val sendIntent = Intent(Intent.ACTION_SEND).apply {
                                type = "text/plain"
                                putExtra(Intent.EXTRA_TEXT, "${track.title} - ${track.artist}")
                            }
                            context.startActivity(
                                Intent.createChooser(sendIntent, "Share track")
                            )
                        }
                        onDismiss()
                    }
                )

                Spacer(modifier = Modifier.height(Dimens.paddingXLarge))
            }
        }
    }
}

@Composable
private fun ActionSheetRow(
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
