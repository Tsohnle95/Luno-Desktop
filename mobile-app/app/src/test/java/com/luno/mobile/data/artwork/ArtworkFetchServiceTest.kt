package com.luno.mobile.data.artwork

import android.content.Context
import android.graphics.Bitmap
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.google.common.truth.Truth.assertThat
import java.io.ByteArrayOutputStream
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.runBlocking
import okhttp3.OkHttpClient
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okio.Buffer
import org.junit.After
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

@RunWith(AndroidJUnit4::class)
@Config(manifest = Config.NONE, sdk = [34])
class ArtworkFetchServiceTest {

    private lateinit var context: Context
    private lateinit var server: MockWebServer
    private lateinit var service: ArtworkFetchService

    @Before
    fun setUp() {
        context = ApplicationProvider.getApplicationContext()
        server = MockWebServer()
        server.start()
        service = ArtworkFetchService(
            baseUrl = server.url("/search").toString(),
            client = OkHttpClient.Builder()
                .connectTimeout(2, TimeUnit.SECONDS)
                .readTimeout(2, TimeUnit.SECONDS)
                .addInterceptor { chain ->
                    val request = chain.request()
                    if (request.url.host == "cover.test") {
                        chain.proceed(
                            request.newBuilder()
                                .url(server.url("/cover.jpg"))
                                .build()
                        )
                    } else {
                        chain.proceed(request)
                    }
                }
                .build()
        )
    }

    @After
    fun tearDown() {
        server.shutdown()
    }

    @Test
    fun searchesForTrackAndPersistsUsableCover() = runBlocking {
        server.enqueue(
            MockResponse().setBody(
                """{"data":[{"album":{"cover_xl":"https://cover.test/cover.jpg"}}]}"""
            )
        )
        server.enqueue(
            MockResponse()
                .setHeader("Content-Type", "image/jpeg")
                .setBody(Buffer().write(jpegBytes()))
        )

        val path = service.fetchAndSave(context, "Artist", "Song")

        assertThat(path).isNotNull()
        assertThat(ArtworkStorage.hasUsableArtwork(path)).isTrue()
        assertThat(server.takeRequest().requestUrl!!.queryParameter("limit")).isEqualTo("1")
        assertThat(server.takeRequest().path).isEqualTo("/cover.jpg")
    }

    @Test
    fun fallsBackToTitleSearchWhenCombinedSearchHasNoResult() = runBlocking {
        server.enqueue(MockResponse().setBody("{\"data\":[]}"))
        server.enqueue(
            MockResponse().setBody(
                """{"data":[{"album":{"cover_medium":"https://cover.test/cover.jpg"}}]}"""
            )
        )
        server.enqueue(
            MockResponse()
                .setHeader("Content-Type", "image/jpeg")
                .setBody(Buffer().write(jpegBytes()))
        )

        val path = service.fetchAndSave(context, "Artist", "Song")

        assertThat(path).isNotNull()
        assertThat(ArtworkStorage.hasUsableArtwork(path)).isTrue()
        assertThat(server.requestCount).isEqualTo(3)
    }

    private fun jpegBytes(): ByteArray {
        val bitmap = Bitmap.createBitmap(2, 2, Bitmap.Config.ARGB_8888)
        return ByteArrayOutputStream().use { output ->
            bitmap.compress(Bitmap.CompressFormat.JPEG, 90, output)
            bitmap.recycle()
            output.toByteArray()
        }
    }
}
