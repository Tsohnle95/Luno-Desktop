package com.luno.mobile.playback

import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import org.schabi.newpipe.extractor.ServiceList
import org.schabi.newpipe.extractor.exceptions.ExtractionException
import org.schabi.newpipe.extractor.services.youtube.YoutubeJavaScriptPlayerManager
import org.schabi.newpipe.extractor.stream.AudioStream
import java.io.BufferedReader
import java.io.IOException
import java.io.InputStreamReader
import java.io.OutputStreamWriter
import java.io.StringReader
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLDecoder

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
    val duration: Long,
    val thumbnailUrl: String = ""
)

object WebSearchService {

    private const val TAG = "WebSearchService"

    private val USER_AGENTS = listOf(
        "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.6422.165 Mobile Safari/537.36",
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36"
    )

    private val INNERTUBE_API_KEY = "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8"
    private val INNERTUBE_BASE = "https://www.youtube.com/youtubei/v1"

    private val youtube = ServiceList.YouTube

    fun searchYouTube(query: String, limit: Int = 15): ExtractionResult<List<WebSearchResult>> {
        return try {
            val searchExtractor = youtube.getSearchExtractor(query.trim())
            searchExtractor.fetchPage()
            val items = searchExtractor.initialPage.items

            val results = items
                .filterIsInstance<org.schabi.newpipe.extractor.stream.StreamInfoItem>()
                .take(limit)
                .map { item ->
                    WebSearchResult(
                        title = item.name,
                        artist = item.uploaderName ?: "Unknown",
                        duration = item.duration,
                        thumbnailUrl = item.thumbnails?.lastOrNull()?.url ?: "",
                        videoId = extractVideoId(item.url),
                        source = "YouTube"
                    )
                }

            if (results.isEmpty()) {
                ExtractionResult.Error("No results found", "Search returned zero items")
            } else {
                ExtractionResult.Success(results)
            }
        } catch (e: IOException) {
            Log.e(TAG, "Network error searching YouTube", e)
            ExtractionResult.Error("Network error: ${e.message}", e.toString())
        } catch (e: ExtractionException) {
            Log.e(TAG, "Extraction error searching YouTube", e)
            ExtractionResult.Error("Search failed: ${e.message}", e.toString())
        } catch (e: Exception) {
            Log.e(TAG, "Unexpected error searching YouTube", e)
            ExtractionResult.Error("Unexpected error: ${e.message}", e.toString())
        }
    }

    fun getPlaylistVideos(playlistUrl: String): ExtractionResult<Pair<String, List<PlaylistVideo>>> {
        val playlistId = extractPlaylistId(playlistUrl)
        if (playlistId == null) {
            return ExtractionResult.Error("Invalid playlist URL", "Could not extract playlist ID from: $playlistUrl")
        }

        return try {
            val resolvedUrl = "https://www.youtube.com/playlist?list=$playlistId"
            val playlistExtractor = youtube.getPlaylistExtractor(resolvedUrl)
            playlistExtractor.fetchPage()

            val name = playlistExtractor.name
            val streams = playlistExtractor.initialPage.items

            val videos = streams.mapNotNull { item ->
                val vidId = extractVideoId(item.url)
                if (vidId.isBlank()) return@mapNotNull null
                PlaylistVideo(
                    videoId = vidId,
                    title = item.name,
                    artist = item.uploaderName ?: "",
                    duration = item.duration,
                    thumbnailUrl = item.thumbnails?.lastOrNull()?.url ?: ""
                )
            }

            if (videos.isEmpty()) {
                ExtractionResult.Error("No playable videos found", "Playlist has no extractable videos")
            } else {
                ExtractionResult.Success(Pair(name, videos))
            }
        } catch (e: IOException) {
            Log.e(TAG, "Network error fetching playlist", e)
            ExtractionResult.Error("Network error: ${e.message}", e.toString())
        } catch (e: ExtractionException) {
            Log.e(TAG, "Extraction error fetching playlist", e)
            ExtractionResult.Error("Playlist error: ${e.message}", e.toString())
        } catch (e: Exception) {
            Log.e(TAG, "Unexpected error fetching playlist", e)
            ExtractionResult.Error("Unexpected error: ${e.message}", e.toString())
        }
    }

