package com.luno.mobile.data.repository

import com.luno.mobile.data.db.entity.Track

/** A set of library tracks that share the desktop duplicate-detection key. */
data class DuplicateGroup(
    val key: String,
    val tracks: List<Track>
)

/**
 * Finds duplicate songs by normalized artist + title.  The normalization is
 * intentionally shared with the desktop checker: common release tags and
 * punctuation should not hide copies of the same song.
 */
fun findDuplicateGroups(tracks: List<Track>): List<DuplicateGroup> = tracks
    .groupBy { normalizeForDuplicate("${it.artist}${it.title}") }
    .filter { (key, group) -> key.isNotEmpty() && group.size > 1 }
    .map { (key, group) -> DuplicateGroup(key, group) }
    .sortedBy { it.tracks.first().title.lowercase() }

internal fun normalizeForDuplicate(value: String): String = value
    .lowercase()
    .replace(DUPLICATE_PARENTHETICAL_TAGS, "")
    .replace(DUPLICATE_BRACKET_TAGS, "")
    .replace(FEATURING_VARIANTS, "feat")
    .replace(FILE_EXTENSION, "")
    .replace(NON_ALPHANUMERIC, "")

private val DUPLICATE_PARENTHETICAL_TAGS = Regex(
    """\((?:official\s*(?:audio|video|music\s*video|lyric\s*video|visualizer)?|lyrics?|audio|hd|hq|4k|remaster(?:ed)?|clean|explicit|radio\s*edit|extended\s*(?:mix|version)?|music\s*video|visualizer|bonus\s*track|deluxe|live|acoustic|remix)\)""",
    RegexOption.IGNORE_CASE
)

private val DUPLICATE_BRACKET_TAGS = Regex(
    """\[(?:official\s*(?:audio|video|music\s*video|lyric\s*video|visualizer)?|lyrics?|audio|hd|hq|4k|remaster(?:ed)?|clean|explicit|radio\s*edit|extended\s*(?:mix|version)?|music\s*video|visualizer|bonus\s*track|deluxe|live|acoustic|remix)\]""",
    RegexOption.IGNORE_CASE
)

private val FEATURING_VARIANTS = Regex("\\b(?:feat\\.?|ft\\.?|featuring)\\b")
private val FILE_EXTENSION = Regex("\\.(mp3|m4a|wav|flac|mp4|webm)$")
private val NON_ALPHANUMERIC = Regex("[^a-z0-9]")
