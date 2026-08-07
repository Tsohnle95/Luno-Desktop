package com.luno.mobile.ui.home

import com.google.common.truth.Truth.assertThat
import com.luno.mobile.data.db.entity.DownloadJob
import com.luno.mobile.data.db.entity.DownloadState
import com.luno.mobile.data.db.entity.Track
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

    @Test
    fun recentlyDownloadedTracks_returnsNewestCompletedLibraryTracksAndCapsAt50() {
        val tracks = (1..51).map { index ->
            Track(uri = "track://$index", title = "Track $index")
        }
        val jobs = tracks.mapIndexed { index, track ->
            DownloadJob(
                id = (index + 1).toLong(),
                sourceUrl = "https://example.com/${index + 1}",
                title = track.title,
                state = DownloadState.COMPLETED,
                localUri = track.uri,
                addedAt = index.toLong(),
                completedAt = index.toLong()
            )
        }

        val result = recentlyDownloadedTracks(
            downloads = jobs,
            tracksByUri = tracks.associateBy { it.uri }
        )

        assertThat(result).hasSize(50)
        assertThat(result.first().uri).isEqualTo("track://51")
        assertThat(result.last().uri).isEqualTo("track://2")
        assertThat(result).doesNotContain(tracks.first())
    }

    @Test
    fun recentlyDownloadedTracks_ignoresIncompleteAndMissingLibraryJobs() {
        val track = Track(uri = "track://completed", title = "Completed")
        val jobs = listOf(
            DownloadJob(
                id = 1,
                sourceUrl = "completed",
                title = "Completed",
                state = DownloadState.COMPLETED,
                localUri = track.uri,
                completedAt = 3
            ),
            DownloadJob(
                id = 2,
                sourceUrl = "queued",
                title = "Queued",
                state = DownloadState.QUEUED,
                localUri = "track://queued",
                completedAt = 4
            ),
            DownloadJob(
                id = 3,
                sourceUrl = "missing",
                title = "Missing",
                state = DownloadState.COMPLETED,
                localUri = "track://missing",
                completedAt = 5
            )
        )

        assertThat(recentlyDownloadedTracks(jobs, mapOf(track.uri to track)))
            .containsExactly(track)
    }
}
