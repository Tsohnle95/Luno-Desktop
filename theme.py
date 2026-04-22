import hashlib

# ── Palette ───────────────────────────────────────────────────────────────────
BG          = "#121212"
BG2         = "#181818"
BG3         = "#282828"
BG4         = "#3e3e3e"
SIDEBAR     = "#000000"
PLAYER_BG   = "#181818"
BORDER      = "#282828"
GREEN       = "#1DB954"
GREEN_HOVER = "#1ed760"
TEXT        = "#FFFFFF"
TEXT2       = "#b3b3b3"
TEXT3       = "#535353"
RED         = "#e91429"
WARNING     = "#f59e0b"
SUCCESS     = "#1DB954"
SC_COLOR    = "#ff5500"
BLUE        = "#4c8bf5"

PLAYLIST_GRADIENTS = [
    ("#1DB954","#158a3e"), ("#4c8bf5","#2D46B9"), ("#e91429","#a00d1c"),
    ("#ff5500","#b33c00"), ("#9b59b6","#6c3483"), ("#1abc9c","#148f77"),
    ("#f39c12","#b7770d"), ("#e74c3c","#922b21"), ("#2ecc71","#1a7a43"),
    ("#3498db","#1a5276"),
]

_PLAYLIST_CACHE = {}
def playlist_gradient(name):
    if name in _PLAYLIST_CACHE: return _PLAYLIST_CACHE[name]
    idx = int(hashlib.md5(name.encode()).hexdigest(), 16) % len(PLAYLIST_GRADIENTS)
    res = PLAYLIST_GRADIENTS[idx]
    _PLAYLIST_CACHE[name] = res
    return res
