package com.luno.mobile.data.discovery

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.google.common.truth.Truth.assertThat
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.runBlocking
import okhttp3.OkHttpClient
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.After
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/**
 * Pins the desktop `DiscoveryService` port (engine.py lines 10–83):
 * two-stage track.getsimilar → artist.gettoptracks fallback, hardcoded 0.8
 * fallback match, 10-entry evict-on-exceed cache, JSON/error/network
 * handling.
 */
@RunWith(AndroidJUnit4::class)
@Config(manifest = Config.NONE, sdk = [34])
class LastfmServiceTest {

    private lateinit var server: MockWebServer
    private var keyProvider: () -> String? = { "test-api-key" }
    private lateinit var service: LastfmService

    @Before
    fun setUp() {
        server = MockWebServer()
        server.start()
        service = LastfmService(
            apiKeyProvider = { keyProvider() },
            baseUrl = server.url("/2.0/").toString(),
            client = OkHttpClient.Builder()
                .connectTimeout(2, TimeUnit.SECONDS)
                .readTimeout(2, TimeUnit.SECONDS)
                .build()
        )
    }

    @After
    fun tearDown() {
        server.shutdown()
    }

    private fun similarResponse(json: String) = MockResponse().setBody("""{"similartracks":{"track":$json}}""")

    @Test
    fun missingApiKey_returnsFailureWithoutNetwork() = runBlocking {
        keyProvider = { null }
        val result = service.getSimilar("Artist", "Title")
        assertThat(result).isInstanceOf(LastfmResult.Failure::class.java)
        assertThat(server.requestCount).isEqualTo(0)
    }

    @Test
    fun blankSeed_returnsEmptySuccessWithoutNetwork() = runBlocking {
        val result = service.getSimilar("  ", "Title")
        assertThat(result).isEqualTo(LastfmResult.Success(emptyList()))
        assertThat(server.requestCount).isEqualTo(0)
    }

    @Test
    fun similarTracks_areParsedWithQueryParams() = runBlocking {
        server.enqueue(
            similarResponse(
                """[{"name":"Song A","match":0.87,"artist":{"name":"Artist A"},
                    "image":[{"#text":"https://x/s.png","size":"small"},
                             {"#text":"https://x/l.png","size":"large"},
                             {"#text":"https://x/mega.png","size":"mega"}]}]"""
            )
        )
        val result = service.getSimilar("Artist", "Title", limit = 7)
        assertThat(result).isInstanceOf(LastfmResult.Success::class.java)
        val tracks = (result as LastfmResult.Success).tracks
        assertThat(tracks).hasSize(1)
        assertThat(tracks[0].artist).isEqualTo("Artist A")
        assertThat(tracks[0].title).isEqualTo("Song A")
        assertThat(tracks[0].match).isWithin(0.0001).of(0.87)
        // Largest image wins (mega > extralarge > large > ...).
        assertThat(tracks[0].imageUrl).isEqualTo("https://x/mega.png")

        val request = server.takeRequest()
        assertThat(request.requestUrl!!.queryParameter("method")).isEqualTo("track.getsimilar")
        assertThat(request.requestUrl!!.queryParameter("artist")).isEqualTo("Artist")
        assertThat(request.requestUrl!!.queryParameter("track")).isEqualTo("Title")
        assertThat(request.requestUrl!!.queryParameter("limit")).isEqualTo("7")
        assertThat(request.requestUrl!!.queryParameter("api_key")).isEqualTo("test-api-key")
        assertThat(server.requestCount).isEqualTo(1)
    }

    @Test
    fun similarTrack_withStringArtist_isParsed() = runBlocking {
        server.enqueue(similarResponse("""[{"name":"Song A","match":0.5,"artist":"Artist A"}]"""))
        val result = service.getSimilar("Artist", "Title")
        val tracks = (result as LastfmResult.Success).tracks
        assertThat(tracks).hasSize(1)
        assertThat(tracks[0].artist).isEqualTo("Artist A")
    }

    @Test
    fun similarTracks_deduplicatePunctuationAccentAndSpacingVariants() = runBlocking {
        server.enqueue(
            similarResponse(
                """[{"name":"Puttin' on the Ritz","match":0.9,"artist":{"name":"Taco"}},
                    {"name":"Puttin\u00a0on the Ritz","match":0.8,"artist":{"name":"tacó"}},
                    {"name":"Another Song","match":0.7,"artist":{"name":"Another Artist"}}]"""
            )
        )

        val result = service.getSimilar("Artist", "Title") as LastfmResult.Success

        assertThat(result.tracks.map { it.title })
            .containsExactly("Puttin' on the Ritz", "Another Song")
            .inOrder()
    }

    @Test
    fun legacyHttpArtworkUrl_isUpgradedToHttps() = runBlocking {
        server.enqueue(
            similarResponse(
                """[{"name":"Song A","match":0.5,"artist":"Artist A",
                    "image":[{"#text":"http://cdn.last.fm/cover.jpg","size":"large"}]}]"""
            )
        )

        val result = service.getSimilar("Artist", "Title") as LastfmResult.Success

        assertThat(result.tracks.single().imageUrl)
            .isEqualTo("https://cdn.last.fm/cover.jpg")
    }

