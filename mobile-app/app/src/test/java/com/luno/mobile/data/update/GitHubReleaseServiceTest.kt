package com.luno.mobile.data.update

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
class GitHubReleaseServiceTest {
    private lateinit var server: MockWebServer
    private lateinit var service: GitHubReleaseService

    @Before
    fun setUp() {
        server = MockWebServer()
        server.start()
        service = GitHubReleaseService(
            owner = "test-owner",
            repository = "test-repo",
            baseUrl = server.url("/").toString().removeSuffix("/"),
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
    fun newerRelease_returnsUpdateAndApkAsset() = runBlocking {
        server.enqueue(
            MockResponse().setBody(
                """{"tag_name":"v0.2.0","name":"Luno 0.2.0","body":"Fixes","html_url":"https://github.com/test-owner/test-repo/releases/tag/v0.2.0","assets":[{"name":"luno.apk","browser_download_url":"https://github.com/test-owner/test-repo/releases/download/v0.2.0/luno.apk"}]}"""
            )
        )

        val result = service.checkForUpdate("0.1.0")

        assertThat(result).isInstanceOf(ReleaseCheckResult.UpdateAvailable::class.java)
        val update = result as ReleaseCheckResult.UpdateAvailable
        assertThat(update.release.version).isEqualTo("0.2.0")
        assertThat(update.release.apkUrl).endsWith("luno.apk")
        val request = server.takeRequest()
        assertThat(request.path).isEqualTo("/repos/test-owner/test-repo/releases/latest")
        assertThat(request.getHeader("Accept")).isEqualTo("application/vnd.github+json")
    }

    @Test
    fun sameVersion_returnsUpToDate() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"tag_name":"0.1.0","html_url":"https://example.test"}"""))

        val result = service.checkForUpdate("0.1.0")

        assertThat(result).isEqualTo(ReleaseCheckResult.UpToDate("0.1.0", "0.1.0"))
    }

    @Test
    fun invalidTag_returnsFailure() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"tag_name":"stable"}"""))

        val result = service.checkForUpdate("0.1.0")

        assertThat(result).isEqualTo(
            ReleaseCheckResult.Failure("The latest release has an invalid version tag")
        )
    }

    @Test
    fun httpFailure_returnsFailure() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(503))

        val result = service.checkForUpdate("0.1.0")

        assertThat(result).isInstanceOf(ReleaseCheckResult.Failure::class.java)
        assertThat((result as ReleaseCheckResult.Failure).message).contains("HTTP 503")
    }

    @Test
    fun versions_supportOptionalPartsAndPrefix() {
        assertThat(GitHubReleaseService.compareVersions("v1.2", "1.2.0")).isEqualTo(0)
        assertThat(GitHubReleaseService.compareVersions("1.2.1", "1.2.0")).isGreaterThan(0)
        assertThat(GitHubReleaseService.compareVersions("1.9.0", "2.0.0")).isLessThan(0)
        assertThat(GitHubReleaseService.compareVersions("development", "2.0.0")).isNull()
    }
}
