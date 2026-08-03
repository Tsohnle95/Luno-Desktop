package com.luno.mobile.data.update

import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.content.FileProvider
import java.io.File
import java.io.IOException
import java.net.URL
import java.util.concurrent.TimeUnit
import okhttp3.OkHttpClient
import okhttp3.Request

sealed interface ApkInstallResult {
    data object InstallerOpened : ApkInstallResult
    data object UnknownSourcesPermissionRequired : ApkInstallResult
    data class Failure(val message: String) : ApkInstallResult
}

/** Downloads a release APK into private cache storage and opens Android's installer. */
object ApkInstaller {
    private const val MAX_APK_BYTES = 200L * 1024L * 1024L
    private val client = OkHttpClient.Builder()
        .connectTimeout(15, TimeUnit.SECONDS)
        .readTimeout(2, TimeUnit.MINUTES)
        .build()

    suspend fun downloadAndOpenInstaller(context: Context, apkUrl: String): ApkInstallResult {
        val url = runCatching { URL(apkUrl) }.getOrNull()
            ?: return ApkInstallResult.Failure("The APK link is invalid")
        if (url.protocol != "https" || url.host != "github.com") {
            return ApkInstallResult.Failure("The APK link is not a trusted GitHub HTTPS URL")
        }
        return try {
            val target = File(context.cacheDir, "luno-update.apk")
            val temporary = File(context.cacheDir, "luno-update.apk.part")
            val request = Request.Builder().url(apkUrl).header("User-Agent", "Luno (Android)").build()
            client.newCall(request).execute().use { response ->
                if (!response.isSuccessful) throw IOException("GitHub returned HTTP ${response.code}")
                val body = response.body ?: throw IOException("GitHub returned an empty APK")
                if (body.contentLength() > MAX_APK_BYTES) throw IOException("The APK is unexpectedly large")
                temporary.outputStream().buffered().use { output ->
                    body.byteStream().use { input ->
                        val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
                        var total = 0L
                        while (true) {
                            val count = input.read(buffer)
                            if (count < 0) break
                            total += count
                            if (total > MAX_APK_BYTES) throw IOException("The APK is unexpectedly large")
                            output.write(buffer, 0, count)
                        }
                    }
                }
            }
            if (!temporary.renameTo(target)) {
                temporary.copyTo(target, overwrite = true)
                temporary.delete()
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O &&
                !context.packageManager.canRequestPackageInstalls()
            ) return ApkInstallResult.UnknownSourcesPermissionRequired

            val contentUri = FileProvider.getUriForFile(
                context, "${context.packageName}.fileprovider", target
            )
            val installIntent = Intent(Intent.ACTION_VIEW).apply {
                setDataAndType(contentUri, "application/vnd.android.package-archive")
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            context.startActivity(installIntent)
            ApkInstallResult.InstallerOpened
        } catch (e: Exception) {
            ApkInstallResult.Failure(e.message ?: "Could not download the APK")
        }
    }
}