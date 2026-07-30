package com.boombastic.mobile.playback

import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import java.io.BufferedReader
import java.io.InputStreamReader
import java.io.OutputStreamWriter
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder

data class WebSearchResult(
    val title: String,
    val artist: String,
    val duration: Long,
    val thumbnailUrl: String,
    val videoId: String,
    val source: String
)

data class PlaylistVideo(
    val videoId: String,
    val title: String,
    val artist: String,
    val duration: Long
)

object WebSearchService {

    private const val TAG = "WebSearchService"

    private const val INNERTUBE_API_KEY = "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8"
    private const val INNERTUBE_BASE = "https://www.youtube.com/youtubei/v1"
    private const val INNERTUBE_CLIENT_NAME = "WEB"
    private const val INNERTUBE_CLIENT_VERSION = "2.20240718.01.00"

    private val PIPED_INSTANCES = listOf(
        "https://api.piped.private.coffee",
        "https://pipedapi.adminforge.de",
        "https://pipedapi.smnz.de",
        "https://piped-api.privacy.com.de",
        "https://pipedapi.ducks.party",
        "https://pipedapi.orangenet.cc",
        "https://pipedapi.drgns.space",
    )

    private val INVIDIOUS_INSTANCES = listOf(
        "https://inv.nadeko.net",
        "https://invidious.private.coffee",
        "https://yewtu.be",
    )

    private const val USER_AGENT = "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.6422.165 Mobile Safari/537.36"

    private var currentPipedInstance = PIPED_INSTANCES[0]

    private fun innertubeContext(): JSONObject {
        val client = JSONObject()
        client.put("clientName", INNERTUBE_CLIENT_NAME)
        client.put("clientVersion", INNERTUBE_CLIENT_VERSION)
        val context = JSONObject()
        context.put("client", client)
        return context
    }

    private fun innertubePost(endpoint: String, body: JSONObject): JSONObject? {
        return try {
            val bodyStr = body.toString()
            val url = URL("$INNERTUBE_BASE/$endpoint?key=$INNERTUBE_API_KEY")
            val conn = url.openConnection() as HttpURLConnection
            conn.requestMethod = "POST"
            conn.doOutput = true
            conn.connectTimeout = 15000
            conn.readTimeout = 15000
            conn.setRequestProperty("Content-Type", "application/json")
            conn.setRequestProperty("User-Agent", USER_AGENT)
            conn.setRequestProperty("Origin", "https://www.youtube.com")
            conn.setRequestProperty("Referer", "https://www.youtube.com")
            conn.setRequestProperty("Accept", "application/json")
            conn.setChunkedStreamingMode(0)

            val writer = OutputStreamWriter(conn.outputStream, Charsets.UTF_8)
            writer.write(bodyStr)
            writer.flush()
            writer.close()

            val responseCode = conn.responseCode
            if (responseCode != 200) {
                Log.w(TAG, "InnerTube $endpoint returned $responseCode")
                val errorStream = conn.errorStream
                if (errorStream != null) {
                    val errBody = errorStream.bufferedReader().readText()
                    Log.w(TAG, "InnerTube error body: ${errBody.take(300)}")
                    errorStream.close()
                }
                conn.disconnect()
                return null
            }

            val reader = BufferedReader(InputStreamReader(conn.inputStream, Charsets.UTF_8))
            val response = reader.readText()
            reader.close()
            conn.disconnect()

            Log.d(TAG, "InnerTube $endpoint response (first 300): ${response.take(300)}")

            val json = JSONObject(response)
            if (json.has("error")) {
                val err = json.optJSONObject("error")
                Log.w(TAG, "InnerTube $endpoint error in body: ${err?.optString("message", "")}")
                return null
            }
            json
        } catch (e: Exception) {
            Log.e(TAG, "InnerTube $endpoint failed", e)
            null
        }
    }

    fun searchYouTube(query: String, limit: Int = 15): List<WebSearchResult> {
        val results = innertubeSearch(query, limit)
        if (results.isNotEmpty()) return results
        Log.w(TAG, "InnerTube search returned 0 results, trying Piped API fallback")
        return pipedSearch(query, limit)
    }

