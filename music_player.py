#!/opt/homebrew/bin/python3.12
"""
Luno v4
- Fixed library search (exact title/artist matching)
- Round play button via Canvas
- Modern thin scrubber and volume slider via Canvas (not ttk Scale)
- Duplicate song detection and management
- Gradient playlist art headers
build command: pyinstaller --noconsole --windowed --noconfirm --clean --name "Luno" music_player.py
pyinstaller --noconsole --windowed --noconfirm --clean 
  --name "Luno"
  --icon "icon.icns" 
  music_player.py

"""



import tkinter as tk
from tkinter import ttk, filedialog, messagebox, simpledialog
import threading
import queue
from collections import OrderedDict
import os, re, csv, time, random, math, hashlib, shutil, traceback, json
from urllib.parse import urlparse
from datetime import datetime
os.environ["PATH"] += os.pathsep + "/opt/homebrew/bin" + os.pathsep + "/usr/local/bin"
os.environ["PATH"] += os.pathsep + "/opt/homebrew/bin"

import sys
from theme import *
from utils import (
    resource_path, format_time, hex_to_rgb, rgb_to_hex, draw_gradient_rect,
    split_track_name, bind_scroll_events, scroll_amount,
)
from widgets import RoundButton, SlimSlider, PlaylistArt
from views.library import LibraryView
from views.downloader import DownloadView
from views.recent import RecentView
from views.settings import SettingsView
from engine import (
    scan_library, probe_soundcloud, parse_exportify_csv, find_duplicates,
    PYGAME_AVAILABLE, YT_DLP_AVAILABLE, MUTAGEN_AVAILABLE,
    pygame, yt_dlp, MP3, LunoEngine, DiscoveryService
)

from views.discover import DiscoverView
from views.song_page import SongPageView
import previews
import rec_artwork
from library_transfer import (
    apply_import_plan,
    build_manifest,
    load_manifest,
    plan_import,
    write_manifest,
)

# ── LIBRARY PATH SETUP ────────────────────────────────────────────────────────
# Default to ~/Music/app_music_folder
MUSIC_ROOT = os.path.join(os.path.expanduser("~"), "Music", "app_music_folder")
FAVORITES_PLAYLIST = "Favorites"

try:
    os.makedirs(MUSIC_ROOT, exist_ok=True)
except Exception as e:
    print(f"Error creating library folder: {e}")

WATCH_ROOTS = [MUSIC_ROOT]
CONFIG_FILE = os.path.join(os.path.expanduser("~"), ".luno_config.json")
# Mobile RecommendationPreviewManager parity: temporary recommendation audio is
# staged OUTSIDE the library root (so library scans and the Unsorted view never
# see it) and wiped at every app launch, exactly like the Android cache preview
# directory (DownloadRepository.PREVIEW_DOWNLOAD_DIR).
PREVIEW_DIR = os.path.join(os.path.expanduser("~"), ".luno_previews")
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
    # Keep artwork bounded. A 1000px RGBA image is roughly 4 MB, so an
    # unbounded cache can consume gigabytes after browsing a large library.
    MAX_ENTRIES = 96
    MAX_PIXELS = 16 * 1024 * 1024
    MAX_INFLIGHT = 24
    MAX_SOURCE_BYTES = 12 * 1024 * 1024
    MAX_SOURCE_PIXELS = 16 * 1024 * 1024

    def __init__(self, app):
        self.app = app
        self.cache = OrderedDict()
        self._cache_pixels = 0
        self._color_cache = {}
        self._color_fetching = set()
        self.fetching = set()
        self._pending = OrderedDict()
        self._callbacks = {}
        self.executor = ThreadPoolExecutor(max_workers=4)

    def get(self, path, size, callback):
        if not path:
            return None
        key = (path, size)
        if key in self.cache:
            self.cache.move_to_end(key)
            return self.cache[key][0]
            
        if key in self.fetching or key in self._pending:
            if callback and callback not in self._callbacks.setdefault(key, []):
                self._callbacks[key].append(callback)
            return None

        self._pending[key] = (path, size)
        self._callbacks[key] = [callback] if callback else []
        self._drain()
        return None

    def _schedule_drain(self):
        try:
            self.app.after(0, self._drain)
        except (RuntimeError, tk.TclError):
            self._drain()

    def _drain(self):
        while self._pending and len(self.fetching) < self.MAX_INFLIGHT:
            key, (path, size) = self._pending.popitem(last=False)
            self.fetching.add(key)
            try:
                self.executor.submit(self._fetch, path, size)
            except RuntimeError:
                self.fetching.discard(key)
                self._callbacks.pop(key, None)
                break

    def _release_key(self, key):
        self.fetching.discard(key)
        self._callbacks.pop(key, None)
        self._schedule_drain()

    def get_dominant_color(self, path, fallback="#1DB954"):
        if not path:
            return fallback
        if path in self._color_cache:
            return self._color_cache[path]
        if (path not in self._color_fetching and
                len(self.fetching) + len(self._color_fetching) < self.MAX_INFLIGHT):
            self._color_fetching.add(path)
            try:
                self.executor.submit(self._fetch_color, path)
            except RuntimeError:
                self._color_fetching.discard(path)
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
                        if len(tag.data) > self.MAX_SOURCE_BYTES:
                            continue
                        with Image.open(io.BytesIO(tag.data)) as source:
                            if source.width * source.height > self.MAX_SOURCE_PIXELS:
                                continue
                            img = source.convert("RGB")
                        tiny = img.resize((1, 1), Image.Resampling.LANCZOS)
                        color = tiny.getpixel((0, 0))
                        img.close()
                        hex_color = rgb_to_hex(*color)
                        self._color_cache[path] = hex_color
                        self.app.after(0, lambda: self.app._update_player_accent(path))
                        return
        except Exception:
            pass
        finally:
            self._color_fetching.discard(path)
            self._schedule_drain()

    def _fetch(self, path, size):
        key = (path, size)
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
            if img_data and len(img_data) <= self.MAX_SOURCE_BYTES:
                with Image.open(io.BytesIO(img_data)) as source:
                    if source.width * source.height > self.MAX_SOURCE_PIXELS:
                        self._release_key(key)
                        return
                    img = source.convert("RGBA")
                target_w, target_h = size if isinstance(size, tuple) else (size, size)
                source_ratio = img.width / img.height
                target_ratio = target_w / target_h
                if source_ratio > target_ratio:
                    crop_w = int(img.height * target_ratio)
                    left = (img.width - crop_w) // 2
                    img = img.crop((left, 0, left + crop_w, img.height))
                else:
                    crop_h = int(img.width / target_ratio)
                    top = (img.height - crop_h) // 2
                    img = img.crop((0, top, img.width, top + crop_h))
                img = img.resize((target_w, target_h), Image.Resampling.LANCZOS)
                
                # Extract dominant color for player bar accent
                try:
                    rgb_img = img.convert("RGB")
                    tiny = rgb_img.resize((1, 1), Image.Resampling.LANCZOS)
                    color = tiny.getpixel((0, 0))
                    self._color_cache[path] = rgb_to_hex(*color)
                    self._color_fetching.discard(path)
                except Exception:
                    pass
                
                def _done(i=img, p=path, s=size):
                    # Must be created in main thread
                    success = False
                    try:
                        pi = ImageTk.PhotoImage(i)
                        i.close()
                        self._store((p, s), pi, pi.width() * pi.height())
                        success = True
                    except Exception:
                        try:
                            i.close()
                        except Exception:
                            pass
                    finally:
                        callbacks = self._callbacks.pop((p, s), [])
                        self.fetching.discard((p, s))
                        if success:
                            for callback in callbacks:
                                try:
                                    callback(p)
                                except Exception:
                                    pass
                            try:
                                self.app._update_player_accent(p)
                            except Exception:
                                pass
                        self._drain()
                self.app.after(0, _done)
            else:
                self._release_key(key)
        except Exception:
            self._release_key(key)

    def _store(self, key, image, pixels):
        """Store the newest artwork while keeping memory use predictable."""
        old = self.cache.pop(key, None)
        if old:
            self._cache_pixels -= old[1]
        self.cache[key] = (image, pixels)
        self._cache_pixels += pixels
        while (len(self.cache) > self.MAX_ENTRIES or
               self._cache_pixels > self.MAX_PIXELS) and len(self.cache) > 1:
            _, (_, old_pixels) = self.cache.popitem(last=False)
            self._cache_pixels -= old_pixels

    def prune_paths(self, paths):
        """Release cached artwork for files no longer in the library."""
        allowed = {
            os.path.abspath(os.path.normpath(path))
            for path in paths
        }
        for key in list(self.cache):
            if os.path.abspath(os.path.normpath(key[0])) not in allowed:
                _, pixels = self.cache.pop(key)
                self._cache_pixels -= pixels
        for key in list(self._pending):
            if os.path.abspath(os.path.normpath(key[0])) not in allowed:
                self._pending.pop(key, None)
                self._callbacks.pop(key, None)
        for key in list(self._callbacks):
            if os.path.abspath(os.path.normpath(key[0])) not in allowed:
                self._callbacks.pop(key, None)
        self._color_cache = {
            path: color
            for path, color in self._color_cache.items()
            if os.path.abspath(os.path.normpath(path)) in allowed
        }

    def shutdown(self):
        self._pending.clear()
        self._callbacks.clear()
        self.executor.shutdown(wait=False, cancel_futures=True)

