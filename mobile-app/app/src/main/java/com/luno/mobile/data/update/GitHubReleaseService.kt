package com.luno.mobile.data.update

import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

data class GitHubRelease(
    val tagName: String,
    val version: String,
    val name: String,
    val notes: String,
    val releaseUrl: String,
    val apkUrl: String?
)

sealed interface ReleaseCheckResult {
    data class UpdateAvailable(
        val currentVersion: String,
        val release: GitHubRelease
    ) : ReleaseCheckResult

    data class UpToDate(
        val currentVersion: String,
        val latestVersion: String
    ) : ReleaseCheckResult

    data class Failure(val message: String) : ReleaseCheckResult
}

/** Reads the latest public GitHub Release used for sideloaded APK updates. */
open class GitHubReleaseService(
    private val owner: String = DEFAULT_OWNER,
    private val repository: String = DEFAULT_REPOSITORY,
    private val baseUrl: String = "https://api.github.com",
    private val client: OkHttpClient = defaultClient()
) {
    companion object {
        const val DEFAULT_OWNER = "Tsohnle95"
        const val DEFAULT_REPOSITORY = "musicPlayer"

        private const val TIMEOUT_SECONDS = 8L
        private val VERSION_PATTERN = Regex("^v?(\\d+)(?:\\.(\\d+))?(?:\\.(\\d+))?(?:[-+].*)?$")

        fun defaultClient(): OkHttpClient = OkHttpClient.Builder()
            .connectTimeout(TIMEOUT_SECONDS, TimeUnit.SECONDS)
            .readTimeout(TIMEOUT_SECONDS, TimeUnit.SECONDS)
            .build()

        internal fun normalizeVersion(value: String): List<Int>? {
            val match = VERSION_PATTERN.matchEntire(value.trim()) ?: return null
            return listOf(
                match.groupValues[1].toInt(),
                match.groupValues[2].ifBlank { "0" }.toInt(),
                match.groupValues[3].ifBlank { "0" }.toInt()
            )
        }

        internal fun compareVersions(left: String, right: String): Int? {
            val leftParts = normalizeVersion(left) ?: return null
            val rightParts = normalizeVersion(right) ?: return null
            return leftParts.zip(rightParts)
                .firstOrNull { (leftPart, rightPart) -> leftPart != rightPart }
                ?.let { (leftPart, rightPart) -> leftPart.compareTo(rightPart) }
                ?: 0
        }
    }

    open suspend fun checkForUpdate(currentVersion: String): ReleaseCheckResult {
        val current = currentVersion.trim()
        if (normalizeVersion(current) == null) {
            return ReleaseCheckResult.Failure("The installed app version is invalid")
        }

        return try {
            val url = "$baseUrl/repos/$owner/$repository/releases/latest".toHttpUrl()
            val request = Request.Builder()
                .url(url)
                .header("Accept", "application/vnd.github+json")
                .header("User-Agent", "Luno/$current (Android)")
                .build()
            val responseBody = withContext(Dispatchers.IO) {
                client.newCall(request).execute().use { response ->
                    if (!response.isSuccessful) {
                        throw ReleaseCheckException("GitHub returned HTTP ${response.code}")
                    }
                    response.body?.string().orEmpty()
                }
            }
            val json = JSONObject(responseBody)
            val tagName = json.optString("tag_name").trim()
            val latestVersion = normalizeVersion(tagName)
                ?: return ReleaseCheckResult.Failure("The latest release has an invalid version tag")
            val comparison = compareVersions(current, latestVersion.joinToString("."))
                ?: return ReleaseCheckResult.Failure("Could not compare app versions")

            if (comparison >= 0) {
                ReleaseCheckResult.UpToDate(current, latestVersion.joinToString("."))
            } else {
                ReleaseCheckResult.UpdateAvailable(
                    currentVersion = current,
                    release = GitHubRelease(
                        tagName = tagName,
                        version = latestVersion.joinToString("."),
                        name = json.optString("name").ifBlank { tagName },
                        notes = json.optString("body").trim(),
                        releaseUrl = json.optString("html_url").trim(),
                        apkUrl = findApkUrl(json)
                    )
                )
            }
        } catch (e: ReleaseCheckException) {
            ReleaseCheckResult.Failure("Could not check GitHub Releases (${e.message})")
        } catch (_: Exception) {
            ReleaseCheckResult.Failure("Could not reach GitHub — check your connection")
        }
    }

    private fun findApkUrl(json: JSONObject): String? {
        val assets = json.optJSONArray("assets") ?: return null
        for (index in 0 until assets.length()) {
            val asset = assets.optJSONObject(index) ?: continue
            val name = asset.optString("name")
            val url = asset.optString("browser_download_url").trim()
            if (name.endsWith(".apk", ignoreCase = true) && url.isNotBlank()) return url
        }
        return null
    }

    private class ReleaseCheckException(message: String) : Exception(message)
}
