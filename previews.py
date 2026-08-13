"""Temporary recommendation preview pipeline (mobile
RecommendationPreviewManager resolveRecommendation/downloadPreview parity).

Previews are staged OUTSIDE the library root so library scans never see them.
Resolution runs YouTube search with a hard 15s deadline and a duration guard
(1s..3h, like the mobile NewPipe search); the download streams the audio
straight through FFmpeg to a 44.1 kHz / 192k MP3 — but WITHOUT the library's
EBU R128 loudnorm pass, because transient previews mirror the mobile raw-stream
path (fast, disposable). The library downloader's single job slot is never
touched, so a preview chain can run while a real download is in flight.
"""

import os
import re

try:
    import yt_dlp
except ImportError:
    yt_dlp = None

from downloader import (
    YDLLogger,
    DownloadCancelled,
    DownloadTimedOut,
    _download_with_timeout,
)

PREVIEW_RESOLVE_TIMEOUT_S = 15
PREVIEW_DOWNLOAD_TIMEOUT_S = 5 * 60
MAX_PREVIEW_SOURCE_DURATION_S = 3 * 60 * 60
MAX_PREVIEW_BYTES = 512 * 1024 * 1024

PREVIEW_RESOLVE_TIMED_OUT_MESSAGE = "Finding a playable source timed out. Tap to retry."
PREVIEW_DOWNLOAD_TIMED_OUT_MESSAGE = "Preview download timed out. Tap to retry."
PREVIEW_TOO_LARGE_MESSAGE = (
    "This source is too large for a quick preview. Use Download to playlist "
    "to save the full song."
)

PREVIEW_EXTS = (".mp3", ".m4a", ".wav", ".flac", ".webm")


class PreviewError(RuntimeError):
    pass


class PreviewTimeoutError(PreviewError):
    pass


class PreviewTooLargeError(PreviewError):
    pass


class PreviewCancelledError(PreviewError):
    pass


def safe_name(artist, title):
    name = f"{artist} - {title}" if artist else (title or "Unknown")
    return re.sub(r'[<>:"/\\|?*]', "", name).strip() or "Unknown"


def resolve_preview_source(artist, title):
    """Mobile resolveRecommendation parity: YouTube search (ytsearch<5>),
    first candidate with a duration of 1s..3h, else the first with an unknown
    duration. Returns a dict with url/video_id/thumbnail or raises
    PreviewError."""
    if not yt_dlp:
        raise PreviewError("yt-dlp not available")
    query = f"{artist} {title}".strip()
    with yt_dlp.YoutubeDL({
        "quiet": True,
        "extract_flat": "in_playlist",
        "ignoreerrors": True,
        "socket_timeout": 30,
    }) as ydl:
        info = ydl.extract_info(f"ytsearch5:{query}", download=False)
    entries = [e for e in (info or {}).get("entries") or [] if e]
    candidate = None
    for entry in entries:
        duration = entry.get("duration") or 0
        if 1 <= duration <= MAX_PREVIEW_SOURCE_DURATION_S:
            candidate = entry
            break
    if candidate is None:
        candidate = next(
            (e for e in entries if not (e.get("duration") or 0)), None
        )
    if candidate is None:
        raise PreviewError("Could not find a song-length version on YouTube")
    video_id = candidate.get("id")
    return {
        "video_id": video_id,
        "url": (candidate.get("webpage_url")
                or (f"https://www.youtube.com/watch?v={video_id}"
                    if video_id else "")),
        "thumbnail": (candidate.get("thumbnail")
                      or (f"https://i.ytimg.com/vi/{video_id}/hqdefault.jpg"
                          if video_id else "")),
        "title": candidate.get("title") or "",
        "duration": candidate.get("duration") or 0,
    }


def _preview_opts(target_dir, safe, cancel_check, error_callback=None):
    hooks = []
    if cancel_check:
        def guarded_progress(progress):
            if cancel_check():
                raise DownloadCancelled("Preview cancelled")
        hooks.append(guarded_progress)
    return {
        "format": "bestaudio/best",
        "outtmpl": os.path.join(target_dir, f"{safe}.%(ext)s"),
        "noplaylist": True,
        "quiet": True,
        "no_warnings": True,
        "ignoreerrors": False,
        "nocheckcertificate": True,
        "socket_timeout": 30,
        "retries": 3,
        "fragment_retries": 3,
        "extractor_retries": 2,
        "logger": YDLLogger(error_callback),
        "progress_hooks": hooks,
        "postprocessors": [
            {"key": "FFmpegExtractAudio", "preferredcodec": "mp3",
             "preferredquality": "192"},
            {"key": "EmbedThumbnail", "already_have_thumbnail": False},
        ],
        "postprocessor_args": {
            # 44.1 kHz is mandatory; loudnorm is skipped for transient
            # previews (mobile parity: previews are raw streams, not library
            # copies), which keeps them quick.
            "FFmpegExtractAudio": ["-ar", "44100", "-ac", "2", "-b:a", "192k"],
        },
        "writethumbnail": True,
        "verbose": False,
    }


def download_preview(artist, title, source_url, target_dir,
                     timeout=PREVIEW_DOWNLOAD_TIMEOUT_S, cancel_check=None,
                     error_callback=None):
    """Downloads a temporary preview into target_dir and returns the final
    audio path. Raises PreviewError / PreviewTimeoutError /
    PreviewTooLargeError / PreviewCancelledError (mobile message parity)."""
    if not yt_dlp:
        raise PreviewError("yt-dlp not available")
    if not source_url:
        raise PreviewError("Could not prepare this song")
    try:
        os.makedirs(target_dir, exist_ok=True)
    except OSError as exc:
        raise PreviewError(f"Could not create preview folder: {exc}")
    safe = safe_name(artist, title)
    opts = _preview_opts(target_dir, safe, cancel_check, error_callback)
    try:
        _download_with_timeout(opts, source_url, timeout=timeout)
    except DownloadTimedOut:
        raise PreviewTimeoutError(PREVIEW_DOWNLOAD_TIMED_OUT_MESSAGE)
    except DownloadCancelled:
        raise PreviewCancelledError("Preview download cancelled")
    for ext in PREVIEW_EXTS:
        path = os.path.join(target_dir, f"{safe}{ext}")
        if os.path.isfile(path):
            if os.path.getsize(path) > MAX_PREVIEW_BYTES:
                try:
                    os.remove(path)
                except OSError:
                    pass
                raise PreviewTooLargeError(PREVIEW_TOO_LARGE_MESSAGE)
            return path
    raise PreviewError("Downloaded audio could not be played")
