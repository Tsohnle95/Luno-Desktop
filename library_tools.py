import csv
import os
import re

from utils import split_track_name


def scan_library(roots):
    lib = {}
    exts = (".mp3", ".m4a", ".wav", ".flac", ".mp4", ".webm")

    def recent_first(path):
        try:
            modified = os.path.getmtime(path)
        except OSError:
            modified = 0.0
        return (-modified, os.path.basename(path).casefold())

    for root in roots:
        if not os.path.exists(root):
            continue
        for entry in sorted(os.scandir(root), key=lambda e: e.name.lower()):
            if entry.is_dir():
                songs = sorted(
                    [os.path.join(entry.path, f) for f in os.listdir(entry.path)
                     if f.lower().endswith(exts)],
                    key=lambda x: os.path.basename(x).lower(),
                )
                lib[entry.name] = songs
        root_songs = [
            os.path.join(root, f) for f in os.listdir(root)
            if f.lower().endswith(exts)
        ]
        if root_songs:
            if "Unsorted" not in lib:
                lib["Unsorted"] = []
            existing = set(lib["Unsorted"])
            for song in root_songs:
                if song not in existing:
                    lib["Unsorted"].append(song)
        if "Unsorted" in lib:
            lib["Unsorted"].sort(key=recent_first)
    return lib


def parse_exportify_csv(filepath):
    tracks = []
    try:
        with open(filepath, newline="", encoding="utf-8-sig") as f:
            for row in csv.DictReader(f):
                title = row.get("Track Name", "").strip()
                artist = row.get("Artist Name(s)", "").strip()
                if title and artist:
                    tracks.append({
                        "title": title,
                        "artist": artist,
                        "album": row.get("Album Name", "").strip(),
                    })
    except Exception as ex:
        return [], str(ex)
    return tracks, None


def _normalize_for_dupe(text):
    """Normalize common metadata variants for duplicate detection."""
    s = text.lower()
    s = re.sub(
        r"\((?:official\s*(?:audio|video|music\s*video|lyric\s*video|visualizer)?|"
        r"lyrics?|audio|hd|hq|4k|remaster(?:ed)?|clean|explicit|"
        r"radio\s*edit|extended\s*(?:mix|version)?|music\s*video|"
        r"visualizer|bonus\s*track|deluxe|live|acoustic|remix)\)",
        "", s, flags=re.IGNORECASE,
    )
    s = re.sub(
        r"\[(?:official\s*(?:audio|video|music\s*video|lyric\s*video|visualizer)?|"
        r"lyrics?|audio|hd|hq|4k|remaster(?:ed)?|clean|explicit|"
        r"radio\s*edit|extended\s*(?:mix|version)?|music\s*video|"
        r"visualizer|bonus\s*track|deluxe|live|acoustic|remix)\]",
        "", s, flags=re.IGNORECASE,
    )
    s = re.sub(r"\b(?:feat\.?|ft\.?|featuring)\b", "feat", s)
    s = re.sub(r"\.(mp3|m4a|wav|flac|mp4|webm)$", "", s)
    return re.sub(r"[^a-z0-9]", "", s)


def find_duplicates(library, allowed_keys=None):
    """Return groups of tracks sharing a normalized metadata key."""
    buckets = {}
    if allowed_keys is None:
        allowed_keys = set()

    for playlist_name, tracks in library.items():
        for path in tracks:
            title, artist = split_track_name(path)
            key = _normalize_for_dupe(f"{artist}{title}")
            if not key or key in allowed_keys:
                continue
            entry = {
                "path": path,
                "pl": playlist_name,
                "title": title,
                "artist": artist,
            }
            buckets.setdefault(key, []).append(entry)
    return [group for group in buckets.values() if len(group) > 1]
