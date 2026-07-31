package com.boombastic.mobile.playback

import android.util.Log
import org.schabi.newpipe.extractor.ServiceList
import org.schabi.newpipe.extractor.exceptions.ExtractionException
import java.io.IOException

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
    private val youtube = ServiceList.YouTube

    fun searchYouTube(query: String, limit: Int = 15): ExtractionResult<List<WebSearchResult>> {
        return try {
            val searchExtractor = youtube.getSearchExtractor(query.trim())
            searchExtractor.fetchPage()
            val page = searchExtractor.initialPage
            val items = page.items

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
            val page = playlistExtractor.initialPage
            val streams = page.items

            val videos = streams.mapNotNull { item ->
                val vidId = extractVideoId(item.url)
                if (vidId.isBlank()) return@mapNotNull null
                PlaylistVideo(
                    videoId = vidId,
                    title = item.name,
                    artist = item.uploaderName ?: "",
                    duration = item.duration
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

    fun getAudioStreamUrl(videoId: String): ExtractionResult<AudioStreamInfo> {
        return try {
            val url = "https://www.youtube.com/watch?v=$videoId"
            val extractor = youtube.getStreamExtractor(url)
            extractor.fetchPage()

            val audioStreams = extractor.audioStreams
            if (audioStreams.isNullOrEmpty()) {
                return ExtractionResult.Error(
                    "No audio streams available",
                    "Video $videoId returned no audio streams"
                )
            }

            val bestAudio = audioStreams.maxByOrNull { it.averageBitrate }
            if (bestAudio == null || bestAudio.content.isNullOrBlank()) {
                return ExtractionResult.Error(
                    "No usable audio stream found",
                    "Video $videoId audio streams had no content URL"
                )
            }

            ExtractionResult.Success(
                AudioStreamInfo(
                    url = bestAudio.content,
                    mimeType = bestAudio.format?.mimeType ?: "audio/mp4",
                    bitrate = bestAudio.averageBitrate
                )
            )
        } catch (e: IOException) {
            Log.e(TAG, "Network error extracting audio URL", e)
            ExtractionResult.Error("Network error: ${e.message}", e.toString())
        } catch (e: ExtractionException) {
            Log.e(TAG, "Extraction error for audio URL", e)
            val status = e.message ?: "Unknown extraction error"
            ExtractionResult.Error("Extraction error: $status", e.toString())
        } catch (e: Exception) {
            Log.e(TAG, "Unexpected error extracting audio URL", e)
            ExtractionResult.Error("Unexpected error: ${e.message}", e.toString())
        }
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
