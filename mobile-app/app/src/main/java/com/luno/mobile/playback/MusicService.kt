package com.luno.mobile.playback

import android.app.PendingIntent
import android.content.Intent
import android.os.Bundle
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

    companion object {
        const val ACTION_OPEN_PLAYER = "com.luno.mobile.action.OPEN_PLAYER"
        private const val PLAYBACK_PREFS = "playback_restore"
        private const val KEY_URI = "uri"
        private const val KEY_TITLE = "title"
        private const val KEY_ARTIST = "artist"
        private const val KEY_ALBUM = "album"
        private const val KEY_DURATION_MS = "duration_ms"
        private const val KEY_ARTWORK_URI = "artwork_uri"
        private const val KEY_SOURCE_CATEGORY = "source_category"
        private const val KEY_SOURCE_NAME = "source_name"
        private const val KEY_SOURCE_PLAYLIST_ID = "source_playlist_id"
        private const val KEY_POSITION_MS = "position_ms"
        private const val KEY_PLAY_WHEN_READY = "play_when_ready"
    }

    private var mediaSession: MediaSession? = null
    private var player: ExoPlayer? = null
    private val artworkExecutor = Executors.newSingleThreadExecutor()
    private val playbackPersistenceListener = object : Player.Listener {
        override fun onMediaItemTransition(mediaItem: MediaItem?, reason: Int) = persistPlayback()
        override fun onIsPlayingChanged(isPlaying: Boolean) = persistPlayback()
        override fun onPlaybackStateChanged(playbackState: Int) = persistPlayback()
        override fun onPositionDiscontinuity(
            oldPosition: Player.PositionInfo,
            newPosition: Player.PositionInfo,
            reason: Int
        ) = persistPlayback()
    }

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

        val p = player ?: return
        restoreLastPlayback(p)
        p.addListener(playbackPersistenceListener)

        val intent = Intent(this, MainActivity::class.java).apply {
            action = ACTION_OPEN_PLAYER
            flags = Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP
        }
        val pendingIntent = PendingIntent.getActivity(
            this, 0, intent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )

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
        persistPlayback()
        if (p == null || p.mediaItemCount == 0) {
            stopSelf()
        }
    }

    override fun onDestroy() {
        persistPlayback()
        // Release session BEFORE player (Media3 contract: session holds player reference).
        // Fields are nulled immediately so cleanup is idempotent.
        mediaSession?.run {
            release()
            mediaSession = null
        }
        player?.run {
            removeListener(playbackPersistenceListener)
            release()
            player = null
        }
        artworkExecutor.shutdown()
        super.onDestroy()
    }

    private fun persistPlayback() {
        val currentPlayer = player ?: return
        val item = currentPlayer.currentMediaItem ?: return
        if (item.mediaMetadata.extras
                ?.getBoolean(MusicController.METADATA_TRANSIENT) == true
        ) {
            return
        }
        val metadata = item.mediaMetadata
        getSharedPreferences(PLAYBACK_PREFS, MODE_PRIVATE).edit()
            .putString(KEY_URI, item.mediaId)
            .putString(KEY_TITLE, metadata.title?.toString().orEmpty())
            .putString(KEY_ARTIST, metadata.artist?.toString().orEmpty())
            .putString(KEY_ALBUM, metadata.albumTitle?.toString().orEmpty())
            .putLong(
                KEY_DURATION_MS,
                metadata.extras?.getLong(MusicController.METADATA_DURATION_MS) ?: 0L
            )
            .putString(KEY_ARTWORK_URI, metadata.artworkUri?.toString())
            .putString(
                KEY_SOURCE_CATEGORY,
                metadata.extras?.getString(MusicController.METADATA_SOURCE_CATEGORY)
            )
            .putString(
                KEY_SOURCE_NAME,
                metadata.extras?.getString(MusicController.METADATA_SOURCE_NAME)
            )
            .putLong(
                KEY_SOURCE_PLAYLIST_ID,
                metadata.extras
                    ?.takeIf { it.containsKey(MusicController.METADATA_PLAYLIST_ID) }
                    ?.getLong(MusicController.METADATA_PLAYLIST_ID)
                    ?: 0L
            )
            .putLong(KEY_POSITION_MS, currentPlayer.currentPosition.coerceAtLeast(0L))
            .putBoolean(
                KEY_PLAY_WHEN_READY,
                currentPlayer.playWhenReady && currentPlayer.playbackState != Player.STATE_ENDED
            )
            .apply()
    }

    private fun restoreLastPlayback(currentPlayer: ExoPlayer) {
        val preferences = getSharedPreferences(PLAYBACK_PREFS, MODE_PRIVATE)
        val uri = preferences.getString(KEY_URI, null)?.takeIf { it.isNotBlank() } ?: return
        val track = MediaTrack(
            uri = uri,
            title = preferences.getString(KEY_TITLE, null).orEmpty().ifBlank { "Unknown" },
            artist = preferences.getString(KEY_ARTIST, null).orEmpty().ifBlank { "Unknown" },
            album = preferences.getString(KEY_ALBUM, null).orEmpty(),
            durationMs = preferences.getLong(KEY_DURATION_MS, 0L),
            artworkUri = preferences.getString(KEY_ARTWORK_URI, null),
            playbackSource = preferences.getString(KEY_SOURCE_CATEGORY, null)
                ?.takeIf { it.isNotBlank() }
                ?.let { category ->
                    PlaybackSource(
                        category = category,
                        name = preferences.getString(KEY_SOURCE_NAME, null),
                        playlistId = preferences.getLong(KEY_SOURCE_PLAYLIST_ID, 0L)
                            .takeIf { it != 0L }
                    )
                }
        )
        val extras = Bundle().apply {
            putLong(MusicController.METADATA_DURATION_MS, track.durationMs)
            putBoolean(MusicController.METADATA_ENRICH_ARTWORK, true)
            track.playbackSource?.let { source ->
                putString(MusicController.METADATA_SOURCE_CATEGORY, source.category)
                source.name?.let { putString(MusicController.METADATA_SOURCE_NAME, it) }
                source.playlistId?.let { putLong(MusicController.METADATA_PLAYLIST_ID, it) }
            }
        }
        val metadata = MediaMetadata.Builder()
            .setTitle(track.title)
            .setArtist(track.artist)
            .setAlbumTitle(track.album)
            .setExtras(extras)
            .apply { track.artworkUri?.let { setArtworkUri(android.net.Uri.parse(it)) } }
            .build()
        currentPlayer.setMediaItem(
            MediaItem.Builder()
                .setMediaId(track.uri)
                .setUri(track.uri)
                .setMediaMetadata(metadata)
                .build()
        )
        currentPlayer.prepare()
        currentPlayer.seekTo(preferences.getLong(KEY_POSITION_MS, 0L).coerceAtLeast(0L))
        currentPlayer.playWhenReady = preferences.getBoolean(KEY_PLAY_WHEN_READY, false)
    }
}
