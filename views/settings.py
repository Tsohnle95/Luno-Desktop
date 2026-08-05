import tkinter as tk
from theme import BG, BG2, BG3, BG4, TEXT, TEXT2, TEXT3, GREEN, BORDER
from utils import draw_gradient_rect, bind_scroll_events, scroll_amount

class SettingsView(tk.Frame):
    def __init__(self, parent, controller):
        super().__init__(parent, bg=BG)
        self.controller = controller
        
        # Track pill widgets for updating styles
        self._pill_groups = {}
        
        self._build_ui()
        
    def _build_ui(self):
        self.grid_rowconfigure(0, weight=1)
        self.grid_columnconfigure(0, weight=1)
        
        # Main Canvas + Scrollbar
        self.canvas = tk.Canvas(self, bg=BG, highlightthickness=0, bd=0)
        from widgets import ModernScrollbar
        self.scrollbar = ModernScrollbar(self, command=self.canvas.yview)
        self.canvas.config(yscrollcommand=self.scrollbar.set)
        
        self.canvas.grid(row=0, column=0, sticky="nsew")
        self.scrollbar.grid(row=0, column=1, sticky="ns")
        
        sf = tk.Frame(self.canvas, bg=BG)
        self.canvas.create_window(0, 0, window=sf, anchor="nw", tags="inner")
        
        def _on_cfg(e):
            self.canvas.itemconfig("inner", width=e.width)
            self.canvas.config(scrollregion=self.canvas.bbox("all"))
        self.canvas.bind("<Configure>", _on_cfg)
        
        # Global Scroll Isolation
        def _on_wheel(e):
            w = e.widget
            while w:
                if w == self:
                    amount = scroll_amount(e, wheel_units=2)
                    if amount:
                        self.canvas.yview_scroll(amount, "units")
                    return
                try: w = w.master
                except: break
        bind_scroll_events(self.canvas, _on_wheel, add="+", bind_all=True)
        
        # Header with Spotify-style flat gradient
        hdr = tk.Canvas(sf, height=180, bg=BG, highlightthickness=0)
        hdr.pack(fill="x")
        def _draw_settings_hdr(e, c=hdr):
            c.delete("grad")
            draw_gradient_rect(c, 0, 0, e.width, 180, "#222222", BG)
            c.tag_raise("title")
        hdr.bind("<Configure>", _draw_settings_hdr)
        hdr.create_text(32, 180-48, text="Settings", font=("Helvetica Neue", 48, "bold"),
                        fill=TEXT, anchor="sw", tags="title")

        content = tk.Frame(sf, bg=BG)
        content.pack(fill="both", expand=True, padx=32, pady=20)

        # ─── DISCOVERY SECTION (Last.fm) ──────────────────────────────────
        self._add_section_header(content, "Discovery & Personalization")
        
        disco_box = tk.Frame(content, bg=BG)
        disco_box.pack(fill="x", pady=(0, 30))

        # Instructions Mini-Card
        inst_f = tk.Frame(disco_box, bg=BG2, padx=20, pady=20, highlightbackground=BORDER, highlightthickness=1)
        inst_f.pack(fill="x", pady=(0, 15))
        
        tk.Label(inst_f, text="Connect to Last.fm", font=("Helvetica Neue", 12, "bold"), 
                 fg=TEXT, bg=BG2).pack(anchor="w")
        
        instr = (
            "1. Sign up for a free account at Last.fm\n"
            "2. Create an API account at last.fm/api/account/create\n"
            "3. Copy your 'API Key' and paste it below."
        )
        tk.Label(inst_f, text=instr, font=("Helvetica Neue", 10), 
                 fg=TEXT3, bg=BG2, justify="left").pack(anchor="w", pady=(8, 0))

        row_key = tk.Frame(disco_box, bg=BG)
        row_key.pack(fill="x", pady=10)
        tk.Label(row_key, text="API Key", font=("Helvetica Neue", 11), fg=TEXT2, bg=BG).pack(side="left")
        
        k_f = tk.Frame(row_key, bg=BG3, highlightbackground=BG4, highlightthickness=1)
        k_f.pack(side="right", fill="x", expand=True, padx=(20, 0))
        key_entry = tk.Entry(k_f, textvariable=self.controller.lastfm_key_var, show="*",
                             bg=BG3, fg=TEXT, insertbackground=TEXT, relief="flat", 
                             font=("Menlo", 11), bd=8,
                             highlightthickness=0, highlightbackground=BG3, highlightcolor=BG3)
        key_entry.pack(fill="x")
        
        row_auto = tk.Frame(disco_box, bg=BG)
        row_auto.pack(fill="x", pady=10)
        tk.Label(row_auto, text="Automatic Recommendations", font=("Helvetica Neue", 11), fg=TEXT2, bg=BG).pack(side="left")
        
        auto_cb = tk.Checkbutton(row_auto, variable=self.controller.auto_discovery_var,
                                 bg=BG, activebackground=BG, selectcolor=BG2, relief="flat",
                                 borderwidth=0, highlightthickness=0)
        auto_cb.pack(side="right")

        # ─── STORAGE SECTION ───────────────────────────────────────────────
        self._add_section_header(content, "Storage & Library")
        
        lib_box = tk.Frame(content, bg=BG)
        lib_box.pack(fill="x", pady=(0, 30))
        
        tk.Label(lib_box, text="Master Music Folder", font=("Helvetica Neue", 11), 
                 fg=TEXT2, bg=BG).pack(anchor="w")
        
        path_f = tk.Frame(lib_box, bg=BG, pady=10)
        path_f.pack(fill="x")
        
        self.master_path_lbl = tk.Label(path_f, text=self.controller.MUSIC_ROOT,
                                        font=("Menlo", 10), fg=TEXT2, bg=BG2,
                                        padx=12, pady=10, anchor="w", highlightbackground=BORDER, highlightthickness=1)
        self.master_path_lbl.pack(side="left", fill="x", expand=True)
        
        ch_btn = tk.Label(path_f, text="CHANGE", font=("Helvetica Neue", 10, "bold"),
                          fg=BG, bg=TEXT2, padx=20, pady=10, cursor="hand2")
        ch_btn.pack(side="left", padx=(12, 0))
        ch_btn.bind("<Button-1>", lambda e: self.controller._change_master_library())

        transfer_f = tk.Frame(lib_box, bg=BG, pady=10)
        transfer_f.pack(fill="x")
        tk.Label(
            transfer_f,
            text="Move playlists or library metadata between Luno installs",
            font=("Helvetica Neue", 10),
            fg=TEXT3,
            bg=BG,
        ).pack(side="left")

        import_btn = tk.Label(
            transfer_f,
            text="IMPORT JSON",
            font=("Helvetica Neue", 10, "bold"),
            fg=BG,
            bg=TEXT2,
            padx=16,
            pady=8,
            cursor="hand2",
        )
        import_btn.pack(side="right", padx=(8, 0))
        import_btn.bind("<Button-1>", lambda e: self.controller._import_json())

        export_btn = tk.Label(
            transfer_f,
            text="EXPORT LIBRARY",
            font=("Helvetica Neue", 10, "bold"),
            fg=BG,
            bg=GREEN,
            padx=16,
            pady=8,
            cursor="hand2",
        )
        export_btn.pack(side="right")
        export_btn.bind("<Button-1>", lambda e: self.controller._export_full_library_json())
        
        art_f = tk.Frame(lib_box, bg=BG, pady=10)
        art_f.pack(fill="x")
        tk.Label(art_f, text="Fetch missing album art for entire library", font=("Helvetica Neue", 10), fg=TEXT3, bg=BG).pack(side="left")
        
        self.art_btn = tk.Label(art_f, text="FETCH ARTWORK", font=("Helvetica Neue", 10, "bold"),
                                fg=BG, bg=GREEN, padx=20, pady=10, cursor="hand2")
        self.art_btn.pack(side="right")
        self.art_btn.bind("<Button-1>", lambda e: self._on_fetch_artwork())
        
        self.art_info_lbl = tk.Label(lib_box, text="", font=("Helvetica Neue", 10), fg=TEXT3, bg=BG)
        self.art_info_lbl.pack(anchor="w", padx=12)
        
        # ─── AUDIO SECTION ────────────────────────────────────────────────
        self._add_section_header(content, "Playback Settings")
        
        aud_box = tk.Frame(content, bg=BG)
        aud_box.pack(fill="x", pady=(0, 30))

        # Crossfade Pills
        self._add_pill_selector(aud_box, "Crossfade Duration", 
                                ["Off", "2s", "5s", "10s", "15s"],
                                self.controller._cf_var,
                                self.controller._set_crossfade)

        # Sleep Timer Pills
        self._add_pill_selector(aud_box, "Sleep Timer", 
                                ["Off", "5 mins", "15 mins", "30 mins", "60 mins"],
                                self.controller._sleep_var,
                                self.controller._set_sleep_timer)

        keys_row = tk.Frame(aud_box, bg=BG)
        keys_row.pack(fill="x", pady=(20, 0))
        tk.Label(
            keys_row, text="Global Media Keys (macOS Remote Commands)",
            font=("Helvetica Neue", 11), fg=TEXT2, bg=BG
        ).pack(side="left")
        keys_cb = tk.Checkbutton(
            keys_row, variable=self.controller.global_keys_var,
            bg=BG, activebackground=BG, selectcolor=BG2, relief="flat",
            borderwidth=0, highlightthickness=0
        )
        keys_cb.pack(side="right")
        
        # ─── MAINTENANCE & DEBUG ──────────────────────────────────────────
        self._add_section_header(content, "Troubleshooting")
        
        dbg_box = tk.Frame(content, bg=BG)
        dbg_box.pack(fill="x", pady=(0, 30))
        
        tk.Label(dbg_box, text="View internal application logs for technical support.", 
                 font=("Helvetica Neue", 10), fg=TEXT3, bg=BG).pack(side="left")
        
        err_btn = tk.Label(dbg_box, text="OPEN LOGS", font=("Helvetica Neue", 10, "bold"),
                           fg=BG, bg=TEXT3, padx=20, pady=10, cursor="hand2")
        err_btn.pack(side="right")
        err_btn.bind("<Button-1>", lambda e: self.controller._show_tab("error_log"))

        # Padding
        tk.Frame(sf, bg=BG, height=100).pack()

    def _add_section_header(self, parent, text):
        f = tk.Frame(parent, bg=BG)
        f.pack(fill="x", pady=(10, 15))
        tk.Label(f, text=text, font=("Helvetica Neue", 16, "bold"), fg=TEXT, bg=BG).pack(side="left")
        tk.Frame(parent, bg=BG4, height=1).pack(fill="x", pady=(0, 20))

    def _add_pill_selector(self, parent, label_text, options, tk_var, callback):
        row = tk.Frame(parent, bg=BG)
        row.pack(fill="x", pady=12)
        
        tk.Label(row, text=label_text, font=("Helvetica Neue", 11), fg=TEXT2, bg=BG).pack(side="left")
        
        pills_f = tk.Frame(row, bg=BG)
        pills_f.pack(side="right")
        
        group_id = label_text
        self._pill_groups[group_id] = []
        
        for opt in options:
            is_active = (tk_var.get() == opt)
            bg_col = GREEN if is_active else BG3
            fg_col = BG if is_active else TEXT3
            
            p = tk.Label(pills_f, text=opt.upper(), font=("Helvetica Neue", 8, "bold"),
                         fg=fg_col, bg=bg_col, padx=12, pady=6, cursor="hand2")
            p.pack(side="left", padx=4)
            
            def _click(e, v=opt, g=group_id):
                tk_var.set(v)
                self._update_pills(g, v)
                callback()
                
            p.bind("<Button-1>", _click)
            self._pill_groups[group_id].append((opt, p))

    def _update_pills(self, group_id, active_val):
        for val, widget in self._pill_groups[group_id]:
            is_active = (val == active_val)
            bg_col = GREEN if is_active else BG3
            fg_col = BG if is_active else TEXT3
            widget.config(bg=bg_col, fg=fg_col)

    def _on_fetch_artwork(self):
        if self.art_btn.cget("text") == "FETCHING...":
            return
            
        self.art_btn.config(text="FETCHING...", bg=TEXT3)
        self.art_info_lbl.config(text="Scanning library...")
        self.controller._fetch_missing_artwork(
            on_progress=self._on_fetch_progress,
            on_complete=self._on_fetch_artwork_done
        )

    def _on_fetch_progress(self, current, total, text):
        self.art_info_lbl.config(text=f"Processing {current}/{total} — {text}")
        
    def _on_fetch_artwork_done(self):
        self.art_btn.config(text="DONE!", bg=BG3, fg=TEXT3)
        self.art_info_lbl.config(text="Artwork fetch complete. Check Error Log for details.")
        self.after(3000, lambda: self.art_btn.config(text="FETCH ARTWORK", bg=GREEN, fg=BG))
