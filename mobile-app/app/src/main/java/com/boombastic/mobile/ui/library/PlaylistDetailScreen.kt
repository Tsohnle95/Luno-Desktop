package com.boombastic.mobile.ui.library

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.boombastic.mobile.BoomBasticApp
import com.boombastic.mobile.data.db.dao.PlaylistWithTracks
import com.boombastic.mobile.data.db.entity.Track
import com.boombastic.mobile.playback.MediaTrack
import com.boombastic.mobile.playback.MusicController
import com.boombastic.mobile.ui.components.ArtworkCollage
import com.boombastic.mobile.ui.components.ArtworkImage
import com.boombastic.mobile.ui.components.TrackActionsSheet
import com.boombastic.mobile.ui.player.formatTime
import com.boombastic.mobile.ui.theme.AccentGreen
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText

/**
 * Playlist detail view (Spotify-inspired): a 2x2 collage of up to four
 * track artworks as the header art, playlist name, description, track
 * count + total duration, play-all button, and the track list.  Tapping a
 * track plays the playlist from that track; long-press opens track
 * actions (add to playlist / delete).
 */
@Composable
fun PlaylistDetailScreen(
    playlistId: Long,
    musicController: MusicController,
    onBack: () -> Unit
) {
    val context = LocalContext.current
    val app = context.applicationContext as BoomBasticApp

    // Track actions sheet — hoisted out of the LazyColumn: composing a
    // ModalBottomSheet inside a lazy item makes it scroll with the list.
    var actionsTrack by remember { mutableStateOf<Track?>(null) }

    // Reads from the app-warmed LibraryData playlists flow: the detail
    // screen is only reachable via a playlist card, so the list is already
    // loaded and the header + tracks render in the same frame as the
    // transition (membership changes flow in live too).
    val playlists by app.libraryData.playlists.collectAsState()
    val playlistWithTracks = playlists.firstOrNull { it.playlist.id == playlistId }

    // All-or-nothing first render: hold the screen behind one static
    // placeholder instead of showing the top bar and then the header and
    // track list popping in afterwards.
    if (playlistWithTracks == null) {
        Box(
            modifier = Modifier.fillMaxSize(),
            contentAlignment = Alignment.Center
        ) {
            CircularProgressIndicator(color = AccentGreen)
        }
        return
    }
    val playlist = playlistWithTracks.playlist
    val tracks = playlistWithTracks.tracks

    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(bottom = Dimens.paddingXLarge)
    ) {
        item {
            // Top bar
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = Dimens.paddingSmall),
                verticalAlignment = Alignment.CenterVertically
            ) {
                IconButton(onClick = onBack) {
                    Icon(
                        imageVector = Icons.AutoMirrored.Filled.ArrowBack,
                        contentDescription = "Back",
                        tint = PrimaryText,
                        modifier = Modifier.size(Dimens.iconSize)
                    )
                }
                Spacer(modifier = Modifier.weight(1f))
                Text(
                    text = "Playlist",
                    style = MaterialTheme.typography.titleMedium,
                    color = SecondaryText,
                    modifier = Modifier.padding(end = Dimens.paddingLarge)
                )
            }
        }

        if (tracks.isEmpty()) {
            // Header without tracks
            item {
                ArtworkCollage(
                    tracks = tracks,
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = Dimens.paddingLarge)
                )
            }
            item {
                PlaylistHeader(
                    name = playlist.name,
                    description = playlist.description,
                    trackCount = 0,
                    totalDurationMs = 0L,
                    onPlayAll = null
                )
            }
            item {
                Text(
                    text = "No tracks in this playlist yet. Long-press a song and choose \"Add to playlist\".",
                    color = SecondaryText,
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.padding(Dimens.paddingLarge)
                )
            }
        } else {
            // Header: 2x2 collage of the first four track artworks
            item {
                ArtworkCollage(
                    tracks = tracks,
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = Dimens.paddingLarge)
                )
            }

            item {
                PlaylistHeader(
                    name = playlist.name,
                    description = playlist.description,
                    trackCount = tracks.size,
                    totalDurationMs = tracks.sumOf { it.durationMs },
                    onPlayAll = {
                        musicController.play(tracks.map { it.toMediaTrack() }, 0)
                    }
                )
            }

            items(tracks, key = { it.uri }) { track ->
                PlaylistTrackRow(
                    track = track,
                    onClick = {
                        val index = tracks.indexOfFirst { it.uri == track.uri }
                        musicController.play(tracks.map { it.toMediaTrack() }, index.coerceAtLeast(0))
                    },
                    onLongPress = { actionsTrack = track }
                )
            }
        }
    }

    // Track actions sheet — outside the LazyColumn so it overlays the
    // list instead of scrolling with it.
    actionsTrack?.let { track ->
        TrackActionsSheet(
            track = track,
            onDismiss = { actionsTrack = null }
        )
    }
}

@Composable
private fun PlaylistHeader(
    name: String,
    description: String,
    trackCount: Int,
    totalDurationMs: Long,
    onPlayAll: (() -> Unit)?
) {
    Column(modifier = Modifier.padding(horizontal = Dimens.paddingLarge)) {
        Spacer(modifier = Modifier.height(Dimens.paddingLarge))
        Text(
            text = name,
            style = MaterialTheme.typography.headlineMedium,
            color = PrimaryText,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis
        )
        if (description.isNotBlank()) {
            Spacer(modifier = Modifier.height(Dimens.paddingSmall))
            Text(
                text = description,
                style = MaterialTheme.typography.bodyMedium,
                color = SecondaryText
            )
        }
        Spacer(modifier = Modifier.height(Dimens.paddingSmall))
        Text(
            text = "$trackCount songs · ${formatTime(totalDurationMs)}",
            style = MaterialTheme.typography.bodySmall,
            color = SecondaryText
        )
        Spacer(modifier = Modifier.height(Dimens.paddingLarge))

        if (onPlayAll != null) {
            Button(
                onClick = onPlayAll,
                colors = ButtonDefaults.buttonColors(containerColor = AccentGreen)
            ) {
                Icon(
                    imageVector = Icons.Filled.PlayArrow,
                    contentDescription = null,
                    modifier = Modifier.size(Dimens.iconSize)
                )
                Spacer(modifier = Modifier.width(Dimens.paddingSmall))
                Text("Play")
            }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun PlaylistTrackRow(
    track: Track,
    onClick: () -> Unit,
    onLongPress: () -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .combinedClickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = onClick,
                onLongClick = onLongPress
            )
            .padding(horizontal = Dimens.paddingLarge, vertical = Dimens.paddingSmall),
        verticalAlignment = Alignment.CenterVertically
    ) {
        ArtworkImage(
            artworkUri = track.albumArtUri(),
            modifier = Modifier
                .size(Dimens.albumArtSmall)
                .clip(RoundedCornerShape(Dimens.cornerSmall)),
            placeholderIconSize = 20.dp,
            decodeSizePx = 192
        )
        Spacer(modifier = Modifier.width(Dimens.paddingMedium))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = track.title,
                style = MaterialTheme.typography.titleSmall,
                color = PrimaryText,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
            Text(
                text = track.artist,
                style = MaterialTheme.typography.bodySmall,
                color = SecondaryText,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
        }
    }
}

private fun Track.toMediaTrack(): MediaTrack = MediaTrack(
    uri = uri,
    title = title,
    artist = artist,
    album = album,
    durationMs = durationMs,
    artworkUri = albumArtUri()
)
