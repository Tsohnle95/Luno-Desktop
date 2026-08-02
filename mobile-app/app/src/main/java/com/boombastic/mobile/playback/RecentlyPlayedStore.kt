package com.boombastic.mobile.playback

import android.content.Context
import android.content.SharedPreferences
import org.json.JSONArray
import org.json.JSONObject

/** Small persistence boundary for the ordered recently-played metadata snapshots. */
interface RecentlyPlayedStore {
    fun load(): List<MediaTrack>
    fun save(tracks: List<MediaTrack>)
    fun clear()
}

/**
 * Stores the bounded history in app-private preferences so it is available on
 * the next process start without requiring a database migration.
 */
internal class SharedPreferencesRecentlyPlayedStore(
    context: Context
) : RecentlyPlayedStore {
    private val preferences: SharedPreferences = context.applicationContext
        .getSharedPreferences(PREFERENCES_NAME, Context.MODE_PRIVATE)

    override fun load(): List<MediaTrack> {
        val raw = preferences.getString(HISTORY_KEY, null) ?: return emptyList()
        return runCatching {
            val entries = JSONArray(raw)
            buildList(entries.length()) {
                for (index in 0 until entries.length()) {
                    val entry = entries.optJSONObject(index) ?: continue
                    val uri = entry.optString(FIELD_URI)
                    if (uri.isBlank()) continue
                    add(
                        MediaTrack(
                            uri = uri,
                            title = entry.optString(FIELD_TITLE, "Unknown"),
                            artist = entry.optString(FIELD_ARTIST, "Unknown"),
                            album = entry.optString(FIELD_ALBUM),
                            durationMs = entry.optLong(FIELD_DURATION_MS, 0L).coerceAtLeast(0L),
                            artworkUri = entry.optString(FIELD_ARTWORK_URI).takeUnless { it.isBlank() }
                        )
                    )
                }
            }
        }.getOrDefault(emptyList())
    }

    override fun save(tracks: List<MediaTrack>) {
        val entries = JSONArray()
        tracks.take(MAX_ENTRIES).forEach { track ->
            entries.put(
                JSONObject().apply {
                    put(FIELD_URI, track.uri)
                    put(FIELD_TITLE, track.title)
                    put(FIELD_ARTIST, track.artist)
                    put(FIELD_ALBUM, track.album)
                    put(FIELD_DURATION_MS, track.durationMs.coerceAtLeast(0L))
                    track.artworkUri?.let { put(FIELD_ARTWORK_URI, it) }
                }
            )
        }
        preferences.edit().putString(HISTORY_KEY, entries.toString()).apply()
    }

    override fun clear() {
        preferences.edit().remove(HISTORY_KEY).apply()
    }

    private companion object {
        const val PREFERENCES_NAME = "recently_played"
        const val HISTORY_KEY = "history"
        const val FIELD_URI = "uri"
        const val FIELD_TITLE = "title"
        const val FIELD_ARTIST = "artist"
        const val FIELD_ALBUM = "album"
        const val FIELD_DURATION_MS = "durationMs"
        const val FIELD_ARTWORK_URI = "artworkUri"
        const val MAX_ENTRIES = 100
    }
}
