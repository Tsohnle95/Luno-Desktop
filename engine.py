import os
import csv
import re
import time
import random
import threading
import requests
from utils import split_track_name, format_time

class DiscoveryService:
    """Handles fetching similar tracks from Last.fm API with a simplified cache."""
    def __init__(self, api_key=None, logger=None):
        self.api_key = api_key
        self.logger = logger
        self.cache = {} # key: (artist, title), value: list of recommendations

    def get_similar(self, artist, title, limit=20):
        if not self.api_key or not artist or not title:
            return []
        
        cache_key = (artist.lower().strip(), title.lower().strip())
        if cache_key in self.cache:
            return self.cache[cache_key]
            
        url = "https://ws.audioscrobbler.com/2.0/"
        
        # STAGE 1: Try track.getsimilar
        params = {
            "method": "track.getsimilar",
            "artist": artist,
            "track": title,
            "api_key": self.api_key,
            "format": "json",
            "limit": limit
        }
        
        results = []
        try:
            resp = requests.get(url, params=params, timeout=8)
            resp.raise_for_status()
            data = resp.json()
            
            if "error" not in data:
                tracks_data = data.get("similartracks", {}).get("track", [])
                if not isinstance(tracks_data, list): tracks_data = [tracks_data] if tracks_data else []
                
                for t in tracks_data:
                    results.append({
                        "artist": t.get("artist", {}).get("name") if isinstance(t.get("artist"), dict) else t.get("artist"),
                        "title": t.get("name"),
                        "match": float(t.get("match", 0))
                    })

            # STAGE 2: Fallback to artist.gettoptracks if no similar tracks found
            if not results:
                if self.logger: self.logger("Discovery", f"No similar tracks for '{title}'. Fetching popular tracks by {artist}...")
                params["method"] = "artist.gettoptracks"
                del params["track"]
                
                resp = requests.get(url, params=params, timeout=8)
                resp.raise_for_status()
                data = resp.json()
                
                if "error" not in data:
                    tracks_data = data.get("toptracks", {}).get("track", [])
                    if not isinstance(tracks_data, list): tracks_data = [tracks_data] if tracks_data else []
                    
                    for t in tracks_data:
                        results.append({
                            "artist": t.get("artist", {}).get("name") if isinstance(t.get("artist"), dict) else t.get("artist"),
                            "title": t.get("name"),
                            "match": 0.8  # Arbitrary match score for fallback
                        })
            
            # Keep cache small
            if len(self.cache) > 10:
                self.cache.clear()
            
            self.cache[cache_key] = results
            return results
        except Exception as e:
            if self.logger: self.logger("Discovery", f"Request Failed: {e}")
            return []


try:
    from mutagen.mp3 import MP3
    MUTAGEN_AVAILABLE = True
except ImportError:
    MP3 = None
    MUTAGEN_AVAILABLE = False

try:
    import pygame
    PYGAME_AVAILABLE = True
except ImportError:
    pygame = None
    PYGAME_AVAILABLE = False

try:
    import yt_dlp
    YT_DLP_AVAILABLE = True
except ImportError:
    yt_dlp = None
    YT_DLP_AVAILABLE = False

def scan_library(roots):
    lib = {}
    EXTS = (".mp3", ".m4a", ".wav", ".flac", ".mp4", ".webm")
    for root in roots:
        if not os.path.exists(root): continue
        for entry in sorted(os.scandir(root), key=lambda e: e.name.lower()):
            if entry.is_dir():
                songs = sorted([os.path.join(entry.path, f) for f in os.listdir(entry.path)
                               if f.lower().endswith(EXTS)],
                               key=lambda x: os.path.basename(x).lower())
                lib[entry.name] = songs
        root_songs = sorted([os.path.join(root, f) for f in os.listdir(root)
                            if f.lower().endswith(EXTS)],
                           key=lambda x: os.path.basename(x).lower())
        if root_songs:
            if "Unsorted" not in lib: lib["Unsorted"] = []
            existing = set(lib["Unsorted"])
            for s in root_songs:
                if s not in existing: lib["Unsorted"].append(s)
    return lib

def probe_soundcloud(query):
    if not YT_DLP_AVAILABLE: return None, None
    try:
        with yt_dlp.YoutubeDL({"quiet":True,"skip_download":True}) as ydl:
            info = ydl.extract_info(f"scsearch1:{query}", download=False)
        if not info: return None, None
        best = max((fmt.get("abr") or 0 for fmt in info.get("formats",[])
                    if fmt.get("acodec","none") != "none"), default=0)
        return (f"scsearch1:{query}", f"{int(best)}kbps") if best > 180 else (None, None)
    except: return None, None