    private fun innertubeSearch(query: String, limit: Int): List<WebSearchResult> {
        val results = mutableListOf<WebSearchResult>()
        try {
            val body = JSONObject()
            body.put("context", innertubeContext())
            body.put("query", query)

            val response = innertubePost("search", body) ?: return emptyList()

            val contents = response.optJSONObject("contents")
                ?.optJSONObject("twoColumnSearchResultsRenderer")
                ?.optJSONObject("primaryContents")
                ?.optJSONObject("sectionListRenderer")
                ?.optJSONArray("contents") ?: return emptyList()

            for (i in 0 until contents.length()) {
                val section = contents.optJSONObject(i)
                val itemSection = section?.optJSONObject("itemSectionRenderer") ?: continue
                val vidContents = itemSection.optJSONArray("contents") ?: continue

                for (j in 0 until vidContents.length()) {
                    if (results.size >= limit) break
                    val video = vidContents.optJSONObject(j)?.optJSONObject("videoRenderer") ?: continue

                    val videoId = video.optString("videoId", "")
                    if (videoId.isBlank()) continue

                    val titleObj = video.optJSONObject("title")
                    val title = extractInnerTubeText(titleObj)

                    val ownerText = video.optJSONObject("ownerText")
                    val artist = extractInnerTubeText(ownerText)

                    val lengthText = video.optJSONObject("lengthText")
                    val duration = parseInnerTubeDuration(lengthText?.optString("simpleText", "") ?: "")

                    val thumbnails = video.optJSONObject("thumbnail")?.optJSONArray("thumbnails")
                    val thumbnail = if (thumbnails != null && thumbnails.length() > 0) {
                        thumbnails.optJSONObject(thumbnails.length() - 1)?.optString("url", "")
                    } else ""

                    results.add(
                        WebSearchResult(
                            title = title,
                            artist = artist,
                            duration = duration,
                            thumbnailUrl = thumbnail ?: "",
                            videoId = videoId,
                            source = "YouTube"
                        )
                    )
                }
                if (results.size >= limit) break
            }
        } catch (e: Exception) {
            Log.e(TAG, "InnerTube search failed", e)
        }
        return results
    }

    private fun extractInnerTubeText(obj: JSONObject?): String {
        if (obj == null) return "Unknown"
        val runs = obj.optJSONArray("runs")
        if (runs != null) {
            val sb = StringBuilder()
            for (i in 0 until runs.length()) {
                sb.append(runs.optJSONObject(i)?.optString("text", ""))
            }
            return sb.toString().trim()
        }
        return obj.optString("simpleText", "Unknown")
    }

    private fun parseInnerTubeDuration(duration: String): Long {
        if (duration.isBlank()) return 0L
        val parts = duration.split(":")
        return when (parts.size) {
            3 -> {
                val h = parts[0].toLongOrNull() ?: 0L
                val m = parts[1].toLongOrNull() ?: 0L
                val s = parts[2].toLongOrNull() ?: 0L
                h * 3600 + m * 60 + s
            }
            2 -> {
                val m = parts[0].toLongOrNull() ?: 0L
                val s = parts[1].toLongOrNull() ?: 0L
                m * 60 + s
            }
            else -> parts[0].toLongOrNull() ?: 0L
        }
    }

    private fun pipedSearch(query: String, limit: Int): List<WebSearchResult> {
        val results = mutableListOf<WebSearchResult>()
        try {
            val encoded = URLEncoder.encode(query, "UTF-8")

            for (instance in PIPED_INSTANCES) {
                val url = URL("$instance/search?q=$encoded&filter=videos")
                Log.d(TAG, "Piped search: $url")
                val conn = url.openConnection() as HttpURLConnection
                conn.connectTimeout = 10000
                conn.readTimeout = 10000
                conn.setRequestProperty("Accept", "application/json")
                conn.setRequestProperty("User-Agent", USER_AGENT)

                val responseCode = conn.responseCode
                if (responseCode != 200) {
                    Log.w(TAG, "Instance $instance returned $responseCode, trying next")
                    continue
                }

                val reader = BufferedReader(InputStreamReader(conn.inputStream))
                val body = reader.readText()
                reader.close()
                conn.disconnect()

                val json = JSONObject(body)
                if (!json.has("items")) {
                    Log.w(TAG, "No 'items' key in response, trying next instance")
                    continue
                }

                val items = json.getJSONArray("items")
                currentPipedInstance = instance

                for (i in 0 until minOf(items.length(), limit)) {
                    val item = items.getJSONObject(i)
                    val itemType = item.optString("type", "")
                    if (itemType != "music" && itemType != "video" && itemType != "stream") continue

                    val title = item.optString("title", "Unknown")
                    val uploader = item.optString("uploaderName", item.optString("uploader", "Unknown Artist"))
                    val duration = item.optLong("duration", 0L)
                    val thumbnail = item.optString("thumbnail", "")
                    val videoId = parseVideoId(item.optString("url", ""))

                    if (videoId.isBlank()) continue

                    results.add(
                        WebSearchResult(
                            title = title,
                            artist = uploader,
                            duration = duration,
                            thumbnailUrl = thumbnail,
                            videoId = videoId,
                            source = "YouTube"
                        )
                    )
                }
                if (results.isNotEmpty()) break
            }
        } catch (e: Exception) {
            Log.e(TAG, "Piped search failed", e)
        }
        return results
    }