    @Test
    fun emptySimilar_fallsBackToArtistTopTracks_withMatch080() = runBlocking {
        server.enqueue(similarResponse("[]"))
        server.enqueue(
            MockResponse().setBody(
                """{"toptracks":{"track":[{"name":"Pop Hit","match":100,"artist":{"name":"Artist A"}}]}}"""
            )
        )

        val result = service.getSimilar("Artist", "Title", limit = 5)
        assertThat(result).isInstanceOf(LastfmResult.Success::class.java)
        val tracks = (result as LastfmResult.Success).tracks
        assertThat(tracks).hasSize(1)
        assertThat(tracks[0].artist).isEqualTo("Artist A")
        assertThat(tracks[0].title).isEqualTo("Pop Hit")
        // Fallback tracks get the hardcoded desktop match score.
        assertThat(tracks[0].match).isWithin(0.0001).of(LastfmService.FALLBACK_MATCH)

        // Discard the stage-1 request; the second request is the fallback.
        server.takeRequest()
        val second = server.takeRequest(1, TimeUnit.SECONDS)!!
        assertThat(second.requestUrl!!.queryParameter("method")).isEqualTo("artist.gettoptracks")
        assertThat(second.requestUrl!!.queryParameter("track")).isNull()
        assertThat(second.requestUrl!!.queryParameter("limit")).isEqualTo("5")
        assertThat(server.requestCount).isEqualTo(2)
    }

    @Test
    fun apiError_returnsFailureWithApiMessage() = runBlocking {
        server.enqueue(
            MockResponse().setBody(
                """{"error":26,"message":"There was a temporary problem with your request. Please try again later."}"""
            )
        )
        val result = service.getSimilar("Artist", "Title")
        assertThat(result).isInstanceOf(LastfmResult.Failure::class.java)
        assertThat((result as LastfmResult.Failure).message)
            .contains("temporary problem")
        assertThat(server.requestCount).isEqualTo(1)
    }

    @Test
    fun httpError_returnsFailure() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(500))
        val result = service.getSimilar("Artist", "Title")
        assertThat(result).isInstanceOf(LastfmResult.Failure::class.java)
        assertThat((result as LastfmResult.Failure).message).contains("Could not reach")
    }

    @Test
    fun networkFailure_returnsFailure() = runBlocking {
        server.shutdown()
        val result = service.getSimilar("Artist", "Title")
        assertThat(result).isInstanceOf(LastfmResult.Failure::class.java)
        assertThat((result as LastfmResult.Failure).message).contains("Could not reach")
    }

    @Test
    fun repeatedFetch_sameSeed_hitsCache() = runBlocking {
        server.enqueue(similarResponse("""[{"name":"Song A","match":0.9,"artist":{"name":"A"}}]"""))
        val first = service.getSimilar("Artist", "Title")
        val second = service.getSimilar("Artist", "Title")
        assertThat(first).isEqualTo(second)
        assertThat(server.requestCount).isEqualTo(1)
    }

    @Test
    fun differentLimits_doNotReuseAShorterCachedRecommendationSet() = runBlocking {
        server.enqueue(
            similarResponse(
                """[{"name":"Song A","match":0.9,"artist":{"name":"A"}}]"""
            )
        )
        service.getSimilar("Artist", "Title", limit = 6)

        server.enqueue(
            similarResponse(
                """[{"name":"Song A","match":0.9,"artist":{"name":"A"}},
                    {"name":"Song B","match":0.8,"artist":{"name":"B"}}]"""
            )
        )
        val larger = service.getSimilar("Artist", "Title", limit = 100)

        assertThat(larger).isInstanceOf(LastfmResult.Success::class.java)
        assertThat((larger as LastfmResult.Success).tracks).hasSize(2)
        assertThat(server.requestCount).isEqualTo(2)
    }

    @Test
    fun emptyResults_areNotCached() = runBlocking {
        // An empty outcome is a transient API state — a Refresh must be able
        // to re-ask instead of being stuck on a cached empty list.
        server.enqueue(similarResponse("[]"))
        server.enqueue(MockResponse().setBody("""{"toptracks":{"track":[]}}"""))
        service.getSimilar("Artist", "Title")
        server.enqueue(similarResponse("[]"))
        server.enqueue(MockResponse().setBody("""{"toptracks":{"track":[]}}"""))
        service.getSimilar("Artist", "Title")
        assertThat(server.requestCount).isEqualTo(4)
    }

    @Test
    fun cache_clearsWhenExceedingTenEntries() = runBlocking {
        // Desktop parity: once the cache exceeds 10 entries it is cleared
        // entirely, so an old seed is re-fetched after the eviction.
        server.enqueue(
            similarResponse("""[{"name":"Seed A","match":0.9,"artist":{"name":"A"}}]""")
        )
        service.getSimilar("Seed A", "A")
        for (i in 2..12) {
            server.enqueue(
                similarResponse("""[{"name":"Song $i","match":0.9,"artist":{"name":"A"}}]""")
            )
            service.getSimilar("Seed $i", "A")
        }
        assertThat(server.requestCount).isEqualTo(12)

        // The 12th insert cleared the cache — the first seed must refetch.
        server.enqueue(
            similarResponse("""[{"name":"Seed A","match":0.9,"artist":{"name":"A"}}]""")
        )
        val refetch = service.getSimilar("Seed A", "A")
        assertThat(refetch).isInstanceOf(LastfmResult.Success::class.java)
        assertThat(server.requestCount).isEqualTo(13)
    }

    @Test
    fun clearCache_forcesRefetch() = runBlocking {
        server.enqueue(similarResponse("""[{"name":"Song A","match":0.9,"artist":{"name":"A"}}]"""))
        service.getSimilar("Artist", "Title")
        service.clearCache()
        server.enqueue(similarResponse("""[{"name":"Song A","match":0.9,"artist":{"name":"A"}}]"""))
        service.getSimilar("Artist", "Title")
        assertThat(server.requestCount).isEqualTo(2)
    }
}
