package com.boombastic.mobile.ui.home

import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.background
import androidx.compose.foundation.border
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
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
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
import com.boombastic.mobile.ui.theme.SurfaceDark
import com.boombastic.mobile.ui.theme.SurfaceElevated
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * Spotify-inspired Home: greeting, edge-clipped "Recently played" and
 * randomized 50-song "Made for you" carousel, and local "Most popular" song
 * and playlist carousels. The Settings drawer is opened from the tappable
 * "Luno" app header instead
 * of a profile icon.
 */
@Composable
fun HomeScreen(
    musicController: MusicController,
    onPlay: (List<MediaTrack>, Int) -> Unit = { _, _ -> },
    onOpenPlaylist: (Long) -> Unit = {},
    onOpenMadeForYou: () -> Unit = {}
) {
    val context = LocalContext.current
    val app = context.applicationContext as BoomBasticApp
    val currentTrack by musicController.currentTrack.collectAsState()
    val recentlyPlayed by musicController.recentlyPlayed.collectAsState()
    // Rendered from the app-warmed LibraryData flows — switching back to
    // Home shows the full carousels in the same frame as the transition.
    val libraryData = app.libraryData
    val allTracks by libraryData.tracks.collectAsState()
    val playlistsWithTracks by libraryData.playlists.collectAsState()
    val libraryLoaded by libraryData.loaded.collectAsState()

    // All-or-nothing first render (same as the other tabs): normally the
    // library data is already warm from startup, so this only shows on the
    // very first app frames.
    if (!libraryLoaded) {
        Box(
            modifier = Modifier.fillMaxSize(),
            contentAlignment = Alignment.Center
        ) {
            CircularProgressIndicator(color = AccentGreen)
        }
        return
    }

    val greeting = getGreeting()
    val displayName = "Listener" // Editable in future

    // Hoisted carousel scroll states: remembered at screen level so each
    // carousel keeps its exact scroll position even when its section
    // swaps content (e.g. EmptyStateCard <-> carousel while the library
    // changes) — with per-item remembers the state could reset or jump.
    val recentlyPlayedListState = rememberLazyListState()
    val madeForYouListState = rememberLazyListState()
    val popularTracksListState = rememberLazyListState()
    val popularPlaylistsListState = rememberLazyListState()

    // Keep one random catalogue sample stable for the lifetime of the
    // current library snapshot. This avoids reshuffling while the screen
    // recomposes, while still refreshing when the catalogue changes.
    val madeForYou = remember(allTracks) { allTracks.shuffled().take(50) }
    val madeForYouMedia = remember(madeForYou) {
        madeForYou.map {
            MediaTrack(
                uri = it.uri,
                title = it.title,
                artist = it.artist,
                album = it.album,
                durationMs = it.durationMs,
                artworkUri = it.albumArtUri()
            )
        }
    }
    val popularTracks = remember(allTracks) {
        allTracks
            .filter { it.playCount > 0 }
            .sortedWith(
                compareByDescending<Track> { it.playCount }
                    .thenBy { it.title.lowercase() }
            )
            .take(20)
    }
    val popularTracksMedia = remember(popularTracks) {
        popularTracks.map {
            MediaTrack(
                uri = it.uri,
                title = it.title,
                artist = it.artist,
                album = it.album,
                durationMs = it.durationMs,
                artworkUri = it.albumArtUri()
            )
        }
    }
    val popularPlaylists = remember(playlistsWithTracks) {
        playlistsWithTracks
            .map { playlistWithTracks ->
                playlistWithTracks to playlistWithTracks.tracks.sumOf { it.playCount }
            }
            .filter { (_, playCount) -> playCount > 0 }
            .sortedByDescending { it.second }
            .map { it.first }
            .take(10)
    }

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
            HomeHero(
                greeting = greeting,
                displayName = displayName,
                trackCount = allTracks.size,
                mixCount = madeForYou.size,
                onClick = if (allTracks.isEmpty()) null else {
                    {
                        app.setMadeForYouTracks(madeForYou)
                        onOpenMadeForYou()
                    }
                }
            )
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
                    subtitle = "Use Download to find music and get started",
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
                            // Next/prev walk the recent history (desktop
                            // "Recently Played" context).
                            onClick = {
                                val index = history.indexOfFirst { it.uri == track.uri }
                                onPlay(history, index.coerceAtLeast(0))
                            }
                        )
                    }
                }
            }
        }

        // Made for you — a random mix from the entire catalogue.
        item(key = "made-header") {
            SectionHeader(
                title = "Made for you",
                supportingText = "A fresh mix from your library",
                onClick = if (allTracks.isEmpty()) null else {
                    {
                        app.setMadeForYouTracks(madeForYou)
                        onOpenMadeForYou()
                    }
                }
            )
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
                    items(madeForYou, key = { it.uri }) { track ->
                        TrackCard(
                            title = track.title,
                            artist = track.artist,
                            artworkUri = track.albumArtUri(),
                            onClick = {
                                val index = madeForYou.indexOfFirst { it.uri == track.uri }
                                onPlay(madeForYouMedia, index.coerceAtLeast(0))
                            }
                        )
                    }
                }
            }
        }

        // Most popular — local play counts are persisted in Room. Songs and
        // playlists are kept in separate edge-clipped carousels.
        item(key = "popular-header") {
            SectionHeader(
                title = "Most popular",
                supportingText = "Based on your local play counts"
            )
        }
        if (popularTracks.isEmpty() && popularPlaylists.isEmpty()) {
            item(key = "popular-empty") {
                EmptyStateCard(
                    title = "Nothing popular yet",
                    subtitle = "Play songs to build your local favorites",
                    modifier = Modifier.padding(horizontal = Dimens.paddingLarge)
                )
            }
        }
        if (popularTracks.isNotEmpty()) {
            item(key = "popular-songs-header") {
                SectionHeader(title = "Popular songs")
            }
            item(key = "popular-songs-carousel") {
                LazyRow(
                    state = popularTracksListState,
                    horizontalArrangement = Arrangement.spacedBy(Dimens.paddingMedium),
                    contentPadding = PaddingValues(horizontal = Dimens.paddingLarge)
                ) {
                    items(popularTracks, key = { it.uri }) { track ->
                        TrackCard(
                            title = track.title,
                            artist = track.artist,
                            artworkUri = track.albumArtUri(),
                            onClick = {
                                val index = popularTracks.indexOfFirst { it.uri == track.uri }
                                onPlay(popularTracksMedia, index.coerceAtLeast(0))
                            }
                        )
                    }
                }
            }
        }
        if (popularPlaylists.isNotEmpty()) {
            item(key = "popular-playlists-header") {
                SectionHeader(title = "Popular playlists")
            }
            item(key = "popular-playlists-carousel") {
                LazyRow(
                    state = popularPlaylistsListState,
                    horizontalArrangement = Arrangement.spacedBy(Dimens.paddingMedium),
                    contentPadding = PaddingValues(horizontal = Dimens.paddingLarge)
                ) {
                    items(popularPlaylists, key = { it.playlist.id }) { playlistWithTracks ->
                        HomePlaylistCard(
                            name = playlistWithTracks.playlist.name,
                            tracks = playlistWithTracks.tracks,
                            subtitle = "${playlistWithTracks.tracks.sumOf { it.playCount }} plays",
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
 * below, the popularity count as the subtitle.
 */
@Composable
private fun HomePlaylistCard(
    name: String,
    tracks: List<Track>,
    subtitle: String,
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
            text = subtitle,
            style = MaterialTheme.typography.bodySmall,
            color = SecondaryText,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis
        )
    }
}

@Composable
private fun HomeHero(
    greeting: String,
    displayName: String,
    trackCount: Int,
    mixCount: Int,
    onClick: (() -> Unit)?
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = Dimens.paddingLarge)
            .clip(RoundedCornerShape(24.dp))
            .background(
                Brush.linearGradient(
                    listOf(
                        SurfaceElevated,
                        SurfaceDark,
                        Color(0xFF102B1C)
                    )
                )
            )
            .border(
                width = 1.dp,
                color = AccentGreen.copy(alpha = 0.2f),
                shape = RoundedCornerShape(24.dp)
            )
            .padding(Dimens.paddingLarge)
            .then(
                if (onClick != null) {
                    Modifier.clickable(
                        interactionSource = remember { MutableInteractionSource() },
                        indication = null,
                        onClick = onClick
                    )
                } else {
                    Modifier
                }
            ),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Column(
            modifier = Modifier
                .weight(1f)
                .widthIn(max = 280.dp)
        ) {
            Text(
                text = "Good $greeting,",
                style = MaterialTheme.typography.bodyMedium,
                color = AccentGreen
            )
            Text(
                text = displayName,
                style = MaterialTheme.typography.headlineMedium,
                color = PrimaryText,
                modifier = Modifier.padding(top = 2.dp)
            )
            Text(
                text = if (trackCount == 0) {
                    "Your next favorite is waiting."
                } else {
                    "Your next favorite is waiting in the mix."
                },
                style = MaterialTheme.typography.bodySmall,
                color = SecondaryText,
                modifier = Modifier.padding(top = Dimens.paddingSmall)
            )
        }
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            modifier = Modifier.padding(start = Dimens.paddingMedium)
        ) {
            Text(
                text = mixCount.toString(),
                style = MaterialTheme.typography.headlineLarge,
                color = AccentGreen,
                fontWeight = FontWeight.Bold
            )
            Text(
                text = "TRACK MIX",
                style = MaterialTheme.typography.labelSmall,
                color = SecondaryText
            )
        }
    }
}

