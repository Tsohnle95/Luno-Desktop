import tkinter as tk
from theme import BG, BG2, BG3, BG4, TEXT, TEXT2, TEXT3, GREEN, GREEN_HOVER, PLAYER_BG, SIDEBAR
from utils import draw_gradient_rect, split_track_name, hex_to_rgb, rgb_to_hex, format_time
from theme import playlist_gradient
from widgets import RoundButton, SlimSlider
import os

class SongPageView(tk.Frame):
    def __init__(self, parent, controller):
        super().__init__(parent, bg=BG)
        self.controller = controller
        
        self._build_ui()
        
    def _build_ui(self):
        # ── BACKGROUND CANVAS ────────────────────────────────────────────────
        self.canvas = tk.Canvas(self, highlightthickness=0, bd=0, bg=BG)
        self.canvas.pack(fill="both", expand=True)
        self.canvas.bind("<Configure>", lambda e: self.update_view())
        
        # ── OVERLAY CONTAINER (Fills Canvas) ─────────────────────────────────
        self.content = tk.Frame(self.canvas, bg=BG, bd=0)
        self.canvas_window = self.canvas.create_window(0, 0, window=self.content, anchor="nw")
        
        # 1. Back button (Top Left)
        self.back_btn = tk.Label(self.content, text="✕", font=("Helvetica Neue", 24),
                                  fg=TEXT2, bg=BG, cursor="hand2", padx=20, pady=20)
        self.back_btn.place(x=20, y=20)
        self.back_btn.bind("<Button-1>", lambda e: self.controller._show_tab("library"))
        self.back_btn.bind("<Enter>", lambda e: self.back_btn.config(fg=TEXT))
        self.back_btn.bind("<Leave>", lambda e: self.back_btn.config(fg=TEXT2))

        # 2. Main Centered Content
        self.main_f = tk.Frame(self.content, bg=BG)
        self.main_f.pack(anchor="n", pady=30, fill="x", expand=True)
        
        self.main_f.columnconfigure(0, weight=1)
        
        # FIXED SIZE CONTAINER FOR ART (Ensures pixels, not characters)
        self.art_container = tk.Frame(self.main_f, bg=BG2, width=480, height=480, 
                                       highlightthickness=2, highlightbackground="#000")
        self.art_container.pack_propagate(False) # Force pixel sizes
        self.art_container.grid(row=0, column=0, pady=(0, 20))
        
        self.art_lbl = tk.Label(self.art_container, text="♫", font=("Helvetica", 80),
                                 fg=TEXT3, bg=BG2)
        self.art_lbl.pack(fill="both", expand=True)
        
        # Typography
        self.title_lbl = tk.Label(self.main_f, text="Song Title", 
                                   font=("Helvetica Neue", 42, "bold"),
                                   fg=TEXT, bg=BG, justify="center")
        self.title_lbl.grid(row=1, column=0, pady=(0, 10))
        
        def _on_main_resize(e):
            if e.width > 50:
                self.title_lbl.config(wraplength=e.width - 40)
        self.main_f.bind("<Configure>", _on_main_resize)
        
        self.artist_lbl = tk.Label(self.main_f, text="Artist Name",
                                    font=("Helvetica Neue", 20),
                                    fg=TEXT2, bg=BG, justify="center")
        self.artist_lbl.grid(row=2, column=0, pady=(0, 20))
        
        # Controls Frame
        self.ctrl_f = tk.Frame(self.main_f, bg=BG)
        self.ctrl_f.grid(row=3, column=0)
        
        # Controls row
        self.btn_row = tk.Frame(self.ctrl_f, bg=BG)
        self.btn_row.pack(anchor="center")
        
        def icon_lbl(text, cmd, size=20, fg=TEXT2):
            l = tk.Label(self.btn_row, text=text, font=("Helvetica Neue", size),
                         fg=fg, bg=BG, cursor="hand2", padx=20)
            l.bind("<Button-1>", lambda e: cmd())
            l.bind("<Enter>",    lambda e: l.config(fg=TEXT))
            l.bind("<Leave>",    lambda e: l.config(fg=fg))
            return l

        self.shuf_lbl = icon_lbl("⇄", lambda: self.controller._toggle_shuffle_ui(), 18)
        self.shuf_lbl.pack(side="left")
        
        icon_lbl("⏮", lambda: self.controller.engine.prev_track(), 26).pack(side="left")
        
        self.play_btn = RoundButton(self.btn_row, 64, lambda: self.controller.engine.toggle_play(), bg=BG)
        self.play_btn.pack(side="left", padx=20)
        
        icon_lbl("⏭", lambda: self.controller.engine.next_track(), 26).pack(side="left")
        
        self.rep_lbl = icon_lbl("↺", lambda: self.controller._toggle_repeat_ui(), 18)
        self.rep_lbl.pack(side="left")

        # Scrubber
        sc_f = tk.Frame(self.ctrl_f, bg=BG)
        sc_f.pack(fill="x", pady=(20, 0), ipadx=40)
        
        self.scrub = SlimSlider(sc_f, from_=0, to=100,
                                command=lambda v: self.controller.engine.on_scrub_moved(v),
                                track_color=BG4, fill_color=GREEN,
                                thumb_color=TEXT, height=22, bg=BG)
        self.scrub.pack(fill="x", expand=True)
        
        time_f = tk.Frame(sc_f, bg=BG)
        time_f.pack(fill="x", pady=5)
        self.time_cur = tk.Label(time_f, text="0:00", font=("Menlo", 10), fg=TEXT3, bg=BG)
        self.time_cur.pack(side="left")
        self.time_tot = tk.Label(time_f, text="0:00", font=("Menlo", 10), fg=TEXT3, bg=BG)
        self.time_tot.pack(side="right")

    def populate(self):
        """Called when this tab is shown."""
        self.update_view()

    def update_view(self):
        if not self.winfo_ismapped(): return
        
        w = self.winfo_width()
        h = self.winfo_height()
        if w < 10 or h < 10: return
        
        # Center horizontally dynamically
        self.canvas.itemconfig(self.canvas_window, width=w)
        
        # Responsive Layout Adjustments
        max_h = h - 260 # Buffer for controls, text, and padding (reduced from 340)
        max_w = w - 60
        art_size = min(max_h, max_w, 800) # Increased max size from 540 to 800
        if art_size < 200: art_size = 200 # Minimum size
        
        # Quantize art size to 20px increments to prevent spamming ArtCache thread pool during rapid window resize
        art_size = (int(art_size) // 20) * 20
        
        self.art_container.config(width=art_size, height=art_size)
        
        if w < 600:
            self.title_lbl.config(font=("Helvetica Neue", 32, "bold"))
            self.artist_lbl.config(font=("Helvetica Neue", 16))
        else:
            self.title_lbl.config(font=("Helvetica Neue", 42, "bold"))
            self.artist_lbl.config(font=("Helvetica Neue", 20))
        
        # Resize canvas window to fill self
        self.canvas.config(width=w, height=h)
        # If content goes beyond height on mobile, update canvas window to still expand
        self.canvas.itemconfig(self.canvas_window, width=w)
        
        # Get current track
        path = ""
        if self.controller.engine.queue and self.controller.engine.queue_idx >= 0:
            path = self.controller.engine.queue[self.controller.engine.queue_idx]
        
        if not path:
            self.title_lbl.config(text="Nothing playing")
            self.artist_lbl.config(text="")
            self._draw_background(BG, BG2)
            return

        title, artist = split_track_name(path)
        self.title_lbl.config(text=title)
        self.artist_lbl.config(text=artist or "Unknown Artist")
        
        # Scrubber state sync
        if hasattr(self.controller, 'scrub_var'):
            self.scrub.set(self.controller.scrub_var.get())

        # Artwork Fetching
        img = self.controller.art_cache.get(path, art_size, lambda p: self.after(0, self.update_view))
        if img:
            self.art_lbl.config(image=img, text="", bg=BG)
        else:
            self.art_lbl.config(image="", text="♫", bg=BG2)
            
        # Background Gradient
        pl = self.controller._selected_playlist or title
        c1, _ = playlist_gradient(pl)
        self._draw_background(c1, BG)

    def _draw_background(self, color1, color2):
        self.canvas.delete("all") # Clear canvas except the window (which is managed by itemid)
        # Re-create window item if cleared by delete("all")
        self.canvas_window = self.canvas.create_window(0, 0, window=self.content, anchor="nw")
        
        w = self.winfo_width()
        h = self.winfo_height()
        if w < 10 or h < 10: return
        
        # Re-sync content size
        self.canvas.itemconfig(self.canvas_window, width=w)
        
        # Dynamic height to cover whatever is larger, the window or the content
        content_h = self.content.winfo_reqheight()
        bg_h = max(h, content_h)

        r1, g1, b1 = hex_to_rgb(color1)
        r1, g1, b1 = r1*0.35, g1*0.35, b1*0.35 # Moodier shade
        r2, g2, b2 = hex_to_rgb(color2)
        
        steps = 60
        for i in range(steps):
            t = (i / steps)**1.5 # Subtle curve
            r, g, b = r1 + (r2 - r1) * t, \
                      g1 + (g2 - g1) * t, \
                      b1 + (b2 - b1) * t
            
            y0 = int(bg_h * i / steps)
            y1 = int(bg_h * (i + 1) / steps)
            self.canvas.create_rectangle(0, y0, w, y1, fill=rgb_to_hex(r, g, b), outline="")
        
        self.canvas.tag_lower("all")
        
        # Sync background colors across UI
        top_color = rgb_to_hex(r1, g1, b1)
        self.content.config(bg=top_color)
        self.main_f.config(bg=top_color)
        self._recursive_bg(self.main_f, top_color)
        self.back_btn.config(bg=top_color)

    def _recursive_bg(self, parent, color):
        parent.config(bg=color)
        for child in parent.winfo_children():
            if child == self.art_container or child == self.art_lbl:
                continue
            try:
                child.config(bg=color)
            except:
                pass
            if isinstance(child, tk.Frame):
                self._recursive_bg(child, color)
