import os
import threading
import re
import time

try:
    import yt_dlp
except ImportError:
    yt_dlp = None

from engine import probe_soundcloud
from utils import split_track_name


def _download_with_timeout(opts, url, timeout=120):
    """Run yt-dlp download in a daemon thread with a timeout.

    If the download exceeds `timeout` seconds, raises TimeoutError
    and the background thread continues as a daemon (harmless).
    """
    exc = []
    done = threading.Event()

    def run():
        try:
            with yt_dlp.YoutubeDL(opts) as ydl:
                ydl.download([url])
        except Exception as e:
            exc.append(e)
        finally:
            done.set()

    t = threading.Thread(target=run, daemon=True)
    t.start()
    if not done.wait(timeout):
        raise TimeoutError(f"Download timed out after {timeout}s")
    if exc:
        raise exc[0]

class YDLLogger:
    """Redirects yt-dlp logs to silence the console while allowing internal logging."""
    def __init__(self, error_callback=None):
        self.error_callback = error_callback

    def debug(self, msg): pass
    def warning(self, msg): 
        if self.error_callback: self.error_callback(msg)

    def error(self, msg): 
        # Send full detail to the app's internal log exclusively
        if self.error_callback:
            self.error_callback(msg)

class Downloader:
    """Handles all background yt-dlp downloading logic, decoupling it from Views."""
    
    @staticmethod
    def _create_opts(out_folder, skip_existing, template="%(title)s.%(ext)s", progress_hook=None, error_callback=None, ignoreerrors=True):
        opts = {
            "format": "bestaudio/best",
            "restrictfilenames": False, 
            "outtmpl": os.path.join(out_folder, template),
            "noplaylist": False,
            "quiet": True, 
            "no_warnings": True,
            "ignoreerrors": ignoreerrors,
            "nocheckcertificate": True,
            "socket_timeout": 30,
            "retries": 5,
            "fragment_retries": 5,
            "extractor_retries": 3,
            "logger": YDLLogger(error_callback),
            "progress_hooks": [progress_hook] if progress_hook else [],
            "postprocessors": [
                {
                    "key": "FFmpegExtractAudio",
                    "preferredcodec": "mp3",
                    "preferredquality": "192",
                },
                {
                    "key": "EmbedThumbnail",
                    "already_have_thumbnail": False
                }
            ],
            "postprocessor_args": {
                "FFmpegExtractAudio": [
                    "-ar", "44100",
                    "-ac", "2",
                    "-b:a", "192k",
                    "-af", "loudnorm=I=-14:LRA=11:TP=-1.5"
                ]
            },
            "writethumbnail": True,
            "verbose": False, 
        }
        return opts

    @staticmethod
    def download_single(url, out_folder, on_complete, on_error, progress_hook=None, error_callback=None):
        if not yt_dlp:
            on_error("yt-dlp not available")
            return
            
        def run():
            try:
                opts = Downloader._create_opts(out_folder, False, progress_hook=progress_hook, error_callback=error_callback)
                with yt_dlp.YoutubeDL(opts) as ydl:
                    ydl.download([url])
                on_complete()
            except Exception as e:
                on_error(e)
        threading.Thread(target=run, daemon=True).start()

    @staticmethod
    def download_youtube_url(url, out_folder, skip_existing, on_complete, on_error, progress_hook=None, error_callback=None):
        """Used for entire YT playlist URLs or single YT videos to preserve original title"""
        if not yt_dlp:
            on_error("yt-dlp not available")
            return
            
        def run():
            try:
                opts = Downloader._create_opts(out_folder, skip_existing, "%(title)s.%(ext)s", progress_hook=progress_hook, error_callback=error_callback)
                # Force quiet to false to see output if needed, but we keep it quiet for UI mostly
                with yt_dlp.YoutubeDL(opts) as ydl:
                    ydl.download([url])
                on_complete()
            except Exception as e:
                on_error(e)
        threading.Thread(target=run, daemon=True).start()

    @staticmethod
    def download_csv_tracks(tracks, get_out_folder_fn, skip_existing, try_sc, on_progress, on_log, on_complete, check_cancel):
        """
        tracks: list of dicts with 'artist', 'title', optional 'playlist_name'
        get_out_folder_fn: lambda playlist_name: return save directory
        """
        if not yt_dlp:
            on_complete()
            return
            
        def run():
            total = len(tracks)
            done = 0
            for t in tracks:
                if check_cancel(): break
                
                artist = t.get('artist', '')
                title = t.get('title', '')
                q = f"{artist} {title}".strip()
                safe_name = re.sub(r'[<>:"/\\|?*]', '', f"{artist} - {title}").strip()
                if not safe_name: safe_name = "Unknown"
                
                # Notify UI we are searching/starting this track
                on_progress(done, total, q)
                
                use_url = t.get("url")
                if try_sc and not use_url:
                    sc_url, _ = probe_soundcloud(q)
                    if sc_url: use_url = sc_url
                    
                if not use_url:
                    use_url = f"ytsearch1:{q}"

                pl_name = t.get("playlist_name")
                out_folder = get_out_folder_fn(pl_name)
                os.makedirs(out_folder, exist_ok=True)
                
                opts = Downloader._create_opts(out_folder, skip_existing, f"{safe_name}.%(ext)s", progress_hook=None, error_callback=lambda msg: on_log(title, False, msg))
                try:
                    with yt_dlp.YoutubeDL(opts) as ydl:
                        ydl.download([use_url])
                    done += 1
                    on_log(title, True, None)
                    on_progress(done, total, None) # Update count
                except Exception as e:
                    on_log(title, False, repr(e))
                    
            on_complete()
            
        threading.Thread(target=run, daemon=True).start()

    @staticmethod
    def download_playlist_sync(url, out_folder, skip_existing, try_sc,
                               on_progress, on_log, on_complete, check_cancel,
                               on_playlist_msg=None):
        """Sync a YT playlist URL to a local folder. Extracts metadata, compares
        against existing files, and downloads only new tracks.

        on_log: called per-track with (title, is_success, err) — track results only.
        on_playlist_msg: optional callback (msg) for informational playlist messages.
        """
        if not yt_dlp:
            on_complete()
            return

        def run():
            try:
                with yt_dlp.YoutubeDL({"quiet": True, "extract_flat": "in_playlist",
                                        "ignoreerrors": True,
                                        "socket_timeout": 30}) as ydl:
                    info = ydl.extract_info(url, download=False)
            except Exception as ex:
                on_log("", False, f"Failed to fetch playlist: {ex}")
                on_complete()
                return

            try:
                entries = info.get("entries") or []
                if not entries:
                    if on_playlist_msg:
                        on_playlist_msg("No entries found")
                    on_complete()
                    return

                if on_playlist_msg:
                    on_playlist_msg(f"Searching {len(entries)} track(s)")

                vid_path = os.path.join(out_folder, ".downloaded_vids")
                legacy_vid_path = os.path.join(out_folder, ".downloaded.txt")
                known_vids = set()
                try:
                    if os.path.exists(vid_path):
                        with open(vid_path, "r", encoding="utf-8") as f:
                            known_vids = {line.strip() for line in f if line.strip()}
                except Exception:
                    known_vids = set()
                try:
                    if os.path.exists(legacy_vid_path):
                        with open(legacy_vid_path, "r", encoding="utf-8") as f:
                            for line in f:
                                parts = line.strip().split()
                                if len(parts) >= 2:
                                    known_vids.add(parts[-1])
                except Exception:
                    pass

                def clean_title(value):
                    value = value or ""
                    value = re.sub(r'\s+', ' ', value).strip()
                    value = re.sub(
                        r'\s*[\[(](official\s+)?(music\s+)?(video|audio|visualizer|lyrics?|lyric\s+video|mv|hd|4k)[\])]',
                        '', value, flags=re.IGNORECASE)
                    value = re.sub(r'\s*\[(official\s+)?(video|audio|visualizer|lyrics?|hd|4k)\]\s*$', '', value, flags=re.IGNORECASE)
                    value = re.sub(r'\s*\((official\s+)?(video|audio|visualizer|lyrics?|hd|4k)\)\s*$', '', value, flags=re.IGNORECASE)
                    return value.strip()

                def make_key(artist, title):
                    return re.sub(r'[\W_]', '', f"{clean_title(artist)}{clean_title(title)}").lower()

                def title_key(title):
                    return re.sub(r'[\W_]', '', clean_title(title)).lower()

                def save_vids(vids):
                    new_vids = [vid for vid in vids if vid and vid not in known_vids]
                    if not new_vids:
                        return
                    try:
                        with open(vid_path, "a", encoding="utf-8") as f:
                            for vid in new_vids:
                                f.write(f"{vid}\n")
                                known_vids.add(vid)
                    except Exception:
                        pass

                def audio_paths_for(safe_name):
                    return {
                        os.path.join(out_folder, f"{safe_name}{ext}")
                        for ext in (".mp3", ".m4a", ".wav", ".flac", ".webm")
                    }

                def make_safe_name(artist, title):
                    if artist and title:
                        name = f"{artist} - {title}"
                    else:
                        name = title or artist or "Unknown"
                    name = re.sub(r'[<>:"/\\|?*]', '', name).strip()
                    return name or "Unknown"

                # Build known set from existing files
                known = set()
                known_titles = set()
                if os.path.isdir(out_folder):
                    for fname in os.listdir(out_folder):
                        if fname.lower().endswith((".mp3", ".m4a", ".wav", ".flac", ".webm")):
                            track_name, artist = split_track_name(fname)
                            known.add(make_key(artist, track_name))
                            known_titles.add(title_key(track_name))

                new_tracks = []
                matched_vids = set()
                skipped_missing = 0
                for e in entries:
                    if not e: continue
                    vid = e.get('id')
                    t_full = e.get("title", "") or ""
                    v_url = (e.get("url") or e.get("webpage_url") or
                             (f"https://www.youtube.com/watch?v={e.get('id')}"
                              if e.get('id') else None))
                    if " - " in t_full:
                        p = t_full.split(" - ", 1)
                        artist_part, title_part = p[0].strip(), p[1].strip()
                    else:
                        artist_part, title_part = "", t_full
                    entry_key = make_key(artist_part, title_part)
                    entry_title_key = title_key(title_part)
                    if vid and vid in known_vids:
                        continue
                    if not entry_key and not entry_title_key:
                        skipped_missing += 1
                        continue
                    if entry_key in known or (not artist_part and entry_title_key in known_titles):
                        if vid:
                            matched_vids.add(vid)
                        continue
                    safe_candidate = make_safe_name(artist_part, title_part or "Unknown")
                    if any(os.path.exists(p) for p in audio_paths_for(safe_candidate)):
                        if vid:
                            matched_vids.add(vid)
                        continue
                    if v_url:
                        new_tracks.append({"artist": artist_part, "title": title_part, "url": v_url, "vid": vid})

                save_vids(matched_vids)
                if skipped_missing and on_playlist_msg:
                    on_playlist_msg(f"Skipped {skipped_missing} untitled unavailable item(s)")

                if not new_tracks:
                    if on_playlist_msg:
                        on_playlist_msg(f"Searched {len(entries)} track(s) — up to date")
                    on_complete()
                    return

                total = len(new_tracks)
                if on_playlist_msg:
                    on_playlist_msg(f"Found {total} new track(s) out of {len(entries)} total")

                for i, t in enumerate(new_tracks):
                    if check_cancel(): break
                    title = t['title'] or "Unknown Track"
                    on_progress(i, total, title)
                    if on_playlist_msg:
                        on_playlist_msg(f"Downloading {i + 1}/{total}: {title}")

                    use_url = t.get("url")
                    if try_sc and not use_url:
                        q = f"{t['artist']} {title}".strip()
                        sc_url, _ = probe_soundcloud(q)
                        if sc_url: use_url = sc_url
                    if not use_url:
                        use_url = f"ytsearch1:{q}"

                    safe_name = make_safe_name(t['artist'], t['title'] or 'Unknown')

                    expected_paths = audio_paths_for(safe_name)
                    before_paths = {p for p in expected_paths if os.path.exists(p)}
                    opts = Downloader._create_opts(out_folder, skip_existing, f"{safe_name}.%(ext)s",
                                                    error_callback=lambda _: None, ignoreerrors=False)
                    try:
                        _download_with_timeout(opts, use_url, timeout=120)
                        after_paths = {p for p in expected_paths if os.path.exists(p)}
                        if not after_paths - before_paths:
                            raise FileNotFoundError("download completed but no audio file was created")
                        save_vids([t.get("vid")])
                        on_log(title, True, None)
                    except TimeoutError:
                        on_log(title, False, "Download timed out (120s)")
                    except Exception as e:
                        on_log(title, False, repr(e))
                    on_progress(i + 1, total, None)

                on_complete()
            except Exception as ex:
                on_log("", False, f"Sync processing error: {ex}")
                on_complete()

        threading.Thread(target=run, daemon=True).start()

    @staticmethod
    def download_sync_tracks(tracks, out_folder, skip_existing, try_sc, on_progress, on_log, on_complete, check_cancel):
        if not yt_dlp:
            on_complete()
            return
            
        def run():
            for i, t in enumerate(tracks):
                if check_cancel(): break
                
                artist = t.get('artist', '')
                title = t.get('title', '')
                q = f"{artist} {title}".strip()
                safe_name = re.sub(r'[<>:"/\\|?*]', '', f"{artist} - {title}").strip()
                if not safe_name: safe_name = "Unknown"
                
                on_progress(i, len(tracks), title)
                
                use_url = t.get("url")
                if try_sc and not use_url:
                    sc_url, _ = probe_soundcloud(q)
                    if sc_url: use_url = sc_url
                if not use_url:
                    use_url = f"ytsearch1:{q}"

                opts = Downloader._create_opts(out_folder, skip_existing, f"{safe_name}.%(ext)s", error_callback=lambda msg: on_log(title, False, msg))
                try:
                    with yt_dlp.YoutubeDL(opts) as ydl:
                        ydl.download([use_url])
                    on_log(title, True, None)
                    on_progress(i + 1, len(tracks), None) # update tick count
                except Exception as e:
                    on_log(title, False, repr(e))
                    
            on_complete()
            
        threading.Thread(target=run, daemon=True).start()
