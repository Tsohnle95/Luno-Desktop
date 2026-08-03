package com.luno.mobile.data.repository

import com.luno.mobile.data.db.dao.DownloadJobDao
import com.luno.mobile.data.db.dao.PlaylistDao
import com.luno.mobile.data.db.dao.PlaylistWithTracks
import com.luno.mobile.data.db.dao.TrackDao
import com.luno.mobile.data.db.entity.DownloadJob
import com.luno.mobile.data.db.entity.Playlist
import com.luno.mobile.data.db.entity.Track
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.stateIn

/**
 * App-lifetime holder for the library's Room-backed flows, warmed eagerly
 * at startup.
 *
 * The screens render from these [StateFlow]s instead of collecting the
 * Room flows themselves.  Collecting the Room flows per screen meant every
 * tab switch re-ran the queries (the playlists-relations query loads
 * thousands of track rows) AFTER the screen faded in — chrome first, then
 * the data visibly popping in, with the loading spinner stuttering while
 * the heavy first composition ran.  Here the queries run once at app
 * start; by the time a screen is navigated to, its content is already
 * warm and renders in the same frame as the transition.
 *
 * [loaded] flips true once every flow has emitted its first value; screens
 * hold behind a spinner only until then (effectively just the very first
 * frames after app start).
 */
class LibraryData(
    appScope: CoroutineScope,
    trackDao: TrackDao,
    playlistDao: PlaylistDao,
    downloadJobDao: DownloadJobDao
) {
    private val tracksLoaded = MutableStateFlow(false)
    private val playlistsLoaded = MutableStateFlow(false)
    private val downloadsLoaded = MutableStateFlow(false)
    private val playlistsWithUrlsLoaded = MutableStateFlow(false)

    val tracks: StateFlow<List<Track>> = trackDao.getAllTracks()
        .onEach { tracksLoaded.value = true }
        .stateIn(appScope, SharingStarted.Eagerly, emptyList())

    val playlists: StateFlow<List<PlaylistWithTracks>> = playlistDao.getAllPlaylistsWithTracks()
        .onEach { playlistsLoaded.value = true }
        .stateIn(appScope, SharingStarted.Eagerly, emptyList())

    val downloads: StateFlow<List<DownloadJob>> = downloadJobDao.getAllDownloads()
        .onEach { downloadsLoaded.value = true }
        .stateIn(appScope, SharingStarted.Eagerly, emptyList())

    val playlistsWithUrls: StateFlow<List<Playlist>> = playlistDao.getPlaylistsWithUrls()
        .onEach { playlistsWithUrlsLoaded.value = true }
        .stateIn(appScope, SharingStarted.Eagerly, emptyList())

    /** True once every library flow has emitted its first value. */
    val loaded: StateFlow<Boolean> = combine(
        tracksLoaded,
        playlistsLoaded,
        downloadsLoaded,
        playlistsWithUrlsLoaded
    ) { tracksReady, playlistsReady, downloadsReady, urlsReady ->
        tracksReady && playlistsReady && downloadsReady && urlsReady
    }.stateIn(appScope, SharingStarted.Eagerly, false)
}
