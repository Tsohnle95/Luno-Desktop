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

@RunWith(AndroidJUnit4::class)
@Config(manifest = Config.NONE, sdk = [34])
class RecommendationArtworkServiceTest {

    private lateinit var server: MockWebServer
    private lateinit var service: RecommendationArtworkService

    @Before
    fun setUp() {
        server = MockWebServer()
        server.start()
        service = RecommendationArtworkService(
            baseUrl = server.url("/search").toString(),
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

    @Test
    fun findsLargestAvailableAlbumCover() = runBlocking {
        server.enqueue(
            MockResponse().setBody(
                """{"data":[{"album":{"cover_medium":"https://x/medium.jpg",
                    "cover_xl":"https://x/xl.jpg"}}]}"""
            )
        )

        val artwork = service.findArtwork("Artist", "Song")

        assertThat(artwork).isEqualTo("https://x/xl.jpg")
        assertThat(server.takeRequest().requestUrl!!.queryParameter("limit")).isEqualTo("1")
    }

    @Test
    fun cachesMissingArtworkResults() = runBlocking {
        server.enqueue(MockResponse().setBody("{\"data\":[]}"))

        assertThat(service.findArtwork("Artist", "Song")).isNull()
        assertThat(service.findArtwork("Artist", "Song")).isNull()
        assertThat(server.requestCount).isEqualTo(1)
    }
}