    private var cachedPlayerJsUrl: String? = null

    private val INVIDIOUS_PROXIES = listOf(
        "https://invidious.drgns.space",
        "https://vid.puffyan.us",
        "https://invidious.fdn.fr",
        "https://invidious.tiekoetter.com",
        "https://iv.nboeck.de",
    )

    fun getAudioStreamUrl(videoId: String): ExtractionResult<AudioStreamInfo> {
        val newPipeResult = newpipeGetAudioUrl(videoId)
        if (newPipeResult is ExtractionResult.Success) return newPipeResult

        val invidiousResult = invidiousGetAudioUrl(videoId)
        if (invidiousResult is ExtractionResult.Success) return invidiousResult

        Log.w(TAG, "Invidious failed for $videoId — trying InnerTube")

        if (cachedPlayerJsUrl == null) {
            cachedPlayerJsUrl = extractPlayerJsUrl(videoId)
            Log.d(TAG, "Player JS URL: ${cachedPlayerJsUrl?.take(80)}")
        }

        return innertubeGetAudioUrl(videoId)
    }

    private fun invidiousGetAudioUrl(videoId: String): ExtractionResult<AudioStreamInfo> {
        for (instance in INVIDIOUS_PROXIES) {
            try {
                val json = fetchInvidiousJson(instance, videoId)
                    ?: fetchInvidiousJsonFromEmbed(instance, videoId)
                if (json == null) continue

                val urlInfo = extractStreamUrlFromResponse(json)
                if (urlInfo == null) continue

                val proxyUrl = rewriteToInvidiousProxy(urlInfo.url, instance)
                Log.d(TAG, "Invidious $instance proxy: ${proxyUrl.take(120)}")
                return ExtractionResult.Success(AudioStreamInfo(proxyUrl, urlInfo.mimeType, urlInfo.bitrate))
            } catch (e: Exception) {
                Log.w(TAG, "Invidious $instance failed for $videoId", e)
            }
        }
        return ExtractionResult.Error("Invidious: all instances failed")
    }

    private fun fetchInvidiousJson(instance: String, videoId: String): JSONObject? {
        try {
            val apiUrl = URL("$instance/api/v1/videos/$videoId")
            Log.d(TAG, "Invidious API: $apiUrl")
            val conn = apiUrl.openConnection() as HttpURLConnection
            conn.connectTimeout = 8000; conn.readTimeout = 8000
            conn.setRequestProperty("Accept", "application/json")
            conn.setRequestProperty("User-Agent", USER_AGENTS[1])
            if (conn.responseCode != 200) {
                Log.d(TAG, "Invidious $instance API returned ${conn.responseCode}")
                conn.disconnect(); return null
            }
            val json = JSONObject(conn.inputStream.bufferedReader().readText())
            conn.disconnect()
            return json
        } catch (e: Exception) { Log.d(TAG, "Invidious API fail: ${e.message}"); return null }
    }

