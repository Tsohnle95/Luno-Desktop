package com.boombastic.mobile.data.artwork

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.media.MediaMetadataRetriever
import android.net.Uri
import androidx.palette.graphics.Palette
import java.io.File
import java.security.MessageDigest

/**
 * Persists and loads embedded album artwork.
 *
 * Artwork is extracted once at import/download time (via
 * [MediaMetadataRetriever.getEmbeddedPicture]), downsized to a small
 * JPEG, and cached under `filesDir/artwork/`.  The returned path string
 * is stored on the [Track][com.boombastic.mobile.data.db.entity.Track]
 * entity and surfaced to the UI as a `file://` artwork URI.
 */
object ArtworkStorage {

    private const val MAX_DIMENSION_PX = 512
    private const val JPEG_QUALITY = 90

    /** Directory holding cached artwork files (created on demand). */
    fun dir(context: Context): File =
        File(context.filesDir, "artwork").apply { mkdirs() }

    /**
     * Extracts embedded artwork from a content/document [uri] and saves it.
     * Returns the absolute file path, or `null` when the source has no
     * embedded artwork (or extraction fails).
     */
    fun saveEmbeddedArtwork(context: Context, uri: Uri): String? {
        val retriever = MediaMetadataRetriever()
        return try {
            retriever.setDataSource(context, uri)
            saveFromRetriever(context, retriever, keyFor(uri.toString()))
        } catch (_: Exception) {
            null
        } finally {
            runCatching { retriever.release() }
        }
    }

    /**
     * Extracts embedded artwork from a local file [path] and saves it.
     * Returns the absolute file path, or `null` when unavailable.
     */
    fun saveEmbeddedArtworkFromPath(context: Context, path: String): String? {
        val retriever = MediaMetadataRetriever()
        return try {
            retriever.setDataSource(path)
            saveFromRetriever(context, retriever, keyFor(path))
        } catch (_: Exception) {
            null
        } finally {
            runCatching { retriever.release() }
        }
    }

    /** Decodes a previously saved artwork file, downsampled for color analysis. */
    fun loadBitmap(context: Context, path: String?): Bitmap? {
        if (path.isNullOrBlank()) return null
        val file = File(path)
        if (!file.exists()) return null

        return try {
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeFile(path, bounds)
            val sampleSize = calculateSampleSize(bounds.outWidth, bounds.outHeight)
            BitmapFactory.decodeFile(
                path,
                BitmapFactory.Options().apply { inSampleSize = sampleSize }
            )
        } catch (_: Exception) {
            null
        }
    }

    /**
     * Dominant color of the artwork file (Palette vibrant → muted →
     * average), or `null` when the artwork cannot be decoded.
     */
    fun dominantColor(context: Context, path: String?): Int? {
        val bitmap = loadBitmap(context, path) ?: return null
        return try {
            val swatch = Palette.from(bitmap)
                .maximumColorCount(16)
                .generate()
            swatch.vibrantSwatch?.rgb
                ?: swatch.mutedSwatch?.rgb
                ?: swatch.dominantSwatch?.rgb
        } catch (_: Exception) {
            null
        }
    }

    /**
     * URI convenience wrapper: resolves a `file://` artwork URI to a path
     * before delegating to [dominantColor].
     */
    fun dominantColor(context: Context, uri: Uri?): Int? {
        if (uri == null || uri.scheme != "file") return null
        return dominantColor(context, uri.path)
    }

    private fun saveFromRetriever(
        context: Context,
        retriever: MediaMetadataRetriever,
        key: String
    ): String? {
        val picture = retriever.embeddedPicture ?: return null
        val bitmap = BitmapFactory.decodeByteArray(picture, 0, picture.size) ?: return null
        val scaled = if (maxOf(bitmap.width, bitmap.height) > MAX_DIMENSION_PX) {
            val scale = MAX_DIMENSION_PX.toFloat() / maxOf(bitmap.width, bitmap.height)
            Bitmap.createScaledBitmap(
                bitmap,
                (bitmap.width * scale).toInt().coerceAtLeast(1),
                (bitmap.height * scale).toInt().coerceAtLeast(1),
                true
            )
        } else {
            bitmap
        }

        val target = File(dir(context), "$key.jpg")
        target.outputStream().use { out ->
            scaled.compress(Bitmap.CompressFormat.JPEG, JPEG_QUALITY, out)
        }
        return target.absolutePath
    }

    private fun keyFor(source: String): String =
        MessageDigest.getInstance("SHA-256")
            .digest(source.toByteArray())
            .take(8)
            .joinToString("") { "%02x".format(it) }

    private fun calculateSampleSize(width: Int, height: Int): Int {
        if (width <= 0 || height <= 0) return 1
        var sampleSize = 1
        while (width / (sampleSize * 2) >= MAX_DIMENSION_PX ||
            height / (sampleSize * 2) >= MAX_DIMENSION_PX
        ) {
            sampleSize *= 2
        }
        return sampleSize
    }
}