    fun getPlaylistVideos(playlistUrl: String): Pair<String, List<PlaylistVideo>> {
        val playlistId = extractPlaylistId(playlistUrl) ?: return Pair("", emptyList())
        val videos = mutableListOf<PlaylistVideo>()
        var playlistName = "Unknown Playlist"

        try {
            val body = JSONObject()
            body.put("context", innertubeContext())
            body.put("browseId", "VL$playlistId")

            val response = innertubePost("browse", body)
            if (response != null) {
                val sidebar = response.optJSONObject("sidebar")
                if (sidebar != null) {
                    val playlistRenderer = sidebar
                        .optJSONObject("playlistSidebarRenderer")
                        ?.optJSONArray("items")
                    if (playlistRenderer != null && playlistRenderer.length() > 0) {
                        playlistName = playlistRenderer.optJSONObject(0)
                            ?.optJSONObject("playlistSidebarPrimaryInfoRenderer")
                            ?.optJSONObject("title")
                            ?.optString("simpleText", "Unknown Playlist") ?: "Unknown Playlist"
                    }
                }

                val contents = response
                    .optJSONArray("contents")
                    ?.optJSONObject(0)
                    ?.optJSONObject("twoColumnBrowseResultsRenderer")
                    ?.optJSONArray("tabs")
                    ?.optJSONObject(0)
                    ?.optJSONObject("tabRenderer")
                    ?.optJSONObject("content")
                    ?.optJSONObject("sectionListRenderer")
                    ?.optJSONArray("contents")
                    ?.optJSONObject(0)
                    ?.optJSONObject("itemSectionRenderer")
                    ?.optJSONArray("contents")
                    ?.optJSONObject(0)
                    ?.optJSONObject("playlistVideoListRenderer")
                    ?.optJSONArray("contents")

                if (contents != null) {
                    for (i in 0 until contents.length()) {
                        val video = contents.optJSONObject(i)?.optJSONObject("playlistVideoRenderer") ?: continue
                        if (video.optBoolean("isPlayable", true) == false) continue

                        val vidId = video.optString("videoId", "")
                        if (vidId.isBlank()) continue

                        val titleObj = video.optJSONObject("title")
                        val title = extractInnerTubeText(titleObj)

                        val shortByline = video.optJSONObject("shortBylineText")
                        val uploader = extractInnerTubeText(shortByline)

                        val length = video.optJSONObject("lengthText")?.optString("simpleText", "")
                        val duration = parseInnerTubeDuration(length ?: "")

                        videos.add(
                            PlaylistVideo(
                                videoId = vidId,
                                title = title,
                                artist = uploader,
                                duration = duration
                            )
                        )
                    }
                    if (videos.isNotEmpty()) return Pair(playlistName, videos)
                }
            }
        } catch (e: Exception) {
            Log.e(TAG, "InnerTube getPlaylistVideos failed", e)
        }

        return pipedGetPlaylistVideos(playlistId, playlistName)
    }

