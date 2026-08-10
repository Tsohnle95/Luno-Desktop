import tkinter as tk
from theme import BG, BG2, BG3, BG4, TEXT, TEXT2, TEXT3, GREEN, GREEN_HOVER, PLAYER_BG, SIDEBAR
from utils import draw_gradient_rect, split_track_name, hex_to_rgb, rgb_to_hex
from theme import playlist_gradient
import os
from concurrent.futures import ThreadPoolExecutor

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
        self.main_f.pack(fill="both", expand=True)
        
        self.main_f.columnconfigure(0, weight=1)
        self.main_f.grid_rowconfigure(0, weight=1)  # top spacer
        self.main_f.grid_rowconfigure(4, weight=1)  # bottom spacer
        
        # FIXED SIZE CONTAINER FOR ART (Ensures pixels, not characters)
        self.art_container = tk.Frame(self.main_f, bg=BG2, width=480, height=480, 
                                       highlightthickness=2, highlightbackground="#000")
        self.art_container.pack_propagate(False) # Force pixel sizes
        self.art_container.grid(row=1, column=0, pady=(0, 20))
        
        self.art_lbl = tk.Label(self.art_container, text="♫", font=("Helvetica", 80),
                                 fg=TEXT3, bg=BG2)
        self.art_lbl.pack(fill="both", expand=True)
        
        # Typography
        self.title_lbl = tk.Label(self.main_f, text="Song Title", 
                                   font=("Helvetica Neue", 42, "bold"),
                                   fg=TEXT, bg=BG, justify="center")
        self.title_lbl.grid(row=2, column=0, pady=(0, 10))
        
        def _on_main_resize(e):
            if e.width > 50:
                self.title_lbl.config(wraplength=e.width - 40)
        self.main_f.bind("<Configure>", _on_main_resize)
        
        self.artist_lbl = tk.Label(self.main_f, text="Artist Name",
                                    font=("Helvetica Neue", 20),
                                    fg=TEXT2, bg=BG, justify="center")
        self.artist_lbl.grid(row=3, column=0, pady=(0, 60))

        self.favorite_btn = tk.Label(
            self.main_f, text="☆  Add to Favorites", font=("Helvetica Neue", 11, "bold"),
            fg=TEXT3, bg=BG, cursor="hand2"
        )
        self.favorite_btn.grid(row=4, column=0, pady=(0, 18))
        self.favorite_btn.bind("<Button-1>", lambda e: self.controller._toggle_current_favorite())
        self.favorite_btn.bind("<Enter>", lambda e: self.favorite_btn.config(fg=TEXT))
        self.favorite_btn.bind("<Leave>", lambda e: self._sync_favorite_button())

    def populate(self):
        """Called when this tab is shown."""
        self.update_view()

    def update_view(self):
        if not self.winfo_ismapped(): return
        
        w = self.winfo_width()
        h = self.winfo_height()
        if w < 10 or h < 10: return
        
        # Fill the canvas both directions to allow vertical centering
        self.canvas.itemconfig(self.canvas_window, width=w, height=h)
        
        # Responsive Layout Adjustments (more vertical space with controls removed)
        max_h = h - 170
        max_w = w - 60
        art_size = min(max_h, max_w, 1000)
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
        
        self.canvas.config(width=w, height=h)
        
        # Get current track
        path = ""
        if self.controller.engine.queue and self.controller.engine.queue_idx >= 0:
            path = self.controller.engine.queue[self.controller.engine.queue_idx]
        
        if not path:
            self._bg_generation += 1
            self._bg_signature = None
            if self._bg_future and not self._bg_future.done():
                self._bg_future.cancel()
            self._bg_loading = False
            self.title_lbl.config(text="Nothing playing")
            self.artist_lbl.config(text="")
            self._sync_favorite_button(None)
            self._draw_background(None)
            return

        title, artist = split_track_name(path)
        self.title_lbl.config(text=title)
        self.artist_lbl.config(text=artist or "Unknown Artist")
        self._sync_favorite_button(path)
        
        # Artwork Fetching
        img = self.controller.art_cache.get(path, art_size, lambda p: self.after(0, self.update_view))
        if img:
            self.art_lbl.config(image=img, text="", bg=BG)
        else:
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
        self._bg_generation += 1
        if self._bg_future and not self._bg_future.done():
            self._bg_future.cancel()
        self._bg_executor.shutdown(wait=False, cancel_futures=True)
        super().destroy()

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
        dark_bg = "#0f0f0f"
        self.content.config(bg=dark_bg)
        self.main_f.config(bg=dark_bg)
        self._recursive_bg(self.main_f, dark_bg)
        self.back_btn.config(bg=dark_bg)

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