    private fun fetchInvidiousJsonFromEmbed(instance: String, videoId: String): JSONObject? {
        try {
            val embedUrl = URL("$instance/embed/$videoId")
            Log.d(TAG, "Invidious embed: $embedUrl")
            val conn = embedUrl.openConnection() as HttpURLConnection
            conn.connectTimeout = 8000; conn.readTimeout = 8000
            conn.setRequestProperty("User-Agent", USER_AGENTS[1])
            if (conn.responseCode != 200) {
                Log.d(TAG, "Invidious embed returned ${conn.responseCode}")
                conn.disconnect(); return null
            }
            val html = conn.inputStream.bufferedReader().readText()
            conn.disconnect()

            // Parse the real player response first: it lists the formats the
            // companion can actually serve, so we can request an audio itag
            // that exists instead of blind-requesting itag=140.
            val responseObj = extractPlayerResponse(html)
            val formats = responseObj
                ?.optJSONObject("streamingData")
                ?.optJSONArray("adaptiveFormats")

            val sourceRegex = Regex("<source\\s+[^>]*src=\"([^\"]+)\"[^>]*>")
            val match = sourceRegex.find(html)
            if (match != null) {
                var src = match.groupValues[1]
                if (src.startsWith("/")) src = "$instance$src"
                if (src.contains("latest_version") || src.contains("videoplayback")) {
                    // Preserve the companion URL's `check` token (companions with
                    // verify_requests enabled reject missing/invalid checks with
                    // HTTP 400) and request an audio itag from the actual format
                    // list. `check` values are already URL-encoded in the source
                    // URL and are passed through as-is.
                    val companionUrl = buildCompanionAudioUrl(src, formats)
                    if (companionUrl != null) {
                        Log.d(TAG, "Invidious companion (check preserved): ${companionUrl.take(150)}")
                        val resp = JSONObject()
                        val fmt = JSONObject()
                        fmt.put("url", companionUrl)
                        fmt.put("mimeType", "audio/mp4")
                        fmt.put("bitrate", 128000)
                        val arr = JSONArray()
                        arr.put(fmt)
                        val sd = JSONObject()
                        sd.put("adaptiveFormats", arr)
                        resp.put("streamingData", sd)
                        return resp
                    }
                }
            }

            return responseObj
        } catch (e: Exception) { Log.d(TAG, "Invidious embed fail: ${e.message}"); return null }
    }

    /**
     * Rebuilds a companion `latest_version` URL with an audio itag chosen from
     * the formats actually present in the page (M4A/AAC, then Opus), keeping
     * the original `check` token. Falls back to itag 140 only when the page
     * exposes no format list.
     */
    private fun buildCompanionAudioUrl(sourceUrl: String, adaptiveFormats: JSONArray?): String? {
        return try {
            val parsed = URL(sourceUrl)
            val params = parsed.query?.split("&")
                ?.mapNotNull { part ->
                    val eq = part.indexOf("=")
                    if (eq > 0) part.substring(0, eq) to part.substring(eq + 1) else null
                }
                ?.toMap()
                ?: emptyMap()
            val id = params["id"] ?: return null
            val check = params["check"] ?: return null
            val itag = pickAudioItag(adaptiveFormats) ?: 140
            val base = "${parsed.protocol}://${parsed.host}${parsed.path}"
            "$base?id=$id&itag=$itag&local=true&check=$check"
        } catch (_: Exception) {
            null
        }
    }

    private fun pickAudioItag(adaptiveFormats: JSONArray?): Int? {
        if (adaptiveFormats == null) return null
        var best: Pair<Int, Int>? = null // (priority, itag)
        for (i in 0 until adaptiveFormats.length()) {
            val fmt = adaptiveFormats.optJSONObject(i) ?: continue
            val mime = fmt.optString("mimeType", "")
            if (!mime.startsWith("audio/")) continue
            val itag = fmt.optInt("itag", 0)
            if (itag <= 0) continue
            val priority = when {
                mime.contains("mp4") || mime.contains("m4a") || mime.contains("aac") -> 2
                mime.contains("opus") -> 1
                else -> 0
            }
            if (best == null || priority > best.first) best = priority to itag
        }
        return best?.second
    }

    private fun rewriteToInvidiousProxy(googlevideoUrl: String, invidiousHost: String): String {
        return try {
            val parsed = URL(googlevideoUrl)
            "$invidiousHost/videoplayback?${parsed.query}&host=${parsed.host}"
        } catch (_: Exception) {
            googlevideoUrl
        }
    }

