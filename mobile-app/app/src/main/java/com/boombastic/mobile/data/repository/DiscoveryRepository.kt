package com.boombastic.mobile.data.repository

import android.content.Context
import com.boombastic.mobile.data.db.entity.Track
import com.boombastic.mobile.data.discovery.LastfmKeyStore
import com.boombastic.mobile.data.discovery.LastfmResult
import com.boombastic.mobile.data.discovery.LastfmService
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Single entry point for Last.fm discovery: owns the encrypted API key
 * (exposed as a [StateFlow] so the UI reacts to drawer changes) and
 * delegates fetches to [LastfmService], cross-referencing recommendations
 * against the user's library and filtering out tracks they already have
 * (desktop parity — `music_player.py` `_update_discovery_results`).
 */
class DiscoveryRepository(
    context: Context,
    private val service: LastfmService? = null
) {

    private val keyStore = LastfmKeyStore(context)

    private val _apiKey = MutableStateFlow(keyStore.getKey())
    val apiKey: StateFlow<String?> = _apiKey.asStateFlow()

    private val lastfm: LastfmService = service
        ?: LastfmService(apiKeyProvider = { _apiKey.value })

    fun setApiKey(key: String) {
        val trimmed = key.trim()
        if (trimmed.isEmpty()) {
            clearApiKey()
            return
        }
        keyStore.setKey(trimmed)
        _apiKey.value = trimmed
        lastfm.clearCache()
    }

    fun clearApiKey() {
        keyStore.clearKey()
        _apiKey.value = null
        lastfm.clearCache()
    }

    /**
     * Fetches recommendations for the seed (artist, title) and filters out
     * anything the user already owns.  Matching follows the desktop rule:
     * the normalized `"artist - title"` string (lowercased, trimmed) must
     * not appear in the library.
     */
    suspend fun getSimilar(
        artist: String,
        title: String,
        limit: Int = 20,
        libraryTracks: List<Track> = emptyList()
    ): LastfmResult {
        val result = lastfm.getSimilar(artist, title, limit)
        if (result !is LastfmResult.Success || libraryTracks.isEmpty()) return result

        val libraryKeys = libraryTracks.mapNotNull { track ->
            val trackArtist = track.artist.trim()
            val trackTitle = track.title.trim()
            if (trackArtist.isEmpty() || trackTitle.isEmpty()) {
                null
            } else {
                "${trackArtist.lowercase()} - ${trackTitle.lowercase()}"
            }
        }.toSet()

        val filtered = result.tracks.filter { recommendation ->
            "${recommendation.artist.lowercase().trim()} - ${recommendation.title.lowercase().trim()}" !in libraryKeys
        }
        return LastfmResult.Success(filtered)
    }
}
