package com.luno.mobile.data.discovery

import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * One Last.fm recommendation: artist, title, match score, and (when the
 * API provides one) an artwork URL.
 */
data class LastfmTrack(
    val artist: String,
    val title: String,
    val match: Double,
    val imageUrl: String? = null
)

/**
 * Outcome of a Last.fm fetch: [Success] with the parsed tracks, or
 * [Failure] with a user-facing message (bad key, API error, network
 * error).  A [Success] may carry an empty list when nothing matched.
 */
sealed interface LastfmResult {
    data class Success(val tracks: List<LastfmTrack>) : LastfmResult
    data class Failure(val message: String) : LastfmResult
}

/**
 * Last.fm recommendations client — a faithful port of the desktop
 * `DiscoveryService` (`engine.py` lines 10–83):
 *
 *  1. `track.getSimilar` for the seed track; on zero results,
 *  2. falls back to `artist.getTopTracks` with a hardcoded 0.8 match score.
 *
 * Results are cached per (artist, title) with the desktop cache policy:
 * the whole cache is cleared once it exceeds 10 entries.  The API key is
 * read through [apiKeyProvider] at call time (never stored here), and the
 * base URL is injectable for tests.
 */
open class LastfmService(
    private val apiKeyProvider: () -> String?,
    private val baseUrl: String = "https://ws.audioscrobbler.com/2.0/",
    private val client: OkHttpClient = defaultClient()
) {

    companion object {
        private const val TIMEOUT_SECONDS = 8L
        private const val MAX_CACHE_ENTRIES = 10

        /** Match score given to `artist.getTopTracks` fallback tracks (desktop parity). */
        const val FALLBACK_MATCH = 0.8

        fun defaultClient(): OkHttpClient = OkHttpClient.Builder()
            .connectTimeout(TIMEOUT_SECONDS, TimeUnit.SECONDS)
            .readTimeout(TIMEOUT_SECONDS, TimeUnit.SECONDS)
            .build()
    }

    private val cache = mutableMapOf<Triple<String, String, Int>, List<LastfmTrack>>()

    /**
     * Fetches similar tracks for (artist, title).  Returns the cached
     * result when one exists; otherwise hits the API (stage 1, then stage
     * 2 fallback) and stores a non-empty result.
     */
    open suspend fun getSimilar(artist: String, title: String, limit: Int = 20): LastfmResult {
        val apiKey = apiKeyProvider()?.trim().orEmpty()
        if (apiKey.isEmpty()) {
            return LastfmResult.Failure("Set a Last.fm API key in Settings first")
        }
        val seedArtist = artist.trim()
        val seedTitle = title.trim()
        if (seedArtist.isEmpty() || seedTitle.isEmpty()) {
            return LastfmResult.Success(emptyList())
        }

        val cacheKey = Triple(
            seedArtist.lowercase(),
            seedTitle.lowercase(),
            limit.coerceAtLeast(0)
        )
        synchronized(cache) {
            cache[cacheKey]?.let { return LastfmResult.Success(it) }
        }

        val similar = fetchSimilar(seedArtist, seedTitle, apiKey, limit)
        if (similar is LastfmResult.Success) {
            storeCache(cacheKey, similar.tracks)
            if (similar.tracks.isNotEmpty()) return similar
        } else {
            return similar
        }

        // Stage 2 (desktop parity): no similar tracks — fetch popular
        // tracks by the artist, all scored 0.8.
        val topTracks = fetchTopTracks(seedArtist, apiKey, limit)
        if (topTracks is LastfmResult.Success) {
            val fallbackTracks = topTracks.tracks.map { it.copy(match = FALLBACK_MATCH) }
            storeCache(cacheKey, fallbackTracks)
            return LastfmResult.Success(fallbackTracks)
        }
        return topTracks
    }

    fun clearCache() {
        synchronized(cache) { cache.clear() }
    }

    private sealed interface ApiOutcome {
        data class Ok(val json: JSONObject) : ApiOutcome
        data class Fail(val result: LastfmResult.Failure) : ApiOutcome
    }

    private suspend fun fetchSimilar(
        artist: String,
        title: String,
        apiKey: String,
        limit: Int
    ): LastfmResult {
        val outcome = callApi(
            linkedMapOf(
                "method" to "track.getsimilar",
                "artist" to artist,
                "track" to title,
                "api_key" to apiKey,
                "format" to "json",
                "limit" to limit.toString()
            )
        )
        return when (outcome) {
            is ApiOutcome.Fail -> outcome.result
            is ApiOutcome.Ok -> LastfmResult.Success(withContext(Dispatchers.Default) {
                parseTracks(outcome.json.optJSONObject("similartracks")?.optJSONArray("track"))
            })
        }
    }

    private suspend fun fetchTopTracks(artist: String, apiKey: String, limit: Int): LastfmResult {
        val outcome = callApi(
            linkedMapOf(
                "method" to "artist.gettoptracks",
                "artist" to artist,
                "api_key" to apiKey,
                "format" to "json",
                "limit" to limit.toString()
            )
        )
        return when (outcome) {
            is ApiOutcome.Fail -> outcome.result
            is ApiOutcome.Ok -> LastfmResult.Success(withContext(Dispatchers.Default) {
                parseTracks(outcome.json.optJSONObject("toptracks")?.optJSONArray("track"))
            })
        }
    }

    private suspend fun callApi(params: Map<String, String>): ApiOutcome {
        return withContext(Dispatchers.IO) {
            try {
                val url = baseUrl.toHttpUrl().newBuilder().apply {
                    params.forEach { (key, value) -> addQueryParameter(key, value) }
                }.build()
                val request = Request.Builder()
                    .url(url)
                    .header("User-Agent", "Luno/0.1.0 (Android)")
                    .build()
                val body = client.newCall(request).execute().use { response ->
                    response.body?.string().orEmpty()
                }
                val json = JSONObject(body)
                if (json.has("error")) {
                    val message = json.optString("message").ifBlank { "Last.fm API error" }
                    ApiOutcome.Fail(LastfmResult.Failure(message))
                } else {
                    ApiOutcome.Ok(json)
                }
            } catch (e: Exception) {
                ApiOutcome.Fail(LastfmResult.Failure("Could not reach Last.fm — check your connection"))
            }
        }
    }

    private fun parseTracks(array: JSONArray?, matchOverride: Double? = null): List<LastfmTrack> {
        if (array == null) return emptyList()
        val out = ArrayList<LastfmTrack>(array.length())
        for (i in 0 until array.length()) {
            val entry = array.optJSONObject(i) ?: continue
            // artist is an object ({"name": ...}) on track.getsimilar and a
            // string on some other endpoints — desktop handles both too.
            val artistNode = entry.opt("artist")
            val artist = when (artistNode) {
                is JSONObject -> artistNode.optString("name")
                is String -> artistNode
                else -> ""
            }
            val title = entry.optString("name")
            if (artist.isBlank() || title.isBlank()) continue
            out += LastfmTrack(
                artist = artist,
                title = title,
                match = matchOverride ?: entry.optDouble("match", 0.0),
                imageUrl = pickLargestImage(entry.optJSONArray("image"))
            )
        }
        return out
    }

    private fun pickLargestImage(images: JSONArray?): String? {
        if (images == null) return null
        val bySize = mutableMapOf<String, String>()
        var firstImage: String? = null
        for (i in 0 until images.length()) {
            val image = images.optJSONObject(i) ?: continue
            val rawUrl = image.optString("#text").ifBlank { image.optString("url") }
            val url = normalizeImageUrl(rawUrl) ?: continue
            firstImage = firstImage ?: url
            bySize[image.optString("size")] = url
        }
        for (size in listOf("mega", "extralarge", "large", "medium", "small")) {
            bySize[size]?.takeIf { it.isNotBlank() }?.let { return it }
        }
        return firstImage
    }

    /** Android blocks cleartext image requests; older Last.fm responses use http URLs. */
    private fun normalizeImageUrl(rawUrl: String): String? {
        val url = rawUrl.trim()
        return when {
            url.startsWith("https://", ignoreCase = true) -> url
            url.startsWith("http://", ignoreCase = true) -> "https://${url.substring(7)}"
            else -> null
        }
    }

    private fun storeCache(key: Triple<String, String, Int>, tracks: List<LastfmTrack>) {
        if (tracks.isEmpty()) return
        synchronized(cache) {
            // Desktop parity: clear the whole cache once it exceeds 10.
            if (cache.size > MAX_CACHE_ENTRIES) cache.clear()
            cache[key] = tracks
        }
    }
}