    private fun newpipeGetAudioUrl(videoId: String): ExtractionResult<AudioStreamInfo> {
        return try {
            val url = "https://www.youtube.com/watch?v=$videoId"
            val extractor = youtube.getStreamExtractor(url)
            extractor.fetchPage()

            val audioStreams = extractor.audioStreams
            if (audioStreams.isNullOrEmpty()) {
                return ExtractionResult.Error("No audio streams available")
            }

            // Prefer M4A/AAC, then Opus, then anything else — highest bitrate
            // wins within a tier.
            val bestAudio = audioStreams
                .filter { !it.content.isNullOrBlank() }
                .maxWithOrNull(
                    compareBy(
                        { streamPriority(it) },
                        { it.averageBitrate }
                    )
                )
            if (bestAudio == null || bestAudio.content.isNullOrBlank()) {
                return ExtractionResult.Error("No usable audio stream found")
            }

            Log.d(
                TAG,
                "NewPipe extraction succeeded for $videoId " +
                    "(itag=${bestAudio.itag}, mime=${bestAudio.format?.mimeType}, " +
                    "bitrate=${bestAudio.averageBitrate})"
            )
            ExtractionResult.Success(
                AudioStreamInfo(
                    url = bestAudio.content,
                    mimeType = bestAudio.format?.mimeType ?: "audio/mp4",
                    bitrate = bestAudio.averageBitrate
                )
            )
        } catch (e: IOException) {
            ExtractionResult.Error("Network error: ${e.message}", e.toString())
        } catch (e: ExtractionException) {
            ExtractionResult.Error("Extraction error: ${e.message}", e.toString())
        } catch (e: Exception) {
            ExtractionResult.Error("Unexpected error: ${e.message}", e.toString())
        }
    }

    private fun streamPriority(stream: AudioStream): Int {
        val mime = stream.format?.mimeType ?: ""
        return when {
            mime.contains("mp4") || mime.contains("m4a") || mime.contains("aac") -> 2
            mime.contains("opus") -> 1
            else -> 0
        }
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
        ITubeClient("ANDROID_MUSIC", "7.09.52", 35, "14"),
        ITubeClient("ANDROID", "20.10.36", 35, "14", "samsung", "SM-S928B"),
        ITubeClient("WEB", "2.20260701.00.00"),
        ITubeClient("TVHTML5_SIMPLY", "7.20260701.00.00"),
    )

