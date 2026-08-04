package com.luno.mobile.ui.home

import com.luno.mobile.data.db.entity.Track
import com.google.common.truth.Truth.assertThat
import org.junit.Test

class HomeScreenTest {
    @Test
    fun catalogueKeyIgnoresMutableTrackMetadata() {
        val before = listOf(
            Track(uri = "track://one", title = "One"),
            Track(uri = "track://two", title = "Two")
        )
        val after = before.map {
            it.copy(
                title = "Updated ${it.title}",
                playCount = it.playCount + 1,
                albumArtPath = "/art/${it.uri.substringAfterLast('/')}.jpg"
            )
        }

        assertThat(homeCatalogueKey(after))
            .containsExactlyElementsIn(homeCatalogueKey(before))
            .inOrder()
    }

    @Test
    fun catalogueKeyChangesWhenLibraryMembershipChanges() {
        val before = listOf(Track(uri = "track://one", title = "One"))
        val after = before + Track(uri = "track://two", title = "Two")

        assertThat(homeCatalogueKey(after))
            .containsExactly("track://one", "track://two")
            .inOrder()
    }

    @Test
    fun catalogueKeyIgnoresRoomOrderingChanges() {
        val firstOrder = listOf(
            Track(uri = "track://two", title = "Two"),
            Track(uri = "track://one", title = "One")
        )
        val secondOrder = firstOrder.asReversed()

        assertThat(homeCatalogueKey(firstOrder))
            .containsExactlyElementsIn(homeCatalogueKey(secondOrder))
            .inOrder()
    }
}
