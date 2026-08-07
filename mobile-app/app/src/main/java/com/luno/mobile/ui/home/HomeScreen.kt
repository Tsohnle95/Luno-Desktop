package com.luno.mobile.ui.home

import android.widget.Toast
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.gestures.scrollBy
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
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.RadioButtonUnchecked
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.luno.mobile.LunoApp
import com.luno.mobile.data.db.dao.PlaylistWithTracks
import com.luno.mobile.data.db.entity.DownloadJob
import com.luno.mobile.data.db.entity.DownloadState
import com.luno.mobile.data.db.entity.Track
import com.luno.mobile.playback.MediaTrack
import com.luno.mobile.playback.MusicController
import com.luno.mobile.ui.components.ArtworkImage
import com.luno.mobile.ui.components.BulkSelectionToolbar
import com.luno.mobile.ui.components.MiniPlayerOverlayHeight
import com.luno.mobile.ui.components.TrackActionsSheet
import com.luno.mobile.ui.theme.AccentGreen
import com.luno.mobile.ui.theme.Dimens
import com.luno.mobile.ui.theme.PrimaryText
import com.luno.mobile.ui.theme.SecondaryText
import com.luno.mobile.ui.theme.SurfaceDark
import com.luno.mobile.ui.theme.SurfaceElevated
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.launch

private data class HomeContentSnapshot(
    val tracks: List<Track>,
    val playlists: List<PlaylistWithTracks>,
    val downloads: List<DownloadJob>,
    val recentlyPlayed: List<MediaTrack>,
    val hasCurrentTrack: Boolean
)

private data class HomeScrollAnchor(
    val key: Any,
    val offset: Int
)

/** Catalogue identity must not change when mutable playback metadata changes. */
internal fun homeCatalogueKey(tracks: List<Track>): List<String> =
    tracks.asSequence().map { it.uri }.sorted().toList()