# ── Main App ──────────────────────────────────────────────────────────────
class LunoMusicPlayer(tk.Tk):
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

    def _show_add_to_playlist_dialog(self, paths, title_text, on_transfer=None,
                                    subtitle=None):
        """Show a searchable destination playlist picker.

        on_transfer(dest_dir, playlist_name) is called after a successful
        transfer instead of the default success messagebox (used by the song
        page's save flows so the destination is never silently Unsorted)."""
        playlists = sorted(self._library.keys(), key=str.casefold)
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
        tk.Label(dlg, text=subtitle or f"{len(paths)} tracks will be copied",
                 font=("Helvetica Neue", 10), 
                  fg=TEXT3, bg=BG2).pack(pady=(0, 15))

        search_frame = tk.Frame(dlg, bg=BG3, highlightbackground=BORDER, highlightthickness=1)
        search_frame.pack(fill="x", padx=30, pady=(0, 10), ipady=2)
        tk.Label(search_frame, text="⌕", font=("Helvetica Neue", 11),
                 fg=TEXT3, bg=BG3, padx=8).pack(side="left")
        search_var = tk.StringVar()
        search_entry = tk.Entry(
            search_frame, textvariable=search_var, bg=BG3, fg=TEXT,
            insertbackground=TEXT, relief="flat", font=("Helvetica Neue", 11),
            bd=0, highlightthickness=0,
        )
        search_entry.pack(side="left", fill="x", expand=True, padx=(0, 8))

        lb_frame = tk.Frame(dlg, bg=BG3, bd=1, highlightbackground=BORDER, highlightthickness=1)
        lb_frame.pack(fill="both", expand=True, padx=30, pady=10)

        lb = tk.Listbox(lb_frame, bg=BG3, fg=TEXT2, font=("Helvetica Neue", 12),
                         relief="flat", borderwidth=0, highlightthickness=0,
                         selectbackground=BG4, activestyle="none")
        from widgets import ModernScrollbar
        lb_sb = ModernScrollbar(lb_frame, command=lb.yview, bg=BG3, width=6)
        lb.configure(yscrollcommand=lb_sb.set)
        lb_sb.pack(side="right", fill="y", padx=(0, 4), pady=5)
        lb.pack(side="left", fill="both", expand=True, padx=5, pady=5)

        visible_playlists = []

        def populate_playlists(*_):
            query = search_var.get().strip().casefold()
            visible_playlists[:] = [
                playlist for playlist in playlists
                if not query or query in playlist.casefold()
            ]
            lb.delete(0, "end")
            for playlist in visible_playlists:
                lb.insert("end", f"  {playlist}")
            if visible_playlists:
                lb.selection_set(0)
                lb.activate(0)

        search_var.trace_add("write", populate_playlists)
        populate_playlists()

        def do_transfer():
            sel = lb.curselection()
            if not sel or sel[0] >= len(visible_playlists):
                return
            target = visible_playlists[sel[0]]
            dest_dir = self._find_playlist_dir(target)
            
            if not dest_dir: return
            
            count = 0
            for src in paths:
                try:
                    shutil.copy2(src, os.path.join(dest_dir, os.path.basename(src)))
                    count += 1
                except: pass
            
            dlg.destroy()
            if on_transfer is not None:
                on_transfer(dest_dir, target)
            else:
                self._refresh_library()
                messagebox.showinfo("Success", f"Copied {count} songs to '{target}'")

        # Styled Green Button (Matching the rest of the Luno UI)
        btn = tk.Label(dlg, text="Confirm Transfer", font=("Helvetica Neue", 11, "bold"),
                       fg=SIDEBAR, bg=GREEN, padx=30, pady=12, cursor="hand2")
        btn.pack(pady=25)
        btn.bind("<Button-1>", lambda e: do_transfer())
        btn.bind("<Enter>", lambda e: btn.config(bg=GREEN_HOVER))
        btn.bind("<Leave>", lambda e: btn.config(bg=GREEN))
        search_entry.bind("<Return>", lambda e: do_transfer())
        lb.bind("<Double-Button-1>", lambda e: do_transfer())
        dlg.after(0, search_entry.focus_set)

    # Now, find the existing _ctx_add_playlist and replace it with this one-liner:
    def _ctx_add_playlist(self):
        """Proxy to show the add to playlist dialog for selected paths."""
        paths = self._get_sel_paths()
        if paths:
            self._show_add_to_playlist_dialog(paths, f"Add {len(paths)} songs to...")

    @staticmethod
    def _track_filename(artist, title, extension):
        """Build the searchable filename used by the library's title parser."""
        def clean(value):
            value = re.sub(r'[\x00-\x1f\x7f<>:"/\\|?*]', " ", str(value or ""))
            return re.sub(r"\s+", " ", value).strip().rstrip(".")

        clean_artist = clean(artist)
        clean_title = clean(title)
        if not clean_title:
            return ""

        stem = f"{clean_artist} - {clean_title}" if clean_artist else clean_title
        # Keep the complete path comfortably below common filesystem limits.
        max_stem_length = max(1, 240 - len(extension))
        return f"{stem[:max_stem_length].rstrip(' .')}{extension}"

    def _ctx_edit_track_info(self):
        """Open the metadata editor for one selected local audio file."""
        paths = self._get_sel_paths()
        if len(paths) != 1:
            if paths:
                messagebox.showwarning(
                    "Edit Track Info",
                    "Select one song at a time to edit its artist and track name.",
                    parent=self,
                )
            return
        self._show_edit_track_info_dialog(paths[0])

    def _show_edit_track_info_dialog(self, path):
        """Collect artist/title values and rename the file for library search."""
        if not os.path.isfile(path):
            messagebox.showerror("Edit Track Info", "This song is no longer available.", parent=self)
            return

        current_title, current_artist = split_track_name(path)
        dlg = tk.Toplevel(self)
        dlg.title("Edit Track Info")
        dlg.configure(bg=BG2)
        dlg.geometry("440x315")
        dlg.resizable(False, False)
        dlg.transient(self)
        dlg.grab_set()

        tk.Label(
            dlg,
            text="Edit Track Info",
            font=("Helvetica Neue", 15, "bold"),
            fg=TEXT,
            bg=BG2,
        ).pack(pady=(24, 4))
        tk.Label(
            dlg,
            text="Names are used for library search and display.",
            font=("Helvetica Neue", 10),
            fg=TEXT3,
            bg=BG2,
        ).pack(pady=(0, 18))

        artist_var = tk.StringVar(value=current_artist)
        title_var = tk.StringVar(value=current_title)

        def add_field(label, variable):
            row = tk.Frame(dlg, bg=BG2)
            row.pack(fill="x", padx=36, pady=6)
            tk.Label(
                row,
                text=label,
                width=12,
                anchor="w",
                font=("Helvetica Neue", 10, "bold"),
                fg=TEXT2,
                bg=BG2,
            ).pack(side="left")
            entry = tk.Entry(
                row,
                textvariable=variable,
                bg=BG3,
                fg=TEXT,
                insertbackground=TEXT,
                relief="flat",
                font=("Helvetica Neue", 11),
                bd=0,
                highlightthickness=0,
            )
            entry.pack(side="left", fill="x", expand=True, ipady=7, padx=(8, 0))
            return entry

        artist_entry = add_field("Artist", artist_var)
        title_entry = add_field("Track name", title_var)

        def save():
            extension = os.path.splitext(path)[1]
            filename = self._track_filename(artist_var.get(), title_var.get(), extension)
            if not filename:
                messagebox.showwarning(
                    "Track name required",
                    "Enter a track name before saving.",
                    parent=dlg,
                )
                title_entry.focus_set()
                return

            old_path = os.path.abspath(os.path.normpath(path))
            new_path = os.path.abspath(os.path.join(os.path.dirname(old_path), filename))
            if new_path.casefold() == old_path.casefold():
                dlg.destroy()
                return
            if os.path.exists(new_path):
                messagebox.showerror(
                    "File already exists",
                    f"A song named '{filename}' already exists in this folder.",
                    parent=dlg,
                )
                return

            try:
                os.rename(old_path, new_path)
            except OSError as exc:
                messagebox.showerror("Could not update track", str(exc), parent=dlg)
                return

            dlg.destroy()
            self._replace_track_path_references(old_path, new_path)
            self._save_config()
            self._refresh_library(reset_track_scroll=True)
            self.status_var.set(f"Updated: {os.path.splitext(filename)[0]}")
            if not self._closing:
                self.after(2200, lambda: self.status_var.set("") if not self._closing else None)

        buttons = tk.Frame(dlg, bg=BG2)
        buttons.pack(fill="x", padx=36, pady=(22, 0))
        cancel = tk.Label(
            buttons,
            text="Cancel",
            font=("Helvetica Neue", 10, "bold"),
            fg=TEXT2,
            bg=BG4,
            padx=18,
            pady=9,
            cursor="hand2",
        )
        cancel.pack(side="right", padx=(8, 0))
        cancel.bind("<Button-1>", lambda _event: dlg.destroy())
        save_button = tk.Label(
            buttons,
            text="Save",
            font=("Helvetica Neue", 10, "bold"),
            fg=SIDEBAR,
            bg=GREEN,
            padx=24,
            pady=9,
            cursor="hand2",
        )
        save_button.pack(side="right")
        save_button.bind("<Button-1>", lambda _event: save())
        save_button.bind("<Enter>", lambda _event: save_button.config(bg=GREEN_HOVER))
        save_button.bind("<Leave>", lambda _event: save_button.config(bg=GREEN))

        dlg.bind("<Return>", lambda _event: save())
        dlg.bind("<Escape>", lambda _event: dlg.destroy())
        dlg.after(0, title_entry.focus_set)

    def _replace_track_path_references(self, old_path, new_path):
        """Keep live queues, history, and favorites valid after a file rename."""
        old_key = os.path.abspath(os.path.normpath(old_path)).casefold()

        def replace_in(paths):
            return [
                new_path
                if isinstance(item, str)
                and os.path.abspath(os.path.normpath(item)).casefold() == old_key
                else item
                for item in paths
            ]

        for owner, attr in (
            (self, "_recently_played"),
            (self, "_queue_playlist_paths"),
            (getattr(self, "engine", None), "queue"),
            (getattr(self, "engine", None), "_original_queue"),
        ):
            if owner is not None and hasattr(owner, attr):
                setattr(owner, attr, replace_in(getattr(owner, attr)))

        favorite_old = self._favorite_key(old_path)
        if favorite_old in self._favorite_paths:
            self._favorite_paths.remove(favorite_old)
            self._favorite_paths.add(self._favorite_key(new_path))

        engine = getattr(self, "engine", None)
        if engine and 0 <= engine.queue_idx < len(engine.queue):
            current_path = engine.queue[engine.queue_idx]
            title, artist = split_track_name(current_path)
            if "now_title" in self.__dict__:
                self.now_title.config(text=title[:48])
            if "now_artist" in self.__dict__:
                self.now_artist.config(text=artist)
            self._update_now_playing_info(title, artist, engine.duration)

        if "_frames" in self.__dict__:
            self._update_queue()
            recent = self._frames.get("recent")
            if recent:
                recent.redraw()

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
        self.title("Luno")
        
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
        self._library_scan_generation = 0
        self._library_scan_state_lock = threading.Lock()
        self._library_scan_active = False
        self._library_scan_pending = False
        self._library_scan_pending_reset = False
        self._dl_playlists         = []
        self._is_dl                = False
        self._discovery_downloads  = 0
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
        self._favorite_paths       = set()
        self._crossfade_secs       = 0
        self._sleep_after_id       = None
        self._sync_running         = False
        self._transfer_running     = False
        self._duplicate_scan_lock  = threading.Lock()
        self._duplicate_scan_active = False
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
        self._discovery_request_lock = threading.Lock()
        self._discovery_request_generation = 0
        self._discovery_request_active = False
        self._discovery_request_pending = None

        # Song-page "Recommended for this song" requests (independent of the
        # Discover-tab machinery so the two surfaces never cancel each other).
        self._song_page_recs_lock = threading.Lock()
        self._song_page_recs_generation = 0
        self._song_page_recs_active = False
        self._song_page_recs_pending = None

        # Discover mode (mobile RecommendationPreviewManager parity): prepares
        # temporary previews in PREVIEW_DIR (outside the library root) behind
        # the current song and restores the captured physical queue when
        # toggled off. Previews are wiped at app launch and at session end;
        # only explicit "Add to Library" promotions copy audio into the library.
        self._discover_active = False
        self._discover_seed = None
        self._discover_chain_running = False
        self._discover_chain_gen = 0
        self._discover_physical_queue = None
        self._discover_physical_idx = None
        self._discover_paths = []
        self._discover_recs = []
        self._discover_rec_pos = 0
        self._discover_states = OrderedDict()   # key -> {"status", "path", "error", "rec", "promoted", "source"}
        self._discover_retry_queue = []
        self._discover_insert_pos = 0
        self._discover_dest = None
        self._discover_downloading = False
        self._discover_deferred_delete = []

        # Mobile RecommendationPreviewManager session parity (2026-08-11):
        # a tap-to-preview session (non-Discover) plays the tapped
        # recommendation immediately with a rolling window of at most five
        # prepared ahead, while Discover keeps a deferred block behind the
        # seed. Save state mirrors the mobile rows: saving / queued /
        # permanent / saveFailure.
        self._discover_discover_mode = False
        self._discover_window_start = 0      # first rec index of the active window
        self._discover_window_end = 0        # exclusive end of the active window
        self._discover_first_play_pending = False
        self._discover_advance_armed = False
        self._discover_saving_keys = set()
        self._discover_saved_uris = {}       # key -> permanent library path
        self._discover_queued_keys = {}      # key -> destination playlist dir
        self._discover_save_failures = {}
        self._preview_resolver_pool = ThreadPoolExecutor(
            max_workers=2, thread_name_prefix="luno-preview-resolver")

        # Mobile startupCleanup parity: a previous crashed session may have
        # left staged preview audio behind — never let it accumulate.
        try:
            shutil.rmtree(self.PREVIEW_DIR, ignore_errors=True)
            os.makedirs(self.PREVIEW_DIR, exist_ok=True)
        except Exception:
            pass
        
        self.art_cache          = ArtCache(self)
        self._np_poll_counter = 0

        # 2. LOAD PERSISTENT CONFIG
        self._load_config()
        self._load_playlist_meta()
        
        # 3. CONFIGURE UI & ENGINE
        self._configure_styles()
        self._build_ui()
        self.engine = LunoEngine(self)
        
        # Restore volume from config (if loaded)
        if hasattr(self, "engine"):
            self.engine.volume = self.vol_var.get() / 100

        self._refresh_library()
        self.after(30000, self._init_watchdog)
        
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
        messagebox.showinfo("Luno", "Luno v4\nProfessional Music Player")

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
        s.configure("Dark.TCombobox",
                    fieldbackground=BG3, background=BG3, foreground=TEXT,
                    bordercolor=BG3, lightcolor=BG3, darkcolor=BG3,
                    arrowcolor=TEXT2)
        s.map("Dark.TCombobox",
              fieldbackground=[("readonly", BG3)],
              foreground=[("readonly", TEXT)])
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

        # Root PanedWindow for Sidebar | Main Content | Queue.
        # sashwidth=6 (bg-colored, invisible) keeps a grabbable drag area without
        # any visible splitter bars; the sash follows the cursor so the tray
        # opens the way you drag.
        self._root_paned = tk.PanedWindow(self, orient="horizontal", bg=BG,
                                           sashwidth=6, sashpad=0, bd=0, sashrelief="flat")
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
        self._root_paned.add(queue_col, width=270, minsize=240, stretch="never")
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
        tk.Label(logo_f, text="Luno", font=("Helvetica Neue",24,"bold"),
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
        self._library_header_btn = tk.Label(
            lib_hdr, text="Library", font=("Helvetica Neue",12,"bold"),
            fg=TEXT2, bg=SIDEBAR, cursor="hand2"
        )
        self._library_header_btn.pack(side="left", padx=8)
        self._library_header_btn.bind("<Button-1>", lambda e: self._select_playlist_card(-1, "All Music"))
        self._library_header_btn.bind("<Enter>", lambda e: self._library_header_btn.config(fg=TEXT))
        self._library_header_btn.bind("<Leave>", lambda e: self._sync_library_header_button())

        self._unsorted_tab = tk.Label(
            lib_hdr, text="Unsorted", font=("Helvetica Neue",9,"bold"),
            fg=TEXT3, bg=SIDEBAR, cursor="hand2"
        )
        self._unsorted_tab.pack(side="left", padx=(8, 0))
        self._unsorted_tab.bind("<Button-1>", lambda e: self._select_playlist_card(-1, "Unsorted"))
        self._unsorted_tab.bind("<Enter>", lambda e: self._unsorted_tab.config(fg=TEXT))
        self._unsorted_tab.bind("<Leave>", lambda e: self._sync_unsorted_tab())

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
        bind_scroll_events(
            self._pl_scroll_canvas,
            self._on_sidebar_wheel,
            add="+",
            bind_all=True,
        )

    # ── Main ──────────────────────────────────────────────────────────────────
    def _build_main(self, parent):
        parent.grid_rowconfigure(0, weight=1)
        parent.grid_columnconfigure(0, weight=1)
        self._frames = {}
        
        self._frames["library"]  = LibraryView(parent, self)
        self._frames["download"] = DownloadView(parent, self)
        self._frames["recent"]   = RecentView(parent, self)
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
        self._q_art_targets = {}  # path -> placeholder art labels (targeted update)
        
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
        bind_scroll_events(self._q_canvas, self._on_queue_wheel, add="+", bind_all=True)

    def _on_queue_wheel(self, event):
        """Clamped scrolling for the queue canvas. Prevents negative scrolling."""
        lo, hi = self._q_canvas.yview()
        # If content fits entirely, don't scroll
        if lo <= 0.0 and hi >= 1.0: return
        # Prevent scrolling past top (delta > 0 is up)
        amount = scroll_amount(event, wheel_units=2)
        if not amount:
            return
        if lo <= 0.001 and amount < 0: return
        # Prevent scrolling past bottom (delta < 0 is down)
        if hi >= 0.999 and amount > 0: return
        
        self._q_canvas.yview_scroll(amount, "units")

    def _set_queue_tab(self, tab):
        self._queue_tab = tab
        if tab == "queue":
            self._q_tab_btn.config(fg=TEXT, font=("Helvetica Neue",11,"bold"))
            self._r_tab_btn.config(fg=TEXT3, font=("Helvetica Neue",11))
        else:
            self._r_tab_btn.config(fg=TEXT, font=("Helvetica Neue",11,"bold"))
            self._q_tab_btn.config(fg=TEXT3, font=("Helvetica Neue",11))
        self._update_queue(scroll_top=True)

    def _update_queue(self, scroll_top=False):
        """Redraws the queue/recent list.

        The rebuild keeps the current scroll position: recreating every row
        widget while the user is scrolled (and snapping back to the top on
        every redraw) is what made the panel flash and jump while artwork
        loaded or tracks changed. Explicit tab switches pass scroll_top=True
        to land at the top.
        """
        try:
            y = self._q_canvas.yview()[0]
        except tk.TclError:
            y = 0.0
        targets = getattr(self, "_q_art_targets", None)
        if targets is not None:
            targets.clear()
        for w in self._q_inner.winfo_children(): w.destroy()
        
        if self._queue_tab == "queue":
            self._draw_queue_items()
        else:
            self._draw_recent_items()
            
        self._q_inner.update_idletasks()
        h = self._q_inner.winfo_reqheight()
        w = self._q_canvas.winfo_width()
        self._q_canvas.config(scrollregion=(0, 0, w, h))
        self._q_canvas.yview_moveto(0.0 if scroll_top else min(y, 1.0))
        
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
            m.add_command(
                label="★ Remove from Favorites" if self._is_favorite(path) else "☆ Add to Favorites",
                command=lambda: self._toggle_favorite_paths([path]),
            )
            if idx is not None:
                m.add_command(label="✕ Remove from Queue", command=lambda: self._ctx_remove_from_queue_idx(idx))
            m.post(e.x_root, e.y_root)

        f.bind("<Button-1>", lambda e: play_from_q())
        f.bind("<Button-2>", show_q_context)
        f.bind("<Button-3>", show_q_context)
        f.bind("<Enter>", lambda e: f.config(bg=BG3))
        f.bind("<Leave>", lambda e: f.config(bg=BG2))

        def _on_q_art_loaded(p):
            # Targeted update: swap the placeholder label for the loaded image
            # instead of rebuilding (and scroll-jumping) the whole panel.
            img = self.art_cache.get(p, 36, None)
            if not img:
                return
            targets = self._q_art_targets.get(p)
            if not targets:
                return
            alive = []
            for lbl in targets:
                try:
                    if lbl.winfo_exists():
                        lbl.config(image=img, text="", bg=BG3, width=36, height=36)
                        alive.append(lbl)
                except tk.TclError:
                    pass
            if alive:
                self._q_art_targets[p] = alive
            else:
                self._q_art_targets.pop(p, None)

        img = self.art_cache.get(path, 36, _on_q_art_loaded)
        if img:
            art = tk.Label(f, image=img, bg=BG3, width=36, height=36)
        else:
            art = tk.Label(f, text="♫", font=("Helvetica Neue",12),
                           fg=(GREEN if is_playing else TEXT3), bg=BG3, width=4, height=2)
            self._q_art_targets.setdefault(path, []).append(art)
        art.pack(side="left", padx=(16,12), pady=6)
        art.bind("<Button-1>", lambda e: play_from_q())

        txt = tk.Frame(f, bg=BG2)
        txt.pack(side="left", fill="both", expand=True)
        txt.bind("<Button-1>", lambda e: play_from_q())

        queue_width = self._q_canvas.winfo_width() or 270
        text_width = max(88, queue_width - 104)
        text_limit = max(14, int(text_width / 7))

        def q_truncate(s, limit):
            if len(s) > limit: return s[:limit-3] + "..."
            return s

        t_lbl = tk.Label(txt, text=q_truncate(title, min(26, text_limit)), font=("Helvetica Neue",10, "bold" if is_playing else "normal"),
                         fg=(GREEN if is_playing else TEXT), bg=BG2, anchor="w")
        t_lbl.pack(fill="x")
        t_lbl.bind("<Button-1>", lambda e: play_from_q())
        
        a_lbl = tk.Label(txt, text=q_truncate(artist, min(32, text_limit)), font=("Helvetica Neue",9),
                         fg=TEXT3, bg=BG2, anchor="w")
        a_lbl.pack(fill="x")
        a_lbl.bind("<Button-1>", lambda e: play_from_q())

        fav_lbl = tk.Label(
            f,
            text="★" if self._is_favorite(path) else "☆",
            font=("Helvetica Neue", 12),
            fg=GREEN if self._is_favorite(path) else TEXT3,
            bg=BG2,
            cursor="hand2",
        )
        fav_lbl.pack(side="right", padx=(4, 12))
        fav_lbl.bind("<Button-1>", lambda e: self._toggle_favorite_paths([path]))

        # Consolidated hover consistency (Now that all widgets exist)
        def _set_row_style(bg, fg1, fg2):
            f.config(bg=bg); txt.config(bg=bg)
            fav_lbl.config(bg=bg)
            if t_lbl.winfo_exists(): t_lbl.config(bg=bg, fg=GREEN if is_playing else fg1)
            if a_lbl.winfo_exists(): a_lbl.config(bg=bg, fg=fg2)

        for w in [f, txt, t_lbl, a_lbl, art, fav_lbl]:
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

        self.favorite_lbl = tk.Label(info, text="☆", font=("Helvetica Neue",14),
                                     fg=TEXT3, bg=PLAYER_BG, cursor="hand2", width=2)
        self.favorite_lbl.grid(row=0, column=2, padx=(8, 16), sticky="e")
        self.favorite_lbl.bind("<Button-1>", lambda e: self._toggle_current_favorite())
        self.favorite_lbl.bind("<Enter>", lambda e: self.favorite_lbl.config(fg=TEXT))
        self.favorite_lbl.bind("<Leave>", lambda e: self._update_favorite_ui())

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
        icon_lbl("⏮", lambda: self._user_prev(), 17, TEXT2).pack(side="left")

        # Round play button
        self.play_btn = RoundButton(btn_row, 36, lambda: self.engine.toggle_play(), bg=PLAYER_BG)
        self.play_btn.pack(side="left", padx=8)

        icon_lbl("⏭", lambda: self._user_next(), 17, TEXT2).pack(side="left")
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
            self._update_favorite_ui()
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
        self._update_favorite_ui()

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
        amount = scroll_amount(event, wheel_units=2)
        if amount:
            self._pl_scroll_canvas.yview_scroll(amount, "units")

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
    def _favorite_key(self, path):
        """Return the stable local-file key used for favorite persistence."""
        return os.path.abspath(os.path.normpath(path)) if path else ""

    def _favorite_paths_in_library(self):
        """Return favorite files in library order, without playlist duplicates."""
        favorites = []
        seen = set()
        for tracks in self._library.values():
            for path in tracks:
                key = self._favorite_key(path)
                if key in self._favorite_paths and key not in seen:
                    favorites.append(path)
                    seen.add(key)
        return favorites

    def _is_favorite(self, path):
        return self._favorite_key(path) in self._favorite_paths

    def _prepare_favorite_menu(self, menu, menu_index):
        """Keep track context-menu wording in sync with the current selection."""
        paths = self._get_sel_paths()
        if not paths:
            return
        all_favorite = all(self._is_favorite(path) for path in paths)
        menu.entryconfig(
            menu_index,
            label="  ★   Remove from Favorites" if all_favorite else "  ☆   Add to Favorites",
        )

    def _toggle_current_favorite(self):
        if not hasattr(self, "engine") or not self.engine.queue:
            return
        idx = self.engine.queue_idx
        if 0 <= idx < len(self.engine.queue):
            self._toggle_favorite_paths([self.engine.queue[idx]])

    def _ctx_toggle_favorite(self):
        self._toggle_favorite_paths(self._get_sel_paths())

    def _toggle_favorite_paths(self, paths):
        keys = [self._favorite_key(path) for path in paths if path]
        keys = list(dict.fromkeys(keys))
        if not keys:
            return

        remove = all(key in self._favorite_paths for key in keys)
        if remove:
            self._favorite_paths.difference_update(keys)
            message = "Removed from Favorites"
        else:
            self._favorite_paths.update(keys)
            message = "Added to Favorites"
        self._save_config()
        self.status_var.set(f"{message} · {len(keys)} song(s)")
        if not self._closing:
            self.after(1800, lambda: self.status_var.set("") if not self._closing else None)
        self._refresh_favorite_views()

    def _refresh_favorite_views(self):
        """Refresh favorite-derived UI without rescanning the filesystem."""
        if not hasattr(self, "_frames"):
            return
        self._populate_sidebar()
        library = self._frames.get("library")
        if library:
            selected_paths = set(library.get_sel_paths())
            track_filter = library._pl_search_entry.get().strip()
            if track_filter == library._pl_search_ph:
                track_filter = ""
            library.populate_track_list(track_filter)
            library._track_sel_idx = [
                index for index, item in enumerate(library._track_items)
                if item[0] in selected_paths
            ]
            library._sel_anchor = library._track_sel_idx[0] if library._track_sel_idx else None
            library._redraw_track_list()
            if self._selected_playlist == FAVORITES_PLAYLIST:
                count = len(self._favorite_paths_in_library())
                library.playlist_meta_lbl.config(text=f"{count} songs")
                library.playlist_subtitle_lbl.config(text="")
                library._current_art.update_art(
                    FAVORITES_PLAYLIST, library._get_current_playlist_paths()
                )
                library._draw_lib_header()
        recent = self._frames.get("recent")
        if recent:
            recent.redraw()
        self._update_queue()
        song_page = self._frames.get("song_page")
        if song_page and song_page.winfo_ismapped():
            song_page.update_view()
        self._update_favorite_ui()

    def _update_favorite_ui(self):
        if not hasattr(self, "favorite_lbl"):
            return
        path = ""
        if hasattr(self, "engine") and self.engine.queue:
            idx = self.engine.queue_idx
            if 0 <= idx < len(self.engine.queue):
                path = self.engine.queue[idx]
        self.favorite_lbl.config(
            text="★" if self._is_favorite(path) else "☆",
            fg=GREEN if path and self._is_favorite(path) else TEXT3,
            cursor="hand2" if path else "arrow",
        )

    def _log_error(self, area, msg):
        """Global error logger."""
        timestamp = datetime.now().strftime("%H:%M:%S")
        entry = f"[{timestamp}] [{area.upper()}] {msg}"
        self._error_logs.append(entry)
        if len(self._error_logs) > 500: self._error_logs.pop(0)
        print(entry) # Also keep in terminal

    def _on_nav_configure(self, event):
        # Adaptive UI: 220px keeps the five remaining nav items expanded while
        # still allowing the playlist pane to reclaim the removed Artists space.
        if event.height < 220:
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
            # 235px accommodates all 5 nav items with a small buffer.
            pos = 235
            self._side_paned.sash_place(0, 0, pos)
        except:
            pass

    def _show_nav_menu(self, event):
        menu = tk.Menu(self, tearoff=0, bg=BG3, fg=TEXT, font=("Helvetica Neue", 11))
        for icon, label, cmd in [
            ("⌂","Home",        lambda: self._show_tab("library")),
            ("🕐","Recent",      lambda: self._show_tab("recent")),
            ("✨","Discover",    lambda: self._show_tab("discover")),
            ("⬇","Downloader", lambda: self._show_tab("download")),
            ("⚙","Settings",    lambda: self._show_tab("settings")),
        ]:
            menu.add_command(label=f"  {icon}  {label}", command=cmd)
        menu.post(event.x_root, event.y_root)

    # ── Discovery Methods ─────────────────────────────────────────────────────
    def _on_track_changed_discovery(self, path):
        """Called by the engine when a track starts playing."""
        self._discover_deferred_flush(path)
        self._discover_consider_handoff()
        self._discover_refill()
        self._discover_maybe_auto_extend()
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

            request = (final_artist, title)
            with self._discovery_request_lock:
                self._discovery_request_generation += 1
                generation = self._discovery_request_generation
                if self._discovery_request_active:
                    self._discovery_request_pending = request
                    return
                self._discovery_request_active = True
            self._start_discovery_request(request, generation)

    def _start_discovery_request(self, request, generation):
        def run():
            try:
                recs = self.discovery.get_similar(*request)
            except Exception as exc:
                self._log_error("Discovery", f"Request failed: {exc}")
                recs = []

            with self._discovery_request_lock:
                self._discovery_request_active = False
                pending = self._discovery_request_pending
                self._discovery_request_pending = None
                next_generation = self._discovery_request_generation
                if pending and not self._closing:
                    self._discovery_request_active = True

            if (not self._closing and
                    generation == self._discovery_request_generation):
                try:
                    self.after(0, lambda: self._update_discovery_results(recs))
                except (RuntimeError, tk.TclError):
                    pass
            if pending and not self._closing:
                self._start_discovery_request(pending, next_generation)

        threading.Thread(
            target=run,
            daemon=True,
            name="luno-discovery",
        ).start()

    def _filter_library_recs(self, recs):
        """Drop owned tracks and duplicate identities from a recommendation list.

        Matches the mobile DiscoveryRepository ownership filter: normalized
        case/whitespace artist-title keys cross-referenced against the library.
        """
        lib_keys = set()
        for tracks in self._library.values():
            for path in tracks:
                t, a = split_track_name(path)
                if t and a:
                    lib_keys.add(f"{a.lower().strip()} - {t.lower().strip()}")

        filtered = []
        seen = set()
        for r in recs:
            r_art = r.get("artist", "").lower().strip()
            r_ttl = r.get("title", "").lower().strip()
            r_key = f"{r_art} - {r_ttl}"
            if r_art and r_ttl and r_key not in lib_keys and r_key not in seen:
                seen.add(r_key)
                filtered.append(r)
        return filtered

    def _update_discovery_results(self, recs):
        if not recs:
            if "discover" in self._frames:
                self._frames["discover"].render_recommendations([])
            return

        filtered = self._filter_library_recs(recs)
        if "discover" in self._frames:
            self._frames["discover"].render_recommendations(filtered)

    def _fetch_song_page_recs(self, artist, title, limit, on_result):
        """Single-flight Last.fm fetch for the song page recommendation section.

        Mirrors the Discover-tab request bound: one active daemon worker plus
        one latest pending request; rapid seed changes replace stale work
        instead of spawning an unbounded thread backlog. `on_result(recs,
        error)` runs on the UI thread only if this request is still the latest.
        """
        request = (artist, title, limit, on_result)
        with self._song_page_recs_lock:
            if self._closing:
                return
            self._song_page_recs_generation += 1
            generation = self._song_page_recs_generation
            if self._song_page_recs_active:
                self._song_page_recs_pending = request
                return
            self._song_page_recs_active = True
        self._start_song_page_recs_request(request, generation)

    def _start_song_page_recs_request(self, request, generation):
        def run():
            artist, title, limit, on_result = request
            error = None
            try:
                if hasattr(self, "discovery"):
                    self.discovery.api_key = self.lastfm_key_var.get().strip()
                recs = self.discovery.get_similar(artist, title, limit=limit)
            except Exception as exc:
                self._log_error("Discovery", f"Request failed: {exc}")
                recs, error = [], str(exc)
            recs = self._filter_library_recs(recs)

            with self._song_page_recs_lock:
                self._song_page_recs_active = False
                pending = self._song_page_recs_pending
                self._song_page_recs_pending = None
                next_generation = self._song_page_recs_generation
                if pending and not self._closing:
                    self._song_page_recs_active = True

            if (not self._closing and
                    generation == self._song_page_recs_generation and
                    on_result is not None):
                try:
                    self.after(0, lambda: on_result(recs, error))
                except (RuntimeError, tk.TclError):
                    pass
            if pending and not self._closing:
                self._start_song_page_recs_request(pending, next_generation)

        threading.Thread(
            target=run,
            daemon=True,
            name="luno-song-page-recs",
        ).start()

    # ── Discover Mode (mobile RecommendationPreviewManager parity) ───────────
    # 2026-08-11 full port: recommendations are prepared into PREVIEW_DIR — a
    # temporary staging area OUTSIDE the library root (the Android equivalent
    # of cacheDir/PREVIEW_DOWNLOAD_DIR) — so library scans and the Unsorted
    # view never see them. Previews are wiped at app launch and at session
    # end; only explicit "Add to Library" promotions copy audio into the real
    # download destination. Per-row states mirror the mobile
    # FullPlayerRecommendationRow machine: waiting -> preparing (spinner) ->
    # ready (green check) / failed (retry). The chain prepares the first
    # DISCOVER_IMMEDIATE_COUNT recommendations passively (mobile
    # prefetchRecommendations), and Discover mode adopts the ready block into
    # the queue behind the current track, autoplaying when the seed ends, with
    # a rolling window of at most five prepared ahead of playback
    # (IMMEDIATE_PREVIEW_AHEAD_COUNT). Playback leaving the block (or a
    # replaced queue) ends the session (mobile natural-handoff parity).
    DISCOVER_IMMEDIATE_COUNT = 5
    PREVIEW_EXTS = (".mp3", ".m4a", ".wav", ".flac", ".webm")
    # Overridable staging location (tests point this at a temp dir).
    PREVIEW_DIR = PREVIEW_DIR

    @staticmethod
    def _discover_key(rec):
        return f"{rec.get('artist', '').lower().strip()} - {rec.get('title', '').lower().strip()}"

    # ── Chain lifecycle (prefetch + rolling window) ─────────────────────────

    def _discover_chain_start(self):
        """Hold the watchdog quiet while the preview chain owns downloads."""
        if self._discover_chain_running or self._closing:
            return
        self._discover_chain_running = True
        self._discovery_downloads += 1

    def _discover_chain_stop(self):
        if not self._discover_chain_running:
            return
        self._discover_chain_running = False
        self._discovery_downloads = max(0, self._discovery_downloads - 1)

    def _discover_notify_ui(self):
        """Bump the UI version and ask the song page to re-render its rows."""
        try:
            sp = getattr(self, "_frames", {}).get("song_page")
            if sp is not None and hasattr(sp, "_on_discover_ui_changed"):
                self.after(0, sp._on_discover_ui_changed)
        except (RuntimeError, tk.TclError):
            pass

    def _discover_prefetch(self, seed, recs):
        """Mobile prefetchRecommendations parity: when the song page delivers
        a fresh recommendation list, passively prepare the first five into
        the preview staging directory without touching playback. A new seed
        (or a refresh) cancels any in-flight preview download and restarts."""
        if self._closing or seed is None or not recs:
            return
        if self._discover_active:
            if (self._discover_seed is not None and
                    tuple(seed) != self._discover_seed):
                # A different song's recommendations arrived while a session
                # was active: the old seed's session is over (mobile
                # natural-handoff / playback-intervention parity).
                self._discover_end_session(ended=True)
            else:
                # The session owns this seed — never reset the chain
                # mid-session (mobile: prefetch returns while active).
                return
        self._discover_chain_gen += 1
        self._discover_chain_stop()
        old_deferred = self._discover_deferred_delete or []
        self._discover_deferred_delete = None
        for p in old_deferred:
            try:
                os.remove(p)
            except OSError:
                pass
        self._discover_seed = tuple(seed)
        self._discover_recs = list(recs)
        self._discover_rec_pos = 0
        self._discover_window_start = 0
        self._discover_window_end = min(
            self.DISCOVER_IMMEDIATE_COUNT, len(recs))
        self._discover_states = OrderedDict()
        self._discover_retry_queue = []
        self._discover_downloading = False
        self._discover_deferred_delete = []
        self._discover_dest = self.PREVIEW_DIR
        self._discover_chain_start()
        self._discover_refill()

    def _discover_refill(self):
        """Mobile rolling-window parity: keep DISCOVER_IMMEDIATE_COUNT ready
        ahead of playback (or simply the first five while passively
        prefetching); never floods the whole list. Only rows inside the
        active window count toward the ready target, so prefetched rows
        outside a tap-to-preview window cannot stall its preparation."""
        if self._closing or self._discover_downloading:
            return
        if self._discover_retry_queue:
            # A user-initiated retry always wins over the rolling window.
            self._discover_chain_start()
            self._discover_prepare_next()
            return
        if self._discover_rec_pos >= self._discover_window_end:
            if self._discover_chain_running:
                self._discover_chain_stop()
                if self._discover_active and self._discover_discover_mode:
                    self.status_var.set("Discover mode on — all recommendations downloaded")
            self._discover_notify_ui()
            return
        window_start = self._discover_window_start
        inserted = set(self._discover_paths)
        if window_start:
            idx_map = {self._discover_key(r): i
                       for i, r in enumerate(self._discover_recs)}
            ready = sum(1 for k, s in self._discover_states.items()
                        if s["status"] in ("ready", "saved")
                        and s.get("path") in inserted
                        and idx_map.get(k, -1) >= window_start)
        else:
            ready = sum(1 for s in self._discover_states.values()
                        if s["status"] in ("ready", "saved")
                        and s.get("path") in inserted)
        if self._discover_active:
            target = self._discover_consumed() + self.DISCOVER_IMMEDIATE_COUNT
        else:
            target = self.DISCOVER_IMMEDIATE_COUNT
        if ready >= target:
            if not self._discover_active and self._discover_chain_running:
                # Passive window full — release the watchdog until needed.
                self._discover_chain_stop()
                self._discover_notify_ui()
            # Mobile waitingKeys parity: rows queued behind the active
            # preparation show "Queued for temporary preview" while the
            # window gate holds the chain.
            self._discover_mark_waiting_band()
            return
        self._discover_chain_start()
        self._discover_prepare_next()

    def _discover_append_recs(self, recs):
        """Mobile appendRecommendations parity: 'Load 25 more' grows the
        chain while keeping already-prepared states."""
        if self._closing or not recs:
            return
        known = {self._discover_key(r) for r in self._discover_recs}
        additions = [r for r in recs if self._discover_key(r) not in known]
        if not additions:
            return
        self._discover_recs.extend(additions)
        if self._discover_discover_mode:
            self._discover_window_end = len(self._discover_recs)
        self._discover_refill()
        self._discover_notify_ui()

    def _discover_retry(self, key):
        """Per-row retry for a failed preview download."""
        if self._closing:
            return
        st = self._discover_states.get(key)
        if not st or st["status"] != "failed":
            return
        st["status"] = "waiting"
        st["error"] = None
        st["path"] = None
        self._discover_retry_queue.append(st["rec"])
        self._discover_chain_start()
        self._discover_refill()
        self._discover_notify_ui()

    def _discover_existing_download(self, artist, title):
        safe_name = re.sub(r'[<>:"/\\|?*]', '', f"{artist} - {title}").strip() or "Unknown"
        for ext in self.PREVIEW_EXTS:
            candidate = os.path.join(self._discover_dest, f"{safe_name}{ext}")
            if os.path.isfile(candidate):
                return candidate
        return None

    def _discover_mark_waiting_band(self):
        """Mobile waitingKeys/prefetchQueuedKeys parity: rows inside the
        active window that are not yet prepared show 'Queued for temporary
        preview'. While passively prefetching the whole first-five window is
        marked; inside a session only the immediate successor waits (the
        rest of a Discover list keeps its default status)."""
        if self._closing:
            return
        if self._discover_active:
            end = self._discover_rec_pos + 1
        else:
            end = self._discover_window_end
        changed = False
        for i in range(self._discover_rec_pos, min(end, self._discover_window_end)):
            rec = self._discover_recs[i]
            k = self._discover_key(rec)
            if k not in self._discover_states:
                self._discover_states[k] = {
                    "status": "waiting", "path": None, "error": None,
                    "rec": rec, "promoted": False, "source": None,
                }
                changed = True
        if changed:
            self._discover_notify_ui()

    def _discover_prepare_next(self):
        """Prepare exactly one recommendation: adopt a staged asset, or
        resolve a playable source (15s deadline) then download it (5 min
        deadline). One recommendation at a time (mobile sequential
        preparation loop) — the preview pipeline never touches the library
        Downloader's single job slot."""
        if self._closing or not self._discover_chain_running:
            return
        if self._discover_retry_queue:
            track = self._discover_retry_queue.pop(0)
        elif self._discover_rec_pos < self._discover_window_end:
            track = self._discover_recs[self._discover_rec_pos]
            self._discover_rec_pos += 1
        else:
            return
        key = self._discover_key(track)
        artist = track.get("artist", "")
        title = track.get("title", "")
        self._discover_mark_waiting_band()

        existing = self._discover_existing_download(artist, title)
        if existing:
            st = self._discover_states.get(key)
            if st is None:
                st = {
                    "status": "waiting", "path": None, "error": None,
                    "rec": track, "promoted": False, "source": None,
                }
                self._discover_states[key] = st
            st["status"] = "ready"
            st["path"] = existing
            st["error"] = None
            st["promoted"] = st.get("promoted", False)
            self._discover_downloading = False
            self._discover_track_ready(key, st, existing)
            return

        st = self._discover_states.get(key)
        if st is None:
            st = {
                "status": "resolving", "path": None, "error": None,
                "rec": track, "promoted": False, "source": None,
            }
            self._discover_states[key] = st
        else:
            st["status"] = "resolving"
            st["path"] = None
            st["error"] = None
            st["source"] = None
        self._discover_notify_ui()
        self._discover_downloading = True
        gen = self._discover_chain_gen
        self._discover_start_resolve(key, st, gen)

    def _discover_fail_preview(self, key, st, message):
        """Mark a preview row failed (retryable). A tap-to-preview session
        whose FIRST item cannot be prepared ends without hijacking playback —
        the seed keeps playing and the session is cleaned up (mobile
        finishEmptyPreviewSession parity); failures inside a running session
        or Discover skip onward."""
        st["status"] = "failed"
        st["error"] = message
        st["path"] = None
        self._discover_downloading = False
        if (self._discover_active and not self._discover_discover_mode
                and self._discover_first_play_pending):
            self._discover_active = False
            self._discover_first_play_pending = False
            self._discover_chain_gen += 1
            self._discover_teardown()
            self._discover_notify_ui()
            return
        self._discover_notify_ui()
        self._discover_refill()

    def _discover_start_resolve(self, key, st, gen):
        """Mobile resolveRecommendation parity: YouTube search on the
        2-thread luno-preview-resolver pool with a 15s hard deadline. The
        first completion wins; a late resolve can never start a download."""
        artist = st["rec"].get("artist", "")
        title = st["rec"].get("title", "")
        settled = {"done": False}

        def on_result(src, err):
            if self._closing:
                return
            if settled["done"]:
                return
            settled["done"] = True
            if gen != self._discover_chain_gen:
                self._discover_downloading = False
                self._discover_refill()
                return
            if self._discover_states.get(key) is not st:
                self._discover_downloading = False
                self._discover_refill()
                return
            if err is not None:
                self._discover_fail_preview(key, st, str(err))
                return
            st["source"] = src
            st["status"] = "preparing"
            self._discover_notify_ui()
            self._discover_start_preview_download(key, st, gen)

        def run():
            try:
                src = previews.resolve_preview_source(artist, title)
                err = None
            except Exception as exc:
                src, err = None, exc
            try:
                self.after(0, lambda: on_result(src, err))
            except (RuntimeError, tk.TclError):
                pass

        self._preview_resolver_pool.submit(run)

        def _timeout():
            try:
                self.after(0, lambda: on_result(
                    None,
                    previews.PreviewTimeoutError(
                        previews.PREVIEW_RESOLVE_TIMED_OUT_MESSAGE)))
            except (RuntimeError, tk.TclError):
                pass

        threading.Timer(previews.PREVIEW_RESOLVE_TIMEOUT_S, _timeout).start()

    def _discover_start_preview_download(self, key, st, gen):
        """Mobile downloadPreview parity: stream the resolved source to
        PREVIEW_DIR on a worker thread with a cooperative 5-minute deadline
        and the chain's cancellation hook."""
        artist = st["rec"].get("artist", "")
        title = st["rec"].get("title", "")
        src = st.get("source") or {}
        url = src.get("url") or ""
        if not url:
            st["status"] = "failed"
            st["error"] = "Could not prepare this song"
            self._discover_downloading = False
            self._discover_notify_ui()
            self._discover_refill()
            return

        def run():
            try:
                path = previews.download_preview(
                    artist, title, url, self._discover_dest,
                    cancel_check=lambda: (
                        self._discover_chain_gen != gen or self._closing),
                    error_callback=lambda msg: None,
                )
                err = None
            except Exception as exc:
                path, err = None, exc
            try:
                self.after(0, lambda: self._discover_download_done(
                    key, st, gen, path, err))
            except (RuntimeError, tk.TclError):
                pass

        threading.Thread(target=run, daemon=True,
                         name="luno-preview-download").start()

    def _discover_download_done(self, key, st, gen, path, err):
        """UI thread: a preview download finished (or failed/timeout)."""
        if self._closing:
            return
        if gen != self._discover_chain_gen or self._discover_states.get(key) is not st:
            # The session was reset while this download was in flight — drop
            # any partial staged file (unless a newer session already owns
            # the same key, in which case its own chain manages it).
            if path and self._discover_states.get(key) is not st:
                try:
                    os.remove(path)
                except OSError:
                    pass
            self._discover_downloading = False
            self._discover_refill()
            return
        self._discover_downloading = False
        if err is not None:
            self._discover_fail_preview(key, st, str(err))
            return
        self._discover_track_ready(key, st, path)

    def _discover_track_ready(self, key, st, path):
        """A recommendation is prepared: adopt it into the active session —
        the first non-Discover item jumps into playback immediately (mobile
        playSequential), Discover items join the deferred block — or just
        mark it ready for the passive prefetch."""
        st["status"] = "ready"
        st["path"] = path
        st["error"] = None
        if self._discover_active:
            if self._discover_discover_mode:
                self._discover_consider_handoff()
                if not self._discover_active:
                    st["status"] = "saved"
                    self._discover_notify_ui()
                    self._discover_refill()
                    return
                self._discover_insert_path(path, st["rec"].get("title", ""))
                if (self._discover_advance_armed and
                        not self._discover_consumed()):
                    # The user pressed Next while the deferred block was
                    # still being prepared — jump into it now.
                    self._discover_advance_armed = False
                    self._discover_jump_into_preview(path)
            else:
                self._discover_insert_path(path, st["rec"].get("title", ""))
                if self._discover_first_play_pending:
                    self._discover_first_play_pending = False
                    self._discover_jump_into_preview(path)
        self._discover_notify_ui()
        self._discover_refill()

    # ── Discover mode toggle / preview sessions ─────────────────────────────

    def _discover_set(self, enabled, recs=None):
        """Toggle Discover mode on/off.

        ON: captures the physical queue, adopts every already-prepared
        (staged) recommendation into the queue behind the current track and
        keeps preparing the rolling window of five into PREVIEW_DIR. The
        block autoplays when the current song ends (engine.next_track walks
        it naturally, Next/Previous route through it).
        OFF: cancels pending preview downloads, deletes the staged preview
        audio and restores the captured physical queue — a playing
        recommendation is allowed to finish, then playback continues at the
        seed's successor (mobile "resume at the seed's successor" parity). A
        user-initiated new queue, or playback that has already walked past
        the block, ends the session without restoring. Nothing ever lands in
        the library unless the user explicitly promotes a row.
        """
        if enabled:
            if self._discover_active or self._closing:
                return
            if not (hasattr(self, "engine") and
                    (self.engine.playing or self.engine.paused) and self.engine.queue):
                self.status_var.set("Discover mode needs a playing track")
                return
            if not recs:
                self.status_var.set("No recommendations to play yet")
                return
            if list(recs) != self._discover_recs:
                self._discover_prefetch(tuple(self._rec_seed_for(recs)), recs)
            self._discover_start_session(recs, 0, discover_mode=True)
            self._discover_adopt_ready()
            self.status_var.set(
                f"Discover mode on — {len(self._discover_paths)} ready song(s) "
                f"queued behind this one")
            self._discover_notify_ui()
            self._discover_refill()
        else:
            if not self._discover_active:
                return
            self._discover_active = False
            self._discover_discover_mode = False
            self._discover_downloading = False
            self._discover_chain_gen += 1  # cancels in-flight preview downloads
            self._discover_teardown()

    def _discover_start_session(self, recs, start_index, discover_mode):
        """Mobile startPreview parity: begin a temporary preview session.

        discover_mode=False (row tap): the tapped recommendation plays
        immediately once prepared (adopting any cached asset) — mobile
        playSequential — with the next five prepared ahead (rolling window).
        discover_mode=True (Discover): the block is inserted behind the
        current song and starts only when the seed finishes naturally
        (deferred handoff). A session captures the physical queue so it can
        be restored on toggle-off / handoff.
        """
        if self._closing or not recs:
            return
        if not (hasattr(self, "engine") and self.engine.queue and
                0 <= self.engine.queue_idx < len(self.engine.queue)):
            self.status_var.set("A song must be playing to preview recommendations")
            return
        first_index = max(0, min(start_index, len(recs) - 1))
        if not os.path.isdir(self.PREVIEW_DIR):
            try:
                os.makedirs(self.PREVIEW_DIR, exist_ok=True)
            except OSError as exc:
                self._log_error("Discover", f"Could not create preview folder: {exc}")
                self.status_var.set("Preview failed — preview folder unavailable")
                return
        self._discover_dest = self.PREVIEW_DIR
        self._discover_chain_gen += 1
        self._discover_chain_stop()
        self._discover_active = True
        self._discover_discover_mode = discover_mode
        self._discover_physical_queue = list(self.engine.queue)
        self._discover_physical_idx = self.engine.queue_idx
        self._discover_paths = []
        if discover_mode:
            # Discover adoption resumes the passive prefetch's progress
            # (mobile: the session loop adopts prefetched assets and keeps
            # preparing from where the prefetch left off), so already-staged
            # rows are never re-walked or re-inserted.
            self._discover_rec_pos = max(
                start_index, self._discover_rec_pos)
        else:
            self._discover_rec_pos = start_index
        self._discover_window_start = first_index
        if discover_mode:
            self._discover_window_end = len(recs)
        else:
            self._discover_window_end = min(
                first_index + 1 + self.DISCOVER_IMMEDIATE_COUNT, len(recs))
        # Retain prepared states whose files are still valid and fall inside
        # the new session's window (mobile retainPreparedRecommendationKeys);
        # rows outside the window revert to untouched and their stale files
        # are cleaned up below.
        window_keys = {
            self._discover_key(r) for r in recs[self._discover_window_start:]
        }
        kept = OrderedDict()
        for k, s in self._discover_states.items():
            if (k in window_keys and s.get("path")
                    and os.path.isfile(s["path"])
                    and s["status"] in ("ready", "saved")):
                kept[k] = s
        stale_files = [
            s.get("path") for k, s in self._discover_states.items()
            if k not in kept and s.get("path")
        ]
        self._discover_states = kept
        self._discover_retry_queue = []
        self._discover_downloading = False
        self._discover_insert_pos = self.engine.queue_idx + 1
        self._discover_deferred_delete = []
        self._discover_first_play_pending = not discover_mode
        self._discover_advance_armed = False
        self._discover_delete_files(stale_files)
        # Callers drive the chain (Discover: adopt_ready first, then refill;
        # row taps: refill straight away) so adopted rows are never prepared
        # twice. The chain's first prepare adopts a staged tapped row (mobile
        # asset adoption) and the first-play pending flag jumps into it.
        self._discover_chain_start()

    def _discover_restart_session(self, recs, start_index):
        """Mobile startPreview-while-active parity: restart the window at the
        tapped row, keep the currently playing preview, drop unplayed
        previews from the queue, and keep already-staged assets for the new
        window so nothing re-downloads."""
        if self._closing or not recs:
            return
        first_index = max(0, min(start_index, len(recs) - 1))
        self._discover_chain_gen += 1   # cancels the in-flight prepare
        self._discover_downloading = False
        cur_idx = self.engine.queue_idx
        dropped = []
        for i in range(len(self.engine.queue) - 1, cur_idx, -1):
            p = self.engine.queue[i]
            if self._is_discover_path(p):
                dropped.append(p)
                self.engine.queue.pop(i)
        self._discover_paths = [p for p in self._discover_paths
                                if p not in dropped]
        self._discover_insert_pos = cur_idx + 1
        self._discover_rec_pos = first_index
        self._discover_window_start = first_index
        if self._discover_discover_mode:
            self._discover_window_end = len(recs)
        else:
            self._discover_window_end = min(
                first_index + 1 + self.DISCOVER_IMMEDIATE_COUNT, len(recs))
        window_keys = {
            self._discover_key(r) for r in recs[self._discover_window_start:]
        }
        kept = OrderedDict()
        for k, s in self._discover_states.items():
            if (k in window_keys and s.get("path")
                    and os.path.isfile(s["path"])
                    and s["status"] in ("ready", "saved")):
                kept[k] = s
        self._discover_states = kept
        self._discover_first_play_pending = not self._discover_discover_mode
        kept_paths = {s.get("path") for s in kept.values()}
        self._discover_delete_files([p for p in dropped if p not in kept_paths])
        self._discover_chain_start()
        self._discover_refill()

    def _discover_jump_into_preview(self, path):
        """Jump playback to an inserted preview (mobile playSequential /
        advanceToDeferredRecommendation parity)."""
        try:
            idx = self.engine.queue.index(path)
        except ValueError:
            return
        self.engine.queue_idx = idx
        self.engine.play_current()
        self.status_var.set("Discover: playing your recommendations")

    def _discover_leave_block(self, forward):
        """Non-Discover Next/Previous parity: leave the preview block and
        play the captured physical track — the seed's successor when moving
        forward, the seed itself when moving backward — then tear down."""
        physical = self._discover_physical_queue
        pidx = self._discover_physical_idx
        if physical is None or pidx is None:
            self._discover_end_session(ended=True)
            return True
        expected = (physical[:pidx + 1] + self._discover_paths +
                    physical[pidx + 1:])
        if self.engine.queue != expected:
            # The user replaced the queue — just end the session.
            self._discover_end_session(ended=True)
            return True
        target = min(pidx + (1 if forward else 0),
                     max(len(physical) - 1, 0))
        self.engine.queue = list(physical)
        self.engine.queue_idx = target
        self._discover_active = False
        self._discover_teardown()
        self.engine.play_current()
        return True

    def _discover_restart_window_after_current(self):
        """Mobile Discover Next-parity: the next recommendation is not queued
        yet — re-anchor preparation right behind the current preview instead
        of leaving the session. Returns False when nothing is left to prepare
        so the press falls through to the player (mobile: at the end, falls
        back to player skip)."""
        cur_idx = self.engine.queue_idx
        key = self._discover_key_for_path(
            self.engine.queue[cur_idx]) if 0 <= cur_idx < len(self.engine.queue) else None
        nxt = -1
        if key is not None:
            nxt = next((i for i, r in enumerate(self._discover_recs)
                        if self._discover_key(r) == key), -1)
        self._discover_rec_pos = max(nxt + 1, 0)
        self._discover_insert_pos = cur_idx + 1
        if self._discover_rec_pos >= self._discover_window_end:
            return False
        self._discover_refill()
        return True

    def _rec_seed_for(self, recs):
        if self._discover_seed is not None:
            return self._discover_seed
        first = recs[0] if recs else {}
        return (first.get("artist", ""), first.get("title", ""))

    def _discover_adopt_ready(self):
        """Discover ON: insert every already-staged recommendation into the
        queue behind the current track (mobile adoption of prefetched
        assets — no re-download)."""
        for rec in self._discover_recs:
            st = self._discover_states.get(self._discover_key(rec))
            if st and st["status"] == "ready" and st.get("path"):
                self._discover_insert_path(st["path"], rec.get("title", ""))

    def _discover_insert_path(self, path, title):
        self._discover_paths.append(path)
        pos = max(self._discover_insert_pos, self.engine.queue_idx + 1)
        self.engine.queue.insert(pos, path)
        self._discover_insert_pos = pos + 1
        self._update_queue()
        self.status_var.set(f"Discover: queued {title}")

    def _discover_consumed(self):
        """How many recommendation-block tracks playback has already walked
        past (0 while the seed is current)."""
        if self._discover_physical_idx is None:
            return 0
        if self.engine.queue_idx <= self._discover_physical_idx:
            return 0
        block_end = self._discover_physical_idx + len(self._discover_paths)
        if self.engine.queue_idx > block_end:
            return len(self._discover_paths)
        return self.engine.queue_idx - self._discover_physical_idx

    # ── Session end (toggle off / natural handoff) ──────────────────────────

    def _discover_consider_handoff(self):
        """Mobile natural-handoff parity: end the session once playback
        leaves the recommendation block (or the queue was replaced by a new
        playback context / the engine looped the playlist). Called whenever a
        track starts and after every completed download."""
        if not self._discover_active or self._closing:
            return
        if self._discover_downloading:
            # An in-flight download still owns the chain; its completion
            # handler re-runs this check before inserting or refilling.
            return
        physical = self._discover_physical_queue
        physical_idx = self._discover_physical_idx
        if physical is None or physical_idx is None:
            return
        expected = (
            physical[:physical_idx + 1] + self._discover_paths +
            physical[physical_idx + 1:]
        )
        block_end = physical_idx + len(self._discover_paths)
        if self.engine.queue != expected:
            self._discover_end_session(ended=True)
            return
        if self.engine.queue_idx > block_end:
            self._discover_end_session(ended=True)

    def _discover_end_session(self, ended=False):
        """Force-end the session (handoff path): cancels pending downloads
        via the generation hook and tears down."""
        self._discover_active = False
        self._discover_downloading = False
        self._discover_chain_gen += 1
        self._discover_teardown(ended=ended)

    def _discover_teardown(self, ended=False):
        """Remove the recommendation block from the queue, restore the
        captured queue where allowed, and wipe the staged preview audio.

        Restore rules (mobile parity): playback still on the seed or inside
        the block -> restore the physical queue and let the current
        recommendation finish (completion advances to the seed's successor).
        Playback already past the block -> rebuild the queue from the
        physical capture at the current position (no rewind). User replaced
        the queue -> leave it alone.
        """
        physical = self._discover_physical_queue
        physical_idx = self._discover_physical_idx
        discover_paths = self._discover_paths
        if physical is not None and physical_idx is not None:
            expected = (
                physical[:physical_idx + 1] + discover_paths +
                physical[physical_idx + 1:]
            )
            block_end = physical_idx + len(discover_paths)
            if self.engine.queue == expected and self.engine.queue_idx <= block_end:
                # Queue untouched — restore it and let the current track
                # finish before advancing to the seed's successor.
                self.engine.queue = list(physical)
                self.engine.queue_idx = physical_idx
                self._update_queue()
                self.status_var.set(
                    "Discover mode off — returned to your library"
                    if not discover_paths else
                    "Discover mode off — finishing the current song"
                )
            elif self.engine.queue == expected:
                # Playback already walked past the block: drop the played
                # previews from the queue and keep the current position
                # (mobile rebuilds the physical queue at the current index).
                self.engine.queue = list(physical)
                self.engine.queue_idx = max(0, self.engine.queue_idx - len(discover_paths))
                self._update_queue()
                self.status_var.set(
                    "Discover mode off — recommendations finished" if ended
                    else "Discover mode off")
            else:
                # The user started a new playback context — leave the queue.
                self.status_var.set(
                    "Discover mode off — recommendations finished" if ended
                    else "Discover mode off")
        self._discover_chain_stop()
        self._discover_cleanup_files(discover_paths)
        # Preview rows revert to idle; promoted rows keep their library copy.
        for key in list(self._discover_states):
            st = self._discover_states[key]
            if st.get("promoted"):
                st["status"] = "saved"
            elif st["status"] in ("ready", "saved", "waiting", "preparing",
                                  "resolving"):
                self._discover_states.pop(key, None)
        self._discover_paths = []
        self._discover_insert_pos = 0
        self._discover_dest = None
        self._discover_downloading = False
        self._discover_discover_mode = False
        self._discover_first_play_pending = False
        self._discover_advance_armed = False
        self._discover_notify_ui()

    def _discover_delete_files(self, paths):
        """Delete staged audio; a currently-playing file is deferred until
        the next track change (mobile: a still-playing recommendation is
        allowed to finish)."""
        cur = self._discover_current_path()
        now, later = [], []
        for p in paths:
            (later if p and p == cur else now).append(p)
        for p in now:
            try:
                os.remove(p)
            except OSError:
                pass
        if later:
            pending = self._discover_deferred_delete or []
            self._discover_deferred_delete = pending + later

    def _discover_cleanup_files(self, paths):
        """Delete staged preview audio. The currently-playing file is deferred
        until the next track change (mobile: a still-playing recommendation
        is allowed to finish)."""
        self._discover_delete_files(paths)
        # Stale previews from previous sessions should not appear in history.
        self._recently_played = [p for p in self._recently_played
                                 if p not in paths]

    def _discover_deferred_flush(self, current_path):
        pending = getattr(self, "_discover_deferred_delete", None)
        if not pending:
            return
        still = []
        for p in pending:
            if p == current_path:
                still.append(p)
                continue
            try:
                os.remove(p)
            except OSError:
                pass
        self._discover_deferred_delete = still or None

    # ── Promotion & row helpers (song page UI) ──────────────────────────────

    def _discover_promote(self, key):
        """Mobile saveToPlaylist parity: let the user pick a destination
        playlist, then copy the staged preview there and refresh the library.
        The staged preview copy stays until session cleanup so the queue
        entry keeps working. The destination is always chosen explicitly —
        previews are never dumped into Unsorted silently."""
        if self._closing:
            return
        if key in self._discover_saving_keys or key in self._discover_queued_keys:
            self.status_var.set("This recommendation is already being saved")
            return
        st = self._discover_states.get(key)
        rec = st.get("rec") if st else None
        saved_path = self._discover_saved_uris.get(key)
        if saved_path:
            # Mobile AddedToPlaylist parity: the local library copy already
            # exists — only link it into the chosen playlist (a fast local
            # copy on desktop, never a re-download).
            def on_added(dest_dir, playlist_name):
                if self._closing:
                    return
                self._refresh_library()
                name = (rec.get("title") if rec else None) or os.path.basename(saved_path)
                self.status_var.set(
                    f"Added {name} to '{playlist_name}'")
            self._show_add_to_playlist_dialog(
                [saved_path],
                "Add to playlist",
                on_transfer=on_added,
                subtitle="Already in your library — a copy is added to the chosen playlist",
            )
            return
        path = st.get("path") if st else None
        if path and os.path.isfile(path):
            self._discover_promote_staged(key, st, rec, path)
        else:
            self._discover_download_new(
                key, rec.get("artist", "") if rec else "",
                rec.get("title", "") if rec else "")

    def _discover_promote_staged(self, key, st, rec, path):
        """Mobile saveToPlaylist promotion parity: pick a destination, then
        copy the staged preview there on a worker thread ('Saving to
        playlist...' while it runs). The staged copy stays until session
        cleanup so the queue entry keeps working."""
        title = rec.get("title", "") if rec else os.path.basename(path)

        def on_transfer(dest_dir, playlist_name):
            if self._closing:
                return
            self._discover_saving_keys.add(key)
            self._discover_save_failures.pop(key, None)
            self._discover_notify_ui()
            dest_path = os.path.join(dest_dir, os.path.basename(path))

            def copy():
                try:
                    shutil.copy2(path, dest_path)
                    ok, err = True, None
                except OSError as exc:
                    ok, err = False, str(exc)
                try:
                    self.after(0, lambda: self._discover_promote_done(
                        key, st, dest_path, playlist_name, ok, err))
                except (RuntimeError, tk.TclError):
                    pass

            threading.Thread(target=copy, daemon=True,
                             name="luno-promote").start()

        self._show_add_to_playlist_dialog(
            [],
            "Download to playlist",
            on_transfer=on_transfer,
            subtitle="The preview will be copied into the chosen playlist",
        )

    def _discover_promote_done(self, key, st, dest_path, playlist_name, ok, err):
        self._discover_saving_keys.discard(key)
        if self._closing:
            return
        if ok:
            st["promoted"] = True
            st["status"] = "saved"
            self._discover_saved_uris[key] = dest_path
            self._discover_save_failures.pop(key, None)
            self._refresh_library()
            self.status_var.set(
                f"Saved {os.path.basename(dest_path)} to '{playlist_name}'")
        else:
            self._discover_save_failures[key] = err or "Could not save recommendation"
        self._discover_notify_ui()

    def _discover_download_new(self, key, artist, title):
        """Mobile saveToPlaylist parity for rows without a staged preview:
        ask where to save first, then download directly into that playlist
        folder (never silently Unsorted). The row flips to 'Download queued
        for playlist' until the job completes — permanent save or an inline
        saveFailure (mobile observeQueuedDownload parity)."""
        if self._closing:
            return
        if key in self._discover_saving_keys or key in self._discover_queued_keys:
            self.status_var.set("This recommendation is already being saved")
            return

        def on_transfer(dest_dir, playlist_name):
            if self._closing:
                return
            st = self._discover_states.get(key)
            if st is not None and st.get("path") and os.path.isfile(st["path"]):
                # A staged asset arrived while the picker was open — promote
                # it instead of downloading again (mobile promotion parity).
                self._discover_promote_staged(key, st, st.get("rec"), st["path"])
                return
            self._discover_queued_keys[key] = dest_dir
            self._discover_save_failures.pop(key, None)
            self._discover_notify_ui()
            self.status_var.set(f"Downloading {title} into '{playlist_name}'…")
            self._download_track(
                artist, title, dest=dest_dir,
                on_finish=lambda ok, path: self._discover_download_observed(
                    key, ok, path, playlist_name))

        self._show_add_to_playlist_dialog(
            [],
            "Download to playlist",
            on_transfer=on_transfer,
            subtitle="The song will be downloaded into the chosen playlist",
        )

    def _discover_download_observed(self, key, ok, path, playlist_name):
        """Mobile observeQueuedDownload parity: a queued playlist download
        finished — record the permanent copy or surface the failure."""
        if self._closing:
            return
        self._discover_queued_keys.pop(key, None)
        if ok and path and os.path.isfile(path):
            self._discover_saved_uris[key] = path
            self._discover_save_failures.pop(key, None)
            self.status_var.set(
                f"Saved {os.path.basename(path)} to '{playlist_name}'")
        else:
            self._discover_save_failures[key] = "Download failed — see Download log"
        st = self._discover_states.get(key)
        if st is not None:
            if ok and path:
                st["status"] = "saved"
                st["promoted"] = True
        self._discover_notify_ui()

    def _discover_play_rec(self, key):
        """Mobile row-tap parity: start a temporary preview session at this
        recommendation. The tapped row is prepared on demand (adopting any
        cached asset) and plays as soon as it is ready; during an active
        session the window restarts from the tapped row (Discover subtitle:
        'tap a song to restart from there')."""
        if self._closing or not (hasattr(self, "engine") and self.engine.queue):
            return
        if not self._discover_recs:
            return
        idx = next((i for i, r in enumerate(self._discover_recs)
                    if self._discover_key(r) == key), -1)
        if idx < 0:
            return
        if self._discover_active:
            cur = self._discover_current_path()
            if (cur is not None and
                    self._discover_key_for_path(cur) == key):
                return
            self._discover_restart_session(self._discover_recs, idx)
        else:
            self._discover_start_session(
                self._discover_recs, idx, discover_mode=False)
            self._discover_refill()

    def _discover_row_state(self, key):
        st = self._discover_states.get(key)
        if st is None:
            saved = self._discover_saved_uris.get(key)
            if saved is None:
                return None
            st = {"status": "saved", "path": saved, "error": None,
                  "rec": None, "promoted": True, "source": None}
        cur = self._discover_current_path()
        return {
            "status": st["status"],
            "error": st.get("error"),
            "path": st.get("path"),
            "promoted": st.get("promoted", False),
            "saving": key in self._discover_saving_keys,
            "queued": key in self._discover_queued_keys,
            "permanent": key in self._discover_saved_uris,
            "saveFailure": self._discover_save_failures.get(key),
            "playing": bool(st.get("path")) and st["path"] == cur,
        }

    def _discover_ready_count(self):
        return sum(1 for s in self._discover_states.values()
                   if s["status"] in ("ready", "saved"))

    def _discover_busy_count(self):
        return sum(1 for s in self._discover_states.values()
                   if s["status"] in ("waiting", "preparing", "resolving"))

    def _discover_chain_done(self):
        return (self._discover_rec_pos >= self._discover_window_end
                and not self._discover_downloading)

    # ── Session routing & helpers (song page / transport / media keys) ──────

    def _discover_current_path(self):
        try:
            if (hasattr(self, "engine") and self.engine.queue and
                    0 <= self.engine.queue_idx < len(self.engine.queue)):
                return self.engine.queue[self.engine.queue_idx]
        except Exception:
            pass
        return None

    def _is_discover_path(self, path):
        """True for any transient preview path: inserted session paths or
        anything staged in PREVIEW_DIR (mobile isTransient parity)."""
        if not path:
            return False
        if path in self._discover_paths:
            return True
        return path.startswith(self.PREVIEW_DIR + os.sep)

    def _discover_key_for_path(self, path):
        for k, s in self._discover_states.items():
            if s.get("path") == path:
                return k
        return None

    def _discover_session_seed(self):
        """(artist, title) of the seed song while a session is active, else
        None — the song page keeps its list pinned to this seed instead of
        re-fetching for every transient preview track (mobile seedUri
        mismatch-display parity)."""
        if self._discover_active and self._discover_seed is not None:
            return self._discover_seed
        return None

    def _discover_current_key(self):
        cur = self._discover_current_path()
        if cur is None or not self._is_discover_path(cur):
            return None
        return self._discover_key_for_path(cur)

    def _next_queued_is_preview(self):
        try:
            nxt = self.engine.queue[self.engine.queue_idx + 1]
        except (IndexError, TypeError):
            return False
        return self._is_discover_path(nxt)

    def _discover_preparing(self):
        return self._discover_downloading or self._discover_chain_running

    def _route_session_next(self):
        """Mobile skipToNext parity: transport Next/Previous route through an
        active preview session; returns True when the press was consumed."""
        if not self._discover_active:
            return False
        cur = self._discover_current_path()
        if cur is None:
            return False
        transient = self._is_discover_path(cur)
        if not self._discover_discover_mode:
            # Non-Discover preview: Next leaves the preview block and plays
            # the physical successor (mobile parity). While the first preview
            # is still preparing the seed is current — fall through to the
            # player.
            if not transient:
                return False
            return self._discover_leave_block(forward=True)
        # Discover mode.
        if not transient:
            # Seed still current: if the first preview is already queued a
            # normal skip walks into it; while the deferred block is still
            # being prepared, the first press arms the auto-advance (mobile
            # advanceToDeferredRecommendation), repeated presses fall
            # through so the keys never feel dead.
            if self._next_queued_is_preview():
                return False
            if self._discover_preparing():
                if not self._discover_advance_armed:
                    self._discover_advance_armed = True
                    return True
            return False
        # A preview is current: a normal skip walks into the queued block.
        # If the next item is not queued yet (preparation gap), re-anchor
        # the window right behind the current preview instead of leaving the
        # session (mobile startPreview at currentIndex+1); at the end of the
        # loaded list the press falls through to the player.
        if self._next_queued_is_preview() or \
                self.engine.queue_idx + 1 >= len(self.engine.queue):
            return False
        return self._discover_restart_window_after_current()

    def _route_session_prev(self):
        if not self._discover_active:
            return False
        cur = self._discover_current_path()
        if cur is None:
            return False
        transient = self._is_discover_path(cur)
        if not self._discover_discover_mode:
            if not transient:
                return False
            # Previous plays the captured physical track at the seed's index
            # (mobile parity) and tears the session down.
            return self._discover_leave_block(forward=False)
        # Discover: a normal previous walks into the block when one exists;
        # otherwise restart the session at the previous recommendation.
        if self.engine.queue_idx - 1 >= 0:
            if self._is_discover_path(self.engine.queue[self.engine.queue_idx - 1]):
                return False
        key = self._discover_current_key()
        idx = -1
        if key is not None:
            idx = next((i for i, r in enumerate(self._discover_recs)
                        if self._discover_key(r) == key), -1)
        if idx > 0:
            self._discover_restart_session(self._discover_recs, idx - 1)
            return True
        return False

    def _user_next(self):
        if (not hasattr(self, "engine") or self.engine is None or
                not getattr(self.engine, "mixer_ready", False)):
            return
        if self._route_session_next():
            return
        self.engine.next_track()

    def _user_prev(self):
        if (not hasattr(self, "engine") or self.engine is None or
                not getattr(self.engine, "mixer_ready", False)):
            return
        if self._route_session_prev():
            return
        self.engine.prev_track()

    def _on_playback_failed(self, path):
        """Mobile recoverFromPlaybackError parity: a preview that fails to
        load marks its row failed, is removed (file + queue), and the
        engine's automatic skip resumes the next item."""
        if not self._discover_active:
            return
        if not self._is_discover_path(path):
            return
        key = self._discover_key_for_path(path)
        st = self._discover_states.get(key) if key else None
        if st is None:
            return
        st["status"] = "failed"
        st["error"] = "This preview could not be played"
        st["path"] = None
        if path in self._discover_paths:
            self._discover_paths.remove(path)
        try:
            if path in self.engine.queue:
                self.engine.queue.remove(path)
        except Exception:
            pass
        try:
            os.remove(path)
        except OSError:
            pass
        self._discover_notify_ui()

    def _discover_maybe_auto_extend(self):
        """Mobile Discover infinite-scroll parity: when playback reaches the
        last loaded recommendation and the list is not exhausted, fetch the
        next page automatically (the song page's load-more guards dedupe,
        in-flight, and the 1,000 cap)."""
        if not (self._discover_active and self._discover_discover_mode):
            return
        if not self._discover_recs:
            return
        cur = self._discover_current_path()
        if cur is None or not self._is_discover_path(cur):
            return
        last = self._discover_key(self._discover_recs[-1])
        if self._discover_key_for_path(cur) != last:
            return
        sp = self._frames.get("song_page")
        if sp is not None and hasattr(sp, "_load_more_recs"):
            try:
                self.after(0, sp._load_more_recs)
            except (RuntimeError, tk.TclError):
                pass

    def _find_downloaded_audio(self, artist, title, dest):
        safe = re.sub(r'[<>:"/\\|?*]', '', f"{artist} - {title}").strip() or "Unknown"
        for ext in self.PREVIEW_EXTS:
            candidate = os.path.join(dest or "", f"{safe}{ext}")
            if os.path.isfile(candidate):
                return candidate
        return None

    def _download_track(self, artist, title, url=None, dest=None, on_finish=None):
        """Standardized interface to bridge Discovery with the Downloader.

        on_finish(success, path) runs on the UI thread after the job
        completes (used by the song page's queued-save observation)."""
        if dest is None:
            dest = (self.dl_dest_var.get() or "").strip()
        if not dest or dest == "— choose playlist —":
            dest = self.MUSIC_ROOT

        try:
            os.makedirs(dest, exist_ok=True)
        except OSError as exc:
            self._log_error("Download", f"Could not create download folder '{dest}': {exc}")
            self.status_var.set("Discovery download failed — invalid destination")
            return

        def dispatch_ui(callback):
            """Run a callback on Tk's main thread; downloads run in a worker."""
            if self._closing:
                return
            try:
                self.after(0, callback)
            except (RuntimeError, tk.TclError):
                # The window may have been closed while yt-dlp was finishing.
                pass
            
        track = {"artist": artist, "title": title, "url": url}
        result = {"success": False}
        
        def on_log(t, success, msg):
            result["success"] = bool(success)
            m = f"{t}: {'✓' if success else '✕'} {msg if msg else ''}"
            dispatch_ui(lambda m=m: self._log_error("Download", m))

        def on_progress(done, total, query):
            message = f"Downloading {query}..." if query else ""
            dispatch_ui(lambda message=message: self.status_var.set(message))

        def on_complete():
            # Runs in the downloader worker; resolve the produced file there
            # so the UI thread never blocks on a filesystem walk.
            result["path"] = self._find_downloaded_audio(artist, title, dest)

            def finish():
                self._discovery_downloads = max(0, self._discovery_downloads - 1)
                self._refresh_library(reset_track_scroll=True)
                self.status_var.set(
                    "Discovery download complete"
                    if result["success"]
                    else "Discovery download failed — see Download log"
                )
                if on_finish is not None:
                    try:
                        on_finish(result["success"], result.get("path"))
                    except Exception:
                        pass
            dispatch_ui(finish)
            
        from downloader import Downloader
        self._discovery_downloads += 1
        Downloader.download_sync_tracks(
            [track],
            dest,
            self.skip_var.get(),
            self.sc_var.get(),
            on_progress=on_progress,
            on_log=on_log,
            on_complete=on_complete,
            check_cancel=lambda: False
        )

    def _refresh_library(self, reset_track_scroll=False):
        """Manual refresh triggered by the user or startup."""
        with self._library_scan_state_lock:
            self._library_scan_generation += 1
            generation = self._library_scan_generation
            if self._library_scan_active:
                self._library_scan_pending = True
                self._library_scan_pending_reset |= reset_track_scroll
                return
            self._library_scan_active = True
        roots = tuple(self._watch_roots)
        threading.Thread(
            target=self._run_library_scan_bg,
            args=(generation, reset_track_scroll, roots),
            daemon=True,
            name="luno-library-scan",
        ).start()

    def _run_library_scan_bg(self, generation=None, reset_track_scroll=False, roots=None):
        """The heavy lifting happens here on a background thread."""
        try:
            new_lib = scan_library(roots if roots is not None else self._watch_roots)
            error = None
        except Exception as exc:
            new_lib = None
            error = exc
        self.after(
            0,
            lambda: self._finish_library_scan(
                new_lib, generation, reset_track_scroll, error
            ),
        )

    def _finish_library_scan(self, new_lib, generation, reset_track_scroll, error=None):
        try:
            if error is not None:
                self._log_error("library", f"Scan failed: {error}")
            else:
                self._finalize_library_update(
                    new_lib, generation, reset_track_scroll
                )
        finally:
            with self._library_scan_state_lock:
                self._library_scan_active = False
                pending = self._library_scan_pending
                pending_reset = self._library_scan_pending_reset
                self._library_scan_pending = False
                self._library_scan_pending_reset = False
            if pending and not self._closing:
                self._refresh_library(reset_track_scroll=pending_reset)

    def _finalize_library_update(self, new_lib, generation=None, reset_track_scroll=False):
        # Multiple refreshes can overlap while a download is finishing. Ignore
        # an older scan so it cannot overwrite a newer library snapshot.
        if generation is not None and generation != self._library_scan_generation:
            return

        # Only refresh UI if contents actually changed to prevent stuttering
        current_hash = hash(tuple(
            (playlist, tuple(paths))
            for playlist, paths in sorted(new_lib.items())
        ))
        if current_hash == self._last_lib_hash:
            if reset_track_scroll:
                lib_view = self._frames.get("library")
                if lib_view:
                    lib_view.reset_track_scroll()
            return
            
        self._last_lib_hash = current_hash
        self._library = new_lib

        # Favorites are metadata over the scanned files, not a physical folder.
        # Drop entries for files that no longer exist so the virtual playlist and
        # persisted state cannot accumulate dead paths.
        known_paths = {
            os.path.abspath(os.path.normpath(path))
            for tracks in self._library.values()
            for path in tracks
        }
        self.art_cache.prune_paths(known_paths)
        before_favorites = set(self._favorite_paths)
        self._favorite_paths.intersection_update(known_paths)
        if self._favorite_paths != before_favorites:
            self._save_config()
        
        # 1. Redraw the sidebar cards (updates song counts)
        self._populate_sidebar()
        
        # 2. Get the current search query so we don't lose our place
        search_q = self.lib_search_entry.get() if self.lib_search_entry.get() != self._lib_ph else ""
        
        lib_view = self._frames.get("library")
        if lib_view and hasattr(lib_view, "prune_duration_cache"):
            lib_view.prune_duration_cache(known_paths)

        # 3. Redraw the track list
        self._populate_track_list("")
        if reset_track_scroll and lib_view:
            lib_view.reset_track_scroll()
        
        # 3b. Refresh the header art with current playlist paths
        if lib_view:
            name = self._selected_playlist or "All Music"
            total = sum(len(v) for v in self._library.values())
            if name == FAVORITES_PLAYLIST:
                cnt = len(self._favorite_paths_in_library())
            else:
                cnt = len(self._library.get(name, [])) if self._selected_playlist else total
            lib_view.update_header_art(
                name,
                f"{cnt} songs",
                "songs yet to find a home" if name == "Unsorted" else "",
            )
        
        # 4. Update the download destination dropdown options
        options = ["— choose playlist —", "All Music", "Unsorted"]
        for pl in sorted(self._library.keys()):
            if pl not in ["Unsorted", "All Music", FAVORITES_PLAYLIST]:
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
        entries = [("All Music", total_songs), (FAVORITES_PLAYLIST, len(self._favorite_paths_in_library()))]
        for name in sorted(self._library.keys()):
            if name not in ["Unsorted", "All Music", FAVORITES_PLAYLIST]:
                entries.append((name, len(self._library[name])))

        self._sync_library_header_button()
        self._sync_unsorted_tab()

        for idx, (name, count) in enumerate(entries):
            card = tk.Frame(self._pl_inner, bg=SIDEBAR, cursor="hand2")
            card.pack(fill="x", pady=1)
            inner = tk.Frame(card, bg=SIDEBAR, cursor="hand2")
            inner.pack(fill="x", padx=8, pady=3)

            pl_paths = []
            if name == FAVORITES_PLAYLIST:
                pl_paths = self._favorite_paths_in_library()
            elif name == "All Music":
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
                if pl_name not in ("All Music", FAVORITES_PLAYLIST):
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

    def _select_playlist_card(self, idx, name):
        self._pl_selected_idx = idx
        # No persistent highlight — just act on the selection
        if name == "All Music":
            self._selected_playlist = None
            total = sum(len(v) for v in self._library.values())
            self._frames["library"].update_header_art("All Music", f"{total} songs")
        elif name == FAVORITES_PLAYLIST:
            self._selected_playlist = FAVORITES_PLAYLIST
            count = len(self._favorite_paths_in_library())
            self._frames["library"].update_header_art(FAVORITES_PLAYLIST, f"{count} songs")
        else:
            self._selected_playlist = name
            cnt = len(self._library.get(name, []))
            self._frames["library"].update_header_art(
                name,
                f"{cnt} songs",
                "songs yet to find a home" if name == "Unsorted" else "",
            )
        self._sync_library_header_button()
        self._sync_unsorted_tab()
        self._populate_track_list()
        self._show_tab("library")
        self._frames["library"].reset_track_scroll()

    def _sync_library_header_button(self):
        if not hasattr(self, "_library_header_btn"):
            return
        self._library_header_btn.config(
            fg=GREEN if self._selected_playlist is None else TEXT2
        )

    def _sync_unsorted_tab(self):
        if not hasattr(self, "_unsorted_tab"):
            return
        count = len(self._library.get("Unsorted", []))
        self._unsorted_tab.config(
            text=f"Unsorted ({count})",
            fg=GREEN if self._selected_playlist == "Unsorted" else TEXT3,
        )

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
        return []

    # ── Portable library transfer ───────────────────────────────────────────
    def _export_json(self, scope="library", selected_paths=None, playlist_names=None):
        """Export metadata references without blocking Tk on audio probing."""
        if self._transfer_running:
            messagebox.showinfo("Transfer in progress", "Wait for the current library transfer to finish.")
            return
        initial = {
            "selected_tracks": "luno-selected.json",
            "playlists": "luno-playlists.json",
            "library": "luno-library.json",
        }.get(scope, "luno-library.json")
        path = filedialog.asksaveasfilename(
            title="Export Library JSON",
            defaultextension=".json",
            filetypes=[("Luno JSON", "*.json"), ("JSON", "*.json"), ("All Files", "*.*")],
            initialfile=initial,
        )
        if not path:
            return

        library_snapshot = {name: list(paths) for name, paths in self._library.items()}
        selected_snapshot = list(selected_paths or [])
        playlist_snapshot = list(playlist_names or [])
        self._transfer_running = True
        self.status_var.set("Preparing export…")

        def run():
            try:
                manifest = build_manifest(
                    library_snapshot,
                    self.MUSIC_ROOT,
                    scope=scope,
                    selected_paths=selected_snapshot,
                    playlist_names=playlist_snapshot,
                )
                write_manifest(path, manifest)
            except Exception as exc:
                self.after(0, lambda: self._finish_transfer_error("Export failed", exc))
                return
            self.after(0, lambda: self._finish_export(path, len(manifest["tracks"])))

        threading.Thread(target=run, daemon=True).start()

    def _finish_export(self, path, track_count):
        self._transfer_running = False
        self.status_var.set("")
        messagebox.showinfo(
            "Export complete",
            f"Exported {track_count} track reference(s) to {os.path.basename(path)}.\n\n"
            "Audio files and local paths are not included.",
        )

    def _export_selected_json(self):
        paths = self._get_sel_paths()
        if not paths:
            messagebox.showinfo("Nothing selected", "Select one or more tracks first.")
            return
        self._export_json("selected_tracks", selected_paths=paths)

    def _export_playlist_json(self):
        selected = self._selected_playlist
        if selected and selected != "All Music":
            self._export_json("playlists", playlist_names=[selected])
        else:
            self._export_json("library")

    def _export_full_library_json(self):
        self._export_json("library")

    def _import_json(self):
        """Merge a portable manifest and optionally queue unresolved tracks."""
        if self._transfer_running:
            messagebox.showinfo("Transfer in progress", "Wait for the current library transfer to finish.")
            return
        path = filedialog.askopenfilename(
            title="Import Library JSON",
            filetypes=[("Luno JSON", "*.json"), ("JSON", "*.json"), ("All Files", "*.*")],
        )
        if not path:
            return

        library_snapshot = {name: list(paths) for name, paths in self._library.items()}
        self._transfer_running = True
        self.status_var.set("Checking import…")

        def run():
            try:
                manifest = load_manifest(path)
                plan = plan_import(manifest, library_snapshot)
                result = apply_import_plan(plan, self.MUSIC_ROOT)
            except Exception as exc:
                self.after(0, lambda: self._finish_transfer_error("Import failed", exc))
                return
            self.after(0, lambda: self._finish_import(result))

        threading.Thread(target=run, daemon=True).start()

    def _finish_transfer_error(self, title, exc):
        self._transfer_running = False
        self.status_var.set("")
        messagebox.showerror(title, str(exc))

    def _finish_import(self, result):
        self._transfer_running = False
        self.status_var.set("")
        missing = result.get("missing", [])
        copied = result.get("copied", 0)
        errors = result.get("errors", [])
        if missing:
            ambiguous = sum(1 for track in missing if track.get("ambiguity_confirmation_required"))
            question = (
                f"Merged {copied} existing track(s). {len(missing)} track(s) still need audio.\n\n"
                "Download the missing tracks using the existing YouTube search/downloader?\n"
                "Metadata-only matches can be a different recording, so confirm only if that is acceptable."
            )
            if ambiguous:
                question += f"\n\n{ambiguous} track(s) require ambiguity confirmation."
            if messagebox.askyesno("Download missing tracks?", question):
                self._queue_import_downloads(missing, result.get("playlist_by_ref", {}))
            else:
                messagebox.showinfo(
                    "Import partially complete",
                    f"Merged {copied} existing track(s); {len(missing)} missing track(s) were not downloaded.",
                )
        else:
            messagebox.showinfo("Import complete", f"Merged {copied} track(s) into the library.")
        if errors:
            self._log_error("Import", "; ".join(errors[:5]))
        self._refresh_library()

    def _queue_import_downloads(self, tracks, playlist_by_ref):
        from downloader import Downloader

        rows = []
        for track in tracks:
            row = {
                "title": track.get("title", "Unknown track"),
                "artist": track.get("artist", ""),
                "playlist_name": playlist_by_ref.get(track.get("ref"), "Unsorted"),
            }
            source = track.get("source") or {}
            if source.get("provider") == "youtube":
                row["url"] = f"https://www.youtube.com/watch?v={source['id']}"
            rows.append(row)

        def folder_for(name):
            existing = self._find_playlist_dir(name)
            if existing:
                return existing
            if name and name != "Unsorted":
                destination = os.path.join(self.MUSIC_ROOT, name)
                os.makedirs(destination, exist_ok=True)
                return destination
            return self.MUSIC_ROOT

        self.status_var.set(f"Downloading {len(rows)} imported track(s)…")
        Downloader.download_csv_tracks(
            rows,
            folder_for,
            skip_existing=True,
            try_sc=False,
            on_progress=lambda done, total, query: self.after(
                0, lambda: self.status_var.set(f"Downloading imported tracks… {done}/{total}")
            ),
            on_log=lambda title, success, error: None,
            on_complete=lambda: self.after(0, self._finish_import_downloads),
            check_cancel=lambda: self._closing,
        )

    def _finish_import_downloads(self):
        self.status_var.set("")
        self._refresh_library()
        messagebox.showinfo("Import downloads complete", "The imported tracks have been added to the library.")

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
        with self._duplicate_scan_lock:
            if self._duplicate_scan_active:
                self._log_error("Duplicates", "A duplicate scan is already in progress")
                return
            self._duplicate_scan_active = True
        if not pl_name: tracks = [p for sub in self._library.values() for p in sub]
        else: tracks = self._library.get(pl_name, [])
        
        if not tracks:
            with self._duplicate_scan_lock:
                self._duplicate_scan_active = False
            messagebox.showinfo("Check Duplicates", "This playlist is empty.")
            return
            
        def run_scan():
            try:
                from engine import find_duplicates
                # find_duplicates expects a dict: { playlist_name: [paths] }
                dupes = find_duplicates(
                    {pl_name or "All Music": tracks},
                    allowed_keys=self.engine.allowed_dupe_keys,
                )
                error = None
            except Exception as exc:
                dupes = []
                error = exc
            self.after(
                0,
                lambda: self._finalize_playlist_dupe_check(
                    dupes, pl_name, error
                ),
            )
            
        threading.Thread(target=run_scan, daemon=True).start()

    def _finalize_playlist_dupe_check(self, dupes, pl_name, error=None):
        with self._duplicate_scan_lock:
            self._duplicate_scan_active = False
        if error is not None:
            messagebox.showerror("Check Duplicates", f"Duplicate scan failed: {error}")
            return
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
        if self._sync_running and not batch_mode:
            self._log_error("Sync", f"Already syncing a playlist; skipped '{pl_name}'")
            return
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

            self._favorite_paths.difference_update(self._favorite_key(path) for path in paths)
            self._save_config()
            
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
        protected_names = ["All Music", "Unsorted", root_name, "Luno_Library"]

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
        if pl_name in ["Unsorted", "Luno_Library", root_name]:
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
        self.engine._original_queue = list(paths)
        
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
        self.bind("<XF86AudioNext>",         lambda e: self._user_next())
        self.bind("<XF86AudioPrev>",         lambda e: self._user_prev())
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
            "next": self._user_next,
            "previous": self._user_prev,
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
            self._mp_info_center.setNowPlayingInfo_({self._mp_title_key: "Luno"})
            self._remote_commands_ready = True
        except Exception as e:
            self._log_error("GlobalKeys", f"Now Playing setup failed: {e}")

    def _update_now_playing_info(self, title="", artist="", duration=0.0):
        """Publish truthful Now Playing metadata/state from engine.
        Rate is 1 when playing, 0 when paused/stopped.
        Elapsed consults the engine's canonical playback position."""
        if not hasattr(self, '_mp_info_center'):
            return
        try:
            rate = 1.0 if (self.engine.playing and not self.engine.paused) else 0.0
            elapsed = self.engine.get_position()

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
        # Only refresh if no download, transfer, or scan is already active.
        download_view = self._frames.get("download")
        download_busy = bool(download_view and getattr(download_view, "is_dl", False))
        with self._library_scan_state_lock:
            scan_active = self._library_scan_active
        if (not self._is_dl and not download_busy and not self._sync_running and
                not self._transfer_running and not self._discovery_downloads and
                not scan_active):
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
        """Restores application state from ~/.luno_config.json"""
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

            favorite_paths = cfg.get("favorite_songs", [])
            if isinstance(favorite_paths, list):
                self._favorite_paths = {
                    self._favorite_key(path)
                    for path in favorite_paths
                    if isinstance(path, str) and path.strip()
                }
                
        except Exception as e:
            print(f"Failed to load config: {e}")

    def _save_config(self):
        """Persists current state to ~/.luno_config.json"""
        cfg = {
            "music_root": self.MUSIC_ROOT,
            "lastfm_key": self.lastfm_key_var.get(),
            "auto_discovery": self.auto_discovery_var.get(),
            "volume": self.vol_var.get(),
            "crossfade": self._cf_var.get(),
            "sleep_timer": self._sleep_var.get(),
            "global_keys": self.global_keys_var.get(),
            "favorite_songs": sorted(self._favorite_paths),
        }
        try:
            with open(CONFIG_FILE, "w") as f:
                json.dump(cfg, f, indent=4)
        except Exception as e:
            print(f"Failed to save config: {e}")

    # ── PLAYLIST URL PERSISTENCE ─────────────────────────────────────────────
    def _playlist_meta_path(self):
        return os.path.join(self.MUSIC_ROOT, ".luno_playlist_urls.json")

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
        try:
            from downloader import Downloader
            Downloader.cancel_active()
        except Exception:
            pass
        if hasattr(self, "art_cache"):
            self.art_cache.shutdown()
        if hasattr(self, "_preview_resolver_pool"):
            try:
                self._preview_resolver_pool.shutdown(
                    wait=False, cancel_futures=True)
            except Exception:
                pass
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
    app = LunoMusicPlayer()
    app.mainloop()
