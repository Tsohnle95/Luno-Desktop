package com.boombastic.mobile.data.export

import org.json.JSONArray
import org.json.JSONObject

/** Thrown for malformed, unsupported, or unsafe user-provided manifests. */
class ManifestFormatException(message: String) : IllegalArgumentException(message)

/**
 * Strict JSON codec for the portable library manifest.
 *
 * This intentionally does not use JSONObject.opt* accessors: permissive
 * coercion makes malformed files look valid and makes forward-compatible
 * fields impossible to audit.
 */
object LibraryManifestCodec {
    const val MAX_INPUT_CHARS = 10 * 1024 * 1024
    private const val MAX_TRACKS = 50_000
    private const val MAX_PLAYLISTS = 5_000
    private const val MAX_STRING_LENGTH = 2_000
    private val referencePattern = Regex("^[A-Za-z0-9._:-]{1,128}$")
    private val providerPattern = Regex("^[a-z0-9._-]{1,32}$")
    private val sourceIdPattern = Regex("^[A-Za-z0-9._~:-]{1,256}$")
    private val controlPattern = Regex("[\\u0000-\\u001f\\u007f]")
    private val providers = setOf("youtube", "soundcloud", "deezer", "musicbrainz")

    fun encode(manifest: LibraryManifest): String {
        // Round-trip through the decoder so programmatic exports get the same
        // validation guarantees as imported files.
        val json = toJson(manifest).toString()
        val normalized = decodeAndValidate(json)
        return toJson(normalized).toString(2)
    }

    fun decodeAndValidate(input: String): LibraryManifest {
        if (input.length > MAX_INPUT_CHARS) {
            throw ManifestFormatException("Manifest is too large")
        }
        val root = try {
            JSONObject(input)
        } catch (error: Exception) {
            throw ManifestFormatException("Manifest is not valid JSON: ${error.message ?: "parse error"}")
        }
        requireKeys(
            root,
            setOf(
                "format",
                "manifest_version",
                "scope",
                "created_at",
                "tracks",
                "playlists",
                "unassigned_track_refs"
            ),
            "manifest"
        )
        if (root.getStrictString("format", "format") != LIBRARY_MANIFEST_FORMAT) {
            throw ManifestFormatException("Unsupported manifest format")
        }
        if (root.getStrictLong("manifest_version", "manifest_version") != LIBRARY_MANIFEST_VERSION.toLong()) {
            throw ManifestFormatException("Unsupported manifest version")
        }
        val scopeName = root.getStrictString("scope", "scope")
        val scope = ManifestScope.entries.firstOrNull { it.wireName == scopeName }
            ?: throw ManifestFormatException("Unsupported manifest scope")
        val createdAt = root.getStrictString("created_at", "created_at")
        if (!createdAt.endsWith("Z")) {
            throw ManifestFormatException("created_at must be an ISO-8601 UTC timestamp")
        }

        val tracksArray = root.getStrictArray("tracks", "tracks")
        val playlistsArray = root.getStrictArray("playlists", "playlists")
        val unassignedArray = root.getStrictArray("unassigned_track_refs", "unassigned_track_refs")
        if (tracksArray.length() > MAX_TRACKS) throw ManifestFormatException("Too many tracks")
        if (playlistsArray.length() > MAX_PLAYLISTS) throw ManifestFormatException("Too many playlists")

        val refs = linkedSetOf<String>()
        val tracks = buildList {
            for (index in 0 until tracksArray.length()) {
                val item = tracksArray.get(index) as? JSONObject
                    ?: throw ManifestFormatException("tracks[$index] must be an object")
                val field = "tracks[$index]"
                requireKeys(
                    item,
                    setOf(
                        "ref",
                        "title",
                        "artist",
                        "album",
                        "duration_ms",
                        "source",
                        "ambiguity_confirmation_required"
                    ),
                    field
                )
                val ref = item.getStrictString("ref", "$field.ref")
                if (!referencePattern.matches(ref) || !refs.add(ref)) {
                    throw ManifestFormatException("Invalid or duplicate $field.ref")
                }
                val title = item.getStrictString("title", "$field.title")
                val artist = item.getOptionalString("artist", "$field.artist")
                val album = item.getOptionalString("album", "$field.album")
                val durationMs = item.getStrictLong("duration_ms", "$field.duration_ms")
                if (durationMs !in 0L..86_400_000L) {
                    throw ManifestFormatException("Invalid $field.duration_ms")
                }
                val ambiguity = item.getStrictBoolean(
                    "ambiguity_confirmation_required",
                    "$field.ambiguity_confirmation_required"
                )
                val source = if (item.isNull("source")) {
                    null
                } else {
                    val sourceObject = item.get("source") as? JSONObject
                        ?: throw ManifestFormatException("$field.source must be an object or null")
                    requireKeys(sourceObject, setOf("provider", "id"), "$field.source")
                    val provider = sourceObject.getStrictString("provider", "$field.source.provider").lowercase()
                    val sourceId = sourceObject.getStrictString("id", "$field.source.id")
                    if (!providerPattern.matches(provider) || provider !in providers) {
                        throw ManifestFormatException("Unsupported $field.source.provider")
                    }
                    if (!sourceIdPattern.matches(sourceId)) {
                        throw ManifestFormatException("Invalid $field.source.id")
                    }
                    ManifestSource(provider, sourceId)
                }
                if (source == null && !ambiguity) {
                    throw ManifestFormatException(
                        "$field requires ambiguity confirmation without a source id"
                    )
                }
                add(
                    ManifestTrack(
                        ref = ref,
                        title = title,
                        artist = artist,
                        album = album,
                        durationMs = durationMs,
                        source = source,
                        ambiguityConfirmationRequired = ambiguity
                    )
                )
            }
        }

        val playlistNames = mutableSetOf<String>()
        val playlists = buildList {
            for (index in 0 until playlistsArray.length()) {
                val item = playlistsArray.get(index) as? JSONObject
                    ?: throw ManifestFormatException("playlists[$index] must be an object")
                val field = "playlists[$index]"
                requireKeys(item, setOf("name", "description", "track_refs"), field)
                val name = item.getStrictString("name", "$field.name")
                validatePlaylistName(name, field)
                if (name.equals("All Music", ignoreCase = true) ||
                    !playlistNames.add(name.lowercase())
                ) {
                    throw ManifestFormatException("Duplicate or virtual $field.name")
                }
                val description = item.getOptionalString("description", "$field.description")
                val refsArray = item.getStrictArray("track_refs", "$field.track_refs")
                val seenRefs = mutableSetOf<String>()
                val trackRefs = buildList {
                    for (refIndex in 0 until refsArray.length()) {
                        val ref = refsArray.get(refIndex) as? String
                            ?: throw ManifestFormatException("$field.track_refs[$refIndex] must be a string")
                        if (!referencePattern.matches(ref) || ref !in refs || !seenRefs.add(ref)) {
                            throw ManifestFormatException("Invalid track reference in $field")
                        }
                        add(ref)
                    }
                }
                add(ManifestPlaylist(name, description, trackRefs))
            }
        }

        val unassigned = buildList {
            val seen = mutableSetOf<String>()
            for (index in 0 until unassignedArray.length()) {
                val ref = unassignedArray.get(index) as? String
                    ?: throw ManifestFormatException("unassigned_track_refs[$index] must be a string")
                if (!referencePattern.matches(ref) || ref !in refs || !seen.add(ref)) {
                    throw ManifestFormatException("Invalid unassigned track reference")
                }
                add(ref)
            }
        }

        return LibraryManifest(scope, createdAt, tracks, playlists, unassigned)
    }