/** Returns the newest completed downloads that still exist in the library. */
internal fun recentlyDownloadedTracks(
    downloads: List<DownloadJob>,
    tracksByUri: Map<String, Track>,
    limit: Int = 50
): List<Track> = downloads
    .asSequence()
    .filter { it.state == DownloadState.COMPLETED && it.localUri.isNotBlank() }
    .sortedWith(
        compareByDescending<DownloadJob> {
            it.completedAt.takeIf { time -> time > 0L } ?: it.addedAt
        }.thenByDescending { it.id }
    )
    .mapNotNull { tracksByUri[it.localUri] }
    .distinctBy { it.uri }
    .take(limit.coerceAtLeast(0))
    .toList()

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
    onPlay: (List<MediaTrack>, Int, Boolean) -> Unit = { _, _, _ -> },
    onOpenPlaylist: (Long) -> Unit = {},
    onOpenMadeForYou: () -> Unit = {}
) {
    val context = LocalContext.current
    val app = context.applicationContext as LunoApp
    val currentTrack by musicController.currentTrack.collectAsState()
    val recentlyPlayed by musicController.recentlyPlayed.collectAsState()
    // Rendered from the app-warmed LibraryData flows — switching back to
    // Home shows the full carousels in the same frame as the transition.
    val libraryData = app.libraryData
    val allTracks by libraryData.tracks.collectAsState()
    val playlistsWithTracks by libraryData.playlists.collectAsState()
    val downloads by libraryData.downloads.collectAsState()
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

    // Hoisted scroll states: keeping the parent state at screen level avoids
    // losing the vertical anchor when Home recomposes after a Room emission.
    val homeListState = rememberSaveable(saver = LazyListState.Saver) {
        LazyListState()
    }

    // Hoisted carousel scroll states: remembered at screen level so each
    // carousel keeps its exact scroll position even when its section
    // swaps content (e.g. EmptyStateCard <-> carousel while the library
    // changes) — with per-item remembers the state could reset or jump.
    val recentlyPlayedListState = rememberLazyListState()
    val recentlyDownloadedListState = rememberLazyListState()
    val madeForYouListState = rememberLazyListState()
    val favoritesListState = rememberLazyListState()
    val popularTracksListState = rememberLazyListState()
    val popularPlaylistsListState = rememberLazyListState()

    // Room emits a new list when a play count changes. Applying that update
    // during a fling can change section membership and move the content under
    // the user's finger. Keep one render snapshot until every Home list is
    // idle, then apply the newest library/history data in one update.
    val latestContent = rememberUpdatedState(
        HomeContentSnapshot(
            tracks = allTracks,
            playlists = playlistsWithTracks,
            downloads = downloads,
            recentlyPlayed = recentlyPlayed,
            hasCurrentTrack = currentTrack != null
        )
    )
    var renderedContent by remember { mutableStateOf(latestContent.value) }
    var anchorToRestore by remember { mutableStateOf<HomeScrollAnchor?>(null) }
    fun isHomeScrolling(): Boolean =
        homeListState.isScrollInProgress ||
            recentlyPlayedListState.isScrollInProgress ||
            recentlyDownloadedListState.isScrollInProgress ||
            madeForYouListState.isScrollInProgress ||
            favoritesListState.isScrollInProgress ||
            popularTracksListState.isScrollInProgress ||
            popularPlaylistsListState.isScrollInProgress

    // Room can emit while a fling is settling. Apply the newest snapshot only
    // while idle, and preserve the first visible keyed item if a section's
    // membership or height changed. This prevents a data refresh from moving
    // the content under the user's finger or appearing to reverse direction.
    fun applyLatestContent() {
        if (isHomeScrolling()) return
        val nextContent = latestContent.value
        if (renderedContent == nextContent) return
        anchorToRestore = homeListState.layoutInfo.visibleItemsInfo
            .firstOrNull()
            ?.let { HomeScrollAnchor(key = it.key, offset = it.offset) }
        renderedContent = nextContent
    }

    LaunchedEffect(allTracks, playlistsWithTracks, downloads, recentlyPlayed, currentTrack != null) {
        applyLatestContent()
    }
    LaunchedEffect(
        homeListState,
        recentlyPlayedListState,
        recentlyDownloadedListState,
        madeForYouListState,
        favoritesListState,
        popularTracksListState,
        popularPlaylistsListState
    ) {
        snapshotFlow { isHomeScrolling() }
            .distinctUntilChanged()
            .collect { isScrolling ->
                if (!isScrolling) {
                    // Wait for the idle layout to be committed before taking
                    // an anchor. A fling can report idle one frame before its
                    // final layout is visible to the parent composition.
                    withFrameNanos { }
                    applyLatestContent()
                }
            }
    }

    LaunchedEffect(renderedContent) {
        val anchor = anchorToRestore ?: return@LaunchedEffect
        withFrameNanos { }
        val updatedItem = homeListState.layoutInfo.visibleItemsInfo
            .firstOrNull { it.key == anchor.key }
        anchorToRestore = null
        // A new gesture may begin during the frame wait. Never issue a
        // corrective scroll once the user has taken control again.
        if (isHomeScrolling()) return@LaunchedEffect
        if (updatedItem != null) {
            val correction = updatedItem.offset - anchor.offset
            if (correction != 0) {
                homeListState.scrollBy(correction.toFloat())
            }
        }
    }

    val homeTracks = renderedContent.tracks
    val homePlaylists = renderedContent.playlists
    val homeDownloads = renderedContent.downloads
    val homeRecentlyPlayed = renderedContent.recentlyPlayed
    val homeHasCurrentTrack = renderedContent.hasCurrentTrack

    // Key the random sample only to catalogue membership. A play-count or
    // artwork update refreshes the card data without changing its order.
    val catalogueUris = remember(homeTracks) { homeCatalogueKey(homeTracks) }
    val madeForYouUris = remember(catalogueUris) {
        catalogueUris.shuffled().take(50)
    }
    val tracksByUri = remember(homeTracks) { homeTracks.associateBy { it.uri } }
    val recentlyDownloaded = remember(homeDownloads, tracksByUri) {
        recentlyDownloadedTracks(homeDownloads, tracksByUri)
    }
    val recentlyDownloadedMedia = remember(recentlyDownloaded) {
        recentlyDownloaded.map {
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
    val madeForYou = remember(madeForYouUris, homeTracks) {
        madeForYouUris.mapNotNull { tracksByUri[it] }
    }
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
    val popularTracks = remember(homeTracks) {
        homeTracks
            .filter { it.playCount > 0 }
            .sortedWith(
                compareByDescending<Track> { it.playCount }
                    .thenBy { it.title.lowercase() }
                    .thenBy { it.uri }
            )
            .take(20)
    }
    val favoriteTracks = remember(homeTracks) {
        homeTracks
            .filter { it.isFavorite }
            .sortedWith(
                compareByDescending<Track> { it.addedAt }
                    .thenBy { it.uri }
            )
            .take(20)
    }
    val favoriteTracksMedia = remember(favoriteTracks) {
        favoriteTracks.map {
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
    val popularPlaylists = remember(homePlaylists) {
        homePlaylists
            .filter { it.playlist.playCount > 0 }
            .sortedWith(
                compareByDescending<PlaylistWithTracks> { it.playlist.playCount }
                    .thenBy { it.playlist.id }
            )
            .take(10)
    }

    var selectionMode by remember { mutableStateOf(false) }
    var selectedKeys by remember { mutableStateOf(setOf<String>()) }
    var actionsTrack by remember { mutableStateOf<Track?>(null) }
    val selectedTracks = homeTracks.filter { it.uri in selectedKeys }
    val selectedPlaylists = homePlaylists.filter {
        "p${it.playlist.id}" in selectedKeys
    }
    val selectableKeys = remember(
        homeRecentlyPlayed,
        recentlyDownloaded,
        madeForYou,
        favoriteTracks,
        popularTracks,
        popularPlaylists
    ) {
        buildSet {
            addAll(homeRecentlyPlayed.map { it.uri })
            addAll(recentlyDownloaded.map { it.uri })
            addAll(madeForYou.map { it.uri })
            addAll(favoriteTracks.map { it.uri })
            addAll(popularTracks.map { it.uri })
            addAll(popularPlaylists.map { "p${it.playlist.id}" })
        }
    }
    val scope = rememberCoroutineScope()

    fun toggleSelection(key: String) {
        val next = if (key in selectedKeys) selectedKeys - key else selectedKeys + key
        selectedKeys = next
        selectionMode = next.isNotEmpty()
    }

    fun beginSelection(key: String) {
        selectionMode = true
        selectedKeys = selectedKeys + key
    }

    fun exitSelection() {
        selectionMode = false
        selectedKeys = emptySet()
    }

    // Edge-to-edge column; each section supplies its own horizontal padding
    // so carousels clip visibly at the screen edges.  Vertical rhythm is
    // standardized: 16dp above the greeting, then every section is broken
    // by a 24dp header gap with an 8dp header-to-content gap.
    LazyColumn(
        state = homeListState,
        modifier = Modifier.fillMaxSize(),
        // The shell overlays the mini-player on Home so its appearance never
        // changes the viewport while this list is being dragged. Keep a
        // permanent end inset so the last carousel remains reachable under
        // either shell state.
        contentPadding = PaddingValues(
            top = Dimens.paddingLarge,
            bottom = Dimens.paddingXLarge + MiniPlayerOverlayHeight
        )
    ) {
        // Greeting header
        item(key = "greeting", contentType = "hero") {
            HomeHero(
                greeting = greeting,
                displayName = displayName,
                trackCount = homeTracks.size,
                mixCount = madeForYou.size,
                onClick = if (homeTracks.isEmpty()) null else {
                    {
                        app.setMadeForYouTracks(madeForYou)
                        onOpenMadeForYou()
                    }
                }
            )
        }

        if (selectionMode) {
            item(key = "selection-toolbar", contentType = "toolbar") {
                BulkSelectionToolbar(
                    selectedTracks = selectedTracks,
                    selectedPlaylists = selectedPlaylists,
                    allSelected = selectableKeys.isNotEmpty() && selectableKeys.all { it in selectedKeys },
                    onSelectAll = { selectAll ->
                        if (selectAll) {
                            selectionMode = true
                            selectedKeys = selectableKeys
                        } else {
                            exitSelection()
                        }
                    },
                    onDismiss = ::exitSelection,
                    onAddToPlaylist = { playlist, trackUris ->
                        scope.launch {
                            app.playlistRepository.addTracksToPlaylist(playlist.id, trackUris)
                            Toast.makeText(context, "Added to ${playlist.name}", Toast.LENGTH_SHORT).show()
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
                    onRemoveTracks = { uris ->
                        scope.launch { uris.forEach { app.libraryRepository.deleteTrack(it) } }
                        exitSelection()
                    },
                    onDeletePlaylists = { ids ->
                        scope.launch { ids.forEach { app.playlistRepository.deletePlaylist(it) } }
                        exitSelection()
                    },
                    modifier = Modifier.padding(horizontal = Dimens.paddingLarge)
                )
            }
        }

        // Recently played — swipeable horizontal carousel of the full
        // persisted history, edge-clipped like "Made for you"
        item(key = "recently-header", contentType = "section-header") {
            SectionHeader(title = "Recently played")
        }
        if (homeRecentlyPlayed.isEmpty() && !homeHasCurrentTrack) {
            item(key = "recently-empty", contentType = "empty-state") {
                EmptyStateCard(
                    title = "No tracks yet",
                    subtitle = "Use Download to find music and get started",
                    modifier = Modifier.padding(horizontal = Dimens.paddingLarge)
                )
            }
        } else {
            item(key = "recently-carousel", contentType = "track-carousel") {
                // distinctBy: a track may legitimately appear twice in
                // history (non-consecutive plays); duplicate keys would
                // make the LazyRow jump or throw.
                val history = homeRecentlyPlayed.distinctBy { it.uri }.take(20)
                LazyRow(
                    state = recentlyPlayedListState,
                    horizontalArrangement = Arrangement.spacedBy(Dimens.paddingMedium),
                    contentPadding = PaddingValues(horizontal = Dimens.paddingLarge)
                ) {
                    items(
                        history,
                        key = { it.uri },
                        contentType = { "track-card" }
                    ) { track ->
                        TrackCard(
                            title = track.title,
                            artist = track.artist,
                            artworkUri = track.artworkUri,
                            // Next/prev walk the recent history (desktop
                            // "Recently Played" context).
                            selected = if (selectionMode) track.uri in selectedKeys else null,
                            onClick = {
                                if (selectionMode) toggleSelection(track.uri) else {
                                    val index = history.indexOfFirst { it.uri == track.uri }
                                    onPlay(history, index.coerceAtLeast(0), false)
                                }
                            },
                            onLongClick = { beginSelection(track.uri) }
                        )
                    }
                }
            }
        }

        // Recently downloaded — completed downloads in completion order,
        // capped at the latest 50 songs and kept edge-clipped like the other
        // Home carousels.
        item(key = "downloaded-header", contentType = "section-header") {
            SectionHeader(
                title = "Recently downloaded",
                supportingText = "Your latest 50 downloads"
            )
        }
        if (recentlyDownloaded.isEmpty()) {
            item(key = "downloaded-empty", contentType = "empty-state") {
                EmptyStateCard(
                    title = "No downloads yet",
                    subtitle = "Downloaded songs will appear here",
                    modifier = Modifier.padding(horizontal = Dimens.paddingLarge)
                )
            }
        } else {
            item(key = "downloaded-carousel", contentType = "track-carousel") {
                LazyRow(
                    state = recentlyDownloadedListState,
                    horizontalArrangement = Arrangement.spacedBy(Dimens.paddingMedium),
                    contentPadding = PaddingValues(horizontal = Dimens.paddingLarge)
                ) {
                    items(
                        recentlyDownloaded,
                        key = { it.uri },
                        contentType = { "track-card" }
                    ) { track ->
                        TrackCard(
                            title = track.title,
                            artist = track.artist,
                            artworkUri = track.albumArtUri(),
                            selected = if (selectionMode) track.uri in selectedKeys else null,
                            onClick = {
                                if (selectionMode) {
                                    toggleSelection(track.uri)
                                } else {
                                    val index = recentlyDownloaded.indexOfFirst { it.uri == track.uri }
                                    onPlay(recentlyDownloadedMedia, index.coerceAtLeast(0), false)
                                }
                            },
                            onLongClick = { beginSelection(track.uri) }
                        )
                    }
                }
            }
        }

        // Made for you — a random mix from the entire catalogue.
        item(key = "made-header", contentType = "section-header") {
            SectionHeader(
                title = "Made for you",
                supportingText = "A fresh mix from your library",
                onClick = if (homeTracks.isEmpty()) null else {
                    {
                        app.setMadeForYouTracks(madeForYou)
                        onOpenMadeForYou()
                    }
                }
            )
        }
        if (homeTracks.isEmpty()) {
            item(key = "made-empty", contentType = "empty-state") {
                EmptyStateCard(
                    title = "Nothing here yet",
                    subtitle = "Songs from your library will appear here",
                    modifier = Modifier.padding(horizontal = Dimens.paddingLarge)
                )
            }
        } else {
            item(key = "made-carousel", contentType = "track-carousel") {
                LazyRow(
                    state = madeForYouListState,
                    horizontalArrangement = Arrangement.spacedBy(Dimens.paddingMedium),
                    contentPadding = PaddingValues(horizontal = Dimens.paddingLarge)
                ) {
                    items(
                        madeForYou,
                        key = { it.uri },
                        contentType = { "track-card" }
                    ) { track ->
                        TrackCard(
                            title = track.title,
                            artist = track.artist,
                            artworkUri = track.albumArtUri(),
                            selected = if (selectionMode) track.uri in selectedKeys else null,
                            onClick = {
                                if (selectionMode) toggleSelection(track.uri) else {
                                    val index = madeForYou.indexOfFirst { it.uri == track.uri }
                                    onPlay(madeForYouMedia, index.coerceAtLeast(0), false)
                                }
                            },
                            onLongClick = { beginSelection(track.uri) }
                        )
                    }
                }
            }
        }

        // Favorites — songs explicitly marked by the user, kept ahead of the
        // popularity section while retaining the same edge-clipped carousel.
        item(key = "favorites-header", contentType = "section-header") {
            SectionHeader(
                title = "Favorites",
                supportingText = "Songs you marked as favorites"
            )
        }
        if (favoriteTracks.isNotEmpty()) {
            item(key = "favorites-carousel", contentType = "track-carousel") {
                LazyRow(
                    state = favoritesListState,
                    horizontalArrangement = Arrangement.spacedBy(Dimens.paddingMedium),
                    contentPadding = PaddingValues(horizontal = Dimens.paddingLarge)
                ) {
                    items(
                        favoriteTracks,
                        key = { it.uri },
                        contentType = { "track-card" }
                    ) { track ->
                        TrackCard(
                            title = track.title,
                            artist = track.artist,
                            artworkUri = track.albumArtUri(),
                            selected = if (selectionMode) track.uri in selectedKeys else null,
                            onClick = {
                                if (selectionMode) {
                                    toggleSelection(track.uri)
                                } else {
                                    val index = favoriteTracks.indexOfFirst { it.uri == track.uri }
                                    onPlay(favoriteTracksMedia, index.coerceAtLeast(0), false)
                                }
                            },
                            onLongClick = { beginSelection(track.uri) },
                            onMenuClick = { actionsTrack = track }
                        )
                    }
                }
            }
        }

        // Most popular — local play counts are persisted in Room. Songs and
        // playlists are kept in separate edge-clipped carousels.
        item(key = "popular-header", contentType = "section-header") {
            SectionHeader(
                title = "Most popular",
                supportingText = "Based on your local play counts"
            )
        }
        if (popularTracks.isEmpty() && popularPlaylists.isEmpty()) {
            item(key = "popular-empty", contentType = "empty-state") {
                EmptyStateCard(
                    title = "Nothing popular yet",
                    subtitle = "Play songs to build your local favorites",
                    modifier = Modifier.padding(horizontal = Dimens.paddingLarge)
                )
            }
        }
        if (popularTracks.isNotEmpty()) {
            item(key = "popular-songs-header", contentType = "section-header") {
                SectionHeader(title = "Popular songs", topPadding = 0.dp)
            }
            item(key = "popular-songs-carousel", contentType = "track-carousel") {
                LazyRow(
                    state = popularTracksListState,
                    horizontalArrangement = Arrangement.spacedBy(Dimens.paddingMedium),
                    contentPadding = PaddingValues(horizontal = Dimens.paddingLarge)
                ) {
                    items(
                        popularTracks,
                        key = { it.uri },
                        contentType = { "track-card" }
                    ) { track ->
                        TrackCard(
                            title = track.title,
                            artist = track.artist,
                            artworkUri = track.albumArtUri(),
                            selected = if (selectionMode) track.uri in selectedKeys else null,
                            onClick = {
                                if (selectionMode) toggleSelection(track.uri) else {
                                    val index = popularTracks.indexOfFirst { it.uri == track.uri }
                                    onPlay(popularTracksMedia, index.coerceAtLeast(0), false)
                                }
                            },
                            onLongClick = { beginSelection(track.uri) }
                        )
                    }
                }
            }
        }
        if (popularPlaylists.isNotEmpty()) {
            item(key = "popular-playlists-header", contentType = "section-header") {
                SectionHeader(title = "Popular playlists", topPadding = 0.dp)
            }
            item(key = "popular-playlists-carousel", contentType = "playlist-carousel") {
                LazyRow(
                    state = popularPlaylistsListState,
                    horizontalArrangement = Arrangement.spacedBy(Dimens.paddingMedium),
                    contentPadding = PaddingValues(horizontal = Dimens.paddingLarge)
                ) {
                    items(
                        popularPlaylists,
                        key = { it.playlist.id },
                        contentType = { "playlist-card" }
                    ) { playlistWithTracks ->
                        HomePlaylistCard(
                            name = playlistWithTracks.playlist.name,
                            tracks = playlistWithTracks.tracks,
                            subtitle = "${playlistWithTracks.playlist.playCount} plays",
                            selected = if (selectionMode) {
                                "p${playlistWithTracks.playlist.id}" in selectedKeys
                            } else {
                                null
                            },
                            onClick = {
                                val key = "p${playlistWithTracks.playlist.id}"
                                if (selectionMode) toggleSelection(key)
                                else onOpenPlaylist(playlistWithTracks.playlist.id)
                            },
                            onLongClick = {
                                beginSelection("p${playlistWithTracks.playlist.id}")
                            }
                        )
                    }
                }
            }
        }
    }

    actionsTrack?.let { track ->
        TrackActionsSheet(
            track = track,
            onDismiss = { actionsTrack = null }
        )
    }
}

/**
 * Home-only playlist card matching the "Made for you" TrackCard layout:
 * square artwork (first track's image — no collage on Home) on top, name
 * below, the popularity count as the subtitle.
 */
@Composable
@OptIn(ExperimentalFoundationApi::class)
private fun HomePlaylistCard(
    name: String,
    tracks: List<Track>,
    subtitle: String,
    onClick: () -> Unit,
    onLongClick: () -> Unit,
    selected: Boolean?
) {
    Column(
        modifier = Modifier
            .width(140.dp)
            .combinedClickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = onClick,
                onLongClick = onLongClick
            )
    ) {
        Box {
            ArtworkImage(
                artworkUri = tracks.firstOrNull()?.albumArtUri(),
                modifier = Modifier
                    .size(Dimens.albumArtMedium)
                    .clip(RoundedCornerShape(Dimens.cornerLarge)),
                placeholderIconSize = 40.dp,
                decodeSizePx = 384
            )
            if (selected != null) {
                Icon(
                    imageVector = if (selected) {
                        Icons.Filled.CheckCircle
                    } else {
                        Icons.Filled.RadioButtonUnchecked
                    },
                    contentDescription = if (selected) "Selected" else "Not selected",
                    tint = if (selected) AccentGreen else SecondaryText,
                    modifier = Modifier
                        .align(Alignment.TopEnd)
                        .padding(Dimens.paddingSmall)
                )
            }
        }
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
    onClick: (() -> Unit)? = null,
    topPadding: Dp = Dimens.paddingXLarge
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(
                start = Dimens.paddingLarge,
                end = Dimens.paddingLarge,
                top = topPadding,
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
@OptIn(ExperimentalFoundationApi::class)
fun TrackCard(
    title: String,
    artist: String,
    artworkUri: String? = null,
    onClick: () -> Unit = {},
    onLongClick: (() -> Unit)? = null,
    selected: Boolean? = null,
    onMenuClick: (() -> Unit)? = null
) {
    Column(
        modifier = Modifier
            .width(140.dp)
            .then(
                if (onLongClick != null) {
                    Modifier.combinedClickable(
                        interactionSource = remember { MutableInteractionSource() },
                        indication = null,
                        onClick = onClick,
                        onLongClick = onLongClick
                    )
                } else {
                    Modifier.clickable(
                        interactionSource = remember { MutableInteractionSource() },
                        indication = null,
                        onClick = onClick
                    )
                }
            )
    ) {
        Box {
            ArtworkImage(
                artworkUri = artworkUri,
                modifier = Modifier
                    .size(Dimens.albumArtMedium)
                    .clip(RoundedCornerShape(Dimens.cornerLarge)),
                placeholderIconSize = 40.dp,
                decodeSizePx = 384
            )
            if (selected != null) {
                Icon(
                    imageVector = if (selected) {
                        Icons.Filled.CheckCircle
                    } else {
                        Icons.Filled.RadioButtonUnchecked
                    },
                    contentDescription = if (selected) "Selected" else "Not selected",
                    tint = if (selected) AccentGreen else SecondaryText,
                    modifier = Modifier
                        .align(Alignment.TopEnd)
                        .padding(Dimens.paddingSmall)
                )
            }
            if (onMenuClick != null) {
                IconButton(
                    onClick = onMenuClick,
                    modifier = Modifier
                        .align(Alignment.BottomEnd)
                        .size(36.dp)
                ) {
                    Icon(
                        imageVector = Icons.Filled.MoreVert,
                        contentDescription = "Track options",
                        tint = AccentGreen
                    )
                }
            }
        }
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
