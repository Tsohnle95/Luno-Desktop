"""Portable playlist/library manifests.

The manifest is deliberately a reference file, not an audio backup.  It is
safe to move between the desktop and Android applications because it contains
metadata and optional provider identifiers, but never local paths or audio
bytes.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
from datetime import datetime, timezone
from typing import Any, Callable, Iterable

from utils import split_track_name


MANIFEST_FORMAT = "boombastic.library.export"
MANIFEST_VERSION = 1
MAX_MANIFEST_BYTES = 10 * 1024 * 1024
MAX_TRACKS = 50_000
MAX_PLAYLISTS = 5_000
MAX_STRING_LENGTH = 2_000

_SCOPES = {"selected_tracks", "playlists", "library"}
_PROVIDERS = {"youtube", "soundcloud", "deezer", "musicbrainz"}
_REF_RE = re.compile(r"^[A-Za-z0-9._:-]{1,128}$")
_PROVIDER_RE = re.compile(r"^[a-z0-9._-]{1,32}$")
_SOURCE_ID_RE = re.compile(r"^[A-Za-z0-9._~:-]{1,256}$")
_CONTROL_RE = re.compile(r"[\x00-\x1f\x7f]")
_DURATION_TOLERANCE_MS = 10_000


class ManifestError(ValueError):
    """Raised when an untrusted manifest is malformed or unsafe."""


def _ensure_string(value: Any, field: str, *, required: bool = True) -> str:
    if not isinstance(value, str):
        if required:
            raise ManifestError(f"{field} must be a string")
        return ""
    if _CONTROL_RE.search(value) or len(value) > MAX_STRING_LENGTH:
        raise ManifestError(f"{field} is invalid or too long")
    if required and not value.strip():
        raise ManifestError(f"{field} cannot be empty")
    return value.strip()


def _check_keys(value: dict[str, Any], allowed: set[str], field: str) -> None:
    unknown = set(value) - allowed
    if unknown:
        raise ManifestError(f"Unknown {field} field(s): {', '.join(sorted(unknown))}")


def _validate_source(value: Any, field: str) -> dict[str, str]:
    if not isinstance(value, dict):
        raise ManifestError(f"{field} must be an object")
    _check_keys(value, {"provider", "id"}, field)
    provider = _ensure_string(value.get("provider"), f"{field}.provider").lower()
    source_id = _ensure_string(value.get("id"), f"{field}.id")
    if not _PROVIDER_RE.fullmatch(provider) or provider not in _PROVIDERS:
        raise ManifestError(f"Unsupported {field}.provider")
    if not _SOURCE_ID_RE.fullmatch(source_id):
        raise ManifestError(f"Invalid {field}.id")
    return {"provider": provider, "id": source_id}


def validate_manifest(value: Any) -> dict[str, Any]:
    """Validate and return a normalized manifest copy.

    This function intentionally rejects unknown fields.  A transfer file is
    user-controlled input and should not grow an executable/dynamic payload
    surface by accident.
    """
    if not isinstance(value, dict):
        raise ManifestError("Manifest root must be an object")
    _check_keys(
        value,
        {
            "format",
            "manifest_version",
            "scope",
            "created_at",
            "tracks",
            "playlists",
            "unassigned_track_refs",
        },
        "manifest",
    )
    if value.get("format") != MANIFEST_FORMAT:
        raise ManifestError("Unsupported manifest format")
    if value.get("manifest_version") != MANIFEST_VERSION:
        raise ManifestError("Unsupported manifest version")
    scope = _ensure_string(value.get("scope"), "scope").lower()
    if scope not in _SCOPES:
        raise ManifestError("Unsupported manifest scope")
    created_at = _ensure_string(value.get("created_at"), "created_at")
    if not created_at.endswith("Z"):
        raise ManifestError("created_at must be an ISO-8601 UTC timestamp")

    raw_tracks = value.get("tracks")
    raw_playlists = value.get("playlists")
    raw_unassigned = value.get("unassigned_track_refs")
    if not isinstance(raw_tracks, list) or len(raw_tracks) > MAX_TRACKS:
        raise ManifestError("tracks must be a bounded array")
    if not isinstance(raw_playlists, list) or len(raw_playlists) > MAX_PLAYLISTS:
        raise ManifestError("playlists must be a bounded array")
    if not isinstance(raw_unassigned, list) or len(raw_unassigned) > MAX_TRACKS:
        raise ManifestError("unassigned_track_refs must be a bounded array")

    tracks: list[dict[str, Any]] = []
    refs: set[str] = set()
    for index, raw_track in enumerate(raw_tracks):
        field = f"tracks[{index}]"
        if not isinstance(raw_track, dict):
            raise ManifestError(f"{field} must be an object")
        _check_keys(
            raw_track,
            {
                "ref",
                "title",
                "artist",
                "album",
                "duration_ms",
                "source",
                "ambiguity_confirmation_required",
            },
            field,
        )
        ref = _ensure_string(raw_track.get("ref"), f"{field}.ref")
        if not _REF_RE.fullmatch(ref) or ref in refs:
            raise ManifestError(f"Invalid or duplicate {field}.ref")
        refs.add(ref)
        title = _ensure_string(raw_track.get("title"), f"{field}.title")
        artist = _ensure_string(raw_track.get("artist", ""), f"{field}.artist", required=False)
        album = _ensure_string(raw_track.get("album", ""), f"{field}.album", required=False)
        duration = raw_track.get("duration_ms", 0)
        if isinstance(duration, bool) or not isinstance(duration, int) or not 0 <= duration <= 86_400_000:
            raise ManifestError(f"{field}.duration_ms is invalid")
        ambiguity = raw_track.get("ambiguity_confirmation_required")
        if not isinstance(ambiguity, bool):
            raise ManifestError(f"{field}.ambiguity_confirmation_required must be boolean")
        source = None
        if raw_track.get("source") is not None:
            source = _validate_source(raw_track["source"], f"{field}.source")
        if source is None and not ambiguity:
            raise ManifestError(f"{field} requires ambiguity confirmation without a source id")
        tracks.append(
            {
                "ref": ref,
                "title": title,
                "artist": artist,
                "album": album,
                "duration_ms": duration,
                "source": source,
                "ambiguity_confirmation_required": ambiguity,
            }
        )

    playlists: list[dict[str, Any]] = []
    playlist_names: set[str] = set()
    for index, raw_playlist in enumerate(raw_playlists):
        field = f"playlists[{index}]"
        if not isinstance(raw_playlist, dict):
            raise ManifestError(f"{field} must be an object")
        _check_keys(raw_playlist, {"name", "description", "track_refs"}, field)
        name = _ensure_string(raw_playlist.get("name"), f"{field}.name")
        _validate_playlist_name(name, field)
        if name.lower() == "all music" or name.casefold() in playlist_names:
            raise ManifestError(f"Duplicate or virtual {field}.name")
        playlist_names.add(name.casefold())
        description = _ensure_string(
            raw_playlist.get("description", ""), f"{field}.description", required=False
        )
        track_refs = raw_playlist.get("track_refs")
        if not isinstance(track_refs, list) or len(track_refs) > MAX_TRACKS:
            raise ManifestError(f"{field}.track_refs must be a bounded array")
        normalized_refs: list[str] = []
        seen_refs: set[str] = set()
        for ref in track_refs:
            clean_ref = _ensure_string(ref, f"{field}.track_refs[]")
            if not _REF_RE.fullmatch(clean_ref) or clean_ref not in refs or clean_ref in seen_refs:
                raise ManifestError(f"Invalid track reference in {field}")
            seen_refs.add(clean_ref)
            normalized_refs.append(clean_ref)
        playlists.append({"name": name, "description": description, "track_refs": normalized_refs})

    unassigned: list[str] = []
    seen_unassigned: set[str] = set()
    for ref in raw_unassigned:
        clean_ref = _ensure_string(ref, "unassigned_track_refs[]")
        if not _REF_RE.fullmatch(clean_ref) or clean_ref not in refs or clean_ref in seen_unassigned:
            raise ManifestError("Invalid unassigned track reference")
        seen_unassigned.add(clean_ref)
        unassigned.append(clean_ref)

    return {
        "format": MANIFEST_FORMAT,
        "manifest_version": MANIFEST_VERSION,
        "scope": scope,
        "created_at": created_at,
        "tracks": tracks,
        "playlists": playlists,
        "unassigned_track_refs": unassigned,
    }


def _validate_playlist_name(name: str, field: str = "playlist") -> None:
    if name in {".", ".."} or "/" in name or "\\" in name:
        raise ManifestError(f"Unsafe {field} name")


def load_manifest(path: str) -> dict[str, Any]:
    """Read and validate a manifest with a bounded input size."""
    try:
        if os.path.getsize(path) > MAX_MANIFEST_BYTES:
            raise ManifestError("Manifest is too large")
        with open(path, "r", encoding="utf-8") as handle:
            return validate_manifest(json.load(handle))
    except ManifestError:
        raise
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise ManifestError(f"Could not read manifest: {exc}") from exc


def write_manifest(path: str, manifest: dict[str, Any]) -> None:
    """Validate and write a manifest as readable JSON."""
    normalized = validate_manifest(manifest)
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(normalized, handle, indent=2, ensure_ascii=True)
        handle.write("\n")


def _metadata_for_path(path: str) -> dict[str, Any]:
    title, artist = split_track_name(path)
    album = ""
    duration_ms = 0
    try:
        from mutagen import File as MutagenFile

        audio = MutagenFile(path, easy=True)
        if audio is not None:
            tags = audio.tags or {}
            title = str(tags.get("title", [title])[0] or title)
            artist = str(tags.get("artist", [artist])[0] or artist)
            album = str(tags.get("album", [""])[0] or "")
            duration_ms = int(max(0.0, float(getattr(audio.info, "length", 0.0))) * 1000)
    except Exception:
        pass
    return {
        "title": title.strip() or "Unknown track",
        "artist": artist.strip(),
        "album": album.strip(),
        "duration_ms": duration_ms,
    }


def _metadata_key(metadata: dict[str, Any]) -> tuple[str, str]:
    def clean(value: str) -> str:
        return re.sub(r"[^a-z0-9]", "", value.casefold())

    return clean(metadata.get("artist", "")), clean(metadata.get("title", ""))


def _new_ref(metadata: dict[str, Any], source: dict[str, str] | None, used: set[str]) -> str:
    if source:
        seed = f"source:{source['provider']}:{source['id']}"
    else:
        seed = "metadata:" + json.dumps(
            [metadata.get("artist", ""), metadata.get("title", ""), metadata.get("album", ""), metadata.get("duration_ms", 0)],
            ensure_ascii=True,
            separators=(",", ":"),
        )
    base = hashlib.sha256(seed.encode("utf-8")).hexdigest()[:24]
    ref = ("src:" if source else "trk:") + base
    suffix = 1
    candidate = ref
    while candidate in used:
        suffix += 1
        candidate = f"{ref}-{suffix}"
    used.add(candidate)
    return candidate


def build_manifest(
    library: dict[str, list[str]],
    root: str,
    *,
    scope: str = "library",
    selected_paths: Iterable[str] | None = None,
    playlist_names: Iterable[str] | None = None,
    source_by_path: dict[str, dict[str, str]] | None = None,
    created_at: str | None = None,
) -> dict[str, Any]:
    """Build a portable manifest from the desktop's path-based library."""
    if scope not in _SCOPES:
        raise ManifestError("Unsupported manifest scope")
    del root  # The root is intentionally never serialized.
    selected = set(os.path.abspath(path) for path in selected_paths or [])
    requested_playlists = set(playlist_names or [])
    include_playlists = scope in {"library", "playlists"}
    if scope == "playlists" and not requested_playlists:
        requested_playlists = {
            name for name in library if name not in {"All Music"}
        }

    included_paths: list[str] = []
    playlist_rows: list[dict[str, Any]] = []
    path_to_ref: dict[str, str] = {}
    tracks: list[dict[str, Any]] = []
    used_refs: set[str] = set()
    source_by_path = source_by_path or {}

    def add_path(path: str) -> str:
        absolute = os.path.abspath(path)
        if absolute in path_to_ref:
            return path_to_ref[absolute]
        metadata = _metadata_for_path(absolute)
        source = source_by_path.get(absolute)
        if source is not None:
            source = _validate_source(source, "source")
        ref = _new_ref(metadata, source, used_refs)
        tracks.append(
            {
                "ref": ref,
                **metadata,
                "source": source,
                "ambiguity_confirmation_required": source is None,
            }
        )
        path_to_ref[absolute] = ref
        included_paths.append(absolute)
        return ref

    if scope == "selected_tracks":
        for path in selected or []:
            if os.path.isfile(path):
                add_path(path)
    else:
        for playlist_name, paths in library.items():
            if playlist_name == "All Music" or playlist_name not in requested_playlists and scope == "playlists":
                continue
            refs = []
            for path in paths:
                if os.path.isfile(path):
                    refs.append(add_path(path))
            playlist_rows.append({"name": playlist_name, "description": "", "track_refs": refs})

    playlist_refs = {ref for row in playlist_rows for ref in row["track_refs"]}
    unassigned = [ref for ref in (path_to_ref[path] for path in included_paths) if ref not in playlist_refs]
    if scope == "selected_tracks":
        unassigned = [track["ref"] for track in tracks]
    now = created_at or datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    return validate_manifest(
        {
            "format": MANIFEST_FORMAT,
            "manifest_version": MANIFEST_VERSION,
            "scope": scope,
            "created_at": now,
            "tracks": tracks,
            "playlists": playlist_rows if include_playlists else [],
            "unassigned_track_refs": unassigned,
        }
    )


