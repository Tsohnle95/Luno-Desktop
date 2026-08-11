import os
import sys

_TOUCHPAD_DELTA_PER_UNIT = 10.0


def _touchpad_vertical_delta(event):
    """Decode the signed Y component from Tk 9's packed touchpad delta."""
    try:
        delta = int(getattr(event, "delta", 0) or 0) & 0xffff
    except (TypeError, ValueError):
        return 0
    if delta & 0x8000:
        delta -= 0x10000
    return delta


def scroll_amount(event, wheel_units=1):
    """Return canvas yview units: negative up, positive down."""
    num = getattr(event, "num", None)
    if num in (4, "4"):
        return -wheel_units
    if num in (5, "5"):
        return wheel_units

    try:
        delta = int(getattr(event, "delta", 0) or 0)
    except (TypeError, ValueError):
        return 0

    if getattr(event, "_luno_touchpad", False):
        return getattr(event, "_luno_scroll_amount", 0)

    if delta > 0:
        return -wheel_units
    if delta < 0:
        return wheel_units
    return 0


def _is_descendant(widget, ancestor):
    while widget is not None:
        if widget == ancestor:
            return True
        try:
            widget = widget.master
        except AttributeError:
            return False
    return False


def bind_scroll_events(widget, callback, add=None, bind_all=False):
    """Bind mouse, legacy wheel, and Tk 9 trackpad scroll events."""
    bind = widget.bind_all if bind_all else widget.bind

    def _dispatch(event):
        if bind_all and not _is_descendant(getattr(event, "widget", None), widget):
            return
        return callback(event)

    for sequence in ("<MouseWheel>", "<Button-4>", "<Button-5>"):
        bind(sequence, _dispatch, add=add)

    touchpad_remainder = 0.0

    def _on_touchpad_scroll(event):
        nonlocal touchpad_remainder
        if bind_all and not _is_descendant(getattr(event, "widget", None), widget):
            return
        event._luno_touchpad = True
        touchpad_remainder += -_touchpad_vertical_delta(event) / _TOUCHPAD_DELTA_PER_UNIT
        amount = int(touchpad_remainder)
        touchpad_remainder -= amount
        if not amount:
            return
        event._luno_scroll_amount = amount
        return callback(event)

    # TouchpadScroll is unavailable on Tk versions before the high-resolution
    # scrolling support was added, so keep the desktop app backward compatible.
    try:
        bind("<TouchpadScroll>", _on_touchpad_scroll, add=add)
    except Exception:
        pass


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

def make_rounded_image(img, size, radius):
    """Return a rounded-corner version of a PIL Image as a PhotoImage."""
    from PIL import Image, ImageDraw
    c = img.resize((size, size), Image.Resampling.LANCZOS).convert("RGBA")
    m = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    ImageDraw.Draw(m).rounded_rectangle((0, 0, size, size), radius, fill=(255,255,255,255))
    c.paste(m, (0, 0), m)
    return c

def split_track_name(path):
    name = os.path.splitext(os.path.basename(path))[0]
    if " - " in name:
        p = name.split(" - ", 1)
        return p[1].strip(), p[0].strip()
    return name, ""