    private fun innertubeGetAudioUrl(videoId: String): ExtractionResult<AudioStreamInfo> {
        for (clientCfg in INNERTUBE_CLIENTS) {
            try {
                val client = JSONObject()
                client.put("clientName", clientCfg.name)
                client.put("clientVersion", clientCfg.version)
                client.put("platform", if (clientCfg.name == "WEB" || clientCfg.name == "TVHTML5_SIMPLY") "DESKTOP" else "MOBILE")
                if (clientCfg.sdk != null) client.put("androidSdkVersion", clientCfg.sdk)
                if (clientCfg.os != null) {
                    client.put("osName", "Android")
                    client.put("osVersion", clientCfg.os)
                }
                if (clientCfg.make != null) client.put("deviceMake", clientCfg.make)
                if (clientCfg.model != null) client.put("deviceModel", clientCfg.model)
                client.put("clientScreen", "WATCH")
                client.put("userAgent", USER_AGENTS[0])

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
                        put("signatureTimestamp", (System.currentTimeMillis() / 1000 - 15000).toInt())
                    })
                })

                val response = innertubePost("player", body)
                if (response == null) {
                    Log.d(TAG, "InnerTube ${clientCfg.name} returned null for $videoId")
                    continue
                }

                if (response.has("error")) {
                    Log.w(TAG, "InnerTube ${clientCfg.name} API error: ${response.optJSONObject("error")?.optString("message", "")}")
                    continue
                }

                val assetsJs = response.optJSONObject("assets")?.optString("js", "")
                val playerJsUrl = if (assetsJs.isNullOrBlank()) cachedPlayerJsUrl else assetsJs
                Log.d(TAG, "InnerTube ${clientCfg.name} player JS URL: ${playerJsUrl?.take(80) ?: "NONE"} for $videoId")

                val urlInfo = extractStreamUrlFromResponse(response, playerJsUrl)
                if (urlInfo != null) {
                    Log.d(TAG, "InnerTube ${clientCfg.name} succeeded for $videoId")
                    return ExtractionResult.Success(urlInfo)
                }

                Log.d(TAG, "InnerTube ${clientCfg.name} no stream for $videoId")
            } catch (e: Exception) {
                Log.e(TAG, "InnerTube ${clientCfg.name} failed for $videoId", e)
            }
        }

        return ExtractionResult.Error("All extraction methods failed")
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
            conn.setRequestProperty("User-Agent", USER_AGENTS[0])
            conn.setRequestProperty("Origin", "https://www.youtube.com")
            conn.setRequestProperty("Referer", "https://www.youtube.com")
            conn.setRequestProperty("Accept", "application/json")

            val writer = OutputStreamWriter(conn.outputStream, Charsets.UTF_8)
            writer.write(bodyStr)
            writer.flush()
            writer.close()

            if (conn.responseCode != 200) {
                val errBody = conn.errorStream?.bufferedReader()?.readText() ?: ""
                Log.w(TAG, "InnerTube $endpoint returned ${conn.responseCode}: ${errBody.take(300)}")
                conn.disconnect()
                return null
            }

            val response = conn.inputStream.bufferedReader().readText()
            conn.disconnect()

            JSONObject(response)
        } catch (e: Exception) {
            Log.e(TAG, "InnerTube $endpoint request failed", e)
            null
        }
    }

    private fun extractStreamUrlFromResponse(response: JSONObject, playerJsUrl: String? = null): AudioStreamInfo? {
        val streamingData = response.optJSONObject("streamingData") ?: return null

        val adaptiveFormats = streamingData.optJSONArray("adaptiveFormats") ?: JSONArray()
        var bestAudio: AudioStreamInfo? = null
        for (i in 0 until adaptiveFormats.length()) {
            val fmt = adaptiveFormats.optJSONObject(i) ?: continue
            val mime = fmt.optString("mimeType", "")
            if (!mime.startsWith("audio/")) continue
            val url = resolveStreamUrl(fmt, playerJsUrl) ?: continue
            val bitrate = fmt.optInt("bitrate", 0)
            if (bestAudio == null || bitrate > bestAudio.bitrate) {
                bestAudio = AudioStreamInfo(url, mime, bitrate)
            }
        }
        if (bestAudio != null) return bestAudio

        val formats = streamingData.optJSONArray("formats") ?: JSONArray()
        for (i in 0 until formats.length()) {
            val fmt = formats.optJSONObject(i) ?: continue
            val mime = fmt.optString("mimeType", "")
            val url = resolveStreamUrl(fmt, playerJsUrl) ?: continue
            return AudioStreamInfo(url, mime, fmt.optInt("bitrate", 0))
        }

        val hls = streamingData.optString("hlsManifestUrl", "")
        if (hls.isNotBlank()) return AudioStreamInfo(hls, "application/x-mpegURL", 0)

        val dash = streamingData.optString("dashManifestUrl", "")
        if (dash.isNotBlank()) return AudioStreamInfo(dash, "application/dash+xml", 0)

        return null
    }

    private fun resolveStreamUrl(fmt: JSONObject, playerJsUrl: String? = null): String? {
        val rawUrl = fmt.optString("url", "")
        if (rawUrl.isNotBlank()) return deobfuscateUrl(rawUrl, playerJsUrl)

        val cipherStr = fmt.optString("signatureCipher", "")
            .ifBlank { fmt.optString("cipher", "") }
        if (cipherStr.isNotBlank()) {
            val decoded = extractCipherParams(cipherStr)
            if (decoded != null) return deobfuscateUrl("${decoded.first}&${decoded.second}=${decoded.third}", playerJsUrl)
        }
        return null
    }

    private fun deobfuscateUrl(url: String, playerJsUrl: String?): String {
        var result = url
        val lsigRegex = Regex("[&?]lsig=[^&]+")
        val lsparamsRegex = Regex("[&?]lsparams=[^&]+")
        result = result.replace(lsigRegex, "").replace(lsparamsRegex, "")
        if (result != url) {
            Log.d(TAG, "Stripped login signature params from URL")
        }
        if (playerJsUrl.isNullOrBlank()) return result
        if (!result.contains("&n=") && !result.contains("?n=")) return result
        return try {
            val deobfuscated = YoutubeJavaScriptPlayerManager.getUrlWithThrottlingParameterDeobfuscated(result, playerJsUrl)
            Log.d(TAG, "Deobfuscated throttling parameter in URL")
            deobfuscated
        } catch (e: Exception) {
            Log.w(TAG, "Failed to deobfuscate throttling parameter", e)
            result
        }
    }

    private fun extractCipherParams(cipher: String): Triple<String, String, String>? {
        val params = cipher.split("&").associate {
            val eq = it.indexOf("=")
            if (eq > 0) it.substring(0, eq) to it.substring(eq + 1) else "" to it
        }
        val url = URLDecoder.decode(params["url"] ?: return null, "UTF-8")
        val sp = params["sp"] ?: "signature"
        val sig = params["s"] ?: params["sig"] ?: return null
        return Triple(url, sp, sig)
    }

    private fun extractPlayerJsUrl(videoId: String): String? {
        for (ua in USER_AGENTS) {
            try {
                val url = URL("https://www.youtube.com/embed/$videoId")
                val conn = url.openConnection() as HttpURLConnection
                conn.connectTimeout = 10000
                conn.readTimeout = 10000
                conn.setRequestProperty("User-Agent", ua)
                conn.instanceFollowRedirects = true
                if (conn.responseCode != 200) { conn.disconnect(); continue }
                val html = conn.inputStream.bufferedReader(Charsets.UTF_8).readText()
                conn.disconnect()
                val jsRegex = Regex("<script\\s+[^>]*src=\"([^\"]*player_ias\\.vflset\\.js[^\"]*)\"")
                val match = jsRegex.find(html)
                if (match != null) {
                    val jsUrl = match.groupValues[1]
                    return if (jsUrl.startsWith("//")) "https:$jsUrl" else jsUrl
                }
            } catch (_: Exception) { continue }
        }
        return null
    }

    private fun watchPageGetAudioUrl(videoId: String): ExtractionResult<AudioStreamInfo> {
        for (uaIdx in USER_AGENTS.indices) {
            val ua = USER_AGENTS[uaIdx]
            try {
                val pageUrl = URL("https://www.youtube.com/watch?v=$videoId")
                val conn = pageUrl.openConnection() as HttpURLConnection
                conn.connectTimeout = 15000
                conn.readTimeout = 15000
                conn.setRequestProperty("User-Agent", ua)
                conn.setRequestProperty("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
                conn.setRequestProperty("Accept-Language", "en-US,en;q=0.5")
                conn.instanceFollowRedirects = true

                if (conn.responseCode != 200) {
                    Log.w(TAG, "Watch page UA[$uaIdx] returned HTTP ${conn.responseCode} for $videoId")
                    conn.disconnect()
                    continue
                }

                val html = conn.inputStream.bufferedReader(Charsets.UTF_8).readText()
                conn.disconnect()
                Log.d(TAG, "Watch page UA[$uaIdx] fetched $videoId (${html.length} bytes)")

                val htmlLower = html.lowercase()
                if (htmlLower.contains("consent") && htmlLower.contains("captcha")) {
                    Log.w(TAG, "Watch page UA[$uaIdx] got bot/consent page for $videoId — snippet: ${html.take(300)}")
                    continue
                }

                val responseObj = extractPlayerResponse(html)
                if (responseObj == null) {
                    Log.w(TAG, "No player response in watch page UA[$uaIdx] for $videoId")
                    continue
                }

                val playabilityStatus = responseObj.optJSONObject("playabilityStatus")
                if (playabilityStatus != null) {
                    val status = playabilityStatus.optString("status", "")
                    if (status == "UNPLAYABLE" || status == "LOGIN_REQUIRED" || status == "ERROR") {
                        val reason = playabilityStatus.optString("reason", status)
                        Log.w(TAG, "Video $videoId is $status: $reason")
                        return ExtractionResult.Error("Video not playable: $reason")
                    }
                }

                val urlInfo = extractStreamUrlFromResponse(responseObj)
                if (urlInfo != null) {
                    Log.d(TAG, "Watch page UA[$uaIdx] succeeded for $videoId")
                    return ExtractionResult.Success(urlInfo)
                }

                Log.w(TAG, "No stream URL in watch page UA[$uaIdx] response for $videoId")
            } catch (e: Exception) {
                Log.e(TAG, "Watch page UA[$uaIdx] failed for $videoId", e)
            }
        }
        return ExtractionResult.Error("No player response found in watch page")
    }

    private fun extractPlayerResponse(html: String): JSONObject? {
        val patterns = listOf(
            Regex("ytInitialPlayerResponse\\s*="),
            Regex("window\\s*\\.\\s*ytInitialPlayerResponse\\s*="),
            Regex("player_response\\s*=\\s*'")
        )

        for (pattern in patterns) {
            val match = pattern.find(html) ?: continue

            val jsonStart = match.range.last + 1
            if (jsonStart >= html.length) continue
            val c = html[jsonStart]
            if (c == '\'') {
                val idx2 = html.indexOf("'", jsonStart + 1)
                if (idx2 < 0) continue
                val encoded = html.substring(jsonStart + 1, idx2)
                try {
                    val decoded = URLDecoder.decode(encoded, "UTF-8")
                    return JSONObject(decoded)
                } catch (_: Exception) { continue }
            }
            if (c != '{') continue

            var depth = 0
            var inString = false
            var escaped = false
            for (i in jsonStart until html.length) {
                val ch = html[i]
                if (escaped) { escaped = false; continue }
                if (ch == '\\' && inString) { escaped = true; continue }
                if (ch == '"') { inString = !inString; continue }
                if (inString) continue
                when (ch) {
                    '{' -> depth++
                    '}' -> {
                        depth--
                        if (depth == 0) {
                            return try { JSONObject(html.substring(jsonStart, i + 1)) } catch (_: Exception) { null }
                        }
                    }
                }
            }
        }

        val scriptRegex = Regex("<script[^>]*>(\\s*var\\s+)?ytInitialPlayerResponse\\s*=\\s*(\\{.+?\\});?\\s*</script>", RegexOption.DOT_MATCHES_ALL)
        val scriptMatch = scriptRegex.find(html)
        if (scriptMatch != null) {
            return try { JSONObject(scriptMatch.groupValues[2]) } catch (_: Exception) { null }
        }

        val br = BufferedReader(StringReader(html));
        var line: String?
        while (br.readLine().also { line = it } != null) {
            val l = line!!.trim()
            if (l.startsWith("ytInitialPlayerResponse")) {
                val idx = l.indexOf('{')
                if (idx >= 0) {
                    return try { JSONObject(l.substring(idx)) } catch (_: Exception) { null }
                }
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

    private fun extractVideoId(urlStr: String): String {
        val watchMatch = Regex("/watch\\?v=([a-zA-Z0-9_-]+)").find(urlStr)
        if (watchMatch != null) return watchMatch.groupValues[1]
        if (urlStr.matches(Regex("^[a-zA-Z0-9_-]{11}$"))) return urlStr
        return ""
    }
}
