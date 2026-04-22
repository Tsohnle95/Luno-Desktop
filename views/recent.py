import tkinter as tk
from theme import BG, BG3, BG4, TEXT, TEXT2, TEXT3, GREEN, GREEN_HOVER, SIDEBAR, BORDER
from utils import draw_gradient_rect, split_track_name, hex_to_rgb, rgb_to_hex
from widgets import playlist_gradient

class RecentView(tk.Frame):
    def __init__(self, parent, controller):
        super().__init__(parent, bg=BG)
        self.controller = controller
        
        self.recent_hover = None
        self._sel_indices = []
        self._sel_anchor = None

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
        self.ctx_menu.add_command(label="  🗑   Delete from Library",  command=self.controller._ctx_remove_from_library)

        self._build_ui()
        
    def _build_ui(self):
        self.grid_rowconfigure(2, weight=1)
        self.grid_columnconfigure(0, weight=1)
        
        hdr = tk.Canvas(self, height=120, bg=BG, highlightthickness=0)
        hdr.grid(row=0, column=0, sticky="ew")
        def _draw_recent_hdr(e, c=hdr):
            c.delete("grad")
            draw_gradient_rect(c, 0, 0, e.width, 120, "#1a1a3a", BG)
            c.tag_raise("title")
        hdr.bind("<Configure>", _draw_recent_hdr)
        hdr.create_text(28, 120-16, text="Recently Played", font=("Helvetica Neue",32,"bold"),
                        fill=TEXT, anchor="sw", tags="title")

        act = tk.Frame(self, bg=BG)
        act.grid(row=1, column=0, sticky="ew", padx=28, pady=(8,6))
        
        play_btn = tk.Label(act, text="▶  Play All",
                            font=("Helvetica Neue",10,"bold"),
                            fg=SIDEBAR, bg=GREEN, padx=14, pady=7, cursor="hand2")
        play_btn.pack(side="left")
        play_btn.bind("<Button-1>", lambda e: self.controller._play_recent_all())
        play_btn.bind("<Enter>", lambda e: play_btn.config(bg=GREEN_HOVER))
        play_btn.bind("<Leave>", lambda e: play_btn.config(bg=GREEN))

        clr = tk.Label(act, text="Clear History",
                       font=("Helvetica Neue",10), fg=TEXT3, bg=BG, cursor="hand2")
        clr.pack(side="left", padx=(16,0))
        clr.bind("<Button-1>", lambda e: self.controller._clear_recent())
        clr.bind("<Enter>", lambda e: clr.config(fg=TEXT2))
        clr.bind("<Leave>", lambda e: clr.config(fg=TEXT3))

        rf = tk.Frame(self, bg=BG)
        rf.grid(row=2, column=0, sticky="nsew", padx=28, pady=(0,16))
        rf.grid_rowconfigure(0, weight=1); rf.grid_columnconfigure(0, weight=1)

        self.recent_canvas = tk.Canvas(rf, bg=BG, highlightthickness=0, bd=0)
        from widgets import ModernScrollbar
        r_sb = ModernScrollbar(rf, command=self.recent_canvas.yview)
        self.recent_canvas.config(yscrollcommand=r_sb.set)
        r_sb.grid(row=0, column=1, sticky="ns")
        self.recent_canvas.grid(row=0, column=0, sticky="nsew")
        self.recent_canvas.bind("<Configure>",  lambda e: self.redraw())
        
        def _scroll_recent(event):
            self.recent_canvas.yview_scroll(int(-1*(event.delta)), "units")
        self.recent_canvas.bind("<MouseWheel>", _scroll_recent)
        self.recent_canvas.bind("<Button-1>",   self._on_recent_click)
        self.recent_canvas.bind("<Double-Button-1>", self._on_recent_dbl)
        self.recent_canvas.bind("<Button-2>",   self._on_recent_right)
        self.recent_canvas.bind("<Button-3>",   self._on_recent_right)
        self.recent_canvas.bind("<Motion>", self._on_recent_hover)
        self.recent_canvas.bind("<Leave>",  lambda e: self._set_recent_hover(None))

    def redraw(self):
        c = self.recent_canvas
        c.delete("all")
        w = c.winfo_width()
        if w < 10 or not self.controller._recently_played: return
        rh = 52
        canvas_h = c.winfo_height()
        full_h = max(len(self.controller._recently_played) * rh, canvas_h, 10)
        c.configure(scrollregion=(0,0,w,full_h))
        try:
            top = max(0, c.canvasy(0)); bot = c.canvasy(canvas_h)
        except: top,bot = 0,full_h
        
        for i, path in enumerate(self.controller._recently_played):
            y0 = i*rh; y1 = y0+rh
            if y1 < top or y0 > bot: continue
            title, artist = split_track_name(path)
            pl = next((pl for pl,tracks in self.controller._library.items() if path in tracks), "")
            is_h = (i == self.recent_hover)
            is_s = (i in self._sel_indices)
            
            bg = BG4 if is_s else (BG3 if is_h else (BG if i%2==0 else "#161616"))
            c.create_rectangle(0,y0,w,y1, fill=bg, outline="")
            
            c1c,c2c = playlist_gradient(pl or title)
            r1g,g1g,b1g = hex_to_rgb(c1c); r2g,g2g,b2g = hex_to_rgb(c2c)
            for si in range(36):
                t = si/36
                c.create_rectangle(12, y0+8+si, 48, y0+9+si,
                    fill=rgb_to_hex(int(r1g+(r2g-r1g)*t),
                                    int(g1g+(g2g-g1g)*t),
                                    int(b1g+(b2g-b1g)*t)), outline="")
            c.create_text(30, y0+26, text="♫", font=("Helvetica Neue",11,"bold"),
                          fill="#ffffff", anchor="center")
            c.create_text(60, y0+rh//2-8, text=title[:55],
                          font=("Helvetica Neue",11,"bold"), fill=TEXT, anchor="w")
            c.create_text(60, y0+rh//2+10, text=artist[:50],
                          font=("Helvetica Neue",9), fill=TEXT3, anchor="w")
            c.create_text(w-16, y0+rh//2, text=pl[:22],
                          font=("Helvetica Neue",9), fill=TEXT3, anchor="e")
            c.create_line(12, y1-1, w-12, y1-1, fill=BORDER)

    def _on_recent_click(self, event):
        cy = self.recent_canvas.canvasy(event.y)
        idx = int(cy // 52)
        if 0 <= idx < len(self.controller._recently_played):
            if (event.state & 0x0004) or (event.state & 0x0008): 
                if idx in self._sel_indices: self._sel_indices.remove(idx)
                else: 
                    self._sel_indices.append(idx)
                    self._sel_anchor = idx
            elif (event.state & 0x0001) and self._sel_anchor is not None:
                start = self._sel_anchor
                step = 1 if idx >= start else -1
                self._sel_indices = []
                for i in range(start, idx + step, step):
                    self._sel_indices.append(i)
            else:
                self._sel_indices = [idx]
                self._sel_anchor = idx
            self.redraw()

    def _on_recent_dbl(self, event):
        cy = self.recent_canvas.canvasy(event.y)
        idx = int(cy // 52)
        if 0 <= idx < len(self.controller._recently_played):
            self.controller._play_with_context(self.controller._recently_played[idx], 
                                                context_list=list(self.controller._recently_played))

    def _on_recent_right(self, event):
        cy = self.recent_canvas.canvasy(event.y)
        idx = int(cy // 52)
        if 0 <= idx < len(self.controller._recently_played):
            if idx not in self._sel_indices:
                self._sel_indices = [idx]
                self._sel_anchor = idx
            self.redraw()
            self.ctx_menu.post(event.x_root, event.y_root)

    def get_sel_paths(self):
        return [self.controller._recently_played[i] for i in self._sel_indices if i < len(self.controller._recently_played)]

    def _on_recent_hover(self, event):
        cy = self.recent_canvas.canvasy(event.y)
        self._set_recent_hover(int(cy // 52))

    def _set_recent_hover(self, idx):
        if self.recent_hover != idx:
            self.recent_hover = idx; self.redraw()
