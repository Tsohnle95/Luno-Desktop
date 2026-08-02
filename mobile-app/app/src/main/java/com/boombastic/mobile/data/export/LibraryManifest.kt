package com.boombastic.mobile.data.export

/** Stable wire identifiers shared by the desktop and Android transfer flows. */
const val LIBRARY_MANIFEST_FORMAT = "boombastic.library.export"
const val LIBRARY_MANIFEST_VERSION = 1

enum class ManifestScope(val wireName: String) {
    SELECTED_TRACKS("selected_tracks"),
    PLAYLISTS("playlists"),
    LIBRARY("library")
}

data class ManifestSource(
    val provider: String,
    val id: String
)

data class ManifestTrack(
    val ref: String,
    val title: String,
    val artist: String,
    val album: String,
    val durationMs: Long,
    val source: ManifestSource?,
    val ambiguityConfirmationRequired: Boolean
)

data class ManifestPlaylist(
    val name: String,
    val description: String,
    val trackRefs: List<String>
)

data class LibraryManifest(
    val scope: ManifestScope,
    val createdAt: String,
    val tracks: List<ManifestTrack>,
    val playlists: List<ManifestPlaylist>,
    val unassignedTrackRefs: List<String>
)

data class ImportPreview(
    val manifest: LibraryManifest,
    val matchedCount: Int,
    val missingTracks: List<ManifestTrack>,
    val ambiguousTracks: List<ManifestTrack>,
    val newPlaylistNames: List<String>,
    val existingPlaylistNames: List<String>
)

data class ImportResult(
    val matchedCount: Int,
    val queuedDownloads: Int,
    val addedMemberships: Int,
    val unresolvedTracks: List<ManifestTrack>,
    val ambiguousTracks: List<ManifestTrack>
)
