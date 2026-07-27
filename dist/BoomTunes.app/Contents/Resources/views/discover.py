import tkinter as tk
from tkinter import ttk, messagebox
from theme import BG, BG2, BG3, BG4, TEXT, TEXT2, TEXT3, GREEN, GREEN_HOVER, BORDER, SIDEBAR
from utils import draw_gradient_rect, hex_to_rgb, rgb_to_hex
import threading

class DiscoverView(tk.Frame):
    def __init__(self, parent, controller):
        super().__init__(parent, bg=BG)
        self.controller = controller
        self.recs = []
        self._build_ui()

    def _build_ui(self):
        self.grid_rowconfigure(1, weight=1)
        self.grid_columnconfigure(0, weight=1)

        # Header
        self.hdr = tk.Canvas(self, height=180, bg=BG, highlightthickness=0)
        self.hdr.grid(row=0, column=0, sticky="ew")
        
        def _draw_hdr(e):
            self.hdr.delete("grad")
            draw_gradient_rect(self.hdr, 0, 0, e.width, 180, "#2c3e50", BG)
            self.hdr.tag_raise("txt")
            
        self.hdr.bind("<Configure>", _draw_hdr)
        self.hdr.create_text(32, 180-40, text="Discovery Hub", font=("Helvetica Neue", 32, "bold"),
                             fill=TEXT, anchor="sw", tags="txt")
        self.hdr.create_text(32, 180-18, text="New tracks based on your recent listening", 
                             font=("Helvetica Neue", 11), fill=TEXT3, anchor="sw", tags="txt")

        # Scrollable container
        self.canvas = tk.Canvas(self, bg=BG, highlightthickness=0, bd=0)
        from widgets import ModernScrollbar
        self.sb = ModernScrollbar(self, command=self.canvas.yview)
        self.canvas.config(yscrollcommand=self.sb.set)
        
        self.canvas.grid(row=1, column=0, sticky="nsew")
        self.sb.grid(row=1, column=1, sticky="ns")
        
        self.inner = tk.Frame(self.canvas, bg=BG)
        self.window = self.canvas.create_window(0, 0, window=self.inner, anchor="nw", tags="win")
        
        self.canvas.bind("<Configure>", lambda e: (
            self.canvas.itemconfig("win", width=e.width),
            self.canvas.config(scrollregion=self.canvas.bbox("all"))
        ))
        
        # Internal mousewheel binding
        def _on_wheel(e):
             self.canvas.yview_scroll(-1 if e.delta > 0 else 1, "units")
        self.canvas.bind("<MouseWheel>", _on_wheel)
        self.inner.bind("<MouseWheel>", _on_wheel)

    def render_recommendations(self, recs):
        self.recs = recs
        for w in self.inner.winfo_children(): w.destroy()
        
        if not recs:
            tk.Label(self.inner, text="No recommendations yet. Play a song or click 'Get Similar Tracks'.",
                     font=("Helvetica Neue", 11), fg=TEXT3, bg=BG, pady=100).pack()
            return

        for i, r in enumerate(recs):
            self._add_row(r, i)
        
        self.inner.update_idletasks()
        self.canvas.config(scrollregion=self.canvas.bbox("all"))

    def _add_row(self, r, i):
        f = tk.Frame(self.inner, bg=BG, padx=32, pady=12)
        f.pack(fill="x")
        
        row_bg = BG if i % 2 == 0 else "#161616"
        f.config(bg=row_bg)
        
        # Info
        info = tk.Frame(f, bg=row_bg)
        info.pack(side="left", fill="both", expand=True)
        
        # Truncate long titles/artists to prevent overflow
        title = r.get("title", "Unknown")
        if len(title) > 42: title = title[:40] + "..."
        
        artist = r.get("artist", "Unknown")
        match = int(r.get("match", 0) * 100)
        artist_str = f"{artist} • {match}% match"
        if len(artist_str) > 50: artist_str = artist_str[:47] + "..."
        
        t_lbl = tk.Label(info, text=title, font=("Helvetica Neue", 11, "bold"), 
                         fg=TEXT, bg=row_bg, anchor="w")
        t_lbl.pack(fill="x")
        
        a_lbl = tk.Label(info, text=artist_str, font=("Helvetica Neue", 9),
                         fg=TEXT3, bg=row_bg, anchor="w")
        a_lbl.pack(fill="x")

        # Actions
        act = tk.Frame(f, bg=row_bg)
        act.pack(side="right", padx=(16, 0))
        
        # Fixed width button for alignment consistency
        btn = tk.Label(act, text="＋ Add to Library", font=("Helvetica Neue", 9, "bold"),
                       fg=BG, bg=TEXT2, padx=12, pady=6, cursor="hand2", width=16)
        btn.pack(side="right")
        
        def do_add(art=r.get("artist", ""), tit=r.get("title", ""), b=btn):
            self.controller._download_track(art, tit)
            b.config(text="✓ Queued", bg=GREEN, state="disabled")
            b.unbind("<Button-1>")

        btn.bind("<Button-1>", lambda e: do_add())
        btn.bind("<Enter>", lambda e: btn.config(bg=TEXT) if btn.cget("state") != "disabled" else None)
        btn.bind("<Leave>", lambda e: btn.config(bg=TEXT2) if btn.cget("state") != "disabled" else None)
        
        # Propagate wheel
        for w in [f, info, act, t_lbl, a_lbl, btn]:
            w.bind("<MouseWheel>", lambda e: self.canvas.yview_scroll(-1 if e.delta > 0 else 1, "units"))
