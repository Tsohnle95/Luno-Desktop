try:
    import yt_dlp
    YT_DLP_AVAILABLE = True
except ImportError:
    yt_dlp = None
    YT_DLP_AVAILABLE = False


def probe_soundcloud(query):
    if not yt_dlp:
        return None, None
    try:
        with yt_dlp.YoutubeDL({"quiet": True, "skip_download": True}) as ydl:
            info = ydl.extract_info(f"scsearch1:{query}", download=False)
        if not info:
            return None, None
        best = max(
            (fmt.get("abr") or 0 for fmt in info.get("formats", [])
             if fmt.get("acodec", "none") != "none"),
            default=0,
        )
        return (f"scsearch1:{query}", f"{int(best)}kbps") if best > 180 else (None, None)
    except Exception:
        return None, None
