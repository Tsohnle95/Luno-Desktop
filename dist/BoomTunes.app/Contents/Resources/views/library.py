import tkinter as tk
from tkinter import ttk, messagebox
from urllib.parse import urlparse
import math
from theme import BG, BG2, BG3, BG4, BORDER, GREEN, GREEN_HOVER, TEXT, TEXT2, TEXT3, WARNING, SIDEBAR
from utils import split_track_name, hex_to_rgb, rgb_to_hex
from widgets import PlaylistArt
from theme import playlist_gradient
import random

class LibraryView(tk.Frame):
    def __init__(self, parent, controller):
        super().__init__(parent, bg=BG)
        self.controller = controller
        
        self._track_items   = []
        self._track_sel_idx = []
        self._sel_anchor = None
        self._track_hover_idx = None
        self._pulse_frame = 0
        self._pulse_running = False
        
        # Context menu
        self.ctx_menu = tk.Menu(self, tearoff=0, bg=BG3, fg=TEXT,
                                activebackground=BG4, activeforeground=TEXT,
                                font=("Helvetica Neue",12), bd=0, relief="flat")
        self.ctx_menu.add_command(label="  ▶   Play Now",          command=self.controller._ctx_play_now)
        self.ctx_menu.add_command(label="  ↑   Play Next",         command=self.controller._ctx_play_next)
        self.ctx_menu.add_command(label="  +   Add to Queue",      command=self.controller._ctx_add_queue)
        self.ctx_menu.add_separator()
        self.ctx_menu.add_command(label="  ♫   Add to Playlist…",  command=self.controller._ctx_add_playlist)
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

        self.viz_toggle_btn = tk.Label(self._hdr_overlay, text="〰", font=("Helvetica",22), fg=TEXT3, bg=BG, cursor="hand2")
        self.viz_toggle_btn.place(relx=1.0, rely=0.0, x=-28, y=18, anchor="ne")
        self.viz_toggle_btn.bind("<Button-1>", lambda e: self._toggle_viz())
        self.viz_toggle_btn.bind("<Enter>", lambda e: self.viz_toggle_btn.config(fg=TEXT))
        self.viz_toggle_btn.bind("<Leave>", lambda e: self._sync_viz_icon())
        self.after(200, self._sync_viz_icon)
        self.after(2000, self._sync_viz_icon) # Extra sync for safety after init

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

        self.viz_canvas = tk.Canvas(title_area, bg=BG, height=28, highlightthickness=0)
        self.viz_canvas.pack(anchor="w", fill="x", pady=(8,0))
        self.after(500, self._draw_visualizer)

        # Action row
        act = tk.Frame(self, bg=BG)
        act.grid(row=1, column=0, columnspan=2, sticky="ew", padx=28, pady=(14,6))

        play_lbl = tk.Label(act, text="▶", font=("Helvetica Neue",20,"bold"),
                            fg=SIDEBAR, bg=GREEN, padx=14, pady=6, cursor="hand2")
        play_lbl.pack(side="left", padx=(0,16))
        play_lbl.bind("<Button-1>", lambda e: self.controller._play_all())
        play_lbl.bind("<Enter>", lambda e: play_lbl.config(bg=GREEN_HOVER))
        play_lbl.bind("<Leave>", lambda e: play_lbl.config(bg=GREEN))

        self.shuf_lbl = tk.Label(act, text="⇄  Shuffle", font=("Helvetica Neue",11),
                        fg=TEXT2, bg=BG, cursor="hand2")
        self.shuf_lbl.pack(side="left", padx=(0,20))
        self.shuf_lbl.bind("<Button-1>", lambda e: self._on_shuffle_button_click())
        self.shuf_lbl.bind("<Enter>", lambda e: self.shuf_lbl.config(fg=TEXT))
        self.shuf_lbl.bind("<Leave>", lambda e: self.shuf_lbl.config(fg=GREEN if self.controller.engine.shuffle else TEXT2))

        recs_btn = tk.Label(act, text="✨  Get Similar Tracks", font=("Helvetica Neue",10),
                             fg=TEXT3, bg=BG, cursor="hand2")
        recs_btn.pack(side="left", padx=(0,20))
        recs_btn.bind("<Button-1>", lambda e: self.controller._trigger_discovery())
        recs_btn.bind("<Enter>", lambda e: recs_btn.config(fg="#F1C40F"))
        recs_btn.bind("<Leave>", lambda e: recs_btn.config(fg=TEXT3))

        self._actions_popup = None

        self.pl_actions_btn = tk.Label(act, text="⚙  Playlist", font=("Helvetica Neue",10),
                                       fg=TEXT3, bg=BG, cursor="hand2")
        self.pl_actions_btn.pack(side="right")
        self.pl_actions_btn.bind("<Button-1>", lambda e: self._show_playlist_actions())
        self.pl_actions_btn.bind("<Enter>", lambda e: self.pl_actions_btn.config(fg=TEXT))
        self.pl_actions_btn.bind("<Leave>", lambda e: self.pl_actions_btn.config(fg=TEXT3))

        # Outside-click handler registered once (not inside _show_playlist_actions)
        self.controller.bind_all("<Button-1>", self._on_app_click, add="+")

        # Thin green progress bar for playlist sync (hidden by default)
        self._sync_prog_frame = tk.Frame(act, bg=BG, height=3)
        self._sync_prog_bar = ttk.Progressbar(self._sync_prog_frame,
            variable=self.controller.prog_var, maximum=100,
            style="Green.Horizontal.TProgressbar")
        self._sync_prog_bar.pack(fill="x")
        self._sync_prog_frame.pack_forget()

        self._act_sep = tk.Frame(act, bg=BG3, height=1)
        self._act_sep.pack(side="bottom", fill="x")

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

            if event.delta > 0:
                self._track_canvas.yview_scroll(-2, "units")
            else:
                self._track_canvas.yview_scroll(2, "units")
            self._redraw_track_list() # Crucial: Redraw after wheel scroll
            
        self._track_canvas.bind("<MouseWheel>", _lib_wheel)
        self._track_canvas.bind("<Configure>",   lambda e: self._redraw_track_list())
        
        self._track_canvas.bind("<Button-1>",    self._on_track_canvas_click)
        self._track_canvas.bind("<Double-Button-1>", self._on_track_canvas_dbl)
        self._track_canvas.bind("<Button-2>",    self._on_track_canvas_right)
        self._track_canvas.bind("<Button-3>",    self._on_track_canvas_right)
        self._track_canvas.bind("<Motion>", self._on_track_hover)
        self._track_canvas.bind("<Leave>",  lambda e: self._set_track_hover(None))

    # ── Sync Progress Bar ──────────────────────────────────────────────────
    def show_sync_progress(self):
        self.controller.prog_var.set(0)
        self._sync_prog_frame.pack(side="bottom", fill="x", before=self._act_sep)

    def hide_sync_progress(self):
        self._sync_prog_frame.pack_forget()

    # ── Playlist Actions Dropdown ────────────────────────────────────────────
    def _on_app_click(self, e):
        if self._actions_popup is None:
            return
        if e.widget == self.pl_actions_btn:
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

        pl_name = self.controller._selected_playlist
        all_playlists = [p for p in sorted(self.controller._library.keys())
                         if p not in ("Unsorted", "All Music")]

        popup = tk.Toplevel(self)
        popup.title("")
        popup.configure(bg=BG2)
        popup.resizable(False, False)
        popup.overrideredirect(True)

        # Position below the actions button
        btn_x = self.pl_actions_btn.winfo_rootx()
        btn_y = self.pl_actions_btn.winfo_rooty()
        btn_h = self.pl_actions_btn.winfo_height()
        popup.geometry(f"420x310+{btn_x}+{btn_y + btn_h + 4}")

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

        # ── Close button at bottom ──
        close_btn = tk.Label(popup, text="✕  Close", font=("Helvetica Neue", 10),
                             fg=TEXT3, bg=BG2, pady=6, cursor="hand2")
        close_btn.grid(row=5, column=0, sticky="ew", padx=16, pady=(0, 12))
        close_btn.bind("<Button-1>", lambda e: self._close_actions_popup())
        close_btn.bind("<Enter>", lambda e: close_btn.config(fg=TEXT, bg=BG3))
        close_btn.bind("<Leave>", lambda e: close_btn.config(fg=TEXT3, bg=BG2))

        # Close on Escape
        popup.bind("<Escape>", lambda e: self._close_actions_popup())

        self._actions_popup = popup
        self._popup_just_opened = True
        self.after(150, lambda: setattr(self, '_popup_just_opened', False))

    # ── Playlist search helpers ──────────────────────────────────────────────
    def _update_placeholder(self):
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
            self._redraw_track_list()
        self.after(80, self._pulse_tick)

    def on_shuffle_changed(self, is_shuffled):
        """Callback from controller to sync shuffle icon color."""
        if is_shuffled:
            self.shuf_lbl.config(fg=GREEN)
        else:
            self.shuf_lbl.config(fg=TEXT2)

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

    def _toggle_viz(self):
        if hasattr(self.controller, '_frames') and 'settings' in self.controller._frames:
            s = self.controller._frames['settings']
            s._toggle_visualizer()
            self._sync_viz_icon()

    def _sync_viz_icon(self):
        if hasattr(self.controller, '_frames') and 'settings' in self.controller._frames:
            on = self.controller._frames['settings'].viz_enabled
            self.viz_toggle_btn.config(fg=GREEN if on else TEXT3)

    def _draw_visualizer(self):
        c = self.viz_canvas
        c.delete("all")
        w = c.winfo_width(); h = c.winfo_height()
        
        viz_enabled = True
        if hasattr(self.controller, '_frames') and 'settings' in self.controller._frames:
            viz_enabled = self.controller._frames['settings'].viz_enabled
            
        if w < 10 or h < 10 or not viz_enabled:
            self.after(500, self._draw_visualizer)
            return

        bars = 32
        bar_w = max(1, (w - bars) // bars)
        c1, _ = playlist_gradient(self.controller._selected_playlist or "vibe")
        
        for i in range(bars):
            if hasattr(self.controller, 'engine') and self.controller.engine.playing and not self.controller.engine.paused:
                height = random.randint(4, h)
            else:
                height = 4
            x0 = i * (bar_w + 1)
            x1 = x0 + bar_w
            r1,g1,b1 = hex_to_rgb(c1)
            r2,g2,b2 = hex_to_rgb(GREEN)
            t = i / bars
            col = rgb_to_hex(int(r1+(r2-r1)*t), int(g1+(g2-g1)*t), int(b1+(b2-b1)*t))
            c.create_rectangle(x0, h-height, x1, h, fill=col, outline="")
        
        self.after(100, self._draw_visualizer)

    def populate_track_list(self, filter_str=""):
        if self.controller._selected_playlist and self.controller._selected_playlist in self.controller._library:
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
                if name not in ["Unsorted", "All Music"]:
                    ordered_playlists.append((name, lib[name]))

        f = filter_str.strip().lower()
        self._track_items = []
        for pl_name, tracks in ordered_playlists:
            for path in tracks:
                title, artist = split_track_name(path)
                if f and f not in title.lower() and f not in artist.lower():
                    continue
                self._track_items.append((path, title, artist, pl_name))
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

        # Get visible range
        v0, v1 = c.yview()
        # Clamp v0 to 0 to prevent negative scroll calculations (elastic bounce)
        v0_clamped = max(0, v0)
        
        # Calculate start and end index relative to the clamped scroll position
        # Using a larger buffer (+5) to prevent "blackness" while scrolling fast
        start_idx = max(0, int((v0_clamped * full_h) // rh) - 5)
        end_idx   = min(total_items, int((v1 * full_h) // rh) + 5)

        cur_path = self.controller.engine.queue[self.controller.engine.queue_idx] if 0 <= self.controller.engine.queue_idx < len(self.controller.engine.queue) else ""

        # Playing animation frame: pulse the indicator slightly
        pulse = 0
        if hasattr(self, '_pulse_frame'):
            pulse = self._pulse_frame or 0
        
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
                for bi in range(n_bars):
                    h_off = int(abs(math.sin((pulse / 20.0) + bi * 1.5)) * (bar_h - 4) + 4)
                    cy = art_yc + bar_h//2
                    c.create_rectangle(bar_x + bi*(bar_w+gap), cy - h_off//2,
                                       bar_x + bi*(bar_w+gap) + bar_w, cy + h_off//2,
                                       fill=GREEN, outline="", tags="row")
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

            c.create_text(w - 25, y0+rh//2, text=pl_disp, font=("Helvetica Neue", 9), fill=TEXT3, anchor="e", tags="row")
            c.create_line(12, y1-1, w-12, y1-1, fill=BORDER, tags="row")

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
            self.ctx_menu.post(event.x_root, event.y_root)

    def get_sel_paths(self):
        paths = []
        for idx in self._track_sel_idx:
            if idx < len(self._track_items):
                paths.append(self._track_items[idx][0])
        return paths

    def _get_current_playlist_paths(self):
        paths = []
        if self.controller._selected_playlist and self.controller._selected_playlist in self.controller._library:
            paths = self.controller._library[self.controller._selected_playlist]
        else:
            for tracks in self.controller._library.values():
                paths.extend(tracks)
        return paths

    def update_header_art(self, text, meta_text):
        self.playlist_title_lbl.config(text=text)
        self.playlist_meta_lbl.config(text=meta_text)
        self._current_art.update_art(text, self._get_current_playlist_paths())
        self._draw_lib_header()
        # Reset in-playlist search when switching playlists
        self._pl_search_entry.delete(0, "end")
        self._update_placeholder()
        self._pl_search_entry.config(fg=TEXT) # Use normal text color
