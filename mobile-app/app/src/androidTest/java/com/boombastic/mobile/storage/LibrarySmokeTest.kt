package com.boombastic.mobile.storage

import android.content.Context
import android.net.Uri
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.boombastic.mobile.BoomBasticApp
import com.boombastic.mobile.data.repository.LibraryRepository
import com.google.common.truth.Truth.assertThat
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.ByteArrayOutputStream
import java.io.File

/**
 * Smoke tests for SAF-based library import and storage interaction.
 *
 * Uses a [FakeUriPermissionPersister] so URI-permission persistence is
 * deterministic and does not require platform URI grants.  The
 * production [LibraryRepository.UriPermissionPersister.Default] is
 * exercised implicitly by every production flow; this test verifies
 * the repository pipeline (duplicate check, metadata extraction, DAO
 * insert) with a real FileProvider-served content URI.
 */
@RunWith(AndroidJUnit4::class)
class LibrarySmokeTest {

    private lateinit var app: BoomBasticApp

    @Before
    fun setUp() {
        @Suppress("UNCHECKED_CAST")
        app = ApplicationProvider.getApplicationContext<Context>() as BoomBasticApp
    }

    @Test
    fun repositoryIsInitialised() {
        assertThat(app.libraryRepository).isNotNull()
    }

    @Test
    fun importWithInvalidUriReturnsFailure() = runBlocking {
        val invalidUri = Uri.parse("content://invalid/uri/does/not/exist.mp3")
        val result = app.libraryRepository.importAudioUri(invalidUri)
        assertThat(result.isFailure).isTrue()
    }

    @Test
    fun importMultipleWithEmptyListReturnsZeroResult() = runBlocking {
        val result = app.libraryRepository.importMultipleUris(emptyList())
        assertThat(result.imported).isEqualTo(0)
        assertThat(result.duplicates).isEqualTo(0)
        assertThat(result.errors).isEqualTo(0)
    }

    @Test
    fun trackDaoIsAccessible() = runBlocking {
        val tracks = app.libraryRepository.getAllTracks().first()
        assertThat(tracks).isNotNull()
    }

    // ── Required coverage: SAF import, revoked URI access ──────────────────

    @Test
    fun safImportSuccess() = runBlocking {
        val fakePersister = FakeUriPermissionPersister()
        val repo = LibraryRepository(
            app, app.database.trackDao(),
            uriPermissionPersister = fakePersister
        )

        val audioFile = createMinimalWavFile(app.cacheDir, "saf_import_test.wav")
        val uri = FileProviderUri.forFile(app, audioFile)

        // Import via the repository — exercises duplicate check, URI permission
        // persistence (recorded by the fake), metadata extraction, and DAO insert.
        val result = repo.importAudioUri(uri)
        assertThat(result.isSuccess).isTrue()
        assertThat(result.getOrNull()!!.uri).isEqualTo(uri.toString())

        // Verify the persister was invoked with the correct URI.
        assertThat(fakePersister.lastPersistedUri).isEqualTo(uri)

        // Clean up.
        repo.deleteTrack(uri.toString())
    }

    @Test
    fun revokedUriAccessDetectedAsFailure() = runBlocking {
        val fakePersister = FakeUriPermissionPersister()
        val repo = LibraryRepository(
            app, app.database.trackDao(),
            uriPermissionPersister = fakePersister
        )

        val audioFile = createMinimalWavFile(app.cacheDir, "revoke_test.wav")
        val uri = FileProviderUri.forFile(app, audioFile)

        // Step 1 — import succeeds.
        val importResult = repo.importAudioUri(uri)
        assertThat(importResult.isSuccess).isTrue()
        assertThat(fakePersister.lastPersistedUri).isEqualTo(uri)

        // Step 2 — simulate revocation: the persister throws on next call,
        // the source file is deleted, and the track is removed from the DB.
        fakePersister.revoke()
        audioFile.delete()
        repo.deleteTrack(uri.toString())

        // Step 3 — re-import must fail because the persister now throws.
        val retryResult = repo.importAudioUri(uri)
        assertThat(retryResult.isFailure).isTrue()
    }

    // ── Test helpers ───────────────────────────────────────────────────────

    companion object {
        /**
         * Creates a minimal but valid 16-bit mono 44.1 kHz WAV file.
         */
        fun createMinimalWavFile(dir: File, name: String): File {
            val file = File(dir, name)
            val sampleRate = 44100
            val bitsPerSample = 16
            val numChannels = 1
            val numSamples = sampleRate / 10 // 100 ms of silence
            val byteRate = sampleRate * numChannels * (bitsPerSample / 8)
            val blockAlign = numChannels * (bitsPerSample / 8)
            val dataSize = numSamples * numChannels * (bitsPerSample / 8)
            val fileSize = 36 + dataSize

            val bos = ByteArrayOutputStream()
            bos.write("RIFF".toByteArray())
            bos.write(intToBytesLE(fileSize, 4))
            bos.write("WAVE".toByteArray())
            bos.write("fmt ".toByteArray())
            bos.write(intToBytesLE(16, 4))
            bos.write(intToBytesLE(1, 2))
            bos.write(intToBytesLE(numChannels, 2))
            bos.write(intToBytesLE(sampleRate, 4))
            bos.write(intToBytesLE(byteRate, 4))
            bos.write(intToBytesLE(blockAlign, 2))
            bos.write(intToBytesLE(bitsPerSample, 2))
            bos.write("data".toByteArray())
            bos.write(intToBytesLE(dataSize, 4))
            repeat(dataSize) { bos.write(0) }
            file.writeBytes(bos.toByteArray())
            return file
        }

        private fun intToBytesLE(value: Int, numBytes: Int): ByteArray {
            val bytes = ByteArray(numBytes)
            for (i in 0 until numBytes) {
                bytes[i] = (value shr (i * 8)).toByte()
            }
            return bytes
        }
    }
}

/**
 * Deterministic fake [LibraryRepository.UriPermissionPersister] that
 * records every invocation and can be switched to throw — simulating
 * a revoked SAF URI grant — without involving the platform
 * [android.content.ContentResolver].
 */
private class FakeUriPermissionPersister : LibraryRepository.UriPermissionPersister {

    @Volatile
    var lastPersistedUri: Uri? = null
        private set

    @Volatile
    private var throwOnNext = false

    override fun persistReadPermission(uri: Uri) {
        if (throwOnNext) {
            throw SecurityException("Simulated revoked URI permission")
        }
        lastPersistedUri = uri
    }

    /** Switch to failure mode for subsequent calls. */
    fun revoke() {
        throwOnNext = true
    }
}

/**
 * Test-only helper that constructs a FileProvider content URI using the
 * authority declared in [androidTest/AndroidManifest.xml].
 */
private object FileProviderUri {
    private const val AUTHORITY = "com.boombastic.mobile.test.fileprovider"

    fun forFile(context: Context, file: File): Uri {
        return androidx.core.content.FileProvider.getUriForFile(context, AUTHORITY, file)
    }
}
