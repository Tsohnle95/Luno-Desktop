import os
import threading
import re

try:
    import yt_dlp
except ImportError:
    yt_dlp = None

from engine import probe_soundcloud

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
    def _create_opts(out_folder, skip_existing, template="%(title)s.%(ext)s", progress_hook=None, error_callback=None):
        opts = {
            "format": "bestaudio/best",
            "restrictfilenames": False, 
            "outtmpl": os.path.join(out_folder, template),
            "noplaylist": False,
            "quiet": True, 
            "no_warnings": True,
            "ignoreerrors": True,
            "nocheckcertificate": True,
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
        if skip_existing:
            opts["download_archive"] = os.path.join(out_folder, ".downloaded.txt")
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
