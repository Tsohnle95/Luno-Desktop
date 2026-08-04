import tkinter as tk
import math
from theme import BG, BG2, BG3, BG4, TEXT, TEXT2, TEXT3, GREEN, BORDER
from utils import draw_gradient_rect, split_track_name, hex_to_rgb, rgb_to_hex
from widgets import playlist_gradient

class ArtistsView(tk.Frame):
    def __init__(self, parent, controller):
        super().__init__(parent, bg=BG)
        self.controller = controller
        
        self.artists_data = {}
        self.artist_tracks = []
        self.artist_hover = None
        self._sel_indices = []
        self._sel_anchor = None
        self._pulse_frame = 0
        self.after(500, self._start_pulse)

        # Context menu
        self.ctx_menu = tk.Menu(self, tearoff=0, bg=BG3, fg=TEXT,
                                activebackground=BG4, activeforeground=TEXT,
                                font=("Helvetica Neue",12), bd=0, relief="flat")
        self.ctx_menu.add_command(label="  ▶   Play Now",          command=self.controller._ctx_play_now)
        self.ctx_menu.add_command(label="  ↑   Play Next",         command=self.controller._ctx_play_next)
        self.ctx_menu.add_command(label="  +   Add to Queue",      command=self.controller._ctx_add_queue)
        self.ctx_menu.add_separator()
        self.ctx_menu.add_command(label="  ♫   Add to Playlist…",  command=self.controller._ctx_add_playlist)
        self.ctx_menu.add_command(label="  ⇧   Export Selected JSON", command=self.controller._export_selected_json)
        self.ctx_menu.add_separator()
        self._favorite_menu_index = self.ctx_menu.index("end") + 1
        self.ctx_menu.add_command(label="  ☆   Add to Favorites", command=self.controller._ctx_toggle_favorite)
        self.ctx_menu.add_separator()
        self.ctx_menu.add_command(label="  🗑   Delete from Library",  command=self.controller._ctx_remove_from_library)

        self._build_ui()
        
    def _build_ui(self):
        self.grid_rowconfigure(1, weight=1)
        self.grid_columnconfigure(0, weight=1)
        
        hdr = tk.Canvas(self, height=120, bg=BG, highlightthickness=0)
        hdr.grid(row=0, column=0, sticky="ew")
        def _draw_artists_hdr(e, c=hdr):
            c.delete("grad")
            draw_gradient_rect(c, 0, 0, e.width, 120, "#2a1a2a", BG)
            c.tag_raise("title")
        hdr.bind("<Configure>", _draw_artists_hdr)
        hdr.create_text(28, 120-16, text="Artists", font=("Helvetica Neue",32,"bold"),
                        fill=TEXT, anchor="sw", tags="title")

        main_area = tk.Frame(self, bg=BG)
        main_area.grid(row=1, column=0, sticky="nsew")
        main_area.grid_rowconfigure(0, weight=1)
        main_area.grid_columnconfigure(0, weight=0)
        main_area.grid_columnconfigure(1, weight=1)

        al = tk.Frame(main_area, bg=BG2, width=220)
        al.grid(row=0, column=0, sticky="nsew")
        al.grid_propagate(False)
        tk.Label(al, text="ALL ARTISTS", font=("Helvetica Neue",9,"bold"),
                 fg=TEXT3, bg=BG2).pack(anchor="w", padx=16, pady=(14,8))
                 
        al_frame = tk.Frame(al, bg=BG2)
        al_frame.pack(fill="both", expand=True)
        
        self.artist_search_var = tk.StringVar()
        s_bar = tk.Frame(al_frame, bg=BG3, highlightthickness=0)
        s_bar.pack(fill="x", padx=16, pady=(0,8))
        tk.Label(s_bar, text="⌕", font=("Helvetica Neue",11), fg=TEXT3, bg=BG3).pack(side="left", padx=(4,0))
        entry = tk.Entry(s_bar, textvariable=self.artist_search_var, bg=BG3, fg=TEXT, insertbackground=TEXT, relief="flat", font=("Helvetica Neue", 10), highlightthickness=0)
        entry.pack(side="left", fill="x", expand=True, padx=4, pady=4)
        entry.bind("<KeyRelease>", self._filter_artists)
        
        self._filtered_artist_keys = []

        self.artist_listbox = tk.Listbox(al_frame, bg=BG2, fg=TEXT2,
                                           font=("Helvetica Neue",11), relief="flat",
                                           selectbackground=BG3, selectforeground=TEXT,
                                           highlightthickness=0, bd=8, activestyle="none")
        from widgets import ModernScrollbar
        a_sb = ModernScrollbar(al_frame, command=self.artist_listbox.yview, width=4)
        self.artist_listbox.config(yscrollcommand=a_sb.set)
        a_sb.pack(side="right", fill="y")
        self.artist_listbox.pack(fill="both", expand=True)
        self.artist_listbox.bind("<<ListboxSelect>>", self._on_artist_select)

        ar = tk.Frame(main_area, bg=BG)
        ar.grid(row=0, column=1, sticky="nsew", padx=(1,0))
        ar.grid_rowconfigure(1, weight=1); ar.grid_columnconfigure(0, weight=1)

        self.artist_header = tk.Label(ar, text="Select an artist",
                                        font=("Helvetica Neue",20,"bold"),
                                        fg=TEXT, bg=BG, anchor="w")
        self.artist_header.grid(row=0, column=0, sticky="ew", padx=24, pady=(16,8))

        self.artist_canvas = tk.Canvas(ar, bg=BG, highlightthickness=0, bd=0)
        as_sb = ModernScrollbar(ar, command=self.artist_canvas.yview)
        self.artist_canvas.config(yscrollcommand=as_sb.set)
        as_sb.grid(row=1, column=1, sticky="ns")
        self.artist_canvas.grid(row=1, column=0, sticky="nsew", padx=(24,0))
        self.artist_canvas.bind("<Configure>", lambda e: self.redraw_tracks())
        
        def _scroll_artist(event):
            self.artist_canvas.yview_scroll(int(-1*(event.delta)), "units")
        self.artist_canvas.bind("<MouseWheel>", _scroll_artist)
        self.artist_canvas.bind("<Button-1>",   self._on_artist_track_click)
        self.artist_canvas.bind("<Double-Button-1>", self._on_artist_track_dbl)
        self.artist_canvas.bind("<Button-2>",   self._on_artist_track_right)
        self.artist_canvas.bind("<Button-3>",   self._on_artist_track_right)
        self.artist_canvas.bind("<Motion>", lambda e: self._set_artist_hover(
            int(self.artist_canvas.canvasy(e.y)//56)))
        self.artist_canvas.bind("<Leave>",  lambda e: self._set_artist_hover(None))

    def populate(self):
        artists = {}
        for pl_name, tracks in self.controller._library.items():
            for path in tracks:
                title, artist = split_track_name(path)
                artist_key = artist.strip() if (artist and artist.strip()) else "Unknown Artist"
                artists.setdefault(artist_key, []).append(path)
                
        for a in artists:
            artists[a] = list(dict.fromkeys(artists[a]))
            
        self.artists_data = dict(sorted(artists.items(), key=lambda x: x[0].lower()))
        self._filter_artists()

    def _filter_artists(self, event=None):
        q = self.artist_search_var.get().strip().lower()
        self.artist_listbox.delete(0, "end")
        self._filtered_artist_keys = []
        for name, tracks in self.artists_data.items():
            if not q or q in name.lower():
                self.artist_listbox.insert("end", f"  {name}  ({len(tracks)})")
                self._filtered_artist_keys.append(name)

    def _on_artist_select(self, event):
        sel = self.artist_listbox.curselection()
        if not sel: return
        artist_name = self._filtered_artist_keys[sel[0]]
        self.artist_tracks = self.artists_data[artist_name]
        self._sel_indices = []
        self._sel_anchor = None
        self.artist_header.config(
            text=f"{artist_name}  ·  {len(self.artist_tracks)} tracks")
        self.redraw_tracks()

    def _start_pulse(self):
        if getattr(self, '_pulse_running', False) or not hasattr(self.controller, 'engine'):
            return
        self._pulse_running = True
        self._pulse_tick()

    def _pulse_tick(self):
        if not getattr(self, '_pulse_running', False) or not hasattr(self.controller, 'engine'):
            self._pulse_running = False
            return
        if self.artist_tracks:
            cur_path = self.controller.engine.queue[self.controller.engine.queue_idx] if 0 <= self.controller.engine.queue_idx < len(self.controller.engine.queue) else ""
            if cur_path and cur_path in self.artist_tracks:
                self._pulse_frame = (self._pulse_frame + 1) % 100
                self.redraw_tracks()
        self.after(80, self._pulse_tick)

    def redraw_tracks(self):
        c = self.artist_canvas
        c.delete("all")
        w = c.winfo_width()
        if w < 10 or not self.artist_tracks: return
        rh = 56
        canvas_h = c.winfo_height()
        full_h = max(len(self.artist_tracks) * rh, canvas_h, 10)
        c.configure(scrollregion=(0,0,w,full_h))
        try:
            top = max(0, c.canvasy(0)); bot = c.canvasy(canvas_h)
        except: top,bot = 0,full_h

        cur_path = self.controller.engine.queue[self.controller.engine.queue_idx] if 0 <= self.controller.engine.queue_idx < len(self.controller.engine.queue) else ""
        
        for i, path in enumerate(self.artist_tracks):
            y0=i*rh; y1=y0+rh
            if y1<top or y0>bot: continue
            title, artist = split_track_name(path)
            pl = next((pl for pl,tracks in self.controller._library.items() if path in tracks), "")
            is_h = (i == self.artist_hover)
            is_s = (i in self._sel_indices)
            is_playing = (path == cur_path)
            
            bg = BG4 if is_s else (BG3 if is_h else (BG if i%2==0 else "#161616"))
            c.create_rectangle(0,y0,w,y1, fill=bg, outline="")
            
            # Art strip: 44px
            art_size = 44
            art_half = art_size // 2
            art_x = 26
            art_yc = y0 + rh // 2
            img = self.controller.art_cache.get(path, art_size, lambda p: self.after(0, self.redraw_tracks))
            if img:
                c.create_image(art_x, art_yc, image=img)
            else:
                c1c,c2c = playlist_gradient(pl or title)
                r1g,g1g,b1g=hex_to_rgb(c1c); r2g,g2g,b2g=hex_to_rgb(c2c)
                for si in range(art_size):
                    t=si/art_size
                    c.create_rectangle(art_x - art_half, y0+6+si, art_x + art_half, y0+7+si,
                        fill=rgb_to_hex(int(r1g+(r2g-r1g)*t),
                                        int(g1g+(g2g-g1g)*t),
                                        int(b1g+(b2g-b1g)*t)),outline="")
                c.create_text(art_x, art_yc, text="♫", font=("Helvetica Neue",12,"bold"), fill="#ffffff", anchor="center")
            
            # Playing indicator
            bar_x = art_x + art_half + 8
            if is_playing:
                pulse = self._pulse_frame or 0
                bar_h = 14; bar_w = 3; gap = 2; n_bars = 4
                for bi in range(n_bars):
                    h_off = int(abs(math.sin((pulse / 20.0) + bi * 1.5)) * (bar_h - 4) + 4)
                    cy = art_yc + bar_h//2
                    c.create_rectangle(bar_x + bi*(bar_w+gap), cy - h_off//2,
                                       bar_x + bi*(bar_w+gap) + bar_w, cy + h_off//2,
                                       fill=GREEN, outline="")
            
            c.create_text(bar_x + 24, y0+rh//2-8, text=title[:55],
                          font=("Helvetica Neue",11,"bold"), fill=GREEN if is_playing else TEXT, anchor="w")
            c.create_text(bar_x + 24, y0+rh//2+10, text=pl[:50],
                          font=("Helvetica Neue",9), fill=TEXT3, anchor="w")
            if self.controller._is_favorite(path):
                c.create_text(w - 22, y0 + rh // 2, text="★",
                              font=("Helvetica Neue", 12), fill=GREEN, anchor="e")
            c.create_line(8, y1-1, w-8, y1-1, fill=BORDER)

    def _on_artist_track_click(self, event):
        cy = self.artist_canvas.canvasy(event.y)
        idx = int(cy // 56)
        if 0 <= idx < len(self.artist_tracks):
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
            self.redraw_tracks()

    def _on_artist_track_dbl(self, event):
        cy = self.artist_canvas.canvasy(event.y)
        idx = int(cy//56)
        if 0 <= idx < len(self.artist_tracks):
            self.controller._play_with_context(self.artist_tracks[idx], context_list=list(self.artist_tracks))

    def _on_artist_track_right(self, event):
        cy = self.artist_canvas.canvasy(event.y)
        idx = int(cy // 56)
        if 0 <= idx < len(self.artist_tracks):
            if idx not in self._sel_indices:
                self._sel_indices = [idx]
                self._sel_anchor = idx
            self.redraw_tracks()
            self.controller._prepare_favorite_menu(self.ctx_menu, self._favorite_menu_index)
            self.ctx_menu.post(event.x_root, event.y_root)

    def get_sel_paths(self):
        return [self.artist_tracks[i] for i in self._sel_indices if i < len(self.artist_tracks)]

    def _set_artist_hover(self, idx):
        if self.artist_hover != idx:
            self.artist_hover = idx; self.redraw_tracks()
