import os
import sys

def resource_path(relative_path):
    """ Get absolute path to resource, works for dev and for PyInstaller """
    try:
        # PyInstaller creates a temp folder and stores path in _MEIPASS
        base_path = sys._MEIPASS
    except Exception:
        base_path = os.path.abspath(".")
    return os.path.join(base_path, relative_path)

def format_time(s):
    if s < 0: s = 0
    m, sec = divmod(int(s), 60)
    return f"{m}:{sec:02d}"

def hex_to_rgb(h):
    h = h.lstrip("#")
    return tuple(int(h[i:i+2], 16) for i in (0, 2, 4))

def rgb_to_hex(r, g, b):
    return f"#{int(r):02x}{int(g):02x}{int(b):02x}"

def draw_gradient_rect(canvas, x1, y1, x2, y2, color1, color2, steps=80):
    r1,g1,b1 = hex_to_rgb(color1)
    r2,g2,b2 = hex_to_rgb(color2)
    h = max(y2 - y1, 1)
    for i in range(steps):
        t  = i / steps
        r  = r1+(r2-r1)*t; g = g1+(g2-g1)*t; b = b1+(b2-b1)*t
        y  = y1 + int(h*i/steps)
        yn = y1 + int(h*(i+1)/steps)
        canvas.create_rectangle(x1, y, x2, yn, fill=rgb_to_hex(r,g,b), outline="")

def split_track_name(path):
    name = os.path.splitext(os.path.basename(path))[0]
    if " - " in name:
        p = name.split(" - ", 1)
        return p[1].strip(), p[0].strip()
    return name, ""
