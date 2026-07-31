package com.boombastic.mobile.ui.home

import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
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
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.boombastic.mobile.BoomBasticApp
import com.boombastic.mobile.data.db.entity.Track
import com.boombastic.mobile.playback.MediaTrack
import com.boombastic.mobile.playback.MusicController
import com.boombastic.mobile.ui.components.ArtworkImage
import com.boombastic.mobile.ui.theme.AccentGreen
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * Spotify-inspired Home: greeting, edge-clipped "Recently played" and
 * "Made for you" carousels, and a quick-action playlist grid.  The
 * Settings drawer is opened from the tappable "Luno" app header instead
 * of a profile icon.
 */
@Composable
fun HomeScreen(
    musicController: MusicController,
    onPlay: (MediaTrack) -> Unit = {},
    onOpenPlaylist: (Long) -> Unit = {}
) {
    val context = LocalContext.current
    val app = context.applicationContext as BoomBasticApp
    val currentTrack by musicController.currentTrack.collectAsState()
    val recentlyPlayed by musicController.recentlyPlayed.collectAsState()
    val allTracks by app.libraryRepository.getAllTracks().collectAsState(initial = emptyList())
    val playlistsWithTracks by app.playlistRepository.getAllPlaylistsWithTracks()
        .collectAsState(initial = emptyList())

    val greeting = getGreeting()
    val displayName = "Listener" // Editable in future

    // Hoisted carousel scroll states: remembered at screen level so each
    // carousel keeps its exact scroll position even when its section
    // swaps content (e.g. EmptyStateCard <-> carousel while the library
    // changes) — with per-item remembers the state could reset or jump.
    val recentlyPlayedListState = rememberLazyListState()
    val madeForYouListState = rememberLazyListState()
    val playlistsListState = rememberLazyListState()

    // Edge-to-edge column; each section supplies its own horizontal padding
    // so carousels clip visibly at the screen edges.  Vertical rhythm is
    // standardized: 16dp above the greeting, then every section is broken
    // by a 24dp header gap with an 8dp header-to-content gap.
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(top = Dimens.paddingLarge, bottom = Dimens.paddingXLarge)
    ) {
        // Greeting header
        item(key = "greeting") {
            Column(modifier = Modifier.padding(horizontal = Dimens.paddingLarge)) {
                Text(
                    text = "Good $greeting",
                    style = MaterialTheme.typography.bodyLarge,
                    color = SecondaryText
                )
                Spacer(modifier = Modifier.height(Dimens.paddingSmall))
                Text(
                    text = displayName,
                    style = MaterialTheme.typography.headlineLarge,
                    color = PrimaryText
                )
            }
        }

        // Recently played — swipeable horizontal carousel of the full
        // in-session history, edge-clipped like "Made for you"
        item(key = "recently-header") {
            SectionHeader(title = "Recently played")
        }
        if (recentlyPlayed.isEmpty() && currentTrack == null) {
            item(key = "recently-empty") {
                EmptyStateCard(
                    title = "No tracks yet",
                    subtitle = "Import audio from the Search tab to get started",
                    modifier = Modifier.padding(horizontal = Dimens.paddingLarge)
                )
            }
        } else {
            item(key = "recently-carousel") {
                // distinctBy: a track may legitimately appear twice in
                // history (non-consecutive plays); duplicate keys would
                // make the LazyRow jump or throw.
                val history = recentlyPlayed.distinctBy { it.uri }.take(20)
                LazyRow(
                    state = recentlyPlayedListState,
                    horizontalArrangement = Arrangement.spacedBy(Dimens.paddingMedium),
                    contentPadding = PaddingValues(horizontal = Dimens.paddingLarge)
                ) {
                    items(history, key = { it.uri }) { track ->
                        TrackCard(
                            title = track.title,
                            artist = track.artist,
                            artworkUri = track.artworkUri,
                            onClick = { onPlay(track) }
                        )
                    }
                }
            }
        }

        // Made for you — library tracks until Last.fm recommendations land
        item(key = "made-header") {
            SectionHeader(title = "Made for you")
        }
        if (allTracks.isEmpty()) {
            item(key = "made-empty") {
                EmptyStateCard(
                    title = "Nothing here yet",
                    subtitle = "Songs from your library will appear here",
                    modifier = Modifier.padding(horizontal = Dimens.paddingLarge)
                )
            }
        } else {
            item(key = "made-carousel") {
                LazyRow(
                    state = madeForYouListState,
                    horizontalArrangement = Arrangement.spacedBy(Dimens.paddingMedium),
                    contentPadding = PaddingValues(horizontal = Dimens.paddingLarge)
                ) {
                    items(allTracks.take(10), key = { it.uri }) { track ->
                        TrackCard(
                            title = track.title,
                            artist = track.artist,
                            artworkUri = track.albumArtUri(),
                            onClick = {
                                onPlay(
                                    MediaTrack(
                                        uri = track.uri,
                                        title = track.title,
                                        artist = track.artist,
                                        album = track.album,
                                        durationMs = track.durationMs,
                                        artworkUri = track.albumArtUri()
                                    )
                                )
                            }
                        )
                    }
                }
            }
        }

        // Quick-action playlists — horizontal carousel (same layout as
        // "Made for you"), edge-clipped
        item(key = "playlists-header") {
            SectionHeader(title = "Your playlists")
        }
        if (playlistsWithTracks.isEmpty()) {
            item(key = "playlists-empty") {
                EmptyStateCard(
                    title = "No playlists yet",
                    subtitle = "Create one from Your Library",
                    modifier = Modifier.padding(horizontal = Dimens.paddingLarge)
                )
            }
        } else {
            item(key = "playlists-carousel") {
                LazyRow(
                    state = playlistsListState,
                    horizontalArrangement = Arrangement.spacedBy(Dimens.paddingMedium),
                    contentPadding = PaddingValues(horizontal = Dimens.paddingLarge)
                ) {
                    items(playlistsWithTracks, key = { it.playlist.id }) { playlistWithTracks ->
                        HomePlaylistCard(
                            name = playlistWithTracks.playlist.name,
                            tracks = playlistWithTracks.tracks,
                            onClick = { onOpenPlaylist(playlistWithTracks.playlist.id) }
                        )
                    }
                }
            }
        }
    }
}