def plan_import(
    manifest: dict[str, Any],
    library: dict[str, list[str]],
    *,
    source_by_path: dict[str, dict[str, str]] | None = None,
) -> dict[str, Any]:
    """Match manifest metadata to local files without changing the library."""
    normalized = validate_manifest(manifest)
    source_by_path = source_by_path or {}
    source_index = {
        (source.get("provider"), source.get("id")): path
        for path, source in source_by_path.items()
        if isinstance(source, dict)
    }
    candidates: dict[tuple[str, str], list[str]] = {}
    for paths in library.values():
        for path in paths:
            metadata = _metadata_for_path(path)
            candidates.setdefault(_metadata_key(metadata), []).append(path)

    matched: dict[str, str] = {}
    missing: list[dict[str, Any]] = []
    used_paths: set[str] = set()
    for track in normalized["tracks"]:
        if track.get("source"):
            source = track["source"]
            source_match = source_index.get((source["provider"], source["id"]))
            options = [source_match] if source_match else []
        else:
            options = candidates.get(_metadata_key(track), [])
        selected = None
        for path in options:
            if path in used_paths:
                continue
            if track["duration_ms"] == 0:
                selected = path
                break
            actual_duration = _metadata_for_path(path)["duration_ms"]
            if actual_duration == 0 or abs(actual_duration - track["duration_ms"]) <= _DURATION_TOLERANCE_MS:
                selected = path
                break
        if selected:
            matched[track["ref"]] = selected
            used_paths.add(selected)
        else:
            missing.append(track)

    return {
        "manifest": normalized,
        "matched": matched,
        "missing": missing,
        "playlist_paths": {
            row["name"]: [matched[ref] for ref in row["track_refs"] if ref in matched]
            for row in normalized["playlists"]
        },
    }


