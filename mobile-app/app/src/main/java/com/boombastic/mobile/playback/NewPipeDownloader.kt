package com.boombastic.mobile.playback

import org.schabi.newpipe.extractor.downloader.Downloader
import org.schabi.newpipe.extractor.downloader.Request
import org.schabi.newpipe.extractor.downloader.Response
import org.schabi.newpipe.extractor.exceptions.ReCaptchaException
import java.io.BufferedReader
import java.io.InputStreamReader
import java.io.OutputStreamWriter
import java.net.HttpURLConnection
import java.net.URL

class NewPipeDownloader : Downloader() {

    private companion object {
        const val USER_AGENT = "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.6422.165 Mobile Safari/537.36"
        const val CONNECT_TIMEOUT = 15000
        const val READ_TIMEOUT = 30000
    }

    override fun execute(request: Request): Response {
        val url = URL(request.url())
        val conn = url.openConnection() as HttpURLConnection
        conn.connectTimeout = CONNECT_TIMEOUT
        conn.readTimeout = READ_TIMEOUT
        conn.instanceFollowRedirects = true
        conn.setRequestProperty("User-Agent", USER_AGENT)

        for ((key, values) in request.headers()) {
            for (value in values) {
                conn.setRequestProperty(key, value)
            }
        }

        val data = request.dataToSend()
        if (data != null && data.isNotEmpty()) {
            conn.doOutput = true
            conn.requestMethod = "POST"
            conn.setRequestProperty("Content-Type", "application/json")
            val writer = OutputStreamWriter(conn.outputStream, Charsets.UTF_8)
            writer.write(String(data, Charsets.UTF_8))
            writer.flush()
            writer.close()
        } else {
            conn.requestMethod = request.httpMethod()
        }

        val responseCode = conn.responseCode
        val responseMessage = conn.responseMessage ?: ""

        if (responseCode == 429) {
            throw ReCaptchaException("Rate limited (429)", url.toString())
        }

        val inputStream = if (responseCode in 200..299) {
            conn.inputStream
        } else {
            conn.errorStream
        }

        val body = if (inputStream != null) {
            BufferedReader(InputStreamReader(inputStream, Charsets.UTF_8))
                .readText()
        } else ""

        conn.disconnect()

        val headers = mutableMapOf<String, MutableList<String>>()
        for (key in conn.headerFields.keys) {
            val values = conn.headerFields[key] ?: continue
            if (key != null) {
                headers[key] = values.toMutableList()
            }
        }

        return Response(
            responseCode,
            responseMessage,
            headers,
            body,
            conn.url.toString()
        )
    }
}
