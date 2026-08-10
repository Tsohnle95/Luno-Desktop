package com.luno.mobile.playback

/**
 * Human-readable context that supplied the current playback queue.
 *
 * The source is deliberately metadata rather than navigation state: the full
 * player can stay open while the user navigates elsewhere, and Media3 can
 * advance through a queue without Compose remembering which screen created it.
 */
data class PlaybackSource(
    val category: String,
    val name: String? = null,
    val playlistId: Long? = null
) {
    fun displayLabel(): String = listOfNotNull(
        category.trim().takeIf { it.isNotEmpty() },
        name?.trim()?.takeIf { it.isNotEmpty() }
    ).joinToString(" · ").ifBlank { "Player" }
}
