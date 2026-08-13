import tkinter as tk
from theme import BG, BG2, BG3, BG4, TEXT, TEXT2, TEXT3, GREEN, GREEN_HOVER, PLAYER_BG, SIDEBAR
from utils import draw_gradient_rect, split_track_name, hex_to_rgb, rgb_to_hex
from utils import bind_scroll_events, scroll_amount
from theme import playlist_gradient
from widgets import ModernScrollbar, Spinner
import os
import shutil
import subprocess
import time
from collections import deque
from concurrent.futures import ThreadPoolExecutor
import rec_artwork
import threading

# Mobile FullPlayerScreen parity: recommendations load in pages of 25 and can
# grow up to 1,000 results.
REC_PAGE_SIZE = 25
REC_LIMIT_CAP = 1000
PAGE_SB_WIDTH = 8
DARK_BG = "#0f0f0f"
REC_ART_CACHE_LIMIT = 128
REC_ART_SIZE = 48  # mobile 48dp row artwork thumb


class _FFmpegVideoPlayer:
    """Timestamp-driven Tk video renderer.

    The audio engine is the clock. A frame is decoded at the current audio
    position, so pausing and seeking audio also pauses/seeks the video.
    """

    def __init__(self, path, label, width, height, after, on_finished,
                 position_provider, playing_provider):
        self.path = path
        self.label = label
        self.width = max(int(width), 2)
        self.height = max(int(height), 2)
        self.after = after
        self.on_finished = on_finished
        self.position_provider = position_provider
        self.playing_provider = playing_provider
        self.running = False
        self._decoder = None
        self._lock = threading.Lock()
        self._latest_frame = None
        self._video_frames = deque(maxlen=12)
        self._present_after = None
        self._photo = None
        self._source_fps = None
        self._decoder_start_position = None
        self._decoded_frames = 0

    def play(self):
        self.running = True
        self._schedule_present()
        threading.Thread(target=self._render_loop, daemon=True,
                         name="luno-video-frames").start()

    def _render_loop(self):
        from PIL import Image
        fps = self._get_source_fps()
        while self.running:
            if not self.playing_provider():
                self._stop_decoder()
                time.sleep(0.05)
                continue
            position = max(float(self.position_provider()), 0.0)
            if (self._decoder is not None and
                    abs(position - getattr(self, "_last_audio_position", position)) > 0.2):
                # A scrub or track handoff moved the audio clock. Restart the
                # decoder at that authoritative position instead of drifting.
                self._stop_decoder()
            if self._decoder is None:
                self._last_audio_position = position
                self._decoder_start_position = position
                self._decoded_frames = 0
                try:
                    vf = (f"scale={self.width}:{self.height}:force_original_aspect_ratio=decrease,"
                          f"pad={self.width}:{self.height}:(ow-iw)/2:(oh-ih)/2:black")
                    self._decoder = subprocess.Popen([
                        "ffmpeg", "-loglevel", "error", "-ss", f"{position:.3f}",
                        "-i", self.path, "-vf", vf, "-an", "-f", "rawvideo",
                        "-pix_fmt", "rgb24", "pipe:1"],
                        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
                except OSError:
                    self.running = False
                    break
            with self._lock:
                buffered_until = (self._video_frames[-1][0]
                                  if self._video_frames else position)
            if buffered_until > position + 0.25:
                time.sleep(0.005)
                continue
            frame_size = self.width * self.height * 3
            data = self._read_exact(frame_size)
            if len(data) != frame_size:
                self._stop_decoder()
                continue
            timestamp = self._decoder_start_position + (
                self._decoded_frames / fps)
            frame = Image.frombytes("RGB", (self.width, self.height), data)
            with self._lock:
                self._video_frames.append((timestamp, frame))
            self._decoded_frames += 1
            self._last_audio_position = position

    def _get_source_fps(self):
        if self._source_fps:
            return self._source_fps
        try:
            result = subprocess.run([
                "ffprobe", "-v", "error", "-select_streams", "v:0",
                "-show_entries", "stream=avg_frame_rate,r_frame_rate",
                "-of", "default=noprint_wrappers=1:nokey=1", self.path,
            ], capture_output=True, text=True, timeout=5, check=True)
            rates = [line.strip() for line in result.stdout.splitlines() if line.strip()]
            for rate in rates:
                if "/" in rate:
                    numerator, denominator = rate.split("/", 1)
                    value = float(numerator) / float(denominator)
                else:
                    value = float(rate)
                if 1.0 <= value <= 240.0:
                    self._source_fps = value
                    break
        except (OSError, ValueError, subprocess.SubprocessError, ZeroDivisionError):
            pass
        return self._source_fps or 30.0

    def _read_exact(self, size):
        chunks = []
        remaining = size
        stream = self._decoder.stdout if self._decoder else None
        while self.running and stream and remaining:
            chunk = stream.read(remaining)
            if not chunk:
                break
            chunks.append(chunk)
            remaining -= len(chunk)
        return b"".join(chunks)

    def _schedule_present(self):
        if not self.running:
            return
        try:
            # The timestamped queue determines which native-rate frame is due;
            # this only controls how often Tk checks the audio clock.
            self._present_after = self.after(5, self._present)
        except (RuntimeError, tk.TclError):
            self.running = False

    def _present(self):
        self._present_after = None
        position = max(float(self.position_provider()), 0.0)
        image = None
        with self._lock:
            while self._video_frames and self._video_frames[0][0] <= position:
                _, image = self._video_frames.popleft()
        if image is not None:
            self._show(image)
        self._schedule_present()

    def _show(self, image):
        if self.running:
            from PIL import ImageTk
            self._photo = ImageTk.PhotoImage(image)
            self.label.config(image=self._photo, text="", anchor="center",
                              width=self.width, height=self.height)

    def stop(self):
        self.running = False
        if self._present_after is not None:
            try:
                self.after_cancel(self._present_after)
            except (RuntimeError, tk.TclError):
                pass
            self._present_after = None
        self._stop_decoder()
        with self._lock:
            self._video_frames.clear()

    def seek(self):
        """Interrupt a blocked decoder so it restarts at the audio clock."""
        self._stop_decoder()

    def resize(self, width, height):
        width = max(int(width), 2)
        height = max(int(height), 2)
        if (width, height) == (self.width, self.height):
            return
        self.width = width
        self.height = height
        self._stop_decoder()
        self.label.config(width=width, height=height)

    def _stop_decoder(self):
        decoder = self._decoder
        self._decoder = None
        with self._lock:
            self._video_frames.clear()
        if decoder is not None:
            try:
                decoder.terminate()
                decoder.wait(timeout=0.5)
            except (OSError, subprocess.TimeoutExpired):
                try:
                    decoder.kill()
                except OSError:
                    pass

class SongPageView(tk.Frame):
    def __init__(self, parent, controller):
        super().__init__(parent, bg=BG)
        self.controller = controller
        self._bg_photo = None
        self._bg_loading = False
        self._bg_executor = ThreadPoolExecutor(max_workers=1, thread_name_prefix="song-page-art")
        self._bg_future = None
        self._bg_generation = 0
        self._bg_signature = None
        self._art_photo = None      # last good artwork for the current path
        self._art_path = None
        self._video_path = None
        self._video_file = None
        self._fullscreen_window = None
        self._fullscreen_player = None
        self._video_size = None
        self._video_player = None
        self._video_generation = 0
        self._video_loading = False
        self._video_probe_attempt = 0
        self._video_photo = None
        self._video_preload_paths = set()
        self._video_preload_states = {}
        self._video_spinner_after = None
        self._video_menu_hide_after = None
        self._video_spinner_frame = 0
        self._video_key_bindings = []
        self.video_preload_var = tk.BooleanVar(value=False)

        # Recommendation section state (mirrors FullPlayerScreen.kt states)
        self._rec_seed = ("", "")          # (artist, title) the list was seeded for
        self._rec_state = "idle"           # idle | loading | error | ready
        self._rec_phase = "init"           # init | load_more
        self._recs = []                    # last delivered recommendation list
        self._rec_limit = REC_PAGE_SIZE    # currently requested limit
        self._rec_exhausted = False
        self._rec_loading_more = False     # a load-more request is in flight
        self._rec_error = None
        self._rec_load_more_error = None
        self._page_restore_y = None        # yview to restore after a re-render

        # Per-row artwork (mobile RecommendationArtwork parity): cached
        # PhotoImages keyed by the normalized recommendation key, fetched on a
        # bounded executor with a Last.fm URL -> Deezer fallback chain and
        # deterministic abstract designs for loading/missing/error states.
        self._rec_art_cache = {}           # key -> PhotoImage (bounded)
        self._rec_art_pending = {}         # key -> {"url", "artist", "title", "deezer_tried"}
        self._rec_art_executor = ThreadPoolExecutor(
            max_workers=2, thread_name_prefix="luno-rec-art")
        self._rec_art_generation = 0

        # Live row registry for in-place state updates (mobile recomposition
        # parity): controller preview-state pushes refresh only the affected
        # rows instead of destroying and rebuilding the whole section, which
        # is what made the page flash during preview downloads.
        self._rec_rows = {}                # key -> {"frame", "title_lbl", ...}
        self._rec_rows_update_pending = False
        self._rec_summary_lbl = None

        self._build_ui()

    def _build_ui(self):
        # ── BACKGROUND CANVAS ────────────────────────────────────────────────
        self.canvas = tk.Canvas(self, highlightthickness=0, bd=0, bg=BG)
        self.canvas.pack(fill="both", expand=True)
        self.canvas.bind("<Configure>", lambda e: self.update_view())

        # ── OVERLAY CONTAINER (Fills Canvas) ─────────────────────────────────
        self.content = tk.Frame(self.canvas, bg=BG, bd=0)
        self.canvas_window = self.canvas.create_window(0, 0, window=self.content, anchor="nw")

        # ── PAGE SCROLL CANVAS (mobile full-player model: the whole page —
        # artwork, metadata, and recommendations — is one scrollable column)
        self.page_canvas = tk.Canvas(self.content, bg=BG, highlightthickness=0, bd=0)
        self.page_sb = ModernScrollbar(self.content, command=self.page_canvas.yview)
        self.page_canvas.config(yscrollcommand=self.page_sb.set)
        self.page_canvas.pack(side="left", fill="both", expand=True)
        self.page_sb.pack(side="right", fill="y")

        self.page_inner = tk.Frame(self.page_canvas, bg=BG)
        self.page_window = self.page_canvas.create_window(0, 0, window=self.page_inner,
                                                          anchor="nw", tags="pagewin")
        self.page_canvas.bind("<Configure>", lambda e: (
            self.page_canvas.itemconfig("pagewin", width=max(e.width - PAGE_SB_WIDTH, 50)),
        ))

        def _on_wheel(e):
            amount = scroll_amount(e)
            if amount:
                self.page_canvas.yview_scroll(amount, "units")
        bind_scroll_events(self.page_canvas, _on_wheel, add="+", bind_all=True)

        # 1. Main Centered Content (natural height; page canvas scrolls/centers)
        self.main_f = tk.Frame(self.page_inner, bg=BG)
        self.main_f.pack(fill="x")

        self.main_f.columnconfigure(0, weight=1)

        # Wide YouTube-style media stage; dimensions are set responsively.
        self.art_container = tk.Frame(self.main_f, bg=BG2, width=480, height=480,
                                       highlightthickness=2, highlightbackground="#000")
        self.art_container.pack_propagate(False) # Force pixel sizes
        self.art_container.grid(row=1, column=0, pady=(0, 20))

        self.art_lbl = tk.Label(self.art_container, text="♫", font=("Helvetica", 80),
                                 fg=TEXT3, bg=BG2)
        self.art_lbl.pack(fill="both", expand=True)
        for widget in (self.art_container, self.art_lbl):
            widget.bind("<Enter>", lambda e: self._show_video_menu())
            widget.bind("<Leave>", lambda e: self._schedule_video_menu_check())
            widget.bind("<Button-1>", lambda e: self._show_video_menu())
        for sequence, handler in (("<KeyPress-f>", self._handle_video_f_key),
                                  ("<KeyPress-F>", self._handle_video_f_key),
                                  ("<Escape>", self._handle_video_escape)):
            self._video_key_bindings.append(
                (sequence, self.bind_all(sequence, handler, add="+")))
        # Video actions live in a compact hover menu below the media stage.
        self.video_menu_btn = tk.Label(self.main_f, text="🎥", font=("Helvetica Neue", 14),
                                       fg=TEXT3, bg=BG, cursor="hand2")
        self.video_menu_btn.grid(row=2, column=0, sticky="e", padx=12, pady=(0, 8))
        self.video_menu_btn.bind("<Enter>", lambda e: self._show_video_menu())
        self.video_menu_btn.bind("<Leave>", lambda e: self._schedule_video_menu_check())
        self.video_menu_window = tk.Toplevel(self)
        self.video_menu_window.withdraw()
        self.video_menu_window.overrideredirect(True)
        self.video_menu_window.attributes("-topmost", False)
        self.video_menu_window.configure(bg=BG3)
        self.video_menu = tk.Frame(self.video_menu_window, bg=BG3,
                                   bd=1, relief="solid")
        self.video_menu.pack(fill="both", expand=True)
        self.video_menu_play = tk.Label(self.video_menu, text="▷  Play video",
                                        font=("Helvetica Neue", 10), fg=TEXT,
                                        bg=BG3, anchor="w", padx=10, pady=6,
                                        cursor="hand2")
        self.video_menu_play.pack(fill="x")
        self.video_menu_play.bind("<Button-1>", lambda e: self._run_video_menu_action(self._request_video))
        self.video_menu_preload = tk.Checkbutton(
            self.video_menu, text="Preload next", variable=self.video_preload_var,
            command=lambda: self._run_video_menu_action(self._video_preload_changed), font=("Helvetica Neue", 10),
            fg=TEXT, bg=BG3, activebackground=BG3, activeforeground=TEXT,
            selectcolor=BG3, highlightthickness=0, borderwidth=0, anchor="w")
        self.video_menu_preload.pack(fill="x", padx=4, pady=2)
        self.video_menu_spinner = tk.Label(self.video_menu, text="", fg=GREEN,
                                           bg=BG3, font=("Helvetica Neue", 11))
        self.video_menu_spinner.place(relx=1.0, rely=0.5, anchor="e", x=-8)
        self.video_menu_fullscreen = tk.Label(
            self.video_menu, text="⛶  Full screen", font=("Helvetica Neue", 10),
            fg=TEXT, bg=BG3, anchor="w", padx=10, pady=6, cursor="hand2")
        self.video_menu_fullscreen.pack(fill="x")
        self.video_menu_fullscreen.bind("<Button-1>", lambda e: self._run_video_menu_action(self._open_fullscreen_video))
        # Keep playback state updates targeting the menu controls.
        self.video_btn = self.video_menu_play
        self.video_spinner = self.video_menu_spinner
        for widget in (self.video_menu, self.video_menu_play,
                       self.video_menu_preload, self.video_menu_fullscreen):
            widget.bind("<Enter>", lambda e: self._cancel_hide_video_menu())
            widget.bind("<Leave>", lambda e: self._schedule_video_menu_check())

        # Typography
        self.title_lbl = tk.Label(self.main_f, text="Song Title",
                                   font=("Helvetica Neue", 42, "bold"),
                                   fg=TEXT, bg=BG, justify="center")
        self.title_lbl.grid(row=3, column=0, pady=(0, 10))

        def _on_main_resize(e):
            if e.width > 50:
                self.title_lbl.config(wraplength=e.width - 40)
        self.main_f.bind("<Configure>", _on_main_resize)

        self.artist_lbl = tk.Label(self.main_f, text="Artist Name",
                                    font=("Helvetica Neue", 20),
                                    fg=TEXT2, bg=BG, justify="center")
        self.artist_lbl.grid(row=4, column=0, pady=(0, 40))

        self.favorite_btn = tk.Label(
            self.main_f, text="☆  Add to Favorites", font=("Helvetica Neue", 11, "bold"),
            fg=TEXT3, bg=BG, cursor="hand2"
        )
        self.favorite_btn.grid(row=5, column=0, pady=(0, 18))
        self.favorite_btn.bind("<Button-1>", lambda e: self.controller._toggle_current_favorite())
        self.favorite_btn.bind("<Enter>", lambda e: self.favorite_btn.config(fg=TEXT))
        self.favorite_btn.bind("<Leave>", lambda e: self._sync_favorite_button())

        # 3. Recommended for this song (mobile FullPlayerScreen parity) —
        #    rows pack into the page flow so the whole page scrolls together.
        self.recs_wrap = tk.Frame(self.main_f, bg=BG)
        self.recs_wrap.grid(row=6, column=0, sticky="ew", padx=40, pady=(0, 24))

        # Header: title + refresh
        self.recs_hdr = tk.Frame(self.recs_wrap, bg=BG)
        self.recs_hdr.pack(fill="x")
        self.recs_title_lbl = tk.Label(self.recs_hdr, text="Recommended for this song",
                                       font=("Helvetica Neue", 16, "bold"),
                                       fg=TEXT, bg=BG, anchor="w")
        self.recs_title_lbl.pack(side="left")
        self.recs_discover_lbl = tk.Label(self.recs_hdr, text="Start discover mode",
                                          font=("Helvetica Neue", 10, "bold"),
                                          fg=TEXT3, bg=BG, cursor="hand2", padx=8)
        self.recs_discover_lbl.pack(side="right", padx=(0, 6))
        self.recs_discover_lbl.bind("<Button-1>", lambda e: self._toggle_discover())
        self.recs_discover_lbl.bind("<Enter>",
                                    lambda e: self.recs_discover_lbl.config(
                                        fg=GREEN if getattr(self.controller, "_discover_active", False) else TEXT))
        self.recs_discover_lbl.bind("<Leave>", lambda e: self._sync_discover_button())
        self.recs_refresh_lbl = tk.Label(self.recs_hdr, text="⟳", font=("Helvetica Neue", 15),
                                         fg=TEXT3, bg=BG, cursor="hand2", padx=10)
        self.recs_refresh_lbl.pack(side="right")
        self.recs_refresh_lbl.bind("<Button-1>", lambda e: self._refresh_recs())
        self.recs_refresh_lbl.bind("<Enter>", lambda e: self.recs_refresh_lbl.config(fg=TEXT))
        self.recs_refresh_lbl.bind("<Leave>", lambda e: self.recs_refresh_lbl.config(fg=TEXT3))

        # State / message line (rebuilt per state)
        self.recs_status_f = tk.Frame(self.recs_wrap, bg=BG)
        self.recs_status_f.pack(fill="x", pady=(2, 6))

        self._render_recs()

    # ── Page layout / scrolling ─────────────────────────────────────────────

    def _layout_page(self, w, h):
        """Width the page window, center it when content fits the viewport,
        otherwise let the user scroll (mobile full-player behavior)."""
        if w < 10 or h < 10:
            return
        try:
            self.page_inner.update_idletasks()
            inner_h = self.page_inner.winfo_reqheight()
            self.page_canvas.itemconfig("pagewin", width=max(w - PAGE_SB_WIDTH, 50))
            if inner_h <= h:
                self.page_canvas.coords(self.page_window, 0, max((h - inner_h) // 2, 0))
                self.page_canvas.yview_moveto(0.0)
                self._page_restore_y = None
            else:
                self.page_canvas.coords(self.page_window, 0, 0)
                if self._page_restore_y is not None:
                    self.page_canvas.yview_moveto(min(self._page_restore_y, 1.0))
                    self._page_restore_y = None
            self.page_canvas.config(scrollregion=self.page_canvas.bbox("all"))
        except tk.TclError:
            pass

    # ── Recommendations ─────────────────────────────────────────────────────

    def _rec_key(self, r):
        return f"{r.get('artist', '').lower().strip()} - {r.get('title', '').lower().strip()}"

    def _recs_api_key(self):
        var = getattr(self.controller, "lastfm_key_var", None)
        return bool(var and var.get().strip())

    def _toggle_discover(self):
        """Mobile full-player 'Start discover mode' / 'Discover mode: On'."""
        if getattr(self.controller, "_discover_active", False):
            self.controller._discover_set(False)
        else:
            if not self._recs:
                return
            if (not getattr(self.controller, "_discover_discover_mode", True)
                    and getattr(self.controller, "_discover_active", False)):
                return
            self.controller._discover_set(True, list(self._recs))
        self._sync_discover_button()

    def _sync_discover_button(self):
        if not hasattr(self, "recs_discover_lbl"):
            return
        active = bool(getattr(self.controller, "_discover_active", False))
        non_discover_session = (
            active and not getattr(self.controller, "_discover_discover_mode", False))
        enabled = active or (
            not non_discover_session and self._recs_api_key()
            and bool(self._rec_seed[0]) and bool(self._recs))
        try:
            self.recs_discover_lbl.config(
                text="Discover mode: On" if active else "Start discover mode",
                fg=GREEN if active else TEXT3,
                cursor="hand2" if enabled else "arrow",
            )
        except tk.TclError:
            pass

    def _on_discover_ui_changed(self):
        """Controller push: a preview download state / session state changed —
        refresh the affected rows IN PLACE (coalesced to one pass per event
        loop drain) so downloading songs never flash the whole section."""
        try:
            if self.winfo_exists() and hasattr(self, "recs_wrap"):
                if getattr(self, "_rec_rows_update_pending", False):
                    return
                self._rec_rows_update_pending = True
                self.after(0, self._update_rec_rows)
        except (RuntimeError, tk.TclError):
            self._rec_rows_update_pending = False

    def _ensure_recs_seed(self, title, artist):
        """Trigger a recommendation fetch when the seed changes (or a key was
        added later). Called from update_view on every UI refresh; the seed
        comparison keeps it a no-op during resizes. While a preview session
        is active the seed stays pinned to the session's song, so transient
        preview tracks never re-seed the section (mobile seedUri
        mismatch-display parity)."""
        has_key = self._recs_api_key()
        sess = None
        if hasattr(self.controller, "_discover_session_seed"):
            sess = self.controller._discover_session_seed()
        if sess is not None:
            artist, title = sess
        seed = (artist or "", title or "")

        if not title or not artist:
            if seed != self._rec_seed:
                self._rec_seed = ("", "")
                self._rec_state = "idle"
                self._render_recs()
            return

        if seed == self._rec_seed:
            if not has_key and self._rec_state != "idle":
                # Key removed while the page was open.
                self._rec_state = "idle"
                self._render_recs()
            elif has_key and self._rec_state == "idle":
                # Key was added later (or nothing was ever loaded) — fetch now.
                self._fetch_recs(init=True)
            return

        # New seed — start a fresh list and scroll back to the top.
        self._rec_seed = seed
        self._recs = []
        self._rec_exhausted = False
        self._rec_error = None
        self._rec_load_more_error = None
        self._rec_art_generation += 1
        self._rec_art_cache.clear()
        self._rec_art_pending.clear()
        if has_key:
            self._fetch_recs(init=True)
        else:
            self._rec_state = "idle"
            self._render_recs()

    def _refresh_recs(self):
        """Manual refresh (mobile refresh icon): re-fetch from page one.
        Disabled while a preview session is active (mobile parity)."""
        if getattr(self.controller, "_discover_active", False):
            return
        if not self._rec_seed[0]:
            return
        self._fetch_recs(init=True)

    def _load_more_recs(self):
        """Mobile 'Load 25 more': grow the list in place. During Discover the
        controller calls this automatically at the end of the loaded list."""
        c = self.controller
        if (getattr(c, "_discover_active", False) and
                not getattr(c, "_discover_discover_mode", False)):
            # Paging is a no-op outside Discover while a session is active.
            return
        if (self._rec_loading_more or self._rec_exhausted or
                self._rec_limit >= REC_LIMIT_CAP):
            return
        self._rec_loading_more = True
        self._rec_limit = min(self._rec_limit + REC_PAGE_SIZE, REC_LIMIT_CAP)
        self._fetch_recs(init=False)

    def _fetch_recs(self, init):
        artist, title = self._rec_seed
        if init:
            self._rec_state = "loading"
            self._rec_phase = "init"
            self._rec_loading_more = False
            self._rec_limit = REC_PAGE_SIZE
            self._rec_exhausted = False
            self._rec_error = None
            self._rec_load_more_error = None
        else:
            self._rec_phase = "load_more"
            self._rec_load_more_error = None
        self._render_recs()
        self.controller._fetch_song_page_recs(artist, title, self._rec_limit,
                                              self._on_recs_result)

    def _on_recs_result(self, recs, error):
        try:
            if not self.winfo_exists():
                return
            self._rec_loading_more = False
            if not self._recs_api_key():
                # Key removed while the request was in flight — drop the
                # result (mobile re-runs its effect on apiKey change).
                self._rec_state = "idle"
                self._render_recs()
                return
            if error:
                if self._rec_phase == "load_more" and self._recs:
                    # Keep the current list, surface the failure on the
                    # load-more row (mobile loadMoreError parity).
                    self._rec_load_more_error = error
                    self._render_recs()
                    return
                self._rec_state = "error"
                self._rec_error = error
                self._render_recs()
                return

            if self._rec_phase == "load_more":
                known = {self._rec_key(r) for r in self._recs}
                additions = [r for r in recs if self._rec_key(r) not in known]
                if not additions:
                    self._rec_exhausted = True
                    self._rec_load_more_error = None
                else:
                    self._recs = self._recs + additions
                    if getattr(self.controller, "_discover_active", False):
                        # Grow the discover session with the new page (mobile
                        # full-player continuation parity).
                        self.controller._discover_append_recs(additions)
                self._rec_state = "ready"
                self._render_recs()
                return

            self._recs = recs
            self._rec_state = "ready"
            if self._rec_phase == "init":
                # Mobile prefetchRecommendations parity: passively prepare
                # the first five into the preview staging area so Discover
                # mode can adopt them instantly.
                if hasattr(self.controller, "_discover_prefetch"):
                    self.controller._discover_prefetch(
                        tuple(self._rec_seed), list(recs))
            self._render_recs()
        except (RuntimeError, tk.TclError):
            pass

    def _render_recs(self):
        if not hasattr(self, "recs_wrap"):
            return

        # Refresh icon visible whenever a seed exists and a key is configured;
        # it is a no-op while a preview session is active (mobile parity).
        has_seed = bool(self._rec_seed[0])
        has_key = self._recs_api_key()
        session_active = bool(getattr(self.controller, "_discover_active", False))
        try:
            self.recs_refresh_lbl.config(
                cursor="hand2" if (has_seed and has_key and not session_active)
                else "arrow",
            )
        except tk.TclError:
            return

        # Preserve the page scroll position when growing the list in place
        # (load-more); every other state change resets to the top.
        keep_yview = None
        if self._rec_phase == "load_more":
            try:
                keep_yview = self.page_canvas.yview()[0]
            except tk.TclError:
                keep_yview = None

        for w in self.recs_status_f.winfo_children():
            w.destroy()
        for w in self.recs_wrap.winfo_children():
            if w is self.recs_hdr or w is self.recs_status_f:
                continue
            w.destroy()
        self._rec_rows.clear()
        self._rec_rows_update_pending = False
        self._rec_summary_lbl = None

        seed_artist, seed_title = self._rec_seed
        if not has_seed:
            tk.Label(self.recs_status_f, text="Play a song to see recommendations",
                     font=("Helvetica Neue", 11), fg=TEXT3, bg=DARK_BG, anchor="w").pack(fill="x")
        elif not has_key:
            tk.Label(self.recs_status_f,
                     text="Add a Last.fm API key in Settings to see recommendations",
                     font=("Helvetica Neue", 11), fg=TEXT3, bg=DARK_BG, anchor="w").pack(fill="x")
        elif self._rec_state == "loading":
            load_row = tk.Frame(self.recs_status_f, bg=DARK_BG)
            load_row.pack(fill="x")
            Spinner(load_row, size=16, width=2, bg=DARK_BG).pack(
                side="left", padx=(0, 8))
            tk.Label(load_row, text="Finding similar songs...",
                     font=("Helvetica Neue", 11), fg=TEXT3, bg=DARK_BG,
                     anchor="w").pack(side="left")
        elif self._rec_state == "error":
            msg = self._rec_error or "Could not load recommendations"
            tk.Label(self.recs_status_f, text=msg,
                     font=("Helvetica Neue", 11), fg=TEXT3, bg=DARK_BG, anchor="w",
                     wraplength=max(self.recs_wrap.winfo_width() - 20, 60)).pack(fill="x")
            retry = tk.Label(self.recs_status_f, text="Retry",
                             font=("Helvetica Neue", 11, "bold"), fg=GREEN, bg=DARK_BG,
                             cursor="hand2", pady=4)
            retry.pack(anchor="w")
            retry.bind("<Button-1>", lambda e: self._fetch_recs(init=True))
            retry.bind("<Enter>", lambda e: retry.config(fg=GREEN_HOVER))
            retry.bind("<Leave>", lambda e: retry.config(fg=GREEN))
        elif self._rec_state == "ready" and not self._recs:
            tk.Label(self.recs_status_f, text="No recommendations available for this song",
                     font=("Helvetica Neue", 11), fg=TEXT3, bg=DARK_BG, anchor="w").pack(fill="x")
        elif self._rec_state == "ready":
            # Mobile full-player subtitle parity; the label is kept alive for
            # in-place session-state updates.
            self._rec_summary_lbl = tk.Label(
                self.recs_status_f, text=self._rec_summary_text(),
                font=("Helvetica Neue", 11), fg=TEXT3, bg=DARK_BG, anchor="w")
            self._rec_summary_lbl.pack(fill="x")

        # Mobile 'Scroll for recommendations' hint (only when a seed + API key
        # exist).
        if has_seed and has_key:
            hint = tk.Label(self.recs_status_f,
                            text="⬇  Scroll for recommendations",
                            font=("Helvetica Neue", 9), fg=GREEN, bg=DARK_BG,
                            anchor="w")
            hint.pack(fill="x", pady=(0, 2))

        # Rows
        if self._rec_state == "ready" and self._recs:
            for i, r in enumerate(self._recs):
                self._add_rec_row(r, i)

            # Load-more footer (mobile "Load 25 more" parity)
            if self._rec_loading_more and self._rec_limit < REC_LIMIT_CAP:
                self._add_load_more_footer(loading=True, error=None)
            elif (self._rec_limit < REC_LIMIT_CAP and not self._rec_exhausted):
                self._add_load_more_footer(loading=False, error=self._rec_load_more_error)

        try:
            w, h = self.winfo_width(), self.winfo_height()
            if keep_yview is not None:
                self._page_restore_y = keep_yview
            self._layout_page(w, h)
        except tk.TclError:
            pass

        self._sync_discover_button()

    def _add_load_more_footer(self, loading, error):
        f = tk.Frame(self.recs_wrap, bg=DARK_BG, padx=28, pady=8)
        f.pack(fill="x")
        if error:
            tk.Label(f, text=error, font=("Helvetica Neue", 9), fg="#e05d5d",
                     bg=DARK_BG, anchor="w",
                     wraplength=max(self.recs_wrap.winfo_width() - 40, 60)).pack(fill="x")
        if loading:
            row = tk.Frame(f, bg=DARK_BG)
            row.pack(anchor="w")
            Spinner(row, size=14, width=2, bg=DARK_BG).pack(side="left", padx=(0, 8))
            tk.Label(row, text="Loading more...", font=("Helvetica Neue", 11, "bold"),
                     fg=TEXT3, bg=DARK_BG, pady=4).pack(side="left")
            return
        btn = tk.Label(f, text="Load 25 more", font=("Helvetica Neue", 11, "bold"),
                       fg=GREEN, bg=DARK_BG, cursor="hand2", pady=4)
        btn.pack(anchor="w")
        btn.bind("<Button-1>", lambda e: self._load_more_recs())
        btn.bind("<Enter>", lambda e: btn.config(fg=GREEN_HOVER))
        btn.bind("<Leave>", lambda e: btn.config(fg=GREEN))

    def _rec_preview_playing(self):
        """True when the current track is a transient preview (mobile
        currentKey != null parity for the header subtitle)."""
        try:
            q = self.controller.engine.queue
            idx = self.controller.engine.queue_idx
            if q and 0 <= idx < len(q):
                return bool(self.controller._is_discover_path(q[idx]))
        except Exception:
            pass
        return False

    def _add_rec_row(self, r, i):
        f = tk.Frame(self.recs_wrap, bg=DARK_BG, padx=16, pady=10)
        f.pack(fill="x")

        row_bg = DARK_BG if i % 2 == 0 else "#161616"
        f.config(bg=row_bg)

        key = self._rec_key(r)

        # Artwork thumb (mobile 48dp parity): the deterministic abstract
        # design renders instantly; the Last.fm/Deezer lookup replaces it
        # when it arrives.
        art_lbl = tk.Label(f, bg=row_bg, borderwidth=0, highlightthickness=0)
        art_lbl.pack(side="left", padx=(0, 12))
        self._rec_art_assign(art_lbl, key, r)

        # Info
        info = tk.Frame(f, bg=row_bg)
        info.pack(side="left", fill="both", expand=True)

        title = r.get("title", "Unknown")
        if len(title) > 42:
            title = title[:40] + "..."

        t_lbl = tk.Label(info, text=title, font=("Helvetica Neue", 11, "bold"),
                         fg=TEXT, bg=row_bg, anchor="w")
        t_lbl.pack(fill="x")

        a_lbl = tk.Label(info, font=("Helvetica Neue", 9),
                         fg=TEXT3, bg=row_bg, anchor="w", justify="left")
        a_lbl.pack(fill="x")

        # Mobile row-tap parity: tapping ANY row starts (or restarts) a
        # temporary preview session at that recommendation.
        def play_ready():
            if hasattr(self.controller, "_discover_play_rec"):
                self.controller._discover_play_rec(key)

        for w in (info, t_lbl, a_lbl):
            w.config(cursor="hand2")
            w.bind("<Button-1>", lambda e: play_ready())

        # Actions (trailing widget slots, mobile FullPlayerRecommendationRow
        # parity). Buttons are rebuilt in place by _update_rec_row when the
        # controller state changes.
        act = tk.Frame(f, bg=row_bg)
        act.pack(side="right", padx=(16, 0))

        self._rec_rows[key] = {
            "rec": r,
            "row_bg": row_bg,
            "frame": f,
            "title_lbl": t_lbl,
            "status_lbl": a_lbl,
            "act": act,
            "last_state": None,
        }
        self._update_rec_row(key, self._rec_rows[key])

    def _rec_row_state(self, key):
        st = None
        if hasattr(self.controller, "_discover_row_state"):
            st = self.controller._discover_row_state(key)
        return st

    def _rec_status_text(self, r, status, error, saving, queued, permanent,
                         save_failure, playing):
        """Mobile statusText priority: saving -> queued -> permanent ->
        saveFailure -> playing -> failure -> queuedPreview -> resolving ->
        preparing -> ready -> default '<artist> - <match>% match'."""
        artist = r.get("artist", "Unknown")
        match = int(r.get("match", 0) * 100)
        error_color = "#e05d5d"
        status_text = None
        status_color = TEXT3
        if saving:
            status_text = "Saving to playlist..."
        elif queued:
            status_text = "Download queued for playlist"
        elif permanent:
            status_text = "Saved to library"
            status_color = GREEN
        elif save_failure:
            status_text = f"Save failed: {save_failure}"
            status_color = error_color
        elif playing:
            status_text = "Previewing now"
            status_color = GREEN
        elif status == "failed":
            status_text = error or "Preview failed"
            status_color = error_color
        elif status == "waiting":
            status_text = "Queued for temporary preview"
        elif status == "resolving":
            status_text = "Finding a playable source..."
        elif status == "preparing":
            status_text = "Downloading temporary preview..."
        elif status == "ready":
            status_text = "Ready in temporary queue"
            status_color = GREEN
        if status_text is None:
            status_text = f"{artist} • {match}% match"
        is_error = bool(save_failure) or (
            status == "failed" and not saving and not queued and not permanent)
        if not is_error and len(status_text) > 60:
            status_text = status_text[:57] + "..."
        return status_text, status_color, is_error

    def _update_rec_row(self, key, w):
        """Refresh one recommendation row in place (no widget teardown): the
        playing highlight, the status line, and the trailing action slots."""
        r = w["rec"]
        st = self._rec_row_state(key)
        status = st["status"] if st else None
        error = st.get("error") if st else None
        saving = st.get("saving", False) if st else False
        queued = st.get("queued", False) if st else False
        permanent = st.get("permanent", False) if st else False
        save_failure = st.get("saveFailure") if st else None
        playing = st.get("playing", False) if st else False

        state_key = (status, error, saving, queued, permanent,
                     save_failure, playing)
        if state_key == w.get("last_state"):
            return
        w["last_state"] = state_key

        status_text, status_color, is_error = self._rec_status_text(
            r, status, error, saving, queued, permanent, save_failure, playing)
        try:
            w["title_lbl"].config(fg=GREEN if playing else TEXT)
            lbl = w["status_lbl"]
            lbl.config(
                text=status_text, fg=status_color,
                wraplength=(max(self.recs_wrap.winfo_width() - 190, 60)
                            if is_error else 0))
            self._rebuild_rec_actions(w, key, r, status, saving, queued,
                                      permanent, save_failure, playing)
        except tk.TclError:
            pass

    def _rebuild_rec_actions(self, w, key, r, status, saving, queued,
                             permanent, save_failure, playing):
        """Trailing widget slots in a fixed visual order — status icon,
        secondary action, save — mirroring the mobile row: an animated green
        spinner while resolving/preparing, a green check circle when ready,
        and a compact 'add to playlist' button that always sits at the far
        right edge. The save slot is fixed-width so rows line up regardless
        of state."""
        act = w["act"]
        for child in act.winfo_children():
            child.destroy()
        row_bg = w["row_bg"]

        def pill(text, fg, bg, width, cmd=None):
            b = tk.Label(act, text=text, font=("Helvetica Neue", 9, "bold"),
                         fg=fg, bg=bg, padx=10, pady=6, width=width)
            b.pack(side="right", padx=(4, 0))
            if cmd is None:
                b.config(cursor="arrow")
                return b
            hover_bg = GREEN if bg == BG3 else GREEN_HOVER
            hover_fg = SIDEBAR if bg == BG3 else fg
            b.config(cursor="hand2")
            b.bind("<Button-1>", lambda e: cmd())
            b.bind("<Enter>", lambda e: b.config(bg=hover_bg, fg=hover_fg))
            b.bind("<Leave>", lambda e: b.config(bg=bg, fg=fg))
            return b

        # 1) Save slot — always the far-right, fixed-width element (mobile
        # PlaylistAdd parity).
        if saving:
            self._busy_pill(act, "Saving…", row_bg)
        elif queued:
            self._busy_pill(act, "Queued…", row_bg)
        elif permanent:
            pill("✓ Saved", GREEN, row_bg, 13)
        else:
            def do_add():
                if hasattr(self.controller, "_discover_promote"):
                    self.controller._discover_promote(key)
                else:
                    self.controller._download_track(
                        r.get("artist", ""), r.get("title", ""))
            pill("＋ Add to playlist", TEXT2, BG3, 13, cmd=do_add)

        # 2) Secondary action slot.
        if status == "ready" and not saving and not queued and not permanent:
            def play_now():
                if hasattr(self.controller, "_discover_play_rec"):
                    self.controller._discover_play_rec(key)
            pill("▶ Play", SIDEBAR, GREEN, 6, cmd=play_now)
        elif status == "failed" and not saving and not queued:
            pill("⟳ Retry", TEXT2, BG3, 7,
                 cmd=lambda: self.controller._discover_retry(key))

        # 3) Status icon slot — fixed 20px so the buttons never shift.
        if status in ("resolving", "preparing"):
            Spinner(act, size=20, bg=row_bg).pack(side="right", padx=(4, 0))
        elif status == "ready" and not permanent:
            self._check_circle(act, row_bg).pack(side="right", padx=(4, 0))
        else:
            tk.Frame(act, width=20, height=20, bg=row_bg).pack(
                side="right", padx=(4, 0))

    def _check_circle(self, parent, bg):
        """Green outlined circle with a checkmark (mobile CheckCircle
        parity) for ready rows."""
        c = tk.Canvas(parent, width=20, height=20, highlightthickness=0,
                      bd=0, bg=bg)
        c.create_oval(2, 2, 18, 18, outline=GREEN, width=2)
        c.create_line(6.5, 10.5, 9, 13, 13.5, 7.5, fill=GREEN, width=2.2,
                      capstyle="round", joinstyle="round")
        return c

    def _busy_pill(self, parent, text, bg):
        """Compact spinner + status pill used by the save slot while a save
        is in flight."""
        f = tk.Frame(parent, bg=bg)
        f.pack(side="right", padx=(4, 0))
        tk.Label(f, text=text, font=("Helvetica Neue", 9, "bold"),
                 fg=TEXT3, bg=bg).pack(side="right", padx=(2, 0))
        Spinner(f, size=14, width=2, bg=bg).pack(side="right")

    def _rec_summary_text(self):
        summary = f"{len(self._recs)} similar songs - tap to preview"
        c = self.controller
        if getattr(c, "_discover_active", False):
            discover = bool(getattr(c, "_discover_discover_mode", False))
            previewing = self._rec_preview_playing()
            if discover and previewing:
                summary = "Temporary preview queue - tap a song to restart from there"
            elif not discover and previewing:
                summary = "Temporary preview - Next returns to your library"
        return summary

    def _update_rec_rows(self):
        """In-place refresh pass: updates the section summary and every live
        row from the controller's current preview state. Never rebuilds
        widgets, so preview downloads do not flash the page."""
        self._rec_rows_update_pending = False
        try:
            if not self.winfo_exists():
                return
        except tk.TclError:
            return

        lbl = self._rec_summary_lbl
        if lbl is not None:
            try:
                if lbl.winfo_exists():
                    lbl.config(text=self._rec_summary_text())
            except tk.TclError:
                pass

        session_active = bool(getattr(self.controller, "_discover_active", False))
        has_seed = bool(self._rec_seed[0])
        has_key = self._recs_api_key()
        try:
            self.recs_refresh_lbl.config(
                cursor="hand2" if (has_seed and has_key and not session_active)
                else "arrow",
            )
        except tk.TclError:
            pass

        for key, w in list(self._rec_rows.items()):
            try:
                if w["frame"].winfo_exists():
                    self._update_rec_row(key, w)
            except tk.TclError:
                pass
        self._sync_discover_button()

    # ── Per-row artwork (mobile RecommendationArtwork parity) ───────────────

    def _rec_art_assign(self, art_lbl, key, rec):
        """Show the deterministic design immediately, then fetch the real
        artwork (Last.fm image -> Deezer fallback) in the background."""
        cached = self._rec_art_cache.get(key)
        if cached is not None:
            try:
                art_lbl.config(image=cached)
                return
            except tk.TclError:
                pass
        design = rec_artwork.render_design(
            rec_artwork.design_index(key), REC_ART_SIZE)
        try:
            from PIL import ImageTk
            photo = ImageTk.PhotoImage(design)
        except Exception:
            photo = None
        if photo is not None:
            self._rec_art_cache_set(key, photo)
            try:
                art_lbl.config(image=photo)
            except tk.TclError:
                return
        self._rec_art_fetch(art_lbl, key, rec)

    def _rec_art_cache_set(self, key, photo):
        if len(self._rec_art_cache) >= REC_ART_CACHE_LIMIT:
            try:
                self._rec_art_cache.pop(next(iter(self._rec_art_cache)))
            except (StopIteration, KeyError):
                pass
        self._rec_art_cache[key] = photo

    def _rec_art_fetch(self, art_lbl, key, rec):
        if key in self._rec_art_pending:
            return
        url = (rec.get("image") or "").strip()
        pending = {
            "url": url,
            "artist": rec.get("artist", ""),
            "title": rec.get("title", ""),
            "deezer_tried": False,
        }
        self._rec_art_pending[key] = pending
        if url:
            self._rec_art_load_url(art_lbl, key, url)
        else:
            self._rec_art_load_deezer(art_lbl, key, pending)

    def _rec_art_load_url(self, art_lbl, key, url):
        def run():
            image = rec_artwork.load_image(url, REC_ART_SIZE)
            try:
                self.after(0, lambda: self._rec_art_apply(art_lbl, key, image))
            except (RuntimeError, tk.TclError):
                pass
        self._rec_art_executor.submit(run)

    def _rec_art_load_deezer(self, art_lbl, key, pending):
        def run():
            url = rec_artwork.find_artwork(pending["artist"], pending["title"])
            image = rec_artwork.load_image(url, REC_ART_SIZE) if url else None
            try:
                self.after(0, lambda: self._rec_art_apply(art_lbl, key, image))
            except (RuntimeError, tk.TclError):
                pass
        self._rec_art_executor.submit(run)

    def _rec_art_apply(self, art_lbl, key, image):
        try:
            if not art_lbl.winfo_exists():
                return
        except tk.TclError:
            return
        if image is None:
            pending = self._rec_art_pending.get(key)
            if pending is not None and pending.get("url") and not pending.get("deezer_tried"):
                # The Last.fm image failed — try Deezer once per key (mobile
                # onArtworkError parity).
                pending["deezer_tried"] = True
                self._rec_art_load_deezer(art_lbl, key, pending)
            return
        try:
            from PIL import ImageTk
            photo = ImageTk.PhotoImage(image)
            self._rec_art_cache_set(key, photo)
            art_lbl.config(image=photo)
        except (RuntimeError, tk.TclError):
            pass

    # ── Page lifecycle ──────────────────────────────────────────────────────

    def populate(self):
        """Called when this tab is shown."""
        self.update_view()
        self._sync_discover_button()

    def update_view(self):
        if not self.winfo_ismapped(): return

        w = self.winfo_width()
        h = self.winfo_height()
        if w < 10 or h < 10: return

        # Fill the canvas both directions to allow vertical centering
        self.canvas.itemconfig(self.canvas_window, width=w, height=h)

        # Responsive Layout Adjustments. Reserve ~230px of the first viewport
        # for the recommendation header + first rows, like the mobile full
        # player's fixed controls over its scrollable content.
        max_h = h - 150
        media_width = max(w - 10, 200)
        media_height = min(max_h, int(media_width * 9 / 16), 700)
        media_height = max(media_height, 160)
        media_width = (int(media_width) // 20) * 20
        media_height = (int(media_height) // 20) * 20
        self.art_container.config(width=media_width, height=media_height)
        self.art_container.update_idletasks()
        if self._video_player is not None:
            self._video_player.resize(media_width, media_height)

        if w < 600:
            self.title_lbl.config(font=("Helvetica Neue", 20, "bold"))
            self.artist_lbl.config(font=("Helvetica Neue", 10))
        else:
            self.title_lbl.config(font=("Helvetica Neue", 24, "bold"))
            self.artist_lbl.config(font=("Helvetica Neue", 12))

        self.canvas.config(width=w, height=h)

        # Get current track
        path = ""
        if self.controller.engine.queue and self.controller.engine.queue_idx >= 0:
            path = self.controller.engine.queue[self.controller.engine.queue_idx]

        if not path:
            self._stop_video()
            self._bg_generation += 1
            self._bg_signature = None
            if self._bg_future and not self._bg_future.done():
                self._bg_future.cancel()
            self._bg_loading = False
            self._art_photo = None
            self._art_path = None
            self.title_lbl.config(text="Nothing playing")
            self.artist_lbl.config(text="")
            self._sync_favorite_button(None)
            self._draw_background(None)
            self._ensure_recs_seed("", "")
            self._layout_page(w, h)
            return

        title, artist = split_track_name(path)
        if not artist:
            # Smart fallback (discovery parity): use the queue's playlist/folder
            # name as the artist unless it is a catch-all bucket.
            pl_ctx = getattr(self.controller, "_queue_playlist_name", None)
            if pl_ctx and pl_ctx not in ["All Music", "Unsorted"]:
                artist = pl_ctx
        self.title_lbl.config(text=title)
        self.artist_lbl.config(text=artist or "Unknown Artist")
        if path != self._video_path:
            self._stop_video()
            self.art_container.update_idletasks()
            self._start_current_staged_video(path)
            if self.video_preload_var.get():
                self._preload_current_video(path)
                self._preload_next_video()
        if self._video_player is None:
            if self._video_preload_paths:
                self._set_video_spinner(True)
            else:
                self._set_video_spinner(False)
                self.video_btn.config(text="▷ Video", fg=TEXT3, cursor="hand2")
        self._sync_favorite_button(path)
        self._ensure_recs_seed(title, artist or "")
        self._layout_page(w, h)

        # Artwork Fetching. The cache is LRU-bounded, so a library refresh
        # (e.g. favoriting a song) can evict this page's entry mid-display;
        # keep showing the last good image for this path instead of flashing
        # the placeholder while the refetch is in flight.
        if self._video_player is not None and self._video_path == path:
            img = None
        else:
            img = self.controller.art_cache.get(
                path, (media_width, media_height), lambda p: self.after(0, self.update_view))
        if img:
            self._art_photo = img
            self._art_path = path
            self.art_lbl.config(image=img, text="", bg=BG2,
                                width=media_width, height=media_height)
        elif self._video_player is not None and self._video_path == path:
            pass
        elif self._art_photo is not None and self._art_path == path:
            try:
                self.art_lbl.config(image=self._art_photo, text="")
            except tk.TclError:
                self._art_photo = None
                self.art_lbl.config(image="", text="♫", bg=BG2)
        else:
            self._art_photo = None
            self._art_path = path if path else None
            self.art_lbl.config(image="", text="♫", bg=BG2)

        # Glassmorphism background. Only one worker is allowed; a newer
        # request supersedes an older result instead of spawning another thread.
        signature = (path, w, h)
        if signature != self._bg_signature:
            self._bg_signature = signature
            self._bg_generation += 1
            generation = self._bg_generation
            if self._bg_future and not self._bg_future.done():
                self._bg_future.cancel()
            self._bg_loading = True
            self._bg_future = self._bg_executor.submit(self._load_blur_bg, path, w, h)
            self._bg_future.add_done_callback(
                lambda future, generation=generation, signature=signature:
                    self._on_blur_ready(future, generation, signature)
            )

    def _load_blur_bg(self, path, width, height):
        """Load and blur album art for the glass background (in thread)."""
        try:
            from mutagen.mp3 import MP3
            from mutagen.id3 import ID3
            from PIL import Image, ImageFilter
            import io
            audio = MP3(path, ID3=ID3)
            img_data = None
            if audio.tags:
                for tag in audio.tags.values():
                    if getattr(tag, 'FrameID', None) == 'APIC':
                        img_data = tag.data
                        break
            if not img_data:
                return None

            img = Image.open(io.BytesIO(img_data)).convert("RGB")
            # Resize to fill the window
            img = img.resize((width, height), Image.Resampling.LANCZOS)
            # Apply strong blur for glassmorphism effect
            blurred = img.filter(ImageFilter.GaussianBlur(radius=40))
            dimmed = blurred.point(lambda p: int(p * 0.45))

            # Darken edges with a gradient
            overlay = Image.new("RGB", (width, height), (0, 0, 0))
            dimmed = Image.blend(dimmed, overlay, 0.35)
            return dimmed
        except Exception:
            return None

    def _on_blur_ready(self, future, generation, signature):
        try:
            image = future.result()
        except Exception:
            image = None

        def apply_result():
            if generation != self._bg_generation or signature != self._bg_signature:
                return
            self._bg_loading = False
            if image is None:
                self._draw_background(None)
                return
            from PIL import ImageTk
            self._draw_background(ImageTk.PhotoImage(image))

        try:
            self.controller.after(0, apply_result)
        except (RuntimeError, tk.TclError):
            pass

    def destroy(self):
        self._stop_video()
        self._close_fullscreen_video()
        if self.video_menu_window is not None:
            try:
                self.video_menu_window.destroy()
            except tk.TclError:
                pass
            self.video_menu_window = None
        if self._video_spinner_after is not None:
            try:
                self.after_cancel(self._video_spinner_after)
            except (RuntimeError, tk.TclError):
                pass
            self._video_spinner_after = None
        self._cancel_hide_video_menu()
        for sequence, bind_id in self._video_key_bindings:
            try:
                self.unbind_all(sequence, bind_id)
            except tk.TclError:
                pass
        self._bg_generation += 1
        if self._bg_future and not self._bg_future.done():
            self._bg_future.cancel()
        self._bg_executor.shutdown(wait=False, cancel_futures=True)
        self._rec_art_executor.shutdown(wait=False, cancel_futures=True)
        super().destroy()

    def _request_video(self):
        """Probe first, then ask before spending bandwidth on a video file."""
        if self._video_player is not None:
            self._stop_video()
            self.video_btn.config(text="▷ Video", fg=TEXT3, cursor="hand2")
            return
        if not shutil.which("ffmpeg"):
            self.video_btn.config(text="FFmpeg unavailable", cursor="arrow")
            return
        if self._video_loading:
            return
        path = self._current_path()
        if not path:
            return
        title, artist = self._video_track_metadata(path)
        target = getattr(self.controller, "PREVIEW_DIR", os.path.expanduser("~/.luno_previews"))
        try:
            from video_preview import find_video
            staged = find_video(artist, title, target)
        except Exception:
            staged = None
        if staged:
            self._video_download_done(path, staged, None)
            return
        self._video_loading = True
        self._video_probe_attempt += 1
        attempt = self._video_probe_attempt
        self.video_btn.config(text="Checking video...", cursor="arrow")

        def run():
            try:
                from video_preview import resolve_video_source
                source = resolve_video_source(artist, title)
                error = None
            except Exception as exc:
                source, error = None, str(exc)
            try:
                self.after(0, lambda: self._video_probe_done(
                    path, artist, title, source, error, attempt))
            except (RuntimeError, tk.TclError):
                pass
        threading.Thread(target=run, daemon=True, name="luno-video-probe").start()

    def _handle_video_f_key(self, event):
        if self._fullscreen_window is not None:
            self._close_fullscreen_video()
        elif self._video_player is not None:
            self._open_fullscreen_video()
        return "break"

    def _handle_video_escape(self, event):
        if self._fullscreen_window is not None:
            self._close_fullscreen_video()
            return "break"

    def _current_path(self):
        try:
            q = self.controller.engine.queue
            i = self.controller.engine.queue_idx
            return q[i] if q and 0 <= i < len(q) else ""
        except Exception:
            return ""

    def _video_track_metadata(self, path):
        """Use one artist/title pair for lookup, staging, and adoption."""
        title, artist = split_track_name(path)
        if not artist:
            playlist = getattr(self.controller, "_queue_playlist_name", None)
            if playlist and playlist not in ("All Music", "Unsorted"):
                artist = playlist
        return title, artist

    def _video_probe_done(self, path, artist, title, source, error, attempt=None):
        self._video_loading = False
        if path != self._current_path():
            return
        if error:
            if attempt == self._video_probe_attempt and attempt < 2:
                self._video_loading = False
                self.after(250, self._request_video)
                return
            self.video_btn.config(text="No video found", fg=TEXT3, cursor="arrow")
            self.controller.status_var.set(error)
            return
        self.video_btn.config(text="Downloading video...", cursor="arrow")
        target = getattr(self.controller, "PREVIEW_DIR", os.path.expanduser("~/.luno_previews"))

        def run():
            try:
                from video_preview import download_video
                result, error = download_video(artist, title, source["url"], target), None
            except Exception as exc:
                result, error = None, str(exc)
            try:
                self.after(0, lambda: self._video_download_done(path, result, error))
            except (RuntimeError, tk.TclError):
                pass
        threading.Thread(target=run, daemon=True, name="luno-video-download").start()

    def _video_download_done(self, path, video_path, error):
        if path != self._current_path():
            return
        if error or not video_path:
            self.video_btn.config(text="Video unavailable", fg=TEXT3, cursor="arrow")
            self.controller.status_var.set(error or "Video could not be downloaded")
            return
        self._video_path = path
        self._video_file = video_path
        self._video_generation += 1
        self.art_container.update_idletasks()
        width = max(self.art_container.winfo_width(), 2)
        height = max(self.art_container.winfo_height(), 2)
        self._video_size = (width, height)
        self._video_player = _FFmpegVideoPlayer(
            video_path, self.art_lbl, width, height, self.after,
            self._on_video_finished,
            lambda: self.controller.engine.get_position(),
            lambda: bool(self.controller.engine.playing and
                         not self.controller.engine.paused))
        self._video_player.play()
        self._set_video_spinner(False)
        self.video_btn.config(text="■ Video", fg=GREEN, cursor="hand2")
        self.controller.status_var.set("Playing song video over album art")

    def _start_current_staged_video(self, path):
        if not path or self._video_player is not None:
            return
        title, artist = self._video_track_metadata(path)
        target = getattr(self.controller, "PREVIEW_DIR", os.path.expanduser("~/.luno_previews"))
        try:
            from video_preview import find_video
            video_path = find_video(artist, title, target)
        except Exception:
            video_path = None
        if video_path:
            self._video_download_done(path, video_path, None)
        elif self.video_preload_var.get():
            try:
                self.after(50, lambda: self._start_current_staged_video(path))
            except (RuntimeError, tk.TclError):
                pass

    def _video_preload_changed(self):
        if self.video_preload_var.get():
            self._set_video_spinner(True)
            self._preload_current_video()
            self._preload_next_video()
        else:
            self._set_video_spinner(False)

    def _preload_current_video(self, path=None):
        path = path or self._current_path()
        if path:
            self._preload_video(path, start_current=True)

    def _preload_video(self, path, start_current=False):
        if not path or path in self._video_preload_paths:
            return
        if self._video_preload_states.get(path) in ("ready", "unavailable"):
            return
        self._video_preload_paths.add(path)
        self._video_preload_states[path] = "downloading"
        self._set_video_spinner(True)
        title, artist = self._video_track_metadata(path)
        target = getattr(self.controller, "PREVIEW_DIR", os.path.expanduser("~/.luno_previews"))

        def run():
            video_path = None
            try:
                from video_preview import find_video, resolve_video_source, download_video
                video_path = find_video(artist, title, target)
                if not video_path:
                    source = resolve_video_source(artist, title)
                    video_path = download_video(artist, title, source["url"], target)
                message = f"Video preloaded: {title}"
            except Exception as exc:
                message = f"Video preload failed: {exc}"
            self._video_preload_paths.discard(path)
            self._video_preload_states[path] = "ready" if video_path else "unavailable"
            try:
                self.after(0, lambda: self._preload_video_done(
                    path, video_path, start_current, message))
            except (RuntimeError, tk.TclError):
                pass
        threading.Thread(target=run, daemon=True, name="luno-video-preload").start()

    def _preload_video_done(self, path, video_path, start_current, message):
        if not video_path:
            title, artist = self._video_track_metadata(path)
            target = getattr(self.controller, "PREVIEW_DIR", os.path.expanduser("~/.luno_previews"))
            try:
                from video_preview import find_video
                video_path = find_video(artist, title, target)
            except Exception:
                video_path = None
        if (start_current and video_path and path == self._current_path() and
                self._video_player is None):
            self._video_download_done(path, video_path, None)
        if not self._video_preload_paths:
            self._set_video_spinner(False)
        self.controller.status_var.set(message)

    def _set_video_spinner(self, loading):
        if loading:
            if self._video_spinner_after is not None:
                return
            self.video_spinner.config(fg=GREEN)
            self.video_menu_spinner.config(fg=GREEN)
            self._animate_video_spinner()
        else:
            if self._video_spinner_after is not None:
                try:
                    self.after_cancel(self._video_spinner_after)
                except (RuntimeError, tk.TclError):
                    pass
                self._video_spinner_after = None
            self.video_spinner.config(text=".", fg=TEXT3)
            self.video_menu_spinner.config(text="", fg=TEXT3)

    def _animate_video_spinner(self):
        if not self._video_preload_paths:
            self._set_video_spinner(False)
            return
        frames = ("|", "/", "-", "\\")
        self.video_spinner.config(text=frames[self._video_spinner_frame % len(frames)])
        self.video_menu_spinner.config(text=frames[self._video_spinner_frame % len(frames)])
        self._video_spinner_frame += 1
        try:
            self._video_spinner_after = self.after(120, self._animate_video_spinner)
        except (RuntimeError, tk.TclError):
            self._video_spinner_after = None

    def _preload_next_video(self):
        try:
            engine = self.controller.engine
            next_idx = engine.queue_idx + 1
            next_path = engine.queue[next_idx]
        except (AttributeError, IndexError):
            self.controller.status_var.set("No next song in the current queue")
            return
        self._preload_video(next_path)

    def _show_video_menu(self):
        self._cancel_hide_video_menu()
        self.video_menu_window.update_idletasks()
        x = self.video_menu_btn.winfo_rootx() - self.video_menu_window.winfo_reqwidth()
        y = self.video_menu_btn.winfo_rooty() + self.video_menu_btn.winfo_height() + 4
        self.video_menu_window.geometry(f"{self.video_menu_window.winfo_reqwidth()}x"
                                        f"{self.video_menu_window.winfo_reqheight()}+{x}+{y}")
        self.video_menu_window.deiconify()

    def _run_video_menu_action(self, action):
        self._cancel_hide_video_menu()
        self.video_menu_window.withdraw()
        self.after(0, action)

    def _cancel_hide_video_menu(self):
        if self._video_menu_hide_after is not None:
            try:
                self.after_cancel(self._video_menu_hide_after)
            except (RuntimeError, tk.TclError):
                pass
            self._video_menu_hide_after = None

    def _hide_video_menu_later(self):
        self._cancel_hide_video_menu()
        try:
            self._video_menu_hide_after = self.after(180, self._hide_video_menu_if_outside)
        except (RuntimeError, tk.TclError):
            self._video_menu_hide_after = None

    def _schedule_video_menu_check(self):
        self._hide_video_menu_later()

    def _hide_video_menu_if_outside(self):
        self._video_menu_hide_after = None
        try:
            x, y = self.winfo_pointerxy()
            for widget in (self.video_menu_btn, self.art_container,
                           self.video_menu_window):
                left = widget.winfo_rootx()
                top = widget.winfo_rooty()
                if left <= x < left + widget.winfo_width() and top <= y < top + widget.winfo_height():
                    return
            self.video_menu_window.withdraw()
        except (RuntimeError, tk.TclError):
            pass

    def on_audio_track_started(self, path):
        """Engine hook: preload independently of Song Page redraw timing."""
        if self.video_preload_var.get():
            # Use the event's immutable path, not a queue index that may have
            # changed again before this UI callback runs.
            self._preload_current_video(path)
            self._preload_next_video()

    def _stop_video(self):
        player = self._video_player
        self._video_player = None
        self._video_path = None
        self._video_file = None
        self._video_size = None
        self._video_generation += 1
        if player is not None:
            try:
                player.stop()
            except Exception:
                pass

    def _open_fullscreen_video(self):
        if not self._video_file or self._video_player is None:
            return
        if self._fullscreen_window is not None:
            return
        window = tk.Toplevel(self)
        window.configure(bg="black")
        window.attributes("-fullscreen", True)
        label = tk.Label(window, bg="black")
        label.pack(fill="both", expand=True)
        window.focus_force()
        self._fullscreen_window = window
        width = max(window.winfo_screenwidth(), 2)
        height = max(window.winfo_screenheight(), 2)
        self._fullscreen_player = _FFmpegVideoPlayer(
            self._video_file, label, width, height, window.after,
            lambda: self._close_fullscreen_video(),
            lambda: self.controller.engine.get_position(),
            lambda: bool(self.controller.engine.playing and
                         not self.controller.engine.paused))
        self._fullscreen_player.play()
        window.bind("<Escape>", lambda e: self._close_fullscreen_video())
        window.protocol("WM_DELETE_WINDOW", self._close_fullscreen_video)

    def _close_fullscreen_video(self):
        player = self._fullscreen_player
        self._fullscreen_player = None
        window = self._fullscreen_window
        self._fullscreen_window = None
        if window is not None:
            try:
                window.destroy()
            except tk.TclError:
                pass
        if player is not None:
            # Do not block Tk while terminating a decoder that may be blocked
            # reading a raw frame. Its stop path is safe on a worker thread.
            threading.Thread(target=player.stop, daemon=True,
                             name="luno-fullscreen-stop").start()

    def _on_video_finished(self):
        self._close_fullscreen_video()
        if self._video_player is not None:
            self._video_player = None
            self._video_file = None
            self._video_size = None
            self.video_btn.config(text="▷ Video", fg=TEXT3, cursor="hand2")


    def _sync_favorite_button(self, path="__current__"):
        if path == "__current__":
            path = ""
            if hasattr(self.controller, "engine") and self.controller.engine.queue:
                idx = self.controller.engine.queue_idx
                if 0 <= idx < len(self.controller.engine.queue):
                    path = self.controller.engine.queue[idx]
        is_favorite = bool(path and self.controller._is_favorite(path))
        self.favorite_btn.config(
            text="★  Remove from Favorites" if is_favorite else "☆  Add to Favorites",
            fg=GREEN if is_favorite else TEXT3,
            cursor="hand2" if path else "arrow",
        )

    def _draw_background(self, bg_img_or_path):
        self.canvas.delete("all")
        w = self.winfo_width()
        h = self.winfo_height()
        if w < 10 or h < 10: return

        # Re-create window item
        self.canvas_window = self.canvas.create_window(0, 0, window=self.content, anchor="nw")
        self.canvas.itemconfig(self.canvas_window, width=w, height=h)

        if bg_img_or_path is not None:
            self._bg_photo = bg_img_or_path
            # Draw glass background
            self.canvas.create_image(0, 0, image=self._bg_photo, anchor="nw")
            # Dark vignette overlay: darker at bottom for readability
            overlay_steps = 50
            for i in range(overlay_steps):
                t = (i / overlay_steps) ** 1.5
                val = int(10 + (30 - 10) * t)
                c = rgb_to_hex(val, val, val)
                y0 = int(h * i / overlay_steps)
                y1 = int(h * (i + 1) / overlay_steps)
                self.canvas.create_rectangle(0, y0, w, y1, fill=c, outline="")
            # Bottom edge fade to PLAYER_BG color
            fade_steps = 20
            fade_start = int(h * 0.7)
            for i in range(fade_steps):
                t = i / fade_steps
                r, g, b = 0x12 + int((0x18 - 0x12) * t), 0x12 + int((0x18 - 0x12) * t), 0x12 + int((0x18 - 0x12) * t)
                y0 = fade_start + int((h - fade_start) * i / fade_steps)
                y1 = fade_start + int((h - fade_start) * (i + 1) / fade_steps)
                self.canvas.create_rectangle(0, y0, w, y1, fill=rgb_to_hex(r, g, b), outline="")
        else:
            # Fallback: dark gradient
            r1, g1, b1 = 0x12, 0x12, 0x12
            r2, g2, b2 = 0x08, 0x08, 0x08
            steps = 30
            for i in range(steps):
                t = (i / steps) ** 1.5
                r, g, b = r1 + (r2 - r1) * t, g1 + (g2 - g1) * t, b1 + (b2 - b1) * t
                y0 = int(h * i / steps)
                y1 = int(h * (i + 1) / steps)
                self.canvas.create_rectangle(0, y0, w, y1, fill=rgb_to_hex(r, g, b), outline="")

        self.canvas.tag_lower("all")

        # Sync UI backgrounds to dark
        self.content.config(bg=DARK_BG)
        self.page_canvas.config(bg=DARK_BG)
        self.page_sb.config(bg=DARK_BG)
        self.page_inner.config(bg=DARK_BG)
        self.main_f.config(bg=DARK_BG)
        self._recursive_bg(self.main_f, DARK_BG)

        # Recommendation section blends with the glass page; the alternating
        # row colors are re-applied by _render_recs.
        self.recs_wrap.config(bg=DARK_BG)
        self.recs_hdr.config(bg=DARK_BG)
        self.recs_title_lbl.config(bg=DARK_BG)
        self.recs_discover_lbl.config(bg=DARK_BG)
        self.recs_refresh_lbl.config(bg=DARK_BG)
        self.recs_status_f.config(bg=DARK_BG)

    def _recursive_bg(self, parent, color):
        parent.config(bg=color)
        for child in parent.winfo_children():
            if child == self.art_container or child == self.art_lbl:
                continue
            if child == self.recs_wrap:
                continue
            try:
                child.config(bg=color)
            except:
                pass
            if isinstance(child, tk.Frame):
                self._recursive_bg(child, color)
