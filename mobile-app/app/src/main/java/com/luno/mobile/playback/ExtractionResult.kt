package com.luno.mobile.playback

sealed class ExtractionResult<out T> {
    data class Success<T>(val data: T) : ExtractionResult<T>()
    data class Error(val message: String, val details: String? = null) : ExtractionResult<Nothing>()
}

data class AudioStreamInfo(
    val url: String,
    val mimeType: String,
    val bitrate: Int
)
