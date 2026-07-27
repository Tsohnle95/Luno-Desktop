#!/opt/homebrew/bin/python3.12
"""
Vibe Music Player v4
- Fixed library search (exact title/artist matching)
- Round play button via Canvas
- Modern thin scrubber and volume slider via Canvas (not ttk Scale)
- Duplicate song detection and management
- Gradient playlist art headers
build command: pyinstaller --noconsole --windowed --noconfirm --clean --name "Vibe" music_player.py
pyinstaller --noconsole --windowed --noconfirm --clean 
  --name "DevTunes" 
  --icon "icon.icns" 
  music_player.py

"""



import tkinter as tk
from tkinter import ttk, filedialog, messagebox, simpledialog
import threading
import queue
import os, re, csv, time, random, math, hashlib, shutil, traceback, json
from urllib.parse import urlparse
from datetime import datetime
os.environ["PATH"] += os.pathsep + "/opt/homebrew/bin" + os.pathsep + "/usr/local/bin"
os.environ["PATH"] += os.pathsep + "/opt/homebrew/bin"

import sys
from theme import *
from utils import resource_path, format_time, hex_to_rgb, rgb_to_hex, draw_gradient_rect, split_track_name
from widgets import RoundButton, SlimSlider, PlaylistArt
from views.library import LibraryView
from views.downloader import DownloadView
from views.recent import RecentView
from views.artists import ArtistsView
from views.settings import SettingsView
from engine import (
    scan_library, probe_soundcloud, parse_exportify_csv, find_duplicates,
    PYGAME_AVAILABLE, YT_DLP_AVAILABLE, MUTAGEN_AVAILABLE,
    pygame, yt_dlp, MP3, VibeEngine, DiscoveryService
)

from views.discover import DiscoverView
from views.song_page import SongPageView

# ── LIBRARY PATH SETUP ────────────────────────────────────────────────────────
# Default to ~/Music/app_music_folder
MUSIC_ROOT = os.path.join(os.path.expanduser("~"), "Music", "app_music_folder")

try:
    os.makedirs(MUSIC_ROOT, exist_ok=True)
except Exception as e:
    print(f"Error creating library folder: {e}")

WATCH_ROOTS = [MUSIC_ROOT]
CONFIG_FILE = os.path.join(os.path.expanduser("~"), ".vibe_config.json")
# ──────────────────────────────────────────────────────────────────────────────
# ──────────────────────────────────────────────────────────────────────────────


from concurrent.futures import ThreadPoolExecutor

try:
    import objc
    from Foundation import NSObject
    _HAS_PYOBJC = True
except ImportError:
    _HAS_PYOBJC = False

try:
    from MediaPlayer import (
        MPNowPlayingInfoCenter,
        MPMediaItemPropertyTitle,
        MPMediaItemPropertyArtist,
        MPMediaItemPropertyPlaybackDuration,
        MPNowPlayingInfoPropertyPlaybackRate,
        MPNowPlayingInfoPropertyElapsedPlaybackTime,
        MPRemoteCommandCenter,
        MPRemoteCommandHandlerStatus,
    )
    _HAS_MEDIAPLAYER = True
except ImportError:
    _HAS_MEDIAPLAYER = False

class ArtCache:
    def __init__(self, app):
        self.app = app
        self.cache = {}
        self._color_cache = {}
        self._color_fetching = set()
        self.fetching = set()
        self.executor = ThreadPoolExecutor(max_workers=4)

    def get(self, path, size, callback):
        if not path:
            return None
        key = (path, size)
        if key in self.cache:
            return self.cache[key]
            
        if key not in self.fetching:
            self.fetching.add(key)
            self.executor.submit(self._fetch, path, size, callback)
        return None

    def get_dominant_color(self, path, fallback="#1DB954"):
        if not path:
            return fallback
        if path in self._color_cache:
            return self._color_cache[path]
        if path not in self._color_fetching:
            self._color_fetching.add(path)
            self.executor.submit(self._fetch_color, path)
        return fallback

    def _fetch_color(self, path):
        try:
            from mutagen.mp3 import MP3
            from mutagen.id3 import ID3
            from PIL import Image
            import io
            audio = MP3(path, ID3=ID3)
            if audio.tags:
                for tag in audio.tags.values():
                    if getattr(tag, 'FrameID', None) == 'APIC':
                        img = Image.open(io.BytesIO(tag.data)).convert("RGB")
                        tiny = img.resize((1, 1), Image.Resampling.LANCZOS)
                        color = tiny.getpixel((0, 0))
                        hex_color = rgb_to_hex(*color)
                        self._color_cache[path] = hex_color
                        self._color_fetching.discard(path)
                        self.app.after(0, lambda: self.app._update_player_accent(path))
                        return
            self._color_fetching.discard(path)
        except Exception:
            self._color_fetching.discard(path)

    def _fetch(self, path, size, callback):
        try:
            from mutagen.mp3 import MP3
            from mutagen.id3 import ID3
            from PIL import Image, ImageTk
            import io
            
            img_data = None
            audio = MP3(path, ID3=ID3)
            if audio.tags:
                for tag in audio.tags.values():
                    if tag.FrameID == 'APIC':
                        img_data = tag.data
                        break
            if img_data:
                img = Image.open(io.BytesIO(img_data))
                img = img.convert("RGBA")
                min_dim = min(img.width, img.height)
                left = (img.width - min_dim)/2
                top = (img.height - min_dim)/2
                img = img.crop((left, top, left+min_dim, top+min_dim))
                img = img.resize((size, size), Image.Resampling.LANCZOS)
                
                # Extract dominant color for player bar accent
                try:
                    rgb_img = img.convert("RGB")
                    tiny = rgb_img.resize((1, 1), Image.Resampling.LANCZOS)
                    color = tiny.getpixel((0, 0))
                    self._color_cache[path] = rgb_to_hex(*color)
                    self._color_fetching.discard(path)
                except Exception:
                    pass
                
                def _done(i=img, p=path, s=size, cb=callback):
                    # Must be created in main thread
                    try:
                        pi = ImageTk.PhotoImage(i)
                        self.cache[(p, s)] = pi
                        self.fetching.discard((p, s))
                        cb(p)
                        self.app._update_player_accent(p)
                    except Exception:
                        self.fetching.discard((p, s))
                self.app.after(0, _done)
            else:
                self.fetching.discard((path, size))
        except Exception:
            self.fetching.discard((path, size))

