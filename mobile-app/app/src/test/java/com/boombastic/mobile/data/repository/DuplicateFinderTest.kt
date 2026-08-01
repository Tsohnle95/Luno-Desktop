package com.boombastic.mobile.data.repository

import com.boombastic.mobile.data.db.entity.Track
import com.google.common.truth.Truth.assertThat
import org.junit.Test

class DuplicateFinderTest {
    @Test
    fun groupsMatchingSongsAcrossCommonReleaseTags() {
        val tracks = listOf(
            Track("content://one", "Song (Official Audio)", "Artist"),
            Track("content://two", "song", "artist"),
            Track("content://three", "Different song", "Artist")
        )

        val groups = findDuplicateGroups(tracks)

        assertThat(groups).hasSize(1)
        assertThat(groups.single().tracks.map { it.uri })
            .containsExactly("content://one", "content://two")
    }

    @Test
    fun normalizesFeaturingVariantsAndPunctuation() {
        val tracks = listOf(
            Track("content://one", "Track feat. Guest", "The Artist"),
            Track("content://two", "Track ft Guest", "the-artist")
        )

        assertThat(findDuplicateGroups(tracks)).hasSize(1)
    }

    @Test
    fun ignoresUniqueAndBlankKeys() {
        val tracks = listOf(
            Track("content://one", "Only song", "Artist"),
            Track("content://two", "", "")
        )

        assertThat(findDuplicateGroups(tracks)).isEmpty()
    }
}