/**
 * Home-only playlist card matching the "Made for you" TrackCard layout:
 * square artwork (first track's image — no collage on Home) on top, name
 * below, track count as the subtitle.
 */
@Composable
private fun HomePlaylistCard(
    name: String,
    tracks: List<Track>,
    onClick: () -> Unit
) {
    Column(
        modifier = Modifier
            .width(140.dp)
            .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = onClick
            )
    ) {
        ArtworkImage(
            artworkUri = tracks.firstOrNull()?.albumArtUri(),
            modifier = Modifier
                .size(Dimens.albumArtMedium)
                .clip(RoundedCornerShape(Dimens.cornerLarge)),
            placeholderIconSize = 40.dp,
            decodeSizePx = 384
        )
        Spacer(modifier = Modifier.height(Dimens.paddingSmall))
        Text(
            text = name,
            style = MaterialTheme.typography.titleSmall,
            color = PrimaryText,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis
        )
        Text(
            text = "${tracks.size} songs",
            style = MaterialTheme.typography.bodySmall,
            color = SecondaryText,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis
        )
    }
}

/**
 * Section title with the standardized rhythm used across Home: 24dp above
 * (break between sections), 8dp below (header-to-content gap).
 */
@Composable
fun SectionHeader(title: String) {
    Text(
        text = title,
        style = MaterialTheme.typography.titleMedium,
        color = PrimaryText,
        fontWeight = FontWeight.Bold,
        modifier = Modifier.padding(
            start = Dimens.paddingLarge,
            end = Dimens.paddingLarge,
            top = Dimens.paddingXLarge,
            bottom = Dimens.paddingSmall
        )
    )
}

@Composable
fun TrackCard(
    title: String,
    artist: String,
    artworkUri: String? = null,
    onClick: () -> Unit = {}
) {
    Column(
        modifier = Modifier
            .width(140.dp)
            .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = onClick
            )
    ) {
        // Album art (120dp rounded square), real artwork when available
        ArtworkImage(
            artworkUri = artworkUri,
            modifier = Modifier
                .size(Dimens.albumArtMedium)
                .clip(RoundedCornerShape(Dimens.cornerLarge)),
            placeholderIconSize = 40.dp,
            decodeSizePx = 384
        )
        Spacer(modifier = Modifier.height(Dimens.paddingSmall))
        Text(
            text = title,
            style = MaterialTheme.typography.titleSmall,
            color = PrimaryText,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis
        )
        Text(
            text = artist,
            style = MaterialTheme.typography.bodySmall,
            color = SecondaryText,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis
        )
    }
}

@Composable
fun EmptyStateCard(
    title: String,
    subtitle: String,
    modifier: Modifier = Modifier
) {
    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(vertical = Dimens.paddingMedium)
    ) {
        Text(
            text = title,
            style = MaterialTheme.typography.titleMedium,
            color = PrimaryText
        )
        Text(
            text = subtitle,
            style = MaterialTheme.typography.bodyMedium,
            color = SecondaryText,
            modifier = Modifier.padding(top = Dimens.paddingSmall)
        )
    }
}

private fun getGreeting(): String {
    val hour = SimpleDateFormat("HH", Locale.getDefault()).format(Date()).toIntOrNull() ?: 12
    return when (hour) {
        in 5..11 -> "morning"
        in 12..17 -> "afternoon"
        else -> "evening"
    }
}
