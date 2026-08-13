"""Optional video lookup and staging for the desktop song page.

Video files are temporary presentation assets, not library downloads.  The
renderer is optional because Tk itself has no video widget; when ``tkvideo`` is
not installed the lookup still reports that limitation without affecting audio.
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
    """Find a YouTube result that has both video and audio streams."""
    if yt_dlp is None:
        raise VideoPreviewError("yt-dlp is not available")
    query = f"{artist} {title}".strip()
    try:
        with yt_dlp.YoutubeDL({
            "quiet": True,
            "extract_flat": "in_playlist",
            "ignoreerrors": True,
            "socket_timeout": 30,
        }) as ydl:
            info = ydl.extract_info(f"ytsearch5:{query}", download=False)
    except Exception as exc:
        raise VideoPreviewError(f"Video search failed: {exc}") from exc

    entries = [entry for entry in (info or {}).get("entries") or [] if entry]
    for entry in entries:
        if entry.get("id"):
            return {
                "url": entry.get("webpage_url")
                or f"https://www.youtube.com/watch?v={entry['id']}",
                "title": entry.get("title") or title,
                "duration": entry.get("duration") or 0,
            }
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
