import tkinter as tk
from theme import PLAYER_BG, TEXT, TEXT2, TEXT3, SIDEBAR, BG4, GREEN, playlist_gradient
from utils import draw_gradient_rect

class RoundButton(tk.Canvas):
    """Circular play/pause button drawn on canvas."""
    def __init__(self, parent, size, command, **kw):
        super().__init__(parent, width=size, height=size,
                         highlightthickness=0, bd=0,
                         bg=kw.pop("bg", PLAYER_BG))
        self._size    = size
        self._command = command
        self._text    = "▶"
        self._hover   = False
        self.bind("<Configure>",      lambda e: self._draw())
        self.bind("<Button-1>",       lambda e: command())
        self.bind("<Enter>",          lambda e: self._on_enter())
        self.bind("<Leave>",          lambda e: self._on_leave())
        self.configure(cursor="hand2")
        self._draw()

    def set_text(self, t):
        self._text = t; self._draw()

    def _on_enter(self):
        self._hover = True; self._draw()

    def _on_leave(self):
        self._hover = False; self._draw()

    def _draw(self):
        self.delete("all")
        s = self._size
        r = s // 2
        fill = TEXT2 if self._hover else TEXT
        self.create_oval(1, 1, s-1, s-1, fill=fill, outline="")
        self.create_text(r, r, text=self._text,
                         font=("Helvetica Neue", int(s*0.38), "bold"),
                         fill=SIDEBAR)

