package com.boombastic.mobile.data.repository

import androidx.room.withTransaction
import com.boombastic.mobile.data.db.AppDatabase
import com.boombastic.mobile.data.db.entity.Playlist
import com.boombastic.mobile.data.db.entity.PlaylistTrack
import com.boombastic.mobile.data.db.entity.Track
import com.boombastic.mobile.data.export.ImportPreview
import com.boombastic.mobile.data.export.ImportResult
import com.boombastic.mobile.data.export.LibraryManifest
import com.boombastic.mobile.data.export.LibraryManifestCodec
import com.boombastic.mobile.data.export.ManifestPlaylist
import com.boombastic.mobile.data.export.ManifestScope
import com.boombastic.mobile.data.export.ManifestSource
import com.boombastic.mobile.data.export.ManifestTrack
import com.boombastic.mobile.playback.ExtractionResult
import com.boombastic.mobile.playback.WebSearchService
import java.security.MessageDigest
import java.time.Instant
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Builds and imports reference-only manifests without exposing Room paths. */
class LibraryTransferRepository(
    private val database: AppDatabase,
    private val downloadRepository: DownloadRepository
) {
    private val trackDao = database.trackDao()
    private val playlistDao = database.playlistDao()
    private val downloadJobDao = database.downloadJobDao()

    suspend fun buildFullLibraryManifest(): LibraryManifest = withContext(Dispatchers.IO) {
        buildManifest(ManifestScope.LIBRARY, null)
    }

    suspend fun buildSelectedTracksManifest(trackUris: Collection<String>): LibraryManifest =
        withContext(Dispatchers.IO) {
            buildManifest(ManifestScope.SELECTED_TRACKS, trackUris.toSet())
        }

    suspend fun buildPlaylistsManifest(playlistIds: Collection<Long>): LibraryManifest =
        withContext(Dispatchers.IO) {
            buildManifest(ManifestScope.PLAYLISTS, null, playlistIds.toSet())
        }

    fun encode(manifest: LibraryManifest): String = LibraryManifestCodec.encode(manifest)

    fun decodeAndValidate(input: String): LibraryManifest =
        LibraryManifestCodec.decodeAndValidate(input)

    suspend fun previewImport(input: String): ImportPreview = withContext(Dispatchers.IO) {
        val manifest = LibraryManifestCodec.decodeAndValidate(input)
        val existingTracks = trackDao.getAllTracksOnce()
        val existingPlaylists = playlistDao.getAllPlaylistsOnce()
        val matches = matchTracks(manifest, existingTracks)
        val missing = manifest.tracks.filterNot { matches.containsKey(it.ref) }
        val existingNames = existingPlaylists.map { it.name }
        val existingNameKeys = existingNames.map { it.normalizedPlaylistName() }.toSet()
        val importedNames = manifest.playlists.map { it.name }
        ImportPreview(
            manifest = manifest,
            matchedCount = matches.size,
            missingTracks = missing,
            ambiguousTracks = manifest.tracks.filter { it.ambiguityConfirmationRequired },
            newPlaylistNames = importedNames.filterNot { it.normalizedPlaylistName() in existingNameKeys },
            existingPlaylistNames = importedNames.filter { it.normalizedPlaylistName() in existingNameKeys }
        )
    }

    /**
     * Additive import. Existing memberships are kept, and missing YouTube
     * tracks are resolved through the same NewPipe/WorkManager pipeline used
     * by search. Metadata-only matches are only accepted after confirmation.
     */
    suspend fun importManifest(input: String, confirmAmbiguous: Boolean): ImportResult =
        withContext(Dispatchers.IO) {
            val manifest = LibraryManifestCodec.decodeAndValidate(input)
            val existingTracks = trackDao.getAllTracksOnce()
            val matches = matchTracks(manifest, existingTracks)
            val ambiguous = manifest.tracks.filter { it.ambiguityConfirmationRequired }
            if (ambiguous.isNotEmpty() && !confirmAmbiguous) {
                throw IllegalArgumentException(
                    "${ambiguous.size} imported track(s) need recording confirmation"
                )
            }

            val playlists = playlistDao.getAllPlaylistsOnce()
            val playlistByName = playlists
                .associateBy { it.name.normalizedPlaylistName() }
                .toMutableMap()
            val playlistIdsByRef = mutableMapOf<String, List<Long>>()
            var addedMemberships = 0

            database.withTransaction {
                for (playlist in manifest.playlists) {
                    val key = playlist.name.normalizedPlaylistName()
                    val target = playlistByName[key] ?: Playlist(
                        name = playlist.name,
                        description = playlist.description
                    ).let { created ->
                        val id = playlistDao.insertPlaylist(created)
                        playlistDao.getPlaylist(id) ?: created.copy(id = id)
                    }.also { playlistByName[key] = it }

                    var nextOrder = playlistDao.maxSortOrder(target.id) + 1
                    for (ref in playlist.trackRefs) {
                        val track = matches[ref]
                        if (track != null) {
                            playlistDao.addTrackToPlaylist(
                                PlaylistTrack(target.id, track.uri, nextOrder++)
                            )
                            addedMemberships++
                        }
                        playlistIdsByRef[ref] =
                            (playlistIdsByRef[ref].orEmpty() + target.id).distinct()
                    }
                }

                if (manifest.unassignedTrackRefs.isNotEmpty()) {
                    val key = UNSORTED_PLAYLIST_NAME.normalizedPlaylistName()
                    val target = playlistByName[key] ?: Playlist(name = UNSORTED_PLAYLIST_NAME).let {
                        val id = playlistDao.insertPlaylist(it)
                        playlistDao.getPlaylist(id) ?: it.copy(id = id)
                    }.also { playlistByName[key] = it }
                    var nextOrder = playlistDao.maxSortOrder(target.id) + 1
                    for (ref in manifest.unassignedTrackRefs) {
                        val track = matches[ref]
                        if (track != null) {
                            playlistDao.addTrackToPlaylist(
                                PlaylistTrack(target.id, track.uri, nextOrder++)
                            )
                            addedMemberships++
                        }
                        playlistIdsByRef[ref] =
                            (playlistIdsByRef[ref].orEmpty() + target.id).distinct()
                    }
                }
            }

            val unresolved = mutableListOf<ManifestTrack>()
            var queued = 0
            for (track in manifest.tracks) {
                if (matches.containsKey(track.ref)) continue
                val destinations = playlistIdsByRef[track.ref].orEmpty()
                val resolvedDestinations = if (destinations.isNotEmpty()) {
                    destinations
                } else {
                    val unsorted = playlistByName[UNSORTED_PLAYLIST_NAME.normalizedPlaylistName()]
                        ?: playlistDao.getPlaylistByName(UNSORTED_PLAYLIST_NAME)
                        ?: Playlist(name = UNSORTED_PLAYLIST_NAME).let {
                            val id = playlistDao.insertPlaylist(it)
                            playlistDao.getPlaylist(id) ?: it.copy(id = id)
                        }
                    listOf(unsorted.id)
                }
                val resolved = resolveTrack(track)
                if (resolved == null) {
                    unresolved += track
                    continue
                }
                downloadRepository.enqueueDownload(
                    sourceUrl = resolved.first,
                    title = track.title,
                    artist = track.artist,
                    playlistId = resolvedDestinations.firstOrNull(),
                    thumbnailUrl = resolved.second,
                    playlistIds = resolvedDestinations.drop(1)
                )
                queued++
            }

            ImportResult(
                matchedCount = matches.size,
                queuedDownloads = queued,
                addedMemberships = addedMemberships,
                unresolvedTracks = unresolved,
                ambiguousTracks = ambiguous
            )
        }

    private suspend fun buildManifest(
        scope: ManifestScope,
        selectedUris: Set<String>?,
        selectedPlaylistIds: Set<Long> = emptySet()
    ): LibraryManifest {
        val allTracks = trackDao.getAllTracksOnce()
        val tracksByUri = allTracks.associateBy { it.uri }
        val sourceByUri = sourceByTrackUri()
        val playlistRows = playlistDao.getAllPlaylistsOnce()
            .filterNot { it.name.equals("All Music", ignoreCase = true) }
            .filter { selectedPlaylistIds.isEmpty() || it.id in selectedPlaylistIds }
        val memberships = playlistDao.getAllPlaylistTracksOnce()
            .groupBy { it.playlistId }
        val includedUris = when {
            selectedUris != null -> selectedUris.filter { it in tracksByUri }.toSet()
            selectedPlaylistIds.isNotEmpty() -> playlistRows
                .flatMap { memberships[it.id].orEmpty() }
                .map { it.trackUri }
                .filter { it in tracksByUri }
                .toSet()
            else -> allTracks.map { it.uri }.toSet()
        }

        val refsByUri = mutableMapOf<String, String>()
        val usedRefs = mutableSetOf<String>()
        val manifestTracks = mutableListOf<ManifestTrack>()
        fun refFor(track: Track): String {
            return refsByUri.getOrPut(track.uri) {
                val source = sourceByUri[track.uri]
                val seed = source?.let { "source:${it.provider}:${it.id}" }
                    ?: "metadata:${track.artist}|${track.title}|${track.album}|${track.durationMs}"
                val base = (if (source != null) "src:" else "trk:") + sha256(seed).take(24)
                var candidate = base
                var suffix = 1
                while (!usedRefs.add(candidate)) {
                    suffix++
                    candidate = "$base-$suffix"
                }
                candidate
            }
        }

        // Preserve the ordered relation rows in each playlist, while track
        // metadata itself is emitted once and referenced by stable refs.
        val manifestPlaylists = if (scope == ManifestScope.SELECTED_TRACKS) {
            emptyList()
        } else {
            playlistRows.map { playlist ->
                ManifestPlaylist(
                    name = playlist.name,
                    description = playlist.description,
                    trackRefs = memberships[playlist.id].orEmpty()
                        .filter { it.trackUri in includedUris }
                        .sortedBy { it.sortOrder }
                        .mapNotNull { relation ->
                            tracksByUri[relation.trackUri]?.let { track ->
                                if (track.uri !in refsByUri) manifestTracks += track.toManifest(
                                    refFor(track),
                                    sourceByUri[track.uri]
                                )
                                refFor(track)
                            }
                        }
                )
            }
        }

        includedUris.sorted().forEach { uri ->
            val track = tracksByUri[uri] ?: return@forEach
            if (track.uri !in refsByUri) {
                manifestTracks += track.toManifest(refFor(track), sourceByUri[track.uri])
            }
        }
        val assigned = manifestPlaylists.flatMap { it.trackRefs }.toSet()
        val unassigned = if (scope == ManifestScope.SELECTED_TRACKS) {
            manifestTracks.map { it.ref }
        } else {
            manifestTracks.map { it.ref }.filterNot { it in assigned }
        }

        return LibraryManifest(
            scope = scope,
            createdAt = Instant.now().toString(),
            tracks = manifestTracks,
            playlists = manifestPlaylists,
            unassignedTrackRefs = unassigned
        )
    }

    private suspend fun sourceByTrackUri(): Map<String, ManifestSource> {
        return downloadJobDao.getCompletedDownloadsOnce().mapNotNull { job ->
            val sourceId = extractYouTubeId(job.thumbnailUrl) ?: extractYouTubeId(job.sourceUrl)
            if (sourceId == null || job.localUri.isBlank()) {
                null
            } else {
                job.localUri to ManifestSource("youtube", sourceId)
            }
        }.toMap()
    }

    private suspend fun matchTracks(
        manifest: LibraryManifest,
        existingTracks: List<Track>
    ): Map<String, Track> {
        return matchByMetadata(manifest, existingTracks, sourceByTrackUri())
    }

    private fun matchByMetadata(
        manifest: LibraryManifest,
        existingTracks: List<Track>,
        sourceByUri: Map<String, ManifestSource>
    ): Map<String, Track> {
        val bySource = existingTracks.mapNotNull { track ->
            sourceByUri[track.uri]?.let { source -> "${source.provider}:${source.id}" to track }
        }.toMap()
        val byMetadata = existingTracks.groupBy {
            normalizeForDuplicate("${it.artist}${it.title}")
        }
        return manifest.tracks.mapNotNull { imported ->
            val sourceMatch = imported.source?.let { bySource["${it.provider}:${it.id}"] }
            val metadataMatch = if (imported.source == null) {
                byMetadata[normalizeForDuplicate("${imported.artist}${imported.title}")]
                    ?.firstOrNull()
            } else {
                null
            }
            (sourceMatch ?: metadataMatch)?.let { imported.ref to it }
        }.toMap()
    }

    private suspend fun resolveTrack(track: ManifestTrack): Pair<String, String>? {
        track.source?.let { source ->
            if (source.provider != "youtube") return null
            return when (val result = withContext(Dispatchers.IO) {
                WebSearchService.getAudioStreamUrl(source.id)
            }) {
                is ExtractionResult.Success -> result.data.url to "https://i.ytimg.com/vi/${source.id}/hqdefault.jpg"
                is ExtractionResult.Error -> null
            }
        }
        val query = listOf(track.artist, track.title).filter { it.isNotBlank() }.joinToString(" ")
        if (query.isBlank()) return null
        return when (val result = withContext(Dispatchers.IO) {
            WebSearchService.searchYouTube(query, limit = 1)
        }) {
            is ExtractionResult.Success -> result.data.firstOrNull()?.let { found ->
                when (val audio = withContext(Dispatchers.IO) {
                    WebSearchService.getAudioStreamUrl(found.videoId)
                }) {
                    is ExtractionResult.Success -> audio.data.url to found.thumbnailUrl
                    is ExtractionResult.Error -> null
                }
            }
            is ExtractionResult.Error -> null
        }
    }

    private fun Track.toManifest(ref: String, source: ManifestSource?): ManifestTrack = ManifestTrack(
        ref = ref,
        title = title,
        artist = artist,
        album = album,
        durationMs = durationMs.coerceIn(0L, 86_400_000L),
        source = source,
        ambiguityConfirmationRequired = source == null
    )

    private fun String.normalizedPlaylistName(): String = trim().lowercase()

    private fun sha256(value: String): String = MessageDigest
        .getInstance("SHA-256")
        .digest(value.toByteArray(Charsets.UTF_8))
        .joinToString("") { byte -> "%02x".format(byte) }

    private fun extractYouTubeId(value: String): String? {
        val match = Regex("(?:/vi/|/embed/|v=|youtu\\.be/)([A-Za-z0-9_-]{6,})").find(value)
            ?: return null
        return match.groupValues[1].takeIf { it.length <= 32 }
    }

    companion object {
        private const val UNSORTED_PLAYLIST_NAME = "Unsorted"
    }
}
