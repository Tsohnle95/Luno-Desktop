import os
import re
import json
import random
import time
import base64
import flet as ft
import pygame
import threading

# Default configurations
DEFAULT_MUSIC_ROOT = os.path.join(os.path.expanduser("~"), "Music", "app_music_folder")
CONFIG_FILE = os.path.join(os.path.expanduser("~"), ".vibe_config.json")

from concurrent.futures import ThreadPoolExecutor
from theme import playlist_gradient

class FletArtCache:
    def __init__(self, player):
        self.player = player
        self.cache = {}  # filepath -> bytes
        self.fetching = set()
        self.executor = ThreadPoolExecutor(max_workers=4)

    def get(self, path, callback):
        if not path:
            return None
        if path in self.cache:
            val = self.cache[path]
            if callback:
                try:
                    callback(path, val)
                except Exception:
                    pass
            return val
            
        if path not in self.fetching:
            self.fetching.add(path)
            self.executor.submit(self._fetch, path, callback)
        return None

    def _fetch(self, path, callback):
        try:
            title, artist, art_bytes = self.player.get_track_metadata(path)
            self.cache[path] = art_bytes or b""
            self.fetching.discard(path)
            if callback:
                try:
                    callback(path, art_bytes)
                except Exception as e:
                    print(f"Error in art callback: {e}")
        except Exception as e:
            self.fetching.discard(path)
            if callback:
                try:
                    callback(path, None)
                except Exception as ex:
                    print(f"Error in art callback fallback: {ex}")