    private fun pipedGetPlaylistVideos(playlistId: String, fallbackName: String): Pair<String, List<PlaylistVideo>> {
        for (instance in PIPED_INSTANCES) {
            try {
                val url = URL("$instance/playlists/$playlistId")
                Log.d(TAG, "Piped playlist fetch: $url")
                val conn = url.openConnection() as HttpURLConnection
                conn.connectTimeout = 15000
                conn.readTimeout = 15000
                conn.setRequestProperty("Accept", "application/json")
                conn.setRequestProperty("User-Agent", USER_AGENT)

                if (conn.responseCode != 200) {
                    Log.w(TAG, "Piped instance $instance returned ${conn.responseCode} for playlist, trying next")
                    conn.disconnect()
                    continue
                }

                val reader = BufferedReader(InputStreamReader(conn.inputStream))
                val body = reader.readText()
                reader.close()
                conn.disconnect()

                val json = JSONObject(body)
                val name = json.optString("name", fallbackName)
                val streams = json.optJSONArray("relatedStreams") ?: JSONArray()

                val videos = mutableListOf<PlaylistVideo>()
                for (i in 0 until streams.length()) {
                    val stream = streams.optJSONObject(i) ?: continue
                    if (stream.optBoolean("isShort", false)) continue

                    val vidId = parseVideoId(stream.optString("url", ""))
                    if (vidId.isBlank()) continue

                    videos.add(
                        PlaylistVideo(
                            videoId = vidId,
                            title = stream.optString("title", "Unknown"),
                            artist = stream.optString("uploaderName", ""),
                            duration = stream.optLong("duration", 0L)
                        )
                    )
                }
                if (videos.isNotEmpty()) {
                    currentPipedInstance = instance
                    return Pair(name, videos)
                }
            } catch (e: Exception) {
                Log.w(TAG, "Piped instance $instance failed for playlist $playlistId", e)
            }
        }
        return Pair(fallbackName, emptyList())
    }

    fun getAudioStreamUrl(videoId: String): String? {
        // 1. Try parsing the YouTube watch page directly (most reliable)
        val watchUrl = watchPageGetAudioUrl(videoId)
        if (watchUrl != null) return watchUrl
        Log.w(TAG, "Watch page parsing failed for $videoId, trying Piped API")

        // 2. Try Piped API instances
        val pipedUrl = pipedGetAudioUrl(videoId)
        if (pipedUrl != null) return pipedUrl
        Log.w(TAG, "Piped failed for $videoId, trying InnerTube with multiple client types")

        // 3. Try InnerTube player endpoint
        val innerTubeUrl = innertubeGetAudioUrl(videoId)
        if (innerTubeUrl != null) return innerTubeUrl
        Log.w(TAG, "InnerTube failed for $videoId, trying Invidious API fallback")

        // 4. Last resort: Invidious
        return invidiousGetAudioUrl(videoId)
    }

