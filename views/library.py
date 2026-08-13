import tkinter as tk
from tkinter import ttk, messagebox
from urllib.parse import urlparse
import math
import os
import threading
from theme import BG, BG2, BG3, BG4, BORDER, GREEN, GREEN_HOVER, TEXT, TEXT2, TEXT3, WARNING, SIDEBAR
from utils import split_track_name, hex_to_rgb, rgb_to_hex, bind_scroll_events, scroll_amount
from widgets import PlaylistArt
from theme import playlist_gradient
import random

class LibraryView(tk.Frame):
    _SORT_MODES = ("A-Z", "Z-A", "Recently added", "Duration")

    def __init__(self, parent, controller):
        super().__init__(parent, bg=BG)
        self.controller = controller
        
        self._track_items   = []
        self._track_sel_idx = []
        self._sel_anchor = None
        self._track_hover_idx = None
        self._favorite_star_positions = {}
        self._pulse_frame = 0
        self._pulse_items = []
        self._pulse_running = False
        self._favorites_first = False
        self._sort_modes = {"Unsorted": "Recently added"}
        self._duration_cache = {}
        self._duration_pending = set()
        self._duration_deferred = set()
        self._duration_load_active = False
        self._track_redraw_after = None
        
        # Context menu
        self.ctx_menu = tk.Menu(self, tearoff=0, bg=BG3, fg=TEXT,
                                activebackground=BG4, activeforeground=TEXT,
                                font=("Helvetica Neue",12), bd=0, relief="flat")
        self.ctx_menu.add_command(label="  ▶   Play Now",          command=self.controller._ctx_play_now)
        self.ctx_menu.add_command(label="  ↑   Play Next",         command=self.controller._ctx_play_next)
        self.ctx_menu.add_command(label="  +   Add to Queue",      command=self.controller._ctx_add_queue)
        self.ctx_menu.add_separator()
        self.ctx_menu.add_command(label="  ♫   Add to Playlist…",  command=self.controller._ctx_add_playlist)
        self.ctx_menu.add_command(label="  ✎   Edit Track Info…",    command=self.controller._ctx_edit_track_info)
        self.ctx_menu.add_command(label="  ⇧   Export Selected JSON", command=self.controller._export_selected_json)
        self.ctx_menu.add_separator()
        self._favorite_menu_index = self.ctx_menu.index("end") + 1
        self.ctx_menu.add_command(label="  ☆   Add to Favorites", command=self.controller._ctx_toggle_favorite)
        self.ctx_menu.add_separator()
        self.ctx_menu.add_command(label="  ✕   Remove from Queue", command=self.controller._ctx_remove_queue)
        self.ctx_menu.add_command(label="  🗑   Delete from Library",  command=self.controller._ctx_remove_from_library)

        self._build_ui()
        
    def _build_ui(self):
        self.grid_rowconfigure(3, weight=1)
        self.grid_columnconfigure(0, weight=1)
        self.grid_columnconfigure(1, weight=0)

        # Gradient header
        self._hdr_canvas = tk.Canvas(self, height=220, highlightthickness=0, bd=0, bg=BG)
        self._hdr_canvas.grid(row=0, column=0, columnspan=2, sticky="ew")

        self._hdr_overlay = tk.Frame(self._hdr_canvas, bg=BG)
        self._hdr_canvas.create_window(0, 0, window=self._hdr_overlay, anchor="nw", tags="overlay")
        self._hdr_canvas.bind("<Configure>",
            lambda e: (self._draw_lib_header(),
                       self._hdr_canvas.itemconfig("overlay", width=e.width, height=220)))

        self.playlist_settings_btn = tk.Label(
            self._hdr_overlay, text="⚙", font=("Helvetica", 20),
            fg=TEXT3, bg=BG, cursor="hand2"
        )
        self.playlist_settings_btn.place(relx=1.0, rely=0.0, x=-28, y=18, anchor="ne")
        self.playlist_settings_btn.bind("<Button-1>", lambda e: self._show_playlist_actions())
        self.playlist_settings_btn.bind("<Enter>", lambda e: self.playlist_settings_btn.config(fg=TEXT))
        self.playlist_settings_btn.bind("<Leave>", lambda e: self.playlist_settings_btn.config(fg=TEXT3))

        self._art_container = tk.Frame(self._hdr_overlay, bg=BG, width=150, height=150)
        self._art_container.pack(side="left", padx=(32,22), pady=(35,0), anchor="n")
        self._art_container.pack_propagate(False)
        self._current_art = PlaylistArt(self._art_container, 150, "All Music", app=self.controller, bg=BG3)
        self._current_art.pack(fill="both", expand=True)

        title_area = tk.Frame(self._hdr_overlay, bg=BG)
        title_area.pack(side="left", fill="both", expand=True, pady=(42,0))
        
        tk.Label(title_area, text="PLAYLIST", font=("Helvetica Neue",10,"bold"),
                 fg=TEXT2, bg=BG, anchor="w").pack(anchor="w")
        
        self.playlist_title_lbl = tk.Label(title_area, text="All Music",
                                            font=("Helvetica Neue",28,"bold"),
                                            fg=TEXT, bg=BG, anchor="w")
        self.playlist_title_lbl.pack(anchor="w", pady=(4,2))

        self.playlist_meta_lbl = tk.Label(title_area, text="",
                                           font=("Helvetica Neue",11),
                                           fg=TEXT2, bg=BG, anchor="w")
        self.playlist_meta_lbl.pack(anchor="w")

        self.playlist_subtitle_lbl = tk.Label(
            title_area, text="", font=("Helvetica Neue",10),
            fg=TEXT3, bg=BG, anchor="w"
        )
        self.playlist_subtitle_lbl.pack(anchor="w", pady=(2, 0))

        # Action row
        act = tk.Frame(self, bg=BG)
        act.grid(row=1, column=0, columnspan=2, sticky="ew", padx=28, pady=(14,6))

        # Flexible spacer columns provide the Tkinter equivalent of
        # justify-content: space-between for the five playlist controls.
        controls = tk.Frame(act, bg=BG)
        controls.pack(side="top", fill="x")
        for spacer in (1, 3, 5, 7):
            controls.grid_columnconfigure(spacer, weight=1)

        play_lbl = tk.Label(controls, text="▶", font=("Helvetica Neue",20,"bold"),
                            fg=SIDEBAR, bg=GREEN, padx=14, pady=6, cursor="hand2")
        play_lbl.grid(row=0, column=0, sticky="w")
        play_lbl.bind("<Button-1>", lambda e: self.controller._play_all())
        play_lbl.bind("<Enter>", lambda e: play_lbl.config(bg=GREEN_HOVER))
        play_lbl.bind("<Leave>", lambda e: play_lbl.config(bg=GREEN))

        self.shuf_lbl = tk.Label(controls, text="Shuffle", font=("Helvetica Neue",11),
                        fg=GREEN, bg=BG, cursor="hand2")
        self.shuf_lbl.grid(row=0, column=2, sticky="sw")
        self._shuffle_indicator = tk.Frame(controls, bg=GREEN, width=52, height=2)
        self._shuffle_indicator.grid(row=1, column=2, sticky="w", pady=(1, 0))
        self._shuffle_indicator.grid_propagate(False)
        self._shuffle_indicator.grid_remove()
        self.shuf_lbl.bind("<Button-1>", lambda e: self._on_shuffle_button_click())
        self.shuf_lbl.bind("<Enter>", lambda e: self.shuf_lbl.config(fg=GREEN_HOVER))
        self.shuf_lbl.bind("<Leave>", lambda e: self.on_shuffle_changed(self.controller.engine.shuffle))

        self.favorites_first_lbl = tk.Label(
            controls, text="☆  Favorites first", font=("Helvetica Neue", 10),
            fg=GREEN, bg=BG, cursor="hand2"
        )
        self.favorites_first_lbl.grid(row=0, column=4, sticky="sw")
        self.favorites_first_lbl.bind("<Button-1>", lambda e: self._toggle_favorites_first())
        self.favorites_first_lbl.bind("<Enter>", lambda e: self.favorites_first_lbl.config(fg=GREEN_HOVER))
        self.favorites_first_lbl.bind("<Leave>", lambda e: self._sync_favorites_first_label())

        self._actions_popup = None

        self.sort_lbl = tk.Label(
            controls, text="Sort  ▾", font=("Helvetica Neue", 10),
            fg=GREEN, bg=BG, cursor="hand2"
        )
        self.sort_lbl.grid(row=0, column=8, sticky="sw")
        self.sort_lbl.bind("<Button-1>", lambda e: self._show_sort_menu())
        self.sort_lbl.bind("<Enter>", lambda e: self.sort_lbl.config(fg=GREEN_HOVER))
        self.sort_lbl.bind("<Leave>", lambda e: self.sort_lbl.config(fg=GREEN))

        self.recs_btn = tk.Label(controls, text="✨  Get Similar Tracks", font=("Helvetica Neue",10),
                                 fg=GREEN, bg=BG, cursor="hand2")
        self.recs_btn.grid(row=0, column=6, sticky="sw")
        self.recs_btn.bind("<Button-1>", lambda e: self._open_discover())
        self.recs_btn.bind("<Enter>", lambda e: self.recs_btn.config(fg=GREEN_HOVER))
        self.recs_btn.bind("<Leave>", lambda e: self.recs_btn.config(fg=GREEN))

        # Outside-click handler registered once (not inside _show_playlist_actions)
        self.controller.bind_all("<Button-1>", self._on_app_click, add="+")

        # Thin green progress bar for playlist sync (hidden by default)
        self._sync_prog_frame = tk.Frame(act, bg=BG, height=3)
        self._sync_prog_bar = ttk.Progressbar(self._sync_prog_frame,
            variable=self.controller.prog_var, maximum=100,
            style="Green.Horizontal.TProgressbar")
        self._sync_prog_bar.pack(fill="x")
        self._sync_prog_frame.pack_forget()

        # In-playlist search bar
        search_row = tk.Frame(self, bg=BG)
        search_row.grid(row=2, column=0, columnspan=2, sticky="ew", padx=28, pady=(4, 12))

        sf = tk.Frame(search_row, bg=BG3, highlightbackground=BG4, highlightthickness=1)
        sf.pack(fill="x", ipady=3)
        tk.Label(sf, text="⌕", font=("Helvetica Neue",11), fg=TEXT3, bg=BG3,
                 padx=8).pack(side="left")
        self._pl_search_ph = "Filter tracks…"
        self._pl_search_entry = tk.Entry(sf, bg=BG3, fg=TEXT, insertbackground=TEXT,
                                          relief="flat", font=("Helvetica Neue",10), bd=0,
                                          highlightthickness=0, highlightbackground=BG3, highlightcolor=BG3)
        self._pl_search_entry.pack(side="left", fill="x", expand=True)

        self._pl_search_clear_btn = tk.Label(
            sf, text="×", font=("Helvetica Neue", 13, "bold"),
            fg=TEXT3, bg=BG3, cursor="hand2", padx=8
        )
        self._pl_search_clear_btn.bind("<Button-1>", lambda e: self._clear_playlist_search())
        self._pl_search_clear_btn.bind("<Enter>", lambda e: self._pl_search_clear_btn.config(fg=TEXT))
        self._pl_search_clear_btn.bind("<Leave>", lambda e: self._pl_search_clear_btn.config(fg=TEXT3))
        self._search_clear_visible = False

        self._pl_search_ph_lbl = tk.Label(sf, text=self._pl_search_ph, font=("Helvetica Neue",10), 
                                          fg=TEXT3, bg=BG3, cursor="xterm")
        # Initialize placeholder visibility state
        self._pl_search_ph_lbl.place(x=32, rely=0.5, anchor="w")
        
        self._pl_search_ph_lbl.bind("<Button-1>", lambda e: self._pl_search_entry.focus_set())
        
        self._pl_search_entry.bind("<FocusIn>", self._pl_search_focus_in)
        self._pl_search_entry.bind("<FocusOut>", self._pl_search_focus_out)
        self._pl_search_entry.bind("<KeyRelease>", self._pl_search_key)

        # Track List container
        self._track_frame = tk.Frame(self, bg=BG)
        self._track_frame.grid(row=3, column=0, columnspan=2, sticky="nsew", padx=(28,0), pady=(0,8))
        self._track_frame.grid_rowconfigure(0, weight=1)
        self._track_frame.grid_columnconfigure(0, weight=1)

        self._track_canvas = tk.Canvas(self._track_frame, bg=BG, highlightthickness=0, bd=0)
        # Redraw on scroll for virtual scrolling
        def _on_canvas_scroll(*args):
             self._track_canvas.yview(*args)
             self._redraw_track_list()
        
        from widgets import ModernScrollbar
        tr_sb = ModernScrollbar(self._track_frame, command=_on_canvas_scroll)
        self._track_canvas.config(yscrollcommand=tr_sb.set)
        tr_sb.grid(row=0, column=1, sticky="ns")
        self._track_canvas.grid(row=0, column=0, sticky="nsew")
        
        def _lib_wheel(event):
            # Only scroll if content exceeds the viewport
            rh = 56
            total_h = len(self._track_items) * rh
            canvas_h = self._track_canvas.winfo_height()
            if total_h <= canvas_h:
                return  # Content fits — no scrolling needed

            amount = scroll_amount(event, wheel_units=2)
            if not amount:
                return
            self._track_canvas.yview_scroll(amount, "units")
            self._schedule_track_redraw()
            
        bind_scroll_events(self._track_canvas, _lib_wheel)
        self._track_canvas.bind("<Configure>",   lambda e: self._redraw_track_list())
        
        self._track_canvas.bind("<Button-1>",    self._on_track_canvas_click)
        self._track_canvas.bind("<Double-Button-1>", self._on_track_canvas_dbl)
        self._track_canvas.bind("<Button-2>",    self._on_track_canvas_right)
        self._track_canvas.bind("<Button-3>",    self._on_track_canvas_right)
        self._track_canvas.bind("<Motion>", self._on_track_hover)
        self._track_canvas.bind("<Leave>",  lambda e: self._set_track_hover(None))

    # ── Track sorting ───────────────────────────────────────────────────────
    def _sort_scope(self):
        return self.controller._selected_playlist or "All Music"

    def _current_sort_mode(self):
        return self._sort_modes.get(self._sort_scope(), "A-Z")

    def _sync_sort_label(self):
        if hasattr(self, "sort_lbl") and self.sort_lbl.winfo_exists():
            self.sort_lbl.config(text="Sort  ▾", fg=GREEN)

    def _show_sort_menu(self):
        if getattr(self, "_sort_menu", None):
            try:
                self._sort_menu.unpost()
            except tk.TclError:
                pass

        menu = tk.Menu(
            self, tearoff=0, bg=BG3, fg=TEXT,
            activebackground=BG4, activeforeground=TEXT,
            font=("Helvetica Neue", 10), bd=0,
        )
        current = self._current_sort_mode()
        for mode in self._SORT_MODES:
            prefix = "✓  " if mode == current else "    "
            menu.add_command(
                label=f"{prefix}{mode}",
                command=lambda selected=mode: self._set_sort_mode(selected),
            )
        self._sort_menu = menu
        menu.post(
            self.sort_lbl.winfo_rootx(),
            self.sort_lbl.winfo_rooty() + self.sort_lbl.winfo_height(),
        )

    def _set_sort_mode(self, mode):
        if mode not in self._SORT_MODES:
            return
        self._sort_modes[self._sort_scope()] = mode
        if getattr(self, "_sort_menu", None):
            try:
                self._sort_menu.unpost()
            except tk.TclError:
                pass
            self._sort_menu = None
        self._sync_sort_label()
        self.populate_track_list(self._pl_search_entry.get().strip())
        self.reset_track_scroll()

    @staticmethod
    def _file_mtime(path):
        try:
            return os.path.getmtime(path)
        except OSError:
            return 0.0

    def _request_track_durations(self, paths):
        pending = [path for path in dict.fromkeys(paths) if path not in self._duration_pending]
        if not pending:
            return
        if self._duration_load_active:
            self._duration_deferred.update(pending)
            return
        self._duration_pending.update(pending)
        self._duration_load_active = True

        def load_durations():
            durations = {}
            try:
                from mutagen import File as MutagenFile
            except ImportError:
                durations = {path: 0.0 for path in pending}
            else:
                for path in pending:
                    duration = 0.0
                    try:
                        audio = MutagenFile(path)
                        duration = float(getattr(getattr(audio, "info", None), "length", 0.0) or 0.0)
                    except Exception:
                        pass
                    durations[path] = duration
            self.after(0, lambda: self._finish_track_durations(durations))

        threading.Thread(target=load_durations, daemon=True).start()

    def _finish_track_durations(self, durations):
        self._duration_cache.update(durations)
        self._duration_pending.difference_update(durations)
        self._duration_load_active = False
        deferred = self._duration_deferred
        self._duration_deferred = set()
        if deferred:
            self._request_track_durations(deferred)
        if self._current_sort_mode() == "Duration":
            self.populate_track_list(self._pl_search_entry.get().strip())
            self.reset_track_scroll()

    def prune_duration_cache(self, paths):
        """Drop duration metadata for files no longer in the scanned library."""
        allowed = {
            os.path.abspath(os.path.normpath(path))
            for path in paths
        }
        self._duration_cache = {
            path: duration
            for path, duration in self._duration_cache.items()
            if os.path.abspath(os.path.normpath(path)) in allowed
        }
        self._duration_pending = {
            path for path in self._duration_pending
            if os.path.abspath(os.path.normpath(path)) in allowed
        }
        self._duration_deferred = {
            path for path in self._duration_deferred
            if os.path.abspath(os.path.normpath(path)) in allowed
        }

    def _sort_track_items(self, items):
        mode = self._current_sort_mode()
        if mode == "Z-A":
            return sorted(items, key=lambda item: (item[1].casefold(), item[2].casefold()), reverse=True)
        if mode == "Recently added":
            return sorted(
                items,
                key=lambda item: (-self._file_mtime(item[0]), item[1].casefold(), item[2].casefold()),
            )
        if mode == "Duration":
            self._request_track_durations(
                item[0] for item in items if item[0] not in self._duration_cache
            )
            return sorted(
                items,
                key=lambda item: (
                    -self._duration_cache.get(item[0], 0.0),
                    item[1].casefold(),
                    item[2].casefold(),
                ),
            )
        return sorted(items, key=lambda item: (item[1].casefold(), item[2].casefold()))

    # ── Sync Progress Bar ──────────────────────────────────────────────────
    def show_sync_progress(self):
        self.controller.prog_var.set(0)
        self._sync_prog_frame.pack(side="bottom", fill="x")

    def hide_sync_progress(self):
        self._sync_prog_frame.pack_forget()

    # ── Playlist Actions Dropdown ────────────────────────────────────────────
    def _open_discover(self):
        """Fetch recommendations for the current context, then show Discover."""
        self.controller._trigger_discovery()
        self.controller._show_tab("discover")

    def _on_app_click(self, e):
        if self._actions_popup is None:
            return
        if e.widget == self.playlist_settings_btn:
            return
        if getattr(self, '_popup_just_opened', False):
            return
        try:
            wx = self._actions_popup.winfo_rootx()
            wy = self._actions_popup.winfo_rooty()
            ww = self._actions_popup.winfo_width()
            wh = self._actions_popup.winfo_height()
            if not (wx <= e.x_root <= wx + ww and wy <= e.y_root <= wy + wh):
                self._close_actions_popup()
        except:
            self._close_actions_popup()

    def _close_actions_popup(self):
        if self._actions_popup:
            # Auto-save URL before destroying
            entry = getattr(self, '_popup_url_entry', None)
            if entry and entry.winfo_exists():
                url = entry.get().strip()
                dest = getattr(self, '_popup_dest_var', None)
                if dest:
                    self.controller._set_playlist_url(dest.get(), url)
            try: self._actions_popup.destroy()
            except: pass
            self._actions_popup = None
            self._popup_url_entry = None
            self._popup_dest_var = None
            self.focus_set()

    def _filter_dest_combobox(self, event, combobox, all_playlists):
        typed = combobox.get().lower()
        if not typed:
            combobox["values"] = all_playlists
        else:
            combobox["values"] = [p for p in all_playlists if typed in p.lower()]
        combobox.event_generate("<Down>")

    def _show_playlist_actions(self):
        """Show the playlist actions dropdown popup. Toggle close if already open."""
        if self._actions_popup:
            self._close_actions_popup()
            return

        pl_name = self.controller._selected_playlist or "All Music"
        all_playlists = [p for p in sorted(self.controller._library.keys())
                         if p not in ("Unsorted", "All Music", "Favorites")]

        popup = tk.Toplevel(self)
        popup.title("")
        popup.configure(bg=BG2)
        popup.resizable(False, False)
        popup.overrideredirect(True)

        # Position below the actions button
        btn_x = self.playlist_settings_btn.winfo_rootx()
        btn_y = self.playlist_settings_btn.winfo_rooty()
        btn_h = self.playlist_settings_btn.winfo_height()
        popup.geometry(f"420x390+{btn_x}+{btn_y + btn_h + 4}")

        popup.columnconfigure(0, weight=1)

        # ── URL Row ──
        tk.Label(popup, text="🔗  Playlist URL", font=("Helvetica Neue", 9, "bold"),
                 fg=TEXT2, bg=BG2, anchor="w").grid(row=0, column=0, sticky="ew", padx=16, pady=(16, 4))

        url_frame = tk.Frame(popup, bg=BG3, highlightbackground=BG4, highlightthickness=1)
        url_frame.grid(row=1, column=0, sticky="ew", padx=16, pady=(0, 8))
        current_url = self.controller._get_playlist_url(pl_name) or ""
        url_entry = tk.Entry(url_frame, bg=BG3, fg=TEXT, insertbackground=TEXT,
                             relief="flat", font=("Helvetica Neue", 10), bd=6,
                             highlightthickness=0)
        url_entry.pack(side="left", fill="x", expand=True)
        if current_url:
            url_entry.insert(0, current_url)

        # ── Destination Row ──
        tk.Label(popup, text="📁  Destination Playlist", font=("Helvetica Neue", 9, "bold"),
                 fg=TEXT2, bg=BG2, anchor="w").grid(row=2, column=0, sticky="ew", padx=16, pady=(8, 4))

        dest_var = tk.StringVar(value=pl_name if pl_name in all_playlists else (all_playlists[0] if all_playlists else ""))
        dest_combo = ttk.Combobox(popup, textvariable=dest_var, values=all_playlists,
                                   font=("Helvetica Neue", 10), state="normal", width=30)
        dest_combo.grid(row=3, column=0, sticky="ew", padx=16, pady=(0, 12))
        dest_combo.bind("<KeyRelease>", lambda e: self._filter_dest_combobox(e, dest_combo, all_playlists))

        # Store on instance so _close_actions_popup can auto-save
        self._popup_url_entry = url_entry
        self._popup_dest_var = dest_var

        # ── Action Buttons ──
        btn_f = tk.Frame(popup, bg=BG2)
        btn_f.grid(row=4, column=0, sticky="ew", padx=16, pady=(4, 4))
        btn_f.columnconfigure((0, 1), weight=1)

        def do_check_dupes():
            dest = dest_var.get()
            self._close_actions_popup()
            self.controller._check_playlist_duplicates(dest)

        def do_sync():
            url = url_entry.get().strip()
            dest = dest_var.get()
            self._close_actions_popup()
            if url:
                parsed = urlparse(url)
                if parsed.scheme not in ('http', 'https') or not parsed.netloc:
                    messagebox.showwarning("Invalid URL", "Must be a valid http/https URL.")
                    return
                self.controller._set_playlist_url(dest, url)
            else:
                self.controller._set_playlist_url(dest, "")
            self.controller._sync_playlist(dest, url=url if url else None)

        dupes_btn = tk.Label(btn_f, text="⚠  Check Duplicates",
                             font=("Helvetica Neue", 10, "bold"),
                             fg=TEXT2, bg=BG3, padx=10, pady=6, cursor="hand2")
        dupes_btn.grid(row=0, column=0, padx=(0, 4), sticky="ew")
        dupes_btn.bind("<Button-1>", lambda e: do_check_dupes())
        dupes_btn.bind("<Enter>", lambda e: dupes_btn.config(fg=TEXT, bg=BG4))
        dupes_btn.bind("<Leave>", lambda e: dupes_btn.config(fg=TEXT2, bg=BG3))

        sync_btn = tk.Label(btn_f, text="⬇  Sync Now",
                            font=("Helvetica Neue", 10, "bold"),
                            fg=SIDEBAR, bg=GREEN, padx=10, pady=6, cursor="hand2")
        sync_btn.grid(row=0, column=1, padx=(4, 0), sticky="ew")
        sync_btn.bind("<Button-1>", lambda e: do_sync())
        sync_btn.bind("<Enter>", lambda e: sync_btn.config(bg=GREEN_HOVER))
        sync_btn.bind("<Leave>", lambda e: sync_btn.config(bg=GREEN))

        export_f = tk.Frame(popup, bg=BG2)
        export_f.grid(row=5, column=0, sticky="ew", padx=16, pady=(4, 4))
        export_f.columnconfigure((0, 1), weight=1)

        export_btn = tk.Label(
            export_f,
            text="⇧  Export JSON",
            font=("Helvetica Neue", 10),
            fg=TEXT2,
            bg=BG3,
            padx=10,
            pady=6,
            cursor="hand2",
        )
        export_btn.grid(row=0, column=0, padx=(0, 4), sticky="ew")
        export_btn.bind("<Button-1>", lambda e: (self._close_actions_popup(), self.controller._export_playlist_json()))
        export_btn.bind("<Enter>", lambda e: export_btn.config(fg=TEXT, bg=BG4))
        export_btn.bind("<Leave>", lambda e: export_btn.config(fg=TEXT2, bg=BG3))

        import_btn = tk.Label(
            export_f,
            text="⇩  Import JSON",
            font=("Helvetica Neue", 10),
            fg=TEXT2,
            bg=BG3,
            padx=10,
            pady=6,
            cursor="hand2",
        )
        import_btn.grid(row=0, column=1, padx=(4, 0), sticky="ew")
        import_btn.bind("<Button-1>", lambda e: (self._close_actions_popup(), self.controller._import_json()))
        import_btn.bind("<Enter>", lambda e: import_btn.config(fg=TEXT, bg=BG4))
        import_btn.bind("<Leave>", lambda e: import_btn.config(fg=TEXT2, bg=BG3))

        # ── Close button at bottom ──
        close_btn = tk.Label(popup, text="✕  Close", font=("Helvetica Neue", 10),
                             fg=TEXT3, bg=BG2, pady=6, cursor="hand2")
        close_btn.grid(row=6, column=0, sticky="ew", padx=16, pady=(0, 12))
        close_btn.bind("<Button-1>", lambda e: self._close_actions_popup())
        close_btn.bind("<Enter>", lambda e: close_btn.config(fg=TEXT, bg=BG3))
        close_btn.bind("<Leave>", lambda e: close_btn.config(fg=TEXT3, bg=BG2))

        # Close on Escape
        popup.bind("<Escape>", lambda e: self._close_actions_popup())

        self._actions_popup = popup
        self._popup_just_opened = True
        self.after(150, lambda: setattr(self, '_popup_just_opened', False))

    # ── Playlist search helpers ──────────────────────────────────────────────
    def _sync_search_clear(self):
        should_show = bool(self._pl_search_entry.get())
        if should_show and not self._search_clear_visible:
            self._pl_search_clear_btn.pack(side="right", padx=(0, 2))
            self._search_clear_visible = True
        elif not should_show and self._search_clear_visible:
            self._pl_search_clear_btn.pack_forget()
            self._search_clear_visible = False

    def _clear_playlist_search(self):
        self._pl_search_entry.delete(0, "end")
        self._sync_search_clear()
        self._update_placeholder()
        self.populate_track_list("")
        self._pl_search_entry.focus_set()

    def _update_placeholder(self):
        self._sync_search_clear()
        if not self._pl_search_entry.get():
            # If empty and we are NOT focused, show it. The user also said "reappear when not focused".
            # Standard HTML: always show if empty. But let's follow standard HTML.
            self._pl_search_ph_lbl.place(x=32, rely=0.5, anchor="w")
        else:
            self._pl_search_ph_lbl.place_forget()

    def _pl_search_focus_in(self, event):
        self._pl_search_ph_lbl.place_forget()

    def _pl_search_focus_out(self, event):
        self._update_placeholder()
        self.populate_track_list(self._pl_search_entry.get().strip())

    def _pl_search_key(self, event):
        self._update_placeholder()
        q = self._pl_search_entry.get()
        self.populate_track_list(q)

    def _start_pulse(self):
        if self._pulse_running or not hasattr(self.controller, 'engine'):
            return
        self._pulse_running = True
        self._pulse_tick()

    def _pulse_tick(self):
        if not self._pulse_running or not hasattr(self.controller, 'engine'):
            self._pulse_running = False
            return
        cur_path = self.controller.engine.queue[self.controller.engine.queue_idx] if 0 <= self.controller.engine.queue_idx < len(self.controller.engine.queue) else ""
        has_playing = any(path == cur_path for path, _, _, _ in self._track_items)
        if has_playing:
            self._pulse_frame = (self._pulse_frame + 1) % 100
            self._update_pulse()
        self.after(80, self._pulse_tick)

    def _update_pulse(self):
        """Animate only the equalizer bars on the playing row. The 80 ms
        heartbeat must never rebuild the whole canvas — that full redraw every
        tick leaks Tk canvas-item memory and grows the process for the whole
        session (mirrors RecentView._update_pulse)."""
        items = getattr(self, "_pulse_items", None)
        if not items or not self.winfo_ismapped():
            return
        c = self._track_canvas
        pulse = self._pulse_frame
        bar_h = 14
        for (item, x0, cy, x1, bi) in items:
            try:
                h_off = int(abs(math.sin((pulse / 20.0) + bi * 1.5)) * (bar_h - 4) + 4)
                c.coords(item, x0, cy - h_off // 2, x1, cy + h_off // 2)
            except tk.TclError:
                self._pulse_items = []
                return

    def on_shuffle_changed(self, is_shuffled):
        """Sync the green Shuffle control and its active underline."""
        self.shuf_lbl.config(
            text="Shuffle",
            fg=GREEN,
            font=("Helvetica Neue", 11, "bold")
            if is_shuffled else ("Helvetica Neue", 11),
        )
        self._shuffle_indicator.config(width=self.shuf_lbl.winfo_reqwidth())
        if is_shuffled:
            self._shuffle_indicator.grid()
        else:
            self._shuffle_indicator.grid_remove()

    def _toggle_favorites_first(self):
        self._favorites_first = not self._favorites_first
        self._sync_favorites_first_label()
        self.populate_track_list(self._pl_search_entry.get().strip())

    def _sync_favorites_first_label(self):
        self.favorites_first_lbl.config(
            text="★  Favorites first" if self._favorites_first else "☆  Favorites first",
            fg=GREEN,
            font=("Helvetica Neue", 10, "bold" if self._favorites_first else "normal"),
        )

    def _draw_lib_header(self):
        w = self._hdr_canvas.winfo_width()
        self._hdr_canvas.delete("grad")
        pl = self.controller._selected_playlist or "All Music"
        c1, _ = playlist_gradient(pl)
        # Blend playlist color into BG
        r1,g1,b1 = hex_to_rgb(c1)
        r2,g2,b2 = hex_to_rgb(BG)
        steps = 40
        h = 220
        
        for i in range(steps):
            t  = i/steps
            r  = int(r1+(r2-r1)*t*1.2)
            g  = int(g1+(g2-g1)*t*1.2)
            b  = int(b1+(b2-b1)*t*1.2)
            r,g,b = max(0,min(255,r)),max(0,min(255,g)),max(0,min(255,b))
            y  = int(h*i/steps)
            yn = int(h*(i+1)/steps)
            self._hdr_canvas.create_rectangle(0, y, w, yn,
                                               fill=rgb_to_hex(r,g,b), outline="",
                                               tags="grad")

    def populate_track_list(self, filter_str=""):
        if self.controller._selected_playlist == "Favorites":
            ordered_playlists = [("Favorites", self.controller._favorite_paths_in_library())]
        elif self.controller._selected_playlist and self.controller._selected_playlist in self.controller._library:
            # Single playlist selected
            ordered_playlists = [(self.controller._selected_playlist,
                                  self.controller._library[self.controller._selected_playlist])]
        else:
            # "All Music" — enforce canonical order: Unsorted first, then alphabetical
            lib = self.controller._library
            ordered_playlists = []
            if "Unsorted" in lib:
                ordered_playlists.append(("Unsorted", lib["Unsorted"]))
            for name in sorted(lib.keys()):
                if name not in ["Unsorted", "All Music", "Favorites"]:
                    ordered_playlists.append((name, lib[name]))

        f = filter_str.strip().lower()
        self._track_items = []
        for pl_name, tracks in ordered_playlists:
            for path in tracks:
                title, artist = split_track_name(path)
                if f and f not in title.lower() and f not in artist.lower():
                    continue
                self._track_items.append((path, title, artist, pl_name))
        self._track_items = self._sort_track_items(self._track_items)
        if self._favorites_first:
            self._track_items.sort(key=lambda item: not self.controller._is_favorite(item[0]))
        self._track_sel_idx = []
        self.after(2000, self._start_pulse)
        self._redraw_track_list()

    def get_selected_track_path(self):
        """Returns the first selected track path in the library list, if any."""
        if hasattr(self, '_track_sel_idx') and self._track_sel_idx:
            # We assume single selection or take the first of a multi-selection
            idx = self._track_sel_idx[0]
            if 0 <= idx < len(self._track_items):
                return self._track_items[idx][0]
        return None

    def _on_art_loaded(self, path):
        if not hasattr(self, "_art_redraw_pending") or not self._art_redraw_pending:
            self._art_redraw_pending = True
            self.after(50, self._do_art_redraw)
            
    def _do_art_redraw(self):
        self._art_redraw_pending = False
        self._redraw_track_list()

    def _redraw_track_list(self):
        c = self._track_canvas
        c.delete("all")
        self._pulse_items = []
        w = c.winfo_width()
        if w < 10: return
        
        rh = 56 # Height of one row
        total_items = len(self._track_items)
        canvas_h = c.winfo_height()
        content_h = total_items * rh
        
        # If content fits within the viewport, lock scrollregion to canvas height
        # to prevent any scrolling. Otherwise allow natural overflow.
        if content_h <= canvas_h:
            full_h = canvas_h
            # Reset scroll position to top when content fits
            c.yview_moveto(0)
        else:
            full_h = content_h
        c.configure(scrollregion=(0, 0, w, full_h))
        self._favorite_star_positions = {}

        # Get visible range
        v0, v1 = c.yview()
        # Clamp v0 to 0 to prevent negative scroll calculations (elastic bounce)
        v0_clamped = max(0, v0)
        
        # Calculate start and end index relative to the clamped scroll position
        # Using a larger buffer (+5) to prevent "blackness" while scrolling fast
        start_idx = max(0, int((v0_clamped * full_h) // rh) - 5)
        end_idx   = min(total_items, int((v1 * full_h) // rh) + 5)

        cur_path = self.controller.engine.queue[self.controller.engine.queue_idx] if 0 <= self.controller.engine.queue_idx < len(self.controller.engine.queue) else ""

        for i in range(start_idx, end_idx):
            path, title, artist, pl_name = self._track_items[i]
            y0, y1 = i * rh, (i + 1) * rh
            
            is_playing = (path == cur_path)
            is_sel = (i in self._track_sel_idx)
            is_hover = (i == self._track_hover_idx)

            bg = BG4 if is_sel else (BG3 if is_hover else (BG if i % 2 == 0 else "#161616"))
            c.create_rectangle(0, y0, w, y1, fill=bg, outline="", tags="row")

            # Art strip: 44px instead of 36px
            art_size = 44
            art_half = art_size // 2
            art_x = 30
            art_yc = y0 + rh // 2
            img = self.controller.art_cache.get(path, art_size, self._on_art_loaded)
            if img:
                c.create_image(art_x, art_yc, image=img, tags="row")
            else:
                c1, _ = playlist_gradient(pl_name)
                c.create_rectangle(art_x - art_half, art_yc - art_half, art_x + art_half, art_yc + art_half, fill=c1, outline="", tags="row")
                c.create_text(art_x, art_yc, text="♫", font=("Helvetica", 14), fill="white", tags="row")

            # Playing indicator: animated bars
            bar_x = art_x + art_half + 10
            if is_playing:
                bar_h = 14
                bar_w = 3
                gap = 2
                n_bars = 4
                cy = art_yc + bar_h // 2
                for bi in range(n_bars):
                    x0 = bar_x + bi * (bar_w + gap)
                    item = c.create_rectangle(x0, cy - bar_h // 2,
                                              x0 + bar_w, cy + bar_h // 2,
                                              fill=GREEN, outline="",
                                              tags=("row", "pulse"))
                    self._pulse_items.append((item, x0, cy, x0 + bar_w, bi))
            else:
                c.create_text(bar_x, art_yc, text=str(i+1), font=("Helvetica", 10), fill=TEXT3, tags="row")

            # Calculate available width for title/artist to prevent overflow into the playlist column
            # Playlist name column is anchored 'e' at w - 25.
            # Using w - 340 to account for larger art
            max_text_w = max(100, w - 340)
            
            def truncate_text(s, available_w, font_size=11, is_bold=False):
                char_w = font_size * (0.85 if is_bold else 0.7)
                limit = int(available_w // char_w)
                if len(s) > limit and limit > 3:
                    return s[:limit-3] + "..."
                return s

            title_disp = truncate_text(title, max_text_w, 11, is_bold=True)
            artist_disp = truncate_text(artist, max_text_w, 9)
            pl_disp = truncate_text(pl_name, 120, 9)

            c.create_text(bar_x + 30, y0+18, text=title_disp, font=("Helvetica Neue", 11, "bold"), fill=GREEN if is_playing else TEXT, anchor="w", tags="row")
            c.create_text(bar_x + 30, y0+36, text=artist_disp, font=("Helvetica Neue", 9), fill=TEXT3, anchor="w", tags="row")

            playlist_item = c.create_text(
                w - 25, y0+rh//2, text=pl_disp,
                font=("Helvetica Neue", 9), fill=TEXT3, anchor="e", tags="row"
            )
            playlist_bbox = c.bbox(playlist_item)
            star_x = playlist_bbox[0] - 16 if playlist_bbox else w - 155
            self._favorite_star_positions[i] = star_x
            c.create_text(
                star_x, y0 + rh // 2,
                text="★" if self.controller._is_favorite(path) else "☆",
                font=("Helvetica Neue", 12),
                fill=GREEN if self.controller._is_favorite(path) else TEXT3,
                anchor="center", tags=("row", "favorite_star")
            )
            c.create_line(12, y1-1, w-12, y1-1, fill=BORDER, tags="row")

    def _schedule_track_redraw(self):
        """Coalesce high-resolution scroll redraws to keep the UI responsive."""
        if self._track_redraw_after is None:
            self._track_redraw_after = self.after(16, self._flush_track_redraw)

    def _flush_track_redraw(self):
        self._track_redraw_after = None
        if self.winfo_exists():
            self._redraw_track_list()

    def _row_at_y(self, y):
        cy = self._track_canvas.canvasy(y)
        idx = int(cy // 56)
        return idx if 0 <= idx < len(self._track_items) else None

    def _set_track_hover(self, idx):
        if self._track_hover_idx != idx:
            self._track_hover_idx = idx
            self._redraw_track_list()

    def _on_track_hover(self, event):
        idx = self._row_at_y(event.y)
        if idx != self._track_hover_idx:
            self._track_hover_idx = idx
            self._redraw_track_list()

    def _on_track_canvas_click(self, event):
        idx = self._row_at_y(event.y)
        if idx is not None:
            # Keep the star independent from row selection so a favorite click
            # does not change the active multi-selection.
            star_x = self._favorite_star_positions.get(
                idx, self._track_canvas.winfo_width() - 155
            )
            if abs(event.x - star_x) <= 14:
                self.controller._toggle_favorite_paths([self._track_items[idx][0]])
                return "break"

            if not isinstance(self._track_sel_idx, list):
                self._track_sel_idx = []

            # 1. Multi-select with Cmd/Ctrl (Toggle)
            if (event.state & 0x0004) or (event.state & 0x0008): 
                if idx in self._track_sel_idx:
                    self._track_sel_idx.remove(idx)
                else:
                    self._track_sel_idx.append(idx)
                    self._sel_anchor = idx
            
            # 2. Multi-select with Shift (Range)
            elif (event.state & 0x0001) and self._sel_anchor is not None:
                start = self._sel_anchor
                step = 1 if idx >= start else -1
                # In range selection, we typically replace the list if only shift is held,
                # or add to it. Standard OS behavior usually replaces current range.
                self._track_sel_idx = []
                for i in range(start, idx + step, step):
                    self._track_sel_idx.append(i)
            
            # 3. Single select (Normal)
            else:
                self._track_sel_idx = [idx]
                self._sel_anchor = idx
            
            self._redraw_track_list()

    def _on_track_canvas_dbl(self, event):
        idx = self._row_at_y(event.y)
        if idx is not None and idx < len(self._track_items):
            paths = [item[0] for item in self._track_items]
            self.controller._play_with_context(self._track_items[idx][0], context_list=paths)

    def _on_shuffle_button_click(self):
        """Standard behavior: If NOT playing, OR if clicking on a different playlist/context,
        start a fresh random session. Otherwise, just toggle the engine state."""
        current_q_name = self.controller._queue_playlist_name
        this_pl_name = self.controller._selected_playlist or "Selection"
        
        # If nothing playing OR we are looking at a different playlist than the one currently in queue
        if not self.controller.engine.playing or current_q_name != this_pl_name:
            if self._track_items:
                paths = [item[0] for item in self._track_items]
                start_path = random.choice(paths)
                self.controller.engine.shuffle = True # Force on
                self.controller._play_with_context(start_path, context_list=paths)
                self.controller.engine.c._queue_playlist_name = this_pl_name # Explicit sync
                self.controller._broadcast_shuffle_state()
        else:
            self.controller._toggle_shuffle_ui()

    def _on_track_canvas_right(self, event):
        idx = self._row_at_y(event.y)
        if idx is not None:
            if idx not in self._track_sel_idx:
                self._track_sel_idx = [idx]
            self._redraw_track_list()
            self.controller._prepare_favorite_menu(self.ctx_menu, self._favorite_menu_index)
            self.ctx_menu.post(event.x_root, event.y_root)

    def get_sel_paths(self):
        paths = []
        for idx in self._track_sel_idx:
            if idx < len(self._track_items):
                paths.append(self._track_items[idx][0])
        return paths

    def _get_current_playlist_paths(self):
        paths = []
        if self.controller._selected_playlist == "Favorites":
            paths = self.controller._favorite_paths_in_library()
        elif self.controller._selected_playlist and self.controller._selected_playlist in self.controller._library:
            paths = self.controller._library[self.controller._selected_playlist]
        else:
            for tracks in self.controller._library.values():
                paths.extend(tracks)
        return paths

    def update_header_art(self, text, meta_text, subtitle_text=""):
        self.playlist_title_lbl.config(text=text)
        self.playlist_meta_lbl.config(text=meta_text)
        self.playlist_subtitle_lbl.config(text=subtitle_text)
        self._current_art.update_art(text, self._get_current_playlist_paths())
        self._draw_lib_header()
        self._sync_sort_label()
        # Reset in-playlist search when switching playlists
        self._pl_search_entry.delete(0, "end")
        self._update_placeholder()
        self._pl_search_entry.config(fg=TEXT) # Use normal text color

    def reset_track_scroll(self):
        """Return the virtual track list to its first row after navigation."""
        self._track_canvas.yview_moveto(0)
        self._redraw_track_list()