class FletVibePlayer:
    def __init__(self, page: ft.Page):
        self.page = page
        
        # Player state & caches
        self.metadata_cache = {}
        self.art_cache = FletArtCache(self)
        
        # Player state variables
        self.music_root = DEFAULT_MUSIC_ROOT
        self.library = {}  # playlist_name -> list of paths
        self.playlists = []  # list of playlist names
        self.current_playlist = "All Music"
        self.current_track = None
        self.queue = []
        self.queue_idx = -1
        self.playing = False
        self.shuffle = False
        self.repeat_mode = "off"  # "off", "one", "all"
        self.volume = 0.7
        self.duration_ms = 0
        self.position_ms = 0
        self.seeking = False
        self.last_search_query = ""
        self.track_limit = 100
        
        # Load persisted config
        self.load_config()
        
        # Audio component using Pygame as requested by project rules
        try:
            import pygame
            self.pygame = pygame
            if not pygame.mixer.get_init():
                pygame.mixer.pre_init(44100, -16, 2, 2048) 
                pygame.mixer.init()
        except Exception as e:
            print(f"Pygame Init Error: {e}")
            self.pygame = None
            
        # Audio position polling thread
        self.audio_thread_running = True
        self.audio_thread = threading.Thread(target=self._audio_poll_loop, daemon=True)
        self.audio_thread.start()
        
        # Setup page configuration
        self.page.title = "Vibe Music Player (Flet Edition)"
        self.page.theme_mode = ft.ThemeMode.DARK
        self.page.bgcolor = "#121212"
        self.page.padding = 0
        self.page.window_width = 1100
        self.page.window_height = 800
        self.page.window_resizable = True
        self.page.on_resize = self.on_page_resize
        
        # Scan initial library
        self.scan_library()
        
        # Initialize UI Components
        self.build_ui()
        self.refresh_ui()

    # ── PERSISTENCE ──────────────────────────────────────────────────────────
    def load_config(self):
        if os.path.exists(CONFIG_FILE):
            try:
                with open(CONFIG_FILE, "r") as f:
                    cfg = json.load(f)
                if "music_root" in cfg and os.path.exists(cfg["music_root"]):
                    self.music_root = cfg["music_root"]
                if "volume" in cfg:
                    self.volume = float(cfg["volume"]) / 100.0
            except Exception as e:
                print(f"Failed to load config: {e}")
                
        # Make sure directory exists
        try:
            os.makedirs(self.music_root, exist_ok=True)
        except Exception:
            pass

    def save_config(self):
        try:
            with open(CONFIG_FILE, "w") as f:
                json.dump({
                    "music_root": self.music_root,
                    "volume": int(self.volume * 100)
                }, f)
        except Exception as e:
            print(f"Failed to save config: {e}")

    # ── LIBRARY SCANNER ───────────────────────────────────────────────────────
    def scan_library(self):
        lib = {}
        EXTS = (".mp3", ".m4a", ".wav", ".flac", ".mp4", ".webm")
        root = self.music_root
        
        if not os.path.exists(root):
            self.library = {"All Music": [], "Unsorted": []}
            self.playlists = ["All Music", "Unsorted"]
            return
            
        # Scan immediate subfolders as playlists
        for entry in sorted(os.scandir(root), key=lambda e: e.name.lower()):
            if entry.is_dir():
                songs = sorted([os.path.join(entry.path, f) for f in os.listdir(entry.path)
                               if f.lower().endswith(EXTS)],
                               key=lambda x: os.path.basename(x).lower())
                # Add even empty subdirectories to match project constraints
                lib[entry.name] = songs
                
        # Scan root folder files as "Unsorted"
        root_songs = sorted([os.path.join(root, f) for f in os.listdir(root)
                            if f.lower().endswith(EXTS)],
                           key=lambda x: os.path.basename(x).lower())
        if root_songs or not lib:
            lib["Unsorted"] = root_songs
            
        # Compute "All Music"
        all_songs = []
        for songs in lib.values():
            all_songs.extend(songs)
        # Unique and sorted
        all_songs = sorted(list(set(all_songs)), key=lambda x: os.path.basename(x).lower())
        
        # Finalize structure
        self.library = {"All Music": all_songs}
        self.library.update(lib)
        self.playlists = sorted([k for k in self.library.keys() if k != "All Music" and k != "Unsorted"], key=lambda s: s.lower())
        # Canonical order
        self.playlists = ["All Music", "Unsorted"] + self.playlists
        
        if self.current_playlist not in self.library:
            self.current_playlist = "All Music"

    # ── AUDIO EVENT HANDLERS (Pygame Backend) ──────────────────────────────────
    track_start_time = 0.0
    seek_offset_sec = 0.0
    paused = False

    def _audio_poll_loop(self):
        while self.audio_thread_running:
            time.sleep(0.1)
            if not self.pygame or not self.playing or self.paused:
                continue
            
            elapsed_sec = time.time() - self.track_start_time
            self.position_ms = int(elapsed_sec * 1000)
            
            if not self.seeking:
                # Thread-safe scrubber update
                self.update_scrubber()
            
            # Check for track completion
            if (self.duration_ms > 0 and self.position_ms >= self.duration_ms) or not self.pygame.mixer.music.get_busy():
                self.playing = False
                self.page.run_task(self._handle_pygame_completed)

    async def _handle_pygame_completed(self, e=None):
        self.handle_track_completed()

    def handle_track_completed(self):
        if self.repeat_mode == "one":
            self.play_current()
        else:
            self.next_track()

    # ── METADATA UTILS ────────────────────────────────────────────────────────
    @staticmethod
    def split_track_name(filepath):
        name = os.path.splitext(os.path.basename(filepath))[0]
        if " - " in name:
            parts = name.split(" - ", 1)
            return parts[1].strip(), parts[0].strip()  # title, artist
        return name, "Unknown Artist"

    def get_track_metadata(self, filepath):
        if filepath in self.metadata_cache:
            return self.metadata_cache[filepath]
            
        title, artist = self.split_track_name(filepath)
        art_bytes = None
        
        try:
            if filepath.lower().endswith(".mp3"):
                from mutagen.mp3 import MP3
                from mutagen.id3 import ID3
                audio = MP3(filepath)
                # Try ID3 tags
                try:
                    tags = ID3(filepath)
                    if 'TIT2' in tags:
                        title = str(tags['TIT2'])
                    if 'TPE1' in tags:
                        artist = str(tags['TPE1'])
                    for tag in tags.values():
                        if tag.FrameID == 'APIC':
                            art_bytes = tag.data
                            break
                except Exception:
                    pass
            elif filepath.lower().endswith((".m4a", ".mp4")):
                from mutagen.mp4 import MP4
                audio = MP4(filepath)
                if '\xa9nam' in audio:
                    title = audio['\xa9nam'][0]
                if '\xa9ART' in audio:
                    artist = audio['\xa9ART'][0]
                if 'covr' in audio:
                    art_bytes = audio['covr'][0]
        except Exception as ex:
            print(f"Error parsing tags: {ex}")
            
        res = (title, artist, art_bytes)
        self.metadata_cache[filepath] = res
        return res

    # ── PLAYBACK ENGINE ───────────────────────────────────────────────────────
    def play_track(self, filepath):
        self.current_track = filepath
        # Build queue from context if not already matching
        context_tracks = self.get_active_tracks()
        if filepath in context_tracks:
            self.queue = list(context_tracks)
            if self.shuffle:
                rest = [t for t in self.queue if t != filepath]
                random.shuffle(rest)
                self.queue = [filepath] + rest
                self.queue_idx = 0
            else:
                try:
                    self.queue_idx = self.queue.index(filepath)
                except ValueError:
                    self.queue_idx = 0
        else:
            self.queue = [filepath]
            self.queue_idx = 0
            
        self.play_current()

    def play_current(self):
        if self.queue_idx < 0 or self.queue_idx >= len(self.queue):
            return
        
        track_path = self.queue[self.queue_idx]
        self.current_track = track_path
        
        if not os.path.exists(track_path):
            print(f"File missing: {track_path}")
            self.next_track()
            return

        # 1. Determine duration using Mutagen
        self.duration_ms = 0
        try:
            from mutagen.mp3 import MP3
            audio_info = MP3(track_path)
            self.duration_ms = int(audio_info.info.length * 1000)
        except Exception:
            try:
                import mutagen
                audio_info = mutagen.File(track_path)
                if audio_info and audio_info.info:
                    self.duration_ms = int(audio_info.info.length * 1000)
            except Exception:
                pass

        # 2. Play using pygame
        if self.pygame:
            try:
                self.pygame.mixer.music.load(track_path)
                self.pygame.mixer.music.set_volume(self.volume)
                self.pygame.mixer.music.play()
                self.track_start_time = time.time()
                self.seek_offset_sec = 0.0
                self.playing = True
                self.paused = False
            except Exception as e:
                print(f"Pygame playback error: {e}")
                self.next_track()
                return
        
        # Reset position tracker in local memory
        self.position_ms = 0
        
        # Load and display metadata
        title, artist, art_bytes = self.get_track_metadata(track_path)
        
        # Update metadata across all views
        self.update_metadata_ui(title, artist, art_bytes)

        # Update play icon states
        self.play_btn.icon = ft.Icons.PAUSE_CIRCLE_FILLED
        self.mobile_play_btn.icon = ft.Icons.PAUSE
        self.player_tab_play_btn.icon = ft.Icons.PAUSE_CIRCLE_FILLED
        self.page.update()

    def toggle_play(self, e=None):
        if not self.current_track and self.get_active_tracks():
            # Play first song in list
            self.play_track(self.get_active_tracks()[0])
            return
            
        if self.playing and not self.paused:
            if self.pygame:
                self.pygame.mixer.music.pause()
            self.paused = True
            self.seek_offset_sec = time.time() - self.track_start_time
        elif self.playing and self.paused:
            if self.pygame:
                self.pygame.mixer.music.unpause()
            self.paused = False
            self.track_start_time = time.time() - self.seek_offset_sec
        else:
            self.play_current()
            
        self.playing = True
        self.play_btn.icon = ft.Icons.PLAY_CIRCLE_FILLED if self.paused else ft.Icons.PAUSE_CIRCLE_FILLED
        self.mobile_play_btn.icon = ft.Icons.PLAY_ARROW if self.paused else ft.Icons.PAUSE
        self.player_tab_play_btn.icon = ft.Icons.PLAY_CIRCLE_FILLED if self.paused else ft.Icons.PAUSE_CIRCLE_FILLED
        self.page.update()

    def next_track(self, e=None):
        if not self.queue:
            return
            
        self.queue_idx += 1
        if self.queue_idx >= len(self.queue):
            if self.repeat_mode == "all":
                self.queue_idx = 0
            else:
                self.queue_idx = len(self.queue) - 1
                self.playing = False
                if self.pygame:
                    self.pygame.mixer.music.stop()
                self.position_ms = 0
                self.update_scrubber()
                return
                
        self.play_current()

    def prev_track(self, e=None):
        if not self.queue:
            return
            
        # Re-play track if we are past 3 seconds
        if self.position_ms > 3000:
            if self.pygame:
                try:
                    self.pygame.mixer.music.play(start=0)
                except Exception:
                    self.pygame.mixer.music.play()
            self.track_start_time = time.time()
            self.seek_offset_sec = 0.0
            self.position_ms = 0
            self.update_scrubber()
            return
            
        self.queue_idx -= 1
        if self.queue_idx < 0:
            if self.repeat_mode == "all":
                self.queue_idx = len(self.queue) - 1
            else:
                self.queue_idx = 0
                
        self.play_current()

    def toggle_shuffle(self, e):
        self.shuffle = not self.shuffle
        self.shuffle_btn.icon_color = "#1DB954" if self.shuffle else ft.Colors.WHITE54
        self.player_tab_shuffle_btn.icon_color = "#1DB954" if self.shuffle else ft.Colors.WHITE54
        
        if self.shuffle and self.current_track:
            rest = [t for t in self.queue if t != self.current_track]
            random.shuffle(rest)
            self.queue = [self.current_track] + rest
            self.queue_idx = 0
        elif not self.shuffle and self.current_track:
            active_tracks = self.get_active_tracks()
            if self.current_track in active_tracks:
                self.queue = list(active_tracks)
                self.queue_idx = self.queue.index(self.current_track)
                
        self.page.update()

    def toggle_repeat(self, e):
        if self.repeat_mode == "off":
            self.repeat_mode = "all"
            self.repeat_btn.icon = ft.Icons.REPEAT
            self.repeat_btn.icon_color = "#1DB954"
            self.player_tab_repeat_btn.icon = ft.Icons.REPEAT
            self.player_tab_repeat_btn.icon_color = "#1DB954"
        elif self.repeat_mode == "all":
            self.repeat_mode = "one"
            self.repeat_btn.icon = ft.Icons.REPEAT_ONE
            self.repeat_btn.icon_color = "#1DB954"
            self.player_tab_repeat_btn.icon = ft.Icons.REPEAT_ONE
            self.player_tab_repeat_btn.icon_color = "#1DB954"
        else:
            self.repeat_mode = "off"
            self.repeat_btn.icon = ft.Icons.REPEAT
            self.repeat_btn.icon_color = ft.Colors.WHITE54
            self.player_tab_repeat_btn.icon = ft.Icons.REPEAT
            self.player_tab_repeat_btn.icon_color = ft.Colors.WHITE54
            
        self.page.update()

    def seek_audio(self, e):
        self.seeking = False
        target_ms = int((e.control.value / 100.0) * self.duration_ms)
        target_sec = target_ms / 1000.0
        if self.pygame and self.playing:
            try:
                self.pygame.mixer.music.play(start=target_sec)
                if self.paused:
                    self.pygame.mixer.music.pause()
            except Exception as ex:
                print(f"Error seeking: {ex}")
                try:
                    self.pygame.mixer.music.play()
                except Exception:
                    pass
        
        if self.paused:
            self.seek_offset_sec = target_sec
        else:
            self.track_start_time = time.time() - target_sec
        self.position_ms = target_ms
        self.update_scrubber()

    def change_volume(self, e):
        self.volume = e.control.value / 100.0
        if self.pygame:
            self.pygame.mixer.music.set_volume(self.volume)
        
        self.volume_slider.value = e.control.value
        self.player_tab_volume_slider.value = e.control.value
        
        if self.volume == 0:
            self.volume_icon.icon = ft.Icons.VOLUME_MUTE
        elif self.volume < 0.4:
            self.volume_icon.icon = ft.Icons.VOLUME_DOWN
        else:
            self.volume_icon.icon = ft.Icons.VOLUME_UP
        self.page.update()
        self.save_config()

    # ── UI HELPERS ────────────────────────────────────────────────────────────
    def get_active_tracks(self):
        tracks = self.library.get(self.current_playlist, [])
        if self.last_search_query:
            query = self.last_search_query.lower()
            filtered = []
            for path in tracks:
                title, artist = self.split_track_name(path)
                if query in title.lower() or query in artist.lower():
                    filtered.append(path)
            return filtered
        return tracks

    @staticmethod
    def format_time_ms(ms):
        if not ms or ms < 0:
            return "0:00"
        seconds = int(ms / 1000)
        minutes = seconds // 60
        seconds = seconds % 60
        return f"{minutes}:{seconds:02d}"

    def update_scrubber(self):
        new_time_cur = self.format_time_ms(self.position_ms)
        if getattr(self, "_last_time_cur", None) == new_time_cur and not self.seeking:
            return
        self._last_time_cur = new_time_cur
        
        if self.duration_ms > 0:
            pct = min((self.position_ms / self.duration_ms) * 100.0, 100.0)
            self.scrub_slider.value = pct
            self.player_tab_scrub.value = pct
        else:
            self.scrub_slider.value = 0
            self.player_tab_scrub.value = 0
            
        self.time_cur.value = self.format_time_ms(self.position_ms)
        self.time_tot.value = self.format_time_ms(self.duration_ms)
        
        self.player_tab_time_cur.value = self.format_time_ms(self.position_ms)
        self.player_tab_time_tot.value = self.format_time_ms(self.duration_ms)
        self.page.update()

    def update_metadata_ui(self, title, artist, art_bytes):
        # Update Main Bottom Bar Player
        self.player_title.value = title
        self.player_artist.value = artist
        self.mobile_player_title.value = title
        self.mobile_player_artist.value = artist
        
        # Update Player Tab / Fullscreen Page
        self.player_tab_title.value = title
        self.player_tab_artist.value = artist
        
        if art_bytes:
            self.player_art.gradient = None
            self.player_art.content = ft.Image(
                src=art_bytes,
                width=56,
                height=56,
                fit=ft.BoxFit.COVER,
                border_radius=4
            )
            
            self.mobile_player_art.gradient = None
            self.mobile_player_art.content = ft.Image(
                src=art_bytes,
                width=40,
                height=40,
                fit=ft.BoxFit.COVER,
                border_radius=4
            )
            
            self.player_tab_art.gradient = None
            self.player_tab_art.content = ft.Image(
                src=art_bytes,
                width=280,
                height=280,
                fit=ft.BoxFit.COVER,
                border_radius=8
            )
        else:
            # Draw gradient container fallback
            self.player_art.gradient = ft.LinearGradient(
                colors=["#1F1F1F", "#2A2A2A"],
                begin=ft.Alignment.TOP_LEFT,
                end=ft.Alignment.BOTTOM_RIGHT
            )
            self.player_art.content = ft.Icon(ft.Icons.MUSIC_NOTE, color=ft.Colors.WHITE30, size=20)
            
            self.mobile_player_art.gradient = ft.LinearGradient(
                colors=["#1F1F1F", "#2A2A2A"],
                begin=ft.Alignment.TOP_LEFT,
                end=ft.Alignment.BOTTOM_RIGHT
            )
            self.mobile_player_art.content = ft.Icon(ft.Icons.MUSIC_NOTE, color=ft.Colors.WHITE30, size=16)
            
            self.player_tab_art.gradient = ft.LinearGradient(
                colors=["#1F1F1F", "#2A2A2A"],
                begin=ft.Alignment.TOP_LEFT,
                end=ft.Alignment.BOTTOM_RIGHT
            )
            self.player_tab_art.content = ft.Icon(ft.Icons.MUSIC_NOTE, color=ft.Colors.WHITE30, size=96)
            
        # Highlight active row in track list
        self.refresh_track_list()
        self.page.update()

    # ── UI BUILDING ───────────────────────────────────────────────────────────
    def build_ui(self):
        # 1. SIDEBAR (Desktop layout)
        self.sidebar_logo = ft.Container(
            content=ft.Row([
                ft.Text("Vibe", font_family="Outfit", size=24, weight=ft.FontWeight.BOLD, color=ft.Colors.WHITE),
                ft.Text("▮", size=10, color="#1DB954")
            ], spacing=2, vertical_alignment=ft.CrossAxisAlignment.END),
            padding=ft.Padding.only(left=24, top=24, bottom=16)
        )
        
        self.nav_library_btn = ft.Container(
            content=ft.Row([
                ft.Icon(ft.Icons.LIBRARY_MUSIC, color=ft.Colors.WHITE),
                ft.Text("Library", size=15, color=ft.Colors.WHITE, weight=ft.FontWeight.W_500)
            ]),
            on_click=lambda e: self.switch_tab("library"),
            padding=ft.Padding.symmetric(horizontal=24, vertical=12),
            border_radius=8,
            bgcolor="#1E1E1E"
        )
        
        self.nav_player_btn = ft.Container(
            content=ft.Row([
                ft.Icon(ft.Icons.PLAY_CIRCLE_OUTLINE, color=ft.Colors.WHITE54),
                ft.Text("Now Playing", size=15, color=ft.Colors.WHITE54, weight=ft.FontWeight.W_500)
            ]),
            on_click=lambda e: self.switch_tab("player"),
            padding=ft.Padding.symmetric(horizontal=24, vertical=12),
            border_radius=8
        )
        
        self.nav_settings_btn = ft.Container(
            content=ft.Row([
                ft.Icon(ft.Icons.SETTINGS, color=ft.Colors.WHITE54),
                ft.Text("Settings", size=15, color=ft.Colors.WHITE54, weight=ft.FontWeight.W_500)
            ]),
            on_click=lambda e: self.switch_tab("settings"),
            padding=ft.Padding.symmetric(horizontal=24, vertical=12),
            border_radius=8
        )
        
        self.sidebar_playlist_list = ft.ListView(
            expand=True,
            spacing=4,
            padding=ft.Padding.only(left=8, right=8)
        )
        
        self.sidebar = ft.Container(
            width=240,
            bgcolor="#090909",
            content=ft.Column([
                self.sidebar_logo,
                ft.Container(
                    content=ft.Column([
                        self.nav_library_btn,
                        self.nav_player_btn,
                        self.nav_settings_btn
                    ], spacing=2),
                    padding=ft.Padding.symmetric(horizontal=8)
                ),
                ft.Divider(height=20, color=ft.Colors.WHITE10),
                ft.Container(
                    content=ft.Text("PLAYLISTS", size=11, color=ft.Colors.WHITE38, weight=ft.FontWeight.BOLD),
                    padding=ft.Padding.only(left=24, bottom=8)
                ),
                self.sidebar_playlist_list
            ], spacing=8),
            visible=True
        )

        # 2. MAIN WORKSPACE (Holds active View)
        # Search component
        self.search_bar = ft.TextField(
            hint_text="Filter tracks...",
            prefix_icon=ft.Icons.SEARCH,
            bgcolor="#1E1E1E",
            border_radius=20,
            border_width=0,
            content_padding=ft.Padding.symmetric(horizontal=16),
            height=40,
            expand=True,
            on_change=self.on_search_change,
            text_size=14
        )
        
        self.playlist_header_title = ft.Text("All Music", font_family="Outfit", size=32, weight=ft.FontWeight.BOLD)
        self.playlist_header_desc = ft.Text("0 tracks", size=13, color=ft.Colors.WHITE38)
        
        self.playlist_header_art = ft.Container(
            width=150,
            height=150,
            border_radius=8,
            gradient=ft.LinearGradient(
                colors=["#1DB954", "#158a3e"],
                begin=ft.Alignment.TOP_LEFT,
                end=ft.Alignment.BOTTOM_RIGHT
            ),
            content=ft.Text("♫", size=60, color=ft.Colors.WHITE, weight=ft.FontWeight.BOLD),
            alignment=ft.Alignment.CENTER
        )
        
        self.playlist_header = ft.Container(
            content=ft.Row([
                self.playlist_header_art,
                ft.Column([
                    ft.Text("PLAYLIST", font_family="Outfit", size=11, weight=ft.FontWeight.BOLD, color=ft.Colors.WHITE38),
                    self.playlist_header_title,
                    self.playlist_header_desc
                ], spacing=4, alignment=ft.MainAxisAlignment.CENTER)
            ], spacing=20, vertical_alignment=ft.CrossAxisAlignment.CENTER),
            padding=ft.Padding.only(left=16, right=16, top=32, bottom=16),
            gradient=ft.LinearGradient(
                colors=["#1DB95422", "#121212"],
                begin=ft.Alignment.TOP_CENTER,
                end=ft.Alignment.BOTTOM_CENTER
            )
        )
        
        self.track_listview = ft.ListView(
            expand=True,
            spacing=0,
            padding=ft.Padding.only(bottom=20)
        )
        
        # Library Tab View
        self.library_view = ft.Column([
            ft.Container(
                content=ft.Row([
                    self.search_bar
                ]),
                padding=ft.Padding.only(left=16, right=16, top=16)
            ),
            self.playlist_header,
            self.track_listview
        ], spacing=0, expand=True)

        # Settings Tab View
        self.settings_music_path = ft.TextField(
            label="Library Folder Path",
            value=self.music_root,
            bgcolor="#1E1E1E",
            border_radius=8,
            border_width=1,
            border_color=ft.Colors.WHITE10,
            height=50,
            text_size=14
        )
        
        self.settings_view = ft.Container(
            content=ft.Column([
                ft.Text("Settings", font_family="Outfit", size=28, weight=ft.FontWeight.BOLD),
                ft.Divider(height=20, color=ft.Colors.WHITE10),
                ft.Text("Storage", size=16, weight=ft.FontWeight.BOLD, color=ft.Colors.WHITE),
                self.settings_music_path,
                ft.Row([
                    ft.ElevatedButton(
                        "Save Path & Rescan",
                        icon=ft.Icons.SAVE,
                        bgcolor="#1DB954",
                        color=ft.Colors.WHITE,
                        on_click=self.save_settings
                    )
                ], alignment=ft.MainAxisAlignment.END),
                ft.Divider(height=40, color=ft.Colors.WHITE10),
                ft.Text("App Status", size=16, weight=ft.FontWeight.BOLD, color=ft.Colors.WHITE),
                ft.Container(
                    bgcolor="#1E1E1E",
                    padding=16,
                    border_radius=8,
                    content=ft.Column([
                        ft.Text(f"Config stored: {CONFIG_FILE}", size=13, color=ft.Colors.WHITE54),
                        ft.Text(f"Vibe Music Player v4.0.0 (Flet)", size=13, color=ft.Colors.WHITE54)
                    ], spacing=8)
                )
            ], spacing=16),
            padding=24,
            expand=True,
            visible=False
        )

        # Now Playing Tab View (Spotify Mobile center artwork style)
        self.player_tab_art = ft.Container(
            width=280,
            height=280,
            border_radius=8,
            gradient=ft.LinearGradient(
                colors=["#1F1F1F", "#2A2A2A"],
                begin=ft.Alignment.TOP_LEFT,
                end=ft.Alignment.BOTTOM_RIGHT
            ),
            content=ft.Icon(ft.Icons.MUSIC_NOTE, color=ft.Colors.WHITE30, size=96),
            alignment=ft.Alignment.CENTER
        )
        
        self.player_tab_title = ft.Text("Nothing playing", size=22, weight=ft.FontWeight.BOLD, overflow=ft.TextOverflow.ELLIPSIS, text_align=ft.TextAlign.CENTER)
        self.player_tab_artist = ft.Text("", size=14, color=ft.Colors.WHITE54, text_align=ft.TextAlign.CENTER)
        
        self.player_tab_time_cur = ft.Text("0:00", size=12, color=ft.Colors.WHITE38)
        self.player_tab_time_tot = ft.Text("0:00", size=12, color=ft.Colors.WHITE38)
        self.player_tab_scrub = ft.Slider(
            min=0, max=100, value=0,
            active_color="#1DB954",
            inactive_color=ft.Colors.WHITE10,
            on_change=self.on_scrub_change,
            on_change_end=self.seek_audio
        )
        
        # Shared player control icons duplicated for Now Playing tab
        self.player_tab_play_btn = ft.IconButton(
            icon=ft.Icons.PLAY_CIRCLE_FILLED,
            icon_size=56,
            icon_color=ft.Colors.WHITE,
            on_click=self.toggle_play
        )
        self.player_tab_shuffle_btn = ft.IconButton(
            icon=ft.Icons.SHUFFLE,
            icon_size=20,
            icon_color=ft.Colors.WHITE54,
            on_click=self.toggle_shuffle
        )
        self.player_tab_prev_btn = ft.IconButton(
            icon=ft.Icons.SKIP_PREVIOUS,
            icon_size=28,
            icon_color=ft.Colors.WHITE,
            on_click=self.prev_track
        )
        self.player_tab_next_btn = ft.IconButton(
            icon=ft.Icons.SKIP_NEXT,
            icon_size=28,
            icon_color=ft.Colors.WHITE,
            on_click=self.next_track
        )
        self.player_tab_repeat_btn = ft.IconButton(
            icon=ft.Icons.REPEAT,
            icon_size=20,
            icon_color=ft.Colors.WHITE54,
            on_click=self.toggle_repeat
        )

        self.player_tab_volume_slider = ft.Slider(
            min=0, max=100, value=70,
            active_color=ft.Colors.WHITE54,
            inactive_color=ft.Colors.WHITE10,
            on_change=self.change_volume,
            width=200
        )
        self.player_tab_volume_row = ft.Row([
            ft.Icon(ft.Icons.VOLUME_DOWN, color=ft.Colors.WHITE54, size=16),
            self.player_tab_volume_slider,
            ft.Icon(ft.Icons.VOLUME_UP, color=ft.Colors.WHITE54, size=16)
        ], alignment=ft.MainAxisAlignment.CENTER, spacing=8, width=320)

        self.player_tab_view = ft.Container(
            content=ft.Column([
                ft.Container(self.player_tab_art, alignment=ft.Alignment.CENTER, padding=ft.Padding.only(top=20, bottom=16)),
                ft.Column([
                    self.player_tab_title,
                    self.player_tab_artist
                ], spacing=4, horizontal_alignment=ft.CrossAxisAlignment.CENTER, width=320),
                ft.Container(
                    content=ft.Column([
                        self.player_tab_scrub,
                        ft.Row([
                            self.player_tab_time_cur,
                            self.player_tab_time_tot
                        ], alignment=ft.MainAxisAlignment.SPACE_BETWEEN)
                    ]),
                    padding=ft.Padding.symmetric(horizontal=16, vertical=8),
                    width=320
                ),
                ft.Row([
                    self.player_tab_shuffle_btn,
                    self.player_tab_prev_btn,
                    self.player_tab_play_btn,
                    self.player_tab_next_btn,
                    self.player_tab_repeat_btn
                ], alignment=ft.MainAxisAlignment.CENTER, spacing=16, width=320),
                ft.Container(height=10),
                self.player_tab_volume_row
            ], horizontal_alignment=ft.CrossAxisAlignment.CENTER),
            expand=True,
            visible=False
        )


        self.main_content = ft.Container(
            bgcolor="#121212",
            content=ft.Stack([
                self.library_view,
                self.settings_view,
                self.player_tab_view
            ]),
            expand=True
        )

        # 3. BOTTOM PLAYER BAR (Desktop Only)
        self.player_art = ft.Container(
            width=56, height=56, border_radius=4,
            gradient=ft.LinearGradient(["#1F1F1F", "#2A2A2A"]),
            content=ft.Icon(ft.Icons.MUSIC_NOTE, color=ft.Colors.WHITE30, size=20)
        )
        self.player_title = ft.Text("Nothing playing", size=14, weight=ft.FontWeight.W_500, overflow=ft.TextOverflow.ELLIPSIS)
        self.player_artist = ft.Text("", size=11, color=ft.Colors.WHITE38, overflow=ft.TextOverflow.ELLIPSIS)
        
        self.track_info_col = ft.Container(
            content=ft.Column([
                self.player_title,
                self.player_artist
            ], spacing=2, alignment=ft.MainAxisAlignment.CENTER),
            width=180
        )
        
        self.play_btn = ft.IconButton(
            icon=ft.Icons.PLAY_CIRCLE_FILLED,
            icon_size=38,
            icon_color=ft.Colors.WHITE,
            on_click=self.toggle_play
        )
        self.shuffle_btn = ft.IconButton(
            icon=ft.Icons.SHUFFLE,
            icon_size=18,
            icon_color=ft.Colors.WHITE54,
            on_click=self.toggle_shuffle
        )
        self.prev_btn = ft.IconButton(
            icon=ft.Icons.SKIP_PREVIOUS,
            icon_size=24,
            icon_color=ft.Colors.WHITE,
            on_click=self.prev_track
        )
        self.next_btn = ft.IconButton(
            icon=ft.Icons.SKIP_NEXT,
            icon_size=24,
            icon_color=ft.Colors.WHITE,
            on_click=self.next_track
        )
        self.repeat_btn = ft.IconButton(
            icon=ft.Icons.REPEAT,
            icon_size=18,
            icon_color=ft.Colors.WHITE54,
            on_click=self.toggle_repeat
        )
        
        self.time_cur = ft.Text("0:00", size=11, color=ft.Colors.WHITE38)
        self.time_tot = ft.Text("0:00", size=11, color=ft.Colors.WHITE38)
        self.scrub_slider = ft.Slider(
            min=0, max=100, value=0,
            active_color="#1DB954",
            inactive_color=ft.Colors.WHITE10,
            on_change=self.on_scrub_change,
            on_change_end=self.seek_audio
        )
        
        self.player_center = ft.Column([
            ft.Row([
                self.shuffle_btn,
                self.prev_btn,
                self.play_btn,
                self.next_btn,
                self.repeat_btn
            ], alignment=ft.MainAxisAlignment.CENTER, spacing=12),
            ft.Container(
                content=ft.Row([
                    self.time_cur,
                    ft.Container(self.scrub_slider, expand=True, padding=0),
                    self.time_tot
                ], spacing=8),
                padding=ft.Padding.only(left=20, right=20)
            )
        ], horizontal_alignment=ft.CrossAxisAlignment.CENTER, spacing=0, width=500)
        
        self.volume_icon = ft.Icon(ft.Icons.VOLUME_UP, color=ft.Colors.WHITE54, size=18)
        self.volume_slider = ft.Slider(
            min=0, max=100, value=70,
            active_color=ft.Colors.WHITE54,
            inactive_color=ft.Colors.WHITE10,
            on_change=self.change_volume,
            width=100
        )
        
        self.player_right = ft.Row([
            self.volume_icon,
            self.volume_slider
        ], spacing=4, alignment=ft.MainAxisAlignment.END, width=150)
        
        self.bottom_player = ft.Container(
            height=90,
            bgcolor="#181818",
            border=ft.Border.only(top=ft.BorderSide(1, "#282828")),
            padding=ft.Padding.symmetric(horizontal=16),
            content=ft.Row([
                ft.Row([self.player_art, self.track_info_col], spacing=12),
                self.player_center,
                self.player_right
            ], alignment=ft.MainAxisAlignment.SPACE_BETWEEN),
            visible=True
        )

        # 4. MOBILE LAYOUT WIDGETS
        # Floating Mini Player (displayed when not on the Now Playing tab)
        self.mobile_player_art = ft.Container(
            width=40, height=40, border_radius=4,
            gradient=ft.LinearGradient(["#1F1F1F", "#2A2A2A"]),
            content=ft.Icon(ft.Icons.MUSIC_NOTE, color=ft.Colors.WHITE30, size=16)
        )
        self.mobile_player_title = ft.Text("Nothing playing", size=13, weight=ft.FontWeight.W_500, overflow=ft.TextOverflow.ELLIPSIS)
        self.mobile_player_artist = ft.Text("", size=11, color=ft.Colors.WHITE38, overflow=ft.TextOverflow.ELLIPSIS)
        
        self.mobile_play_btn = ft.IconButton(
            icon=ft.Icons.PLAY_ARROW,
            icon_size=24,
            icon_color=ft.Colors.WHITE,
            on_click=self.toggle_play
        )
        
        self.mobile_mini_player = ft.Container(
            margin=ft.Margin.all(8),
            padding=ft.Padding.symmetric(horizontal=12, vertical=8),
            border_radius=8,
            bgcolor="#181818",
            content=ft.Row([
                ft.Row([self.mobile_player_art, ft.Column([self.mobile_player_title, self.mobile_player_artist], spacing=1, width=150)], spacing=10),
                ft.Row([
                    self.mobile_play_btn,
                    ft.IconButton(ft.Icons.SKIP_NEXT, icon_size=24, icon_color=ft.Colors.WHITE, on_click=self.next_track)
                ], spacing=4)
            ], alignment=ft.MainAxisAlignment.SPACE_BETWEEN),
            on_click=lambda e: self.switch_tab("player"),
            visible=False
        )
        
        # Bottom Navigation Bar for Mobile
        self.mobile_nav = ft.NavigationBar(
            destinations=[
                ft.NavigationBarDestination(icon=ft.Icons.LIBRARY_MUSIC, label="Library"),
                ft.NavigationBarDestination(icon=ft.Icons.PLAY_CIRCLE_OUTLINE, label="Player"),
                ft.NavigationBarDestination(icon=ft.Icons.SETTINGS, label="Settings")
            ],
            selected_index=0,
            on_change=self.on_mobile_nav_change,
            bgcolor="#090909",
            visible=False
        )

        # Assemble main screen container
        self.content_row = ft.Row([
            self.sidebar,
            self.main_content
        ], spacing=0, expand=True)

        self.root_layout = ft.Column([
            self.content_row,
            self.mobile_mini_player,
            self.bottom_player,
            self.mobile_nav
        ], spacing=0, expand=True)
        
        self.page.add(self.root_layout)

    # ── UI NAVIGATION & ACTIONS ──────────────────────────────────────────────
    def refresh_playlist_header_art(self):
        playlist_name = self.current_playlist
        tracks = self.library.get(playlist_name, [])
        c1, c2 = playlist_gradient(playlist_name)
        
        def set_fallback_art():
            self.playlist_header_art.gradient = ft.LinearGradient(
                colors=[c1, c2],
                begin=ft.Alignment.TOP_LEFT,
                end=ft.Alignment.BOTTOM_RIGHT
            )
            self.playlist_header_art.content = ft.Text(
                "♫",
                size=60,
                color=ft.Colors.WHITE,
                weight=ft.FontWeight.BOLD
            )
            try:
                self.playlist_header_art.update()
            except Exception:
                pass

        # Blend header background
        self.playlist_header.gradient = ft.LinearGradient(
            colors=[f"{c1}33", "#121212"],
            begin=ft.Alignment.TOP_CENTER,
            end=ft.Alignment.BOTTOM_CENTER
        )
        try:
            self.playlist_header.update()
        except Exception:
            pass

        if not tracks:
            set_fallback_art()
            return

        if len(tracks) >= 4:
            target_tracks = tracks[:4]
            loaded_art = [None] * 4
            
            def check_grid_completed():
                valid_arts = [a for a in loaded_art if a]
                if valid_arts:
                    grid_controls = []
                    for idx, art_data in enumerate(loaded_art):
                        if art_data:
                            grid_controls.append(
                                ft.Image(
                                    src=art_data,
                                    width=75,
                                    height=75,
                                    fit=ft.BoxFit.COVER
                                )
                            )
                        else:
                            tc1, tc2 = playlist_gradient(f"{playlist_name}_{idx}")
                            grid_controls.append(
                                ft.Container(
                                    width=75,
                                    height=75,
                                    gradient=ft.LinearGradient(colors=[tc1, tc2]),
                                    alignment=ft.Alignment.CENTER,
                                    content=ft.Text("♫", size=20, color=ft.Colors.WHITE)
                                )
                            )
                    
                    self.playlist_header_art.gradient = None
                    self.playlist_header_art.content = ft.Column([
                        ft.Row([grid_controls[0], grid_controls[1]], spacing=0),
                        ft.Row([grid_controls[2], grid_controls[3]], spacing=0)
                    ], spacing=0)
                else:
                    set_fallback_art()
                try:
                    self.playlist_header_art.update()
                except Exception:
                    pass

            pending = 4
            def make_quadrant_cb(idx):
                def cb(path, art_data):
                    nonlocal pending
                    loaded_art[idx] = art_data
                    pending -= 1
                    if pending == 0:
                        check_grid_completed()
                return cb

            for idx, path in enumerate(target_tracks):
                self.art_cache.get(path, make_quadrant_cb(idx))
                
        else:
            target_path = tracks[0]
            
            def cb(path, art_data):
                if art_data:
                    self.playlist_header_art.gradient = None
                    self.playlist_header_art.content = ft.Image(
                        src=art_data,
                        width=150,
                        height=150,
                        fit=ft.BoxFit.COVER,
                        border_radius=8
                    )
                else:
                    set_fallback_art()
                try:
                    self.playlist_header_art.update()
                except Exception:
                    pass
                    
            self.art_cache.get(target_path, cb)

    def refresh_ui(self):
        # Repopulate sidebar playlists
        self.sidebar_playlist_list.controls.clear()
        for pl in self.playlists:
            is_active = (pl == self.current_playlist)
            self.sidebar_playlist_list.controls.append(
                ft.Container(
                    content=ft.Text(pl, size=13, color=ft.Colors.WHITE if is_active else ft.Colors.WHITE54, weight=ft.FontWeight.BOLD if is_active else ft.FontWeight.NORMAL),
                    on_click=self.on_playlist_click,
                    data=pl,
                    padding=ft.Padding.symmetric(horizontal=16, vertical=8),
                    border_radius=4,
                    bgcolor="#1E1E1E" if is_active else ft.Colors.TRANSPARENT
                )
            )
            
        # Update Library content
        self.playlist_header_title.value = self.current_playlist
        tracks_count = len(self.library.get(self.current_playlist, []))
        self.playlist_header_desc.value = f"{tracks_count} tracks"
        
        self.refresh_playlist_header_art()
        self.refresh_track_list()
        self.on_page_resize() # Re-evaluate screen layouts

    def refresh_track_list(self):
        self.track_listview.controls.clear()
        tracks = self.get_active_tracks()
        pl_name = self.current_playlist
        
        limit = min(len(tracks), getattr(self, "track_limit", 100))
        paginated_tracks = tracks[:limit]
        
        for i, path in enumerate(paginated_tracks):
            title, artist = self.split_track_name(path)
            is_playing = (self.current_track == path)
            
            # Create a beautiful fallback gradient container matching the track name
            c1, _ = playlist_gradient(f"{title}_{artist}")
            album_art_container = ft.Container(
                width=36,
                height=36,
                border_radius=4,
                gradient=ft.LinearGradient(
                    colors=[c1, "#1e1e1e"],
                    begin=ft.Alignment.TOP_LEFT,
                    end=ft.Alignment.BOTTOM_RIGHT
                ),
                alignment=ft.Alignment.CENTER,
                content=ft.Text("♫", size=14, color=ft.Colors.WHITE38, weight=ft.FontWeight.BOLD)
            )
            
            self.track_listview.controls.append(
                ft.Container(
                    content=ft.Row([
                        ft.Row([
                            ft.Icon(ft.Icons.PLAY_ARROW if is_playing else ft.Icons.MUSIC_NOTE, color="#1DB954" if is_playing else ft.Colors.WHITE38, size=16),
                            album_art_container,
                            ft.Column([
                                ft.Text(title, size=14, color="#1DB954" if is_playing else ft.Colors.WHITE, weight=ft.FontWeight.W_500, overflow=ft.TextOverflow.ELLIPSIS),
                                ft.Text(artist, size=11, color=ft.Colors.WHITE38, overflow=ft.TextOverflow.ELLIPSIS)
                            ], spacing=2, width=300)
                        ], spacing=12),
                        ft.IconButton(
                            icon=ft.Icons.MORE_VERT,
                            icon_size=16,
                            icon_color=ft.Colors.WHITE38
                        )
                    ], alignment=ft.MainAxisAlignment.SPACE_BETWEEN),
                    padding=ft.Padding.symmetric(horizontal=16, vertical=8),
                    on_click=self.on_track_click,
                    data=path,
                    border_radius=4,
                    ink=True
                )
            )

        if len(tracks) > limit:
            self.track_listview.controls.append(
                ft.Container(
                    content=ft.Row([
                        ft.Text(f"Showing {limit} of {len(tracks)} tracks. Tap here to load more...", color="#1DB954", size=14, weight=ft.FontWeight.BOLD)
                    ], alignment=ft.MainAxisAlignment.CENTER),
                    padding=ft.Padding.symmetric(vertical=16),
                    on_click=self.load_more_tracks,
                    border_radius=4,
                    ink=True
                )
            )

    def load_more_tracks(self, e):
        self.track_limit += 100
        self.refresh_track_list()
        self.page.update()

    def on_playlist_click(self, e):
        self.current_playlist = e.control.data
        self.track_limit = 100
        self.refresh_ui()

    def on_track_click(self, e):
        self.play_track(e.control.data)

    def on_search_change(self, e):
        self.last_search_query = e.control.value
        self.track_limit = 100
        self.refresh_track_list()
        self.page.update()

    def on_scrub_change(self, e):
        self.seeking = True

    def switch_tab(self, tab_name):
        self.library_view.visible = (tab_name == "library")
        self.settings_view.visible = (tab_name == "settings")
        self.player_tab_view.visible = (tab_name == "player")
        
        # Update Nav Sidebar styling
        self.nav_library_btn.bgcolor = "#1E1E1E" if tab_name == "library" else ft.Colors.TRANSPARENT
        self.nav_library_btn.content.controls[0].color = ft.Colors.WHITE if tab_name == "library" else ft.Colors.WHITE54
        self.nav_library_btn.content.controls[1].color = ft.Colors.WHITE if tab_name == "library" else ft.Colors.WHITE54
        
        self.nav_player_btn.bgcolor = "#1E1E1E" if tab_name == "player" else ft.Colors.TRANSPARENT
        self.nav_player_btn.content.controls[0].color = ft.Colors.WHITE if tab_name == "player" else ft.Colors.WHITE54
        self.nav_player_btn.content.controls[1].color = ft.Colors.WHITE if tab_name == "player" else ft.Colors.WHITE54
        
        self.nav_settings_btn.bgcolor = "#1E1E1E" if tab_name == "settings" else ft.Colors.TRANSPARENT
        self.nav_settings_btn.content.controls[0].color = ft.Colors.WHITE if tab_name == "settings" else ft.Colors.WHITE54
        self.nav_settings_btn.content.controls[1].color = ft.Colors.WHITE if tab_name == "settings" else ft.Colors.WHITE54
        
        # Handle Mini Player visibility on mobile
        self.update_mini_player_visibility(tab_name)
        self.page.update()

    def update_mini_player_visibility(self, tab_name):
        width = self.page.width if (self.page.width is not None and self.page.width > 0) else 1100
        if width < 600:
            self.mobile_mini_player.visible = (tab_name != "player" and self.current_track is not None)
        else:
            self.mobile_mini_player.visible = False

    def on_mobile_nav_change(self, e):
        idx = e.control.selected_index
        tabs = ["library", "player", "settings"]
        self.switch_tab(tabs[idx])

    def on_page_resize(self, e=None):
        width = self.page.width if (self.page.width is not None and self.page.width > 0) else 1100
        
        # Adaptive responsive visibility thresholds
        if width < 600:
            # Mobile View Mode
            self.sidebar.visible = False
            self.bottom_player.visible = False
            self.mobile_nav.visible = True
            
            # Match Nav Bar state
            current_idx = self.mobile_nav.selected_index
            tabs = ["library", "player", "settings"]
            self.switch_tab(tabs[current_idx])
        else:
            # Desktop View Mode
            self.sidebar.visible = True
            self.bottom_player.visible = True
            self.mobile_nav.visible = False
            self.mobile_mini_player.visible = False
            
            # Auto fallback if Player tab was selected
            if self.player_tab_view.visible:
                self.switch_tab("library")
                
        self.page.update()

    def save_settings(self, e):
        path = self.settings_music_path.value.strip()
        if os.path.exists(path):
            self.music_root = path
            self.save_config()
            self.scan_library()
            self.refresh_ui()
            self.page.show_snack_bar(ft.SnackBar(ft.Text("Path updated and scanned!"), open=True))
        else:
            self.page.show_snack_bar(ft.SnackBar(ft.Text("Invalid folder path!"), open=True))

def main(page: ft.Page):
    FletVibePlayer(page)

if __name__ == "__main__":
    ft.run(main)