def parse_exportify_csv(filepath):
    tracks = []
    try:
        with open(filepath, newline="", encoding="utf-8-sig") as f:
            for row in csv.DictReader(f):
                t = row.get("Track Name","").strip()
                a = row.get("Artist Name(s)","").strip()
                if t and a:
                    tracks.append({"title":t,"artist":a,"album":row.get("Album Name","").strip()})
    except Exception as ex: return [], str(ex)
    return tracks, None

def _normalize_for_dupe(text):
    """Aggressive normalization for duplicate detection.
    Strips common YouTube/Spotify suffixes, normalizes feat variants,
    removes all non-alphanumeric chars, and lowercases."""
    s = text.lower()
    # Strip common parenthetical/bracket suffixes
    s = re.sub(r'\((?:official\s*(?:audio|video|music\s*video|lyric\s*video|visualizer)?|'
               r'lyrics?|audio|hd|hq|4k|remaster(?:ed)?|clean|explicit|'
               r'radio\s*edit|extended\s*(?:mix|version)?|music\s*video|'
               r'visualizer|bonus\s*track|deluxe|live|acoustic|remix)\)',
               '', s, flags=re.IGNORECASE)
    s = re.sub(r'\[(?:official\s*(?:audio|video|music\s*video|lyric\s*video|visualizer)?|'
               r'lyrics?|audio|hd|hq|4k|remaster(?:ed)?|clean|explicit|'
               r'radio\s*edit|extended\s*(?:mix|version)?|music\s*video|'
               r'visualizer|bonus\s*track|deluxe|live|acoustic|remix)\]',
               '', s, flags=re.IGNORECASE)
    # Normalize featuring variants
    s = re.sub(r'\b(?:feat\.?|ft\.?|featuring)\b', 'feat', s)
    # Strip file extension artifacts
    s = re.sub(r'\.(mp3|m4a|wav|flac|mp4|webm)$', '', s)
    # Remove ALL non-alphanumeric (keeps letters + digits only)
    s = re.sub(r'[^a-z0-9]', '', s)
    return s

def find_duplicates(library, allowed_keys=None):
    """Groups tracks by normalized key. Returns list of dupe groups,
    each group being a list of {'path', 'pl', 'title', 'artist'} dicts.
    Excludes any keys present in allowed_keys."""
    buckets = {}
    if allowed_keys is None: allowed_keys = set()
    
    for pl_name, tracks in library.items():
        for path in tracks:
            title, artist = split_track_name(path)
            key = _normalize_for_dupe(f"{artist}{title}")
            if not key or key in allowed_keys:
                continue
            entry = {"path": path, "pl": pl_name, "title": title, "artist": artist}
            buckets.setdefault(key, []).append(entry)
    # Only keep groups with 2+ entries
    return [group for group in buckets.values() if len(group) > 1]

