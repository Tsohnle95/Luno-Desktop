package com.boombastic.mobile.ui.library

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
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
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
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
import com.boombastic.mobile.ui.components.ArtworkImage
import com.boombastic.mobile.ui.components.TrackActionsSheet
import com.boombastic.mobile.ui.player.formatTime
import com.boombastic.mobile.ui.theme.AccentGreen
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText
import com.boombastic.mobile.ui.theme.SurfaceDark

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

    var playlistWithTracks by remember { mutableStateOf<PlaylistWithTracks?>(null) }
    LaunchedEffect(playlistId) {
        playlistWithTracks = app.playlistRepository.getPlaylistWithTracks(playlistId)
    }

    val data = playlistWithTracks
    val playlist = data?.playlist
    val tracks = data?.tracks ?: emptyList()

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

        if (playlist == null) {
            item {
                Text(
                    text = "Loading...",
                    color = SecondaryText,
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.padding(Dimens.paddingLarge)
                )
            }
        } else if (tracks.isEmpty()) {
            // Header without tracks
            item {
                PlaylistCollage(
                    tracks = tracks,
                    modifier = Modifier.padding(horizontal = Dimens.paddingLarge)
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
                PlaylistCollage(
                    tracks = tracks,
                    modifier = Modifier.padding(horizontal = Dimens.paddingLarge)
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
                    }
                )
            }
        }
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

/** 2x2 square collage of up to four track artworks (Spotify playlist header). */
@Composable
private fun PlaylistCollage(
    tracks: List<Track>,
    modifier: Modifier = Modifier
) {
    val cells = tracks.take(4)

    Column(
        modifier = modifier
            .fillMaxWidth()
            .aspectRatio(1f)
            .clip(RoundedCornerShape(Dimens.cornerLarge))
            .background(SurfaceDark)
    ) {
        for (row in 0 until 2) {
            Row(modifier = Modifier.weight(1f)) {
                for (col in 0 until 2) {
                    val index = row * 2 + col
                    val track = cells.getOrNull(index)
                    Box(
                        modifier = Modifier
                            .weight(1f)
                            .fillMaxSize(),
                        contentAlignment = Alignment.Center
                    ) {
                        ArtworkImage(
                            artworkUri = track?.albumArtUri(),
                            modifier = Modifier.fillMaxSize(),
                            placeholderIconSize = 36.dp
                        )
                    }
                }
            }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun PlaylistTrackRow(
    track: Track,
    onClick: () -> Unit
) {
    var showActions by remember { mutableStateOf(false) }

    Row(
        modifier = Modifier
            .fillMaxWidth()
            .combinedClickable(
                onClick = onClick,
                onLongClick = { showActions = true }
            )
            .padding(horizontal = Dimens.paddingLarge, vertical = Dimens.paddingSmall),
        verticalAlignment = Alignment.CenterVertically
    ) {
        ArtworkImage(
            artworkUri = track.albumArtUri(),
            modifier = Modifier
                .size(Dimens.albumArtSmall)
                .clip(RoundedCornerShape(Dimens.cornerSmall)),
            placeholderIconSize = 20.dp
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

    if (showActions) {
        TrackActionsSheet(
            track = track,
            onDismiss = { showActions = false }
        )
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