def apply_import_plan(
    plan: dict[str, Any],
    root: str,
    *,
    copy_file: Callable[[str, str], Any] = shutil.copy2,
) -> dict[str, Any]:
    """Merge matched files into manifest playlists; never deletes files."""
    manifest = validate_manifest(plan["manifest"])
    matched = dict(plan.get("matched", {}))
    playlist_rows = list(manifest["playlists"])
    if not playlist_rows:
        refs = list(manifest["unassigned_track_refs"])
        playlist_rows = [{"name": "Unsorted", "description": "", "track_refs": refs}]

    copied = 0
    skipped = 0
    errors: list[str] = []
    playlist_by_ref = {
        ref: row["name"]
        for row in playlist_rows
        for ref in row["track_refs"]
    }
    for row in playlist_rows:
        try:
            _validate_playlist_name(row["name"])
            destination = root if row["name"].casefold() == "unsorted" else os.path.join(root, row["name"])
            os.makedirs(destination, exist_ok=True)
        except (OSError, ManifestError) as exc:
            errors.append(f"{row['name']}: {exc}")
            continue
        for ref in row["track_refs"]:
            source = matched.get(ref)
            if not source:
                continue
            destination_path = os.path.join(destination, os.path.basename(source))
            if os.path.realpath(source) == os.path.realpath(destination_path):
                skipped += 1
                continue
            if os.path.exists(destination_path):
                stem, extension = os.path.splitext(os.path.basename(source))
                suffix = 1
                while os.path.exists(destination_path):
                    suffix += 1
                    destination_path = os.path.join(destination, f"{stem} (imported {suffix}){extension}")
            try:
                copy_file(source, destination_path)
                copied += 1
            except OSError as exc:
                errors.append(f"{os.path.basename(source)}: {exc}")

    return {
        "copied": copied,
        "skipped": skipped,
        "missing": list(plan.get("missing", [])),
        "errors": errors,
        "playlist_by_ref": playlist_by_ref,
    }


__all__ = [
    "MANIFEST_FORMAT",
    "MANIFEST_VERSION",
    "ManifestError",
    "apply_import_plan",
    "build_manifest",
    "load_manifest",
    "plan_import",
    "validate_manifest",
    "write_manifest",
]