    private fun toJson(manifest: LibraryManifest): JSONObject = JSONObject().apply {
        put("format", LIBRARY_MANIFEST_FORMAT)
        put("manifest_version", LIBRARY_MANIFEST_VERSION)
        put("scope", manifest.scope.wireName)
        put("created_at", manifest.createdAt)
        put(
            "tracks",
            JSONArray().apply {
                manifest.tracks.forEach { track ->
                    put(JSONObject().apply {
                        put("ref", track.ref)
                        put("title", track.title)
                        put("artist", track.artist)
                        put("album", track.album)
                        put("duration_ms", track.durationMs)
                        if (track.source == null) {
                            put("source", JSONObject.NULL)
                        } else {
                            put(
                                "source",
                                JSONObject().apply {
                                    put("provider", track.source.provider)
                                    put("id", track.source.id)
                                }
                            )
                        }
                        put("ambiguity_confirmation_required", track.ambiguityConfirmationRequired)
                    })
                }
            }
        )
        put(
            "playlists",
            JSONArray().apply {
                manifest.playlists.forEach { playlist ->
                    put(JSONObject().apply {
                        put("name", playlist.name)
                        put("description", playlist.description)
                        put("track_refs", JSONArray(playlist.trackRefs))
                    })
                }
            }
        )
        put("unassigned_track_refs", JSONArray(manifest.unassignedTrackRefs))
    }

    private fun validatePlaylistName(name: String, field: String) {
        if (name.length > MAX_STRING_LENGTH ||
            name == "." ||
            name == ".." ||
            name.contains('/') ||
            name.contains('\\') ||
            controlPattern.containsMatchIn(name)
        ) {
            throw ManifestFormatException("Unsafe $field.name")
        }
    }

    private fun requireKeys(value: JSONObject, allowed: Set<String>, field: String) {
        val unknown = value.keys().asSequence().filterNot { it in allowed }.toList()
        if (unknown.isNotEmpty()) {
            throw ManifestFormatException("Unknown $field field(s): ${unknown.sorted().joinToString()}")
        }
    }

    private fun JSONObject.getStrictString(key: String, field: String): String {
        val value = requiredValue(key, field)
        if (value !is String || value.length > MAX_STRING_LENGTH || controlPattern.containsMatchIn(value)) {
            throw ManifestFormatException("$field must be a valid string")
        }
        if (value.isBlank()) throw ManifestFormatException("$field cannot be empty")
        return value.trim()
    }

    private fun JSONObject.getOptionalString(key: String, field: String): String {
        if (!has(key) || isNull(key)) return ""
        val value = requiredValue(key, field)
        if (value !is String || value.length > MAX_STRING_LENGTH || controlPattern.containsMatchIn(value)) {
            throw ManifestFormatException("$field must be a valid string")
        }
        return value.trim()
    }

    private fun JSONObject.getStrictLong(key: String, field: String): Long {
        val value = requiredValue(key, field)
        val number = value as? Number
            ?: throw ManifestFormatException("$field must be an integer")
        val longValue = number.toLong()
        if (number.toDouble() != longValue.toDouble()) {
            throw ManifestFormatException("$field must be an integer")
        }
        return longValue
    }

    private fun JSONObject.getStrictBoolean(key: String, field: String): Boolean {
        return requiredValue(key, field) as? Boolean
            ?: throw ManifestFormatException("$field must be boolean")
    }

    private fun JSONObject.getStrictArray(key: String, field: String): JSONArray {
        return requiredValue(key, field) as? JSONArray
            ?: throw ManifestFormatException("$field must be an array")
    }

    private fun JSONObject.requiredValue(key: String, field: String): Any {
        return try {
            get(key)
        } catch (error: Exception) {
            throw ManifestFormatException("Missing $field")
        }
    }
}
