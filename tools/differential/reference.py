"""Reference adapter for deterministic, target-independent operations.

The adapter deliberately accepts synthetic paths and records. It does not scan
the caller's filesystem or invoke the future implementation.
"""

from __future__ import annotations

import os
from typing import Any

from library_tools import _normalize_for_dupe
from utils import split_track_name

OPERATIONS = {
    "parse_track_name",
    "normalize_duplicate",
    "scan_order",
    "duplicates",
    "queue_insert",
    "duration_match",
    "markers",
}


def _scan_order(value: dict[str, Any]) -> dict[str, list[str]]:
    """Apply the Python scanner's observable ordering to synthetic entries."""
    playlists: dict[str, list[dict[str, Any]]] = {}
    for entry in value.get("entries", []):
        if not str(entry.get("extension", os.path.splitext(entry["path"])[1])).lower() in {
            ".mp3", ".m4a", ".wav", ".flac", ".mp4", ".webm",
        }:
            continue
        playlist = entry.get("playlist", "Unsorted")
        playlists.setdefault(playlist, []).append(entry)

    result: dict[str, list[str]] = {}
    # os.scandir entries are case-insensitively sorted; root files are added
    # afterward as Unsorted, matching library_tools.scan_library.
    ordered_playlists = sorted((p for p in playlists if p != "Unsorted"), key=str.casefold)
    if "Unsorted" in playlists:
        ordered_playlists.append("Unsorted")
    for playlist in ordered_playlists:
        entries = playlists[playlist]
        if playlist == "Unsorted":
            entries = sorted(entries, key=lambda e: (-float(e.get("mtime", 0)), os.path.basename(e["path"]).casefold()))
        else:
            entries = sorted(entries, key=lambda e: os.path.basename(e["path"]).lower())
        result[playlist] = [e["path"] for e in entries]
    return result


def _duplicates(value: dict[str, Any]) -> list[list[dict[str, str]]]:
    buckets: dict[str, list[dict[str, str]]] = {}
    allowed = set(value.get("allowed_keys", []))
    for playlist, paths in value.get("library", {}).items():
        for path in paths:
            title, artist = split_track_name(path)
            key = _normalize_for_dupe(f"{artist}{title}")
            if not key or key in allowed:
                continue
            buckets.setdefault(key, []).append({
                "path": path, "pl": playlist, "title": title, "artist": artist,
            })
    return [group for group in buckets.values() if len(group) > 1]


def _queue_insert(value: dict[str, Any]) -> dict[str, Any]:
    queue = list(value.get("queue", []))
    selected = list(value.get("selected", []))
    index = int(value.get("queue_idx", -1))
    user_count = int(value.get("user_queue_count", 0))
    mode = value["mode"]
    if not selected:
        return {"queue": queue, "queue_idx": index, "user_queue_count": user_count}
    if not queue:
        return {"queue": selected, "queue_idx": 0, "user_queue_count": len(selected) - 1}
    for path in (reversed(selected) if mode == "play_next" else selected):
        insertion = index + 1 if mode == "play_next" and index >= 0 else (0 if mode == "play_next" else index + 1 + user_count)
        insertion = max(0, min(insertion, len(queue)))
        queue.insert(insertion, path)
        user_count += 1
    return {"queue": queue, "queue_idx": index, "user_queue_count": user_count}


def _markers(value: dict[str, Any]) -> dict[str, list[str]]:
    # Marker files are line-oriented sets. Failed markers may carry a tabbed
    # error message; legacy entries contain a URL/title followed by the ID.
    def clean(key: str) -> list[str]:
        return [line.strip() for line in value.get(key, []) if isinstance(line, str) and line.strip()]
    downloaded = clean("downloaded")
    failed = [line.split("\t", 1)[0] for line in clean("failed")]
    for line in clean("legacy"):
        parts = line.split()
        if len(parts) >= 2:
            downloaded.append(parts[-1])
    return {"downloaded": list(dict.fromkeys(downloaded)), "failed": list(dict.fromkeys(failed)), "legacy": clean("legacy")}


def evaluate(operation: str, value: dict[str, Any]) -> dict[str, Any]:
    if operation not in OPERATIONS:
        raise ValueError(f"unsupported differential operation: {operation}")
    if operation == "parse_track_name":
        title, artist = split_track_name(value["path"])
        return {"title": title, "artist": artist}
    if operation == "normalize_duplicate":
        return {"key": _normalize_for_dupe(value["text"])}
    if operation == "scan_order":
        return _scan_order(value)
    if operation == "duplicates":
        return {"groups": _duplicates(value)}
    if operation == "queue_insert":
        return _queue_insert(value)
    if operation == "duration_match":
        expected = int(value["expected_ms"])
        actual = int(value["actual_ms"])
        return {"match": expected == 0 or actual == 0 or abs(actual - expected) <= 10_000}
    return _markers(value)