class SlimSlider(tk.Canvas):
    """
    A slim, modern horizontal slider:
    - Thin track with rounded ends
    - Small circular thumb that only appears on hover
    - Click anywhere to jump
    """
    def __init__(self, parent, from_=0, to=100, command=None,
                 track_color=BG4, fill_color=TEXT2, thumb_color=TEXT,
                 height=20, **kw):
        super().__init__(parent, height=height, highlightthickness=0, bd=0,
                         bg=kw.pop("bg", PLAYER_BG), cursor="hand2")
        self._from    = from_
        self._to      = to
        self._value   = from_
        self._command = command
        self._track   = track_color
        self._fill    = fill_color
        self._thumb   = thumb_color
        self._hover   = False
        self._drag    = False
        self._h       = height

        self.bind("<Configure>",      lambda e: self._draw())
        self.bind("<Enter>",          lambda e: self._set_hover(True))
        self.bind("<Leave>",          lambda e: self._set_hover(False))
        self.bind("<ButtonPress-1>",  self._on_press)
        self.bind("<B1-Motion>",      self._on_drag)
        self.bind("<ButtonRelease-1>",self._on_release)

    def get(self): return self._value

    def set(self, val, trigger=False):
        self._value = max(self._from, min(self._to, val))
        self._draw()
        if trigger and self._command:
            self._command(self._value)

    def _set_hover(self, v):
        self._hover = v; self._draw()

    def _pct(self):
        r = self._to - self._from
        return (self._value - self._from) / r if r else 0

    def _val_from_x(self, x):
        w = self.winfo_width()
        pad = 8
        pct = max(0, min(1, (x - pad) / max(w - 2*pad, 1)))
        return self._from + pct * (self._to - self._from)

    def _draw(self):
        self.delete("all")
        w = self.winfo_width()
        h = self._h
        pad = 8
        cy  = h // 2
        th  = 4 if self._hover else 3
        r   = th // 2

        # Background track
        self.create_rounded_rect = self._rrect
        self._rrect(pad, cy-r, w-pad, cy+r, r, fill=self._track)

        # Filled portion
        fw = pad + int((w - 2*pad) * self._pct())
        if fw > pad:
            fill = GREEN if self._hover else TEXT3
            self._rrect(pad, cy-r, fw, cy+r, r, fill=fill)

        # Thumb (only on hover)
        if self._hover or self._drag:
            tr = 6
            tx = pad + int((w - 2*pad) * self._pct())
            self.create_oval(tx-tr, cy-tr, tx+tr, cy+tr,
                             fill=TEXT, outline="")

    def _rrect(self, x1, y1, x2, y2, r, fill):
        r = min(r, (y2-y1)//2, max((x2-x1)//2, 1))
        self.create_rectangle(x1+r, y1, x2-r, y2, fill=fill, outline="")
        self.create_rectangle(x1, y1+r, x2, y2-r, fill=fill, outline="")
        for cx, cy in [(x1+r,y1+r),(x2-r,y1+r),(x1+r,y2-r),(x2-r,y2-r)]:
            self.create_oval(cx-r, cy-r, cx+r, cy+r, fill=fill, outline="")

    def _on_press(self, e):
        self._drag = True
        self.set(self._val_from_x(e.x), trigger=True)

    def _on_drag(self, e):
        self._value = max(self._from, min(self._to, self._val_from_x(e.x)))
        self._draw()
        if self._command:
            self._command(self._value)

    def _on_release(self, e):
        self._drag = False
        val = self._val_from_x(e.x)
        self.set(val, trigger=True)

class PlaylistArt(tk.Canvas):
    def __init__(self, parent, size, name, app=None, paths=None, **kw):
        super().__init__(parent, width=size, height=size,
                         highlightthickness=0, bd=0, **kw)
        self._size = size
        self._name = name
        self._app = app
        self._paths = paths or []
        self._images = []          # Strong refs to prevent GC
        self._last_state = None    # Fingerprint of last draw to skip no-ops
        self._last_w = 0
        self._last_h = 0
        self._redraw_pending = False
        self.bind("<Configure>", self._on_configure)
        self.after_idle(self._draw)

    def _on_configure(self, event):
        # Only redraw if the widget actually resized
        if event.width != self._last_w or event.height != self._last_h:
            self._last_w = event.width
            self._last_h = event.height
            self._schedule_redraw()

    def _on_art_loaded(self, path):
        # Debounce: multiple images arriving close together get batched
        self._schedule_redraw()

    def _schedule_redraw(self):
        if not self._redraw_pending:
            self._redraw_pending = True
            self.after(30, self._do_redraw)

    def _do_redraw(self):
        self._redraw_pending = False
        self._draw()

    def _draw(self):
        # Build a fingerprint of what we're about to draw so we can skip no-ops
        target_paths = self._paths[:4] if len(self._paths) >= 4 else self._paths[:1] if self._paths else []
        required_size = self._size // 2 if len(target_paths) == 4 else self._size

        img_ids = []
        fetched = []
        if self._app and target_paths:
            for path in target_paths:
                img = self._app.art_cache.get(path, required_size, self._on_art_loaded)
                fetched.append(img)
                img_ids.append(id(img) if img else 0)

        state = (self._name, tuple(img_ids), self._size)
        if state == self._last_state:
            return  # Nothing changed, skip the redraw entirely
        self._last_state = state

        self.delete("all")
        self._images.clear()

        # Fallback gradient + icon (always drawn as base layer)
        c1, c2 = playlist_gradient(self._name)
        draw_gradient_rect(self, 0, 0, self._size, self._size, c1, c2)
        s = self._size
        self.create_text(s//2+2, s//2+2, text="♫",
                         font=("Helvetica Neue", int(s*0.38), "bold"), fill="#000000")
        self.create_text(s//2, s//2, text="♫",
                         font=("Helvetica Neue", int(s*0.38), "bold"), fill="#ffffff")

        if not fetched:
            return

        if len(target_paths) == 1:
            img = fetched[0]
            if img:
                self.create_image(self._size//2, self._size//2, image=img)
                self._images.append(img)
        elif len(target_paths) == 4:
            half_s = self._size / 2.0
            coords = [
                (half_s/2, half_s/2),
                (self._size - half_s/2, half_s/2),
                (half_s/2, self._size - half_s/2),
                (self._size - half_s/2, self._size - half_s/2)
            ]
            for idx, img in enumerate(fetched):
                if img:
                    x, y = coords[idx]
                    self.create_image(x, y, image=img)
                    self._images.append(img)

    def update_art(self, name, paths):
        self._name = name
        self._paths = paths or []
        self._last_state = None  # Force redraw
        self._draw()

    def update_name(self, name):
        self._name = name
        self._last_state = None
        self._draw()

class ModernScrollbar(tk.Canvas):
    """
    A custom, thin, dark scrollbar for modern looks.
    Bypasses native OS styling limitations.
    Connects to any widget with yview/xview.
    """
    def __init__(self, parent, command=None, orient="vertical", width=8, **kw):
        bg_col = kw.pop("bg", kw.get("bg", parent.cget("bg") if hasattr(parent, "cget") else "#121212"))
        super().__init__(parent, width=width if orient=="vertical" else 1,
                         height=width if orient=="horizontal" else 1,
                         highlightthickness=0, bd=0, bg=bg_col, **kw)
        self._command = command
        self._orient = orient
        self._width = width
        self._color = "#333333"
        self._hover_color = "#444444"
        self._active_color = "#555555"
        self._cur_color = self._color
        
        self._lo = 0.0
        self._hi = 1.0
        self._drag_start = None
        
        self.bind("<Enter>", lambda e: self._set_color(self._hover_color))
        self.bind("<Leave>", lambda e: self._set_color(self._color))
        self.bind("<ButtonPress-1>", self._on_press)
        self.bind("<B1-Motion>", self._on_drag)
        self.bind("<ButtonRelease-1>", lambda e: self._set_color(self._hover_color))
        self.bind("<Configure>", lambda e: self._draw())

    def set(self, lo, hi):
        """Standard scrollbar set method (lo, hi are 0.0 to 1.0)."""
        self._lo = float(lo)
        self._hi = float(hi)
        self._draw()

    def _set_color(self, c):
        self._cur_color = c; self._draw()

    def _on_press(self, e):
        # Check if click is on the thumb
        w, h = self.winfo_width(), self.winfo_height()
        if self._orient == "vertical":
            y_lo = self._lo * h
            y_hi = self._hi * h
            if y_lo <= e.y <= y_hi:
                self._drag_start = e.y
                self._set_color(self._active_color)
            else:
                # Page jump
                self._drag_start = None
                target = e.y / h
                self._scroll_to(target - (self._hi - self._lo)/2)
        else:
            x_lo = self._lo * w
            x_hi = self._hi * w
            if x_lo <= e.x <= x_hi:
                self._drag_start = e.x
                self._set_color(self._active_color)
            else:
                self._drag_start = None
                target = e.x / w
                self._scroll_to(target - (self._hi - self._lo)/2)

    def _on_drag(self, e):
        if self._drag_start is None: return
        w, h = self.winfo_width(), self.winfo_height()
        if self._orient == "vertical":
            delta = (e.y - self._drag_start) / h
            self._scroll_to(self._lo + delta)
            self._drag_start = e.y
        else:
            delta = (e.x - self._drag_start) / w
            self._scroll_to(self._lo + delta)
            self._drag_start = e.x

    def _scroll_to(self, lo):
        lo = max(0.0, min(1.0 - (self._hi - self._lo), lo))
        if self._command:
            self._command("moveto", lo)

    def _draw(self):
        self.delete("all")
        w, h = self.winfo_width(), self.winfo_height()
        if w < 2 or h < 2: return
        
        r = self._width // 2
        if self._orient == "vertical":
            y0 = self._lo * h
            y1 = self._hi * h
            # Draw thin rounded thumb
            if y1 - y0 < r*2: y1 = y0 + r*2
            self._rrect(1, y0, w-1, y1, r-1, fill=self._cur_color)
        else:
            x0 = self._lo * w
            x1 = self._hi * w
            if x1 - x0 < r*2: x1 = x0 + r*2
            self._rrect(x0, 1, x1, h-1, r-1, fill=self._cur_color)

    def _rrect(self, x1, y1, x2, y2, r, fill):
        r = max(1, min(r, (y2-y1)//2, (x2-x1)//2))
        self.create_rectangle(x1+r, y1, x2-r, y2, fill=fill, outline="")
        self.create_rectangle(x1, y1+r, x2, y2-r, fill=fill, outline="")
        for cx, cy in [(x1+r,y1+r),(x2-r,y1+r),(x1+r,y2-r),(x2-r,y2-r)]:
            self.create_oval(cx-r, cy-r, cx+r, cy+r, fill=fill, outline="")
