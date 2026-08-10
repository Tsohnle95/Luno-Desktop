package com.luno.mobile.data.export

import com.google.common.truth.Truth.assertThat
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import java.nio.charset.StandardCharsets

@RunWith(AndroidJUnit4::class)
@Config(manifest = Config.NONE, sdk = [34])
class LibraryManifestCodecTest {
    private fun fixture(name: String): String =
        javaClass.getResourceAsStream("/$name")!!.use {
            it.readBytes().toString(StandardCharsets.UTF_8)
        }

    private fun manifest(
        source: ManifestSource? = ManifestSource("youtube", "abc12345678")
    ) = LibraryManifest(
        scope = ManifestScope.LIBRARY,
        createdAt = "2026-08-02T00:00:00Z",
        tracks = listOf(
            ManifestTrack(
                ref = "src:one",
                title = "Song",
                artist = "Artist",
                album = "Album",
                durationMs = 210_000,
                source = source,
                ambiguityConfirmationRequired = source == null
            )
        ),
        playlists = listOf(
            ManifestPlaylist("Favorites", "", listOf("src:one"))
        ),
        unassignedTrackRefs = emptyList()
    )

    @Test
    fun roundTripPreservesSourceAndMembershipOrder() {
        val decoded = LibraryManifestCodec.decodeAndValidate(
            LibraryManifestCodec.encode(manifest())
        )

        assertThat(decoded).isEqualTo(manifest())
    }

    @Test
    fun metadataOnlyTrackRequiresExplicitAmbiguityConfirmation() {
        val json = LibraryManifestCodec.encode(manifest(source = null))
        assertThat(LibraryManifestCodec.decodeAndValidate(json).tracks.single().ambiguityConfirmationRequired)
            .isTrue()
    }

    @Test
    fun unknownFieldsAndUnsafePlaylistNamesAreRejected() {
        val json = LibraryManifestCodec.encode(manifest())
        val unknown = json.replace("\"format\"", "\"secret\":\"nope\",\"format\"")
        assertThat(runCatching { LibraryManifestCodec.decodeAndValidate(unknown) }.isFailure).isTrue()

        val unsafe = json.replace("\"Favorites\"", "\"../escape\"")
        assertThat(runCatching { LibraryManifestCodec.decodeAndValidate(unsafe) }.isFailure).isTrue()
    }

    @Test
    fun invalidReferenceAndUnsupportedVersionAreRejected() {
        val invalidRef = LibraryManifestCodec.encode(manifest())
            .replace("src:one", "../local-path")
        assertThat(runCatching { LibraryManifestCodec.decodeAndValidate(invalidRef) }.isFailure).isTrue()

        val invalidVersion = LibraryManifestCodec.encode(manifest())
            .replace("\"manifest_version\": 1", "\"manifest_version\": 99")
        assertThat(runCatching { LibraryManifestCodec.decodeAndValidate(invalidVersion) }.isFailure).isTrue()
    }

    @Test
    fun sharedValidFixtureIsAccepted() {
        val decoded = LibraryManifestCodec.decodeAndValidate(fixture("valid.json"))

        assertThat(decoded.scope).isEqualTo(ManifestScope.LIBRARY)
        assertThat(decoded.tracks).hasSize(2)
        assertThat(decoded.tracks[1].ambiguityConfirmationRequired).isTrue()
    }

    @Test
    fun sharedUnknownFieldFixtureIsRejected() {
        assertThat(runCatching {
            LibraryManifestCodec.decodeAndValidate(fixture("invalid-unknown-field.json"))
        }.isFailure).isTrue()
    }
}
