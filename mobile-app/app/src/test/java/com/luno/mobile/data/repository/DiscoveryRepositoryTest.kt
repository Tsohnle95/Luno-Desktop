package com.luno.mobile.data.repository

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.luno.mobile.data.db.entity.Track
import com.luno.mobile.data.discovery.LastfmResult
import com.luno.mobile.data.discovery.LastfmService
import com.luno.mobile.data.discovery.LastfmTrack
import com.google.common.truth.Truth.assertThat
import kotlinx.coroutines.runBlocking
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/**
 * Pins the DiscoveryRepository contract: encrypted key storage round-trip,
 * library cross-referencing (desktop `_update_discovery_results` parity —
 * normalized "artist - title" matching), and fetch delegation.
 */
@RunWith(AndroidJUnit4::class)
@Config(manifest = Config.NONE, sdk = [34])
class DiscoveryRepositoryTest {

    private class FakeLastfmService : LastfmService(apiKeyProvider = { null }, baseUrl = "http://unused/") {
        var result: LastfmResult = LastfmResult.Success(emptyList())
        var lastArtist: String? = null
        var lastTitle: String? = null
        var lastLimit: Int = -1

        override suspend fun getSimilar(artist: String, title: String, limit: Int): LastfmResult {
            lastArtist = artist
            lastTitle = title
            lastLimit = limit
            return result
        }
    }

    private lateinit var context: Context
    private lateinit var fakeService: FakeLastfmService
    private lateinit var repository: DiscoveryRepository

    @Before
    fun setUp() {
        context = ApplicationProvider.getApplicationContext()
        fakeService = FakeLastfmService()
        repository = DiscoveryRepository(context, fakeService)
    }

    private fun track(title: String, artist: String) =
        Track(uri = "content://test/$artist-$title", title = title, artist = artist)

    @Test
    fun setApiKey_trimsAndPersists() = runBlocking {
        repository.setApiKey("  abc123  ")
        assertThat(repository.apiKey.value).isEqualTo("abc123")

        // Persisted across repository instances (same SharedPreferences file).
        val fresh = DiscoveryRepository(context, fakeService)
        assertThat(fresh.apiKey.value).isEqualTo("abc123")
    }

    @Test
    fun setApiKey_blankClearsKey() {
        repository.setApiKey("key")
        repository.setApiKey("   ")
        assertThat(repository.apiKey.value).isNull()
    }

    @Test
    fun clearApiKey_removesKey() {
        repository.setApiKey("key")
        repository.clearApiKey()
        assertThat(repository.apiKey.value).isNull()
    }

    @Test
    fun getSimilar_filtersTracksAlreadyInLibrary() {
        runBlocking {
            fakeService.result = LastfmResult.Success(
                listOf(
                    LastfmTrack(artist = "Artist A", title = "Song A", match = 0.9),
                    LastfmTrack(artist = "Artist B", title = "Song B", match = 0.8),
                    LastfmTrack(artist = "Artist C", title = "Song C", match = 0.7)
                )
            )
            val library = listOf(
                // Case/whitespace-insensitive match on the normalized key.
                track(title = "  song A ", artist = "ARTIST a"),
                track(title = "Song B", artist = "Artist B")
            )

            val result = repository.getSimilar("Artist", "Title", libraryTracks = library)

            assertThat(result).isInstanceOf(LastfmResult.Success::class.java)
            val tracks = (result as LastfmResult.Success).tracks
            assertThat(tracks.map { it.title }).containsExactly("Song C")
        }
    }

    @Test
    fun getSimilar_filtersPunctuationAndAccentVariantsAlreadyInLibrary() {
        runBlocking {
            fakeService.result = LastfmResult.Success(
                listOf(
                    LastfmTrack(artist = "Beyonce", title = "Halo Live", match = 0.9),
                    LastfmTrack(artist = "Another Artist", title = "Another Song", match = 0.8)
                )
            )

            val library = listOf(track(title = "Halo (Live)", artist = "Beyoncé"))

            val result = repository.getSimilar("Artist", "Title", libraryTracks = library)

            assertThat((result as LastfmResult.Success).tracks.map { it.title })
                .containsExactly("Another Song")
        }
    }

    @Test
    fun getSimilar_deduplicatesEquivalentRecommendationMetadata() = runBlocking {
        fakeService.result = LastfmResult.Success(
            listOf(
                LastfmTrack(artist = "Taco", title = "Puttin' on the Ritz", match = 0.9),
                LastfmTrack(artist = " tacó ", title = "Puttin\u00a0on the Ritz", match = 0.8),
                LastfmTrack(artist = "Another Artist", title = "Another Song", match = 0.7)
            )
        )

        val result = repository.getSimilar("Artist", "Title") as LastfmResult.Success

        assertThat(result.tracks.map { it.title })
            .containsExactly("Puttin' on the Ritz", "Another Song")
            .inOrder()
    }

    @Test
    fun getSimilar_noLibrary_returnsEverything() = runBlocking {
        fakeService.result = LastfmResult.Success(
            listOf(LastfmTrack(artist = "Artist A", title = "Song A", match = 0.9))
        )
        val result = repository.getSimilar("Artist", "Title")
        assertThat((result as LastfmResult.Success).tracks).hasSize(1)
    }

    @Test
    fun getSimilar_passesThroughFailure() = runBlocking {
        fakeService.result = LastfmResult.Failure("There was a temporary problem")
        val result = repository.getSimilar("Artist", "Title")
        assertThat(result).isInstanceOf(LastfmResult.Failure::class.java)
        assertThat((result as LastfmResult.Failure).message).contains("temporary problem")
    }

    @Test
    fun getSimilar_forwardsSeedAndLimit() = runBlocking {
        repository.getSimilar("Seed Artist", "Seed Title", limit = 5)
        assertThat(fakeService.lastArtist).isEqualTo("Seed Artist")
        assertThat(fakeService.lastTitle).isEqualTo("Seed Title")
        assertThat(fakeService.lastLimit).isEqualTo(5)
    }
}