# ── Main App ──────────────────────────────────────────────────────────────
class VibeMusicPlayer(tk.Tk):
    def _ctx_set_playlist_url(self, pl_name):
        """Prompt user to set or remove a playlist URL."""
        current = self._get_playlist_url(pl_name) or ""
        new_url = simpledialog.askstring(
            "Playlist URL",
            f"Enter YouTube playlist URL for '{pl_name}':\n\nLeave empty and press OK to remove the current URL.",
            parent=self,
            initialvalue=current
        )
        if new_url is None:
            return  # cancelled
        new_url = new_url.strip()
        if new_url:
            parsed = urlparse(new_url)
            if parsed.scheme not in ('http', 'https') or not parsed.netloc:
                messagebox.showwarning("Invalid URL", "Must be a valid http/https URL.")
                return
        self._set_playlist_url(pl_name, new_url)
        if new_url:
            messagebox.showinfo("Saved", f"Playlist URL saved for '{pl_name}'.")
        else:
            messagebox.showinfo("Removed", f"Playlist URL removed for '{pl_name}'.")
        self._populate_sidebar()

    def _ctx_dump_playlist(self, source_pl_name):
        """Copies all songs from one playlist folder to another."""
        if source_pl_name == "All Music":
            messagebox.showwarning("Warning", "Cannot dump 'All Music'. Select a specific folder.")
            return
        
        paths = self._library.get(source_pl_name, [])
        if not paths:
            messagebox.showinfo("Empty", "This playlist has no songs to move.")
            return
            
        self._show_add_to_playlist_dialog(paths, f"Dump all from {source_pl_name}")

    def _show_add_to_playlist_dialog(self, paths, title_text):
        """Styled dialog to        # Global Media Keys Toggle
        row_keys = tk.Frame(viz_box, bg=BG)
        row_keys.pack(fill="x", pady=(20, 0))
        tk.Label(row_keys, text="Global Media Keys (Experimental / macOS Crash Risk)", font=("Helvetica Neue", 11), fg=TEXT2, bg=BG).pack(side="left")
lder."""
        playlists = [p for p in self._library.keys()]
        if not playlists:
            messagebox.showinfo("No Playlists", "Create a destination playlist first.")
            return

        dlg = tk.Toplevel(self)
        dlg.title(title_text)
        dlg.configure(bg=BG2)
        dlg.geometry("360x480")
        dlg.resizable(False, False)
        dlg.transient(self)
        dlg.grab_set()
        
        tk.Label(dlg, text=title_text, font=("Helvetica Neue", 14, "bold"), 
                 fg=TEXT, bg=BG2).pack(pady=(25, 5))
        tk.Label(dlg, text=f"{len(paths)} tracks will be copied", font=("Helvetica Neue", 10), 
                 fg=TEXT3, bg=BG2).pack(pady=(0, 15))
        
        lb_frame = tk.Frame(dlg, bg=BG3, bd=1, highlightbackground=BORDER, highlightthickness=1)
        lb_frame.pack(fill="both", expand=True, padx=30, pady=10)

        lb = tk.Listbox(lb_frame, bg=BG3, fg=TEXT2, font=("Helvetica Neue", 12), 
                        relief="flat", borderwidth=0, highlightthickness=0, 
                        selectbackground=BG4, activestyle="none")
        lb.pack(fill="both", expand=True, padx=5, pady=5)
        
        for pl in playlists:
            lb.insert("end", f"  {pl}")

        def do_transfer():
            sel = lb.curselection()
            if not sel: return
            target = playlists[sel[0]]
            dest_dir = self._find_playlist_dir(target)
            
            if not dest_dir: return
            
            count = 0
            for src in paths:
                try:
                    shutil.copy2(src, os.path.join(dest_dir, os.path.basename(src)))
                    count += 1
                except: pass
            
            dlg.destroy()
            self._refresh_library()
            messagebox.showinfo("Success", f"Copied {count} songs to '{target}'")

        # Styled Green Button (Matching the rest of the Vibe UI)
        btn = tk.Label(dlg, text="Confirm Transfer", font=("Helvetica Neue", 11, "bold"),
                       fg=SIDEBAR, bg=GREEN, padx=30, pady=12, cursor="hand2")
        btn.pack(pady=25)
        btn.bind("<Button-1>", lambda e: do_transfer())
        btn.bind("<Enter>", lambda e: btn.config(bg=GREEN_HOVER))
        btn.bind("<Leave>", lambda e: btn.config(bg=GREEN))

    # Now, find the existing _ctx_add_playlist and replace it with this one-liner:
    def _ctx_add_playlist(self):
        """Proxy to show the add to playlist dialog for selected paths."""
        paths = self._get_sel_paths()
        if paths:
            self._show_add_to_playlist_dialog(paths, f"Add {len(paths)} songs to...")

    def _create_playlist_dialog(self, set_dest=False):
        """Creates a new playlist folder and refreshes the library."""
        name = simpledialog.askstring("New Playlist", "Enter playlist name:", parent=self)
        if not name or not name.strip():
            return
        
        safe_name = re.sub(r'[<>:"/\\|?*]', '', name).strip()
        if not safe_name:
            return
            
        pl_dir = os.path.join(MUSIC_ROOT, safe_name)
        try:
            os.makedirs(pl_dir, exist_ok=True)
            self._refresh_library()
            if set_dest and hasattr(self, 'dl_dest_var'):
                self.dl_dest_var.set(pl_dir)
        except Exception as e:
            messagebox.showerror("Error", f"Could not create folder: {e}")
    def __init__(self):
        super().__init__()
        
        # Export global constant to instance for views to access
        self.MUSIC_ROOT = MUSIC_ROOT
        
        # 1. This happens the second you open the app
        self.title("BoomTunes") 
        
        try:
            self.create_command('tkAboutDialog', self._show_about)
        except:
            pass 

        self.geometry("1180x800")
        self.minsize(960, 660)
        self.configure(bg=BG)
        self.resizable(True, True)

        self._watch_roots          = [MUSIC_ROOT]
        self._library              = {}
        self._last_lib_hash        = 0
        self._dl_playlists         = []
        self._is_dl                = False
        self._dl_done              = 0
        self._search_results       = []
        self._search_offset        = 0
        self._queue_playlist_name  = None
        self._queue_playlist_paths = []
        self._error_logs           = []
        self._selected_playlist    = None
        self._current_tab          = "library"
        self._lib_searching        = False
        self._recently_played      = []
        self._crossfade_secs       = 0
        self._sleep_after_id       = None
        self._viz_enabled          = False
        self._viz_after_id         = None
        self._sync_running         = False
        self._playlist_urls        = {}
        self._media_command_queue  = queue.SimpleQueue()
        self._closing = False
        self._poll_after_id = None

        self.vol_var        = tk.DoubleVar(value=70)
        self.scrub_var      = tk.DoubleVar(value=0)
        self.status_var     = tk.StringVar(value="")
        self.prog_var       = tk.DoubleVar(value=0)
        self.search_var     = tk.StringVar()
        self.lib_search_var = tk.StringVar()
        self.yt_url_var     = tk.StringVar()
        self.dl_dest_var    = tk.StringVar(value=MUSIC_ROOT)
        self.sc_var         = tk.BooleanVar(value=True)
        self.skip_var       = tk.BooleanVar(value=True)
        self.meta_var       = tk.BooleanVar(value=True)

        self._cf_var        = tk.StringVar(value="Off")
        self._sleep_var     = tk.StringVar(value="Off")
        
        # Last.fm Discovery State
        self.lastfm_key_var     = tk.StringVar(value="")
        self.auto_discovery_var = tk.BooleanVar(value=False)
        self.global_keys_var    = tk.BooleanVar(value=True)
        self.discovery          = DiscoveryService(logger=self._log_error)
        
        self.art_cache          = ArtCache(self)
        self._np_poll_counter = 0

        # 2. LOAD PERSISTENT CONFIG
        self._load_config()
        self._load_playlist_meta()
        
        # 3. CONFIGURE UI & ENGINE
        self._configure_styles()
        self._build_ui()
        self.engine = VibeEngine(self)
        
        # Restore volume from config (if loaded)
        if hasattr(self, "engine"):
            self.engine.volume = self.vol_var.get() / 100

        self._refresh_library()
        self._init_watchdog()
        
        # Auto-save traces
        self.lastfm_key_var.trace_add("write", lambda *a: self._save_config())
        self.auto_discovery_var.trace_add("write", lambda *a: self._save_config())
        self._cf_var.trace_add("write", lambda *a: self._save_config())
        self._sleep_var.trace_add("write", lambda *a: self._save_config())
        self.global_keys_var.trace_add("write", lambda *a: (self._save_config(), self._on_global_keys_changed()))
        
        # Close Protocol
        self.protocol("WM_DELETE_WINDOW", self._on_closing)
        
        # Initial sidebar sash placement (40/60 split)
        self.after(500, self._set_initial_sidebar_sash)
        
        # 4. Media key handling: MPRemoteCommandCenter in-process registration.
        #    No subprocess, no 'defaults write' — command callbacks enqueue onto
        #    _media_command_queue and return promptly; _poll_media_commands drains it.
        self._init_remote_commands()
        self._poll_after_id = None
        if self.global_keys_var.get():
            self.after(1500, self._start_media_key_listener)
        self.after(100, self._poll_media_commands)

        if not PYGAME_AVAILABLE: self._log_error("warning", "pygame not available")
        if not YT_DLP_AVAILABLE: self._log_error("warning", "yt-dlp not found - brew install yt-dlp")

    def _show_about(self):
        # 2. This ONLY happens if you click "About Dev Tunes" in the menu
        messagebox.showinfo("BoomTunes", "BoomTunes v4\nProfessional Music Player")

    def _fetch_missing_artwork(self, on_progress, on_complete):
        """Batch process to fetch missing album art for entire library"""
        def run():
            import requests
            try:
                from mutagen.mp3 import MP3
                from mutagen.id3 import ID3, APIC, error
            except ImportError:
                self._log_error("artwork", "Mutagen missing - cannot embed artwork")
                self.after(0, on_complete)
                return

            def check_has_art(file_path):
                try:
                    audio = MP3(file_path, ID3=ID3)
                    if audio.tags:
                        for tag in audio.tags.keys():
                            if tag.startswith('APIC'):
                                return True
                except: pass
                return False

            def embed_art(file_path, img_data):
                try:
                    audio = MP3(file_path, ID3=ID3)
                    if audio.tags is None:
                        audio.add_tags()
                    audio.tags.add(
                        APIC(
                            encoding=3, # utf-8
                            mime='image/jpeg',
                            type=3, # 3 is for the cover image
                            desc=u'Cover',
                            data=img_data
                        )
                    )
                    audio.save(v2_version=3)
                    return True
                except Exception:
                    return False

            total = 0
            processed = 0
            failed = 0
            
            # Flatten all paths
            all_paths = []
            for paths in self._library.values():
                for p in paths:
                    all_paths.append(p)
                    
            # Deduplicate while preserving order mostly
            all_paths = list(dict.fromkeys(all_paths))
            total_paths = len(all_paths)
                    
            for i, path in enumerate(all_paths):
                if not os.path.exists(path) or not path.lower().endswith('.mp3'):
                    continue
                    
                title, artist = split_track_name(path)
                
                def _update_ui(idx=i, tot=total_paths, t=title):
                    on_progress(idx + 1, tot, t)
                self.after(0, _update_ui)
                
                if check_has_art(path):
                    continue
                    
                query = f"{artist} {title}".strip()
                if not query:
                    continue
                    
                total += 1
                try:
                    # Fake a standard browser user-agent to avoid immediate bot bans
                    headers = {
                        "User-Agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:109.0) Gecko/20100101 Firefox/111.0"
                    }
                    # Use Deezer API (More generous rate limits than Apple, up to 50 requests / 5 seconds)
                    url = f"https://api.deezer.com/search?q={requests.utils.quote(query)}"
                    resp = requests.get(url, headers=headers, timeout=5)
                    
                    if resp.status_code == 200:
                        data = resp.json()
                        if data.get('data') and len(data['data']) > 0:
                            # Try to get the highest resolution cover available
                            album_data = data['data'][0].get('album', {})
                            art_url = album_data.get('cover_xl') or album_data.get('cover_xL') or album_data.get('cover_big') or album_data.get('cover_medium') or album_data.get('cover')
                            
                            if art_url:
                                img_resp = requests.get(art_url, headers=headers, timeout=5)
                                if img_resp.status_code == 200:
                                    img_data = img_resp.content
                                    
                                    if embed_art(path, img_data):
                                        processed += 1
                                        self._log_error("artwork", f"Embedded art for: {title}")
                                    else:
                                        failed += 1
                                        self._log_error("artwork", f"Failed to save {title} to file.")
                                else:
                                    failed += 1
                                    self._log_error("artwork", f"Art download returned {img_resp.status_code}")
                        else:
                            # ── FALLLBACK: yt-dlp ──
                            self._log_error("artwork", f"No results on Deezer for: {query}... trying YouTube fallback")
                            import yt_dlp
                            from PIL import Image
                            import io
                            ydl_opts = {'quiet': True, 'no_warnings': True, 'skip_download': True, 'extract_flat': False}
                            try:
                                with yt_dlp.YoutubeDL(ydl_opts) as ydl:
                                    info = ydl.extract_info(f"ytsearch1:{query}", download=False)
                                    if 'entries' in info and len(info['entries']) > 0:
                                        thumbnails = info['entries'][0].get('thumbnails', [])
                                        success = False
                                        for tb in reversed(thumbnails):
                                            yt_url = tb.get('url')
                                            if not yt_url:
                                                continue
                                            
                                            yt_resp = requests.get(yt_url, headers=headers, timeout=5)
                                            if yt_resp.status_code == 200:
                                                # Convert YouTube's (likely WEBP) image buffer natively into standard JPEG using Pillow
                                                try:
                                                    img = Image.open(io.BytesIO(yt_resp.content)).convert("RGB")
                                                    out_buffer = io.BytesIO()
                                                    img.save(out_buffer, format="JPEG", quality=90)
                                                    std_jpeg_data = out_buffer.getvalue()
                                                    
                                                    if embed_art(path, std_jpeg_data):
                                                        processed += 1
                                                        self._log_error("artwork", f"Embedded YouTube art for: {title}")
                                                        success = True
                                                        break # Break the thumbnail loop on success
                                                except Exception as conv_err:
                                                    self._log_error("artwork", f"Image conversion failed: {conv_err}")
                                        if success:
                                            continue
                            except Exception as yte:
                                self._log_error("artwork", f"YT fallback failed: {yte}")
                            
                            failed += 1
                    elif resp.status_code == 403 or resp.status_code == 429:
                        self._log_error("artwork", f"{resp.status_code}! Deezer API rate limit hit. Pausing for 5 seconds...")
                        import time
                        time.sleep(5)
                        failed += 1
                    else:
                        failed += 1
                        self._log_error("artwork", f"Search API returned {resp.status_code}")
                        
                    # Polite sleep to avoid hammering the API
                    import time
                    time.sleep(0.3)
                    
                except Exception as e:
                    failed += 1
                    self._log_error("artwork", f"Search failed for {title}: {str(e)}")
                    
            self._log_error("artwork", f"Sync Completed. Added: {processed}, Failed/Not Found: {failed}")
            self.after(0, on_complete)

        threading.Thread(target=run, daemon=True).start()

    def _configure_styles(self):
        s = ttk.Style(self)
        s.theme_use("clam")
        s.configure("Dark.TNotebook", background=BG2, borderwidth=0, tabmargins=0)
        s.configure("Dark.TNotebook.Tab", background=BG2, foreground=TEXT3,
                    padding=[18,8], font=("Helvetica Neue",10,"bold"), borderwidth=0)
        s.map("Dark.TNotebook.Tab",
              background=[("selected",BG2)], foreground=[("selected",TEXT)])
        s.configure("Green.Horizontal.TProgressbar",
                    troughcolor=BG3, background=GREEN,
                    bordercolor=BG3, lightcolor=GREEN, darkcolor=GREEN, thickness=3)
        for style, bg, rh in [("Lib.Treeview",BG,48),
                               ("Queue.Treeview",BG2,38),
                               ("Results.Treeview",BG,44)]:
            s.configure(style, background=bg, foreground=TEXT2,
                        fieldbackground=bg, borderwidth=0,
                        bordercolor=bg, lightcolor=bg, darkcolor=bg,
                        font=("Helvetica Neue",11), rowheight=rh)
            s.map(style, background=[("selected",BG3)], foreground=[("selected",TEXT)])
            s.configure(f"{style}.Heading", background=bg, foreground=TEXT3,
                        borderwidth=0, bordercolor=bg, lightcolor=bg, darkcolor=bg,
                        font=("Helvetica Neue",9,"bold"), relief="flat")

    # ═══════════════════════════════════════════════════════════════════════════
    def _build_ui(self):
        self.grid_rowconfigure(0, weight=1)
        self.grid_rowconfigure(1, weight=0)
        self.grid_columnconfigure(0, weight=1)

        # Root PanedWindow for Sidebar | Main Content | Queue
        self._root_paned = tk.PanedWindow(self, orient="horizontal", bg=BG,
                                           sashwidth=3, sashpad=0, bd=0, sashrelief="flat")
        self._root_paned.grid(row=0, column=0, sticky="nsew")

        sidebar = tk.Frame(self._root_paned, bg=SIDEBAR, width=240)
        self._root_paned.add(sidebar, width=240, minsize=240, stretch="never")
        sidebar.grid_propagate(False)
        self._build_sidebar(sidebar)

        main = tk.Frame(self._root_paned, bg=BG)
        self._root_paned.add(main, minsize=400, stretch="always")
        self._build_main(main)

        # 3rd Column: Global Queue
        queue_col = tk.Frame(self._root_paned, bg=BG2, width=270)
        self._root_paned.add(queue_col, width=270, minsize=40, stretch="never")
        queue_col.grid_propagate(False)
        self._build_queue_panel(queue_col)

        tk.Frame(self, bg="#000", height=1).grid(row=1, column=0, columnspan=2, sticky="ew")
        player = tk.Frame(self, bg=PLAYER_BG, height=94)
        player.grid(row=1, column=0, columnspan=2, sticky="ew")
        player.grid_propagate(False)
        self._build_player(player)
        self._bind_keys()

    # ── Sidebar ───────────────────────────────────────────────────────────────
    def _build_sidebar(self, parent):
        # 1. Logo area (always top)
        logo_f = tk.Frame(parent, bg=SIDEBAR)
        logo_f.pack(fill="x", padx=24, pady=(28,24))
        tk.Label(logo_f, text="BoomTunes", font=("Helvetica Neue",24,"bold"),
                 fg=TEXT, bg=SIDEBAR).pack(side="left")
        tk.Label(logo_f, text="▮", font=("Helvetica Neue",9),
                 fg=GREEN, bg=SIDEBAR).pack(side="left", padx=(3,0), pady=(10,0))

        # 2. Vertical PanedWindow for Nav | Playlists
        self._side_paned = tk.PanedWindow(parent, orient="vertical", bg=SIDEBAR,
                                           sashwidth=3, sashpad=0, bd=0, sashrelief="flat")
        self._side_paned.pack(fill="both", expand=True)

        self._nav_container = tk.Frame(self._side_paned, bg=SIDEBAR)
        self._side_paned.add(self._nav_container, minsize=40, stretch="never")
        
        # Single nav button (collapsed state)
        self._nav_menu_btn = tk.Label(self._nav_container, text="☰  Menu", font=("Helvetica Neue",13,"bold"),
                                       fg=TEXT2, bg=SIDEBAR, padx=16, pady=12, cursor="hand2", anchor="w")
        self._nav_menu_btn.bind("<Button-1>", lambda e: self._show_nav_menu(e))
        self._nav_menu_btn.bind("<Enter>", lambda e: self._nav_menu_btn.config(bg=BG3, fg=TEXT))
        self._nav_menu_btn.bind("<Leave>", lambda e: self._nav_menu_btn.config(bg=SIDEBAR, fg=TEXT2))

        # Regular nav items container
        self._nav_items_inner = tk.Frame(self._nav_container, bg=SIDEBAR)
        self._nav_items_inner.pack(fill="both", expand=True)
        
        for icon, label, cmd in [
            ("⌂","Home",        lambda: self._show_tab("library")),
            ("🕐","Recent",      lambda: self._show_tab("recent")),
            ("👤","Artists",     lambda: self._show_tab("artists")),
            ("✨","Discover",    lambda: self._show_tab("discover")),
            ("⬇","Downloader", lambda: self._show_tab("download")),
            ("⚙","Settings",    lambda: self._show_tab("settings")),
        ]:
            f = tk.Frame(self._nav_items_inner, bg=SIDEBAR, cursor="hand2")
            f.pack(fill="x")
            row = tk.Frame(f, bg=SIDEBAR, cursor="hand2")
            row.pack(fill="x", padx=16, pady=1)
            tk.Label(row, text=icon, font=("Helvetica Neue",14), fg=TEXT3,
                     bg=SIDEBAR, width=2, anchor="center").pack(side="left", padx=(0,10))
            lbl = tk.Label(row, text=label, font=("Helvetica Neue",13),
                           fg=TEXT2, bg=SIDEBAR, anchor="w", cursor="hand2", pady=9)
            lbl.pack(side="left", fill="x", expand=True)
            def _b(c, fr=f, rw=row, lb=lbl):
                lb.bind("<Button-1>", lambda e: c())
                rw.bind("<Button-1>", lambda e: c())
                for w in (lb,rw,fr):
                    w.bind("<Enter>", lambda e,_f=fr,_r=rw,_l=lb: 
                           [_f.config(bg=BG3),_r.config(bg=BG3),_l.config(bg=BG3,fg=TEXT)])
                    w.bind("<Leave>", lambda e,_f=fr,_r=rw,_l=lb: 
                           [_f.config(bg=SIDEBAR),_r.config(bg=SIDEBAR),_l.config(bg=SIDEBAR,fg=TEXT2)])
            _b(cmd)

        self._playlist_container = tk.Frame(self._side_paned, bg=SIDEBAR)
        self._side_paned.add(self._playlist_container, minsize=100, stretch="always")
        
        # Watch nav height
        self._nav_container.bind("<Configure>", self._on_nav_configure)

        # 3. Visual Sash Indicator (Draggable Bar)
        sash_handle = tk.Frame(self._playlist_container, bg=BG4, height=2)
        sash_handle.pack(fill="x", pady=(0, 16))
        
        lib_hdr = tk.Frame(self._playlist_container, bg=SIDEBAR)
        lib_hdr.pack(fill="x", padx=16, pady=(0,8))
        tk.Label(lib_hdr, text="Your Library",
                 font=("Helvetica Neue",12,"bold"), fg=TEXT2, bg=SIDEBAR).pack(side="left", padx=8)
        for txt, cmd in [("+", self._create_playlist_dialog), ("↻", self._refresh_library)]:
            l = tk.Label(lib_hdr, text=txt, font=("Helvetica Neue",15 if txt=="+" else 14),
                         fg=TEXT3, bg=SIDEBAR, cursor="hand2")
            l.pack(side="right", padx=2)
            l.bind("<Button-1>", lambda e, c=cmd: c())
            l.bind("<Enter>",    lambda e, _l=l: _l.config(fg=TEXT))
            l.bind("<Leave>",    lambda e, _l=l: _l.config(fg=TEXT3))

        # Library search
        sb_f = tk.Frame(self._playlist_container, bg=BG3, highlightbackground=BG4, highlightthickness=1)
        sb_f.pack(fill="x", padx=12, pady=(0,8))
        tk.Label(sb_f, text="⌕", font=("Helvetica Neue",11),
                 fg=TEXT3, bg=BG3, padx=8).pack(side="left")
        self._lib_ph = "Search playlists"
        self.lib_search_entry = tk.Entry(sb_f, bg=BG3, fg=TEXT3, insertbackground=TEXT,
                                         relief="flat", font=("Helvetica Neue",10), bd=0,
                                         highlightthickness=0, highlightbackground=BG3, highlightcolor=BG3)
        self.lib_search_entry.pack(side="left", fill="both", expand=True)
        self.lib_search_entry.insert(0, self._lib_ph)
        self.lib_search_entry.bind("<FocusIn>",  self._lib_focus_in)
        self.lib_search_entry.bind("<FocusOut>", self._lib_focus_out)
        self.lib_search_entry.bind("<KeyRelease>", self._lib_key_released)

        # Scrollable canvas playlist panel
        pl_outer = tk.Frame(self._playlist_container, bg=SIDEBAR)
        pl_outer.pack(fill="both", expand=True)

        self._pl_scroll_canvas = tk.Canvas(pl_outer, bg=SIDEBAR,
                                            highlightthickness=0, bd=0)
        from widgets import ModernScrollbar
        pl_sb = ModernScrollbar(pl_outer, command=self._pl_scroll_canvas.yview, bg=SIDEBAR, width=4)
        self._pl_scroll_canvas.config(yscrollcommand=pl_sb.set)
        pl_sb.pack(side="right", fill="y")
        self._pl_scroll_canvas.pack(fill="both", expand=True)

        self._pl_inner = tk.Frame(self._pl_scroll_canvas, bg=SIDEBAR)
        self._pl_canvas_window = self._pl_scroll_canvas.create_window(
            0, 0, window=self._pl_inner, anchor="nw")

        def _update_sidebar_scroll(event=None):
            self._pl_inner.update_idletasks()
            content_h = self._pl_inner.winfo_reqheight()
            canvas_h = self._pl_scroll_canvas.winfo_height()
            w = self._pl_scroll_canvas.winfo_width()
            if content_h <= canvas_h:
                # Content fits — lock scrollregion to canvas so it can't scroll
                self._pl_scroll_canvas.configure(scrollregion=(0, 0, w, canvas_h))
                self._pl_scroll_canvas.yview_moveto(0)
            else:
                self._pl_scroll_canvas.configure(scrollregion=(0, 0, w, content_h))
            self._pl_scroll_canvas.itemconfig(self._pl_canvas_window, width=w)

        self._pl_inner.bind("<Configure>", _update_sidebar_scroll)
        self._pl_scroll_canvas.bind("<Configure>", _update_sidebar_scroll)
        self._pl_scroll_canvas.bind("<MouseWheel>", self._on_sidebar_wheel)

    # ── Main ──────────────────────────────────────────────────────────────────
    def _build_main(self, parent):
        parent.grid_rowconfigure(0, weight=1)
        parent.grid_columnconfigure(0, weight=1)
        self._frames = {}
        
        self._frames["library"]  = LibraryView(parent, self)
        self._frames["download"] = DownloadView(parent, self)
        self._frames["recent"]   = RecentView(parent, self)
        self._frames["artists"]  = ArtistsView(parent, self)
        self._frames["settings"] = SettingsView(parent, self)
        self._frames["discover"] = DiscoverView(parent, self)
        from views.error_log import ErrorLogView
        self._frames["error_log"] = ErrorLogView(parent, self)
        self._frames["song_page"] = SongPageView(parent, self)
        
        for name in self._frames:
            self._frames[name].grid(row=0, column=0, sticky="nsew")

        self._show_tab("library")

    def _show_tab(self, name):
        self._current_tab = name
        for f in self._frames.values(): f.grid_remove()
        self._frames[name].grid()
        
        if name == "library":
            self._frames["library"].populate_track_list()
        elif name == "artists":
            self._frames["artists"].populate()
        elif name == "recent":
            self._frames["recent"].redraw()
        elif name == "error_log":
            self._frames["error_log"].populate()
        elif name == "song_page":
            self._frames["song_page"].populate()

    # ── Queue Panel ─────────────────────────────────────────────────────────

    # ── Queue Panel ─────────────────────────────────────────────────────────

    def _build_queue_panel(self, parent):
        self._queue_tab = "queue"
        
        inner = tk.Frame(parent, bg=BG2)
        inner.pack(fill="both", expand=True)

        # 1. Tab Headers
        tabs = tk.Frame(inner, bg=BG2)
        tabs.pack(fill="x", padx=16, pady=(16,8))
        
        self._q_tab_btn = tk.Label(tabs, text="Queue", font=("Helvetica Neue",11,"bold"),
                                   fg=TEXT, bg=BG2, cursor="hand2", padx=4)
        self._q_tab_btn.pack(side="left")
        self._q_tab_btn.bind("<Button-1>", lambda e: self._set_queue_tab("queue"))
        
        self._r_tab_btn = tk.Label(tabs, text="Recently Played", font=("Helvetica Neue",11),
                                   fg=TEXT3, bg=BG2, cursor="hand2", padx=16)
        self._r_tab_btn.pack(side="left")
        self._r_tab_btn.bind("<Button-1>", lambda e: self._set_queue_tab("recent"))

        # 2. Scrollable Area
        q_outer = tk.Frame(inner, bg=BG2)
        q_outer.pack(fill="both", expand=True)
        
        self._q_canvas = tk.Canvas(q_outer, bg=BG2, highlightthickness=0, bd=0)
        from widgets import ModernScrollbar
        self._q_sb = ModernScrollbar(q_outer, command=self._q_canvas.yview, bg=BG2, width=4)
        self._q_canvas.config(yscrollcommand=self._q_sb.set)
        
        self._q_sb.pack(side="right", fill="y")
        self._q_canvas.pack(side="left", fill="both", expand=True)
        
        self._q_inner = tk.Frame(self._q_canvas, bg=BG2)
        self._q_window = self._q_canvas.create_window(0, 0, window=self._q_inner, anchor="nw")
        
        self._q_canvas.bind("<Configure>", lambda e: self._q_canvas.itemconfig(self._q_window, width=e.width))
        self._q_canvas.bind("<MouseWheel>", self._on_queue_wheel)

    def _on_queue_wheel(self, event):
        """Clamped scrolling for the queue canvas. Prevents negative scrolling."""
        lo, hi = self._q_canvas.yview()
        # If content fits entirely, don't scroll
        if lo <= 0.0 and hi >= 1.0: return
        # Prevent scrolling past top (delta > 0 is up)
        if lo <= 0.001 and event.delta > 0: return
        # Prevent scrolling past bottom (delta < 0 is down)
        if hi >= 0.999 and event.delta < 0: return
        
        self._q_canvas.yview_scroll(-2 if event.delta > 0 else 2, "units")

    def _set_queue_tab(self, tab):
        self._queue_tab = tab
        if tab == "queue":
            self._q_tab_btn.config(fg=TEXT, font=("Helvetica Neue",11,"bold"))
            self._r_tab_btn.config(fg=TEXT3, font=("Helvetica Neue",11))
        else:
            self._r_tab_btn.config(fg=TEXT, font=("Helvetica Neue",11,"bold"))
            self._q_tab_btn.config(fg=TEXT3, font=("Helvetica Neue",11))
        self._update_queue()

    def _update_queue(self):
        """Redraws the queue/recent list."""
        for w in self._q_inner.winfo_children(): w.destroy()
        
        if self._queue_tab == "queue":
            self._draw_queue_items()
        else:
            self._draw_recent_items()
            
        self._q_inner.update_idletasks()
        h = self._q_inner.winfo_reqheight()
        w = self._q_canvas.winfo_width()
        self._q_canvas.config(scrollregion=(0, 0, w, h))
        # Reset scroll to top to ensure "Now Playing" is visible
        self._q_canvas.yview_moveto(0)
        
        # Ensure scrolling works even on child widgets recursively
        self._bind_mousewheel_recursive(self._q_inner)

    def _bind_mousewheel_recursive(self, widget):
        def _on_wheel(e):
             lo, hi = self._q_canvas.yview()
             if lo <= 0.0 and hi >= 1.0: return
             if lo <= 0.001 and e.delta > 0: return
             if hi >= 0.999 and e.delta < 0: return
             self._q_canvas.yview_scroll(-1 if e.delta > 0 else 1, "units")
        
        widget.bind("<MouseWheel>", _on_wheel)
        for child in widget.winfo_children():
            self._bind_mousewheel_recursive(child)

    def _draw_queue_items(self):
        q = self.engine.queue
        idx = self.engine.queue_idx
        
        # Section: Now Playing
        if 0 <= idx < len(q):
            self._add_q_section_hdr("Now Playing")
            self._add_q_item(q[idx], is_playing=True)
            
            # Section: Next In Queue (Unified)
            if idx + 1 < len(q):
                self._add_q_section_hdr("Next In Queue")
                # Show up to 50 next tracks
                for i in range(idx + 1, min(idx + 51, len(q))):
                    self._add_q_item(q[i], q_idx=i)
        elif not q:
            tk.Label(self._q_inner, text="Queue is empty", font=("Helvetica Neue",10),
                     fg=TEXT3, bg=BG2, pady=40).pack()

    def _draw_recent_items(self):
        self._add_q_section_hdr("Recently Played")
        # Most recent first, up to 100
        for path in self._recently_played[:100]:
            self._add_q_item(path)

    def _add_q_section_hdr(self, text):
        tk.Label(self._q_inner, text=text.upper(), font=("Helvetica Neue",9,"bold"),
                 fg=TEXT3, bg=BG2, anchor="w", pady=(18 if text!="Now Playing" else 8)).pack(fill="x", padx=16)

    def _add_q_item(self, path, is_playing=False, q_idx=None):
        title, artist = split_track_name(path)
        f = tk.Frame(self._q_inner, bg=BG2, cursor="hand2")
        f.pack(fill="x", pady=2)
        
        # Click handlers
        def play_from_q(pt=path, qi=q_idx):
            if qi is not None:
                diff = qi - self.engine.queue_idx
                if diff > 0 and diff <= getattr(self.engine, 'user_queue_count', 0):
                    self.engine.user_queue_count -= diff
                else:
                    self.engine.user_queue_count = 0
                self.engine.queue_idx = qi
                self.engine.play_current()
            else:
                # Recently played or context-less item: play with full context
                self._play_with_context(pt)

        def show_q_context(e, idx=q_idx):
            m = tk.Menu(self, tearoff=0, bg=BG3, fg=TEXT, font=("Helvetica Neue", 11))
            m.add_command(label="▶ Play Now", command=lambda: play_from_q())
            if idx is not None:
                m.add_command(label="✕ Remove from Queue", command=lambda: self._ctx_remove_from_queue_idx(idx))
            m.post(e.x_root, e.y_root)

        f.bind("<Button-1>", lambda e: play_from_q())
        f.bind("<Button-2>", show_q_context)
        f.bind("<Button-3>", show_q_context)
        f.bind("<Enter>", lambda e: f.config(bg=BG3))
        f.bind("<Leave>", lambda e: f.config(bg=BG2))

        def _on_q_art_loaded(p):
            if not getattr(self, "_q_art_pending", False):
                self._q_art_pending = True
                self.after(50, lambda: [setattr(self, "_q_art_pending", False), self._update_queue()])
                
        img = self.art_cache.get(path, 36, _on_q_art_loaded)
        if img:
            art = tk.Label(f, image=img, bg=BG3, width=36, height=36)
        else:
            art = tk.Label(f, text="♫", font=("Helvetica Neue",12),
                           fg=(GREEN if is_playing else TEXT3), bg=BG3, width=4, height=2)
        art.pack(side="left", padx=(16,12), pady=6)
        art.bind("<Button-1>", lambda e: play_from_q())

        txt = tk.Frame(f, bg=BG2)
        txt.pack(side="left", fill="both", expand=True)
        txt.bind("<Button-1>", lambda e: play_from_q())

        def q_truncate(s, limit=28):
            if len(s) > limit: return s[:limit-3] + "..."
            return s

        t_lbl = tk.Label(txt, text=q_truncate(title, 26), font=("Helvetica Neue",10, "bold" if is_playing else "normal"),
                         fg=(GREEN if is_playing else TEXT), bg=BG2, anchor="w")
        t_lbl.pack(fill="x")
        t_lbl.bind("<Button-1>", lambda e: play_from_q())
        
        a_lbl = tk.Label(txt, text=q_truncate(artist, 32), font=("Helvetica Neue",9),
                         fg=TEXT3, bg=BG2, anchor="w")
        a_lbl.pack(fill="x")
        a_lbl.bind("<Button-1>", lambda e: play_from_q())

        # Consolidated hover consistency (Now that all widgets exist)
        def _set_row_style(bg, fg1, fg2):
            f.config(bg=bg); txt.config(bg=bg)
            if t_lbl.winfo_exists(): t_lbl.config(bg=bg, fg=GREEN if is_playing else fg1)
            if a_lbl.winfo_exists(): a_lbl.config(bg=bg, fg=fg2)

        for w in [f, txt, t_lbl, a_lbl, art]:
            w.bind("<Enter>", lambda e: _set_row_style(BG3, TEXT, TEXT3))
            w.bind("<Leave>", lambda e: _set_row_style(BG2, TEXT if is_playing else TEXT, TEXT3))

    # ── Player bar ────────────────────────────────────────────────────────────
    def _build_player(self, parent):
        parent.grid_columnconfigure(1, weight=1)

        # Accent line at top of player bar
        self._player_accent = tk.Canvas(parent, height=2, highlightthickness=0, bd=0,
                                         bg=PLAYER_BG)
        self._player_accent.grid(row=0, column=0, columnspan=3, sticky="ew")
        self._player_accent._accent_id = None

        # Track info + mini art
        info = tk.Frame(parent, bg=PLAYER_BG, width=300)
        info.grid(row=1, column=0, sticky="nsew", padx=(20,0))
        info.grid_propagate(False)
        info.grid_rowconfigure(0, weight=1)
        info.grid_columnconfigure(1, weight=1)

        self.player_art_lbl = tk.Label(info, text="♫",
                                        font=("Helvetica Neue",20),
                                        fg=TEXT3, bg=BG3,
                                        width=4, anchor="center")
        self.player_art_lbl.grid(row=0, column=0, padx=(0,12), pady=10, sticky="ns")
        txt_col = tk.Frame(info, bg=PLAYER_BG)
        txt_col.grid(row=0, column=1, sticky="nsew", pady=(8, 0))
        self.now_title = tk.Label(txt_col, text="Nothing playing",
                                   font=("Helvetica Neue",13,"bold"),
                                   fg=TEXT, bg=PLAYER_BG, anchor="w", wraplength=220)
        self.now_title.pack(anchor="w", pady=0)
        self.now_artist = tk.Label(txt_col, text="",
                                    font=("Helvetica Neue",10),
                                    fg=TEXT2, bg=PLAYER_BG, anchor="w", wraplength=220)
        self.now_artist.pack(anchor="w")

        # Song Page Trigger
        for w in [self.player_art_lbl, self.now_title, self.now_artist, txt_col]:
            w.bind("<Button-1>", lambda e: self._show_tab("song_page"), add="+")
            w.config(cursor="hand2")

        # Controls
        ctrl = tk.Frame(parent, bg=PLAYER_BG)
        ctrl.grid(row=1, column=1, sticky="nsew", padx=10)

        btn_row = tk.Frame(ctrl, bg=PLAYER_BG)
        btn_row.pack(pady=(8,4))

        def icon_lbl(text, cmd, size=13, fg=TEXT3):
            l = tk.Label(btn_row, text=text, font=("Helvetica Neue",size),
                         fg=fg, bg=PLAYER_BG, cursor="hand2", padx=8)
            l.bind("<Button-1>", lambda e: cmd())
            l.bind("<Enter>",    lambda e: l.config(fg=TEXT))
            l.bind("<Leave>", lambda e: l.config(fg=GREEN if (l == getattr(self, "shuf_lbl", None) and self.engine.shuffle) or (l == getattr(self, "rep_lbl", None) and self.engine.repeat) else fg))
            return l

        self.shuf_lbl = icon_lbl("⇄", lambda: self._toggle_shuffle_ui())
        self.shuf_lbl.pack(side="left")
        icon_lbl("⏮", lambda: self.engine.prev_track(), 17, TEXT2).pack(side="left")

        # Round play button
        self.play_btn = RoundButton(btn_row, 36, lambda: self.engine.toggle_play(), bg=PLAYER_BG)
        self.play_btn.pack(side="left", padx=8)

        icon_lbl("⏭", lambda: self.engine.next_track(), 17, TEXT2).pack(side="left")
        self.rep_lbl = icon_lbl("↺", lambda: self._toggle_repeat_ui())
        self.rep_lbl.pack(side="left")

        # Slim scrubber
        sr = tk.Frame(ctrl, bg=PLAYER_BG)
        sr.pack(fill="x", padx=16, pady=(0,4))
        self.time_cur = tk.Label(sr, text="0:00", font=("Menlo",9),
                                  fg=TEXT3, bg=PLAYER_BG, width=4)
        self.time_cur.pack(side="left")
        self.scrub = SlimSlider(sr, from_=0, to=100,
                                command=lambda v: self.engine.on_scrub_moved(v),
                                track_color=BG4, fill_color=TEXT3,
                                thumb_color=TEXT, height=20, bg=PLAYER_BG)
        self.scrub.pack(side="left", fill="x", expand=True, padx=8)
        self.time_tot = tk.Label(sr, text="0:00", font=("Menlo",9),
                                  fg=TEXT3, bg=PLAYER_BG, width=4)
        self.time_tot.pack(side="left")

        # Volume
        vol_frame = tk.Frame(parent, bg=PLAYER_BG, width=220)
        vol_frame.grid(row=1, column=2, sticky="e", padx=(0,20))
        vol_frame.grid_propagate(False)
        vi = tk.Frame(vol_frame, bg=PLAYER_BG)
        vi.pack(side="right", pady=26)
        tk.Label(vi, text="🔊", font=("Helvetica Neue",11),
                 fg=TEXT3, bg=PLAYER_BG).pack(side="left", padx=(0,6))
        self.vol_slider = SlimSlider(vi, from_=0, to=100,
                                     command=lambda v: self.engine.on_volume_moved(v),
                                     track_color=BG4, fill_color=TEXT3,
                                     thumb_color=TEXT, height=20, bg=PLAYER_BG)
        self.vol_slider.set(70)
        self.vol_slider.pack(side="left")
        self.vol_slider.configure(width=120)

    def update_player_art(self, path):
        if not path:
            self.player_art_lbl.config(image="", text="♫", width=4, height=2, bg=BG3)
            self._update_player_accent(path)
            return
            
        def _on_p_art_loaded(p):
            if self.engine.queue and self.engine.queue_idx >= 0 and self.engine.queue_idx < len(self.engine.queue):
                if p == self.engine.queue[self.engine.queue_idx]:
                    img = self.art_cache.get(p, 60, lambda x: None)
                    if img:
                        self.player_art_lbl.config(image=img, text="", width=60, height=60, bg=PLAYER_BG)
        
        img = self.art_cache.get(path, 60, _on_p_art_loaded)
        if img:
            self.player_art_lbl.config(image=img, text="", width=60, height=60, bg=PLAYER_BG)
        else:
            self.player_art_lbl.config(image="", text="♫", width=4, height=2, bg=BG3)
        self._update_player_accent(path)

    def _update_player_accent(self, path):
        color = self.art_cache.get_dominant_color(path, fallback=GREEN)
        accent = self._player_accent
        accent.delete("all")
        w = accent.winfo_width()
        if w > 2:
            r, g, b = hex_to_rgb(color)
            accent.create_rectangle(0, 0, w, 2, fill=color, outline="")
            accent.create_rectangle(0, 2, w, 2, fill=PLAYER_BG, outline="")
        self.scrub.set_accent(color)

    def _on_sidebar_wheel(self, event):
        # Only scroll if content exceeds the viewport
        self._pl_inner.update_idletasks()
        content_h = self._pl_inner.winfo_reqheight()
        canvas_h = self._pl_scroll_canvas.winfo_height()
        if content_h <= canvas_h:
            return  # Content fits — no scrolling
        if event.delta > 0:
            self._pl_scroll_canvas.yview_scroll(-2, "units")
        else:
            self._pl_scroll_canvas.yview_scroll(2, "units")

    def _broadcast_shuffle_state(self):
        """Updates all shuffle icons/labels across all views."""
        # 1. Update player bar icon
        if self.engine.shuffle:
            self.shuf_lbl.config(text="🔀", fg=GREEN)
        else:
            self.shuf_lbl.config(text="⇄", fg=TEXT3)
            
        # 2. Update library view label if active
        lib = self._frames.get("library")
        if lib and hasattr(lib, "on_shuffle_changed"):
            lib.on_shuffle_changed(self.engine.shuffle)

    def _toggle_shuffle_ui(self):
        # If nothing is playing, start playback from current view
        if not self.engine.playing:
            self._shuffle_all()
            return

        self.engine.toggle_shuffle()
        self._broadcast_shuffle_state()

    def _toggle_repeat_ui(self):
        self.engine.repeat = not self.engine.repeat
        if self.engine.repeat:
            self.rep_lbl.config(text="🔂", fg=GREEN)
        else:
            self.rep_lbl.config(text="↺", fg=TEXT3)
        self._update_queue()

    # ═══════════════════════════════════════════════════════════════════════════
    # LIBRARY
    # ═══════════════════════════════════════════════════════════════════════════
    

    

    def _log_error(self, area, msg):
        """Global error logger."""
        timestamp = datetime.now().strftime("%H:%M:%S")
        entry = f"[{timestamp}] [{area.upper()}] {msg}"
        self._error_logs.append(entry)
        if len(self._error_logs) > 500: self._error_logs.pop(0)
        print(entry) # Also keep in terminal

    def _on_nav_configure(self, event):
        # Adaptive UI: 275px threshold accommodates all 6 nav items comfortably.
        if event.height < 275:
            if self._nav_items_inner.winfo_ismapped():
                self._nav_items_inner.pack_forget()
                self._nav_menu_btn.pack(fill="x")
        else:
            if not self._nav_items_inner.winfo_ismapped():
                self._nav_menu_btn.pack_forget()
                self._nav_items_inner.pack(fill="both", expand=True)

    def _set_initial_sidebar_sash(self):
        """Places the sidebar sash to favor the fully expanded nav menu on startup."""
        try:
            self._side_paned.update_idletasks()
            h = self._side_paned.winfo_height()
            if h <= 1:
                self.after(200, self._set_initial_sidebar_sash)
                return
            # 280px accommodates all 6 nav items comfortably with a small buffer.
            pos = 280
            self._side_paned.sash_place(0, 0, pos)
        except:
            pass

    def _show_nav_menu(self, event):
        menu = tk.Menu(self, tearoff=0, bg=BG3, fg=TEXT, font=("Helvetica Neue", 11))
        for icon, label, cmd in [
            ("⌂","Home",        lambda: self._show_tab("library")),
            ("🕐","Recent",      lambda: self._show_tab("recent")),
            ("👤","Artists",     lambda: self._show_tab("artists")),
            ("✨","Discover",    lambda: self._show_tab("discover")),
            ("⬇","Downloader", lambda: self._show_tab("download")),
            ("⚙","Settings",    lambda: self._show_tab("settings")),
        ]:
            menu.add_command(label=f"  {icon}  {label}", command=cmd)
        menu.post(event.x_root, event.y_root)

    # ── Discovery Methods ─────────────────────────────────────────────────────
    def _on_track_changed_discovery(self, path):
        """Called by the engine when a track starts playing."""
        if self.auto_discovery_var.get():
            self._trigger_discovery(path)

    def _trigger_discovery(self, path=None):
        """Fetches similar tracks based on a path, current track, or selected track."""
        if not path:
            # 1. Try currently playing
            if hasattr(self, 'engine') and self.engine.playing and self.engine.queue:
                path = self.engine.queue[self.engine.queue_idx]
            
            # 2. Try selected in library if nothing playing
            if not path and "library" in self._frames:
                path = self._frames["library"].get_selected_track_path()
            
            if not path:
                self._log_error("Discovery", "No track playing or selected to find similar music for.")
                return
                
        title, artist = split_track_name(path)
        if not title:
            self._log_error("Discovery", f"Could not find track title for: {os.path.basename(path)}")
            return
        
        # Ensure latest key is used
        if hasattr(self, 'discovery'):
            self.discovery.api_key = self.lastfm_key_var.get().strip()
            
            # Smart Fallback: If artist is missing in filename, use the Playlist/Folder name
            # as the artist, unless it is "Unsorted" or "All Music".
            final_artist = artist
            if not final_artist:
                pl_ctx = self._queue_playlist_name
                if pl_ctx and pl_ctx not in ["All Music", "Unsorted"]:
                    final_artist = pl_ctx
                    self._log_error("Discovery", f"Filename missing artist. Falling back to folder: '{final_artist}'")
            
            if not final_artist:
                self._log_error("Discovery", f"No artist found in filename or folder for: {os.path.basename(path)}. Last.fm requires an artist name.")
                return

            def run():
                recs = self.discovery.get_similar(final_artist, title)
                self.after(0, lambda: self._update_discovery_results(recs))
            
            threading.Thread(target=run, daemon=True).start()

    def _update_discovery_results(self, recs):
        if not recs:
            if "discover" in self._frames:
                self._frames["discover"].render_recommendations([])
            return

        # Build a set of normalized "Artist - Title" strings from library for filtering
        lib_keys = set()
        for tracks in self._library.values():
            for path in tracks:
                t, a = split_track_name(path)
                if t and a:
                    key = f"{a.lower().strip()} - {t.lower().strip()}"
                    lib_keys.add(key)
        
        # Filter out anything the user already has
        filtered = []
        for r in recs:
            r_art = r.get("artist", "").lower().strip()
            r_ttl = r.get("title", "").lower().strip()
            r_key = f"{r_art} - {r_ttl}"
            
            if r_key not in lib_keys:
                filtered.append(r)
        
        if "discover" in self._frames:
            self._frames["discover"].render_recommendations(filtered)

    def _download_track(self, artist, title, url=None):
        """Standardized interface to bridge Discovery with the Downloader."""
        dest = self.dl_dest_var.get()
        if not dest or dest == "— choose playlist —":
            dest = MUSIC_ROOT
            
        track = {"artist": artist, "title": title, "url": url}
        
        def on_log(t, success, msg):
            area = "Download"
            m = f"{t}: {'✓' if success else '✕'} {msg if msg else ''}"
            self._log_error(area, m)
            
        from downloader import Downloader
        Downloader.download_sync_tracks(
            [track],
            dest,
            self.skip_var.get(),
            self.sc_var.get(),
            on_progress=lambda d, tot, q: self.status_var.set(f"Downloading {q}..." if q else ""),
            on_log=on_log,
            on_complete=lambda: [self._refresh_library(), self.status_var.set("Discovery download complete")],
            check_cancel=lambda: False
        )

    def _refresh_library(self):
        """Manual refresh triggered by the user or startup."""
        threading.Thread(target=self._run_library_scan_bg, daemon=True).start()

    def _run_library_scan_bg(self):
        """The heavy lifting happens here on a background thread."""
        new_lib = scan_library(self._watch_roots)
        self.after(0, lambda: self._finalize_library_update(new_lib))

    def _finalize_library_update(self, new_lib):
        # Only refresh UI if contents actually changed to prevent stuttering
        current_hash = hash(str(new_lib))
        if current_hash == self._last_lib_hash:
            return
            
        self._last_lib_hash = current_hash
        self._library = new_lib
        
        # 1. Redraw the sidebar cards (updates song counts)
        self._populate_sidebar()
        
        # 2. Get the current search query so we don't lose our place
        search_q = self.lib_search_entry.get() if self.lib_search_entry.get() != self._lib_ph else ""
        
        # 3. Redraw the track list
        self._populate_track_list("")
        
        # 3b. Refresh the header art with current playlist paths
        lib_view = self._frames.get("library")
        if lib_view:
            name = self._selected_playlist or "All Music"
            total = sum(len(v) for v in self._library.values())
            cnt = len(self._library.get(name, [])) if self._selected_playlist else total
            lib_view.update_header_art(name, f"{cnt} songs")
        
        # 4. Update the download destination dropdown options
        options = ["— choose playlist —", "All Music", "Unsorted"]
        for pl in sorted(self._library.keys()):
            if pl not in ["Unsorted", "All Music"]:
                options.append(f"{pl}  ({len(self._library[pl])})")
        self._dest_options = options
        
        # 5. Update timestamp in Settings
        settings = self._frames.get("settings")
        if settings and hasattr(settings, 'last_refresh_lbl') and settings.last_refresh_lbl.winfo_exists():
            settings.last_refresh_lbl.config(text=f"Last refreshed: {time.strftime('%H:%M:%S')}")

        # Broadcast state handoff to all initialized views
        for name, frame in self._frames.items():
            if hasattr(frame, 'on_library_updated'):
                frame.on_library_updated()
            elif hasattr(frame, 'populate') and name != "discover":
                frame.populate()
            
            # Specific updates for download view
            if name == "download" and hasattr(frame, "update_dest_combo"):
                frame.update_dest_combo(self._dest_options)

    def _populate_sidebar(self):
        """Redraws sidebar cards with right-click menu restored."""
        for w in self._pl_inner.winfo_children():
            w.destroy()
        self._pl_card_frames = []

        

        # Organize playlist order
        total_songs = sum(len(v) for v in self._library.values())
        entries = [("All Music", total_songs)]
        if "Unsorted" in self._library: entries.append(("Unsorted", len(self._library["Unsorted"])))
        for name in sorted(self._library.keys()):
            if name not in ["Unsorted", "All Music"]:
                entries.append((name, len(self._library[name])))

        for idx, (name, count) in enumerate(entries):
            card = tk.Frame(self._pl_inner, bg=SIDEBAR, cursor="hand2")
            card.pack(fill="x", pady=1)
            inner = tk.Frame(card, bg=SIDEBAR, cursor="hand2")
            inner.pack(fill="x", padx=8, pady=3)

            pl_paths = []
            if name == "All Music":
                for t in self._library.values(): pl_paths.extend(t)
            else:
                pl_paths = self._library.get(name, [])

            art = PlaylistArt(inner, 42, name, app=self, paths=pl_paths, bg=SIDEBAR)
            art.pack(side="left", padx=(0, 10))
            txt_f = tk.Frame(inner, bg=SIDEBAR)
            txt_f.pack(side="left", fill="both", expand=True)
            nl = tk.Label(txt_f, text=name, font=("Helvetica Neue", 11, "bold"), fg=TEXT, bg=SIDEBAR, anchor="w")
            nl.pack(anchor="w")
            has_url = bool(self._get_playlist_url(name))
            url_indicator = "  🔗" if has_url else ""
            cl = tk.Label(txt_f, text=f"{count} songs{url_indicator}", font=("Helvetica Neue", 9), fg=TEXT3, bg=SIDEBAR, anchor="w")
            cl.pack(anchor="w")

            def show_context(e, pl_name=name):
                m = tk.Menu(self, tearoff=0, bg=BG3, fg=TEXT, font=("Helvetica Neue", 12))
                if pl_name != "All Music":
                    m.add_command(label=f"Dump '{pl_name}' elsewhere...", command=lambda: self._ctx_dump_playlist(pl_name))
                    m.add_separator()
                    m.add_command(label="🔗 Set Playlist URL...", command=lambda: self._ctx_set_playlist_url(pl_name))
                    if self._get_playlist_url(pl_name):
                        m.add_command(label="⬇ Sync Now", command=lambda: self._sync_playlist(pl_name))
                    m.add_separator()
                    m.add_command(label=f"🧹 Empty Playlist", command=lambda: self._ctx_clear_playlist_contents(pl_name))
                    if pl_name != "Unsorted":
                        m.add_command(label=f"🗑 Delete Folder", command=lambda: self._ctx_delete_playlist_folder(pl_name))
                m.post(e.x_root, e.y_root)

            for w in [card, inner, art, nl, cl, txt_f]:
                w.bind("<Button-1>", lambda e, n=name, i=idx: self._select_playlist_card(i, n))
                w.bind("<Button-2>", show_context)
                w.bind("<Button-3>", show_context)
                w.bind("<Enter>", lambda e, c=card, inn=inner, n=nl, cll=cl, tx=txt_f: [x.config(bg=BG3) for x in [c,inn,n,cll,tx]])
                w.bind("<Leave>", lambda e, c=card, inn=inner, n=nl, cll=cl, tx=txt_f: [x.config(bg=SIDEBAR) for x in [c,inn,n,cll,tx]])
                w.bind("<MouseWheel>", self._on_sidebar_wheel, add="+")

    def _select_playlist_card(self, idx, name):
        self._pl_selected_idx = idx
        # No persistent highlight — just act on the selection
        if name == "All Music":
            self._selected_playlist = None
            total = sum(len(v) for v in self._library.values())
            self._frames["library"].update_header_art("All Music", f"{total} songs")
        else:
            self._selected_playlist = name
            cnt = len(self._library.get(name, []))
            self._frames["library"].update_header_art(name, f"{cnt} songs")
        self._populate_track_list()
        self._show_tab("library")

    def _on_playlist_select(self, event):
        pass  # replaced by _select_playlist_card

    def _populate_track_list(self, filter_str=""):
        self._frames["library"].populate_track_list(filter_str)

    # ── Library search — fixed, no placeholder interference ──────────────────
    def _lib_focus_in(self, event):
        if self.lib_search_entry.get() == self._lib_ph:
            self.lib_search_entry.delete(0, "end")
            self.lib_search_entry.config(fg=TEXT)

    def _lib_focus_out(self, event):
        if not self.lib_search_entry.get().strip():
            self._lib_searching = True
            self.lib_search_entry.insert(0, self._lib_ph)
            self.lib_search_entry.config(fg=TEXT3)
            self._lib_searching = False
            # Restore all cards without destroying/rebuilding (no flash)
            self._reshow_all_cards()

    def _reshow_all_cards(self):
        """Lightweight repack: unpack all cards then re-pack in original children order.
        No widget destruction — avoids flicker from _populate_sidebar()."""
        cards = list(self._pl_inner.winfo_children())
        for c in cards:
            c.pack_forget()
        for c in cards:
            c.pack(fill="x", pady=1)
        # Reset scroll to top
        self._pl_scroll_canvas.yview_moveto(0)

    def _lib_key_released(self, event):
        q = self.lib_search_entry.get().strip().lower()
        if q == self._lib_ph.lower(): return
        
        # Snapshot the current children order (canonical from _populate_sidebar)
        cards = list(self._pl_inner.winfo_children())
        
        # Unpack all first to guarantee re-pack order
        for card_frame in cards:
            card_frame.pack_forget()
        
        if not q:
            # Empty query — just repack all in order
            for card_frame in cards:
                card_frame.pack(fill="x", pady=1)
            self._pl_scroll_canvas.yview_moveto(0)
            return
        
        # Re-pack only matching cards in their original order
        for card_frame in cards:
            found = False
            for inner in card_frame.winfo_children():
                for txt_f in inner.winfo_children():
                    if isinstance(txt_f, tk.Frame):
                         for lbl in txt_f.winfo_children():
                             if isinstance(lbl, tk.Label) and lbl.cget("font").count("bold"):
                                 pl_name = lbl.cget("text").lower()
                                 if q in pl_name:
                                     found = True
                                 break
            
            if found:
                card_frame.pack(fill="x", pady=1)

        # Scroll to top to see search results if scrolled down
        self._pl_scroll_canvas.yview_moveto(0)

    def _get_sel_paths(self):
        tab = self._current_tab
        if tab == "library" and "library" in self._frames:
            return self._frames["library"].get_sel_paths()
        elif tab == "recent" and "recent" in self._frames:
            return self._frames["recent"].get_sel_paths()
        elif tab == "artists" and "artists" in self._frames:
            return self._frames["artists"].get_sel_paths()
        return []

    def _on_track_dbl(self, event): pass   # handled by canvas
    def _on_right_click(self, event): pass  # handled by canvas

    # ── Context Menu Actions ──────────────────────────────────────────────────
    def _ctx_play_now(self):
        p = self._get_sel_paths()
        if p: self._play_with_context(p[0])

    def _ctx_play_next(self):
        paths = self._get_sel_paths()
        if not paths: return
        
        if not self.engine.queue:
            self.engine.queue = list(paths)
            self.engine.queue_idx = 0
            self.engine.user_queue_count = len(paths) - 1
            self.engine._original_queue = list(paths)
            self._update_queue()
            self.engine.play_current()
            return

        for path in reversed(paths):
            ins = self.engine.queue_idx + 1 if self.engine.queue_idx >= 0 else 0
            self.engine.queue.insert(ins, path)
            self.engine.user_queue_count = getattr(self.engine, 'user_queue_count', 0) + 1
            if hasattr(self.engine, '_original_queue') and self.engine._original_queue:
                try:
                    curr = self.engine.queue[self.engine.queue_idx]
                    orig_idx = self.engine._original_queue.index(curr)
                    self.engine._original_queue.insert(orig_idx + 1, path)
                except ValueError:
                    self.engine._original_queue.insert(ins, path)

        self._update_queue()

    def _ctx_add_queue(self):
        paths = self._get_sel_paths()
        if not paths: return
        
        if not self.engine.queue:
            self.engine.queue = list(paths)
            self.engine.queue_idx = 0
            self.engine.user_queue_count = len(paths) - 1
            self.engine._original_queue = list(paths)
            self._update_queue()
            self.engine.play_current()
            return
            
        for path in paths:
            ins = self.engine.queue_idx + 1 + getattr(self.engine, 'user_queue_count', 0)
            if ins > len(self.engine.queue): ins = len(self.engine.queue)
            
            self.engine.queue.insert(ins, path)
            self.engine.user_queue_count = getattr(self.engine, 'user_queue_count', 0) + 1
            
            if hasattr(self.engine, '_original_queue') and self.engine._original_queue:
                try:
                    curr = self.engine.queue[self.engine.queue_idx]
                    orig_idx = self.engine._original_queue.index(curr)
                    self.engine._original_queue.insert(orig_idx + self.engine.user_queue_count, path)
                except ValueError:
                    self.engine._original_queue.append(path)
                    
        self._update_queue()





    def _check_playlist_duplicates(self, pl_name):
        """Localized duplicate check for a specific playlist."""
        if not pl_name: tracks = [p for sub in self._library.values() for p in sub]
        else: tracks = self._library.get(pl_name, [])
        
        if not tracks:
            messagebox.showinfo("Check Duplicates", "This playlist is empty.")
            return
            
        def run_scan():
            from engine import find_duplicates
            # find_duplicates expects a dict: { playlist_name: [paths] }
            dupes = find_duplicates({pl_name or "All Music": tracks}, 
                                    allowed_keys=self.engine.allowed_dupe_keys)
            self.after(0, lambda: self._finalize_playlist_dupe_check(dupes, pl_name))
            
        threading.Thread(target=run_scan, daemon=True).start()

    def _finalize_playlist_dupe_check(self, dupes, pl_name):
        if not dupes:
            messagebox.showinfo("Check Duplicates", f"No duplicates found in '{pl_name or 'All Music'}'.")
            return
            
        # Show results in a confirm dialog or redirect to downloader
        msg = f"Found {len(dupes)} sets of duplicates in '{pl_name or 'All Music'}'.\n\nWould you like to open the Duplicate Finder hub to manage them?"
        if messagebox.askyesno("Duplicates Found", msg):
            self._show_tab("download")
            self._frames["download"].show_dupes_tab()
            # Optionally we could pass the filtered list to the downloader, 
            # but for now we'll just open the hub as requested.

    # ── PLAYLIST URL SYNC ──────────────────────────────────────────────────
    def _sync_playlist(self, pl_name, url=None, on_done=None, batch_mode=False,
                       on_track_log=None, on_playlist_msg=None, on_progress_update=None):
        """Sync a YT playlist URL to a local playlist folder.

        If url is None, looks up the stored URL for pl_name.
        Delegates to Downloader.download_playlist_sync for fetch/compare/download.
        When batch_mode=True, suppresses the completion messagebox.
        on_track_log: optional callback (title, is_success, err) for per-track log messages.
        on_playlist_msg: optional callback (msg) for informational playlist messages.
        """
        if not url:
            url = self._get_playlist_url(pl_name)
        if not url:
            if batch_mode:
                self._log_error("Sync Batch", f"✗ No URL configured for '{pl_name}'")
                if on_done: on_done(False, 0, 0)
            else:
                messagebox.showwarning("No URL", f"No playlist URL configured for '{pl_name}'.\n\nSet one via the playlist actions dropdown or right-click the playlist in the sidebar.")
            return

        dest = self._find_playlist_dir(pl_name)
        if not dest:
            if batch_mode:
                self._log_error("Sync Batch", f"✗ '{pl_name}' — folder not found on disk")
                if on_done: on_done(False, 0, 0)
            else:
                messagebox.showerror("Not Found", f"Playlist folder '{pl_name}' not found on disk.")
            return

        self._sync_running = True
        self.status_var.set(f"Syncing {pl_name}…")
        lib = self._frames.get("library")
        if lib: lib.show_sync_progress()

        from downloader import Downloader
        attempted = [0]
        succeeded = [0]

        def on_progress(idx, tot, title):
            pct = (idx / max(tot, 1)) * 100
            if on_progress_update:
                self.after(0, lambda p=pct: on_progress_update(p))
            if title:
                attempted[0] += 1
                self.after(0, lambda: self.status_var.set(f"Syncing {pl_name}: {title} ({idx+1}/{tot})"))
            else:
                if not on_progress_update:
                    self.after(0, lambda p=pct: self.prog_var.set(p))

        def on_log(title, is_success, err):
            if err == "already have":
                msg = f"  ↻ {title}"
                inc = False
            else:
                msg = f"  {'✓' if is_success else '✗'} {title}"
                if err: msg += f" — {err}"
                inc = is_success
            self._log_error("Sync", msg)
            if inc:
                succeeded[0] += 1
            if on_track_log:
                self.after(0, lambda: on_track_log(title, is_success, err))

        def _on_playlist_msg(msg):
            self._log_error("Sync Playlist", msg)
            if on_playlist_msg:
                self.after(0, lambda: on_playlist_msg(msg))

        def on_complete():
            self.after(0, self._refresh_library)
            count = succeeded[0]
            if attempted[0] == 0:
                msg = f"'{pl_name}' is up to date! ✓"
            elif count == attempted[0]:
                msg = f"Downloaded {count} new track(s) to '{pl_name}'."
            else:
                msg = f"Downloaded {count}/{attempted[0]} new track(s) to '{pl_name}'."
            if on_progress_update:
                self.after(0, lambda: on_progress_update(100))
            else:
                self.after(0, lambda: self.prog_var.set(100))
            self.after(0, lambda: self.status_var.set(msg))
            if batch_mode:
                self.after(0, lambda: self._log_error("Sync Batch", msg))
                self.after(0, lambda: setattr(self, '_sync_running', False))
            else:
                self.after(0, lambda: [
                    messagebox.showinfo("Sync Complete", msg),
                    setattr(self, '_sync_running', False),
                    self._frames["library"].focus_set() if "library" in self._frames else None
                ])
            self.after(500, lambda: (
                self._frames["library"].hide_sync_progress()
                if "library" in self._frames else None
            ))
            if on_done: self.after(0, lambda: on_done(True, succeeded[0], attempted[0]))

        Downloader.download_playlist_sync(
            url, dest, self.skip_var.get(),
            self.sc_var.get(), on_progress, on_log, on_complete,
            lambda: not self._sync_running, on_playlist_msg=_on_playlist_msg)

    def _sync_all_playlists(self, urls_and_names, on_all_done=None,
                            on_playlist_start=None, on_playlist_done=None,
                            on_track_log=None, on_playlist_msg=None,
                            on_batch_progress=None):
        """Sync a list of (name, url) tuples sequentially."""
        total = len(urls_and_names)
        total_new = [0]
        total_attempted = [0]
        def process_next(idx):
            if idx >= total:
                if on_batch_progress:
                    on_batch_progress(100)
                if on_all_done: on_all_done(total_new[0], total_attempted[0])
                return
            name, url = urls_and_names[idx]
            if on_batch_progress:
                on_batch_progress((idx / max(total, 1)) * 100)
            self._log_error("Sync Batch", f"▶ Syncing '{name}' [{idx+1}/{total}]...")
            if on_playlist_start:
                on_playlist_start(name, idx + 1, total)
            def _done(success=True, count=0, attempted_count=0):
                total_new[0] += count
                total_attempted[0] += attempted_count
                if on_playlist_done:
                    on_playlist_done(name, success, count, attempted_count)
                if on_batch_progress:
                    on_batch_progress(((idx + 1) / max(total, 1)) * 100)
                process_next(idx + 1)
            def _playlist_progress(pct):
                if on_batch_progress:
                    on_batch_progress(((idx + (pct / 100)) / max(total, 1)) * 100)
            self._sync_playlist(pl_name=name, url=url, on_done=_done,
                                batch_mode=True, on_track_log=on_track_log,
                                on_playlist_msg=on_playlist_msg,
                                on_progress_update=_playlist_progress)
        process_next(0)

    def _ctx_remove_from_library(self):
        paths = self._get_sel_paths()
        if not paths: return
        
        # Confirm deletion
        if messagebox.askyesno("Delete Song", f"Permanently delete {len(paths)} song(s) from your storage?"):
            for path in paths:
                try:
                    if os.path.exists(path):
                        os.remove(path)
                except Exception as e:
                    print(f"Error deleting file: {e}")
            
            # Refresh the UI to show the songs are gone
            self._refresh_library()
            


    def _ctx_clear_playlist_contents(self, pl_name):
        """Deletes files inside the folder but keeps the folder."""
        tracks = self._library.get(pl_name, [])
        if not tracks: return
        
        confirm = messagebox.askyesno("Empty Playlist", 
            f"Are you sure you want to delete all {len(tracks)} songs in '{pl_name}'?\n\nThis will NOT delete the folder.")
        
        if confirm:
            for path in tracks:
                try:
                    if os.path.exists(path): os.remove(path)
                except: pass
            self._refresh_library()

    def _ctx_delete_playlist_folder(self, pl_name):
        """Deletes the files AND the folder."""
        # 1. Safety Block: Identify names that represent protected folders
        root_name = os.path.basename(MUSIC_ROOT)
        protected_names = ["All Music", "Unsorted", root_name, "Vibe_Library"]

        if pl_name in protected_names:
            messagebox.showwarning("Access Denied", 
                f"You cannot delete '{pl_name}' because it is your core library folder.\n\n"
                "To remove songs from here, select the songs and choose 'Delete from Library'.")
            return

        # 2. Find the actual path
        folder_path = self._find_playlist_dir(pl_name)
        
        # 3. Final Path Safety Check: Ensure the path isn't literally the MUSIC_ROOT
        if not folder_path or not os.path.exists(folder_path):
            messagebox.showerror("Error", f"Could not locate folder '{pl_name}' on disk.")
            return
            
        if os.path.abspath(folder_path) == os.path.abspath(MUSIC_ROOT):
            messagebox.showerror("Safety Error", "Target path is the Root Directory. Deletion blocked.")
            return

        # 4. Confirmation
        count = len(self._library.get(pl_name, []))
        confirm = messagebox.askyesno("Delete Playlist", 
            f"Permanently delete the folder '{pl_name}' and all {count} songs inside?")
        
        if confirm:
            try:
                import shutil
                shutil.rmtree(folder_path)
                
                # Reset view to "All Music" if we were inside that playlist
                if self._selected_playlist == pl_name:
                    self._select_playlist_card(0, "All Music")
                
                self._refresh_library()
                messagebox.showinfo("Success", f"Deleted playlist: {pl_name}")
            except Exception as e:
                messagebox.showerror("Error", f"Failed to delete folder: {e}")              

    def _ctx_remove_from_queue_idx(self, idx):
        if 0 <= idx < len(self.engine.queue):
            self.engine.queue.pop(idx)
            if self.engine.queue_idx >= idx:
                self.engine.queue_idx = max(0, self.engine.queue_idx - 1)
            self._update_queue()

    def _ctx_remove_queue(self):
        # This was for the old treeview, now we use _ctx_remove_from_queue_idx
        # But if called from library, it might mean "clear queue" or something?
        # Let's just point it to a safe "Clear Queue" for now or leave it.
        pass

    def _find_playlist_dir(self, pl_name):
        # If it's one of the root-level names, return the MUSIC_ROOT
        root_name = os.path.basename(MUSIC_ROOT)
        if pl_name in ["Unsorted", "Vibe_Library", root_name]:
            return MUSIC_ROOT
            
        # Otherwise, look for the subfolder
        for root in self._watch_roots:
            path = os.path.join(root, pl_name)
            if os.path.isdir(path):
                return path
        return None

    # ── Player Operations (Main Delegates) ──────────────────────────────────
    def _play_with_context(self, path, context_list=None):
        self.engine.play_with_context(path, context_list=context_list)

    def _play_recent_all(self):
        if not self._recently_played:
            return
        self._play_with_context(self._recently_played[0],
                                context_list=list(self._recently_played))

    def _clear_recent(self):
        self._recently_played.clear()
        self._frames["recent"].redraw()

    def _play_all(self):
        paths = [item[0] for item in self._frames["library"]._track_items]
        if not paths: return
        self.engine.queue = list(paths)
        self.engine.queue_idx = 0
        self._update_queue()
        self.engine.play_current()

    def _shuffle_all(self):
        """Action handler for the Library Shuffle button. Syncs with global state."""
        # Ensure we are pulling paths from the currently visible tab
        for tab_name, view in self._frames.items():
            if view.winfo_ismapped() and hasattr(view, "_track_items"):
                 paths = [item[0] for item in view._track_items]
                 break
        else:
             paths = [item[0] for item in self._frames["library"]._track_items]
             
        if not paths: return
        
        # Set context so we can un-shuffle later
        self._queue_playlist_paths = list(paths)
        
        # Scramble
        shuffled = list(paths); random.shuffle(shuffled)
        self.engine.queue = shuffled
        self.engine.queue_idx = 0
        
        # Activate global shuffle mode and sync UI
        self.engine.shuffle = True
        self._broadcast_shuffle_state()
        
        self._update_queue()
        self.engine.play_current()

    # ═══════════════════════════════════════════════════════════════════════════
    # SEARCH
    # ═══════════════════════════════════════════════════════════════════════════
    # ── Search/Downloader ───────────────────────────────────────────────────

    # ═══════════════════════════════════════════════════════════════════════════
    # DOWNLOADER
    # ═══════════════════════════════════════════════════════════════════════════
    # ── Downloader / Utils ───────────────────────────────────────────────────

    # ═══════════════════════════════════════════════════════════════════════════
    # SLEEP TIMER
    # ═══════════════════════════════════════════════════════════════════════════
    def _set_sleep_timer(self):
        val = self._sleep_var.get()
        if hasattr(self, "_sleep_after_id") and self._sleep_after_id:
            self.after_cancel(self._sleep_after_id)
            self._sleep_after_id = None
            
        settings = self._frames.get("settings")
        if val == "Off":
            if settings and hasattr(settings, "sleep_lbl"):
                settings.sleep_lbl.config(text="Sleep timer off")
            return
            
        mins = int(val.split()[0])
        self._sleep_after_id = self.after(mins * 60 * 1000, self._sleep_trigger)
        if settings and hasattr(settings, "sleep_lbl"):
            settings.sleep_lbl.config(text=f"Pausing in {mins} minutes")

    def _sleep_trigger(self):
        if self.engine.playing and not self.engine.paused:
            self.engine.toggle_play()
            
        settings = self._frames.get("settings")
        if settings and hasattr(settings, "sleep_lbl"):
            settings.sleep_lbl.config(text="Sleep timer fired — playback paused")
            
        self._sleep_var.set("Off")

    # ═══════════════════════════════════════════════════════════════════════════
    # CROSSFADE
    # ═══════════════════════════════════════════════════════════════════════════
    def _set_crossfade(self):
        val = self._cf_var.get()
        self._crossfade_secs = 0 if val == "Off" else int(val.replace("s",""))

    def _do_crossfade_out(self, on_done):
        if not PYGAME_AVAILABLE or not hasattr(self,"_crossfade_secs") or self._crossfade_secs <= 0:
            on_done(); return
        steps = 20; interval = int(self._crossfade_secs * 1000 / steps)
        orig_vol = self.engine.volume; step_vol = orig_vol / steps
        def fade(rem):
            if rem <= 0:
                pygame.mixer.music.set_volume(orig_vol); on_done(); return
            pygame.mixer.music.set_volume(max(0, pygame.mixer.music.get_volume() - step_vol))
            self.after(interval, lambda: fade(rem-1))
        fade(steps)

    def _do_crossfade_in(self):
        if not PYGAME_AVAILABLE or not hasattr(self,"_crossfade_secs") or self._crossfade_secs <= 0: return
        steps = 20; interval = int(self._crossfade_secs * 1000 / steps)
        target = self.engine.volume; step_vol = target / steps
        pygame.mixer.music.set_volume(0)
        def fade(rem):
            if rem <= 0:
                pygame.mixer.music.set_volume(target); return
            pygame.mixer.music.set_volume(min(target, pygame.mixer.music.get_volume() + step_vol))
            self.after(interval, lambda: fade(rem-1))
        fade(steps)

    # ═══════════════════════════════════════════════════════════════════════════
    # SMART PLAYLISTS
    # ═══════════════════════════════════════════════════════════════════════════
    def _generate_smart_playlist(self, mode):
        paths = []; name = None
        if mode == "most_played":
            freq = {}
            for p in self._recently_played: freq[p] = freq.get(p,0)+1
            paths = sorted(freq, key=lambda x: freq[x], reverse=True)[:30]
            name  = "Most Played"
        elif mode == "recent":
            all_t = [(p, os.path.getmtime(p))
                     for tracks in self._library.values()
                     for p in tracks if os.path.exists(p)]
            all_t.sort(key=lambda x: x[1], reverse=True)
            paths = [p for p,_ in all_t[:50]]; name = "Recently Added"
        elif mode == "artist":
            artist = simpledialog.askstring("Artist Playlist","Enter artist name:",parent=self)
            if not artist: return
            artist = artist.strip().lower()
            for tracks in self._library.values():
                for p in tracks:
                    _, a = split_track_name(p)
                    if artist in a.lower(): paths.append(p)
            name = f"{artist.title()} — All Tracks"
        if not paths:
            messagebox.showinfo("No Tracks","No tracks found for this smart playlist."); return
        pl_dir = os.path.join(MUSIC_ROOT, name)
        os.makedirs(pl_dir, exist_ok=True)
        for src in paths:
            dst = os.path.join(pl_dir, os.path.basename(src))
            if not os.path.exists(dst):
                try: shutil.copy2(src, dst)
                except: pass
        self._refresh_library()
        messagebox.showinfo("Done", f"Created \"{name}\" with {len(paths)} tracks.")

    # ═══════════════════════════════════════════════════════════════════════════
    # KEYBOARD SHORTCUTS
    # ═══════════════════════════════════════════════════════════════════════════
    def _bind_keys(self):
        self.bind("<XF86AudioPlay>",         lambda e: self._media_toggle_play() if hasattr(self, 'engine') and self.engine is not None and getattr(self.engine, 'mixer_ready', False) else None)
        self.bind("<XF86AudioNext>",         lambda e: self.engine.next_track() if hasattr(self, 'engine') and self.engine is not None and getattr(self.engine, 'mixer_ready', False) else None)
        self.bind("<XF86AudioPrev>",         lambda e: self.engine.prev_track() if hasattr(self, 'engine') and self.engine is not None and getattr(self.engine, 'mixer_ready', False) else None)
        self.bind("<Up>",             lambda e: self._change_volume(0.05))
        self.bind("<Down>",           lambda e: self._change_volume(-0.05))


    def _should_handle_media_keys(self):
        return self.global_keys_var.get()

    def _on_global_keys_changed(self):
        if self.global_keys_var.get():
            self.after(0, self._start_media_key_listener)
        else:
            self._stop_media_key_listener()

    def _media_ready(self):
        """Returns True if the playback system is fully initialized and ready for commands."""
        if self._closing:
            return False
        if not hasattr(self, 'engine') or self.engine is None:
            return False
        if not getattr(self.engine, 'mixer_ready', False):
            return False
        return True

    def _media_play(self):
        if not self._media_ready():
            return
        if self.engine.playing and not self.engine.paused:
            return
        if self.engine.paused or self.engine.queue:
            self.engine.toggle_play()
            return
        self._play_all()

    def _media_pause(self):
        if not self._media_ready():
            return
        if self.engine.playing and not self.engine.paused:
            self.engine.toggle_play()

    def _media_toggle_play(self):
        if not self._media_ready():
            return
        if not self.engine.playing and not self.engine.queue:
            self._play_all()
            return
        self.engine.toggle_play()

    def _enqueue_media_command(self, command):
        if self._closing:
            return
        if not hasattr(self, 'engine') or self.engine is None:
            return
        if not getattr(self.engine, 'mixer_ready', False):
            return
        self._media_command_queue.put(command)

    def _start_media_key_listener(self):
        """Register MPRemoteCommandCenter handlers directly (no subprocess).
        Register togglePlayPauseCommand instead of separate play/pause to avoid
        macOS routing a single physical press to multiple commands."""
        if not self._should_handle_media_keys():
            return
        if not hasattr(self, 'engine') or self.engine is None:
            return
        if not (_HAS_PYOBJC and _HAS_MEDIAPLAYER):
            return
        if getattr(self, "_mp_registered", False):
            return

        try:
            self._mp_command_center = MPRemoteCommandCenter.sharedCommandCenter()
            tokens = []
            handlers = []

            def _make_handler(cmd_name):
                def handler(_event):
                    try:
                        self._enqueue_media_command(cmd_name)
                    except BaseException:
                        pass
                    return 0   # MPRemoteCommandHandlerStatus.Success
                return handler

            command_map = [
                ("toggle",   self._mp_command_center.togglePlayPauseCommand),
                ("next",     self._mp_command_center.nextTrackCommand),
                ("previous", self._mp_command_center.previousTrackCommand),
            ]

            for name, getter in command_map:
                cmd = getter()
                handler = _make_handler(name)
                token = cmd.addTargetWithHandler_(handler)
                handlers.append(handler)
                tokens.append((cmd, token))

            self._mp_handlers = handlers
            self._mp_tokens = tokens
            self._mp_registered = True
            self._log_error("GlobalKeys", "Media key handlers registered")
        except Exception as e:
            self._log_error("GlobalKeys", f"Failed to register media keys: {e}")

    def _stop_media_key_listener(self):
        """Unregister MPRemoteCommandCenter handlers and clear Now Playing."""
        if not (_HAS_PYOBJC and _HAS_MEDIAPLAYER):
            return
        if not getattr(self, "_mp_registered", False):
            return
        try:
            for cmd, token in getattr(self, '_mp_tokens', []):
                try:
                    if hasattr(cmd, 'removeTarget_') and token is not None:
                        cmd.removeTarget_(token)
                    # Disable the command to avoid stale system target
                    if hasattr(cmd, 'setEnabled_'):
                        cmd.setEnabled_(False)
                except Exception:
                    pass
            self._mp_tokens = []
            self._mp_handlers = []
            self._mp_registered = False
            self._mp_command_center = None
            # Clear Now Playing info
            self._clear_now_playing()
            self._log_error("GlobalKeys", "Media key handlers unregistered")
        except Exception as e:
            self._log_error("GlobalKeys", f"Failed to unregister media keys: {e}")

    def _poll_media_commands(self):
        actions = {
            "play": self._media_play,
            "pause": self._media_pause,
            "toggle": self._media_toggle_play,
            "next": self.engine.next_track,
            "previous": self.engine.prev_track,
            "stop": self.engine.stop,
            "volume_up": lambda: self._change_volume(0.05),
            "volume_down": lambda: self._change_volume(-0.05),
        }
        while True:
            try:
                command = self._media_command_queue.get_nowait()
            except queue.Empty:
                break
            action = actions.get(command)
            if action:
                try:
                    if not self._media_ready():
                        break
                    action()
                except Exception as e:
                    self._log_error("GlobalKeys", f"Media command failed: {e}")

        # Periodic Now Playing elapsed/rate update (~every 2 s)
        self._np_poll_counter += 1
        if self._np_poll_counter % 40 == 0:
            self._refresh_now_playing()

        if not self._closing:
            self._poll_after_id = self.after(50, self._poll_media_commands)



    def _init_remote_commands(self):
        """Set up Now Playing info center key strings (called once at startup).
        Command handler registration happens in _start_media_key_listener."""
        if getattr(self, "_remote_commands_ready", False):
            return
        if not (_HAS_PYOBJC and _HAS_MEDIAPLAYER):
            return
        try:
            self._mp_title_key = MPMediaItemPropertyTitle
            self._mp_artist_key = MPMediaItemPropertyArtist
            self._mp_duration_key = MPMediaItemPropertyPlaybackDuration
            self._mp_rate_key = MPNowPlayingInfoPropertyPlaybackRate
            self._mp_elapsed_key = MPNowPlayingInfoPropertyElapsedPlaybackTime
            self._mp_info_center = MPNowPlayingInfoCenter.defaultCenter()
            self._mp_info_center.setNowPlayingInfo_({self._mp_title_key: "BoomTunes"})
            self._remote_commands_ready = True
        except Exception as e:
            self._log_error("GlobalKeys", f"Now Playing setup failed: {e}")

    def _update_now_playing_info(self, title="", artist="", duration=0.0):
        """Publish truthful Now Playing metadata/state from engine.
        Rate is 1 when playing, 0 when paused/stopped.
        Elapsed is computed from track_start or seek_pos."""
        if not hasattr(self, '_mp_info_center'):
            return
        try:
            rate = 1.0 if (self.engine.playing and not self.engine.paused) else 0.0
            elapsed = 0.0
            if self.engine.playing and not self.engine.paused:
                elapsed = time.time() - self.engine.track_start
            elif self.engine.paused:
                elapsed = self.engine.seek_pos

            now_playing = {
                self._mp_title_key: title or "",
                self._mp_artist_key: artist or "",
                self._mp_duration_key: duration,
                self._mp_rate_key: rate,
                self._mp_elapsed_key: elapsed,
            }
            self._mp_info_center.setNowPlayingInfo_(now_playing)
        except Exception as e:
            print(f"[GK] NowPlaying update failed: {e}", flush=True)

    def _refresh_now_playing(self):
        """Periodic refresh of elapsed/rate without changing metadata."""
        if not hasattr(self, '_mp_info_center') or not getattr(self, "_remote_commands_ready", False):
            return
        if not self.engine.queue or self.engine.queue_idx < 0:
            return
        try:
            path = self.engine.queue[self.engine.queue_idx]
            title, artist = split_track_name(path)
            self._update_now_playing_info(title, artist, self.engine.duration)
        except Exception:
            pass

    def _change_volume(self, delta):
        new_vol = max(0, min(1, self.engine.volume + delta))
        self.engine.volume = new_vol
        self.vol_slider.set(new_vol * 100)
        if PYGAME_AVAILABLE: pygame.mixer.music.set_volume(new_vol)

    def _init_watchdog(self):
        """Starts the automatic background refresh loop."""
        # Only refresh if we aren't currently in the middle of a download
        if not self._is_dl:
            self._refresh_library()
        
        # Schedule the next run in 30 seconds
        self.after(30000, self._init_watchdog)
        
    def _change_master_library(self):
        new_dir = filedialog.askdirectory(title="Select Master Music Folder")
        if not new_dir: return
        
        global MUSIC_ROOT
        MUSIC_ROOT = new_dir
        self.MUSIC_ROOT = new_dir
        self._watch_roots = [new_dir]
        self.dl_dest_var.set(new_dir)
        
        settings = self._frames.get("settings")
        if settings and hasattr(settings, "master_path_lbl"):
            settings.master_path_lbl.config(text=new_dir)
            
        self._load_playlist_meta()
        self._refresh_library()
        
    # ── PERSISTENCE ────────────────────────────────────────────────────────
    def _load_config(self):
        """Restores application state from ~/.vibe_config.json"""
        if not os.path.exists(CONFIG_FILE):
            return
            
        try:
            with open(CONFIG_FILE, "r") as f:
                cfg = json.load(f)
            
            # Library Path
            if "music_root" in cfg and os.path.exists(cfg["music_root"]):
                global MUSIC_ROOT
                MUSIC_ROOT = cfg["music_root"]
                self.MUSIC_ROOT = MUSIC_ROOT
                self._watch_roots = [MUSIC_ROOT]
                self.dl_dest_var.set(MUSIC_ROOT)
            
            # API Keys
            if "lastfm_key" in cfg:
                self.lastfm_key_var.set(cfg["lastfm_key"])
                if hasattr(self, "discovery"):
                    self.discovery.api_key = cfg["lastfm_key"]
            
            if "auto_discovery" in cfg:
                self.auto_discovery_var.set(cfg["auto_discovery"])
                
            # Playback Settings
            if "volume" in cfg:
                self.vol_var.set(cfg["volume"])
            
            if "crossfade" in cfg:
                self._cf_var.set(cfg["crossfade"])
                self._set_crossfade()
                
            if "sleep_timer" in cfg:
                self._sleep_var.set(cfg["sleep_timer"])
                # We don't auto-start sleep timer on load, just restore the preference

            if "global_keys" in cfg:
                self.global_keys_var.set(cfg["global_keys"])
                
        except Exception as e:
            print(f"Failed to load config: {e}")

    def _save_config(self):
        """Persists current state to ~/.vibe_config.json"""
        cfg = {
            "music_root": self.MUSIC_ROOT,
            "lastfm_key": self.lastfm_key_var.get(),
            "auto_discovery": self.auto_discovery_var.get(),
            "volume": self.vol_var.get(),
            "crossfade": self._cf_var.get(),
            "sleep_timer": self._sleep_var.get(),
            "global_keys": self.global_keys_var.get()
        }
        try:
            with open(CONFIG_FILE, "w") as f:
                json.dump(cfg, f, indent=4)
        except Exception as e:
            print(f"Failed to save config: {e}")

    # ── PLAYLIST URL PERSISTENCE ─────────────────────────────────────────────
    def _playlist_meta_path(self):
        return os.path.join(self.MUSIC_ROOT, ".vibe_playlist_urls.json")

    def _load_playlist_meta(self):
        path = self._playlist_meta_path()
        if os.path.exists(path):
            try:
                with open(path, "r") as f:
                    self._playlist_urls = json.load(f)
            except Exception:
                self._playlist_urls = {}

    def _save_playlist_meta(self):
        path = self._playlist_meta_path()
        try:
            with open(path, "w") as f:
                json.dump(self._playlist_urls, f, indent=4)
        except Exception as e:
            print(f"Failed to save playlist URLs: {e}")

    def _get_playlist_url(self, name):
        return self._playlist_urls.get(name)

    def _set_playlist_url(self, name, url):
        if url:
            self._playlist_urls[name] = url
        else:
            self._playlist_urls.pop(name, None)
        self._save_playlist_meta()

    def _clear_now_playing(self):
        """Clear Now Playing info (empty dict publishes no-track state)."""
        if hasattr(self, '_mp_info_center'):
            try:
                self._mp_info_center.setNowPlayingInfo_({})
            except Exception:
                pass

    def _on_closing(self):
        """Cleanly saves config and quits."""
        self._closing = True
        # Cancel scheduled poll callbacks
        if getattr(self, "_poll_after_id", None):
            try:
                self.after_cancel(self._poll_after_id)
            except Exception:
                pass
            self._poll_after_id = None
        self._save_config()
        self._save_playlist_meta()
        self._stop_media_key_listener()
        self._clear_now_playing()
        if hasattr(self, "engine"):
            if PYGAME_AVAILABLE:
                pygame.mixer.quit()
        self.destroy()
        sys.exit(0)
        


import signal
def signal_handler(sig, frame):
    pygame.mixer.quit()
    sys.exit(0)
signal.signal(signal.SIGINT, signal_handler)

if __name__ == "__main__":
    app = VibeMusicPlayer()
    app.mainloop()
