package com.boombastic.mobile.ui.library

import android.widget.Toast
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.clickable
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
import androidx.compose.material.icons.filled.Clear
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.RadioButtonUnchecked
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Shuffle
import androidx.compose.material.icons.filled.UploadFile
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import com.boombastic.mobile.BoomBasticApp
import com.boombastic.mobile.data.db.dao.PlaylistWithTracks
import com.boombastic.mobile.data.db.entity.Track
import com.boombastic.mobile.playback.MediaTrack
import com.boombastic.mobile.playback.MusicController
import com.boombastic.mobile.ui.components.ArtworkCollage
import com.boombastic.mobile.ui.components.ArtworkImage
import com.boombastic.mobile.ui.components.BulkSelectionToolbar
import com.boombastic.mobile.ui.components.SortChip
import com.boombastic.mobile.ui.components.TrackSortMode
import com.boombastic.mobile.ui.components.sortedByMode
import com.boombastic.mobile.ui.player.formatTime
import com.boombastic.mobile.ui.theme.AccentGreen
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText
import com.boombastic.mobile.ui.theme.SurfaceDark
import com.boombastic.mobile.ui.theme.SurfaceElevated
import kotlinx.coroutines.launch

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
    onBack: () -> Unit,
    onExportPlaylist: (Long) -> Unit = {},
    onExportTracks: (List<String>) -> Unit = {},
    virtualName: String? = null,
    virtualDescription: String = "",
    virtualTracks: List<Track>? = null
) {
    val context = LocalContext.current
    val app = context.applicationContext as BoomBasticApp
    val scope = rememberCoroutineScope()

    // Sort mode (A–Z / Z–A / Recently added / Duration) — same chip as the Library
    // tab; declared before the early return so the saveable state's hook
    // order never changes.
    var sortMode by rememberSaveable { mutableStateOf(TrackSortMode.AZ) }

    // In-playlist search query — filters the track list (and therefore the
    // play context) by title/artist/album, Spotify style.
    var query by rememberSaveable { mutableStateOf("") }
    var showSearch by rememberSaveable { mutableStateOf(false) }
    var selectionMode by remember { mutableStateOf(false) }
    var selectedUris by remember { mutableStateOf(setOf<String>()) }

    // Reads from the app-warmed LibraryData playlists flow: the detail
    // screen is only reachable via a playlist card, so the list is already
    // loaded and the header + tracks render in the same frame as the
    // transition (membership changes flow in live too).
    val playlists by app.libraryData.playlists.collectAsState()
    val playlistWithTracks = playlists.firstOrNull { it.playlist.id == playlistId }
    val shuffleEnabled by musicController.shuffleEnabled.collectAsState()

    // All-or-nothing first render: hold the screen behind one static
    // placeholder instead of showing the top bar and then the header and
    // track list popping in afterwards.
    if (virtualTracks == null && playlistWithTracks == null) {
        Box(
            modifier = Modifier.fillMaxSize(),
            contentAlignment = Alignment.Center
        ) {
            CircularProgressIndicator(color = AccentGreen)
        }
        return
    }
    val playlist = playlistWithTracks?.playlist
    val tracks = virtualTracks ?: playlistWithTracks?.tracks.orEmpty()
    // Virtual playlists (for example Made for you) do not own persisted
    // playlist-view counts. Only a real playlist detail view supplies a
    // playlist playback context.
    val playbackPlaylistId = playlist?.id?.takeIf { virtualTracks == null }
    val playlistName = when {
        virtualName != null -> virtualName
        playlist?.name?.equals("Unsorted", ignoreCase = true) == true -> {
            "Unsorted - songs yet to find a home"
        }
        else -> playlist?.name.orEmpty()
    }
    val playlistDescription = virtualDescription.ifBlank { playlist?.description.orEmpty() }

    // In-playlist search: filter the membership by title/artist/album
    // (case-insensitive), then sort the filtered set — the list rows AND
    // the play context walk the same filtered order.
    val filteredTracks = remember(tracks, query) {
        if (query.isBlank()) tracks
        else tracks.filter {
            it.title.contains(query, ignoreCase = true) ||
                it.artist.contains(query, ignoreCase = true) ||
                it.album.contains(query, ignoreCase = true)
        }
    }
    val sortedTracks = remember(filteredTracks, sortMode) { filteredTracks.sortedByMode(sortMode) }
    val selectedTracks = tracks.filter { it.uri in selectedUris }

    fun toggleSelection(uri: String) {
        val next = if (uri in selectedUris) selectedUris - uri else selectedUris + uri
        selectedUris = next
        selectionMode = next.isNotEmpty()
    }

    fun beginSelection(uri: String) {
        selectionMode = true
        selectedUris = selectedUris + uri
    }

    fun exitSelection() {
        selectionMode = false
        selectedUris = emptySet()
    }

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
                if (virtualTracks == null && playlistId > 0L) {
                    IconButton(onClick = { onExportPlaylist(playlistId) }) {
                        Icon(
                            imageVector = Icons.Filled.UploadFile,
                            contentDescription = "Export playlist",
                            tint = AccentGreen,
                            modifier = Modifier.size(Dimens.iconSize)
                        )
                    }
                }
                Spacer(modifier = Modifier.weight(1f))
            }
        }

        if (selectionMode) {
            item {
                BulkSelectionToolbar(
                    selectedTracks = selectedTracks,
                    selectedPlaylists = emptyList(),
                    allSelected = tracks.isNotEmpty() && selectedUris.size == tracks.size,
                    onSelectAll = { selectAll ->
                        if (selectAll) {
                            selectionMode = true
                            selectedUris = tracks.map { it.uri }.toSet()
                        } else {
                            exitSelection()
                        }
                    },
                    onDismiss = ::exitSelection,
                    onAddToPlaylist = { target, trackUris ->
                        scope.launch {
                            app.playlistRepository.addTracksToPlaylist(target.id, trackUris)
                            Toast.makeText(context, "Added to ${target.name}", Toast.LENGTH_SHORT).show()
                        }
                        exitSelection()
                    },
                    onCreatePlaylist = { name, description, trackUris ->
                        scope.launch {
                            app.playlistRepository.createPlaylist(name, description).onSuccess { playlist ->
                                app.playlistRepository.addTracksToPlaylist(playlist.id, trackUris)
                                Toast.makeText(context, "Created ${playlist.name}", Toast.LENGTH_SHORT).show()
                            }
                        }
                        exitSelection()
                    },
                    onExportTracks = { uris ->
                        onExportTracks(uris)
                        exitSelection()
                    },
                    onRemoveTracks = { uris ->
                        scope.launch {
                            uris.forEach { app.playlistRepository.removeTrackFromPlaylist(playlistId, it) }
                        }
                        exitSelection()
                    },
                    modifier = Modifier.padding(horizontal = Dimens.paddingLarge)
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
                    name = playlistName,
                    description = playlistDescription,
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
                    name = playlistName,
                    description = playlistDescription,
                    trackCount = tracks.size,
                    totalDurationMs = tracks.sumOf { it.durationMs },
                    onPlayAll = {
                        musicController.play(
                            sortedTracks.map { it.toMediaTrack() },
                            0,
                            playbackPlaylistId
                        )
                    },
                    onShuffleAll = {
                        musicController.playShuffled(
                            sortedTracks.map { it.toMediaTrack() },
                            playbackPlaylistId
                        )
                    },
                    onSearch = { showSearch = !showSearch },
                    searchVisible = showSearch,
                    sortMode = sortMode,
                    onSortModeChange = { sortMode = it },
                    shuffleActive = shuffleEnabled
                )
            }

            if (showSearch) {
                item {
                    Column {
                        Spacer(modifier = Modifier.height(Dimens.paddingMedium))
                        OutlinedTextField(
                            value = query,
                            onValueChange = { query = it },
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(horizontal = Dimens.paddingLarge),
                            placeholder = {
                                Text(text = "Search in this playlist", color = SecondaryText)
                            },
                            leadingIcon = {
                                Icon(
                                    imageVector = Icons.Filled.Search,
                                    contentDescription = "Search",
                                    tint = SecondaryText
                                )
                            },
                            trailingIcon = {
                                if (query.isNotBlank()) {
                                    IconButton(onClick = { query = "" }) {
                                        Icon(
                                            imageVector = Icons.Filled.Clear,
                                            contentDescription = "Clear search",
                                            tint = SecondaryText
                                        )
                                    }
                                }
                            },
                            singleLine = true,
                            shape = RoundedCornerShape(24.dp),
                            colors = OutlinedTextFieldDefaults.colors(
                                focusedTextColor = PrimaryText,
                                unfocusedTextColor = PrimaryText,
                                cursorColor = AccentGreen,
                                focusedBorderColor = AccentGreen,
                                unfocusedBorderColor = SurfaceElevated,
                                focusedContainerColor = SurfaceDark,
                                unfocusedContainerColor = SurfaceDark
                            )
                        )
                        Spacer(modifier = Modifier.height(Dimens.paddingMedium))
                    }
                }
            }

            if (filteredTracks.isEmpty()) {
                item {
                    Text(
                        text = "No songs match \"$query\" in this playlist.",
                        color = SecondaryText,
                        style = MaterialTheme.typography.bodyMedium,
                        modifier = Modifier.padding(Dimens.paddingLarge)
                    )
                }
            } else {
                items(
                    sortedTracks,
                    key = { it.uri },
                    contentType = { "playlist-track" }
                ) { track ->
                    PlaylistTrackRow(
                        track = track,
                        selected = if (selectionMode) track.uri in selectedUris else null,
                        onClick = {
                            if (selectionMode) {
                                toggleSelection(track.uri)
                            } else {
                                val index = sortedTracks.indexOfFirst { it.uri == track.uri }
                                musicController.play(
                                    sortedTracks.map { it.toMediaTrack() },
                                    index.coerceAtLeast(0),
                                    playbackPlaylistId
                                )
                            }
                        },
                        onLongPress = { beginSelection(track.uri) }
                    )
                }
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
    onPlayAll: (() -> Unit)?,
    onShuffleAll: (() -> Unit)? = null,
    onSearch: (() -> Unit)? = null,
    searchVisible: Boolean = false,
    sortMode: TrackSortMode = TrackSortMode.AZ,
    onSortModeChange: ((TrackSortMode) -> Unit)? = null,
    shuffleActive: Boolean = false
) {
    Column(modifier = Modifier.padding(horizontal = Dimens.paddingLarge)) {
        Spacer(modifier = Modifier.height(Dimens.paddingLarge))
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                text = name,
                style = MaterialTheme.typography.headlineMedium,
                color = PrimaryText,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f)
            )
            if (onSearch != null) {
                IconButton(onClick = onSearch) {
                    Icon(
                        imageVector = Icons.Filled.Search,
                        contentDescription = if (searchVisible) {
                            "Hide playlist search"
                        } else {
                            "Search playlist"
                        },
                        tint = PrimaryText,
                        modifier = Modifier.size(Dimens.iconSize)
                    )
                }
            }
        }
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
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically
            ) {
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
                if (onShuffleAll != null) {
                    Spacer(modifier = Modifier.width(Dimens.paddingLarge))
                    // Shuffle — desktop All-Music style (icon + text,
                    // no box), plays the playlist shuffled.
                    Row(
                        modifier = Modifier
                            .clip(RoundedCornerShape(Dimens.cornerMedium))
                            .clickable(
                                interactionSource = remember { MutableInteractionSource() },
                                indication = null,
                                onClick = onShuffleAll
                            )
                            .padding(vertical = Dimens.paddingSmall),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Icon(
                            imageVector = Icons.Filled.Shuffle,
                            contentDescription = null,
                            tint = AccentGreen,
                            modifier = Modifier.size(Dimens.iconSize)
                        )
                        Spacer(modifier = Modifier.width(Dimens.paddingSmall))
                        Text(
                            text = "Shuffle",
                            style = MaterialTheme.typography.labelLarge,
                            color = AccentGreen,
                            fontWeight = if (shuffleActive) {
                                FontWeight.Bold
                            } else {
                                FontWeight.Medium
                            },
                            textDecoration = if (shuffleActive) {
                                TextDecoration.Underline
                            } else {
                                TextDecoration.None
                            }
                        )
                    }
                }

                Spacer(modifier = Modifier.weight(1f))

                // Filter menu stays opposite Play at the far right.
                if (onSortModeChange != null) {
                    SortChip(
                        mode = sortMode,
                        onModeChange = onSortModeChange,
                        compact = true
                    )
                }
            }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun PlaylistTrackRow(
    track: Track,
    selected: Boolean?,
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
