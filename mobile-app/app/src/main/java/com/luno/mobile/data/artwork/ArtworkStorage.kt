package com.luno.mobile.data.artwork

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
 * is stored on the [Track][com.luno.mobile.data.db.entity.Track]
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

    /**
     * Decodes raw image [bytes] (e.g. a fetched YouTube video thumbnail),
     * downsizes them like embedded artwork, and saves them to the cache.
     * Returns the absolute file path, or `null` when the bytes are not a
     * decodable image.
     */
    fun saveImageBytes(context: Context, bytes: ByteArray): String? {
        val bitmap = decodeSized(bytes) ?: return null
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

        val target = File(dir(context), "${keyFor(scaled.hashCode().toString())}.jpg")
        target.outputStream().use { out ->
            scaled.compress(Bitmap.CompressFormat.JPEG, JPEG_QUALITY, out)
        }
        return target.absolutePath
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

    /** True only when [path] points to a readable image, not just an old path. */
    fun hasUsableArtwork(path: String?): Boolean {
        if (path.isNullOrBlank()) return false
        val file = File(path)
        if (!file.isFile || file.length() == 0L) return false
        return try {
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeFile(file.absolutePath, bounds)
            bounds.outWidth > 0 && bounds.outHeight > 0
        } catch (_: Exception) {
            false
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
        val bitmap = decodeSized(picture) ?: return null
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

    /**
     * Bounds-checked decode: reads the image dimensions first and samples
     * down before allocating the bitmap, so a huge embedded picture (some
     * albums embed 3000px+ art) can never OOM the process — an
     * `OutOfMemoryError` would otherwise escape the per-file `catch` in
     * the import loop and kill the whole import.
     */
    private fun decodeSized(bytes: ByteArray): Bitmap? {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null
        val sampleSize = calculateSampleSize(bounds.outWidth, bounds.outHeight)
        return BitmapFactory.decodeByteArray(
            bytes,
            0,
            bytes.size,
            BitmapFactory.Options().apply { inSampleSize = sampleSize }
        )
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