    /**
     * Fetches the YouTube watch page HTML and extracts the streaming data
     * from the embedded ytInitialPlayerResponse JSON.
     * This mimics what youtube-dl does.
     */
    private fun watchPageGetAudioUrl(videoId: String): String? {
        try {
            val pageUrl = URL("https://www.youtube.com/watch?v=$videoId")
            Log.d(TAG, "Watch page fetch: $pageUrl")
            val conn = pageUrl.openConnection() as HttpURLConnection
            conn.connectTimeout = 15000
            conn.readTimeout = 15000
            conn.setRequestProperty("User-Agent", USER_AGENT)
            conn.setRequestProperty("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            conn.setRequestProperty("Accept-Language", "en-US,en;q=0.5")
            conn.setRequestProperty("Referer", "https://www.youtube.com")
            conn.instanceFollowRedirects = true

            val responseCode = conn.responseCode
            if (responseCode != 200) {
                Log.w(TAG, "Watch page returned $responseCode for $videoId")
                conn.disconnect()
                return null
            }

            val reader = BufferedReader(InputStreamReader(conn.inputStream, Charsets.UTF_8))
            val html = reader.readText()
            reader.close()
            conn.disconnect()

            val jsonStr = extractJsonFromHtml(html, "ytInitialPlayerResponse")
            if (jsonStr == null) {
                Log.w(TAG, "No ytInitialPlayerResponse found in watch page for $videoId")
                return null
            }

            val response = JSONObject(jsonStr)

            // Check playability
            val playabilityStatus = response.optJSONObject("playabilityStatus")
            if (playabilityStatus != null) {
                val status = playabilityStatus.optString("status", "")
                if (status == "UNPLAYABLE" || status == "LOGIN_REQUIRED" || status == "ERROR") {
                    val reason = playabilityStatus.optString("reason", status)
                    Log.w(TAG, "Watch page: video $videoId is $status ($reason)")
                }
            }

            val streamUrl = extractStreamUrlFromResponse(response)
            if (streamUrl != null) {
                Log.d(TAG, "Watch page parsing succeeded for $videoId")
                return streamUrl
            }

            val ytcfgPattern = Regex("ytcfg\\.set\\s*\\(\\s*(\\{.+?\\})\\s*\\)\\s*;")
            val cfgMatch = ytcfgPattern.find(html)
            if (cfgMatch != null) {
                Log.d(TAG, "Found ytcfg in watch page for $videoId, but no streaming data")
            }
        } catch (e: Exception) {
            Log.e(TAG, "Watch page fetch failed for $videoId", e)
        }
        return null
    }

    /**
     * Extracts a JSON variable assignment from HTML by finding
     * `var VAR_NAME = {` and counting braces to extract the full object.
     */
    private fun extractJsonFromHtml(html: String, varName: String): String? {
        val startMarker = "$varName = "
        val idx = html.indexOf(startMarker)
        if (idx < 0) return null
        val jsonStart = idx + startMarker.length
        if (jsonStart >= html.length || html[jsonStart] != '{') return null
        var depth = 0
        var inString = false
        var escaped = false
        for (i in jsonStart until html.length) {
            val c = html[i]
            if (escaped) { escaped = false; continue }
            if (c == '\\' && inString) { escaped = true; continue }
            if (c == '"') { inString = !inString; continue }
            if (inString) continue
            when (c) {
                '{' -> depth++
                '}' -> {
                    depth--
                    if (depth == 0) {
                        return html.substring(jsonStart, i + 1)
                    }
                }
            }
        }
        return null
    }

    private data class ITubeClient(
        val name: String,
        val version: String,
        val sdk: Int? = null,
        val os: String? = null,
        val make: String? = null,
        val model: String? = null
    )

    private val INNERTUBE_CLIENTS = listOf(
        ITubeClient("ANDROID_MUSIC", "6.27.56", 35, "14"),
        ITubeClient("ANDROID", "19.43.38", 35, "14", "samsung", "SM-S928B"),
        ITubeClient("TVHTML5_SIMPLY", "7.20250101.00.00"),
        ITubeClient("WEB", "2.20250101.00.00"),
    )

    private fun innertubeGetAudioUrl(videoId: String): String? {
        for (clientCfg in INNERTUBE_CLIENTS) {
            try {
                val client = JSONObject()
                client.put("clientName", clientCfg.name)
                client.put("clientVersion", clientCfg.version)
                client.put("platform", "MOBILE")
                if (clientCfg.sdk != null) client.put("androidSdkVersion", clientCfg.sdk)
                if (clientCfg.os != null) {
                    client.put("osName", "Android")
                    client.put("osVersion", clientCfg.os)
                }
                if (clientCfg.make != null) client.put("deviceMake", clientCfg.make)
                if (clientCfg.model != null) client.put("deviceModel", clientCfg.model)
                client.put("userAgent", USER_AGENT)
                client.put("clientScreen", "WATCH")
                val context = JSONObject()
                context.put("client", client)

                val body = JSONObject()
                body.put("context", context)
                body.put("videoId", videoId)
                body.put("contentCheckOk", true)
                body.put("racyCheckOk", true)
                body.put("playbackContext", JSONObject().apply {
                    put("contentPlaybackContext", JSONObject().apply {
                        put("html5Preference", "HTML5_PREF_WANTS")
                        put("signatureTimestamp", (System.currentTimeMillis() / 1000 - 10000).toInt())
                    })
                })

                val response = innertubePost("player", body)
                if (response == null) {
                    Log.d(TAG, "InnerTube ${clientCfg.name} returned null response for $videoId")
                    continue
                }

                val playabilityStatus = response.optJSONObject("playabilityStatus")
                if (playabilityStatus != null) {
                    val statusStr = playabilityStatus.optString("status", "")
                    if (statusStr != "OK") {
                        val reason = playabilityStatus.optString("reason", statusStr)
                        Log.d(TAG, "InnerTube ${clientCfg.name} status $statusStr ($reason) for $videoId, still trying to extract")
                    }
                }

                val url = extractStreamUrlFromResponse(response)
                if (url != null) {
                    Log.d(TAG, "InnerTube ${clientCfg.name} succeeded for $videoId")
                    return url
                }

                Log.d(TAG, "InnerTube ${clientCfg.name} no stream in response for $videoId")
            } catch (e: Exception) {
                Log.e(TAG, "InnerTube ${clientCfg.name} failed for $videoId", e)
            }
        }
        return null
    }

    private fun extractStreamUrlFromResponse(response: JSONObject): String? {
        val streamingData = response.optJSONObject("streamingData") ?: return null

        val adaptiveFormats = streamingData.optJSONArray("adaptiveFormats") ?: JSONArray()

        var bestAudio: StreamInfo? = null
        var bestCipher: Triple<String, String, String>? = null // url, sp, sig
        for (i in 0 until adaptiveFormats.length()) {
            val fmt = adaptiveFormats.optJSONObject(i) ?: continue
            val mime = fmt.optString("mimeType", "")
            if (!mime.startsWith("audio/")) continue

            val directUrl = fmt.optString("url", "")
            if (directUrl.isNotBlank()) {
                val info = StreamInfo(directUrl, mime, fmt.optInt("bitrate", 0))
                if (bestAudio == null || info.bitrate > bestAudio.bitrate) {
                    bestAudio = info
                }
            } else {
                val cipher = fmt.optString("signatureCipher", "").ifBlank {
                    fmt.optString("cipher", "")
                }
                if (cipher.isNotBlank()) {
                    val decoded = extractCipherParams(cipher)
                    if (decoded != null && (bestCipher == null ||
                            fmt.optInt("bitrate", 0) > 0)) {
                        bestCipher = decoded
                    }
                }
            }
        }

        if (bestAudio != null) return bestAudio.url
        if (bestCipher != null) return "${bestCipher.first}&${bestCipher.second}=${bestCipher.third}"

        val formats = streamingData.optJSONArray("formats") ?: JSONArray()
        for (i in 0 until formats.length()) {
            val fmt = formats.optJSONObject(i) ?: continue
            val directUrl = fmt.optString("url", "")
            if (directUrl.isNotBlank()) return directUrl
            val cipher = fmt.optString("cipher", "").ifBlank {
                fmt.optString("signatureCipher", "")
            }
            if (cipher.isNotBlank()) {
                val decoded = extractCipherParams(cipher)
                if (decoded != null) return "${decoded.first}&${decoded.second}=${decoded.third}"
            }
        }

        val hls = streamingData.optString("hlsManifestUrl", "")
        if (hls.isNotBlank()) return hls

        val dash = streamingData.optString("dashManifestUrl", "")
        if (dash.isNotBlank()) return dash

        return null
    }

    private fun extractCipherParams(cipher: String): Triple<String, String, String>? {
        val params = cipher.split("&").associate {
            val eq = it.indexOf("=")
            if (eq > 0) it.substring(0, eq) to it.substring(eq + 1) else "" to it
        }
        val url = java.net.URLDecoder.decode(params["url"] ?: return null, "UTF-8")
        val sp = params["sp"] ?: "signature"
        val sig = params["s"] ?: params["sig"] ?: return null
        return Triple(url, sp, sig)
    }

    private fun invidiousGetAudioUrl(videoId: String): String? {
        for (instance in INVIDIOUS_INSTANCES) {
            try {
                val url = URL("$instance/api/v1/videos/$videoId")
                Log.d(TAG, "Invidious stream fetch: $url")
                val conn = url.openConnection() as HttpURLConnection
                conn.connectTimeout = 15000
                conn.readTimeout = 15000
                conn.setRequestProperty("Accept", "application/json")
                conn.setRequestProperty("User-Agent", USER_AGENT)

                if (conn.responseCode != 200) {
                    Log.w(TAG, "Invidious instance $instance returned ${conn.responseCode}, trying next")
                    conn.disconnect()
                    continue
                }

                val reader = BufferedReader(InputStreamReader(conn.inputStream))
                val body = reader.readText()
                reader.close()
                conn.disconnect()

                val json = JSONObject(body)
                val adaptiveFormats = json.optJSONArray("adaptiveFormats") ?: JSONArray()

                var bestAudio: StreamInfo? = null
                for (i in 0 until adaptiveFormats.length()) {
                    val fmt = adaptiveFormats.optJSONObject(i) ?: continue
                    val mime = fmt.optString("mimeType", "")
                    if (!mime.startsWith("audio/")) continue
                    val sUrl = fmt.optString("url", "")
                    if (sUrl.isBlank()) continue
                    val info = StreamInfo(sUrl, mime, fmt.optInt("bitrate", 0))
                    if (bestAudio == null || info.bitrate > bestAudio.bitrate) {
                        bestAudio = info
                    }
                }

                if (bestAudio != null) return bestAudio.url

                val formatStreams = json.optJSONArray("formatStreams") ?: JSONArray()
                for (i in 0 until formatStreams.length()) {
                    val fmt = formatStreams.optJSONObject(i) ?: continue
                    val mime = fmt.optString("mimeType", "")
                    if (!mime.startsWith("audio/")) continue
                    val sUrl = fmt.optString("url", "")
                    if (sUrl.isNotBlank()) return sUrl
                }
            } catch (e: Exception) {
                Log.w(TAG, "Invidious instance $instance failed for $videoId", e)
            }
        }
        return null
    }

    private fun pipedGetAudioUrl(videoId: String): String? {
        for (instance in PIPED_INSTANCES) {
            try {
                val url = URL("$instance/streams/$videoId")
                Log.d(TAG, "Piped stream fetch: $url")
                val conn = url.openConnection() as HttpURLConnection
                conn.connectTimeout = 15000
                conn.readTimeout = 15000
                conn.setRequestProperty("Accept", "application/json")
                conn.setRequestProperty("User-Agent", USER_AGENT)

                if (conn.responseCode != 200) {
                    Log.w(TAG, "Piped instance $instance returned ${conn.responseCode}, trying next")
                    conn.disconnect()
                    continue
                }

                val reader = BufferedReader(InputStreamReader(conn.inputStream))
                val body = reader.readText()
                reader.close()
                conn.disconnect()

                val json = JSONObject(body)
                val audioStreams = json.optJSONArray("audioStreams") ?: JSONArray()

                var bestStream: StreamInfo? = null
                for (i in 0 until audioStreams.length()) {
                    val stream = audioStreams.optJSONObject(i) ?: continue
                    val sUrl = stream.optString("url", "")
                    if (sUrl.isBlank()) continue
                    val info = StreamInfo(
                        url = sUrl,
                        mimeType = stream.optString("mimeType", ""),
                        bitrate = stream.optInt("bitrate", 0)
                    )
                    if (bestStream == null || info.bitrate > bestStream.bitrate) {
                        bestStream = info
                    }
                }

                if (bestStream != null) {
                    currentPipedInstance = instance
                    return bestStream.url
                }

                val videoStreams = json.optJSONArray("videoStreams") ?: JSONArray()
                for (i in 0 until videoStreams.length()) {
                    val stream = videoStreams.optJSONObject(i) ?: continue
                    if (stream.optBoolean("videoOnly", false)) continue
                    val sUrl = stream.optString("url", "")
                    if (sUrl.isNotBlank()) {
                        currentPipedInstance = instance
                        return sUrl
                    }
                }
            } catch (e: Exception) {
                Log.w(TAG, "Piped instance $instance failed for $videoId", e)
            }
        }
        return null
    }

    fun extractPlaylistId(url: String): String? {
        val listParam = Regex("[?&]list=([a-zA-Z0-9_-]+)").find(url)
            ?.groupValues?.getOrNull(1)
        if (!listParam.isNullOrBlank()) return listParam

        if (url.matches(Regex("^[a-zA-Z0-9_-]{10,}$"))) return url

        return null
    }

    private fun parseVideoId(urlStr: String): String {
        val watchMatch = Regex("/watch\\?v=([a-zA-Z0-9_-]+)").find(urlStr)
        if (watchMatch != null) return watchMatch.groupValues[1]

        if (urlStr.matches(Regex("^[a-zA-Z0-9_-]{11}$"))) return urlStr

        return ""
    }

    data class StreamInfo(
        val url: String,
        val mimeType: String,
        val bitrate: Int
    )
}