"""Optional video lookup and staging for the desktop song page.

Video files are temporary presentation assets, not library downloads.
"""

import os
import re

try:
    import yt_dlp
except ImportError:
    yt_dlp = None


VIDEO_EXTS = (".mp4", ".mkv", ".webm", ".mov")


class VideoPreviewError(RuntimeError):
    pass


def _safe_name(artist, title):
    value = f"{artist} - {title}" if artist else title or "Unknown"
    return re.sub(r'[<>:"/\\|?*]', "", value).strip() or "Unknown"


def resolve_video_source(artist, title):
    """Find the best usable YouTube result across several search variants.

    Filenames are not always ``artist - title`` and YouTube's flat search
    entries do not consistently include ``webpage_url``, so search broadly and
    always build the canonical watch URL from the result id.
    """
    if yt_dlp is None:
        raise VideoPreviewError("yt-dlp is not available")
    queries = []
    for query in (f"{artist} - {title}", f"{title} {artist}", title):
        query = query.strip()
        if query and query not in queries:
            queries.append(query)
    entries = []
    seen_ids = set()
    errors = []
    try:
        with yt_dlp.YoutubeDL({
            "quiet": True,
            "extract_flat": "in_playlist",
            "ignoreerrors": True,
            "socket_timeout": 30,
        }) as ydl:
            for query in queries:
                try:
                    info = ydl.extract_info(f"ytsearch10:{query}", download=False)
                except Exception as exc:
                    errors.append(str(exc))
                    continue
                for entry in (info or {}).get("entries") or []:
                    video_id = entry.get("id") if entry else None
                    if entry and video_id and video_id not in seen_ids:
                        seen_ids.add(video_id)
                        entries.append(entry)
    except Exception as exc:
        raise VideoPreviewError(f"Video search failed: {exc}") from exc

    if entries:
        wanted = f"{artist} {title}".lower()
        entries.sort(key=lambda entry: (
            wanted not in (entry.get("title") or "").lower(),
            -(entry.get("duration") or 0) if entry.get("duration") else 0,
        ))
        entry = entries[0]
        video_id = entry["id"]
        return {
            "url": f"https://www.youtube.com/watch?v={video_id}",
            "title": entry.get("title") or title,
            "duration": entry.get("duration") or 0,
        }
    if errors:
        raise VideoPreviewError(f"YouTube search failed: {errors[-1]}")
    raise VideoPreviewError("No video was found for this song")


def download_video(artist, title, source_url, target_dir):
    """Download one merged MP4 and return its path."""
    if yt_dlp is None:
        raise VideoPreviewError("yt-dlp is not available")
    os.makedirs(target_dir, exist_ok=True)
    safe = _safe_name(artist, title)
    opts = {
        "format": "bestvideo[ext=mp4]+bestaudio[ext=m4a]/best[ext=mp4]/best",
        "outtmpl": os.path.join(target_dir, f"{safe}.%(ext)s"),
        "merge_output_format": "mp4",
        "noplaylist": True,
        "quiet": True,
        "no_warnings": True,
        "ignoreerrors": False,
        "socket_timeout": 30,
        "retries": 2,
        "fragment_retries": 2,
    }
    try:
        with yt_dlp.YoutubeDL(opts) as ydl:
            ydl.download([source_url])
    except Exception as exc:
        raise VideoPreviewError(f"Video download failed: {exc}") from exc
    for ext in VIDEO_EXTS:
        path = os.path.join(target_dir, f"{safe}{ext}")
        if os.path.isfile(path):
            return path
    raise VideoPreviewError("The downloaded video could not be opened")


def find_video(artist, title, target_dir):
    """Return an already staged video for this track, if one exists."""
    safe = _safe_name(artist, title)
    for ext in VIDEO_EXTS:
        path = os.path.join(target_dir, f"{safe}{ext}")
        if os.path.isfile(path):
            return path
    return None