/**
 * Section title with the standardized rhythm used across Home: 24dp above
 * (break between sections), 8dp below (header-to-content gap).
 */
@Composable
fun SectionHeader(
    title: String,
    supportingText: String? = null,
    onClick: (() -> Unit)? = null
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(
                start = Dimens.paddingLarge,
                end = Dimens.paddingLarge,
                top = Dimens.paddingXLarge,
                bottom = Dimens.paddingSmall
            ),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Column(
            modifier = Modifier
                .weight(1f)
                .then(
                    if (onClick != null) {
                        Modifier.clickable(
                            interactionSource = remember { MutableInteractionSource() },
                            indication = null,
                            onClick = onClick
                        )
                    } else {
                        Modifier
                    }
                )
        ) {
            Text(
                text = title,
                style = MaterialTheme.typography.titleMedium,
                color = PrimaryText,
                fontWeight = FontWeight.Bold
            )
            supportingText?.let {
                Text(
                    text = it,
                    style = MaterialTheme.typography.bodySmall,
                    color = SecondaryText,
                    modifier = Modifier.padding(top = 2.dp)
                )
            }
        }
        if (onClick != null) {
            Text(
                text = "View all",
                style = MaterialTheme.typography.labelLarge,
                color = AccentGreen,
                modifier = Modifier.clickable(
                    interactionSource = remember { MutableInteractionSource() },
                    indication = null,
                    onClick = onClick
                )
            )
        }
    }
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
