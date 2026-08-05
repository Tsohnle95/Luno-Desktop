import tkinter as tk
from tkinter import ttk, filedialog, messagebox
import threading
import os, re
from urllib.parse import urlparse
from theme import BG, BG2, BG3, BG4, BORDER, GREEN, GREEN_HOVER, TEXT, TEXT2, TEXT3, SUCCESS, WARNING, RED, SC_COLOR, BLUE, SIDEBAR
from utils import split_track_name, format_time
from engine import parse_exportify_csv
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
        self._search_card_rows = []
        self._search_card_selected = None
        self._search_art_cache = {}
        self._search_art_fetching = set()
        self._downloaded_expanded = False
        self._direct_details_visible = False

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
        hdr = tk.Frame(self._inner, bg=BG, height=92)
        hdr.grid(row=row_idx, column=0, sticky="ew", padx=28, pady=(24, 0))
        hdr.grid_propagate(False)
        tk.Label(hdr, text="Downloader", font=("Helvetica Neue", 28, "bold"),
                 fg=TEXT, bg=BG, anchor="w").pack(anchor="w")
        tk.Label(hdr, text="Search and queue music.", font=("Helvetica Neue", 10),
                 fg=TEXT2, bg=BG, anchor="w").pack(anchor="w", pady=(2, 0))

        self._activity_row = tk.Frame(hdr, bg=BG, cursor="hand2")
        self._activity_row.pack(fill="x", pady=(10, 0))
        self._activity_row.pack_forget()
        self._activity_dot = tk.Label(self._activity_row, text="●", font=("Helvetica Neue", 9),
                                      fg=GREEN, bg=BG)
        self._activity_dot.pack(side="left")
        self._activity_lbl = tk.Label(self._activity_row, text="", font=("Helvetica Neue", 9),
                                      fg=TEXT2, bg=BG, anchor="w")
        self._activity_lbl.pack(side="left", padx=(7, 0))
        self._activity_link = tk.Label(self._activity_row, text="View queue  →",
                                       font=("Helvetica Neue", 9, "bold"), fg=GREEN, bg=BG,
                                       cursor="hand2")
        self._activity_link.pack(side="right")
        for widget in (self._activity_row, self._activity_link):
            widget.bind("<Button-1>", lambda e: self._switch_tab("urlqueue"))

        # ── ROW 1: TAB BAR ───────────────────────────────────────────────────
        row_idx = 1
        tab_bar_f = tk.Frame(self._inner, bg=BG)
        tab_bar_f.grid(row=row_idx, column=0, sticky="ew", padx=28, pady=(10, 10))

        self._tab_btns = {}
        for tab_id, label in [("search", "⌕  Search"),
                               ("playlist", "📋  YT / CSV"),
                               ("direct", "↗  Direct URL"),
                               ("sync", "🔗  Sync"),
                               ("urlqueue", "📋 URL Queue"),
                               ("dupes", "⚠ Duplicates"),
                               ("allowed", "✓ Allowed Dups")]:
            btn = tk.Label(tab_bar_f, text=label, font=("Helvetica Neue", 8, "bold"),
                           fg=TEXT3, bg=BG, padx=6, pady=8, cursor="hand2")
            btn.pack(side="left")
            btn.bind("<Button-1>", lambda e, t=tab_id: self._switch_tab(t))
            btn.bind("<Enter>", lambda e, b=btn: b.config(fg=TEXT) if b.cget("bg") == BG else None)
            btn.bind("<Leave>", lambda e, b=btn, t=tab_id: b.config(fg=TEXT3) if t != self._active_tab else None)
            self._tab_btns[tab_id] = btn

        # Thin progress bar (hidden by default, shown only during downloads)
        row_idx = 2
        self._prog_frame = tk.Frame(self._inner, bg=BG, height=3)
        self._prog_frame.grid(row=row_idx, column=0, sticky="ew", padx=28, pady=(4, 12))
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
        self._build_direct_tab()
        self._build_urlqueue_tab()
        self._build_dupes_tab()
        self._build_allowed_tab()

        # ── ROW 4: RESULTS ───────────────────────────────────────────────────
        row_idx = 4
        self.results_f = tk.Frame(self._inner, bg=BG, bd=0, highlightthickness=0)
        self.results_f.grid(row=row_idx, column=0, sticky="ew", padx=28, pady=(4, 0))
        self.results_f.grid_columnconfigure(0, weight=1)

        self.search_results_f = tk.Frame(self.results_f, bg=BG)
        self.search_results_f.grid(row=0, column=0, sticky="ew")
        self.search_results_f.grid_columnconfigure(0, weight=1)
        self._search_results_heading = tk.Label(
            self.search_results_f, text="Search results appear here.",
            font=("Helvetica Neue", 12, "bold"),
            fg=TEXT, bg=BG, anchor="w"
        )
        self._search_results_heading.pack(fill="x", pady=(4, 2))
        self._search_results_subheading = tk.Label(
            self.search_results_f,
            text="Use the tabs above for playlists, CSV imports, or a direct audio URL.",
            font=("Helvetica Neue", 9),
            fg=TEXT2, bg=BG, anchor="w"
        )
        self._search_results_subheading.pack(fill="x", pady=(0, 4))
        self._search_cards_f = tk.Frame(self.search_results_f, bg=BG)
        self._search_cards_f.pack(fill="x")

        self._tree_results_f = tk.Frame(self.results_f, bg=BG)
        self._tree_results_f.grid(row=0, column=0, sticky="ew")
        self._tree_results_f.grid_columnconfigure(0, weight=1)
        self._tree_results_f.grid_remove()

        self.results_tree = ttk.Treeview(self._tree_results_f, style="Results.Treeview",
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

        tree_sb = ModernScrollbar(self._tree_results_f, command=self.results_tree.yview,
                                  bg=BG, width=4)
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

        # ── ROW 5: CONTROLS (destination + options) ──────────────────────────
        row_idx = 5
        self.ctrl_f = tk.Frame(self._inner, bg=BG2, highlightbackground=BORDER,
                               highlightthickness=1)
        self.ctrl_f.grid(row=row_idx, column=0, sticky="ew", padx=28, pady=(14, 6))

        controls_inner = tk.Frame(self.ctrl_f, bg=BG2)
        controls_inner.pack(fill="x", padx=12, pady=9)
        tk.Label(controls_inner, text="SAVE TO", font=("Helvetica Neue", 9, "bold"),
                 fg=TEXT2, bg=BG2).pack(side="left", padx=(0, 6))
        self.dest_combo_var = tk.StringVar(value="— choose playlist —")
        self.dest_combo = ttk.Combobox(controls_inner, textvariable=self.dest_combo_var,
                                       font=("Helvetica Neue", 10), state="readonly", width=20,
                                       style="Dark.TCombobox")
        self.dest_combo.pack(side="left")
        self.dest_combo.bind("<<ComboboxSelected>>", self._on_dest_selected)

        # + New playlist button
        new_pl_btn = tk.Label(controls_inner, text="+ New", font=("Helvetica Neue", 9, "bold"),
                              fg=TEXT2, bg=BG3, padx=10, pady=4, cursor="hand2")
        new_pl_btn.pack(side="left", padx=(8, 0))
        new_pl_btn.bind("<Button-1>", lambda e: self._create_playlist_and_refresh())
        new_pl_btn.bind("<Enter>", lambda e: new_pl_btn.config(fg=TEXT, bg=BG4))
        new_pl_btn.bind("<Leave>", lambda e: new_pl_btn.config(fg=TEXT2, bg=BG3))

        # Spacer
        tk.Frame(controls_inner, bg=BG2, width=14).pack(side="left")

        for var, lbl in [(self.controller.skip_var, "Skip Existing"),
                         (self.controller.sc_var, "SC First")]:
            tk.Checkbutton(controls_inner, text=lbl, variable=var,
                           font=("Helvetica Neue", 9), fg=TEXT2, bg=BG2,
                           selectcolor=BG3, activebackground=BG2,
                           activeforeground=TEXT, highlightthickness=0,
                           bd=0, cursor="hand2").pack(side="left", padx=(0, 10))

        clear_btn = tk.Label(controls_inner, text="Clear", font=("Helvetica Neue", 10),
                             fg=TEXT3, bg=BG2, padx=6, cursor="hand2")
        clear_btn.pack(side="right")
        clear_btn.bind("<Button-1>", lambda e: self._clear_results())
        clear_btn.bind("<Enter>", lambda e: clear_btn.config(fg=TEXT))
        clear_btn.bind("<Leave>", lambda e: clear_btn.config(fg=TEXT3))

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

        # ── ROW 7: DOWNLOADED SONGS ACCORDION ────────────────────────────────
        row_idx = 7
        self._build_downloaded_section(row_idx)

        # ── ROW 8: COLLAPSIBLE LOG (toggle dropdown) ─────────────────────────
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

    def _build_downloaded_section(self, row_idx):
        """Build the mobile-parity completed-downloads accordion."""
        self.downloaded_f = tk.Frame(self._inner, bg=BG)
        self.downloaded_f.grid(row=row_idx, column=0, sticky="ew", padx=28, pady=(8, 0))
        self.downloaded_f.grid_columnconfigure(0, weight=1)

        header = tk.Frame(self.downloaded_f, bg=BG, cursor="hand2")
        header.grid(row=0, column=0, sticky="ew")
        header.grid_columnconfigure(1, weight=1)
        icon = tk.Label(header, text="↓", font=("Helvetica Neue", 13, "bold"),
                        fg=GREEN, bg=BG3, width=3, pady=4, cursor="hand2")
        icon.grid(row=0, column=0, rowspan=2, padx=(0, 10))
        self._downloaded_title_lbl = tk.Label(
            header, text="Downloaded songs", font=("Helvetica Neue", 11, "bold"),
            fg=TEXT, bg=BG, anchor="w", cursor="hand2"
        )
        self._downloaded_title_lbl.grid(row=0, column=1, sticky="ew")
        self._downloaded_count_lbl = tk.Label(
            header, text="Completed downloads will appear here", font=("Helvetica Neue", 9),
            fg=TEXT2, bg=BG, anchor="w", cursor="hand2"
        )
        self._downloaded_count_lbl.grid(row=1, column=1, sticky="ew", pady=(2, 0))
        self._downloaded_toggle_lbl = tk.Label(
            header, text="⌄", font=("Helvetica Neue", 16), fg=TEXT2, bg=BG,
            cursor="hand2", padx=8
        )
        self._downloaded_toggle_lbl.grid(row=0, column=2, rowspan=2)
        for widget in (header, icon, self._downloaded_title_lbl,
                       self._downloaded_count_lbl, self._downloaded_toggle_lbl):
            widget.bind("<Button-1>", lambda e: self._toggle_downloaded_section())

        tk.Frame(self.downloaded_f, bg=BG4, height=1).grid(
            row=1, column=0, sticky="ew", pady=(10, 0)
        )
        self._downloaded_body = tk.Frame(self.downloaded_f, bg=BG)
        self._downloaded_body.grid(row=2, column=0, sticky="ew", pady=(2, 0))
        self._downloaded_body.grid_remove()

    def _build_direct_tab(self):
        f = tk.Frame(self._tab_content, bg=BG)
        self._tab_frames["direct"] = f

        tk.Label(f, text="Direct audio URL", font=("Helvetica Neue", 13, "bold"),
                 fg=TEXT, bg=BG, anchor="w").pack(anchor="w")
        tk.Label(f, text="Paste a link to an audio file and queue it directly.",
                 font=("Helvetica Neue", 10), fg=TEXT2, bg=BG,
                 anchor="w").pack(anchor="w", pady=(2, 10))

        self.direct_url_var = tk.StringVar()
        bar = tk.Frame(f, bg=BG2, highlightbackground=BG4, highlightthickness=1)
        bar.pack(fill="x")
        tk.Label(bar, text="↗", font=("Helvetica Neue", 13), fg=TEXT2, bg=BG2,
                 padx=10).pack(side="left")
        self.direct_url_entry = tk.Entry(
            bar, textvariable=self.direct_url_var, bg=BG2, fg=TEXT,
            insertbackground=TEXT, relief="flat", font=("Helvetica Neue", 11),
            bd=8, highlightthickness=0
        )
        self.direct_url_entry.pack(side="left", fill="x", expand=True)
        self.direct_url_entry.bind("<Return>", lambda e: self._queue_direct_url())
        self.direct_url_entry.bind("<KP_Enter>", lambda e: self._queue_direct_url())
        direct_queue_btn = tk.Label(bar, text="QUEUE", font=("Helvetica Neue", 9, "bold"),
                                    fg=SIDEBAR, bg=GREEN, padx=18, pady=8,
                                    cursor="hand2")
        direct_queue_btn.pack(side="right")
        direct_queue_btn.bind("<Button-1>", lambda e: self._queue_direct_url())
        direct_queue_btn.bind("<Enter>", lambda e: direct_queue_btn.config(bg=GREEN_HOVER))
        direct_queue_btn.bind("<Leave>", lambda e: direct_queue_btn.config(bg=GREEN))

        details_toggle = tk.Label(f, text="Add details  ⌄", font=("Helvetica Neue", 10, "bold"),
                                 fg=GREEN, bg=BG, cursor="hand2")
        details_toggle.pack(anchor="w", pady=(8, 4))
        details_toggle.bind("<Button-1>", lambda e: self._toggle_direct_details(details_toggle))

        self._direct_details_f = tk.Frame(f, bg=BG)
        self.direct_title_var = tk.StringVar()
        self.direct_artist_var = tk.StringVar()
        for label, variable in (("Title", self.direct_title_var), ("Artist", self.direct_artist_var)):
            tk.Label(self._direct_details_f, text=label, font=("Helvetica Neue", 9, "bold"),
                     fg=TEXT2, bg=BG).pack(anchor="w", pady=(4, 2))
            entry = tk.Entry(self._direct_details_f, textvariable=variable, bg=BG2, fg=TEXT,
                             insertbackground=TEXT, relief="flat", font=("Helvetica Neue", 10),
                             bd=8, highlightthickness=0)
            entry.pack(fill="x")

        tk.Frame(f, bg=BG4, height=1).pack(fill="x", pady=(14, 0))

    # ═════════════════════════════════════════════════════════════════════════
    # TAB BUILDERS
    # ═════════════════════════════════════════════════════════════════════════

    def _build_search_tab(self):
        f = tk.Frame(self._tab_content, bg=BG)
        self._tab_frames["search"] = f

        tk.Label(f, text="Search YouTube and SoundCloud for individual songs.",
                 font=("Helvetica Neue", 10), fg=TEXT2, bg=BG).pack(anchor="w", pady=(0, 8))

        bar = tk.Frame(f, bg=BG2, highlightbackground=BG4, highlightthickness=1)
        bar.pack(fill="x")
        tk.Label(bar, text="⌕", font=("Helvetica Neue", 13), fg=TEXT2, bg=BG2,
                 padx=10).pack(side="left")
        self.search_entry = tk.Entry(bar, textvariable=self.controller.search_var,
                                     bg=BG2, fg=TEXT, insertbackground=TEXT,
                                     relief="flat", font=("Helvetica Neue", 11), bd=8,
                                     highlightthickness=0, highlightbackground=BG2, highlightcolor=BG2)
        self.search_entry.pack(side="left", fill="x", expand=True)
        self.search_entry.bind("<Return>", lambda e: self.do_search())
        self.search_entry.bind("<KP_Enter>", lambda e: self.do_search())

        self.search_clear_btn = tk.Label(bar, text="×", font=("Helvetica Neue", 15),
                                         fg=TEXT2, bg=BG2, padx=8, cursor="hand2")
        self.search_clear_btn.pack(side="right")
        self.search_clear_btn.bind("<Button-1>", lambda e: self._clear_search_input())
        self.search_clear_btn.bind("<Enter>", lambda e: self.search_clear_btn.config(fg=TEXT))
        self.search_clear_btn.bind("<Leave>", lambda e: self.search_clear_btn.config(fg=TEXT2))
        self.search_clear_btn.pack_forget()
        self.controller.search_var.trace_add("write", lambda *_: self._sync_search_clear_button())

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

    def _clear_search_input(self):
        self.controller.search_var.set("")
        self._clear_results()

    def _sync_search_clear_button(self):
        if not hasattr(self, "search_clear_btn"):
            return
        if self.controller.search_var.get().strip():
            if not self.search_clear_btn.winfo_ismapped():
                self.search_clear_btn.pack(side="right")
        elif self.search_clear_btn.winfo_ismapped():
            self.search_clear_btn.pack_forget()

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

        tk.Frame(f, bg=BG3, height=1).pack(fill="x", pady=(10, 6))

        self.sync_all_btn = tk.Label(f, text="⬇  Sync All Playlists",
            font=("Helvetica Neue", 10, "bold"),
            fg=SIDEBAR, bg=GREEN, padx=14, pady=8, cursor="hand2")
        self.sync_all_btn.pack(anchor="w")
        self.sync_all_btn.bind("<Button-1>", lambda e: self._do_sync_all())
        self.sync_all_btn.bind("<Enter>", lambda e: self.sync_all_btn.config(bg=GREEN_HOVER))
        self.sync_all_btn.bind("<Leave>", lambda e: self.sync_all_btn.config(bg=GREEN))

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

    def _build_urlqueue_tab(self):
        f = tk.Frame(self._tab_content, bg=BG)
        self._tab_frames["urlqueue"] = f

        tk.Label(f, text="Queue multiple YouTube playlist URLs to sync & download sequentially.",
                 font=("Helvetica Neue", 10), fg=TEXT3, bg=BG, wraplength=700).pack(anchor="w", pady=(0, 10))

        # URL input area
        tk.Label(f, text="Paste URLs (one per line):", font=("Helvetica Neue", 9, "bold"),
                 fg=TEXT2, bg=BG, anchor="w").pack(anchor="w", pady=(0, 4))
        url_text_frame = tk.Frame(f, bg=BG3, highlightbackground=BG4, highlightthickness=1)
        url_text_frame.pack(fill="x", pady=(0, 6))
        self.uq_url_text = tk.Text(url_text_frame, bg=BG3, fg=TEXT, insertbackground=TEXT,
                                    relief="flat", font=("Helvetica Neue", 10), bd=4,
                                    highlightthickness=0, height=4, wrap="word")
        self.uq_url_text.pack(side="left", fill="x", expand=True, padx=4, pady=4)

        # Action row
        uq_act = tk.Frame(f, bg=BG)
        uq_act.pack(fill="x", pady=(0, 10))

        fetch_btn = tk.Label(uq_act, text="Fetch Names", font=("Helvetica Neue", 9, "bold"),
                             fg=SIDEBAR, bg=GREEN, padx=12, pady=5, cursor="hand2")
        fetch_btn.pack(side="left")
        fetch_btn.bind("<Button-1>", lambda e: self._uq_fetch_urls())
        fetch_btn.bind("<Enter>", lambda e: fetch_btn.config(bg=GREEN_HOVER))
        fetch_btn.bind("<Leave>", lambda e: fetch_btn.config(bg=GREEN))

        clear_btn = tk.Label(uq_act, text="Clear", font=("Helvetica Neue", 9),
                             fg=TEXT3, bg=BG, padx=10, pady=5, cursor="hand2")
        clear_btn.pack(side="left", padx=(8, 0))
        clear_btn.bind("<Button-1>", lambda e: self._uq_clear())
        clear_btn.bind("<Enter>", lambda e: clear_btn.config(fg=TEXT))
        clear_btn.bind("<Leave>", lambda e: clear_btn.config(fg=TEXT3))

        # Queued items display
        tk.Label(f, text="Queued Playlists:", font=("Helvetica Neue", 9, "bold"),
                 fg=TEXT2, bg=BG, anchor="w").pack(anchor="w", pady=(0, 4))
        self._uq_queue_frame = tk.Frame(f, bg=BG2, highlightbackground=BORDER, highlightthickness=1)
        self._uq_queue_frame.pack(fill="x", pady=(0, 10))
        self._uq_items = []  # list of {"url": ..., "title": ..., "count": ..., "dest": ...}
        self._uq_lock = threading.Lock()

        self._uq_status_lbl = tk.Label(f, text="No URLs queued.", font=("Helvetica Neue", 10),
                                       fg=TEXT3, bg=BG, anchor="w")
        self._uq_status_lbl.pack(anchor="w", pady=(0, 6))

        # Destination override row
        dest_row = tk.Frame(f, bg=BG)
        dest_row.pack(fill="x", pady=(0, 12))
        tk.Label(dest_row, text="Destination:", font=("Helvetica Neue", 9, "bold"),
                 fg=TEXT2, bg=BG).pack(side="left", padx=(0, 8))
        self.uq_dest_var = tk.StringVar(value="Auto (match by playlist name)")
        uq_dest_options = ["Auto (match by playlist name)"]
        self.uq_dest_combo = ttk.Combobox(dest_row, textvariable=self.uq_dest_var,
                                          values=uq_dest_options, font=("Helvetica Neue", 10),
                                          state="readonly", width=30,
                                          style="Dark.TCombobox")
        self.uq_dest_combo.pack(side="left")

        # Process button
        self.uq_process_btn = tk.Label(f, text="▶  Sync & Download All Sequentially",
                                       font=("Helvetica Neue", 10, "bold"),
                                       fg=SIDEBAR, bg=GREEN, padx=16, pady=8, cursor="hand2")
        self.uq_process_btn.pack(anchor="w")
        self.uq_process_btn.bind("<Button-1>", lambda e: self._uq_process_all())
        self.uq_process_btn.bind("<Enter>", lambda e: self.uq_process_btn.config(bg=GREEN_HOVER))
        self.uq_process_btn.bind("<Leave>", lambda e: self.uq_process_btn.config(bg=GREEN))

        self._uq_running = False

    def _uq_redraw_list(self):
        for w in self._uq_queue_frame.winfo_children():
            w.destroy()
        with self._uq_lock:
            items = list(self._uq_items)
        if not items:
            self._uq_status_lbl.config(text="No URLs queued.")
            return
        self._uq_status_lbl.config(text=f"{len(items)} playlist(s) queued")
        for i, item in enumerate(items):
            title = item.get("title") or item["url"][:50]
            count = item.get("count", "?")
            row = tk.Frame(self._uq_queue_frame, bg=BG2)
            row.pack(fill="x", padx=8, pady=2)
            lbl = tk.Label(row, text=f"  {title}  ({count} tracks)", font=("Helvetica Neue", 10),
                           fg=TEXT, bg=BG2, anchor="w")
            lbl.pack(side="left", fill="x", expand=True)
            rm_btn = tk.Label(row, text="✕", font=("Helvetica Neue", 10),
                              fg=TEXT3, bg=BG2, padx=8, cursor="hand2")
            rm_btn.pack(side="right")
            def _remove(idx=i):
                with self._uq_lock:
                    self._uq_items.pop(idx)
                self._uq_redraw_list()
            rm_btn.bind("<Button-1>", lambda e, idx=i: _remove())
            rm_btn.bind("<Enter>", lambda e: rm_btn.config(fg=RED))
            rm_btn.bind("<Leave>", lambda e: rm_btn.config(fg=TEXT3))

    def _uq_clear(self):
        with self._uq_lock:
            self._uq_items = []
        self.uq_url_text.delete("1.0", "end")
        self._uq_redraw_list()
        self._uq_status_lbl.config(text="Cleared.")

    def _uq_fetch_urls(self):
        text = self.uq_url_text.get("1.0", "end").strip()
        if not text:
            messagebox.showwarning("No URLs", "Paste one or more playlist URLs first.")
            return
        urls = [line.strip() for line in text.splitlines() if line.strip()]
        if not urls:
            return

        self._uq_status_lbl.config(text=f"Fetching {len(urls)} playlist(s)...", fg=WARNING)

        def run():
            import yt_dlp
            for url in urls:
                try:
                    with yt_dlp.YoutubeDL({"quiet": True, "extract_flat": "in_playlist",
                                            "ignoreerrors": True,
                                            "socket_timeout": 30}) as ydl:
                        info = ydl.extract_info(url, download=False)
                    title = info.get("title", "Unknown Playlist")
                    count = len(info.get("entries") or [])
                    with self._uq_lock:
                        if not any(item["url"] == url for item in self._uq_items):
                            self._uq_items.append({"url": url, "title": title, "count": count})
                except Exception as ex:
                    with self._uq_lock:
                        self._uq_items.append({"url": url, "title": f"❌ {url[:50]}", "count": 0})
                    self._log(f"Failed to fetch {url}: {ex}", "error")
                self.after(0, self._uq_redraw_list)
            self.after(0, lambda: self._uq_status_lbl.config(text=f"Fetched {len(urls)} playlist(s).", fg=TEXT3))
            self.after(0, self._refresh_dest_options)

        threading.Thread(target=run, daemon=True).start()

    def _refresh_dest_options(self):
        """Populate destination combo with all playlist names."""
        options = ["Auto (match by playlist name)"]
        if hasattr(self.controller, '_library'):
            for name in sorted(self.controller._library.keys()):
                if name not in ("Unsorted", "All Music", "Favorites"):
                    options.append(name)
        self.uq_dest_combo["values"] = options
        if self.uq_dest_var.get() not in options:
            self.uq_dest_var.set("Auto (match by playlist name)")

    def _uq_process_all(self):
        if self._uq_running:
            messagebox.showwarning("Busy", "Already processing.")
            return
        if not self._uq_items:
            messagebox.showwarning("Empty", "No playlists queued.")
            return

        self._uq_running = True
        self.uq_process_btn.config(fg=TEXT3, bg=BG4)
        self._show_progress()
        self._show_log()
        self._log("▶ Starting URL queue batch...", "normal")
        self._uq_process_next(0)

    def _uq_process_next(self, idx):
        if idx >= len(self._uq_items):
            self._uq_running = False
            self.after(0, lambda: self.uq_process_btn.config(fg=SIDEBAR, bg=GREEN))
            self._log("✅ All playlist syncs complete!", "success")
            self.after(0, self.controller._refresh_library)
            self._hide_progress()
            return

        item = self._uq_items[idx]
        url = item["url"]
        title = item.get("title", "Playlist")
        dest_mode = self.uq_dest_var.get()

        # Determine destination folder
        if dest_mode == "Auto (match by playlist name)":
            dest_name = item.get("dest_name") or title
            dest = self.controller._find_playlist_dir(dest_name)
            if not dest:
                # Try creating it
                safe_name = re.sub(r'[<>:"/\\|?*]', '', dest_name).strip()
                if safe_name:
                    dest = os.path.join(self.controller.MUSIC_ROOT, safe_name)
                    os.makedirs(dest, exist_ok=True)
        else:
            dest_name = dest_mode
            dest = self.controller._find_playlist_dir(dest_name)

        if not dest:
            self._log(f"✗ No destination for '{title}' — skipping.", "error")
            self.after(0, lambda: self._uq_process_next(idx + 1))
            return

        os.makedirs(dest, exist_ok=True)
        total_items = len(self._uq_items)
        self._set_dl_status(f"Processing playlist {idx+1}/{total_items}: {title}", WARNING)
        self._log(f"▶ [{idx+1}/{total_items}] Syncing '{title}'...", "normal")

        def on_progress(track_idx, track_total, track_name):
            if track_name:
                self._set_dl_status(f"[{idx+1}/{total_items}] {title}: downloading {track_name} ({track_idx+1}/{track_total})", WARNING)
            else:
                self.after(0, lambda ti=track_idx: self.controller.prog_var.set((ti / max(track_total, 1)) * 100))

        def on_log(track_name, is_success, err):
            if is_success:
                self.after(0, lambda: self._log(f"  ✓ {track_name}", "success"))
            else:
                self.after(0, lambda: self._log(f"  ✗ {track_name}: {err}", "error"))

        def on_complete():
            self._log(f"✓ [{idx+1}/{total_items}] '{title}' done", "success")
            self.after(0, lambda: self._uq_process_next(idx + 1))

        from downloader import Downloader
        Downloader.download_playlist_sync(
            url, dest, self.controller.skip_var.get(),
            self.controller.sc_var.get(), on_progress, on_log, on_complete,
            lambda: not self._uq_running,
            on_playlist_msg=lambda msg: self._log(
                f"  · {msg}", "normal"))

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
        if tab_id in ("dupes", "allowed", "urlqueue"):
            self.ctrl_f.grid_remove()
            self.results_f.grid_remove()
            self.action_f.grid_remove()
            self._tab_frames[tab_id].pack(fill="both", expand=True)
            if tab_id == "dupes":
                self._populate_dupes()
            elif tab_id == "allowed":
                self._populate_allowed()
            else:
                self._uq_redraw_list()
                self._refresh_dest_options()
        elif tab_id == "direct":
            # The direct URL form owns its queue action, matching the mobile
            # source screen. Destination and table controls are not relevant.
            self.ctrl_f.grid_remove()
            self.results_f.grid_remove()
            self.action_f.grid_remove()
        else:
            self.ctrl_f.grid()
            self.results_f.grid()
            self.action_f.grid()
            if "dupes" in self._tab_frames:
                self._tab_frames["dupes"].pack_forget()
            if "allowed" in self._tab_frames:
                self._tab_frames["allowed"].pack_forget()
            self._show_result_surface(tab_id)
            
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
            initialfile="luno_allowed_backup.json"
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
        self._activity_lbl.config(text="Download in progress", fg=TEXT2)
        self._activity_row.pack(fill="x", pady=(10, 0))

    def _hide_progress(self):
        self.after(1500, lambda: self._prog_frame.grid_remove())
        self.after(1500, self._hide_activity)

    def _hide_activity(self):
        if self._activity_row.winfo_ismapped():
            self._activity_row.pack_forget()

    def _toggle_direct_details(self, toggle):
        self._direct_details_visible = not self._direct_details_visible
        if self._direct_details_visible:
            self._direct_details_f.pack(fill="x")
            toggle.config(text="Hide details  ⌃")
        else:
            self._direct_details_f.pack_forget()
            toggle.config(text="Add details  ⌄")
        self._inner.update_idletasks()
        self._scroll_canvas.config(scrollregion=self._scroll_canvas.bbox("all"))

    def _toggle_downloaded_section(self):
        self._downloaded_expanded = not self._downloaded_expanded
        if self._downloaded_expanded:
            self._downloaded_toggle_lbl.config(text="⌃", fg=GREEN)
            self._downloaded_body.grid()
            self._refresh_downloaded_section()
        else:
            self._downloaded_toggle_lbl.config(text="⌄", fg=TEXT2)
            self._downloaded_body.grid_remove()
        self._inner.update_idletasks()
        self._scroll_canvas.config(scrollregion=self._scroll_canvas.bbox("all"))

    def _downloaded_paths(self):
        paths = []
        seen = set()
        for path in self.controller._library.get("Unsorted", []):
            key = os.path.abspath(os.path.normpath(path))
            if key not in seen and os.path.isfile(path):
                paths.append(path)
                seen.add(key)
        return paths

    def _refresh_downloaded_section(self):
        if not hasattr(self, "_downloaded_body"):
            return
        paths = self._downloaded_paths()
        if paths:
            self._downloaded_count_lbl.config(text=f"{len(paths)} songs saved in Unsorted")
        else:
            self._downloaded_count_lbl.config(text="Completed downloads will appear here")
        if not self._downloaded_expanded:
            return

        for child in self._downloaded_body.winfo_children():
            child.destroy()
        if not paths:
            tk.Label(self._downloaded_body,
                     text="Start a search above and your finished songs will collect here.",
                     font=("Helvetica Neue", 9), fg=TEXT2, bg=BG,
                     anchor="w").pack(fill="x", pady=(8, 4))
            return

        for path in paths[:40]:
            self._add_downloaded_row(path)
        if len(paths) > 40:
            tk.Label(self._downloaded_body, text=f"Showing 40 of {len(paths)} songs",
                     font=("Helvetica Neue", 9), fg=TEXT3, bg=BG,
                     anchor="w").pack(fill="x", pady=(6, 0))

    def _add_downloaded_row(self, path):
        title, artist = split_track_name(path)
        row = tk.Frame(self._downloaded_body, bg=BG, cursor="hand2")
        row.pack(fill="x", pady=1)
        art = tk.Label(row, text="♫", font=("Helvetica Neue", 14), fg=TEXT3,
                       bg=BG3, width=4, height=2, cursor="hand2")
        art.pack(side="left", padx=(0, 10), pady=4)

        text = tk.Frame(row, bg=BG, cursor="hand2")
        text.pack(side="left", fill="both", expand=True, pady=4)
        tk.Label(text, text=title or os.path.basename(path),
                 font=("Helvetica Neue", 10, "bold"), fg=TEXT, bg=BG,
                 anchor="w").pack(fill="x")
        if artist:
            tk.Label(text, text=artist, font=("Helvetica Neue", 9), fg=TEXT2,
                     bg=BG, anchor="w").pack(fill="x", pady=(2, 0))

        def play(_event=None, p=path):
            self.controller._play_with_context(p)

        for widget in (row, art, text, *text.winfo_children()):
            widget.bind("<Button-1>", play)
            widget.bind("<Enter>", lambda e, r=row: r.config(bg=BG3))
            widget.bind("<Leave>", lambda e, r=row: r.config(bg=BG))

        img = self.controller.art_cache.get(path, 48, lambda p: self._refresh_downloaded_section())
        if img:
            art.config(image=img, text="", width=48, height=48, bg=BG)
            art._downloaded_image = img

    def on_library_updated(self):
        """Refresh downloaded-song metadata after the controller rescans."""
        self._refresh_downloaded_section()

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
        def update():
            self.dl_status_lbl.config(text=text, fg=color)
            if self._activity_row.winfo_ismapped():
                self._activity_lbl.config(text=text or "Download in progress", fg=color)
        self.after(0, update)

    def _clear_dl_status(self):
        def clear():
            self.dl_status_lbl.config(text="", fg=TEXT3)
            if self._activity_row.winfo_ismapped():
                self._activity_lbl.config(text="Download in progress", fg=TEXT2)
        self.after(0, clear)

    # ═════════════════════════════════════════════════════════════════════════
    # SEARCH LOGIC
    # ═════════════════════════════════════════════════════════════════════════

    def _queue_direct_url(self):
        url = self.direct_url_var.get().strip()
        parsed = urlparse(url)
        if parsed.scheme not in ("http", "https") or not parsed.netloc:
            messagebox.showwarning("Invalid URL", "Enter a valid http/https audio URL.")
            return
        if self.is_dl:
            messagebox.showwarning("Busy", "A download is already in progress.")
            return

        dest = self._get_dest_dir()
        os.makedirs(dest, exist_ok=True)
        self.is_dl = True
        self._show_progress()
        self._show_log()
        title = self.direct_title_var.get().strip() or os.path.basename(parsed.path) or "Direct audio"
        artist = self.direct_artist_var.get().strip()
        display = f"{artist} - {title}" if artist else title
        self._set_dl_status(f"Queuing {display}", WARNING)
        self._log(f"⬇ {display}", "normal")

        def on_done():
            def finish():
                self.is_dl = False
                self.direct_url_var.set("")
                self.direct_title_var.set("")
                self.direct_artist_var.set("")
                self.controller._refresh_library()
                self._set_dl_status(f"✓ {display}", SUCCESS)
                self._log(f"✓ {display}", "success")
                self._refresh_downloaded_section()
            self.after(0, finish)
            self._hide_progress()

        def on_error(error):
            def finish():
                self.is_dl = False
                self._set_dl_status(f"✗ Failed — {error}", RED)
                self._log(f"✗ {display}: {error}", "error")
            self.after(0, finish)
            self._hide_progress()

        Downloader.download_single(
            url, dest, on_done, on_error, progress_hook=self._on_yt_progress
        )

    def _show_result_surface(self, tab_id):
        if tab_id == "search":
            self._tree_results_f.grid_remove()
            self.search_results_f.grid()
        else:
            self.search_results_f.grid_remove()
            self._tree_results_f.grid()

    def _selected_search_result(self):
        index = self._search_card_selected
        if index is None or index >= len(self.search_results):
            return None
        return self.search_results[index]

    def _select_search_card(self, index):
        self._search_card_selected = index
        for row_index, item in enumerate(self._search_card_rows):
            row, widgets = item
            selected = row_index == index
            bg = BG3 if selected else BG
            row.config(bg=bg)
            for widget in widgets:
                try:
                    widget.config(bg=bg)
                except tk.TclError:
                    pass

    def _search_thumbnail_url(self, entry):
        thumbnail = entry.get("thumbnail")
        if thumbnail:
            return thumbnail
        thumbnails = entry.get("thumbnails") or []
        if thumbnails:
            return thumbnails[-1].get("url")
        return ""

    def _render_search_results(self, results):
        for child in self._search_cards_f.winfo_children():
            child.destroy()
        self._search_card_rows = []
        self._search_card_selected = None

        if not results:
            if self.controller.search_var.get().strip():
                self._search_results_heading.config(text="No matches yet")
                self._search_results_subheading.config(
                    text="Try adding the artist or a different title."
                )
            else:
                self._search_results_heading.config(text="Search results appear here.")
                self._search_results_subheading.config(
                    text="Use the tabs above for playlists, CSV imports, or a direct audio URL."
                )
            return

        query = self.controller.search_var.get().strip()
        self._search_results_heading.config(text=f"Search results  ·  {len(results)} matches")
        self._search_results_subheading.config(text=f'For "{query}"')
        for index, result in enumerate(results):
            self._add_search_result_card(index, result)

    def _add_search_result_card(self, index, result):
        row = tk.Frame(self._search_cards_f, bg=BG, cursor="hand2")
        row.pack(fill="x", pady=1)
        row.grid_columnconfigure(1, weight=1)

        art = tk.Label(row, text="♫", font=("Helvetica Neue", 16), fg=TEXT3,
                       bg=BG3, width=5, height=3, cursor="hand2")
        art.grid(row=0, column=0, padx=(0, 12), pady=6, sticky="ns")

        content = tk.Frame(row, bg=BG, cursor="hand2")
        content.grid(row=0, column=1, sticky="nsew", pady=6)
        title = result.get("title") or "Unknown title"
        artist = result.get("artist") or "Unknown artist"
        duration = format_time(result.get("duration", 0)) if result.get("duration") else ""
        metadata = " • ".join(value for value in (artist, duration) if value)
        tk.Label(content, text=title, font=("Helvetica Neue", 11, "bold"), fg=TEXT,
                 bg=BG, anchor="w", justify="left", wraplength=420).pack(fill="x")
        tk.Label(content, text=metadata or result.get("source", ""),
                 font=("Helvetica Neue", 9), fg=TEXT2, bg=BG, anchor="w",
                 justify="left").pack(fill="x", pady=(3, 0))
        tk.Label(content, text=result.get("source", ""), font=("Helvetica Neue", 8),
                 fg=TEXT3, bg=BG, anchor="w").pack(fill="x", pady=(2, 0))

        action = tk.Label(row, text="DOWNLOAD", font=("Helvetica Neue", 8, "bold"),
                          fg=SIDEBAR, bg=GREEN, padx=10, pady=6, cursor="hand2")
        action.grid(row=0, column=2, padx=(10, 0), pady=6, sticky="e")

        self._search_card_rows.append((row, [art, content]))

        def select(_event=None, selected=index):
            row.focus_set()
            self._select_search_card(selected)

        def download(_event=None, selected=index):
            self._select_search_card(selected)
            self._download_selected()

        for widget in (row, art, content, *content.winfo_children()):
            widget.bind("<Button-1>", select)
            widget.bind("<Double-Button-1>", lambda e: self._on_tree_double_click())
        row.configure(takefocus=True)
        action.bind("<Button-1>", download)
        action.bind("<Enter>", lambda e, button=action: button.config(bg=GREEN_HOVER))
        action.bind("<Leave>", lambda e, button=action: button.config(bg=GREEN))

        thumbnail_url = result.get("thumbnail", "")
        if thumbnail_url:
            self._load_search_artwork(thumbnail_url, art)

    def _load_search_artwork(self, url, label):
        if url in self._search_art_cache:
            image = self._search_art_cache[url]
            label.config(image=image, text="", width=60, height=60, bg=BG)
            label._search_image = image
            return
        if url in self._search_art_fetching:
            return
        self._search_art_fetching.add(url)

        def fetch():
            try:
                import io
                import requests
                from PIL import Image, ImageOps

                response = requests.get(url, timeout=8)
                response.raise_for_status()
                image = Image.open(io.BytesIO(response.content)).convert("RGB")
                image = ImageOps.fit(image, (60, 60), method=Image.Resampling.LANCZOS)
            except Exception:
                self._search_art_fetching.discard(url)
                return
            self.after(0, lambda: self._finish_search_artwork(url, image, label))

        threading.Thread(target=fetch, daemon=True).start()

    def _finish_search_artwork(self, url, image, label):
        self._search_art_fetching.discard(url)
        try:
            from PIL import ImageTk
            photo = ImageTk.PhotoImage(image)
        except Exception:
            return
        self._search_art_cache[url] = photo
        if label.winfo_exists():
            label.config(image=photo, text="", width=60, height=60, bg=BG)
            label._search_image = photo

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
                        "source": "YouTube", "duration": e.get("duration", 0),
                        "thumbnail": self._search_thumbnail_url(e),
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
                        "source": "SoundCloud", "duration": e.get("duration", 0),
                        "thumbnail": self._search_thumbnail_url(e),
                    })
            except: pass

            def update_ui():
                self.search_results = results
                self._render_search_results(results)
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
                                            "ignoreerrors": True,
                                            "socket_timeout": 30}) as ydl:
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

    def _do_sync_all(self):
        """Sync every playlist in the library that has a URL configured."""
        urls = self.controller._playlist_urls
        lib = self.controller._library
        pairs = [(name, url) for name, url in urls.items() if name in lib]
        if not pairs:
            messagebox.showwarning("No URLs",
                "No playlists have URLs configured.\n\n"
                "Set a URL via the playlist actions dropdown or "
                "right-click a playlist in the sidebar.")
            return
        self.sync_all_btn.config(fg=TEXT3, bg=BG4)
        self._show_progress()
        self._show_log()
        self._log(f"▶ Starting batch sync of {len(pairs)} playlist(s)...", "normal")
        self.controller._sync_all_playlists(
            pairs,
            on_track_log=lambda title, is_success, err: self._log(
                f"    {'↻' if err == 'already have' else '✓' if is_success else '✗'} {title}"
                + (f" — {err}" if err and err != "already have" else ""),
                "normal" if err == "already have" else "success" if is_success else "error"),
            on_playlist_msg=lambda msg: self._log(
                f"  · {msg}", "normal"),
            on_playlist_start=lambda name, i, total: self._log(
                f"▶ Syncing '{name}'  [{i}/{total} batch]...", "normal"),
            on_playlist_done=lambda name, success, count, attempted: self._log(
                f"  {'✓' if success else '✗'} '{name}'"
                + (f" — {count} new" if attempted and count == attempted else
                   f" — {count}/{attempted} new" if attempted else
                   f" {'done' if success else '— skipped'}."),
                "success" if success else "warning"),
            on_all_done=lambda new_count, attempted: self.after(0, lambda: [
                self.sync_all_btn.config(fg=SIDEBAR, bg=GREEN),
                self._hide_progress(),
                self._log(
                    f"✅ All playlist syncs complete. ({new_count} new)"
                    if attempted and new_count == attempted else
                    f"✅ All playlist syncs complete. ({new_count}/{attempted} new)"
                    if attempted else "✅ All playlist syncs complete — up to date.",
                    "success"),
                self.controller.status_var.set(
                    f"All playlists synced ✓ ({new_count} new)"
                    if attempted and new_count == attempted else
                    f"All playlists synced ✓ ({new_count}/{attempted} new)"
                    if attempted else "All playlists synced ✓"),
                self.controller._refresh_library(),
            ]),
            on_batch_progress=lambda pct: self.after(0, lambda p=pct: self.controller.prog_var.set(p))
        )

    # ═════════════════════════════════════════════════════════════════════════
    # YT/CSV PREVIEW (fetch metadata → populate tree → then download)
    # ═════════════════════════════════════════════════════════════════════════

    def _preview_yt_playlist(self):
        """Fetch YT playlist metadata and show tracks in results tree for preview."""
        url = self.controller.yt_url_var.get().strip()
        if not url:
            messagebox.showwarning("No URL", "Enter a YouTube playlist URL.")
            return
        parsed = urlparse(url)
        if parsed.scheme not in ('http', 'https') or not parsed.netloc:
            messagebox.showwarning("Invalid", "Must be a valid http/https URL.")
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
                                        "ignoreerrors": True,
                                        "socket_timeout": 30}) as ydl:
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
        track = None
        if self._active_tab == "search":
            track = self._selected_search_result()
            if not track:
                messagebox.showwarning("No Selection", "Select a result first.")
                return
        else:
            sel = self.results_tree.selection()
            if not sel:
                messagebox.showwarning("No Selection", "Select a result first.")
                return

            idx = self.results_tree.index(sel[0])
            vals = self.results_tree.item(sel[0], "values")
            if self.search_results and idx < len(self.search_results):
                track = self.search_results[idx]
            elif self.sync_tracks:
                for candidate in self.sync_tracks:
                    if candidate["title"] == vals[1]:
                        track = candidate
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
                progress = float(clean_p)
                self.after(0, lambda value=progress: self.controller.prog_var.set(value))
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
        self._render_search_results([])
        self.search_results = []
        self.sync_tracks = []
        self._preview_tracks = []
        self.dl_all_btn.config(fg=TEXT3, bg=BG4)
        self.controller.prog_var.set(0)
        self._clear_dl_status()
        self.results_tree.config(height=1)
        self.after(50, lambda: self._scroll_canvas.config(scrollregion=self._scroll_canvas.bbox("all")))

    def _on_tree_double_click(self):
        if self._active_tab == "search":
            result = self._selected_search_result()
            if not result:
                return
            title = (result.get("title") or "").lower()
        else:
            sel = self.results_tree.selection()
            if not sel:
                return
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
        self._refresh_dest_options()

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
