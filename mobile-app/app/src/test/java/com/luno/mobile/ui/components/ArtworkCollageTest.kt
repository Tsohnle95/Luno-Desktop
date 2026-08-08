package com.luno.mobile.ui.components

import com.google.common.truth.Truth.assertThat
import com.luno.mobile.data.db.entity.Track
import org.junit.Test

class ArtworkCollageTest {
    @Test
    fun `collage skips tracks without cached artwork`() {
        val missingArtwork = Track(uri = "missing", title = "Missing")
        val artworkTracks = (1..4).map { index ->
            Track(
                uri = "art-$index",
                title = "Art $index",
                albumArtPath = "/art/$index.jpg"
            )
        }

        assertThat(collageArtworkTracks(listOf(missingArtwork) + artworkTracks))
            .containsExactlyElementsIn(artworkTracks)
            .inOrder()
    }
}