class VibeEngine:
    """The central heart of playback, managing audio state and polling."""
    def __init__(self, controller):
        self.c = controller
        
        # Audio Initialization
        self.mixer_ready = False
        if PYGAME_AVAILABLE:
            try:
                pygame.mixer.quit()
                pygame.mixer.pre_init(44100, -16, 2, 2048) 
                pygame.mixer.init()
                self.mixer_ready = True
            except Exception as e:
                self.c._log_error("engine", f"Mixer Init Error: {e}")
                self.mixer_ready = False
        
        # State
        self.queue = []
        self.queue_idx = -1
        self.user_queue_count = 0
        self.playing = False
        self.paused = False
        self.duration = 0.0
        self.track_start = 0.0
        self.seek_pos = 0.0
        self.volume = 0.7
        self.shuffle = False
        self.repeat = False
        self.crossfade_secs = 0
        self.consecutive_fails = 0  # Skip throttle
        
        # Duplicate Exclusion
        self.allowed_dupe_keys = set()
        self._load_allowed_dupes()
        
        # Completion latch: set True before advance/repeat, reset in play_current,
        # checked in _poll to prevent dual-firing across ticks.
        self._completion_dispatched = False
        
        # Start Poll
        self._poll_after_id = None
        self._poll()



    def play_with_context(self, path, context_list=None):
        """Plays a track and attempts to build a queue from its playlist context or provided list."""
        if context_list:
            self.c._queue_playlist_name = "Selection"
            self.c._queue_playlist_paths = list(context_list)
        else:
            for pl_name, tracks in self.c._library.items():
                if path in tracks:
                    self.c._queue_playlist_name = pl_name
                    self.c._queue_playlist_paths = list(tracks)
                    break
        
        # Always refresh queue to the intended playlist context
        self.user_queue_count = 0
        self.queue = list(self.c._queue_playlist_paths)
        self._original_queue = list(self.queue)
        
        if self.shuffle:
            # Scramble future tracks relative to this one
            rest = [p for p in self.queue if p != path]
            random.shuffle(rest)
            self.queue = [path] + rest
            self.queue_idx = 0
        else:
            try:
                self.queue_idx = self.queue.index(path)
            except ValueError:
                self.queue_idx = 0

        self.c._update_queue()
        self.play_current()

    def play_current(self):
        if getattr(self.c, '_closing', False):
            return
        if not PYGAME_AVAILABLE or self.queue_idx < 0 or self.queue_idx >= len(self.queue):
            return
        # Reset completion latch so poll knows a new track generation is active
        self._completion_dispatched = False
        path = self.queue[self.queue_idx]
        if not os.path.exists(path):
            self.consecutive_fails += 1
            if self.consecutive_fails > 5:
                self.c._log_error("engine", f"Stopped: 5+ missing tracks ({os.path.basename(path)})")
                self.playing = False
                return
            
            self.c._log_error("engine", f"Missing: {os.path.basename(path)}")
            # Break synchronous recursion with .after()
            self.c.after(100, self.next_track)
            return
        
        try:
            pygame.mixer.music.load(path)
            pygame.mixer.music.set_volume(self.volume)
            pygame.mixer.music.play()
            
            self.consecutive_fails = 0 # Success!
            self.playing = True
            self.paused = False
            self.track_start = time.time()
            self.seek_pos = 0.0
            
            try:
                audio = MP3(path)
                self.duration = audio.info.length
            except:
                self.duration = 0.0

            # Update UI through controller
            title, artist = split_track_name(path)
            self.c.now_title.config(text=title[:48])
            self.c.now_artist.config(text=artist)
            self.c.play_btn.set_text("⏸")
            self.c.time_tot.config(text=format_time(self.duration))
            self.c.update_player_art(path)
            self.c._update_now_playing_info(title, artist, self.duration)
            
            # Sync Song Page if active
            sp = self.c._frames.get("song_page")
            if sp and hasattr(sp, "update_view"):
                self.c.after(0, sp.update_view)
            
            # History
            if path not in self.c._recently_played:
                self.c._recently_played.insert(0, path)
            else:
                self.c._recently_played.remove(path)
                self.c._recently_played.insert(0, path)
            self.c._recently_played = self.c._recently_played[:100]
            
            self.c._update_queue()
            self.c._frames["library"]._redraw_track_list()
            # Hook for Spotify-style discovery
            self.c._on_track_changed_discovery(path)
        except Exception as e:
            self.c._log_error("engine", f"Playback failed: {e}")
            self.consecutive_fails += 1
            self.playing = False
            self.c.after(100, self.next_track)

    def toggle_shuffle(self):
        self.shuffle = not self.shuffle
        if self.shuffle:
            if self.queue:
                current = self.queue[self.queue_idx] if 0 <= self.queue_idx < len(self.queue) else None
                self._original_queue = list(self.queue)
                # Shuffle ONLY the tracks that are NOT the current one
                rest = [p for i, p in enumerate(self.queue) if i != self.queue_idx]
                random.shuffle(rest)
                self.queue = [current] + rest if current else rest
                self.queue_idx = 0
        else:
            # Restore original order relative to current track
            if self._original_queue:
                current = self.queue[self.queue_idx] if 0 <= self.queue_idx < len(self.queue) else None
                self.queue = list(self._original_queue)
                if current in self.queue:
                    self.queue_idx = self.queue.index(current)
        
        self.c._broadcast_shuffle_state()
        self.c._update_queue()

    def toggle_play(self):
        if not PYGAME_AVAILABLE or not self.mixer_ready: return
        if not self.playing and self.queue:
            if self.queue_idx < 0: self.queue_idx = 0
            self.play_current()
        elif self.paused:
            pygame.mixer.music.unpause()
            self.paused = False
            self.track_start = time.time() - self.seek_pos
            self.c.play_btn.set_text("⏸")
            if self.queue and 0 <= self.queue_idx < len(self.queue):
                title, artist = split_track_name(self.queue[self.queue_idx])
                self.c._update_now_playing_info(title, artist, self.duration)
        elif self.playing:
            pygame.mixer.music.pause()
            self.paused = True
            self.seek_pos = time.time() - self.track_start
            self.c.play_btn.set_text("▶")

    def next_track(self):
        if not self.queue or not self.mixer_ready: return
        
        if getattr(self, 'user_queue_count', 0) > 0:
            self.user_queue_count -= 1
        
        nxt = self.queue_idx + 1
        
        if nxt < len(self.queue):
            self.queue_idx = nxt
            self.play_current()
        elif self.c._queue_playlist_paths:
            # Loop around to the start of the playlist or extend
            pl = self.c._queue_playlist_paths
            if self.shuffle:
                # Re-shuffle the entire set if we loop in shuffle mode
                new_q = list(pl)
                random.shuffle(new_q)
                self.queue = new_q
            else:
                self.queue = list(pl)
            self.queue_idx = 0
            self.c._update_queue()
            self.play_current()
        else:
            self.queue_idx = 0
            self.play_current()

    def prev_track(self):
        if not self.queue or not self.mixer_ready: return
        self.user_queue_count = 0
        self.queue_idx = max(0, self.queue_idx - 1)
        self.play_current()

    def stop(self):
        if not PYGAME_AVAILABLE or not self.mixer_ready: return
        if self.playing or self.paused:
            pygame.mixer.music.stop()
            self.playing = False
            self.paused = False
            self.seek_pos = 0
            self.track_start = 0
            self.c.play_btn.set_text("▶")

    def on_scrub_moved(self, val):
        if not PYGAME_AVAILABLE or self.duration <= 0: return
        pos = (float(val) / 100) * self.duration
        self.seek_pos = pos
        self.track_start = time.time() - pos
        pygame.mixer.music.play(start=pos)
        if self.paused: pygame.mixer.music.pause()

    def on_volume_moved(self, val):
        self.volume = float(val) / 100
        if PYGAME_AVAILABLE: pygame.mixer.music.set_volume(self.volume)

    def _poll(self):
        """Internal playback watchdog with generation latch and mixer guard."""
        try:
            if self.playing and not self.paused and self.mixer_ready:
                elapsed = time.time() - self.track_start
                if self.duration > 0:
                    pct = min((elapsed / self.duration) * 100, 100)
                    self.c.scrub.set(pct)
                    self.c.time_cur.config(text=format_time(elapsed))
                    
                    # Sync Song Page scrubber/time
                    sp = self.c._frames.get("song_page")
                    if sp and sp.winfo_ismapped():
                        if hasattr(sp, "scrub"): sp.scrub.set(pct)
                        if hasattr(sp, "time_cur"): sp.time_cur.config(text=format_time(elapsed))
                        if hasattr(sp, "time_tot"): sp.time_tot.config(text=format_time(self.duration))
                        if hasattr(sp, "play_btn"): sp.play_btn.set_text("⏸")
                    
                    # Completion latch: at most one advance per track generation.
                    if self._completion_dispatched:
                        return  # wait for play_current to reset
                    
                    # Track completion detection
                    if elapsed >= self.duration:
                        self._completion_dispatched = True
                        if self.repeat: self.play_current()
                        else: self.next_track()
                        return  # next _poll handles new state
                    elif not pygame.mixer.music.get_busy() and elapsed > 0.5:
                        # get_busy false before duration (short track, error, end).
                        # 0.5s startup grace avoids transient idle after load().
                        self._completion_dispatched = True
                        if self.repeat: self.play_current()
                        else: self.next_track()
                        return
        except:
            pass
        if not getattr(self.c, '_closing', False):
            self._poll_after_id = self.c.after(400, self._poll)

    def clear_queue(self):
        self.queue = []
        self.queue_idx = -1
        self.user_queue_count = 0
        self.playing = False
        if PYGAME_AVAILABLE: pygame.mixer.music.stop()
        self.c.play_btn.set_text("▶")
        self.c.now_title.config(text="Nothing playing")
        self.c.now_artist.config(text="")
        self.c.update_player_art("")
        self.c.scrub.set(0)
        self.c.time_cur.config(text="0:00")
        self.c._clear_now_playing()
        self.c._update_queue()

    def _load_allowed_dupes(self):
        import json
        path = os.path.join(self.c.MUSIC_ROOT, ".vibe_allowed_dups.json")
        if os.path.exists(path):
            try:
                with open(path, "r") as f:
                    self.allowed_dupe_keys = set(json.load(f))
            except: pass

    def save_allowed_dupes(self):
        import json
        path = os.path.join(self.c.MUSIC_ROOT, ".vibe_allowed_dups.json")
        try:
            with open(path, "w") as f:
                json.dump(list(self.allowed_dupe_keys), f)
        except: pass
