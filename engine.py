import os
import time
import random
import threading
from utils import format_time, split_track_name
from library_tools import find_duplicates, parse_exportify_csv, scan_library
from discovery import DiscoveryService
from source_lookup import YT_DLP_AVAILABLE, probe_soundcloud, yt_dlp

try:
    from mutagen import File as MutagenFile
    from mutagen.mp3 import MP3  # Keep for backward compat
    MUTAGEN_AVAILABLE = True
except ImportError:
    MutagenFile = None
    MP3 = None
    MUTAGEN_AVAILABLE = False

try:
    import pygame
    PYGAME_AVAILABLE = True
except ImportError:
    pygame = None
    PYGAME_AVAILABLE = False

class LunoEngine:
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
        self._original_queue = []
        self.queue_idx = -1
        self.user_queue_count = 0
        self.playing = False
        self.paused = False
        self.duration = 0.0
        self.track_start = 0.0
        self.seek_pos = 0.0
        self._playback_base_pos = 0.0
        self._playback_base_mono = 0.0
        self._last_position = 0.0
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
        # Clear stale footer position immediately while new track loads
        self.c.scrub.set(0)
        self.c.time_cur.config(text=format_time(0.0))
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
            self.track_start = time.monotonic()
            self.seek_pos = 0.0
            self._playback_base_pos = 0.0
            self._playback_base_mono = self.track_start
            self._last_position = 0.0
            
            # Generic duration probing via Mutagen (supports mp3, m4a, flac, etc.)
            try:
                audio = MutagenFile(path) if MUTAGEN_AVAILABLE else None
                self.duration = audio.info.length if audio and audio.info else 0.0
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
            # Mobile recoverFromPlaybackError parity: let the controller mark
            # a failed preview row and drop its file before the auto-skip.
            if hasattr(self.c, "_on_playback_failed"):
                try:
                    self.c._on_playback_failed(path)
                except Exception:
                    pass
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
            now = time.monotonic()
            mixer_elapsed = self._mixer_elapsed_seconds()
            self._playback_base_pos = self.seek_pos - mixer_elapsed if mixer_elapsed is not None else self.seek_pos
            self._playback_base_mono = now
            self.track_start = now - self.seek_pos
            self.c.play_btn.set_text("⏸")
            if self.queue and 0 <= self.queue_idx < len(self.queue):
                title, artist = split_track_name(self.queue[self.queue_idx])
                self.c._update_now_playing_info(title, artist, self.duration)
        elif self.playing:
            self.seek_pos = self.get_position()
            pygame.mixer.music.pause()
            self.paused = True
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
                # Re-shuffle the entire set if we loop in shuffle mode, while
                # avoiding an immediate repeat across the cycle boundary.
                previous = (
                    self.queue[self.queue_idx]
                    if 0 <= self.queue_idx < len(self.queue)
                    else None
                )
                new_q = list(pl)
                random.shuffle(new_q)
                if previous is not None and len(new_q) > 1 and new_q[0] == previous:
                    eligible = [
                        i for i in range(1, len(new_q)) if new_q[i] != previous
                    ]
                    if eligible:
                        swap_idx = random.choice(eligible)
                        new_q[0], new_q[swap_idx] = new_q[swap_idx], new_q[0]
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

    def get_position(self):
        """Canonical playback position in seconds, clamped to [0, duration].
        Prefers pygame's mixer playback clock while audio is busy, with a
        monotonic fallback for startup/end states or unsupported codecs."""
        if not self.playing and not self.paused:
            return 0.0
        if self.paused:
            pos = self.seek_pos
        else:
            mixer_elapsed = self._mixer_elapsed_seconds()
            if mixer_elapsed is not None:
                pos = self._playback_base_pos + mixer_elapsed
            else:
                pos = self._playback_base_pos + (time.monotonic() - self._playback_base_mono)
        pos = self._clamp_position(pos)
        self._last_position = pos
        return pos

    def _mixer_elapsed_seconds(self):
        if not PYGAME_AVAILABLE or not self.mixer_ready:
            return None
        try:
            if not pygame.mixer.music.get_busy():
                return None
            pos_ms = pygame.mixer.music.get_pos()
        except Exception:
            return None
        if pos_ms is None or pos_ms < 0:
            return None
        return pos_ms / 1000.0

    def _clamp_position(self, pos):
        if self.duration > 0:
            pos = min(pos, self.duration)
        return max(0.0, pos)

    def _update_position_display(self, elapsed=None):
        elapsed = self.get_position() if elapsed is None else self._clamp_position(elapsed)
        pct = min((elapsed / self.duration) * 100, 100) if self.duration > 0 else 0.0
        self.c.scrub.set(pct)
        self.c.time_cur.config(text=format_time(elapsed))

        sp = self.c._frames.get("song_page")
        if sp and sp.winfo_ismapped():
            if hasattr(sp, "scrub"): sp.scrub.set(pct)
            if hasattr(sp, "time_cur"): sp.time_cur.config(text=format_time(elapsed))
            if hasattr(sp, "time_tot"): sp.time_tot.config(text=format_time(self.duration))
            if hasattr(sp, "play_btn"): sp.play_btn.set_text("⏸" if not self.paused else "▶")

    def on_scrub_moved(self, val):
        if not PYGAME_AVAILABLE or self.duration <= 0: return
        pos = self._clamp_position((float(val) / 100) * self.duration)
        self.seek_pos = pos
        now = time.monotonic()
        self.track_start = now - pos
        self._playback_base_pos = pos
        self._playback_base_mono = now
        self._last_position = pos
        pygame.mixer.music.play(start=pos)
        if self.paused: pygame.mixer.music.pause()
        self._update_position_display(pos)
        if self.queue and 0 <= self.queue_idx < len(self.queue):
            title, artist = split_track_name(self.queue[self.queue_idx])
            self.c._update_now_playing_info(title, artist, self.duration)

    def on_volume_moved(self, val):
        self.volume = float(val) / 100
        if PYGAME_AVAILABLE: pygame.mixer.music.set_volume(self.volume)

    def _poll(self):
        """Internal playback watchdog with generation latch and mixer guard.

        Every invocation reschedules exactly one successor unless the controller
        is closing. Completion detection uses the canonical playback position
        and works for both known-duration and unknown-duration tracks."""
        try:
            if self.playing and not self.paused and self.mixer_ready:
                elapsed = self.get_position()

                self._update_position_display(elapsed)

                # Completion latch: at most one advance per track generation.
                if self._completion_dispatched:
                    pass  # wait for play_current to reset
                else:
                    # Duration-based completion
                    duration_known = self.duration > 0
                    if duration_known and elapsed >= self.duration:
                        self._completion_dispatched = True
                        if self.repeat: self.play_current()
                        else: self.next_track()
                    # Mixer-idle completion (works for unknown duration too)
                    elif not pygame.mixer.music.get_busy() and elapsed > 0.5:
                        # 0.5s startup grace avoids transient idle after load().
                        self._completion_dispatched = True
                        if self.repeat: self.play_current()
                        else: self.next_track()
        except:
            pass
        # Unconditional reschedule while controller is alive
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
        path = os.path.join(self.c.MUSIC_ROOT, ".luno_allowed_dups.json")
        if os.path.exists(path):
            try:
                with open(path, "r") as f:
                    self.allowed_dupe_keys = set(json.load(f))
            except: pass

    def save_allowed_dupes(self):
        import json
        path = os.path.join(self.c.MUSIC_ROOT, ".luno_allowed_dups.json")
        try:
            with open(path, "w") as f:
                json.dump(list(self.allowed_dupe_keys), f)
        except: pass
