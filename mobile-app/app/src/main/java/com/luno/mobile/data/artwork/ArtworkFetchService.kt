package com.luno.mobile.data.artwork

import android.content.Context
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject
import java.util.concurrent.TimeUnit

/** Downloads a cover from Deezer and stores it in the app artwork cache. */
class ArtworkFetchService(
    private val baseUrl: String = DEEZER_SEARCH_URL,
    private val client: OkHttpClient = defaultClient()
) {
    suspend fun fetchAndSave(context: Context, artist: String, title: String): String? =
        withContext(Dispatchers.IO) {
            try {
                for (query in searchQueries(artist, title)) {
                    val imageUrl = findImageUrl(query) ?: continue
                    val path = fetchAndSaveImage(context, imageUrl)
                    if (!path.isNullOrBlank() && ArtworkStorage.hasUsableArtwork(path)) {
                        return@withContext path
                    }
                }
                null
            } catch (e: CancellationException) {
                throw e
            } catch (_: Throwable) {
                null
            }
        }

    private fun findImageUrl(query: String): String? {
        val searchUrl = baseUrl.toHttpUrl().newBuilder()
            .addQueryParameter("q", query)
            .addQueryParameter("limit", "1")
            .build()
        return client.newCall(
            Request.Builder()
                .url(searchUrl)
                .header("Accept", "application/json")
                .header("User-Agent", "Luno/1.0 (Android)")
                .build()
        ).execute().use { response ->
            if (!response.isSuccessful) return null
            val album = JSONObject(response.body?.string().orEmpty())
                .optJSONArray("data")
                ?.optJSONObject(0)
                ?.optJSONObject("album")
                ?: return null
            normalizeImageUrl(
                album.optString("cover_xl")
                    .ifBlank { album.optString("cover_big") }
                    .ifBlank { album.optString("cover_medium") }
            )
        }
    }

    /** Keep working with less structured metadata when a combined query misses. */
    private fun searchQueries(artist: String, title: String): List<String> {
        val cleanArtist = artist.trim()
            .takeUnless { it.isBlank() || it.equals("Unknown Artist", ignoreCase = true) }
        val cleanTitle = title.trim()
            .takeUnless { it.isBlank() || it.equals("Unknown Track", ignoreCase = true) }
        return buildList {
            if (cleanArtist != null && cleanTitle != null) {
                add("$cleanArtist $cleanTitle")
                add(cleanTitle)
                add(cleanArtist)
            } else {
                cleanArtist?.let(::add)
                cleanTitle?.let(::add)
            }
        }.distinct()
    }

    private fun normalizeImageUrl(rawUrl: String): String? {
        val url = rawUrl.trim()
        return when {
            url.startsWith("https://", ignoreCase = true) -> url
            url.startsWith("http://", ignoreCase = true) -> "https://${url.substring(7)}"
            else -> null
        }
    }

    /** Fetches a known image URL, such as a stored YouTube thumbnail. */
    suspend fun fetchAndSaveImage(context: Context, imageUrl: String): String? =
        withContext(Dispatchers.IO) {
            try {
                val normalizedUrl = if (imageUrl.startsWith("http://")) {
                    "https://${imageUrl.substring(7)}"
                } else {
                    imageUrl
                }
                val bytes = client.newCall(
                    Request.Builder()
                        .url(normalizedUrl)
                        .header("User-Agent", "Luno/1.0 (Android)")
                        .build()
                ).execute().use { response ->
                    if (!response.isSuccessful) null else response.body?.bytes()
                } ?: return@withContext null
                ArtworkStorage.saveImageBytes(context, bytes)
            } catch (e: CancellationException) {
                throw e
            } catch (_: Throwable) {
                null
            }
        }

    companion object {
        private const val DEEZER_SEARCH_URL = "https://api.deezer.com/search"
        private const val TIMEOUT_SECONDS = 8L

        private fun defaultClient(): OkHttpClient = OkHttpClient.Builder()
            .connectTimeout(TIMEOUT_SECONDS, TimeUnit.SECONDS)
            .readTimeout(TIMEOUT_SECONDS, TimeUnit.SECONDS)
            .build()
    }
}
