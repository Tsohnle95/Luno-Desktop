package com.luno.mobile.playback

import android.app.PendingIntent
import android.content.Intent
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import com.luno.mobile.MainActivity
import com.google.common.util.concurrent.ListenableFuture
import com.google.common.util.concurrent.SettableFuture
import java.util.concurrent.Executors

class MusicService : MediaSessionService() {

    private var mediaSession: MediaSession? = null
    private var player: ExoPlayer? = null
    private val artworkExecutor = Executors.newSingleThreadExecutor()

    override fun onCreate() {
        super.onCreate()

        val audioAttributes = AudioAttributes.Builder()
            .setContentType(C.AUDIO_CONTENT_TYPE_MUSIC)
            .setUsage(C.USAGE_MEDIA)
            .build()

        player = ExoPlayer.Builder(this)
            .setAudioAttributes(audioAttributes, true)
            .setHandleAudioBecomingNoisy(true)
            .build()

        player?.playWhenReady = true

        val intent = Intent(this, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_SINGLE_TOP
        }
        val pendingIntent = PendingIntent.getActivity(
            this, 0, intent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )

        val p = player ?: return
        mediaSession = MediaSession.Builder(this, p)
            .setSessionActivity(pendingIntent)
            .setCallback(ArtworkEnrichingCallback())
            .build()
    }

    /**
     * Enriches incoming media items with `artworkData` bytes loaded from
     * their `artworkUri` (cached embedded artwork).  The media-session
     * notification and lock-screen controls render artwork from
     * `artworkData`, so without this enrichment those surfaces would
     * show no artwork even though the in-app UI loads the same file.
     *
     * Only items flagged `METADATA_ENRICH_ARTWORK` by [MusicController]
     * (the play-start item plus a small window around it, and manually
     * queued items) are enriched.  Loading artworkData for **every** item
     * of a large context queue (this library: ~170MB of cached artwork
     * across 4k+ tracks) holds the whole library in memory at once and
     * gets the process LMK/OOM-killed.
     */
    private inner class ArtworkEnrichingCallback : MediaSession.Callback {
        @androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
        override fun onAddMediaItems(
            mediaSession: MediaSession,
            controller: MediaSession.ControllerInfo,
            mediaItems: List<MediaItem>
        ): ListenableFuture<List<MediaItem>> {
            val output = SettableFuture.create<List<MediaItem>>()
            artworkExecutor.execute {
                try {
                    val enriched = mediaItems.map { item ->
                        val metadata = item.mediaMetadata
                        val artworkUri = metadata.artworkUri
                        val shouldEnrich = metadata.extras
                            ?.getBoolean(MusicController.METADATA_ENRICH_ARTWORK) == true
                        if (shouldEnrich && artworkUri != null && metadata.artworkData == null) {
                            val bytes = try {
                                contentResolver.openInputStream(artworkUri)?.use { it.readBytes() }
                            } catch (_: Exception) {
                                null
                            }
                            if (bytes != null && bytes.isNotEmpty()) {
                                item.buildUpon()
                                    .setMediaMetadata(
                                        metadata.buildUpon()
                                            .setArtworkData(bytes, MediaMetadata.PICTURE_TYPE_FRONT_COVER)
                                            .build()
                                    )
                                    .build()
                            } else {
                                item
                            }
                        } else {
                            item
                        }
                    }
                    output.set(enriched)
                } catch (e: Exception) {
                    output.setException(e)
                }
            }
            return output
        }
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? {
        return mediaSession
    }

    override fun onTaskRemoved(rootIntent: Intent?) {
        val p = player
        if (p == null || !p.playWhenReady || p.mediaItemCount == 0) {
            stopSelf()
        }
    }

    override fun onDestroy() {
        // Release session BEFORE player (Media3 contract: session holds player reference).
        // Fields are nulled immediately so cleanup is idempotent.
        mediaSession?.run {
            release()
            mediaSession = null
        }
        player?.run {
            release()
            player = null
        }
        artworkExecutor.shutdown()
        super.onDestroy()
    }
}
