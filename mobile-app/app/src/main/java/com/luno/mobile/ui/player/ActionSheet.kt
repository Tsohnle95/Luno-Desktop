package com.luno.mobile.ui.player

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
import androidx.compose.material.icons.automirrored.filled.PlaylistAdd
import androidx.compose.material.icons.automirrored.filled.QueueMusic
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Person
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
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.luno.mobile.LunoApp
import com.luno.mobile.playback.MediaTrack
import com.luno.mobile.playback.MusicController
import com.luno.mobile.playback.RecommendationSaveOutcome
import com.luno.mobile.ui.components.EditTrackMetadataDialog
import com.luno.mobile.ui.components.PlaylistPickerSheet
import com.luno.mobile.ui.create.CreatePlaylistSheet
import com.luno.mobile.ui.theme.Dimens
import com.luno.mobile.ui.theme.PrimaryText
import com.luno.mobile.ui.theme.SecondaryText
import com.luno.mobile.ui.theme.SurfaceDark
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

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
    val app = context.applicationContext as LunoApp
    val currentTrack by musicController.currentTrack.collectAsState()
    val allTracks by app.libraryData.tracks.collectAsState()
    val editableTrack = currentTrack?.uri?.let { uri ->
        allTracks.firstOrNull { it.uri == uri }
    }
    val scope = rememberCoroutineScope()
    var showPlaylistPicker by rememberSaveable { mutableStateOf(false) }
    var showCreatePlaylist by rememberSaveable { mutableStateOf(false) }
    var showMetadataDialog by rememberSaveable { mutableStateOf(false) }
    var playlistTargetUri by rememberSaveable { mutableStateOf<String?>(null) }
    var playlistTargetTitle by rememberSaveable { mutableStateOf<String?>(null) }
    var playlistTargetIsTransient by rememberSaveable { mutableStateOf(false) }
    var playlistTargetTrack by remember { mutableStateOf<MediaTrack?>(null) }

    fun addCurrentTrackToPlaylist(playlistId: Long, playlistName: String) {
        val targetUri = playlistTargetUri ?: return
        val targetTitle = playlistTargetTitle ?: "Track"
        val targetIsTransient = playlistTargetIsTransient
        val targetTrack = playlistTargetTrack ?: MediaTrack(
            uri = targetUri,
            title = targetTitle
        )
        app.appScope.launch {
            val result = if (targetIsTransient) {
                app.recommendationPreviewManager.saveCurrentToPlaylist(playlistId, targetUri)
            } else {
                app.playlistRepository.addMediaTrackToPlaylist(playlistId, targetTrack)
                    .map { RecommendationSaveOutcome.AddedToPlaylist(targetUri) }
            }
            withContext(Dispatchers.Main.immediate) {
                Toast.makeText(
                    context.applicationContext,
                    result.fold(
                        onSuccess = { outcome ->
                            app.rememberRecentlySavedPlaylist(playlistId)
                            when (outcome) {
                                is RecommendationSaveOutcome.Queued ->
                                    "Download queued for $playlistName: $targetTitle"
                                is RecommendationSaveOutcome.Saved,
                                is RecommendationSaveOutcome.AddedToPlaylist ->
                                    "Added $targetTitle to $playlistName"
                            }
                        },
                        onFailure = {
                            "Could not add $targetTitle: ${it.message ?: "Try again"}"
                        }
                    ),
                    Toast.LENGTH_SHORT
                ).show()
            }
        }
        onDismiss()
    }

    if (showMetadataDialog && editableTrack != null) {
        EditTrackMetadataDialog(
            track = editableTrack,
            onDismiss = { showMetadataDialog = false },
            onSave = { title, artist ->
                scope.launch {
                    app.libraryRepository.updateTrackMetadata(
                        uri = editableTrack.uri,
                        title = title,
                        artist = artist
                    ).onSuccess {
                        musicController.updateTrackMetadata(editableTrack.uri, title, artist)
                        showMetadataDialog = false
                        Toast.makeText(
                            context,
                            "Song details updated",
                            Toast.LENGTH_SHORT
                        ).show()
                    }.onFailure { error ->
                        Toast.makeText(
                            context,
                            "Could not update song: ${error.message ?: "Try again"}",
                            Toast.LENGTH_LONG
                        ).show()
                    }
                }
            }
        )
    } else if (showCreatePlaylist) {
        CreatePlaylistSheet(
            onDismiss = { showCreatePlaylist = false },
            onCreated = { playlist ->
                showCreatePlaylist = false
                addCurrentTrackToPlaylist(playlist.id, playlist.name)
            }
        )
    } else if (showPlaylistPicker) {
        PlaylistPickerSheet(
            onPick = { playlist ->
                showPlaylistPicker = false
                addCurrentTrackToPlaylist(playlist.id, playlist.name)
            },
            onDismiss = { showPlaylistPicker = false },
            onCreateNew = {
                showPlaylistPicker = false
                showCreatePlaylist = true
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
                ActionSheetRow(
                    icon = Icons.AutoMirrored.Filled.PlaylistAdd,
                    label = "Add to playlist",
                    onClick = {
                        currentTrack?.let { track ->
                            playlistTargetUri = track.uri
                            playlistTargetTitle = track.title
                            playlistTargetIsTransient = track.isTransient
                            playlistTargetTrack = track
                            showPlaylistPicker = true
                        }
                    }
                )
                if (editableTrack != null) {
                    ActionSheetRow(
                        icon = Icons.Filled.Edit,
                        label = "Edit title and artist",
                        onClick = { showMetadataDialog = true }
                    )
                }
                ActionSheetRow(
                    icon = Icons.Filled.SkipNext,
                    label = "Play next",
                    onClick = {
                        currentTrack?.let { musicController.playNext(it) }
                        onDismiss()
                    }
                )
                ActionSheetRow(
                    icon = Icons.AutoMirrored.Filled.QueueMusic,
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
