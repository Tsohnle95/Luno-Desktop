import tkinter as tk
from tkinter import ttk, filedialog, messagebox
import threading
import os, re
from theme import BG, BG2, BG3, BG4, BORDER, GREEN, GREEN_HOVER, TEXT, TEXT2, TEXT3, SUCCESS, WARNING, RED, SC_COLOR, BLUE, SIDEBAR
from utils import draw_gradient_rect, split_track_name, format_time
from engine import probe_soundcloud, parse_exportify_csv
from widgets import ModernScrollbar
import yt_dlp
from downloader import Downloader

class DownloadView(tk.Frame):
    def __init__(self, parent, controller):
        super().__init__(parent, bg=BG)
        self.controller = controller

        # Internal State
        self.search_results = []
        self.sync_tracks = []
        self.dl_playlists = []
        self.is_dl = False
        self.sync_running = False
        self._last_yt_title = None
        self._active_tab = "search"
        self._preview_tracks = []  # Tracks from YT/CSV preview
        self._search_limit = 10

        self._build_ui()

    def _build_ui(self):
        self.grid_rowconfigure(0, weight=1)
        self.grid_columnconfigure(0, weight=1)

        # ── SCROLLABLE CONTAINER ─────────────────────────────────────────────
        self._scroll_canvas = tk.Canvas(self, bg=BG, highlightthickness=0, bd=0)
        self._scroll_sb = ModernScrollbar(self, command=self._scroll_canvas.yview)
        self._scroll_canvas.config(yscrollcommand=self._scroll_sb.set)

        self._scroll_canvas.grid(row=0, column=0, sticky="nsew")
        self._scroll_sb.grid(row=0, column=1, sticky="ns")

        self._inner = tk.Frame(self._scroll_canvas, bg=BG)
        self._inner_window = self._scroll_canvas.create_window(0, 0, window=self._inner,
                                                                anchor="nw", tags="inner")

        def _on_scroll_cfg(e):
            self._scroll_canvas.itemconfig("inner", width=e.width)
            self._scroll_canvas.config(scrollregion=self._scroll_canvas.bbox("all"))
        self._scroll_canvas.bind("<Configure>", _on_scroll_cfg)

        def _on_wheel(e):
            bbox = self._scroll_canvas.bbox("all")
            if not bbox: return
            
            c_height = self._scroll_canvas.winfo_height()
            if bbox[3] - bbox[1] <= c_height: 
                return # Content fits; disable scrolling
                
            y_v = self._scroll_canvas.yview()
            if e.delta > 0 and y_v[0] <= 0: return # Clamped at top
            if e.delta < 0 and y_v[1] >= 1.0: return # Clamped at bottom
            
            if e.delta > 0:
                self._scroll_canvas.yview_scroll(-2, "units")
            else:
                self._scroll_canvas.yview_scroll(2, "units")
            
        self._scroll_canvas.bind("<MouseWheel>", _on_wheel)
        self._inner.bind("<MouseWheel>", _on_wheel)

        # Propagate wheel events from child widgets
        def _bind_wheel_recursive(widget):
            if isinstance(widget, ttk.Treeview) or isinstance(widget, tk.Text): return
            widget.bind("<MouseWheel>", _on_wheel)
            for child in widget.winfo_children():
                _bind_wheel_recursive(child)
        self._inner.bind("<Map>", lambda e: self.after(100, lambda: _bind_wheel_recursive(self._inner)))

        # Now build all content inside self._inner
        self._inner.grid_columnconfigure(0, weight=1)

        row_idx = 0
        # ── ROW 0: HEADER ────────────────────────────────────────────────────
        hdr = tk.Canvas(self._inner, height=90, bg=BG, highlightthickness=0)
        hdr.grid(row=row_idx, column=0, sticky="ew")
        def _draw_dl_hdr(e, c=hdr):
            c.delete("grad")
            draw_gradient_rect(c, 0, 0, e.width, 90, "#1a2a2a", BG)
            c.tag_raise("title")
        hdr.bind("<Configure>", _draw_dl_hdr)
        hdr.create_text(28, 90 - 12, text="Downloader",
                        font=("Helvetica Neue", 28, "bold"), fill=TEXT, anchor="sw",
                        tags="title")

        # ── ROW 1: TAB BAR ───────────────────────────────────────────────────
        row_idx = 1
        tab_bar_f = tk.Frame(self._inner, bg=BG)
        tab_bar_f.grid(row=row_idx, column=0, sticky="ew", padx=28, pady=(12, 12))

        self._tab_btns = {}
        for tab_id, label in [("search", "⌕  Search"),
                               ("playlist", "📋  YT / CSV"),
                               ("sync", "🔗  Sync"),
                               ("dupes", "⚠ Duplicates"),
                               ("allowed", "✓ Allowed Dups")]:
            btn = tk.Label(tab_bar_f, text=label, font=("Helvetica Neue", 11, "bold"),
                           fg=TEXT3, bg=BG, padx=16, pady=8, cursor="hand2")
            btn.pack(side="left")
            btn.bind("<Button-1>", lambda e, t=tab_id: self._switch_tab(t))
            btn.bind("<Enter>", lambda e, b=btn: b.config(fg=TEXT) if b.cget("bg") == BG else None)
            btn.bind("<Leave>", lambda e, b=btn, t=tab_id: b.config(fg=TEXT3) if t != self._active_tab else None)
            self._tab_btns[tab_id] = btn

        # Thin progress bar (hidden by default, shown only during downloads)
        row_idx = 2
        self._prog_frame = tk.Frame(self._inner, bg=BG, height=3)
        self._prog_frame.grid(row=row_idx, column=0, sticky="ew", padx=28, pady=(16, 16))
        self._prog_frame.grid_remove()  # Hidden by default

        self._prog_bar = ttk.Progressbar(self._prog_frame, variable=self.controller.prog_var,
                                          maximum=100, style="Green.Horizontal.TProgressbar")
        self._prog_bar.pack(fill="x")

        # ── ROW 3: TAB CONTENT (Source inputs) ───────────────────────────────
        row_idx = 3
        self._tab_content = tk.Frame(self._inner, bg=BG)
        self._tab_content.grid(row=row_idx, column=0, sticky="ew", padx=28, pady=(14, 0))
        self._tab_content.grid_columnconfigure(0, weight=1)

        self._tab_frames = {}
        self._build_search_tab()
        self._build_sync_tab()
        self._build_playlist_tab()
        self._build_dupes_tab()
        self._build_allowed_tab()

        # ── Divider ──────────────────────────────────────────────────────────
        row_idx = 4
        tk.Frame(self._inner, bg=BG4, height=1).grid(row=row_idx, column=0,
                                                       sticky="ew", padx=28, pady=(20, 8))

        # ── ROW 5: CONTROLS (destination + options) ──────────────────────────
        row_idx = 5
        self.ctrl_f = tk.Frame(self._inner, bg=BG)
        self.ctrl_f.grid(row=row_idx, column=0, sticky="ew", padx=28, pady=(14, 6))

        tk.Label(self.ctrl_f, text="SAVE TO", font=("Helvetica Neue", 9, "bold"),
                 fg=TEXT3, bg=BG).pack(side="left", padx=(0, 6))
        self.dest_combo_var = tk.StringVar(value="— choose playlist —")
        self.dest_combo = ttk.Combobox(self.ctrl_f, textvariable=self.dest_combo_var,
                                       font=("Helvetica Neue", 10), state="readonly", width=20)
        self.dest_combo.pack(side="left")
        self.dest_combo.bind("<<ComboboxSelected>>", self._on_dest_selected)

        # + New playlist button
        new_pl_btn = tk.Label(self.ctrl_f, text="+ New", font=("Helvetica Neue", 9, "bold"),
                              fg=TEXT2, bg=BG3, padx=10, pady=4, cursor="hand2")
        new_pl_btn.pack(side="left", padx=(8, 0))
        new_pl_btn.bind("<Button-1>", lambda e: self._create_playlist_and_refresh())
        new_pl_btn.bind("<Enter>", lambda e: new_pl_btn.config(fg=TEXT, bg=BG4))
        new_pl_btn.bind("<Leave>", lambda e: new_pl_btn.config(fg=TEXT2, bg=BG3))

        # Spacer
        tk.Frame(self.ctrl_f, bg=BG, width=14).pack(side="left")

        for var, lbl in [(self.controller.skip_var, "Skip Existing"),
                         (self.controller.sc_var, "SC First")]:
            tk.Checkbutton(self.ctrl_f, text=lbl, variable=var,
                           font=("Helvetica Neue", 9), fg=TEXT2, bg=BG,
                           selectcolor=BG3, activebackground=BG,
                           activeforeground=TEXT, highlightthickness=0,
                           bd=0, cursor="hand2").pack(side="left", padx=(0, 10))

        clear_btn = tk.Label(self.ctrl_f, text="Clear", font=("Helvetica Neue", 10),
                             fg=TEXT3, bg=BG, padx=6, cursor="hand2")
        clear_btn.pack(side="right")
        clear_btn.bind("<Button-1>", lambda e: self._clear_results())
        clear_btn.bind("<Enter>", lambda e: clear_btn.config(fg=TEXT))
        clear_btn.bind("<Leave>", lambda e: clear_btn.config(fg=TEXT3))

        # ── ROW 7: RESULTS TREE ──────────────────────────────────────────────
        row_idx = 7
        self.results_f = tk.Frame(self._inner, bg=BG, bd=0, highlightthickness=0)
        self.results_f.grid(row=row_idx, column=0, sticky="ew", padx=28, pady=(10, 0))
        self.results_f.grid_columnconfigure(0, weight=1)

        self.results_tree = ttk.Treeview(self.results_f, style="Results.Treeview",
                                         columns=("status", "title", "artist", "info"),
                                         show="headings", height=1,
                                         selectmode="extended")
        self.results_tree.heading("status", text="STATUS / SOURCE")
        self.results_tree.heading("title",  text="TITLE")
        self.results_tree.heading("artist", text="ARTIST")
        self.results_tree.heading("info",   text="INFO")
        self.results_tree.column("status", width=120, stretch=False)
        self.results_tree.column("title",  width=300, stretch=True)
        self.results_tree.column("artist", width=200, stretch=True)
        self.results_tree.column("info",   width=80,  stretch=False, anchor="e")

        tree_sb = ModernScrollbar(self.results_f, command=self.results_tree.yview, bg=BG, width=4)
        self.results_tree.config(yscrollcommand=tree_sb.set)
        tree_sb.grid(row=0, column=1, sticky="ns")
        self.results_tree.grid(row=0, column=0, sticky="ew")

        self.results_tree.tag_configure("new",  foreground=GREEN)
        self.results_tree.tag_configure("have", foreground=TEXT3)
        self.results_tree.tag_configure("yt",   foreground=TEXT2)
        self.results_tree.tag_configure("sc",   foreground=SC_COLOR)
        self.results_tree.bind("<Double-Button-1>", lambda e: self._on_tree_double_click())
        self.results_tree.bind("<Return>", lambda e: self._download_selected())
        self.results_tree.bind("<KP_Enter>", lambda e: self._download_selected())

        # ── ROW 6: ACTION BAR (buttons + inline download status) ─────────────
        row_idx = 6
        self.action_f = tk.Frame(self._inner, bg=BG)
        self.action_f.grid(row=row_idx, column=0, sticky="ew", padx=28, pady=(12, 8))
        self.action_f.grid_columnconfigure(1, weight=1)  # Status label gets stretch

        # Left side: buttons
        btn_f = tk.Frame(self.action_f, bg=BG)
        btn_f.grid(row=0, column=0, sticky="w")

        self.dl_sel_btn = tk.Label(btn_f, text="⬇ Download Selected",
                                   font=("Helvetica Neue", 10, "bold"),
                                   fg=SIDEBAR, bg=GREEN, padx=14, pady=7, cursor="hand2")
        self.dl_sel_btn.pack(side="left")
        self.dl_sel_btn.bind("<Button-1>", lambda e: self._download_selected())
        self.dl_sel_btn.bind("<Enter>", lambda e: self.dl_sel_btn.config(bg=GREEN_HOVER))
        self.dl_sel_btn.bind("<Leave>", lambda e: self.dl_sel_btn.config(bg=GREEN))

        self.dl_all_btn = tk.Label(btn_f, text="⬇ Download All New",
                                   font=("Helvetica Neue", 10, "bold"),
                                   fg=TEXT3, bg=BG4, padx=14, pady=7, cursor="hand2")
        self.dl_all_btn.pack(side="left", padx=(8, 0))
        self.dl_all_btn.bind("<Button-1>", lambda e: self._download_all())

        # Right side: inline download status
        self.dl_status_lbl = tk.Label(self.action_f, text="",
                                      font=("Helvetica Neue", 10, "bold"),
                                      fg=TEXT3, bg=BG, anchor="e")
        self.dl_status_lbl.grid(row=0, column=1, sticky="e", padx=(12, 0))

        # ── ROW 8: COLLAPSIBLE LOG (toggle dropdown) ────────────────────────
        row_idx = 8
        self.log_f = tk.Frame(self._inner, bg=BG2, highlightbackground=BORDER,
                              highlightthickness=1)
        self.log_f.grid(row=row_idx, column=0, sticky="ew", padx=28, pady=(4, 16))

        self._log_expanded = False

        log_hdr = tk.Frame(self.log_f, bg=BG2, cursor="hand2")
        log_hdr.pack(fill="x")
        self._log_toggle_lbl = tk.Label(log_hdr, text="▶  Download Log",
                                         font=("Helvetica Neue", 10, "bold"),
                                         fg=TEXT3, bg=BG2, pady=8, padx=12,
                                         cursor="hand2", anchor="w")
        self._log_toggle_lbl.pack(side="left", fill="x", expand=True)
        for w in [log_hdr, self._log_toggle_lbl]:
            w.bind("<Button-1>", lambda e: self._toggle_log())
            w.bind("<Enter>", lambda e: self._log_toggle_lbl.config(fg=TEXT))
            w.bind("<Leave>", lambda e: self._log_toggle_lbl.config(fg=TEXT3))

        # Log body (hidden by default)
        self._log_body = tk.Frame(self.log_f, bg=BG2)

        self.log_text = tk.Text(self._log_body, bg=BG2, fg=TEXT2, font=("Helvetica Neue",10), 
                                bd=0, highlightthickness=0, wrap="word", height=6)
        log_sb = ModernScrollbar(self._log_body, command=self.log_text.yview, bg=BG2, width=4)
        self.log_text.config(yscrollcommand=log_sb.set)
        log_sb.pack(side="right", fill="y")
        self.log_text.pack(side="left", fill="both", expand=True, padx=(8, 0), pady=(0, 8))
        self.log_text.config(state="disabled")
        
        self.log_text.tag_config("normal", foreground=TEXT2)
        self.log_text.tag_config("success", foreground=GREEN)
        self.log_text.tag_config("error", foreground=RED)
        self.log_text.tag_config("warning", foreground=WARNING)

        # Bottom padding for scroll comfort
        tk.Frame(self._inner, bg=BG, height=40).grid(row=9, column=0, sticky="ew")

        # Bind inner resize to update scrollregion
        self._inner.bind("<Configure>",
            lambda e: self._scroll_canvas.config(scrollregion=self._scroll_canvas.bbox("all")))
            
        self._switch_tab("search")

    # ═════════════════════════════════════════════════════════════════════════
    # TAB BUILDERS
    # ═════════════════════════════════════════════════════════════════════════

    def _build_search_tab(self):
        f = tk.Frame(self._tab_content, bg=BG)
        self._tab_frames["search"] = f

        tk.Label(f, text="Search YouTube & SoundCloud for individual songs.",
                 font=("Helvetica Neue", 10), fg=TEXT3, bg=BG).pack(anchor="w", pady=(0, 10))

        bar = tk.Frame(f, bg=BG3, highlightbackground=BG4, highlightthickness=1)
        bar.pack(fill="x")
        tk.Label(bar, text="⌕", font=("Helvetica Neue", 13), fg=TEXT3, bg=BG3,
                 padx=10).pack(side="left")
        self.search_entry = tk.Entry(bar, textvariable=self.controller.search_var,
                                     bg=BG3, fg=TEXT, insertbackground=TEXT,
                                     relief="flat", font=("Helvetica Neue", 11), bd=8,
                                     highlightthickness=0, highlightbackground=BG3, highlightcolor=BG3)
        self.search_entry.pack(side="left", fill="x", expand=True)
        self.search_entry.bind("<Return>", lambda e: self.do_search())
        self.search_entry.bind("<KP_Enter>", lambda e: self.do_search())

        s_btn = tk.Label(bar, text="SEARCH", font=("Helvetica Neue", 9, "bold"),
                         fg=SIDEBAR, bg=GREEN, padx=18, pady=8, cursor="hand2")
        s_btn.pack(side="right")
        s_btn.bind("<Button-1>", lambda e: self.do_search())
        s_btn.bind("<Enter>", lambda e: s_btn.config(bg=GREEN_HOVER))
        s_btn.bind("<Leave>", lambda e: s_btn.config(bg=GREEN))

        self.load_more_btn = tk.Label(bar, text="LOAD MORE", font=("Helvetica Neue", 8, "bold"),
                                     fg=TEXT2, bg=BG3, padx=12, pady=8, cursor="hand2")
        self.load_more_btn.pack(side="right", padx=(0, 2))
        self.load_more_btn.bind("<Button-1>", lambda e: self._load_more_search())
        self.load_more_btn.bind("<Enter>", lambda e: self.load_more_btn.config(fg=TEXT, bg=BG4))
        self.load_more_btn.bind("<Leave>", lambda e: self.load_more_btn.config(fg=TEXT2, bg=BG3))
        self.load_more_btn.pack_forget() # Hidden until first search

    def _build_sync_tab(self):
        f = tk.Frame(self._tab_content, bg=BG)
        self._tab_frames["sync"] = f

        tk.Label(f, text="Compare a YouTube playlist against your local library. New tracks show as ⬇ New.",
                 font=("Helvetica Neue", 10), fg=TEXT3, bg=BG, wraplength=700).pack(anchor="w", pady=(0, 10))

        bar = tk.Frame(f, bg=BG3, highlightbackground=BG4, highlightthickness=1)
        bar.pack(fill="x")
        tk.Label(bar, text="🔗", font=("Helvetica Neue", 11), fg=TEXT3, bg=BG3,
                 padx=10).pack(side="left")
        self.sync_entry = tk.Entry(bar, textvariable=self.controller.yt_url_var,
                                   bg=BG3, fg=TEXT, insertbackground=TEXT,
                                   relief="flat", font=("Helvetica Neue", 11), bd=8,
                                   highlightthickness=0, highlightbackground=BG3, highlightcolor=BG3)
        self.sync_entry.pack(side="left", fill="x", expand=True)
        self.sync_entry.bind("<Return>", lambda e: self._sync_check())
        self.sync_entry.bind("<KP_Enter>", lambda e: self._sync_check())

        c_btn = tk.Label(bar, text="CHECK SYNC", font=("Helvetica Neue", 9, "bold"),
                         fg=SIDEBAR, bg=GREEN, padx=14, pady=8, cursor="hand2")
        c_btn.pack(side="right")
        c_btn.bind("<Button-1>", lambda e: self._sync_check())
        c_btn.bind("<Enter>", lambda e: c_btn.config(bg=GREEN_HOVER))
        c_btn.bind("<Leave>", lambda e: c_btn.config(bg=GREEN))

    def _build_playlist_tab(self):
        f = tk.Frame(self._tab_content, bg=BG)
        self._tab_frames["playlist"] = f

        tk.Label(f, text="Preview & download entire YouTube playlists or batch-import from Spotify CSVs.",
                 font=("Helvetica Neue", 10), fg=TEXT3, bg=BG, wraplength=700).pack(anchor="w", pady=(0, 10))

        # YT URL
        yt_bar = tk.Frame(f, bg=BG3, highlightbackground=BG4, highlightthickness=1)
        yt_bar.pack(fill="x", pady=(0, 8))
        tk.Label(yt_bar, text="▶", font=("Helvetica Neue", 11), fg=TEXT3, bg=BG3,
                 padx=10).pack(side="left")
        self.pl_yt_entry = tk.Entry(yt_bar, textvariable=self.controller.yt_url_var,
                                    bg=BG3, fg=TEXT, insertbackground=TEXT,
                                    relief="flat", font=("Helvetica Neue", 11), bd=8,
                                    highlightthickness=0, highlightbackground=BG3, highlightcolor=BG3)
        self.pl_yt_entry.pack(side="left", fill="x", expand=True)
        self.pl_yt_entry.bind("<Return>", lambda e: self._preview_yt_playlist())
        self.pl_yt_entry.bind("<KP_Enter>", lambda e: self._preview_yt_playlist())

        preview_btn = tk.Label(yt_bar, text="PREVIEW", font=("Helvetica Neue", 9, "bold"),
                               fg=SIDEBAR, bg=GREEN, padx=14, pady=8, cursor="hand2")
        preview_btn.pack(side="right")
        preview_btn.bind("<Button-1>", lambda e: self._preview_yt_playlist())
        preview_btn.bind("<Enter>", lambda e: preview_btn.config(bg=GREEN_HOVER))
        preview_btn.bind("<Leave>", lambda e: preview_btn.config(bg=GREEN))

        # CSV row
        csv_row = tk.Frame(f, bg=BG)
        csv_row.pack(fill="x")

        csv_btn = tk.Label(csv_row, text="+ Load Spotify CSV",
                           font=("Helvetica Neue", 10, "bold"),
                           fg=TEXT2, bg=BG3, padx=12, pady=6, cursor="hand2")
        csv_btn.pack(side="left")
        csv_btn.bind("<Button-1>", lambda e: self._add_csvs())
        csv_btn.bind("<Enter>", lambda e: csv_btn.config(fg=TEXT))
        csv_btn.bind("<Leave>", lambda e: csv_btn.config(fg=TEXT2))
        self.csv_count_lbl = tk.Label(csv_row, text="",
                                      font=("Helvetica Neue", 9), fg=TEXT3, bg=BG)
        self.csv_count_lbl.pack(side="left", padx=10)

        csv_dl_btn = tk.Label(csv_row, text="Download CSVs", font=("Helvetica Neue", 10, "bold"),
                              fg=TEXT3, bg=BG4, padx=12, pady=6, cursor="hand2")
        csv_dl_btn.pack(side="right")
        csv_dl_btn.bind("<Button-1>", lambda e: self._start_csv_dl())

    def _build_dupes_tab(self):
        f = tk.Frame(self._tab_content, bg=BG)
        self._tab_frames["dupes"] = f
        
        lbl = tk.Label(f, text="These tracks appear to be duplicates based on name/artist. Check them before deleting.", 
                       font=("Helvetica Neue", 10), fg=TEXT3, bg=BG, justify="left")
        lbl.pack(fill="x", pady=(0,10))
        
        self.dupe_status_lbl = tk.Label(f, text="Ready to scan.", font=("Helvetica Neue", 10, "bold"), fg=TEXT2, bg=BG)
        self.dupe_status_lbl.pack(anchor="w", pady=(0,10))
        
        # Action row
        act = tk.Frame(f, bg=BG)
        act.pack(fill="x", pady=(0,10))
        
        del_btn = tk.Label(act, text="🗑  Delete Selected", font=("Helvetica Neue", 10, "bold"), 
                           fg=BG, bg=RED, padx=14, pady=6, cursor="hand2")
        del_btn.pack(side="left")
        del_btn.bind("<Button-1>", lambda e: self._delete_selected_dupes())
        
        allow_btn = tk.Label(act, text="✓  Allow Selection", font=("Helvetica Neue", 10, "bold"), 
                            fg=BG, bg=GREEN, padx=14, pady=6, cursor="hand2")
        allow_btn.pack(side="left", padx=(10,0))
        allow_btn.bind("<Button-1>", lambda e: self._allow_selected_dupes())
        
        refresh_btn = tk.Label(act, text="↻  Refresh", font=("Helvetica Neue", 10, "bold"), 
                              fg=TEXT2, bg=BG3, padx=14, pady=6, cursor="hand2")
        refresh_btn.pack(side="right")
        refresh_btn.bind("<Button-1>", lambda e: self._populate_dupes())
        tree_f = tk.Frame(f, bg=BG2, highlightbackground=BORDER, highlightthickness=1)
        tree_f.pack(fill="both", expand=True)
        
        cols = ("playlist", "filename", "raw_path")
        self.dupes_tree = ttk.Treeview(tree_f, columns=cols, show="tree headings", 
                                       style="Modern.Treeview", height=12)
        self.dupes_tree.heading("#0", text="Track / Group", anchor="w")
        self.dupes_tree.heading("playlist", text="Playlist", anchor="w")
        self.dupes_tree.heading("filename", text="Filename", anchor="w")
        self.dupes_tree.column("#0", width=350)
        self.dupes_tree.column("playlist", width=120)
        self.dupes_tree.column("filename", width=300)
        self.dupes_tree.column("raw_path", width=0, stretch=False) # Hidden
        
        self.dupes_tree.bind("<Double-1>", self._on_dupe_dbl_click)

        sb = ModernScrollbar(tree_f, command=self.dupes_tree.yview, bg=BG2, width=4)
        hsb = ModernScrollbar(tree_f, orient="horizontal", command=self.dupes_tree.xview, bg=BG2, width=4)
        self.dupes_tree.configure(yscrollcommand=sb.set, xscrollcommand=hsb.set)
        
        tree_f.grid_rowconfigure(0, weight=1)
        tree_f.grid_columnconfigure(0, weight=1)
        sb.grid(row=0, column=1, sticky="ns")
        hsb.grid(row=1, column=0, sticky="ew")
        self.dupes_tree.grid(row=0, column=0, sticky="nsew")

    def _populate_dupes(self):
        self.dupe_status_lbl.config(text="🔍 Scanning for duplicates...", fg=TEXT3)
        self.dupes_tree.delete(*self.dupes_tree.get_children())
        
        def run_scan():
            from engine import find_duplicates
            groups = find_duplicates(self.controller._library, allowed_keys=self.controller.engine.allowed_dupe_keys)
            self.after(0, lambda: self._start_progressive_population(groups))
            
        threading.Thread(target=run_scan, daemon=True).start()

    def _start_progressive_population(self, groups):
        total_dupes = sum(len(g) - 1 for g in groups) if groups else 0
        if not groups:
            self.dupe_status_lbl.config(text="No duplicates found. ✓", fg=SUCCESS)
            return
            
        self.dupe_status_lbl.config(text=f"{len(groups)} groups  •  {total_dupes} duplicates", fg=WARNING)
        
        # Start the batching process
        self._progressive_insert(groups, 0)

    def _progressive_insert(self, groups, start_idx):
        if self._active_tab != "dupes": return # Cancel if user switched away
        
        batch_size = 20
        end_idx = min(start_idx + batch_size, len(groups))
        
        def truncate_path(p, limit=50):
            if len(p) <= limit: return p
            return p[:20] + "..." + p[-27:]

        for i in range(start_idx, end_idx):
            group = groups[i]
            orig = group[0]
            display = f"{orig['artist']} — {orig['title']}" if orig['artist'] else orig['title']
            node = self.dupes_tree.insert("", "end", text=f"  {display}  ({len(group)} copies)", open=True, iid=f"g{i}")
            for j, entry in enumerate(group):
                fn = os.path.basename(entry["path"])
                self.dupes_tree.insert(node, "end", text="  ⚠ Duplicate" if j>0 else "  ✓ Track",
                                       values=(entry["pl"], fn, entry["path"]), iid=f"g{i}_e{j}")
        
        if end_idx < len(groups):
            self.after(10, lambda: self._progressive_insert(groups, end_idx))
        else:
            self._scroll_canvas.config(scrollregion=self._scroll_canvas.bbox("all"))

    def _finalize_dupes(self, groups):
        """Deprecated by progressive_insert, kept for signature safety."""
        self._start_progressive_population(groups)

    def _delete_selected_dupes(self):
        sel = self.dupes_tree.selection()
        if not sel:
            self.dupe_status_lbl.config(text="Select an entry to delete.", fg=WARNING)
            return
        
        paths_to_delete = []
        for item_id in sel:
            vals = self.dupes_tree.item(item_id, "values")
            if not vals or len(vals) < 3:
                continue
            # use the raw_path (index 2) for deletion logic
            paths_to_delete.append(vals[2])
            
        if not paths_to_delete: return
        
        confirm = messagebox.askyesno("Delete", f"Permanently delete {len(paths_to_delete)} file(s)?\nThey will be deleted from disk.")
        if not confirm:
            return
            
        deleted = 0
        for p in paths_to_delete:
            try:
                if os.path.exists(p):
                    os.remove(p)
                    deleted += 1
            except Exception as e:
                self.controller._log_error("DupeDel", f"Failed to delete {p}: {e}")
                
        self.controller._refresh_library()
        self.after(500, self._populate_dupes)
        self.dupe_status_lbl.config(text=f"✓ Deleted {deleted} file(s)", fg=SUCCESS)
        
    def _on_dupe_dbl_click(self, event=None):
        sel = self.dupes_tree.selection()
        if not sel: return
        item = sel[0]
        vals = self.dupes_tree.item(item, "values")
        
        path = None
        if vals and len(vals) >= 3:
            path = vals[2] # raw_path
        elif item.startswith("g"):
            # Group node: play the first child track for comparison
            children = self.dupes_tree.get_children(item)
            if children:
                c_vals = self.dupes_tree.item(children[0], "values")
                if c_vals and len(c_vals) >= 3:
                    path = c_vals[2]

        if path:
            self.controller._play_with_context(path)

    def _switch_tab(self, tab_id):
        self._active_tab = tab_id
        for tid, frame in self._tab_frames.items():
            frame.pack_forget()
        self._tab_frames[tab_id].pack(fill="x")
        
        # Toggle component visibility
        if tab_id == "dupes":
            self.ctrl_f.grid_remove()
            self.results_f.grid_remove()
            self.action_f.grid_remove()
            self._tab_frames["dupes"].pack(fill="both", expand=True)
            self._populate_dupes()
        elif tab_id == "allowed":
            self.ctrl_f.grid_remove()
            self.results_f.grid_remove()
            self.action_f.grid_remove()
            self._tab_frames["allowed"].pack(fill="both", expand=True)
            self._populate_allowed()
        else:
            self.ctrl_f.grid()
            self.results_f.grid()
            self.action_f.grid()
            if "dupes" in self._tab_frames:
                self._tab_frames["dupes"].pack_forget()
            if "allowed" in self._tab_frames:
                self._tab_frames["allowed"].pack_forget()
            
        # Update tab button styles
        for tid, btn in self._tab_btns.items():
            if tid == tab_id:
                btn.config(fg=TEXT, bg=BG3)
            else:
                btn.config(fg=TEXT3, bg=BG)
        
        # Update canvas boundaries when tabs change
        self.after(50, lambda: self._scroll_canvas.config(scrollregion=self._scroll_canvas.bbox("all")))

    def show_dupes_tab(self):
        self._switch_tab("dupes")

    def _allow_selected_dupes(self):
        sel = self.dupes_tree.selection()
        if not sel: return
        from engine import _normalize_for_dupe
        
        count = 0
        keys_to_add = set()

        for item in sel:
            # Check if this is a group node (starts with 'g' and has no underscore for index)
            # OR if it's an entry node (contains '_e')
            if "_e" in item:
                # Individual track entry
                vals = self.dupes_tree.item(item, "values")
                raw_path = vals[2] if vals and len(vals)>2 else None
                if raw_path:
                    title, artist = split_track_name(raw_path)
                    key = _normalize_for_dupe(f"{artist}{title}")
                    if key: keys_to_add.add(key)
            elif item.startswith("g"):
                # Group node: Add the first child's key (they all share the same key)
                children = self.dupes_tree.get_children(item)
                if children:
                    vals = self.dupes_tree.item(children[0], "values")
                    raw_path = vals[2] if vals and len(vals)>2 else None
                    if raw_path:
                        title, artist = split_track_name(raw_path)
                        key = _normalize_for_dupe(f"{artist}{title}")
                        if key: keys_to_add.add(key)

        for k in keys_to_add:
            if k not in self.controller.engine.allowed_dupe_keys:
                self.controller.engine.allowed_dupe_keys.add(k)
                count += 1
        
        if count:
            self.controller.engine.save_allowed_dupes()
            messagebox.showinfo("Allowed", f"Added {count} track sets to allowed list. They will be ignored in future scans.")
            self._populate_dupes()
        else:
            self.dupe_status_lbl.config(text="Selection already allowed or invalid.", fg=WARNING)

    def _populate_allowed(self):
        self.allowed_tree.delete(*self.allowed_tree.get_children())
        for key in sorted(self.controller.engine.allowed_dupe_keys):
            self.allowed_tree.insert("", "end", values=(key,))

    def _unallow_selected_dupes(self):
        sel = self.allowed_tree.selection()
        if not sel: return
        for item in sel:
            key = self.allowed_tree.item(item, "values")[0]
            if key in self.controller.engine.allowed_dupe_keys:
                self.controller.engine.allowed_dupe_keys.remove(key)
        self.controller.engine.save_allowed_dupes()
        self._populate_allowed()

    def _export_allowed_dupes(self):
        import json
        path = filedialog.asksaveasfilename(
            title="Export Allowed Duplicates Backup",
            defaultextension=".json",
            filetypes=[("JSON", "*.json"), ("All Files", "*.*")],
            initialfile="vibe_allowed_backup.json"
        )
        if not path: return
        try:
            with open(path, "w") as f:
                json.dump(list(self.controller.engine.allowed_dupe_keys), f, indent=4)
            messagebox.showinfo("Export Successful", f"Backup saved: {os.path.basename(path)}")
        except Exception as e:
            messagebox.showerror("Export Failed", str(e))

    def _import_allowed_dupes(self):
        import json
        path = filedialog.askopenfilename(
            title="Import Allowed Duplicates Backup",
            filetypes=[("JSON", "*.json"), ("All Files", "*.*")]
        )
        if not path: return
        try:
            with open(path, "r") as f:
                keys = json.load(f)
            if not isinstance(keys, list):
                raise ValueError("Invalid backup format")
            
            added = 0
            for k in keys:
                if k not in self.controller.engine.allowed_dupe_keys:
                    self.controller.engine.allowed_dupe_keys.add(k)
                    added += 1
            
            if added > 0:
                self.controller.engine.save_allowed_dupes()
                self._populate_allowed()
                messagebox.showinfo("Import Successful", f"Added {added} keys from backup.")
            else:
                messagebox.showinfo("Import Complete", "All keys in backup were already present.")
        except Exception as e:
            messagebox.showerror("Import Failed", str(e))

    # ═════════════════════════════════════════════════════════════════════════
    # CREATE PLAYLIST (next to SAVE TO)
    # ═════════════════════════════════════════════════════════════════════════

    def _create_playlist_and_refresh(self):
        """Create a new playlist folder, refresh library, and update combo."""
        self.controller._create_playlist_dialog(set_dest=True)
        # After creation, refresh the combo with updated options
        self.after(500, lambda: self._refresh_dest_combo())

    def _refresh_dest_combo(self):
        """Pull fresh options from controller and update the combo."""
        if hasattr(self.controller, '_dest_options'):
            self.update_dest_combo(self.controller._dest_options)

    # ═════════════════════════════════════════════════════════════════════════
    # PROGRESS BAR (conditional visibility)
    # ═════════════════════════════════════════════════════════════════════════

    def _show_progress(self):
        self.controller.prog_var.set(0)
        self._prog_frame.grid()

    def _hide_progress(self):
        self.after(1500, lambda: self._prog_frame.grid_remove())

    # ═════════════════════════════════════════════════════════════════════════
    # LOG
    # ═════════════════════════════════════════════════════════════════════════

    def _toggle_log(self):
        self._log_expanded = not self._log_expanded
        if self._log_expanded:
            self._log_body.pack(fill="both", expand=True)
            self._log_toggle_lbl.config(text="▼  Download Log")
        else:
            self._log_body.pack_forget()
            self._log_toggle_lbl.config(text="▶  Download Log")
        # Update scrollregion after log toggle
        self._inner.update_idletasks()
        self._scroll_canvas.config(scrollregion=self._scroll_canvas.bbox("all"))

    def _show_log(self):
        if not self._log_expanded:
            self._toggle_log()

    def _hide_log(self):
        if self._log_expanded:
            self._toggle_log()

    def _log(self, msg, type="normal"):
        self.log_text.config(state="normal")
        self.log_text.insert("end", "• " + msg + "\n", type)
        self.log_text.config(state="disabled")
        self.log_text.see("end")

    # ═════════════════════════════════════════════════════════════════════════
    # INLINE DOWNLOAD STATUS (right-aligned in action bar)
    # ═════════════════════════════════════════════════════════════════════════

    def _set_dl_status(self, text, color=WARNING):
        self.after(0, lambda: self.dl_status_lbl.config(text=text, fg=color))

    def _clear_dl_status(self):
        self.after(0, lambda: self.dl_status_lbl.config(text="", fg=TEXT3))

    # ═════════════════════════════════════════════════════════════════════════
    # SEARCH LOGIC
    # ═════════════════════════════════════════════════════════════════════════

    def do_search(self, reset_limit=True):
        q = self.controller.search_var.get().strip()
        if not q: return
        
        if reset_limit:
            self._search_limit = 10
            self._clear_results()
            self.load_more_btn.pack_forget()
        
        self._set_dl_status(f"Searching {self._search_limit} results…", WARNING)

        def run():
            results = []
            try:
                # YT Search limit
                with yt_dlp.YoutubeDL({"quiet": True, "extract_flat": True}) as ydl:
                    info = ydl.extract_info(f"ytsearch{self._search_limit}:{q}", download=False)
                for e in (info.get("entries") or []):
                    results.append({
                        "title": e.get("title", "?"),
                        "artist": e.get("uploader") or e.get("channel", "?"),
                        "url": f"https://www.youtube.com/watch?v={e.get('id', '')}",
                        "source": "YouTube", "duration": e.get("duration", 0)
                    })
            except Exception as e:
                self._set_dl_status(f"Search Error: {e}", RED)

            try:
                # SC Search usually returns fewer but we'll try to match N
                sc_count = 5 if self._search_limit <= 10 else 10
                with yt_dlp.YoutubeDL({"quiet": True, "extract_flat": True}) as ydl:
                    info = ydl.extract_info(f"scsearch{sc_count}:{q}", download=False)
                for e in (info.get("entries") or []):
                    results.append({
                        "title": e.get("title", "?"),
                        "artist": e.get("uploader") or e.get("channel", "?"),
                        "url": e.get("url") or e.get("webpage_url", ""),
                        "source": "SoundCloud", "duration": e.get("duration", 0)
                    })
            except: pass

            def update_ui():
                self.results_tree.delete(*self.results_tree.get_children())
                self.search_results = results
                for r in results:
                    tag = "sc" if r["source"] == "SoundCloud" else "yt"
                    dur = format_time(r["duration"]) if r["duration"] else "?"
                    self.results_tree.insert("", "end", tags=(tag,),
                        values=(r["source"], r["title"], r["artist"], dur))
                
                self.results_tree.config(height=min(12, max(1, len(results))))
                self._set_dl_status(
                    f"{len(results)} results" if results else "No results", TEXT3)
                
                if results and len(results) >= self._search_limit:
                    self.load_more_btn.pack(side="right", padx=(0, 2))
                
                self.after(50, lambda: self._scroll_canvas.config(scrollregion=self._scroll_canvas.bbox("all")))

            self.after(0, update_ui)
        threading.Thread(target=run, daemon=True).start()

    def _load_more_search(self):
        self._search_limit += 10
        self.do_search(reset_limit=False)

    # ═════════════════════════════════════════════════════════════════════════
    # SYNC LOGIC
    # ═════════════════════════════════════════════════════════════════════════

    def _sync_check(self):
        url = self.controller.yt_url_var.get().strip()
        if not url and not self.dl_playlists:
            messagebox.showwarning("No Source", "Enter a YT URL or load a CSV first.")
            return

        self._clear_results()
        self._set_dl_status("Syncing…", WARNING)

        known = set()
        for tracks in self.controller._library.values():
            for path in tracks:
                t, a = split_track_name(path)
                key = re.sub(r'[\W_]', '', f"{a}{t}").lower()
                known.add(key)

        def run():
            source_tracks = []
            if url:
                try:
                    with yt_dlp.YoutubeDL({"quiet": True, "extract_flat": "in_playlist",
                                           "ignoreerrors": True}) as ydl:
                        info = ydl.extract_info(url, download=False)
                    for e in (info.get("entries") or []):
                        t_full = e.get("title", "")
                        v_url = (e.get("url") or e.get("webpage_url") or
                                 (f"https://www.youtube.com/watch?v={e.get('id')}"
                                  if e.get('id') else None))
                        if " - " in t_full:
                            p = t_full.split(" - ", 1)
                            source_tracks.append({"artist": p[0].strip(),
                                                  "title": p[1].strip(), "url": v_url})
                        else:
                            source_tracks.append({"artist": "", "title": t_full,
                                                  "url": v_url})
                except Exception as ex:
                    self._set_dl_status(f"YT Error: {ex}", RED)

            for pl in self.dl_playlists:
                source_tracks.extend(pl["tracks"])

            def update_ui():
                self.sync_tracks = []
                new_count = 0
                for t in source_tracks:
                    key = re.sub(r'[\W_]', '',
                                 f"{t.get('artist', '')}{t.get('title', '')}").lower()
                    if key in known:
                        self.results_tree.insert("", "end", tags=("have",),
                            values=("✓ Have", t["title"], t.get("artist", ""), ""))
                    else:
                        self.results_tree.insert("", "end", tags=("new",),
                            values=("⬇ New", t["title"], t.get("artist", ""), ""))
                        self.sync_tracks.append(t)
                        new_count += 1

                if new_count > 0:
                    self._set_dl_status(f"{new_count} new tracks found!", SUCCESS)
                    self.dl_all_btn.config(fg=SIDEBAR, bg=GREEN)
                else:
                    self._set_dl_status("Library is up to date ✓", SUCCESS)
                self.results_tree.config(height=min(8, max(1, len(source_tracks))))
                self.after(50, lambda: self._scroll_canvas.config(scrollregion=self._scroll_canvas.bbox("all")))

            self.after(0, update_ui)
        threading.Thread(target=run, daemon=True).start()

    # ═════════════════════════════════════════════════════════════════════════
    # YT/CSV PREVIEW (fetch metadata → populate tree → then download)
    # ═════════════════════════════════════════════════════════════════════════

    def _preview_yt_playlist(self):
        """Fetch YT playlist metadata and show tracks in results tree for preview."""
        url = self.controller.yt_url_var.get().strip()
        if not url:
            messagebox.showwarning("No URL", "Enter a YouTube playlist URL.")
            return
        if not url.startswith("http"):
            messagebox.showwarning("Invalid", "URL must start with http.")
            return

        # Force playlist extraction
        if "list=" in url and "playlist?list=" not in url:
            list_id = re.search(r"list=([a-zA-Z0-9_-]+)", url)
            if list_id:
                url = f"https://www.youtube.com/playlist?list={list_id.group(1)}"

        self._clear_results()
        self._set_dl_status("Fetching playlist…", WARNING)

        known = set()
        for tracks in self.controller._library.values():
            for path in tracks:
                t, a = split_track_name(path)
                key = re.sub(r'[\W_]', '', f"{a}{t}").lower()
                known.add(key)

        def run():
            source_tracks = []
            try:
                with yt_dlp.YoutubeDL({"quiet": True, "extract_flat": "in_playlist",
                                       "ignoreerrors": True}) as ydl:
                    info = ydl.extract_info(url, download=False)
                for e in (info.get("entries") or []):
                    if not e: continue
                    t_full = e.get("title", "")
                    v_url = (e.get("url") or e.get("webpage_url") or
                             (f"https://www.youtube.com/watch?v={e.get('id')}"
                              if e.get('id') else None))
                    if " - " in t_full:
                        p = t_full.split(" - ", 1)
                        source_tracks.append({"artist": p[0].strip(),
                                              "title": p[1].strip(), "url": v_url})
                    else:
                        source_tracks.append({"artist": "", "title": t_full,
                                              "url": v_url})
            except Exception as ex:
                self._set_dl_status(f"YT Error: {ex}", RED)
                return

            def update_ui():
                self.sync_tracks = []
                self._preview_tracks = source_tracks
                new_count = 0
                for t in source_tracks:
                    key = re.sub(r'[\W_]', '',
                                 f"{t.get('artist', '')}{t.get('title', '')}").lower()
                    if key in known:
                        self.results_tree.insert("", "end", tags=("have",),
                            values=("✓ Have", t["title"], t.get("artist", ""), ""))
                    else:
                        self.results_tree.insert("", "end", tags=("new",),
                            values=("⬇ New", t["title"], t.get("artist", ""), ""))
                        self.sync_tracks.append(t)
                        new_count += 1

                total = len(source_tracks)
                if new_count > 0:
                    self._set_dl_status(
                        f"Preview: {total} tracks ({new_count} new)", SUCCESS)
                    self.dl_all_btn.config(fg=SIDEBAR, bg=GREEN)
                else:
                    self._set_dl_status(
                        f"Preview: {total} tracks — all in library ✓", SUCCESS)
                self.results_tree.config(height=min(8, max(1, total)))
                self.after(50, lambda: self._scroll_canvas.config(scrollregion=self._scroll_canvas.bbox("all")))

            self.after(0, update_ui)
        threading.Thread(target=run, daemon=True).start()

    # ═════════════════════════════════════════════════════════════════════════
    # DOWNLOAD LOGIC
    # ═════════════════════════════════════════════════════════════════════════

    def _download_selected(self):
        sel = self.results_tree.selection()
        if not sel:
            messagebox.showwarning("No Selection", "Select a result first.")
            return

        idx = self.results_tree.index(sel[0])
        vals = self.results_tree.item(sel[0], "values")

        track = None
        if self.search_results and idx < len(self.search_results):
            track = self.search_results[idx]
        elif self.sync_tracks:
            for t in self.sync_tracks:
                if t["title"] == vals[1]:
                    track = t
                    break

        if not track or not track.get("url"):
            messagebox.showwarning("No URL", "Cannot download: no URL for this track.")
            return

        dest = self._get_dest_dir(track.get("playlist_name"))
        os.makedirs(dest, exist_ok=True)
        self.is_dl = True
        self._show_progress()
        self._show_log()
        title_str = f"{track.get('artist', '')} - {track['title']}".strip(" -")
        self._set_dl_status(f"Downloading {title_str} — 1/1", WARNING)
        self._log(f"⬇ {title_str}", "normal")

        def on_done():
            self.is_dl = False
            self.after(0, self.controller._refresh_library)
            self.after(0, lambda: self._set_dl_status(f"✓ {title_str} — 1/1", SUCCESS))
            self.after(0, lambda: self._log(f"✓ {title_str}", "success"))
            self._hide_progress()

        def on_err(e):
            self.is_dl = False
            self.after(0, lambda: self._set_dl_status(f"✗ Failed — {e}", RED))
            self.after(0, lambda: self._log(f"✗ {title_str}: {e}", "error"))
            self._hide_progress()

        Downloader.download_single(track["url"], dest, on_done, on_err,
                                   progress_hook=self._on_yt_progress)

    def _download_all(self):
        if not self.sync_tracks:
            messagebox.showwarning("Nothing", "Run a sync check or preview first.")
            return

        self.is_dl = True
        self._show_progress()
        self._show_log()

        dest = self._get_dest_dir()
        os.makedirs(dest, exist_ok=True)
        total = len(self.sync_tracks)
        self._set_dl_status(f"Starting — 0/{total}", WARNING)
        self._log(f"⬇ Starting bulk download: {total} tracks", "normal")

        def on_progress(idx, tot, title):
            if title:
                self._set_dl_status(f"Downloading {title} — {idx + 1}/{tot}", WARNING)
            else:
                self.after(0, lambda i=idx, to=tot: self.controller.prog_var.set((i / to) * 100))
                self._set_dl_status(f"✓ Downloaded — {idx}/{tot}", SUCCESS)

        def on_log(title, is_success, err):
            if is_success:
                self.after(0, lambda: self._log(f"✓ {title}", "success"))
            else:
                self.after(0, lambda: self._log(f"✗ {title}: {err}", "error"))

        def on_complete():
            self.is_dl = False
            self.after(0, self.controller._refresh_library)
            self._set_dl_status(f"✓ Complete — {total}/{total}", SUCCESS)
            self.after(0, lambda: self._log(f"✅ All {total} tracks done!", "success"))
            self._hide_progress()

        Downloader.download_sync_tracks(
            self.sync_tracks, dest, self.controller.skip_var.get(),
            self.controller.sc_var.get(), on_progress, on_log, on_complete,
            lambda: not self.is_dl)

    def _start_csv_dl(self):
        if not self.dl_playlists:
            messagebox.showwarning("No CSVs", "Load CSV files first.")
            return

        self.is_dl = True
        self._show_progress()
        self._show_log()
        total = sum(len(pl["tracks"]) for pl in self.dl_playlists)
        self._set_dl_status(f"Starting CSV — 0/{total}", WARNING)

        flat_tracks = []
        for pl in self.dl_playlists:
            for t in pl["tracks"]:
                t_cpy = dict(t)
                t_cpy["playlist_name"] = pl["name"]
                flat_tracks.append(t_cpy)

        def get_out_folder(pl_name):
            return self._get_dest_dir(pl_name)

        def on_progress(done, tot, track_q):
            if track_q:
                self._set_dl_status(f"Downloading {track_q} — {done + 1}/{tot}", WARNING)
            else:
                self.after(0, lambda d=done: self.controller.prog_var.set((d / tot) * 100))
                self._set_dl_status(f"✓ Downloaded — {done}/{tot}", SUCCESS)

        def on_log(title, is_success, err):
            if is_success:
                self.after(0, lambda: self._log(f"✓ {title}", "success"))
            else:
                self.after(0, lambda: self._log(f"✗ {title}: {err}", "error"))

        def on_complete():
            self.is_dl = False
            self.after(0, self.controller._refresh_library)
            self._set_dl_status(f"✓ CSV Complete — {total}/{total}", SUCCESS)
            self.after(0, lambda: self._log("✅ CSV batch complete!", "success"))
            self._hide_progress()

        Downloader.download_csv_tracks(
            flat_tracks, get_out_folder, self.controller.skip_var.get(),
            self.controller.sc_var.get(), on_progress, on_log, on_complete,
            lambda: not self.is_dl)

    def _on_yt_progress(self, d):
        idx = d.get('playlist_index', 1)
        tot = d.get('playlist_count', 1)

        if d['status'] == 'downloading':
            p = d.get('_percent_str', ' 0%').strip()
            fn = d.get('filename', '')
            # Extract clean title from filename
            base = os.path.splitext(os.path.basename(fn))[0] if fn else ""
            display = base[:40] if base else f"Track {idx}"
            self._set_dl_status(f"Downloading {display} — {idx}/{tot}", WARNING)
            try:
                clean_p = re.sub(r'\x1b\[[0-9;]*m', '', p).replace('%', '').strip()
                self.controller.prog_var.set(float(clean_p))
            except: pass
        elif d['status'] == 'finished':
            fn = d.get('filename', '')
            base = os.path.splitext(os.path.basename(fn))[0] if fn else f"Track {idx}"
            self._set_dl_status(f"✓ {base[:40]} — {idx}/{tot}", SUCCESS)
            self.after(0, lambda b=base: self._log(f"✓ {b}", "success"))
            self.after(0, self.controller._refresh_library)

    # ═════════════════════════════════════════════════════════════════════════
    # CSV HELPERS
    # ═════════════════════════════════════════════════════════════════════════

    def _add_csvs(self):
        paths = filedialog.askopenfilenames(
            title="Select Exportify CSV files",
            filetypes=[("CSV", "*.csv"), ("All", "*.*")])
        if not paths: return
        for p in paths:
            tracks, err = parse_exportify_csv(p)
            if err or not tracks:
                self._log(f"⚠ {os.path.basename(p)}: {err or 'empty'}", "warning")
                continue
            name = re.sub(r'\.csv$', '', os.path.basename(p), flags=re.IGNORECASE).strip()
            if any(pl["name"] == name for pl in self.dl_playlists):
                self._log(f"⚠ Already loaded: {name}", "warning")
                continue
            self.dl_playlists.append({"name": name, "tracks": tracks})
            self._log(f"📄 {name} ({len(tracks)} tracks)", "success")

        total = sum(len(pl["tracks"]) for pl in self.dl_playlists)
        self.csv_count_lbl.config(
            text=f"{len(self.dl_playlists)} playlists • {total} tracks" if self.dl_playlists else "")

    # ═════════════════════════════════════════════════════════════════════════
    # UTILITY
    # ═════════════════════════════════════════════════════════════════════════

    def _clear_results(self):
        self.results_tree.delete(*self.results_tree.get_children())
        self.search_results = []
        self.sync_tracks = []
        self._preview_tracks = []
        self.dl_all_btn.config(fg=TEXT3, bg=BG4)
        self.controller.prog_var.set(0)
        self._clear_dl_status()
        self.results_tree.config(height=1)
        self.after(50, lambda: self._scroll_canvas.config(scrollregion=self._scroll_canvas.bbox("all")))

    def _on_tree_double_click(self):
        sel = self.results_tree.selection()
        if not sel: return
        vals = self.results_tree.item(sel[0], "values")
        title = vals[1].lower()
        for pl_name, tracks in self.controller._library.items():
            for path in tracks:
                if title in path.lower():
                    self.controller._play_with_context(path)
                    self._set_dl_status(f"▶ {os.path.basename(path)}", SUCCESS)
                    return
        self._set_dl_status(f"✗ No local match", RED)

    def _get_dest_dir(self, subfolder=None):
        """Determine download destination. Defaults to MUSIC_ROOT (Unsorted)
        unless the user explicitly picks a playlist from the SAVE TO combo."""
        if self.dest_combo_var.get() != "— choose playlist —":
            return self.controller.dl_dest_var.get()
        # Always default to MUSIC_ROOT (Unsorted) — never auto-create subfolders
        return self.controller.MUSIC_ROOT

    def _on_dest_selected(self, event):
        sel = self.dest_combo_var.get().split("  (")[0].strip()
        path = self.controller._find_playlist_dir(sel)
        if path: self.controller.dl_dest_var.set(path)

    def update_dest_combo(self, options):
        self.dest_combo["values"] = options
        if self.dest_combo_var.get() not in options:
            self.dest_combo_var.set("— choose playlist —")

    # ═════════════════════════════════════════════════════════════════════════
    # ALLOWED DUPLICATES HUB
    # ═════════════════════════════════════════════════════════════════════════

    def _build_allowed_tab(self):
        f = tk.Frame(self._tab_content, bg=BG)
        self._tab_frames["allowed"] = f
        
        lbl = tk.Label(f, text="These duplicates are 'allowed' and will be ignored in future scans.", 
                       font=("Helvetica Neue", 10), fg=TEXT3, bg=BG, justify="left")
        lbl.pack(fill="x", pady=(0,10))
        
        act = tk.Frame(f, bg=BG)
        act.pack(fill="x", pady=(0,10))
        
        unallow_btn = tk.Label(act, text="↺  Restore Selection", font=("Helvetica Neue", 10, "bold"), 
                               fg=BG, bg=BLUE, padx=14, pady=6, cursor="hand2")
        unallow_btn.bind("<Button-1>", lambda e: self._unallow_selected_dupes())
        
        # Backup buttons
        tk.Frame(act, bg=BG, width=20).pack(side="left") # Spacer
        
        export_btn = tk.Label(act, text="💾 Export Backup", font=("Helvetica Neue", 10), 
                              fg=TEXT2, bg=BG3, padx=12, pady=6, cursor="hand2")
        export_btn.pack(side="left")
        export_btn.bind("<Button-1>", lambda e: self._export_allowed_dupes())
        
        import_btn = tk.Label(act, text="📂 Import Backup", font=("Helvetica Neue", 10), 
                              fg=TEXT2, bg=BG3, padx=12, pady=6, cursor="hand2")
        import_btn.pack(side="left", padx=(8, 0))
        import_btn.bind("<Button-1>", lambda e: self._import_allowed_dupes())
        
        tree_f = tk.Frame(f, bg=BG2, highlightbackground=BORDER, highlightthickness=1)
        tree_f.pack(fill="both", expand=True)
        
        self.allowed_tree = ttk.Treeview(tree_f, columns=("Key",), show="headings", 
                                         style="Modern.Treeview", height=10)
        self.allowed_tree.heading("Key", text="Allowed Duplicate Identifier", anchor="w")
        self.allowed_tree.column("Key", width=600)
        
        sb = ModernScrollbar(tree_f, command=self.allowed_tree.yview, bg=BG2, width=4)
        self.allowed_tree.configure(yscrollcommand=sb.set)
        
        tree_f.grid_rowconfigure(0, weight=1)
        tree_f.grid_columnconfigure(0, weight=1)
        self.allowed_tree.grid(row=0, column=0, sticky="nsew")
        sb.grid(row=0, column=1, sticky="ns")

