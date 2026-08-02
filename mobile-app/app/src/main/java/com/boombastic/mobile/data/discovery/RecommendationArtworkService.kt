package com.boombastic.mobile.data.discovery

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject
import java.util.concurrent.TimeUnit

/** Finds a cover image when Last.fm does not provide usable artwork. */
class RecommendationArtworkService(
    private val baseUrl: String = "https://api.deezer.com/search",
    private val client: OkHttpClient = defaultClient()
) {

    companion object {
        private const val TIMEOUT_SECONDS = 8L

        fun defaultClient(): OkHttpClient = OkHttpClient.Builder()
            .connectTimeout(TIMEOUT_SECONDS, TimeUnit.SECONDS)
            .readTimeout(TIMEOUT_SECONDS, TimeUnit.SECONDS)
            .build()
    }

    private val cache = mutableMapOf<String, String?>()

    /** Returns the best matching album cover, or null when the lookup fails. */
    suspend fun findArtwork(artist: String, title: String): String? {
        val key = "${artist.trim().lowercase()}|${title.trim().lowercase()}"
        synchronized(cache) {
            if (cache.containsKey(key)) return cache[key]
        }

        val artwork = withContext(Dispatchers.IO) {
            runCatching {
                val url = baseUrl.toHttpUrl().newBuilder()
                    .addQueryParameter("q", "artist:\"$artist\" track:\"$title\"")
                    .addQueryParameter("limit", "1")
                    .build()
                val request = Request.Builder()
                    .url(url)
                    .header("User-Agent", "Luno/0.1.0 (Android)")
                    .build()
                client.newCall(request).execute().use { response ->
                    if (!response.isSuccessful) return@use null
                    val track = JSONObject(response.body?.string().orEmpty())
                        .optJSONArray("data")
                        ?.optJSONObject(0)
                    val album = track?.optJSONObject("album") ?: return@use null
                    normalizeUrl(
                        album.optString("cover_xl")
                            .ifBlank { album.optString("cover_big") }
                            .ifBlank { album.optString("cover_medium") }
                    )
                }
            }.getOrNull()
        }

        synchronized(cache) { cache[key] = artwork }
        return artwork
    }

    private fun normalizeUrl(rawUrl: String): String? {
        val url = rawUrl.trim()
        return when {
            url.startsWith("https://", ignoreCase = true) -> url
            url.startsWith("http://", ignoreCase = true) -> "https://${url.substring(7)}"
            else -> null
        }
    }
}
