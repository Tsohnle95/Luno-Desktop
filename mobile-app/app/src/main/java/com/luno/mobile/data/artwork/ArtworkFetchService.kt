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
    private val client: OkHttpClient = defaultClient()
) {
    suspend fun fetchAndSave(context: Context, artist: String, title: String): String? =
        withContext(Dispatchers.IO) {
            try {
                val query = listOf(artist, title)
                    .map(String::trim)
                    .filter { it.isNotBlank() && !it.equals("Unknown Artist", ignoreCase = true) }
                    .joinToString(" ")
                if (query.isBlank()) return@withContext null

                val searchUrl = "https://api.deezer.com/search".toHttpUrl().newBuilder()
                    .addQueryParameter("q", query)
                    .addQueryParameter("limit", "1")
                    .build()
                val imageUrl = client.newCall(
                    Request.Builder()
                        .url(searchUrl)
                        .header("User-Agent", "Luno/1.0 (Android)")
                        .build()
                ).execute().use { response ->
                    if (!response.isSuccessful) return@withContext null
                    val album = JSONObject(response.body?.string().orEmpty())
                        .optJSONArray("data")
                        ?.optJSONObject(0)
                        ?.optJSONObject("album")
                        ?: return@withContext null
                    album.optString("cover_xl")
                        .ifBlank { album.optString("cover_big") }
                        .ifBlank { album.optString("cover_medium") }
                        .trim()
                        .takeIf { it.startsWith("https://") || it.startsWith("http://") }
                        ?.let { url ->
                            if (url.startsWith("http://")) "https://${url.substring(7)}" else url
                        }
                } ?: return@withContext null

                val bytes = client.newCall(
                    Request.Builder()
                        .url(imageUrl)
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
        private const val TIMEOUT_SECONDS = 8L

        private fun defaultClient(): OkHttpClient = OkHttpClient.Builder()
            .connectTimeout(TIMEOUT_SECONDS, TimeUnit.SECONDS)
            .readTimeout(TIMEOUT_SECONDS, TimeUnit.SECONDS)
            .build()
    }
}
