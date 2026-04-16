#!/opt/homebrew/bin/python3.12
"""
Vibe Music Player v4
- Fixed library search (exact title/artist matching)
- Round play button via Canvas
- Modern thin scrubber and volume slider via Canvas (not ttk Scale)
- Duplicate song detection and management
- Gradient playlist art headers
build command: pyinstaller --noconsole --windowed --noconfirm --clean --name "Vibe" music_player.py
"""



import tkinter as tk
from tkinter import ttk, filedialog, messagebox, simpledialog
import threading
import os, re, csv, time, random, math, hashlib, shutil
os.environ["PATH"] += os.pathsep + "/opt/homebrew/bin" + os.pathsep + "/usr/local/bin"
os.environ["PATH"] += os.pathsep + "/opt/homebrew/bin"

import sys

def resource_path(relative_path):
    """ Get absolute path to resource, works for dev and for PyInstaller """
    try:
        # PyInstaller creates a temp folder and stores path in _MEIPASS
        base_path = sys._MEIPASS
    except Exception:
        base_path = os.path.abspath(".")
    return os.path.join(base_path, relative_path)

try:
    from mutagen.mp3 import MP3
    MUTAGEN_AVAILABLE = True
except ImportError:
    MUTAGEN_AVAILABLE = False

try:
    import pygame
    pygame.mixer.quit()
    # 2048 buffer is the 'sweet spot' for macOS CoreAudio. 
    # 4096 (what you had) often causes the 'shattering' sound on M1/M2/M3 Macs.
    pygame.mixer.pre_init(44100, -16, 2, 2048) 
    pygame.mixer.init()
    PYGAME_AVAILABLE = True
except Exception as e:
    print(f"Mixer Init Error: {e}")
    PYGAME_AVAILABLE = False

try:
    import yt_dlp
    YT_DLP_AVAILABLE = True
except ImportError:
    YT_DLP_AVAILABLE = False

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
# ── LIBRARY PATH SETUP ────────────────────────────────────────────────────────
import sys

# ── LIBRARY PATH SETUP ────────────────────────────────────────────────────────
# Default to ~/Music/app_music_folder
MUSIC_ROOT = os.path.join(os.path.expanduser("~"), "Music", "app_music_folder")

try:
    os.makedirs(MUSIC_ROOT, exist_ok=True)
except Exception as e:
    print(f"Error creating library folder: {e}")

WATCH_ROOTS = [MUSIC_ROOT]
# ──────────────────────────────────────────────────────────────────────────────
# ──────────────────────────────────────────────────────────────────────────────

def format_time(s):
    if s < 0: s = 0
    m, sec = divmod(int(s), 60)
    return f"{m}:{sec:02d}"

def playlist_gradient(name):
    idx = int(hashlib.md5(name.encode()).hexdigest(), 16) % len(PLAYLIST_GRADIENTS)
    return PLAYLIST_GRADIENTS[idx]

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

def scan_library(roots):
    lib = {}
    # Supported formats
    EXTS = (".mp3", ".m4a", ".wav", ".flac", ".mp4", ".webm")
    
    for root in roots:
        if not os.path.exists(root): continue
        
        # 1. Scan Subfolders (Playlists)
        for entry in sorted(os.scandir(root), key=lambda e: e.name.lower()):
            if entry.is_dir():
                songs = sorted([os.path.join(entry.path, f) for f in os.listdir(entry.path)
                               if f.lower().endswith(EXTS)],
                              key=lambda x: os.path.basename(x).lower())
                if songs:
                    lib[entry.name] = songs
        
        # 2. Scan Root Folder (Unsorted)
        root_songs = sorted([os.path.join(root, f) for f in os.listdir(root)
                            if f.lower().endswith(EXTS)],
                           key=lambda x: os.path.basename(x).lower())
        if root_songs:
            if "Unsorted" not in lib:
                lib["Unsorted"] = []
            # Use a set to prevent duplicates if multiple roots are used
            existing = set(lib["Unsorted"])
            for s in root_songs:
                if s not in existing:
                    lib["Unsorted"].append(s)
    return lib

def probe_soundcloud(query):
    try:
        with yt_dlp.YoutubeDL({"quiet":True,"skip_download":True}) as ydl:
            info = ydl.extract_info(f"scsearch1:{query}", download=False)
        if not info: return None, None
        best = max((fmt.get("abr") or 0 for fmt in info.get("formats",[])
                    if fmt.get("acodec","none") != "none"), default=0)
        return (f"scsearch1:{query}", f"{int(best)}kbps") if best > 180 else (None, None)
    except: return None, None

def parse_exportify_csv(filepath):
    tracks = []
    try:
        with open(filepath, newline="", encoding="utf-8-sig") as f:
            for row in csv.DictReader(f):
                t = row.get("Track Name","").strip()
                a = row.get("Artist Name(s)","").strip()
                if t and a:
                    tracks.append({"title":t,"artist":a,"album":row.get("Album Name","").strip()})
    except Exception as ex: return [], str(ex)
    return tracks, None

def split_track_name(path):
    name = os.path.splitext(os.path.basename(path))[0]
    if " - " in name:
        p = name.split(" - ", 1)
        return p[1].strip(), p[0].strip()
    return name, ""

def find_duplicates(library):
    """Find duplicate tracks across playlists by normalized title+artist."""
    seen = {}
    dupes = []
    for pl_name, tracks in library.items():
        for path in tracks:
            title, artist = split_track_name(path)
            key = re.sub(r'[\W_]','', f"{artist}{title}").lower()
            if key in seen:
                dupes.append({"path": path, "pl": pl_name,
                               "orig_path": seen[key]["path"],
                               "orig_pl": seen[key]["pl"],
                               "title": title, "artist": artist})
            else:
                seen[key] = {"path": path, "pl": pl_name}
    return dupes


# ── Custom Canvas Widgets ─────────────────────────────────────────────────────

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

    def _on_release(self, e):
        self._drag = False
        val = self._val_from_x(e.x)
        self.set(val, trigger=True)


class PlaylistArt(tk.Canvas):
    def __init__(self, parent, size, name, **kw):
        super().__init__(parent, width=size, height=size,
                         highlightthickness=0, bd=0, **kw)
        self._size = size; self._name = name
        self.bind("<Configure>", lambda e: self._draw())
        self._draw()

    def _draw(self):
        self.delete("all")
        c1, c2 = playlist_gradient(self._name)
        draw_gradient_rect(self, 0, 0, self._size, self._size, c1, c2)
        s = self._size
        self.create_text(s//2+2, s//2+2, text="♫",
                         font=("Helvetica Neue", int(s*0.38), "bold"), fill="#000000")
        self.create_text(s//2, s//2, text="♫",
                         font=("Helvetica Neue", int(s*0.38), "bold"), fill="#ffffff")

    def update_name(self, name):
        self._name = name; self._draw()


# ── Main App ──────────────────────────────────────────────────────────────────
class VibeMusicPlayer(tk.Tk):
    def _ctx_dump_playlist(self, source_pl_name):
        """Copies all songs from one playlist folder to another."""
        if source_pl_name == "All Music":
            messagebox.showwarning("Warning", "Cannot dump 'All Music'. Select a specific folder.")
            return
        
        paths = self._library.get(source_pl_name, [])
        if not paths:
            messagebox.showinfo("Empty", "This playlist has no songs to move.")
            return
            
        self._show_add_to_playlist_dialog(paths, f"Dump all from {source_pl_name}")

    def _show_add_to_playlist_dialog(self, paths, title_text):
        """Styled dialog to move selected files to a playlist folder."""
        playlists = [p for p in self._library.keys()]
        if not playlists:
            messagebox.showinfo("No Playlists", "Create a destination playlist first.")
            return

        dlg = tk.Toplevel(self)
        dlg.title(title_text)
        dlg.configure(bg=BG2)
        dlg.geometry("360x480")
        dlg.resizable(False, False)
        dlg.transient(self)
        dlg.grab_set()
        
        tk.Label(dlg, text=title_text, font=("Helvetica Neue", 14, "bold"), 
                 fg=TEXT, bg=BG2).pack(pady=(25, 5))
        tk.Label(dlg, text=f"{len(paths)} tracks will be copied", font=("Helvetica Neue", 10), 
                 fg=TEXT3, bg=BG2).pack(pady=(0, 15))
        
        lb_frame = tk.Frame(dlg, bg=BG3, bd=1, highlightbackground=BORDER, highlightthickness=1)
        lb_frame.pack(fill="both", expand=True, padx=30, pady=10)

        lb = tk.Listbox(lb_frame, bg=BG3, fg=TEXT2, font=("Helvetica Neue", 12), 
                        relief="flat", borderwidth=0, highlightthickness=0, 
                        selectbackground=BG4, activestyle="none")
        lb.pack(fill="both", expand=True, padx=5, pady=5)
        
        for pl in playlists:
            lb.insert("end", f"  {pl}")

        def do_transfer():
            sel = lb.curselection()
            if not sel: return
            target = playlists[sel[0]]
            dest_dir = self._find_playlist_dir(target)
            
            if not dest_dir: return
            
            count = 0
            for src in paths:
                try:
                    shutil.copy2(src, os.path.join(dest_dir, os.path.basename(src)))
                    count += 1
                except: pass
            
            dlg.destroy()
            self._refresh_library()
            messagebox.showinfo("Success", f"Copied {count} songs to '{target}'")

        # Styled Green Button (Matching the rest of the Vibe UI)
        btn = tk.Label(dlg, text="Confirm Transfer", font=("Helvetica Neue", 11, "bold"),
                       fg=SIDEBAR, bg=GREEN, padx=30, pady=12, cursor="hand2")
        btn.pack(pady=25)
        btn.bind("<Button-1>", lambda e: do_transfer())
        btn.bind("<Enter>", lambda e: btn.config(bg=GREEN_HOVER))
        btn.bind("<Leave>", lambda e: btn.config(bg=GREEN))

    # Now, find the existing _ctx_add_playlist and replace it with this one-liner:
    def _ctx_add_playlist(self):
        paths = self._get_sel_paths()
        if paths:
            self._show_add_to_playlist_dialog(paths, f"Add {len(paths)} songs to...")
    def __init__(self):
        super().__init__()
        # 1. This happens the second you open the app
        self.title("DevTunes") 
        
        try:
            self.create_command('tkAboutDialog', self._show_about)
        except:
            pass 

        self.geometry("1180x800")
        self.minsize(960, 660)
        self.configure(bg=BG)
        self.resizable(True, True)

        self._watch_roots          = [MUSIC_ROOT]
        self._queue                = []
        self._queue_idx            = -1
        self._playing              = False
        self._paused               = False
        self._duration             = 0.0
        self._track_start          = 0.0
        self._seek_pos             = 0.0
        self._shuffle              = False
        self._repeat               = False
        self._volume               = 0.7
        self._library              = {}
        self._dl_playlists         = []
        self._is_dl                = False
        self._dl_done              = 0
        self._search_results       = []
        self._search_offset        = 0
        self._queue_playlist_name  = None
        self._queue_playlist_paths = []
        self._selected_playlist    = None
        self._lib_searching        = False
        self._recently_played      = []
        self._crossfade_ms         = 0
        self._crossfade_secs       = 0
        self._sleep_after_id       = None
        self._viz_enabled          = False
        self._viz_after_id         = None
        self._sync_running         = False

        self.vol_var        = tk.DoubleVar(value=70)
        self.scrub_var      = tk.DoubleVar(value=0)
        self.status_var     = tk.StringVar(value="")
        self.prog_var       = tk.DoubleVar(value=0)
        self.search_var     = tk.StringVar()
        self.lib_search_var = tk.StringVar()
        self.yt_url_var     = tk.StringVar()
        self.dl_dest_var    = tk.StringVar(value=MUSIC_ROOT)
        self.sc_var         = tk.BooleanVar(value=True)
        self.skip_var       = tk.BooleanVar(value=True)
        self.meta_var       = tk.BooleanVar(value=True)

        self._configure_styles()
        self._build_ui()
        self._refresh_library()
        self._poll()
        self._init_watchdog()

        if not PYGAME_AVAILABLE: self._log("⚠ pygame not available","warning")
        if not YT_DLP_AVAILABLE: self._log("⚠ yt-dlp not found — brew install yt-dlp","warning")

    def _show_about(self):
        # 2. This ONLY happens if you click "About Dev Tunes" in the menu
        messagebox.showinfo("DevTunes", "DevTunes v4\nProfessional Music Player")
    def _configure_styles(self):
        s = ttk.Style(self)
        s.theme_use("clam")
        s.configure("Dark.TNotebook", background=BG2, borderwidth=0, tabmargins=0)
        s.configure("Dark.TNotebook.Tab", background=BG2, foreground=TEXT3,
                    padding=[18,8], font=("Helvetica Neue",10,"bold"), borderwidth=0)
        s.map("Dark.TNotebook.Tab",
              background=[("selected",BG2)], foreground=[("selected",TEXT)])
        s.configure("Green.Horizontal.TProgressbar",
                    troughcolor=BG3, background=GREEN,
                    bordercolor=BG3, lightcolor=GREEN, darkcolor=GREEN, thickness=3)
        for style, bg, rh in [("Lib.Treeview",BG,48),
                               ("Queue.Treeview",BG2,38),
                               ("Results.Treeview",BG,44)]:
            s.configure(style, background=bg, foreground=TEXT2,
                        fieldbackground=bg, borderwidth=0,
                        font=("Helvetica Neue",11), rowheight=rh)
            s.map(style, background=[("selected",BG3)], foreground=[("selected",TEXT)])
            s.configure(f"{style}.Heading", background=bg, foreground=TEXT3,
                        borderwidth=0, font=("Helvetica Neue",9,"bold"), relief="flat")

    # ═══════════════════════════════════════════════════════════════════════════
    def _build_ui(self):
        self.grid_rowconfigure(0, weight=1)
        self.grid_rowconfigure(1, weight=0)
        self.grid_columnconfigure(0, weight=0)
        self.grid_columnconfigure(1, weight=1)

        sidebar = tk.Frame(self, bg=SIDEBAR, width=240)
        sidebar.grid(row=0, column=0, sticky="nsew")
        sidebar.grid_propagate(False)
        self._build_sidebar(sidebar)

        main = tk.Frame(self, bg=BG)
        main.grid(row=0, column=1, sticky="nsew")
        self._build_main(main)

        tk.Frame(self, bg="#000", height=1).grid(row=1, column=0, columnspan=2, sticky="ew")
        player = tk.Frame(self, bg=PLAYER_BG, height=94)
        player.grid(row=1, column=0, columnspan=2, sticky="ew")
        player.grid_propagate(False)
        self._build_player(player)
        self._bind_keys()

    # ── Sidebar ───────────────────────────────────────────────────────────────
    def _build_sidebar(self, parent):
        logo_f = tk.Frame(parent, bg=SIDEBAR)
        logo_f.pack(fill="x", padx=24, pady=(28,24))
        tk.Label(logo_f, text="DevTunes", font=("Helvetica Neue",24,"bold"),
                 fg=TEXT, bg=SIDEBAR).pack(side="left")
        tk.Label(logo_f, text="▮", font=("Helvetica Neue",9),
                 fg=GREEN, bg=SIDEBAR).pack(side="left", padx=(3,0), pady=(10,0))

        for icon, label, cmd in [
            ("⌂","Home",        lambda: self._show_tab("library")),
            ("⌕","Search",      lambda: self._show_tab("search")),
            ("🕐","Recent",      lambda: self._show_tab("recent")),
            ("👤","Artists",     lambda: self._show_tab("artists")),
            ("⬇","Download",    lambda: self._show_tab("download")),
            ("🔄","Sync",        lambda: self._show_tab("sync")),
            ("⚙","Settings",    lambda: self._show_tab("settings")),
        ]:
            f = tk.Frame(parent, bg=SIDEBAR, cursor="hand2")
            f.pack(fill="x")
            row = tk.Frame(f, bg=SIDEBAR, cursor="hand2")
            row.pack(fill="x", padx=16, pady=1)
            tk.Label(row, text=icon, font=("Helvetica Neue",14), fg=TEXT3,
                     bg=SIDEBAR, width=2, anchor="center").pack(side="left", padx=(0,10))
            lbl = tk.Label(row, text=label, font=("Helvetica Neue",13),
                           fg=TEXT2, bg=SIDEBAR, anchor="w", cursor="hand2", pady=9)
            lbl.pack(side="left", fill="x", expand=True)
            def _b(c, fr=f, rw=row, lb=lbl):
                lb.bind("<Button-1>", lambda e: c())
                rw.bind("<Button-1>", lambda e: c())
                for w in (lb,rw,fr):
                    w.bind("<Enter>", lambda e,_f=fr,_r=rw,_l=lb:
                           [_f.config(bg=BG3),_r.config(bg=BG3),_l.config(bg=BG3,fg=TEXT)])
                    w.bind("<Leave>", lambda e,_f=fr,_r=rw,_l=lb:
                           [_f.config(bg=SIDEBAR),_r.config(bg=SIDEBAR),_l.config(bg=SIDEBAR,fg=TEXT2)])
            _b(cmd)

        tk.Frame(parent, bg=BG3, height=1).pack(fill="x", padx=16, pady=(16,12))

        lib_hdr = tk.Frame(parent, bg=SIDEBAR)
        lib_hdr.pack(fill="x", padx=16, pady=(0,8))
        tk.Label(lib_hdr, text="Your Library",
                 font=("Helvetica Neue",12,"bold"), fg=TEXT2, bg=SIDEBAR).pack(side="left", padx=8)
        for txt, cmd in [("+", self._create_playlist_dialog), ("↻", self._refresh_library)]:
            l = tk.Label(lib_hdr, text=txt, font=("Helvetica Neue",15 if txt=="+" else 14),
                         fg=TEXT3, bg=SIDEBAR, cursor="hand2")
            l.pack(side="right", padx=2)
            l.bind("<Button-1>", lambda e, c=cmd: c())
            l.bind("<Enter>",    lambda e, _l=l: _l.config(fg=TEXT))
            l.bind("<Leave>",    lambda e, _l=l: _l.config(fg=TEXT3))

        # Library search — use a simple flag to avoid placeholder triggering filter
        sb_f = tk.Frame(parent, bg=BG3, highlightbackground=BG4, highlightthickness=1)
        sb_f.pack(fill="x", padx=12, pady=(0,8))
        tk.Label(sb_f, text="⌕", font=("Helvetica Neue",11),
                 fg=TEXT3, bg=BG3, padx=8).pack(side="left")
        self._lib_ph = "Search library"
        self.lib_search_entry = tk.Entry(sb_f, bg=BG3, fg=TEXT3, insertbackground=TEXT,
                                         relief="flat", font=("Helvetica Neue",10), bd=6)
        self.lib_search_entry.pack(side="left", fill="both", expand=True)
        self.lib_search_entry.insert(0, self._lib_ph)
        self.lib_search_entry.bind("<FocusIn>",  self._lib_focus_in)
        self.lib_search_entry.bind("<FocusOut>", self._lib_focus_out)
        self.lib_search_entry.bind("<KeyRelease>", self._lib_key_released)

        # Scrollable canvas playlist panel — each playlist is a card with gradient art
        pl_outer = tk.Frame(parent, bg=SIDEBAR)
        pl_outer.pack(fill="both", expand=True)

        self._pl_scroll_canvas = tk.Canvas(pl_outer, bg=SIDEBAR,
                                            highlightthickness=0, bd=0)
        pl_sb = tk.Scrollbar(pl_outer, command=self._pl_scroll_canvas.yview,
                             bg=SIDEBAR, troughcolor=SIDEBAR,
                             highlightthickness=0, bd=0, width=3)
        self._pl_scroll_canvas.config(yscrollcommand=pl_sb.set)
        pl_sb.pack(side="right", fill="y")
        self._pl_scroll_canvas.pack(fill="both", expand=True)

        self._pl_inner = tk.Frame(self._pl_scroll_canvas, bg=SIDEBAR)
        self._pl_canvas_window = self._pl_scroll_canvas.create_window(
            0, 0, window=self._pl_inner, anchor="nw")

        def _update_sidebar_scroll(event=None):
            self._pl_inner.update_idletasks()
            self._pl_scroll_canvas.configure(scrollregion=self._pl_scroll_canvas.bbox("all"))
            self._pl_scroll_canvas.itemconfig(self._pl_canvas_window, width=self._pl_scroll_canvas.winfo_width())

        def _sidebar_wheel(event):
            if event.delta > 0:
                self._pl_scroll_canvas.yview_scroll(-2, "units")
            else:
                self._pl_scroll_canvas.yview_scroll(2, "units")

        self._pl_inner.bind("<Configure>", _update_sidebar_scroll)
        self._pl_scroll_canvas.bind("<Configure>", _update_sidebar_scroll)
        self._pl_scroll_canvas.bind("<Enter>", lambda e: self.bind_all("<MouseWheel>", _sidebar_wheel))
        self._pl_scroll_canvas.bind("<Leave>", lambda e: self.unbind_all("<MouseWheel>"))

    # ── Main ──────────────────────────────────────────────────────────────────
    def _build_main(self, parent):
        parent.grid_rowconfigure(0, weight=1)
        parent.grid_columnconfigure(0, weight=1)
        self._frames = {}
        for name in ("library","search","download","settings","recent","artists","sync"):
            f = tk.Frame(parent, bg=BG)
            f.grid(row=0, column=0, sticky="nsew")
            self._frames[name] = f
        self._build_library_view(self._frames["library"])
        self._build_search_view(self._frames["search"])
        self._build_download_view(self._frames["download"])
        self._build_settings_view(self._frames["settings"])
        self._build_recent_view(self._frames["recent"])
        self._build_artists_view(self._frames["artists"])
        self._build_sync_view(self._frames["sync"])
        self._show_tab("library")

    def _show_tab(self, name):
        for f in self._frames.values(): f.grid_remove()
        self._frames[name].grid()
        if name == "artists":
            self._populate_artists_view()
        elif name == "recent":
            self._redraw_recent()

    # ── Library view ─────────────────────────────────────────────────────────
    def _build_library_view(self, parent):
        parent.grid_rowconfigure(2, weight=1)
        parent.grid_columnconfigure(0, weight=1)
        parent.grid_columnconfigure(1, weight=0)

        # Gradient header
        self._hdr_canvas = tk.Canvas(parent, height=220, highlightthickness=0, bd=0, bg=BG)
        self._hdr_canvas.grid(row=0, column=0, columnspan=2, sticky="ew")

        self._hdr_overlay = tk.Frame(self._hdr_canvas, bg=BG)
        self._hdr_canvas.create_window(0, 0, window=self._hdr_overlay, anchor="nw", tags="overlay")
        self._hdr_canvas.bind("<Configure>",
            lambda e: (self._draw_lib_header(),
                       self._hdr_canvas.itemconfig("overlay", width=e.width, height=220)))

        self._art_container = tk.Frame(self._hdr_overlay, bg=BG, width=150, height=150)
        self._art_container.pack(side="left", padx=(32,22), pady=(35,0), anchor="n")
        self._art_container.pack_propagate(False)
        self._current_art = PlaylistArt(self._art_container, 150, "All Music", bg=BG3)
        self._current_art.pack(fill="both", expand=True)

        title_area = tk.Frame(self._hdr_overlay, bg=BG)
        title_area.pack(side="left", fill="both", expand=True, pady=(42,0))
        tk.Label(title_area, text="PLAYLIST", font=("Helvetica Neue",10,"bold"),
                 fg=TEXT2, bg=BG, anchor="w").pack(anchor="w")
        self.playlist_title_lbl = tk.Label(title_area, text="All Music",
                                            font=("Helvetica Neue",36,"bold"),
                                            fg=TEXT, bg=BG, anchor="w")
        self.playlist_title_lbl.pack(anchor="w", pady=(4,4))
        self.playlist_meta_lbl = tk.Label(title_area, text="",
                                           font=("Helvetica Neue",11),
                                           fg=TEXT2, bg=BG, anchor="w")
        self.playlist_meta_lbl.pack(anchor="w")

        # Action row
        act = tk.Frame(parent, bg=BG)
        act.grid(row=1, column=0, columnspan=2, sticky="ew", padx=28, pady=(14,6))

        play_lbl = tk.Label(act, text="▶", font=("Helvetica Neue",20,"bold"),
                            fg=SIDEBAR, bg=GREEN, padx=14, pady=6, cursor="hand2")
        play_lbl.pack(side="left", padx=(0,16))
        play_lbl.bind("<Button-1>", lambda e: self._play_all())
        play_lbl.bind("<Enter>", lambda e: play_lbl.config(bg=GREEN_HOVER))
        play_lbl.bind("<Leave>", lambda e: play_lbl.config(bg=GREEN))

        shuf = tk.Label(act, text="⇄  Shuffle", font=("Helvetica Neue",11),
                        fg=TEXT2, bg=BG, cursor="hand2")
        shuf.pack(side="left", padx=(0,20))
        shuf.bind("<Button-1>", lambda e: self._shuffle_all())
        shuf.bind("<Enter>", lambda e: shuf.config(fg=TEXT))
        shuf.bind("<Leave>", lambda e: shuf.config(fg=TEXT2))

        dupes_btn = tk.Label(act, text="⚠  Check Duplicates", font=("Helvetica Neue",10),
                             fg=TEXT3, bg=BG, cursor="hand2")
        dupes_btn.pack(side="right")
        dupes_btn.bind("<Button-1>", lambda e: self._show_duplicates())
        dupes_btn.bind("<Enter>", lambda e: dupes_btn.config(fg=WARNING))
        dupes_btn.bind("<Leave>", lambda e: dupes_btn.config(fg=TEXT3))

        tk.Frame(act, bg=BG3, height=1).pack(side="bottom", fill="x")

        # Custom canvas track list with album art
        track_outer = tk.Frame(parent, bg=BG)
        track_outer.grid(row=2, column=0, sticky="nsew", padx=(28,0), pady=(0,8))
        track_outer.grid_rowconfigure(0, weight=1)
        track_outer.grid_columnconfigure(0, weight=1)

        self._track_canvas = tk.Canvas(track_outer, bg=BG, highlightthickness=0, bd=0)
        tr_sb = tk.Scrollbar(track_outer, command=self._track_canvas.yview,
                             bg=BG, troughcolor=BG, highlightthickness=0, bd=0, width=6)
        self._track_canvas.config(yscrollcommand=tr_sb.set)
        tr_sb.grid(row=0, column=1, sticky="ns")
        self._track_canvas.grid(row=0, column=0, sticky="nsew")
        self._track_canvas.bind("<Configure>",   lambda e: self._redraw_track_list())
        def _lib_wheel(event):
            if event.delta > 0:
                self._track_canvas.yview_scroll(-2, "units")
            else:
                self._track_canvas.yview_scroll(2, "units")
            
        self._track_canvas.bind("<Enter>", lambda e: self.bind_all("<MouseWheel>", _lib_wheel))
        self._track_canvas.bind("<Leave>", lambda e: self.unbind_all("<MouseWheel>"))
        self._track_canvas.bind("<Button-1>",    self._on_track_canvas_click)
        self._track_canvas.bind("<Double-Button-1>", self._on_track_canvas_dbl)
        self._track_canvas.bind("<Button-2>",    self._on_track_canvas_right)
        self._track_canvas.bind("<Button-3>",    self._on_track_canvas_right)
        self._track_items   = []   # list of (path, title, artist, pl_name)
        self._track_sel_idx = []
        self._track_hover_idx = None
        self._track_canvas.bind("<Motion>", self._on_track_hover)
        self._track_canvas.bind("<Leave>",  lambda e: self._set_track_hover(None))
        # Keep ttk.Treeview as hidden stub so existing code refs don't break
        self.track_tree = ttk.Treeview(track_outer, style="Lib.Treeview",
                                        columns=("num","title","artist","playlist"),
                                        show="headings", selectmode="extended",
                                        height=0)
        self.track_tree.grid_remove()

        # Queue panel
        qp = tk.Frame(parent, bg=BG2, width=270)
        qp.grid(row=0, column=1, rowspan=3, sticky="nsew")
        qp.grid_propagate(False)
        self._build_queue_panel(qp)

        # Context menu
        self.ctx_menu = tk.Menu(self, tearoff=0, bg=BG3, fg=TEXT,
                                activebackground=BG4, activeforeground=TEXT,
                                font=("Helvetica Neue",12), bd=0, relief="flat")
        self.ctx_menu.add_command(label="  ▶   Play Now",          command=self._ctx_play_now)
        self.ctx_menu.add_command(label="  ↑   Play Next",         command=self._ctx_play_next)
        self.ctx_menu.add_command(label="  +   Add to Queue",      command=self._ctx_add_queue)
        self.ctx_menu.add_separator()
        self.ctx_menu.add_command(label="  ♫   Add to Playlist…",  command=self._ctx_add_playlist)
        self.ctx_menu.add_separator()
        self.ctx_menu.add_command(label="  ✕   Remove from Queue", command=self._ctx_remove_queue)
        self.ctx_menu.add_command(label="  🗑   Delete from Library",  command=self._ctx_remove_from_library)


    def _draw_lib_header(self):
        w = self._hdr_canvas.winfo_width()
        self._hdr_canvas.delete("grad")
        pl = self._selected_playlist or "All Music"
        c1, _ = playlist_gradient(pl)
        # Blend playlist color into BG
        r1,g1,b1 = hex_to_rgb(c1)
        r2,g2,b2 = hex_to_rgb(BG)
        # Overlay: dark tinted gradient
        steps = 40
        h = 220
        for i in range(steps):
            t  = i/steps
            r  = int(r1+(r2-r1)*t*1.2)
            g  = int(g1+(g2-g1)*t*1.2)
            b  = int(b1+(b2-b1)*t*1.2)
            r,g,b = max(0,min(255,r)),max(0,min(255,g)),max(0,min(255,b))
            y  = int(h*i/steps)
            yn = int(h*(i+1)/steps)
            self._hdr_canvas.create_rectangle(0, y, w, yn,
                                               fill=rgb_to_hex(r,g,b), outline="",
                                               tags="grad")

    def _build_queue_panel(self, parent):
        tk.Frame(parent, bg=BORDER, width=1).pack(side="left", fill="y")
        inner = tk.Frame(parent, bg=BG2)
        inner.pack(fill="both", expand=True)

        hdr = tk.Frame(inner, bg=BG2)
        hdr.pack(fill="x", padx=16, pady=(20,4))
        tk.Label(hdr, text="Queue", font=("Helvetica Neue",16,"bold"),
                 fg=TEXT, bg=BG2).pack(side="left")
        clr = tk.Label(hdr, text="Clear", font=("Helvetica Neue",10),
                       fg=TEXT3, bg=BG2, cursor="hand2")
        clr.pack(side="right")
        clr.bind("<Button-1>", lambda e: self._clear_queue())
        clr.bind("<Enter>",    lambda e: clr.config(fg=TEXT2))
        clr.bind("<Leave>",    lambda e: clr.config(fg=TEXT3))

        self.queue_ctx_lbl   = tk.Label(inner, text="", font=("Helvetica Neue",9),
                                         fg=GREEN, bg=BG2, anchor="w", padx=16)
        self.queue_ctx_lbl.pack(fill="x", pady=(0,2))
        self.queue_count_lbl = tk.Label(inner, text="", font=("Helvetica Neue",9),
                                         fg=TEXT3, bg=BG2, anchor="w", padx=16)
        self.queue_count_lbl.pack(fill="x", pady=(0,8))

        # Now playing mini art in queue panel
        self.queue_now_art = tk.Label(inner, text="♫",
                                       font=("Helvetica Neue",20), fg=GREEN, bg=BG2)
        self.queue_now_art.pack(pady=(0,4))
        self.queue_now_title = tk.Label(inner, text="",
                                         font=("Helvetica Neue",10,"bold"),
                                         fg=TEXT, bg=BG2, wraplength=230)
        self.queue_now_title.pack(padx=16)
        self.queue_now_artist = tk.Label(inner, text="",
                                          font=("Helvetica Neue",9),
                                          fg=TEXT3, bg=BG2)
        self.queue_now_artist.pack(padx=16, pady=(2,10))

        tk.Frame(inner, bg=BG3, height=1).pack(fill="x", padx=16, pady=(0,8))

        qf = tk.Frame(inner, bg=BG2)
        qf.pack(fill="both", expand=True)
        self.queue_tree = ttk.Treeview(qf, style="Queue.Treeview",
                                       columns=("name",), show="tree", selectmode="browse")
        self.queue_tree.column("#0",   width=0,   stretch=False)
        self.queue_tree.column("name", width=250, stretch=True)
        q_sb = tk.Scrollbar(qf, command=self.queue_tree.yview,
                            bg=BG2, troughcolor=BG2, highlightthickness=0, bd=0, width=4)
        self.queue_tree.config(yscrollcommand=q_sb.set)
        q_sb.pack(side="right", fill="y")
        self.queue_tree.pack(fill="both", expand=True)
        self.queue_tree.bind("<Double-Button-1>", self._on_queue_dbl)
        self.queue_tree.tag_configure("playing",  foreground=GREEN)
        self.queue_tree.tag_configure("normal",   foreground=TEXT2)
        self.queue_tree.tag_configure("upcoming", foreground=TEXT3)

    # ── Search view ───────────────────────────────────────────────────────────
    def _build_search_view(self, parent):
        parent.grid_rowconfigure(2, weight=1)
        parent.grid_columnconfigure(0, weight=1)
        hdr = tk.Canvas(parent, height=120, bg=BG, highlightthickness=0)
        hdr.grid(row=0, column=0, sticky="ew")
        hdr.bind("<Configure>",
            lambda e: draw_gradient_rect(hdr, 0, 0, e.width, 120, "#1a1a4a", BG))
        tk.Label(parent, text="Search", font=("Helvetica Neue",32,"bold"),
                 fg=TEXT, bg=BG, anchor="w").grid(row=0, column=0, sticky="sw", padx=28, pady=(0,16))
        sb = tk.Frame(parent, bg=BG3, highlightbackground=BG4, highlightthickness=1)
        sb.grid(row=1, column=0, sticky="ew", padx=28, pady=(0,16))
        tk.Label(sb, text="⌕", font=("Helvetica Neue",15),
                 fg=TEXT3, bg=BG3, padx=12).pack(side="left")
        self.search_entry = tk.Entry(sb, textvariable=self.search_var,
                                     bg=BG3, fg=TEXT, insertbackground=TEXT,
                                     relief="flat", font=("Helvetica Neue",14), bd=10)
        self.search_entry.pack(side="left", fill="both", expand=True)
        self.search_entry.bind("<Return>", lambda e: self._do_search())
        s_btn = tk.Label(sb, text="Search", font=("Helvetica Neue",10,"bold"),
                         fg=SIDEBAR, bg=GREEN, padx=18, pady=12, cursor="hand2")
        s_btn.pack(side="right")
        s_btn.bind("<Button-1>", lambda e: self._do_search())
        s_btn.bind("<Enter>", lambda e: s_btn.config(bg=GREEN_HOVER))
        s_btn.bind("<Leave>", lambda e: s_btn.config(bg=GREEN))
        rf = tk.Frame(parent, bg=BG)
        rf.grid(row=2, column=0, sticky="nsew", padx=28, pady=(0,8))
        rf.grid_rowconfigure(0, weight=1); rf.grid_columnconfigure(0, weight=1)
        self.results_tree = ttk.Treeview(rf, style="Results.Treeview",
                                          columns=("source","title","artist","duration"),
                                          show="headings")
        self.results_tree.heading("source",   text="SOURCE")
        self.results_tree.heading("title",    text="TITLE")
        self.results_tree.heading("artist",   text="CHANNEL / ARTIST")
        self.results_tree.heading("duration", text="TIME")
        self.results_tree.column("source",   width=100, stretch=False)
        self.results_tree.column("title",    width=340, stretch=True)
        self.results_tree.column("artist",   width=220, stretch=True)
        self.results_tree.column("duration", width=70,  stretch=False, anchor="e")
        r_sb = tk.Scrollbar(rf, command=self.results_tree.yview,
                            bg=BG, troughcolor=BG, highlightthickness=0, bd=0, width=6)
        self.results_tree.config(yscrollcommand=r_sb.set)
        r_sb.grid(row=0, column=1, sticky="ns")
        self.results_tree.grid(row=0, column=0, sticky="nsew")
        self.results_tree.tag_configure("yt", foreground=TEXT2)
        self.results_tree.tag_configure("sc", foreground=SC_COLOR)
        act = tk.Frame(parent, bg=BG)
        act.grid(row=3, column=0, sticky="ew", padx=28, pady=(8,16))
        dl = tk.Label(act, text="⬇  Download Selected",
                      font=("Helvetica Neue",10,"bold"),
                      fg=SIDEBAR, bg=GREEN, padx=16, pady=8, cursor="hand2")
        dl.pack(side="left")
        dl.bind("<Button-1>", lambda e: self._download_result())
        dl.bind("<Enter>", lambda e: dl.config(bg=GREEN_HOVER))
        dl.bind("<Leave>", lambda e: dl.config(bg=GREEN))
        more = tk.Label(act, text="Load More", font=("Helvetica Neue",10),
                        fg=TEXT2, bg=BG3, padx=14, pady=8, cursor="hand2")
        more.pack(side="left", padx=(10,0))
        more.bind("<Button-1>", lambda e: self._do_search(append=True))
        more.bind("<Enter>", lambda e: more.config(fg=TEXT))
        more.bind("<Leave>", lambda e: more.config(fg=TEXT2))
        self.search_status_lbl = tk.Label(act, text="",
                                           font=("Helvetica Neue",10), fg=TEXT3, bg=BG)
        self.search_status_lbl.pack(side="right")

    # ── Download view ─────────────────────────────────────────────────────────
    def _build_download_view(self, parent):
        parent.grid_rowconfigure(4, weight=1)
        parent.grid_columnconfigure(0, weight=1)
        hdr = tk.Canvas(parent, height=120, bg=BG, highlightthickness=0)
        hdr.grid(row=0, column=0, sticky="ew")
        hdr.bind("<Configure>",
            lambda e: draw_gradient_rect(hdr, 0, 0, e.width, 120, "#1a2a1a", BG))
        tk.Label(parent, text="Download", font=("Helvetica Neue",32,"bold"),
                 fg=TEXT, bg=BG, anchor="w").grid(row=0, column=0, sticky="sw", padx=28, pady=(0,16))
        nb = ttk.Notebook(parent, style="Dark.TNotebook")
        nb.grid(row=1, column=0, sticky="ew", padx=28)
        self._tab_yt  = tk.Frame(nb, bg=BG2)
        self._tab_csv = tk.Frame(nb, bg=BG2)
        nb.add(self._tab_yt,  text="  YouTube Playlist  ")
        nb.add(self._tab_csv, text="  Spotify CSV  ")
        self._build_yt_tab(); self._build_csv_tab()
        dest_f = tk.Frame(parent, bg=BG)
        dest_f.grid(row=2, column=0, sticky="ew", padx=28, pady=(12,4))
        tk.Label(dest_f, text="SAVE TO", font=("Helvetica Neue",9,"bold"),
                 fg=TEXT3, bg=BG).pack(side="left", padx=(0,12))
        self.dest_combo_var = tk.StringVar(value="— choose playlist —")
        self._dest_options  = []
        self.dest_combo = ttk.Combobox(dest_f, textvariable=self.dest_combo_var,
                                        font=("Helvetica Neue",10), state="readonly", width=28)
        self.dest_combo.pack(side="left", padx=(0,8))
        self.dest_combo.bind("<<ComboboxSelected>>", self._on_dest_selected)
        np = tk.Label(dest_f, text="+ New Playlist", font=("Helvetica Neue",10),
                      fg=GREEN, bg=BG, cursor="hand2")
        np.pack(side="left", padx=(4,0))
        np.bind("<Button-1>", lambda e: self._create_playlist_dialog(set_dest=True))
        np.bind("<Enter>", lambda e: np.config(fg=GREEN_HOVER))
        np.bind("<Leave>", lambda e: np.config(fg=GREEN))
        self.dest_path_lbl = tk.Label(dest_f, textvariable=self.dl_dest_var,
                                       font=("Helvetica Neue",9), fg=TEXT3, bg=BG)
        self.dest_path_lbl.pack(side="left", padx=(12,0))
        opts = tk.Frame(parent, bg=BG)
        opts.grid(row=3, column=0, sticky="ew", padx=28, pady=(8,8))
        for var, lbl in [(self.meta_var,"Embed metadata"),
                         (self.skip_var,"Skip existing"),(self.sc_var,"SoundCloud first")]:
            tk.Checkbutton(opts, text=lbl, variable=var, font=("Helvetica Neue",10),
                           fg=TEXT2, bg=BG, selectcolor=BG3, activebackground=BG,
                           activeforeground=TEXT, cursor="hand2",
                           highlightthickness=0, bd=0).pack(side="left", padx=(0,16))
        ctrl = tk.Frame(parent, bg=BG)
        ctrl.grid(row=4, column=0, sticky="nsew", padx=28, pady=(0,16))
        ctrl.grid_rowconfigure(2, weight=1); ctrl.grid_columnconfigure(0, weight=1)
        self.dl_btn = tk.Label(ctrl, text="⬇  Download All as MP3",
                               font=("Helvetica Neue",11,"bold"),
                               fg=SIDEBAR, bg=GREEN, padx=24, pady=12,
                               cursor="hand2", anchor="center")
        self.dl_btn.grid(row=0, column=0, sticky="ew", pady=(0,12))
        self.dl_btn.bind("<Button-1>", lambda e: self._toggle_dl())
        self.dl_btn.bind("<Enter>", lambda e: self.dl_btn.config(bg=GREEN_HOVER) if not self._is_dl else None)
        self.dl_btn.bind("<Leave>", lambda e: self.dl_btn.config(bg=GREEN) if not self._is_dl else None)
        pr = tk.Frame(ctrl, bg=BG); pr.grid(row=1, column=0, sticky="ew", pady=(0,8))
        ttk.Progressbar(pr, variable=self.prog_var, maximum=100,
                        style="Green.Horizontal.TProgressbar").pack(fill="x")
        sr = tk.Frame(pr, bg=BG); sr.pack(fill="x", pady=(4,0))
        tk.Label(sr, textvariable=self.status_var, font=("Helvetica Neue",9),
                 fg=TEXT3, bg=BG, anchor="w").pack(side="left")
        self.dl_count_lbl = tk.Label(sr, text="", font=("Helvetica Neue",9),
                                      fg=TEXT3, bg=BG, anchor="e")
        self.dl_count_lbl.pack(side="right")
        log_f = tk.Frame(ctrl, bg=BG2, highlightbackground=BORDER, highlightthickness=1)
        log_f.grid(row=2, column=0, sticky="nsew")
        self.log_text = tk.Text(log_f, bg="#000000", fg="#FFFFFF", font=("Courier", 10),
                                relief="flat", state="disabled", bd=10,
                                wrap="word")
        ls = tk.Scrollbar(log_f, command=self.log_text.yview,
                          bg=BG2, troughcolor=BG2, highlightthickness=0, bd=0, width=4)
        self.log_text.config(yscrollcommand=ls.set)
        ls.pack(side="right", fill="y"); self.log_text.pack(fill="both", expand=True)
        for tag, col in [("success",SUCCESS),("warning",WARNING),("error",RED),
                         ("sc",SC_COLOR),("yt",TEXT2),("normal",TEXT2),("playlist",BLUE)]:
            self.log_text.tag_config(tag, foreground=col)

    def _build_yt_tab(self):
        f = self._tab_yt
        tk.Label(f, text="YouTube Playlist URL", font=("Helvetica Neue",10,"bold"),
                 fg=TEXT3, bg=BG2).pack(anchor="w", padx=16, pady=(14,4))
        row = tk.Frame(f, bg=BG3, highlightbackground=BG4, highlightthickness=1)
        row.pack(fill="x", padx=16, pady=(0,8))
        self._yt_ph = "https://www.youtube.com/playlist?list=..."
        self.yt_entry = tk.Entry(row, textvariable=self.yt_url_var, bg=BG3, fg=TEXT,
                                 insertbackground=TEXT, relief="flat",
                                 font=("Helvetica Neue",10), bd=10)
        self.yt_entry.pack(side="left", fill="both", expand=True)
        self.yt_entry.insert(0, self._yt_ph); self.yt_entry.config(fg=TEXT3)
        self.yt_entry.bind("<FocusIn>",  lambda e: self._yt_fi())
        self.yt_entry.bind("<FocusOut>", lambda e: self._yt_fo())
        prev = tk.Label(row, text="Preview", font=("Helvetica Neue",9,"bold"),
                        fg=TEXT2, bg=BG4, padx=14, pady=10, cursor="hand2")
        prev.pack(side="right"); prev.bind("<Button-1>", lambda e: self._fetch_yt())
        self.yt_info_lbl = tk.Label(f, text="", font=("Helvetica Neue",9),
                                    fg=TEXT3, bg=BG2, anchor="w", padx=16)
        self.yt_info_lbl.pack(fill="x", pady=(0,12))

    def _build_csv_tab(self):
        f = self._tab_csv
        for s in ["1.  exportify.net → log in → Export playlist as CSV",
                  "2.  Load CSVs below — each saves to its own playlist folder",
                  "3.  Or choose a destination above to override"]:
            tk.Label(f, text=s, font=("Helvetica Neue",9), fg=TEXT3, bg=BG2,
                     anchor="w", padx=16, pady=2).pack(fill="x")
        br = tk.Frame(f, bg=BG2); br.pack(fill="x", padx=16, pady=(10,4))
        add = tk.Label(br, text="+ Add CSV Files", font=("Helvetica Neue",10,"bold"),
                       fg=SIDEBAR, bg=GREEN, padx=14, pady=6, cursor="hand2")
        add.pack(side="left")
        add.bind("<Button-1>", lambda e: self._add_csvs())
        add.bind("<Enter>", lambda e: add.config(bg=GREEN_HOVER))
        add.bind("<Leave>", lambda e: add.config(bg=GREEN))
        clr = tk.Label(br, text="Clear", font=("Helvetica Neue",10),
                       fg=TEXT3, bg=BG2, padx=10, cursor="hand2")
        clr.pack(side="left", padx=(8,0)); clr.bind("<Button-1>", lambda e: self._clear_csvs())
        self.csv_count_lbl = tk.Label(br, text="No playlists loaded",
                                       font=("Helvetica Neue",9), fg=TEXT3, bg=BG2)
        self.csv_count_lbl.pack(side="right")
        lf = tk.Frame(f, bg=BG3); lf.pack(fill="both", expand=True, padx=16, pady=(0,12))
        self.csv_listbox = tk.Listbox(lf, bg=BG3, fg=TEXT2, font=("Helvetica Neue",10),
                                       relief="flat", selectbackground=BG4,
                                       selectforeground=TEXT, highlightthickness=0,
                                       bd=8, activestyle="none")
        sb = tk.Scrollbar(lf, command=self.csv_listbox.yview,
                          bg=BG3, troughcolor=BG3, highlightthickness=0, bd=0, width=4)
        self.csv_listbox.config(yscrollcommand=sb.set)
        sb.pack(side="right", fill="y"); self.csv_listbox.pack(fill="both", expand=True)

    # ── Settings view ─────────────────────────────────────────────────────────
    def _build_settings_view(self, parent):
        parent.grid_rowconfigure(1, weight=1)
        parent.grid_columnconfigure(0, weight=1)

        # Fixed header
        hdr = tk.Canvas(parent, height=120, bg=BG, highlightthickness=0)
        hdr.grid(row=0, column=0, sticky="ew")
        hdr.bind("<Configure>",
            lambda e: draw_gradient_rect(hdr, 0, 0, e.width, 120, "#2a1a1a", BG))
        tk.Label(parent, text="Settings", font=("Helvetica Neue",32,"bold"),
                 fg=TEXT, bg=BG, anchor="w").grid(row=0, column=0, sticky="sw", padx=28, pady=(0,16))

        # Scrollable body
        scroll_outer = tk.Frame(parent, bg=BG)
        scroll_outer.grid(row=1, column=0, sticky="nsew")
        scroll_outer.grid_rowconfigure(0, weight=1)
        scroll_outer.grid_columnconfigure(0, weight=1)

        s_canvas = tk.Canvas(scroll_outer, bg=BG, highlightthickness=0, bd=0)
        s_sb = tk.Scrollbar(scroll_outer, command=s_canvas.yview,
                            bg=BG, troughcolor=BG, highlightthickness=0, bd=0, width=6)
        s_canvas.config(yscrollcommand=s_sb.set)
        s_sb.grid(row=0, column=1, sticky="ns")
        s_canvas.grid(row=0, column=0, sticky="nsew")

        s_inner = tk.Frame(s_canvas, bg=BG)
        s_win = s_canvas.create_window((0, 0), window=s_inner, anchor="nw")

        # FIX: Robust scrollregion update
        def _update_settings_scroll(event=None):
            s_inner.update_idletasks()
            s_canvas.configure(scrollregion=s_canvas.bbox("all"))
            if event and hasattr(event, 'width'):
                s_canvas.itemconfig(s_win, width=event.width)

        s_canvas.bind("<Configure>", _update_settings_scroll)
        s_inner.bind("<Configure>", _update_settings_scroll)

      # 1. Define the scroll function
        def _scroll_settings(event):
            s_canvas.yview_scroll(int(-1*(event.delta)), "units")
        
        s_canvas.bind("<Enter>", lambda e: s_canvas.bind_all("<MouseWheel>", _scroll_settings))
        s_canvas.bind("<Leave>", lambda e: s_canvas.unbind_all("<MouseWheel>"))
        
        parent = s_inner  # redirect packing to scrollable frame

        # Master Storage UI
        mlf = tk.Frame(parent, bg=BG)
        mlf.pack(fill="x", padx=28, pady=(20,8))
        tk.Label(mlf, text="MASTER MUSIC STORAGE", font=("Helvetica Neue",10,"bold"), fg=TEXT3, bg=BG).pack(anchor="w", pady=(0,8))
        
        path_row = tk.Frame(mlf, bg=BG2, highlightbackground=BORDER, highlightthickness=1)
        path_row.pack(fill="x", pady=(0,8))
        
        self.master_path_lbl = tk.Label(path_row, text=MUSIC_ROOT, font=("Menlo", 10), fg=TEXT2, bg=BG2, anchor="w", padx=12, pady=10)
        self.master_path_lbl.pack(side="left", fill="x", expand=True)
        
        change_btn = tk.Label(path_row, text="Change Folder", font=("Helvetica Neue", 10, "bold"), fg=SIDEBAR, bg=GREEN, padx=15, pady=10, cursor="hand2")
        change_btn.pack(side="right")
        change_btn.bind("<Button-1>", lambda e: self._change_master_library())
        
        tk.Frame(parent, bg=BG3, height=1).pack(fill="x", padx=28, pady=(16,12))

        tk.Frame(parent, bg=BG3, height=1).pack(fill="x", padx=28, pady=(16,12))

        info = tk.Frame(parent, bg=BG)
        info.pack(fill="x", padx=28, pady=(0,8))
        tk.Label(info, text="AUTO-REFRESH", font=("Helvetica Neue",10,"bold"),
                 fg=TEXT3, bg=BG).pack(anchor="w", pady=(0,6))
        tk.Label(info, text="Library refreshes every 30 seconds automatically.",
                 font=("Helvetica Neue",10), fg=TEXT2, bg=BG).pack(anchor="w")
        self.last_refresh_lbl = tk.Label(info, text="", font=("Helvetica Neue",9),
                                          fg=TEXT3, bg=BG, anchor="w")
        self.last_refresh_lbl.pack(anchor="w", pady=(6,0))

        tk.Frame(parent, bg=BG3, height=1).pack(fill="x", padx=28, pady=(16,12))

        # ── Sleep Timer ───────────────────────────────────────────────────────
        sleep_sec = tk.Frame(parent, bg=BG)
        sleep_sec.pack(fill="x", padx=28, pady=(0,8))
        tk.Label(sleep_sec, text="SLEEP TIMER", font=("Helvetica Neue",10,"bold"),
                 fg=TEXT3, bg=BG).pack(anchor="w", pady=(0,6))
        tk.Label(sleep_sec, text="Auto-pause playback after a set time.",
                 font=("Helvetica Neue",10), fg=TEXT2, bg=BG).pack(anchor="w", pady=(0,10))
        sleep_row = tk.Frame(sleep_sec, bg=BG)
        sleep_row.pack(anchor="w")
        self._sleep_var = tk.StringVar(value="Off")
        for opt in ["Off","15 min","30 min","45 min","60 min","90 min"]:
            rb = tk.Radiobutton(sleep_row, text=opt, variable=self._sleep_var, value=opt,
                                font=("Helvetica Neue",10), fg=TEXT2, bg=BG,
                                selectcolor=BG3, activebackground=BG, activeforeground=TEXT,
                                cursor="hand2", highlightthickness=0,
                                command=self._set_sleep_timer)
            rb.pack(side="left", padx=(0,16))
        self._sleep_lbl = tk.Label(sleep_sec, text="", font=("Helvetica Neue",9),
                                    fg=GREEN, bg=BG, anchor="w")
        self._sleep_lbl.pack(anchor="w", pady=(6,0))

        tk.Frame(parent, bg=BG3, height=1).pack(fill="x", padx=28, pady=(16,12))

        # ── Crossfade ─────────────────────────────────────────────────────────
        cf_sec = tk.Frame(parent, bg=BG)
        cf_sec.pack(fill="x", padx=28, pady=(0,8))
        tk.Label(cf_sec, text="CROSSFADE", font=("Helvetica Neue",10,"bold"),
                 fg=TEXT3, bg=BG).pack(anchor="w", pady=(0,6))
        tk.Label(cf_sec, text="Fade out current track and fade in the next. Uses volume automation.",
                 font=("Helvetica Neue",10), fg=TEXT2, bg=BG).pack(anchor="w", pady=(0,10))
        cf_row = tk.Frame(cf_sec, bg=BG)
        cf_row.pack(anchor="w")
        self._cf_var = tk.StringVar(value="Off")
        for opt in ["Off","2s","4s","6s","8s"]:
            rb = tk.Radiobutton(cf_row, text=opt, variable=self._cf_var, value=opt,
                                font=("Helvetica Neue",10), fg=TEXT2, bg=BG,
                                selectcolor=BG3, activebackground=BG, activeforeground=TEXT,
                                cursor="hand2", highlightthickness=0,
                                command=self._set_crossfade)
            rb.pack(side="left", padx=(0,16))

        tk.Frame(parent, bg=BG3, height=1).pack(fill="x", padx=28, pady=(16,12))

        # ── Visualizer ────────────────────────────────────────────────────────
        viz_section = tk.Frame(parent, bg=BG)
        viz_section.pack(fill="x", padx=28, pady=(0,8))
        viz_hdr = tk.Frame(viz_section, bg=BG)
        viz_hdr.pack(fill="x")
        tk.Label(viz_hdr, text="AUDIO VISUALIZER", font=("Helvetica Neue",10,"bold"),
                 fg=TEXT3, bg=BG).pack(side="left", pady=(0,8))
        self._viz_toggle_btn = tk.Label(viz_hdr, text="Enable",
                                         font=("Helvetica Neue",10,"bold"),
                                         fg=SIDEBAR, bg=GREEN, padx=12, pady=4, cursor="hand2")
        self._viz_toggle_btn.pack(side="right")
        self._viz_toggle_btn.bind("<Button-1>", lambda e: self._toggle_visualizer())
        self._viz_toggle_btn.bind("<Enter>", lambda e: self._viz_toggle_btn.config(bg=GREEN_HOVER))
        self._viz_toggle_btn.bind("<Leave>", lambda e: self._viz_toggle_btn.config(bg=GREEN))
        self._viz_canvas = tk.Canvas(viz_section, bg=BG2, height=80, highlightthickness=0, bd=0)
        self._viz_canvas.pack(fill="x", pady=(6,0))

        tk.Frame(parent, bg=BG3, height=1).pack(fill="x", padx=28, pady=(16,12))

        # ── Smart Playlists ───────────────────────────────────────────────────
        sp_sec = tk.Frame(parent, bg=BG)
        sp_sec.pack(fill="x", padx=28, pady=(0,8))
        tk.Label(sp_sec, text="SMART PLAYLISTS", font=("Helvetica Neue",10,"bold"),
                 fg=TEXT3, bg=BG).pack(anchor="w", pady=(0,6))
        tk.Label(sp_sec, text="Auto-generate playlists based on your listening habits.",
                 font=("Helvetica Neue",10), fg=TEXT2, bg=BG).pack(anchor="w", pady=(0,10))
        sp_row = tk.Frame(sp_sec, bg=BG)
        sp_row.pack(anchor="w")
        for lbl, cmd in [("Most Played", lambda: self._generate_smart_playlist("most_played")),
                         ("Recently Added", lambda: self._generate_smart_playlist("recent")),
                         ("By Artist", lambda: self._generate_smart_playlist("artist"))]:
            b = tk.Label(sp_row, text=lbl, font=("Helvetica Neue",10,"bold"),
                         fg=SIDEBAR, bg=GREEN, padx=12, pady=6, cursor="hand2")
            b.pack(side="left", padx=(0,8))
            b.bind("<Button-1>", lambda e, c=cmd: c())
            b.bind("<Enter>", lambda e, _b=b: _b.config(bg=GREEN_HOVER))
            b.bind("<Leave>", lambda e, _b=b: _b.config(bg=GREEN))

        tk.Frame(parent, bg=BG3, height=1).pack(fill="x", padx=28, pady=(16,12))

        # ── Export Playlist ───────────────────────────────────────────────────
        exp_section = tk.Frame(parent, bg=BG)
        exp_section.pack(fill="x", padx=28, pady=(0,8))
        tk.Label(exp_section, text="EXPORT PLAYLIST", font=("Helvetica Neue",10,"bold"),
                 fg=TEXT3, bg=BG).pack(anchor="w", pady=(0,8))
        tk.Label(exp_section, text="Export queue or playlist as M3U for VLC, Foobar2000, etc.",
                 font=("Helvetica Neue",10), fg=TEXT2, bg=BG).pack(anchor="w", pady=(0,10))
        exp_btn = tk.Label(exp_section, text="⬆  Export as M3U",
                           font=("Helvetica Neue",10,"bold"),
                           fg=SIDEBAR, bg=GREEN, padx=14, pady=8, cursor="hand2")
        exp_btn.pack(anchor="w")
        exp_btn.bind("<Button-1>", lambda e: self._export_playlist_m3u())
        exp_btn.bind("<Enter>", lambda e: exp_btn.config(bg=GREEN_HOVER))
        exp_btn.bind("<Leave>", lambda e: exp_btn.config(bg=GREEN))

        tk.Frame(parent, bg=BG3, height=1).pack(fill="x", padx=28, pady=(16,12))

        # ── Keyboard Shortcuts ────────────────────────────────────────────────
        keys_section = tk.Frame(parent, bg=BG)
        keys_section.pack(fill="x", padx=28, pady=(0,24))
        tk.Label(keys_section, text="KEYBOARD SHORTCUTS", font=("Helvetica Neue",10,"bold"),
                 fg=TEXT3, bg=BG).pack(anchor="w", pady=(0,8))
        shortcuts = [
            ("Media Play","Play / Pause"), 
            ("Media Next","Next track"),
            ("Media Prev","Previous track"),
            ("↑  ↓","Volume up / down"),
        ]
        for keys_lbl, desc in shortcuts:
            row_f = tk.Frame(keys_section, bg=BG)
            row_f.pack(fill="x", pady=2)
            tk.Label(row_f, text=keys_lbl, font=("Menlo",10,"bold"),
                     fg=TEXT, bg=BG3, padx=10, pady=4, width=10,
                     anchor="center").pack(side="left", padx=(0,12))
            tk.Label(row_f, text=desc, font=("Helvetica Neue",10),
                     fg=TEXT2, bg=BG, anchor="w").pack(side="left")
            tk.Frame(parent, bg=BG, height=100).pack(fill="x")

    # ── Player bar ────────────────────────────────────────────────────────────
    def _build_player(self, parent):
        parent.grid_columnconfigure(1, weight=1)

        # Track info + mini art
        info = tk.Frame(parent, bg=PLAYER_BG, width=300)
        info.grid(row=0, column=0, sticky="nsew", padx=(20,0))
        info.grid_propagate(False)
        info.grid_rowconfigure(0, weight=1)
        info.grid_columnconfigure(1, weight=1)

        self.player_art_lbl = tk.Label(info, text="♫",
                                        font=("Helvetica Neue",20),
                                        fg=TEXT3, bg=BG3,
                                        width=4, anchor="center")
        self.player_art_lbl.grid(row=0, column=0, padx=(0,12), pady=16, sticky="ns")

        txt_col = tk.Frame(info, bg=PLAYER_BG)
        txt_col.grid(row=0, column=1, sticky="nsew", pady=20)
        self.now_title = tk.Label(txt_col, text="Nothing playing",
                                   font=("Helvetica Neue",12,"bold"),
                                   fg=TEXT, bg=PLAYER_BG, anchor="w", wraplength=220)
        self.now_title.pack(anchor="w")
        self.now_artist = tk.Label(txt_col, text="",
                                    font=("Helvetica Neue",10),
                                    fg=TEXT3, bg=PLAYER_BG, anchor="w")
        self.now_artist.pack(anchor="w", pady=(2,0))

        # Controls
        ctrl = tk.Frame(parent, bg=PLAYER_BG)
        ctrl.grid(row=0, column=1, sticky="nsew", padx=10)

        btn_row = tk.Frame(ctrl, bg=PLAYER_BG)
        btn_row.pack(pady=(8,4))

        def icon_lbl(text, cmd, size=13, fg=TEXT3):
            l = tk.Label(btn_row, text=text, font=("Helvetica Neue",size),
                         fg=fg, bg=PLAYER_BG, cursor="hand2", padx=8)
            l.bind("<Button-1>", lambda e: cmd())
            l.bind("<Enter>",    lambda e: l.config(fg=TEXT))
            l.bind("<Leave>", lambda e: l.config(fg=GREEN if (l == self.shuf_lbl and self._shuffle) or (l == self.rep_lbl and self._repeat) else fg))
            return l

        self.shuf_lbl = icon_lbl("⇄", self._toggle_shuffle)
        self.shuf_lbl.pack(side="left")
        icon_lbl("⏮", self._prev_track, 17, TEXT2).pack(side="left")

        # Round play button
        self.play_btn = RoundButton(btn_row, 36, self._toggle_play, bg=PLAYER_BG)
        self.play_btn.pack(side="left", padx=8)

        icon_lbl("⏭", self._next_track, 17, TEXT2).pack(side="left")
        self.rep_lbl = icon_lbl("↺", self._toggle_repeat)
        self.rep_lbl.pack(side="left")

        # Slim scrubber
        sr = tk.Frame(ctrl, bg=PLAYER_BG)
        sr.pack(fill="x", padx=16, pady=(0,4))
        self.time_cur = tk.Label(sr, text="0:00", font=("Menlo",9),
                                  fg=TEXT3, bg=PLAYER_BG, width=4)
        self.time_cur.pack(side="left")
        self.scrub = SlimSlider(sr, from_=0, to=100,
                                command=self._on_scrub_moved,
                                track_color=BG4, fill_color=TEXT3,
                                thumb_color=TEXT, height=20, bg=PLAYER_BG)
        self.scrub.pack(side="left", fill="x", expand=True, padx=8)
        self.time_tot = tk.Label(sr, text="0:00", font=("Menlo",9),
                                  fg=TEXT3, bg=PLAYER_BG, width=4)
        self.time_tot.pack(side="left")

        # Volume
        vol_frame = tk.Frame(parent, bg=PLAYER_BG, width=220)
        vol_frame.grid(row=0, column=2, sticky="e", padx=(0,20))
        vol_frame.grid_propagate(False)
        vi = tk.Frame(vol_frame, bg=PLAYER_BG)
        vi.pack(side="right", pady=26)
        tk.Label(vi, text="🔊", font=("Helvetica Neue",11),
                 fg=TEXT3, bg=PLAYER_BG).pack(side="left", padx=(0,6))
        self.vol_slider = SlimSlider(vi, from_=0, to=100,
                                     command=self._on_volume_moved,
                                     track_color=BG4, fill_color=TEXT3,
                                     thumb_color=TEXT, height=20, bg=PLAYER_BG)
        self.vol_slider.set(70)
        self.vol_slider.pack(side="left")
        self.vol_slider.configure(width=120)

    # ═══════════════════════════════════════════════════════════════════════════
    # LIBRARY
    # ═══════════════════════════════════════════════════════════════════════════
    

    

    def _refresh_library(self):
        """Manual refresh triggered by the user or startup."""
        threading.Thread(target=self._run_library_scan_bg, daemon=True).start()

    def _run_library_scan_bg(self):
        """The heavy lifting happens here on a background thread."""
        new_lib = scan_library(self._watch_roots)
        self.after(0, lambda: self._finalize_library_update(new_lib))

    def _finalize_library_update(self, new_lib):

        self._library = new_lib
        
        # 1. Redraw the sidebar cards (updates song counts)
        self._populate_sidebar()
        
        # 2. Get the current search query so we don't lose our place
        search_q = self.lib_search_entry.get() if self.lib_search_entry.get() != self._lib_ph else ""
        
        # 3. Redraw the track list
        self._populate_track_list(search_q)
        
        # 4. Update the download destination dropdown
        self._update_dest_combo()
        
        # 5. Update timestamp in Settings
        self.last_refresh_lbl.config(text=f"Last refreshed: {time.strftime('%H:%M:%S')}")

    def _populate_sidebar(self):
        """Redraws sidebar cards with right-click menu restored."""
        for w in self._pl_inner.winfo_children():
            w.destroy()
        self._pl_card_frames = []

        

        # Organize playlist order
        total_songs = sum(len(v) for v in self._library.values())
        entries = [("All Music", total_songs)]
        if "Unsorted" in self._library: entries.append(("Unsorted", len(self._library["Unsorted"])))
        for name in sorted(self._library.keys()):
            if name not in ["Unsorted", "All Music"]:
                entries.append((name, len(self._library[name])))

        for idx, (name, count) in enumerate(entries):
            card = tk.Frame(self._pl_inner, bg=SIDEBAR, cursor="hand2")
            card.pack(fill="x", pady=1)
            inner = tk.Frame(card, bg=SIDEBAR, cursor="hand2")
            inner.pack(fill="x", padx=8, pady=3)
            art = PlaylistArt(inner, 42, name, bg=SIDEBAR)
            art.pack(side="left", padx=(0, 10))
            txt_f = tk.Frame(inner, bg=SIDEBAR)
            txt_f.pack(side="left", fill="both", expand=True)
            nl = tk.Label(txt_f, text=name, font=("Helvetica Neue", 11, "bold"), fg=TEXT, bg=SIDEBAR, anchor="w")
            nl.pack(anchor="w")
            cl = tk.Label(txt_f, text=f"{count} songs", font=("Helvetica Neue", 9), fg=TEXT3, bg=SIDEBAR, anchor="w")
            cl.pack(anchor="w")

            def show_context(e, pl_name=name):
                m = tk.Menu(self, tearoff=0, bg=BG3, fg=TEXT, font=("Helvetica Neue", 12))
                if pl_name != "All Music":
                    m.add_command(label=f"Dump '{pl_name}' elsewhere...", command=lambda: self._ctx_dump_playlist(pl_name))
                    m.add_separator()
                    m.add_command(label=f"🧹 Empty Playlist", command=lambda: self._ctx_clear_playlist_contents(pl_name))
                    if pl_name != "Unsorted":
                        m.add_command(label=f"🗑 Delete Folder", command=lambda: self._ctx_delete_playlist_folder(pl_name))
                m.post(e.x_root, e.y_root)

            for w in [card, inner, art, nl, cl, txt_f]:
                w.bind("<Button-1>", lambda e, n=name, i=idx: self._select_playlist_card(i, n))
                w.bind("<Button-2>", show_context)
                w.bind("<Button-3>", show_context)
                w.bind("<Enter>", lambda e, c=card, inn=inner, n=nl, cll=cl, tx=txt_f: [x.config(bg=BG3) for x in [c,inn,n,cll,tx]])
                w.bind("<Leave>", lambda e, c=card, inn=inner, n=nl, cll=cl, tx=txt_f: [x.config(bg=SIDEBAR) for x in [c,inn,n,cll,tx]])
                # WE REMOVED THE DIRECT MOUSEWHEEL BIND HERE TO PREVENT MULTI-SCROLL BUGS

    def _select_playlist_card(self, idx, name):
        self._pl_selected_idx = idx
        # No persistent highlight — just act on the selection
        if name == "All Music":
            self._selected_playlist = None
            total = sum(len(v) for v in self._library.values())
            self.playlist_meta_lbl.config(text=f"{total} songs")
            self.playlist_title_lbl.config(text="All Music")
            self._current_art.update_name("All Music")
        else:
            self._selected_playlist = name
            cnt = len(self._library.get(name, []))
            self.playlist_meta_lbl.config(text=f"{cnt} songs")
            self.playlist_title_lbl.config(text=name)
            self._current_art.update_name(name)
        self._draw_lib_header()
        self._populate_track_list()
        self._show_tab("library")

    def _on_playlist_select(self, event):
        pass  # replaced by _select_playlist_card

    def _populate_track_list(self, filter_str=""):
        playlists = (
            {self._selected_playlist: self._library[self._selected_playlist]}
            if self._selected_playlist and self._selected_playlist in self._library
            else self._library
        )
        f = filter_str.strip().lower()
        self._track_items = []
        for pl_name, tracks in playlists.items():
            for path in tracks:
                title, artist = split_track_name(path)
                if f and f not in title.lower() and f not in artist.lower():
                    continue
                self._track_items.append((path, title, artist, pl_name))
        self._track_sel_idx = []
        self._redraw_track_list()

    def _redraw_track_list(self):
        """Draws all songs in the list so they never disappear or turn black."""
        c = self._track_canvas
        c.delete("all")
        w = c.winfo_width()
        if w < 10: return
        
        rh = 52 # Height of one row
        total_items = len(self._track_items)
        
        # This tells the window exactly how tall the total list is
        c.configure(scrollregion=(0, 0, w, max(total_items * rh, 100)))

        cur_path = self._queue[self._queue_idx] if 0 <= self._queue_idx < len(self._queue) else ""

        for i in range(total_items):
            path, title, artist, pl_name = self._track_items[i]
            y0, y1 = i * rh, (i + 1) * rh
            
            is_playing = (path == cur_path)
            is_sel = (i in self._track_sel_idx)
            is_hover = (i == self._track_hover_idx)

            # Draw Row Background
            bg = BG4 if is_sel else (BG3 if is_hover else (BG if i % 2 == 0 else "#161616"))
            c.create_rectangle(0, y0, w, y1, fill=bg, outline="")

            # Draw the little Playlist Color Box
            c1, _ = playlist_gradient(pl_name)
            c.create_rectangle(12, y0+8, 48, y0+44, fill=c1, outline="")
            c.create_text(30, y0+26, text="♫", font=("Helvetica", 12), fill="white")

            # Draw Song Number or Play Icon
            txt_col = GREEN if is_playing else TEXT3
            c.create_text(70, y0+26, text="▶" if is_playing else str(i+1), font=("Helvetica", 10), fill=txt_col)

            # Draw Title & Artist Text
            c.create_text(95, y0+18, text=title[:55], font=("Helvetica Neue", 11, "bold"), fill=GREEN if is_playing else TEXT, anchor="w")
            c.create_text(95, y0+36, text=artist[:50], font=("Helvetica Neue", 9), fill=TEXT3, anchor="w")

            # Draw Playlist Name on the right
            c.create_text(w - 25, y0+26, text=pl_name[:20], font=("Helvetica Neue", 9), fill=TEXT3, anchor="e")
            c.create_line(12, y1-1, w-12, y1-1, fill=BORDER)

    def _row_at_y(self, y):
        cy = self._track_canvas.canvasy(y)
        idx = int(cy // 52)
        return idx if 0 <= idx < len(self._track_items) else None

    def _set_track_hover(self, idx):
        if self._track_hover_idx != idx:
            self._track_hover_idx = idx
            self._redraw_track_list()

    def _on_track_hover(self, event):
        idx = self._row_at_y(event.y)
        if idx != self._track_hover_idx:
            self._track_hover_idx = idx
            self._redraw_track_list()

    def _on_track_canvas_click(self, event):
        idx = self._row_at_y(event.y)
        if idx is not None:
            # Ensure the variable is initialized as a list
            if not isinstance(self._track_sel_idx, list):
                self._track_sel_idx = []

            # Handle Multi-select with Modifiers (Cmd on Mac / Ctrl on Windows)
            if (event.state & 0x0004) or (event.state & 0x0008): 
                if idx in self._track_sel_idx:
                    self._track_sel_idx.remove(idx)
                else:
                    self._track_sel_idx.append(idx)
            # Handle Shift Select
            elif (event.state & 0x0001) and self._track_sel_idx:
                start = self._track_sel_idx[-1]
                step = 1 if idx > start else -1
                for i in range(start, idx + step, step):
                    if i not in self._track_sel_idx:
                        self._track_sel_idx.append(i)
            # Normal Single Click
            else:
                self._track_sel_idx = [idx]
            
            self._redraw_track_list()

    def _on_track_canvas_dbl(self, event):
        idx = self._row_at_y(event.y)
        if idx is not None and idx < len(self._track_items):
            self._play_with_context(self._track_items[idx][0])

    def _on_track_canvas_right(self, event):
        idx = self._row_at_y(event.y)
        if idx is not None:
            # If the right-clicked item isn't already selected, select only it
            if idx not in self._track_sel_idx:
                self._track_sel_idx = [idx]
            self._redraw_track_list()
            self.ctx_menu.post(event.x_root, event.y_root)

    # ── Library search — fixed, no placeholder interference ──────────────────
    def _lib_focus_in(self, event):
        if self.lib_search_entry.get() == self._lib_ph:
            self.lib_search_entry.delete(0, "end")
            self.lib_search_entry.config(fg=TEXT)

    def _lib_focus_out(self, event):
        if not self.lib_search_entry.get().strip():
            self._lib_searching = True
            self.lib_search_entry.insert(0, self._lib_ph)
            self.lib_search_entry.config(fg=TEXT3)
            self._lib_searching = False
            self._populate_track_list("")  # clear filter

    def _lib_key_released(self, event):
        q = self.lib_search_entry.get()
        if q == self._lib_ph: return
        self._populate_track_list(q)

    def _get_sel_paths(self):
        paths = []
        for idx in self._track_sel_idx:
            if idx < len(self._track_items):
                paths.append(self._track_items[idx][0])
        return paths

    def _on_track_dbl(self, event): pass   # handled by canvas
    def _on_right_click(self, event): pass  # handled by canvas

    def _ctx_play_now(self):
        p = self._get_sel_paths()
        if p: self._play_with_context(p[0])

    def _ctx_play_next(self):
        for path in reversed(self._get_sel_paths()):
            ins = self._queue_idx+1 if self._queue_idx >= 0 else 0
            self._queue.insert(ins, path)
        self._update_queue()

    def _ctx_add_queue(self):
        for p in self._get_sel_paths(): self._queue.append(p)
        self._update_queue()

    def _ctx_clear_playlist_contents(self, pl_name):
            """Deletes files inside the folder but keeps the folder."""
            tracks = self._library.get(pl_name, [])
            if not tracks: return
            
            confirm = messagebox.askyesno("Empty Playlist", 
                f"Are you sure you want to delete all {len(tracks)} songs in '{pl_name}'?\n\nThe folder will not be deleted.")
            
            if confirm:
                for path in tracks:
                    try:
                        if os.path.exists(path): os.remove(path)
                    except: pass
                self._refresh_library()    

    def _ctx_delete_playlist_folder(self, pl_name):
        """Deletes the files AND the folder."""
        folder_path = self._find_playlist_dir(pl_name)
        if not folder_path: return
        
        # Safety: Never delete the root library folder
        if os.path.abspath(folder_path) == os.path.abspath(MUSIC_ROOT):
            messagebox.showwarning("Blocked", "Cannot delete the root library folder.")
            return
            
        confirm = messagebox.askyesno("Delete Folder", 
            f"This will permanently delete the folder '{pl_name}' and all songs inside. Continue?")
            
        if confirm:
            import shutil
            try:
                shutil.rmtree(folder_path)
                if self._selected_playlist == pl_name:
                    self._select_playlist_card(0, "All Music")
            except Exception as e:
                messagebox.showerror("Error", f"Failed to delete: {e}")
            self._refresh_library()

    def _ctx_add_playlist(self):
        paths = self._get_sel_paths()
        if not paths: return
        playlists = list(self._library.keys())
        if not playlists:
            messagebox.showinfo("No Playlists","Create a playlist first."); return
        dlg = tk.Toplevel(self); dlg.title("Add to Playlist")
        dlg.configure(bg=BG2); dlg.geometry("320x400")
        dlg.resizable(False, False); dlg.transient(self); dlg.grab_set()
        tk.Label(dlg, text="Add to Playlist", font=("Helvetica Neue",14,"bold"),
                 fg=TEXT, bg=BG2).pack(padx=20, pady=(20,4), anchor="w")
        tk.Label(dlg, text=f"{len(paths)} track(s) selected",
                 font=("Helvetica Neue",10), fg=TEXT3, bg=BG2).pack(padx=20, anchor="w")
        lb = tk.Listbox(dlg, bg=BG3, fg=TEXT2, font=("Helvetica Neue",11),
                        relief="flat", selectbackground=BG4, selectforeground=TEXT,
                        highlightthickness=0, bd=10, activestyle="none")
        lb.pack(fill="both", expand=True, padx=12, pady=(12,8))
        for pl in playlists:
            lb.insert("end", f"  {pl}  ({len(self._library[pl])})")
        br = tk.Frame(dlg, bg=BG2); br.pack(fill="x", padx=12, pady=(0,16))
        def do_add():
            sel = lb.curselection()
            if not sel: return
            target = playlists[sel[0]]
            td = self._find_playlist_dir(target)
            if not td: messagebox.showerror("Error",f"Cannot find folder for '{target}'"); dlg.destroy(); return
            copied = 0
            for src in paths:
                dst = os.path.join(td, os.path.basename(src))
                if not os.path.exists(dst): shutil.copy2(src, dst); copied += 1
            dlg.destroy(); self._refresh_library()
            messagebox.showinfo("Done", f"Copied {copied} track(s) to '{target}'.")
        np = tk.Label(br, text="+ New Playlist", font=("Helvetica Neue",10),
                      fg=GREEN, bg=BG2, cursor="hand2")
        np.pack(side="left"); np.bind("<Button-1>",
            lambda e: (dlg.destroy(), self._create_playlist_dialog(copy_paths=paths)))
        add_b = tk.Label(br, text="Add", font=("Helvetica Neue",10,"bold"),
                         fg=SIDEBAR, bg=GREEN, padx=16, pady=6, cursor="hand2")
        add_b.pack(side="right"); add_b.bind("<Button-1>", lambda e: do_add())
        add_b.bind("<Enter>", lambda e: add_b.config(bg=GREEN_HOVER))
        add_b.bind("<Leave>", lambda e: add_b.config(bg=GREEN))
        cancel = tk.Label(br, text="Cancel", font=("Helvetica Neue",10),
                          fg=TEXT2, bg=BG3, padx=12, pady=6, cursor="hand2")
        cancel.pack(side="right", padx=(0,8))
        cancel.bind("<Button-1>", lambda e: dlg.destroy())

    def _ctx_remove_from_library(self):
        paths = self._get_sel_paths()
        if not paths: return
        
        # Confirm deletion
        if messagebox.askyesno("Delete Song", f"Permanently delete {len(paths)} song(s) from your storage?"):
            for path in paths:
                try:
                    if os.path.exists(path):
                        os.remove(path)
                except Exception as e:
                    print(f"Error deleting file: {e}")
            
            # Refresh the UI to show the songs are gone
            self._refresh_library()
            
    def _ctx_delete_playlist_folder(self, pl_name):
        # 1. Safety Block: Identify names that represent the Main Root
        root_name = os.path.basename(MUSIC_ROOT)
        protected_names = ["All Music", "Unsorted", root_name, "Vibe_Library"]

        if pl_name in protected_names:
            messagebox.showwarning("Access Denied", 
                f"You cannot delete '{pl_name}' because it is your core library folder.\n\n"
                "To remove songs from here, select the songs and choose 'Delete from Library'.")
            return

        # 2. Find the actual path
        folder_path = self._find_playlist_dir(pl_name)
        
        # 3. Final Path Safety Check: Ensure the path isn't literally the MUSIC_ROOT
        if not folder_path or not os.path.exists(folder_path):
            messagebox.showerror("Error", f"Could not locate folder '{pl_name}' on disk.")
            return
            
        if os.path.abspath(folder_path) == os.path.abspath(MUSIC_ROOT):
            messagebox.showerror("Safety Error", "Target path is the Root Directory. Deletion blocked.")
            return

        # 4. Confirmation
        count = len(self._library.get(pl_name, []))
        confirm = messagebox.askyesno("Delete Playlist", 
            f"Permanently delete the folder '{pl_name}' and all {count} songs inside?")
        
        if confirm:
            try:
                import shutil
                shutil.rmtree(folder_path)
                
                # Reset view to "All Music" if we were inside that playlist
                if self._selected_playlist == pl_name:
                    self._select_playlist_card(0, "All Music")
                
                self._refresh_library()
                messagebox.showinfo("Success", f"Deleted playlist: {pl_name}")
            except Exception as e:
                messagebox.showerror("Error", f"Failed to delete folder: {e}")              

    def _ctx_remove_queue(self):
        sel = self.queue_tree.selection()
        if not sel: return
        idx = self.queue_tree.index(sel[0])
        if 0 <= idx < len(self._queue):
            self._queue.pop(idx)
            if self._queue_idx >= idx:
                self._queue_idx = max(0, self._queue_idx-1)
        self._update_queue()

    def _find_playlist_dir(self, pl_name):
        # If it's one of the root-level names, return the MUSIC_ROOT
        root_name = os.path.basename(MUSIC_ROOT)
        if pl_name in ["Unsorted", "Vibe_Library", root_name]:
            return MUSIC_ROOT
            
        # Otherwise, look for the subfolder
        for root in self._watch_roots:
            path = os.path.join(root, pl_name)
            if os.path.isdir(path):
                return path
        return None

    def _play_all(self):
        paths = [item[0] for item in self._track_items]
        if not paths: return
        pl = self._selected_playlist
        self._queue = list(paths); self._queue_idx = 0
        self._queue_playlist_name  = pl
        self._queue_playlist_paths = list(self._library.get(pl,paths)) if pl else list(paths)
        self._update_queue(); self._play_current()

    def _shuffle_all(self):
        paths = [item[0] for item in self._track_items]
        if not paths: return
        pl = self._selected_playlist
        shuffled = list(paths); random.shuffle(shuffled)
        self._queue = shuffled; self._queue_idx = 0
        self._queue_playlist_name  = pl
        self._queue_playlist_paths = list(self._library.get(pl,paths)) if pl else list(paths)
        self._update_queue(); self._play_current()

    # ── Duplicates ────────────────────────────────────────────────────────────
    def _show_duplicates(self):
        dupes = find_duplicates(self._library)
        if not dupes:
            messagebox.showinfo("No Duplicates","No duplicate tracks found across your library! 🎉")
            return
        dlg = tk.Toplevel(self); dlg.title(f"Duplicates ({len(dupes)} found)")
        dlg.configure(bg=BG2); dlg.geometry("700x480")
        dlg.transient(self); dlg.grab_set()
        tk.Label(dlg, text=f"Found {len(dupes)} duplicate track(s)",
                 font=("Helvetica Neue",14,"bold"), fg=TEXT, bg=BG2).pack(padx=20, pady=(20,4), anchor="w")
        tk.Label(dlg, text="These tracks appear in multiple playlists. Select duplicates to delete the copy.",
                 font=("Helvetica Neue",10), fg=TEXT3, bg=BG2, wraplength=640).pack(padx=20, anchor="w")

        cols = ("title","artist","playlist","original_playlist")
        tree = ttk.Treeview(dlg, style="Lib.Treeview", columns=cols,
                            show="headings", selectmode="extended")
        tree.heading("title",              text="TITLE")
        tree.heading("artist",             text="ARTIST")
        tree.heading("playlist",           text="DUPLICATE IN")
        tree.heading("original_playlist",  text="ORIGINAL IN")
        tree.column("title",             width=200, stretch=True)
        tree.column("artist",            width=160, stretch=True)
        tree.column("playlist",          width=140, stretch=False)
        tree.column("original_playlist", width=140, stretch=False)
        tree.pack(fill="both", expand=True, padx=12, pady=(12,8))
        for d in dupes:
            tree.insert("", "end", values=(d["title"], d["artist"], d["pl"], d["orig_pl"]),
                        tags=(d["path"],))

        br = tk.Frame(dlg, bg=BG2); br.pack(fill="x", padx=12, pady=(0,16))

        def delete_selected():
            sel = tree.selection()
            if not sel: return
            n = len(sel)
            if not messagebox.askyesno("Delete Duplicates",
                                       f"Delete {n} duplicate file(s)? This cannot be undone."):
                return
            for iid in sel:
                path = tree.item(iid,"tags")[0]
                try: os.remove(path)
                except: pass
                tree.delete(iid)
            self._refresh_library()
            messagebox.showinfo("Done",f"Deleted {n} duplicate(s).")

        del_btn = tk.Label(br, text="🗑  Delete Selected Duplicates",
                           font=("Helvetica Neue",10,"bold"),
                           fg=TEXT, bg=RED, padx=16, pady=8, cursor="hand2")
        del_btn.pack(side="left")
        del_btn.bind("<Button-1>", lambda e: delete_selected())
        close = tk.Label(br, text="Close", font=("Helvetica Neue",10),
                         fg=TEXT2, bg=BG3, padx=14, pady=8, cursor="hand2")
        close.pack(side="right"); close.bind("<Button-1>", lambda e: dlg.destroy())

    # ── Playlist management ───────────────────────────────────────────────────
    def _create_playlist_dialog(self, set_dest=False, copy_paths=None):
        name = simpledialog.askstring("New Playlist","Playlist name:",parent=self)
        if not name or not name.strip(): return
        name = name.strip()
        pl_dir = os.path.join(MUSIC_ROOT, name)
        os.makedirs(pl_dir, exist_ok=True)
        if copy_paths:
            for src in copy_paths:
                dst = os.path.join(pl_dir, os.path.basename(src))
                if not os.path.exists(dst): shutil.copy2(src, dst)
        self._refresh_library()
        if set_dest: self.dl_dest_var.set(pl_dir); self._update_dest_combo()
        return pl_dir

    def _add_watch_folder(self):
        path = filedialog.askdirectory(title="Select music folder to watch")
        if path and path not in self._watch_roots:
            self._watch_roots.append(path)
            self.watch_listbox.insert("end", f"  {path}")
            self._refresh_library()

    def _remove_watch_folder(self):
        sel = self.watch_listbox.curselection()
        if not sel: return
        if sel[0] == 0:
            messagebox.showwarning("Cannot remove","Default folder cannot be removed."); return
        self._watch_roots.pop(sel[0])
        self.watch_listbox.delete(sel[0])
        self._refresh_library()

    def _update_dest_combo(self):
        options = ["Internal Library (Standalone)"] + list(self._library.keys())
        self._dest_options = [MUSIC_ROOT] + [
            self._find_playlist_dir(pl) or os.path.join(MUSIC_ROOT, pl)
            for pl in self._library.keys()
        ]
        self.dest_combo["values"] = options

    def _on_dest_selected(self, event):
        idx = self.dest_combo.current()
        if 0 <= idx < len(self._dest_options):
            self.dl_dest_var.set(self._dest_options[idx])

    # ═══════════════════════════════════════════════════════════════════════════
    # PLAYER
    # ═══════════════════════════════════════════════════════════════════════════
    def _play_with_context(self, path):
        for pl_name, tracks in self._library.items():
            if path in tracks:
                self._queue_playlist_name  = pl_name
                self._queue_playlist_paths = list(tracks)
                break
        if path not in self._queue: self._queue.append(path)
        self._queue_idx = self._queue.index(path)
        self._update_queue(); self._play_current()

    def _play_current(self):
        if not PYGAME_AVAILABLE or self._queue_idx < 0 or self._queue_idx >= len(self._queue):
            return
        path = self._queue[self._queue_idx]
        if not os.path.exists(path): self._next_track(); return
        try:
            pygame.mixer.music.load(path)
            pygame.mixer.music.set_volume(self._volume)
            pygame.mixer.music.play()
            self._playing     = True
            self._paused      = False
            self._track_start = time.time()
            self._seek_pos    = 0.0
            try:
                from mutagen.mp3 import MP3
                audio = MP3(path)
                self._duration = audio.info.length
            except Exception:
                self._duration = 0.0


            title, artist = split_track_name(path)
            self.now_title.config(text=title[:48])
            self.now_artist.config(text=artist)
            self.play_btn.set_text("⏸")
            self.time_tot.config(text=format_time(self._duration))
        
            if path not in self._recently_played:
                self._recently_played.insert(0, path)
            else:
                self._recently_played.remove(path)
                self._recently_played.insert(0, path)
            self._recently_played = self._recently_played[:50]
            self._update_queue()
            self._populate_track_list(
                self.lib_search_entry.get()
                if self.lib_search_entry.get() != self._lib_ph else "")
        except: self._next_track()


    def _toggle_play(self):
        if not PYGAME_AVAILABLE: return
        if not self._playing and self._queue:
            if self._queue_idx < 0: self._queue_idx = 0
            self._play_current()
        elif self._paused:
            pygame.mixer.music.unpause()
            self._paused = False
            self._track_start = time.time() - self._seek_pos
            self.play_btn.set_text("⏸")
        elif self._playing:
            pygame.mixer.music.pause()
            self._paused = True
            self._seek_pos = time.time() - self._track_start
            self.play_btn.set_text("▶")

    def _next_track(self):
        if not self._queue: return
        nxt = (random.randint(0, len(self._queue)-1)
               if self._shuffle else self._queue_idx+1)
        if nxt < len(self._queue):
            self._queue_idx = nxt
            if hasattr(self,"_crossfade_secs") and self._crossfade_secs > 0:
                self._do_crossfade_out(lambda: (self._play_current(), self._do_crossfade_in()))
            else:
                self._play_current()
        elif self._queue_playlist_paths:
            last = self._queue[-1] if self._queue else None
            pl   = self._queue_playlist_paths
            if last and last in pl:
                rest = pl[pl.index(last)+1:]
                if rest:
                    self._queue.extend(rest); self._queue_idx = nxt
                    self._update_queue(); self._play_current(); return
            self._queue = list(pl); self._queue_idx = 0
            self._update_queue(); self._play_current()
        else:
            self._queue_idx = 0; self._play_current()

    def _prev_track(self):
        if not self._queue: return
        self._queue_idx = max(0, self._queue_idx-1); self._play_current()
        
    def _toggle_shuffle(self):
        self._shuffle = not self._shuffle
        if self._shuffle:
            self.shuf_lbl.config(text="🔀", fg=GREEN)
        else:
            self.shuf_lbl.config(text="⇄", fg=TEXT3)

    def _toggle_repeat(self):
        self._repeat = not self._repeat
        if self._repeat:
            self.rep_lbl.config(text="🔂", fg=GREEN)
        else:
            self.rep_lbl.config(text="↺", fg=TEXT3)

    def _on_scrub_moved(self, val):
        """Called on any scrub interaction."""
        if not PYGAME_AVAILABLE or self._duration <= 0: return
        pos = (float(val) / 100) * self._duration
        self._seek_pos    = pos
        self._track_start = time.time() - pos
        pygame.mixer.music.play(start=pos)
        if self._paused: pygame.mixer.music.pause()

    def _on_volume_moved(self, val):
        self._volume = float(val) / 100
        if PYGAME_AVAILABLE: pygame.mixer.music.set_volume(self._volume)

    def _poll(self):
        try:
            if self._playing:
                if not self._paused:
                    elapsed = time.time() - self._track_start
                    if self._duration > 0:
                        pct = min((elapsed/self._duration)*100, 100)
                        self.scrub.set(pct)
                        self.time_cur.config(text=format_time(elapsed))
                        
                        if elapsed >= self._duration:
                            if self._repeat: self._play_current()
                            else: self._next_track()
                
                # Only auto-skip if NOT paused and the mixer is idle
                if not self._paused and not pygame.mixer.music.get_busy():
                    if self._repeat: self._play_current()
                    else: self._next_track()
        except: pass
        if self._viz_enabled:
            self._draw_visualizer()
        self.after(400, self._poll)

    def _update_queue(self):
        self.queue_tree.delete(*self.queue_tree.get_children())
        for i, path in enumerate(self._queue):
            title, _ = split_track_name(path)
            if i == self._queue_idx:
                tag, display = "playing", f"▶  {title}"
            elif i < self._queue_idx:
                tag, display = "upcoming", title
            else:
                tag, display = "normal", title
            self.queue_tree.insert("", "end", values=(display,), tags=(tag,))
        self.queue_count_lbl.config(text=f"{len(self._queue)} tracks")
        self.queue_ctx_lbl.config(
            text=f"From: {self._queue_playlist_name}" if self._queue_playlist_name else "")
        # Update now playing in queue panel
        if 0 <= self._queue_idx < len(self._queue):
            title, artist = split_track_name(self._queue[self._queue_idx])
            self.queue_now_title.config(text=title)
            self.queue_now_artist.config(text=artist)
            items = self.queue_tree.get_children()
            if items and self._queue_idx < len(items):
                self.queue_tree.see(items[self._queue_idx])

    def _on_queue_dbl(self, event):
        sel = self.queue_tree.selection()
        if sel:
            self._queue_idx = self.queue_tree.index(sel[0])
            self._play_current()

    def _clear_queue(self):
        self._queue=[]; self._queue_idx=-1; self._playing=False
        self._queue_playlist_name=None; self._queue_playlist_paths=[]
        if PYGAME_AVAILABLE: pygame.mixer.music.stop()
        self.play_btn.set_text("▶")
        self.now_title.config(text="Nothing playing"); self.now_artist.config(text="")
        self.player_art_lbl.config(text="♫", bg=BG3)
        self.scrub.set(0); self.time_cur.config(text="0:00")
        self.queue_now_title.config(text=""); self.queue_now_artist.config(text="")
        self._update_queue()

    # ═══════════════════════════════════════════════════════════════════════════
    # SEARCH
    # ═══════════════════════════════════════════════════════════════════════════
    def _do_search(self, append=False):
        q = self.search_var.get().strip()
        if not q: return
        if not append:
            self.results_tree.delete(*self.results_tree.get_children())
            self._search_results=[]; self._search_offset=0
        self.search_status_lbl.config(text="Searching…", fg=TEXT3)
        def run():
            results = []
            # --- 1. Fetch YouTube Results ---
            try:
                with yt_dlp.YoutubeDL({"quiet":True,"extract_flat":True}) as ydl:
                    info = ydl.extract_info(f"ytsearch10:{q}", download=False)
                for e in (info.get("entries") or []):
                    results.append({
                        "title": e.get("title", "?"),
                        "uploader": e.get("uploader") or e.get("channel", "?"),
                        "url": f"https://www.youtube.com/watch?v={e.get('id','')}",
                        "source": "YouTube",
                        "duration": e.get("duration", 0)
                    })
            except: 
                pass

            # --- 2. Fetch SoundCloud Results ---
            try:
                with yt_dlp.YoutubeDL({"quiet":True,"extract_flat":True}) as ydl:
                    info = ydl.extract_info(f"scsearch5:{q}", download=False)
                for e in (info.get("entries") or []):
                    results.append({
                        "title": e.get("title", "?"),
                        "uploader": e.get("uploader") or e.get("channel", "?"),
                        "url": e.get("url") or e.get("webpage_url", ""),
                        "source": "SoundCloud",
                        "duration": e.get("duration", 0)
                    })
            except: 
                pass

            # --- 3. UI Update Function (The 'Callback') ---
            def update_ui():
                self._search_results.extend(results)
                self._search_offset += len(results)
                
                for r in results:
                    tag = "sc" if r["source"] == "SoundCloud" else "yt"
                    self.results_tree.insert("", "end", tags=(tag,),
                        values=(r["source"], r["title"], r["uploader"],
                                format_time(r["duration"]) if r["duration"] else "?"))
                
                self.search_status_lbl.config(
                    text=f"{len(self._search_results)} results" if self._search_results else "No results",
                    fg=SUCCESS if self._search_results else WARNING)
            
            # --- 4. Schedule the UI update on the Main Thread ---
            self.after(0, update_ui)

        # This line triggers the background thread
        threading.Thread(target=run, daemon=True).start()

    def _download_result(self):
        sel = self.results_tree.selection()
        if not sel:
            messagebox.showwarning("No Selection", "Select a result first.")
            return
        result = self._search_results[self.results_tree.index(sel[0])]
        out_folder = self.dl_dest_var.get() or MUSIC_ROOT
        os.makedirs(out_folder, exist_ok=True)
        self.search_status_lbl.config(text="Downloading…", fg=WARNING)

        def run():
            try:
                opts = {
                    "format": "bestaudio/best",
                    "outtmpl": os.path.join(out_folder, "%(title)s.%(ext)s"),
                    "postprocessors": [{
                        "key": "FFmpegExtractAudio",
                        "preferredcodec": "mp3",
                        "preferredquality": "192",
                    }],
                    "postprocessor_args": {
                        "FFmpegExtractAudio": [
                            "-ar", "44100",
                            "-ac", "2",
                            "-b:a", "192k",
                        ]
                    },
                    "writethumbnail": False,
                }
                with yt_dlp.YoutubeDL(opts) as ydl:
                    ydl.download([result["url"]])
                self.after(0, self._refresh_library)
                self.after(0, lambda: self.search_status_lbl.config(text="✓ Download Complete", fg=SUCCESS))
            except Exception as ex:
                self.after(0, lambda: self.search_status_lbl.config(text=f"✗ {ex}", fg=RED))

        threading.Thread(target=run, daemon=True).start()

    # ═══════════════════════════════════════════════════════════════════════════
    # DOWNLOADER
    # ═══════════════════════════════════════════════════════════════════════════
    def _log(self, msg, tag="normal"):
        self.log_text.config(state="normal")
        self.log_text.insert("end", msg+"\n", tag)
        self.log_text.see("end")
        self.log_text.config(state="disabled")

    def _yt_fi(self):
        if self.yt_entry.get()==self._yt_ph:
            self.yt_entry.delete(0,"end"); self.yt_entry.config(fg=TEXT)

    def _yt_fo(self):
        if not self.yt_entry.get().strip():
            self.yt_entry.insert(0,self._yt_ph); self.yt_entry.config(fg=TEXT3)

    def _fetch_yt(self):
        url = self.yt_url_var.get().strip()
        if not url or url==self._yt_ph: return
        self._log(f"\n🔍 Fetching: {url}","normal")
        def run():
            try:
                with yt_dlp.YoutubeDL({"quiet":True,"extract_flat":True}) as ydl:
                    info = ydl.extract_info(url, download=False)
                title = info.get("title","Unknown")
                count = len(info.get("entries") or []) or 1
                self.after(0, lambda: self.yt_info_lbl.config(text=f"  📋  {title}    🎵  {count} tracks", fg=TEXT2))
                self.after(0, lambda: self._log(f'✓ "{title}" — {count} tracks',"success"))
            except Exception as ex: self.after(0, lambda: self._log(f"✗ {ex}","error"))
        threading.Thread(target=run, daemon=True).start()

    def _add_csvs(self):
        paths = filedialog.askopenfilenames(title="Select Exportify CSV files",
                                            filetypes=[("CSV","*.csv"),("All","*.*")])
        if not paths: return
        added = 0
        for path in paths:
            tracks, err = parse_exportify_csv(path)
            if err or not tracks:
                self._log(f"⚠ {os.path.basename(path)}: {err or 'empty'}","warning"); continue
            name = re.sub(r'\.csv$','',os.path.basename(path),flags=re.IGNORECASE).strip()
            if any(p["name"]==name for p in self._dl_playlists):
                self._log(f"⚠ Already loaded: {name}","warning"); continue
            self._dl_playlists.append({"name":name,"tracks":tracks})
            self.csv_listbox.insert("end",f"  {name}  ({len(tracks)} tracks)")
            added += 1
        total = sum(len(p["tracks"]) for p in self._dl_playlists)
        self.csv_count_lbl.config(
            text=f"{len(self._dl_playlists)} playlists  •  {total} tracks",
            fg=SUCCESS if self._dl_playlists else TEXT3)
        if added: self._log(f"✓ {added} playlist(s) — {total} total tracks.","success")

    def _clear_csvs(self):
        self._dl_playlists=[]; self.csv_listbox.delete(0,"end")
        self.csv_count_lbl.config(text="No playlists loaded",fg=TEXT3)

    def _toggle_dl(self):
        if self._is_dl:
            self._is_dl=False; self.dl_btn.config(text="⬇  Download All as MP3",bg=GREEN)
            self.status_var.set("Cancelled"); self._log("⚠ Cancelled.","warning")
        else:
            if self._dl_playlists: self._start_csv_dl()
            else:
                url = self.yt_url_var.get().strip()
                if not url or url==self._yt_ph:
                    messagebox.showwarning("Nothing to download","Paste a YouTube URL or load CSV files."); return
                self._start_yt_dl(url)

    def _dl_hook(self, d):
        if not self._is_dl: raise yt_dlp.utils.DownloadCancelled()
        if d.get("status")=="downloading":
            pct = float(re.sub(r"[^\d.]","",d.get("_percent_str","0%").strip()) or 0)
            self.prog_var.set(pct)
            self.status_var.set(f"{pct:.0f}%  {d.get('_speed_str','').strip()}")

    def _make_pp(self):
        return [{
            "key": "FFmpegExtractAudio",
            "preferredcodec": "mp3",
            "preferredquality": "192",
        }]

    def _dest_dir(self, playlist_name=None):
        chosen = self.dl_dest_var.get()
        if chosen and chosen != MUSIC_ROOT and os.path.isabs(chosen): return chosen
        if playlist_name:
            return os.path.join(MUSIC_ROOT, re.sub(r'[<>:"/\\|?*]','',playlist_name).strip())
        return MUSIC_ROOT

    def _start_yt_dl(self, url):
        label = "Playlist_Download"
        out_folder = self._dest_dir(label)
        os.makedirs(out_folder, exist_ok=True)
        self._is_dl = True
        self.dl_btn.config(text="⏹  Stop", bg=BG4)
        self.prog_var.set(0)
        self._log(f"\n📡 Starting YouTube Download...", "normal")

        def run():
            try:
                opts = {
                    "format": "bestaudio/best",
                    # 'restrictfilenames' prevents weird characters from breaking Mac file paths
                    "restrictfilenames": True, 
                    "outtmpl": os.path.join(out_folder, "%(title)s.%(ext)s"),
                    "noplaylist": True,
                    # Remove 'quiet': True so we can see errors in the console if it hangs
                    "quiet": False, 
                    "postprocessors": [{
                        "key": "FFmpegExtractAudio",
                        "preferredcodec": "mp3",
                        "preferredquality": "192",
                    }],
                    # We removed the "-ar 44100" argument here. 
                    # Forcing a sample rate on Mac is what causes the 'sheet metal' audio glitch.
                    "writethumbnail": False,
                }
                
                if self.skip_var.get():
                    opts["download_archive"] = os.path.join(out_folder, ".downloaded.txt")
                
                with yt_dlp.YoutubeDL(opts) as ydl:
                    ydl.download([url])
                
                if self._is_dl:
                    self.after(0, lambda: self._log("✅ Download Complete!", "success"))
                    self.after(0, lambda: self.prog_var.set(100))
                    self.after(0, self._refresh_library)
            except Exception as ex:
                self.after(0, lambda: self._log(f"✗ Error: {ex}", "error"))
                print(f"DL Debug Error: {ex}") # This prints to your terminal/IDE
            
            self._is_dl = False
            self.after(0, lambda: self.dl_btn.config(text="⬇  Download All as MP3", bg=GREEN))

        threading.Thread(target=run, daemon=True).start()

        label = "Playlist_Download"
        out_folder = self._dest_dir(label)
        os.makedirs(out_folder, exist_ok=True)
        self._is_dl = True
        self.dl_btn.config(text="⏹  Stop", bg=BG4)
        self.prog_var.set(0)
        self._log(f"\n📡 Starting YouTube Download...", "normal")

        def run():
            try:
                opts = {
                    "format": "bestaudio/best",
                    "outtmpl": os.path.join(out_folder, "%(title)s.%(ext)s"),
                    "postprocessors": [{
                        "key": "FFmpegExtractAudio",
                        "preferredcodec": "mp3",
                        "preferredquality": "192",
                    }],
                    "postprocessor_args": {
                        "FFmpegExtractAudio": [
                            "-ar", "44100",
                            "-ac", "2",
                            "-b:a", "192k",
                        ]
                    },
                    "writethumbnail": False,
                }
                if self.skip_var.get():
                    opts["download_archive"] = os.path.join(out_folder, ".downloaded.txt")
                with yt_dlp.YoutubeDL(opts) as ydl:
                    ydl.download([url])
                if self._is_dl:
                    self.after(0, lambda: self._log("✅ Download Sequence Complete!", "success"))
                    self.after(0, lambda: self.prog_var.set(100))
                    self.after(0, self._refresh_library)
            except Exception as ex:
                self.after(0, lambda: self._log(f"✗ Error: {ex}", "error"))
            self._is_dl = False
            self.after(0, lambda: self.dl_btn.config(text="⬇  Download All as MP3", bg=GREEN))

        threading.Thread(target=run, daemon=True).start()

    def _start_csv_dl(self):
        playlists = list(self._dl_playlists)
        total = sum(len(p["tracks"]) for p in playlists)
        self._is_dl = True
        self._dl_done = 0
        self.dl_btn.config(text="⏹  Stop", bg=BG4)
        self.prog_var.set(0)
        self._log(f"\n⬇ Processing {len(playlists)} playlists ({total} tracks total)", "normal")

        def run():
            done_count = 0
            for pi, pl in enumerate(playlists):
                if not self._is_dl: break
                name = pl["name"]
                tracks = pl["tracks"]
                out_folder = self._dest_dir(name)
                os.makedirs(out_folder, exist_ok=True)
                self.after(0, lambda n=name: self._log(f"\n📂 Syncing Playlist: {n}", "playlist"))

                for i, t in enumerate(tracks):
                    if not self._is_dl: break
                    track_query = f"{t['artist']} {t['title']}"
                    
                    # Clean the filename for Mac safety
                    safe_name = re.sub(r'[<>:"/\\|?*]', '', f"{t['artist']} - {t['title']}").strip()
                    self.after(0, lambda q=track_query: self.status_var.set(f"Searching: {q}"))

                    use_url = f"ytsearch1:{track_query}"
                    if self.sc_var.get():
                        sc_url, sc_info = probe_soundcloud(track_query)
                        if sc_url: use_url = sc_url

                    opts = {
                        "format": "bestaudio/best",
                        "restrictfilenames": True, # Prevents files from disappearing due to bad characters
                        "outtmpl": os.path.join(out_folder, f"{safe_name}.%(ext)s"),
                        "noplaylist": True,
                        "quiet": False, # Setting to False so you can see errors in the terminal
                        "postprocessors": [{
                            "key": "FFmpegExtractAudio",
                            "preferredcodec": "mp3",
                            "preferredquality": "192",
                        }],
                        # FIXED: Removed the 'postprocessor_args' that were forcing sample rates.
                        # This is the primary fix for the "sheet metal/broken glass" audio quality.
                        "writethumbnail": False,
                    }
                    
                    if self.skip_var.get():
                        opts["download_archive"] = os.path.join(out_folder, ".downloaded.txt")

                    try:
                        with yt_dlp.YoutubeDL(opts) as ydl:
                            ydl.download([use_url])
                        done_count += 1
                        self.after(0, lambda d=done_count: self.prog_var.set((d/total)*100))
                        self.after(0, lambda d=done_count: self.dl_count_lbl.config(text=f"{d} / {total}"))
                    except Exception as e:
                        # Log the specific error so you know why a track failed
                        self.after(0, lambda tn=t['title'], err=e: self._log(f"⚠ Failed {tn}: {err}", "error"))
                        print(f"CSV DL Error: {e}")

            self.after(0, self._refresh_library)
            self._is_dl = False
            self.after(0, lambda: self.dl_btn.config(text="⬇  Download All as MP3", bg=GREEN))

        threading.Thread(target=run, daemon=True).start()

        playlists = list(self._dl_playlists)
        total = sum(len(p["tracks"]) for p in playlists)
        self._is_dl = True
        self._dl_done = 0
        self.dl_btn.config(text="⏹  Stop", bg=BG4)
        self.prog_var.set(0)
        self._log(f"\n⬇ Processing {len(playlists)} playlists ({total} tracks total)", "normal")

        def run():
            done_count = 0
            for pi, pl in enumerate(playlists):
                if not self._is_dl: break
                name = pl["name"]
                tracks = pl["tracks"]
                out_folder = self._dest_dir(name)
                os.makedirs(out_folder, exist_ok=True)
                self.after(0, lambda n=name: self._log(f"\n📂 Syncing Playlist: {n}", "playlist"))

                for i, t in enumerate(tracks):
                    if not self._is_dl: break
                    track_query = f"{t['artist']} {t['title']}"
                    safe_name = re.sub(r'[<>:"/\\|?*]', '', f"{t['artist']} - {t['title']}").strip()
                    self.after(0, lambda q=track_query: self.status_var.set(f"Searching: {q}"))

                    use_url = f"ytsearch1:{track_query}"
                    if self.sc_var.get():
                        sc_url, sc_info = probe_soundcloud(track_query)
                        if sc_url: use_url = sc_url

                    opts = {
                        "format": "bestaudio/best",
                        "outtmpl": os.path.join(out_folder, f"{safe_name}.%(ext)s"),
                        "postprocessors": [{
                            "key": "FFmpegExtractAudio",
                            "preferredcodec": "mp3",
                            "preferredquality": "192",
                        }],
                        "postprocessor_args": {
                            "FFmpegExtractAudio": [
                                "-ar", "44100",
                                "-ac", "2",
                                "-b:a", "192k",
                            ]
                        },
                        "writethumbnail": False,
                    }
                    if self.skip_var.get():
                        opts["download_archive"] = os.path.join(out_folder, ".downloaded.txt")

                    try:
                        with yt_dlp.YoutubeDL(opts) as ydl:
                            ydl.download([use_url])
                        done_count += 1
                        self.after(0, lambda d=done_count: self.prog_var.set((d/total)*100))
                        self.after(0, lambda d=done_count: self.dl_count_lbl.config(text=f"{d} / {total}"))
                    except:
                        pass

            self.after(0, self._refresh_library)
            self._is_dl = False
            self.after(0, lambda: self.dl_btn.config(text="⬇  Download All as MP3", bg=GREEN))

        threading.Thread(target=run, daemon=True).start()

    # ═══════════════════════════════════════════════════════════════════════════
    # SLEEP TIMER
    # ═══════════════════════════════════════════════════════════════════════════
    def _set_sleep_timer(self):
        val = self._sleep_var.get()
        if hasattr(self, "_sleep_after_id") and self._sleep_after_id:
            self.after_cancel(self._sleep_after_id)
            self._sleep_after_id = None
        if val == "Off":
            self._sleep_lbl.config(text="Sleep timer off")
            return
        mins = int(val.split()[0])
        self._sleep_after_id = self.after(mins * 60 * 1000, self._sleep_trigger)
        self._sleep_lbl.config(text=f"Pausing in {mins} minutes")

    def _sleep_trigger(self):
        if self._playing and not self._paused:
            self._toggle_play()
        self._sleep_lbl.config(text="Sleep timer fired — playback paused")
        self._sleep_var.set("Off")

    # ═══════════════════════════════════════════════════════════════════════════
    # CROSSFADE
    # ═══════════════════════════════════════════════════════════════════════════
    def _set_crossfade(self):
        val = self._cf_var.get()
        self._crossfade_secs = 0 if val == "Off" else int(val.replace("s",""))

    def _do_crossfade_out(self, on_done):
        if not PYGAME_AVAILABLE or not hasattr(self,"_crossfade_secs") or self._crossfade_secs <= 0:
            on_done(); return
        steps = 20; interval = int(self._crossfade_secs * 1000 / steps)
        orig_vol = self._volume; step_vol = orig_vol / steps
        def fade(rem):
            if rem <= 0:
                pygame.mixer.music.set_volume(orig_vol); on_done(); return
            pygame.mixer.music.set_volume(max(0, pygame.mixer.music.get_volume() - step_vol))
            self.after(interval, lambda: fade(rem-1))
        fade(steps)

    def _do_crossfade_in(self):
        if not PYGAME_AVAILABLE or not hasattr(self,"_crossfade_secs") or self._crossfade_secs <= 0: return
        steps = 20; interval = int(self._crossfade_secs * 1000 / steps)
        target = self._volume; step_vol = target / steps
        pygame.mixer.music.set_volume(0)
        def fade(rem):
            if rem <= 0:
                pygame.mixer.music.set_volume(target); return
            pygame.mixer.music.set_volume(min(target, pygame.mixer.music.get_volume() + step_vol))
            self.after(interval, lambda: fade(rem-1))
        fade(steps)

    # ═══════════════════════════════════════════════════════════════════════════
    # SMART PLAYLISTS
    # ═══════════════════════════════════════════════════════════════════════════
    def _generate_smart_playlist(self, mode):
        paths = []; name = None
        if mode == "most_played":
            freq = {}
            for p in self._recently_played: freq[p] = freq.get(p,0)+1
            paths = sorted(freq, key=lambda x: freq[x], reverse=True)[:30]
            name  = "Most Played"
        elif mode == "recent":
            all_t = [(p, os.path.getmtime(p))
                     for tracks in self._library.values()
                     for p in tracks if os.path.exists(p)]
            all_t.sort(key=lambda x: x[1], reverse=True)
            paths = [p for p,_ in all_t[:50]]; name = "Recently Added"
        elif mode == "artist":
            artist = simpledialog.askstring("Artist Playlist","Enter artist name:",parent=self)
            if not artist: return
            artist = artist.strip().lower()
            for tracks in self._library.values():
                for p in tracks:
                    _, a = split_track_name(p)
                    if artist in a.lower(): paths.append(p)
            name = f"{artist.title()} — All Tracks"
        if not paths:
            messagebox.showinfo("No Tracks","No tracks found for this smart playlist."); return
        pl_dir = os.path.join(MUSIC_ROOT, name)
        os.makedirs(pl_dir, exist_ok=True)
        for src in paths:
            dst = os.path.join(pl_dir, os.path.basename(src))
            if not os.path.exists(dst):
                try: shutil.copy2(src, dst)
                except: pass
        self._refresh_library()
        messagebox.showinfo("Done", f"Created \"{name}\" with {len(paths)} tracks.")

    # ═══════════════════════════════════════════════════════════════════════════
    # KEYBOARD SHORTCUTS
    # ═══════════════════════════════════════════════════════════════════════════
    def _bind_keys(self):
        self.bind("<XF86AudioPlay>",         lambda e: self._toggle_play())
        self.bind("<XF86AudioNext>",         lambda e: self._next_track())
        self.bind("<XF86AudioPrev>",         lambda e: self._prev_track())
        self.bind("<Up>",             lambda e: self._change_volume(0.05))
        self.bind("<Down>",           lambda e: self._change_volume(-0.05))

    def _seek_relative(self, secs):
        if not PYGAME_AVAILABLE or self._duration <= 0: return
        new_pos = max(0, min(self._duration, (time.time() - self._track_start) + secs))
        self._seek_pos    = new_pos
        self._track_start = time.time() - new_pos
        pygame.mixer.music.play(start=new_pos)
        if self._paused: pygame.mixer.music.pause()

    def _change_volume(self, delta):
        new_vol = max(0, min(1, self._volume + delta))
        self._volume = new_vol
        self.vol_slider.set(new_vol * 100)
        if PYGAME_AVAILABLE: pygame.mixer.music.set_volume(new_vol)

    # ═══════════════════════════════════════════════════════════════════════════
    # RECENTLY PLAYED VIEW
    # ═══════════════════════════════════════════════════════════════════════════
    def _build_recent_view(self, parent):
        parent.grid_rowconfigure(2, weight=1)
        parent.grid_columnconfigure(0, weight=1)
        hdr = tk.Canvas(parent, height=120, bg=BG, highlightthickness=0)
        hdr.grid(row=0, column=0, sticky="ew")
        hdr.bind("<Configure>",
            lambda e: draw_gradient_rect(hdr, 0, 0, e.width, 120, "#1a1a3a", BG))
        tk.Label(parent, text="Recently Played",
                 font=("Helvetica Neue",32,"bold"), fg=TEXT, bg=BG,
                 anchor="w").grid(row=0, column=0, sticky="sw", padx=28, pady=(0,16))

        act = tk.Frame(parent, bg=BG)
        act.grid(row=1, column=0, sticky="ew", padx=28, pady=(8,6))
        play_btn = tk.Label(act, text="▶  Play All",
                            font=("Helvetica Neue",10,"bold"),
                            fg=SIDEBAR, bg=GREEN, padx=14, pady=7, cursor="hand2")
        play_btn.pack(side="left")
        play_btn.bind("<Button-1>", lambda e: self._play_recent_all())
        play_btn.bind("<Enter>", lambda e: play_btn.config(bg=GREEN_HOVER))
        play_btn.bind("<Leave>", lambda e: play_btn.config(bg=GREEN))

        clr = tk.Label(act, text="Clear History",
                       font=("Helvetica Neue",10), fg=TEXT3, bg=BG, cursor="hand2")
        clr.pack(side="left", padx=(16,0))
        clr.bind("<Button-1>", lambda e: self._clear_recent())
        clr.bind("<Enter>", lambda e: clr.config(fg=TEXT2))
        clr.bind("<Leave>", lambda e: clr.config(fg=TEXT3))

        # Canvas track list for recently played
        rf = tk.Frame(parent, bg=BG)
        rf.grid(row=2, column=0, sticky="nsew", padx=28, pady=(0,16))
        rf.grid_rowconfigure(0, weight=1); rf.grid_columnconfigure(0, weight=1)

        self._recent_canvas = tk.Canvas(rf, bg=BG, highlightthickness=0, bd=0)
        r_sb = tk.Scrollbar(rf, command=self._recent_canvas.yview,
                            bg=BG, troughcolor=BG, highlightthickness=0, bd=0, width=6)
        self._recent_canvas.config(yscrollcommand=r_sb.set)
        r_sb.grid(row=0, column=1, sticky="ns")
        self._recent_canvas.grid(row=0, column=0, sticky="nsew")
        self._recent_canvas.bind("<Configure>",  lambda e: self._redraw_recent())
        def _scroll_recent(event):
            self._recent_canvas.yview_scroll(int(-1*(event.delta)), "units")
        self._recent_canvas.bind("<Enter>", lambda e: self._recent_canvas.bind_all("<MouseWheel>", _scroll_recent))
        self._recent_canvas.bind("<Leave>", lambda e: self._recent_canvas.unbind_all("<MouseWheel>"))
        self._recent_canvas.bind("<Double-Button-1>", self._on_recent_dbl)
        self._recent_hover = None
        self._recent_canvas.bind("<Motion>", self._on_recent_hover)
        self._recent_canvas.bind("<Leave>",  lambda e: self._set_recent_hover(None))

    def _redraw_recent(self):
        c = self._recent_canvas
        c.delete("all")
        w = c.winfo_width()
        if w < 10 or not self._recently_played: return
        rh = 52
        c.configure(scrollregion=(0,0,w,len(self._recently_played)*rh))
        try:
            top = c.canvasy(0); bot = c.canvasy(c.winfo_height())
        except: top,bot = 0,len(self._recently_played)*rh
        for i, path in enumerate(self._recently_played):
            y0 = i*rh; y1 = y0+rh
            if y1 < top or y0 > bot: continue
            title, artist = split_track_name(path)
            pl = next((pl for pl,tracks in self._library.items() if path in tracks), "")
            is_h = (i == self._recent_hover)
            bg = BG3 if is_h else (BG if i%2==0 else "#161616")
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

    def _on_recent_dbl(self, event):
        cy = self._recent_canvas.canvasy(event.y)
        idx = int(cy // 52)
        if 0 <= idx < len(self._recently_played):
            self._play_with_context(self._recently_played[idx])

    def _on_recent_hover(self, event):
        cy = self._recent_canvas.canvasy(event.y)
        self._set_recent_hover(int(cy // 52))

    def _set_recent_hover(self, idx):
        if self._recent_hover != idx:
            self._recent_hover = idx; self._redraw_recent()

    def _play_recent_all(self):
        if not self._recently_played: return
        self._queue = list(self._recently_played); self._queue_idx = 0
        self._queue_playlist_name = None; self._queue_playlist_paths = []
        self._update_queue(); self._play_current()

    def _clear_recent(self):
        self._recently_played = []; self._redraw_recent()

    # ═══════════════════════════════════════════════════════════════════════════
    # ARTISTS VIEW
    # ═══════════════════════════════════════════════════════════════════════════
    def _build_artists_view(self, parent):
        parent.grid_rowconfigure(1, weight=1)
        parent.grid_columnconfigure(0, weight=1)
        hdr = tk.Canvas(parent, height=120, bg=BG, highlightthickness=0)
        hdr.grid(row=0, column=0, sticky="ew")
        hdr.bind("<Configure>",
            lambda e: draw_gradient_rect(hdr, 0, 0, e.width, 120, "#2a1a2a", BG))
        tk.Label(parent, text="Artists",
                 font=("Helvetica Neue",32,"bold"), fg=TEXT, bg=BG,
                 anchor="w").grid(row=0, column=0, sticky="sw", padx=28, pady=(0,16))

        main_area = tk.Frame(parent, bg=BG)
        main_area.grid(row=1, column=0, sticky="nsew")
        main_area.grid_rowconfigure(0, weight=1)
        main_area.grid_columnconfigure(0, weight=0)
        main_area.grid_columnconfigure(1, weight=1)

        # Artist list (left)
        al = tk.Frame(main_area, bg=BG2, width=220)
        al.grid(row=0, column=0, sticky="nsew")
        al.grid_propagate(False)
        tk.Label(al, text="ALL ARTISTS", font=("Helvetica Neue",9,"bold"),
                 fg=TEXT3, bg=BG2).pack(anchor="w", padx=16, pady=(14,8))
        al_frame = tk.Frame(al, bg=BG2)
        al_frame.pack(fill="both", expand=True)
        self._artist_listbox = tk.Listbox(al_frame, bg=BG2, fg=TEXT2,
                                           font=("Helvetica Neue",11), relief="flat",
                                           selectbackground=BG3, selectforeground=TEXT,
                                           highlightthickness=0, bd=8, activestyle="none")
        a_sb = tk.Scrollbar(al_frame, command=self._artist_listbox.yview,
                            bg=BG2, troughcolor=BG2, highlightthickness=0, bd=0, width=4)
        self._artist_listbox.config(yscrollcommand=a_sb.set)
        a_sb.pack(side="right", fill="y")
        self._artist_listbox.pack(fill="both", expand=True)
        self._artist_listbox.bind("<<ListboxSelect>>", self._on_artist_select)

        # Artist track list (right)
        ar = tk.Frame(main_area, bg=BG)
        ar.grid(row=0, column=1, sticky="nsew", padx=(1,0))
        ar.grid_rowconfigure(1, weight=1); ar.grid_columnconfigure(0, weight=1)

        self._artist_header = tk.Label(ar, text="Select an artist",
                                        font=("Helvetica Neue",20,"bold"),
                                        fg=TEXT, bg=BG, anchor="w")
        self._artist_header.grid(row=0, column=0, sticky="ew", padx=24, pady=(16,8))

        self._artist_canvas = tk.Canvas(ar, bg=BG, highlightthickness=0, bd=0)
        as_sb = tk.Scrollbar(ar, command=self._artist_canvas.yview,
                             bg=BG, troughcolor=BG, highlightthickness=0, bd=0, width=6)
        self._artist_canvas.config(yscrollcommand=as_sb.set)
        as_sb.grid(row=1, column=1, sticky="ns")
        self._artist_canvas.grid(row=1, column=0, sticky="nsew", padx=(24,0))
        self._artist_canvas.bind("<Configure>", lambda e: self._redraw_artist_tracks())
        def _scroll_artist(event):
            self._artist_canvas.yview_scroll(int(-1*(event.delta)), "units")
        self._artist_canvas.bind("<Enter>", lambda e: self._artist_canvas.bind_all("<MouseWheel>", _scroll_artist))
        self._artist_canvas.bind("<Leave>", lambda e: self._artist_canvas.unbind_all("<MouseWheel>"))
        self._artist_canvas.bind("<Double-Button-1>", self._on_artist_track_dbl)
        self._artist_tracks = []
        self._artist_hover  = None
        self._artist_canvas.bind("<Motion>", lambda e: self._set_artist_hover(
            int(self._artist_canvas.canvasy(e.y)//52)))
        self._artist_canvas.bind("<Leave>",  lambda e: self._set_artist_hover(None))

    def _populate_artists_view(self):
        artists = {}
        for pl_name, tracks in self._library.items():
            for path in tracks:
                title, artist = split_track_name(path)
                # If no artist is found in filename, group under "Unknown Artist"
                artist_key = artist.strip() if (artist and artist.strip()) else "Unknown Artist"
                artists.setdefault(artist_key, []).append(path)
                
        # Deduplicate and Sort
        for a in artists:
            artists[a] = list(dict.fromkeys(artists[a]))
            
        self._artists_data = dict(sorted(artists.items(), key=lambda x: x[0].lower()))
        self._artist_listbox.delete(0, "end")
        for name, tracks in self._artists_data.items():
            self._artist_listbox.insert("end", f"  {name}  ({len(tracks)})")

    def _on_artist_select(self, event):
        sel = self._artist_listbox.curselection()
        if not sel: return
        artist_name = list(self._artists_data.keys())[sel[0]]
        self._artist_tracks = self._artists_data[artist_name]
        self._artist_header.config(
            text=f"{artist_name}  ·  {len(self._artist_tracks)} tracks")
        self._redraw_artist_tracks()

    def _redraw_artist_tracks(self):
        c = self._artist_canvas
        c.delete("all")
        w = c.winfo_width()
        if w < 10 or not self._artist_tracks: return
        rh = 52
        c.configure(scrollregion=(0,0,w,len(self._artist_tracks)*rh))
        try:
            top = c.canvasy(0); bot = c.canvasy(c.winfo_height())
        except: top,bot = 0,len(self._artist_tracks)*rh
        for i, path in enumerate(self._artist_tracks):
            y0=i*rh; y1=y0+rh
            if y1<top or y0>bot: continue
            title, artist = split_track_name(path)
            pl = next((pl for pl,tracks in self._library.items() if path in tracks), "")
            is_h = (i == self._artist_hover)
            bg = BG3 if is_h else (BG if i%2==0 else "#161616")
            c.create_rectangle(0,y0,w,y1, fill=bg, outline="")
            c1c,c2c = playlist_gradient(pl or title)
            r1g,g1g,b1g=hex_to_rgb(c1c); r2g,g2g,b2g=hex_to_rgb(c2c)
            for si in range(36):
                t=si/36
                c.create_rectangle(8,y0+8+si,44,y0+9+si,
                    fill=rgb_to_hex(int(r1g+(r2g-r1g)*t),
                                    int(g1g+(g2g-g1g)*t),
                                    int(b1g+(b2g-b1g)*t)),outline="")
            c.create_text(26,y0+26,text="♫",font=("Helvetica Neue",11,"bold"),fill="#ffffff",anchor="center")
            c.create_text(56,y0+rh//2-8,text=title[:55],
                          font=("Helvetica Neue",11,"bold"),fill=TEXT,anchor="w")
            c.create_text(56,y0+rh//2+10,text=pl[:50],
                          font=("Helvetica Neue",9),fill=TEXT3,anchor="w")
            c.create_line(8,y1-1,w-8,y1-1,fill=BORDER)

    def _on_artist_track_dbl(self, event):
        cy = self._artist_canvas.canvasy(event.y)
        idx = int(cy//52)
        if 0 <= idx < len(self._artist_tracks):
            self._play_with_context(self._artist_tracks[idx])

    def _set_artist_hover(self, idx):
        if self._artist_hover != idx:
            self._artist_hover = idx; self._redraw_artist_tracks()

    # ═══════════════════════════════════════════════════════════════════════════
    # AUDIO VISUALIZER
    # ═══════════════════════════════════════════════════════════════════════════
    def _draw_visualizer(self):
        if not hasattr(self, '_viz_canvas') or not self._viz_enabled: return
        c = self._viz_canvas
        c.delete("all")
        w = c.winfo_width(); h = c.winfo_height()
        if w < 10 or h < 10: return
        bars = 32
        bar_w = max(1, (w - bars) // bars)
        c1, _ = playlist_gradient(self._selected_playlist or "vibe")
        for i in range(bars):
            if self._playing and not self._paused:
                height = random.randint(4, h-4)
            else:
                height = 4
            x0 = i * (bar_w + 1)
            x1 = x0 + bar_w
            r1,g1,b1 = hex_to_rgb(c1)
            r2,g2,b2 = hex_to_rgb(GREEN)
            t = i / bars
            col = rgb_to_hex(int(r1+(r2-r1)*t), int(g1+(g2-g1)*t), int(b1+(b2-b1)*t))
            c.create_rectangle(x0, h-height, x1, h, fill=col, outline="")

    def _toggle_visualizer(self):
        self._viz_enabled = not self._viz_enabled
        if self._viz_enabled:
            self._viz_toggle_btn.config(text="Disable", bg=RED)
            self._draw_visualizer()
        else:
            self._viz_toggle_btn.config(text="Enable", bg=GREEN)
            if hasattr(self, '_viz_canvas'):
                self._viz_canvas.delete("all")

    # ═══════════════════════════════════════════════════════════════════════════
    # EXPORT PLAYLIST
    # ═══════════════════════════════════════════════════════════════════════════
    def _export_playlist_m3u(self):
        if not self._queue and not self._selected_playlist:
            messagebox.showwarning("Nothing to export",
                                   "Load a playlist or build a queue first."); return
        paths = (self._queue if self._queue
                 else self._library.get(self._selected_playlist, []))
        if not paths:
            messagebox.showwarning("Empty","No tracks to export."); return
        dest = filedialog.asksaveasfilename(
            title="Export Playlist",
            defaultextension=".m3u",
            filetypes=[("M3U Playlist","*.m3u"),("All","*.*")],
            initialfile=f"{self._selected_playlist or 'queue'}.m3u")
        if not dest: return
        with open(dest, "w", encoding="utf-8") as f:
            f.write("#EXTM3U\n")
            for path in paths:
                title, artist = split_track_name(path)
                f.write(f"#EXTINF:-1,{artist} - {title}\n{path}\n")
        messagebox.showinfo("Exported", f"Saved {len(paths)} tracks to:\n{dest}")

    # ═══════════════════════════════════════════════════════════════════════════
    # LIBRARY SYNC — compare against YouTube/Spotify sources and download new tracks
    # ═══════════════════════════════════════════════════════════════════════════
    def _build_sync_view(self, parent):
        parent.grid_rowconfigure(3, weight=1)
        parent.grid_columnconfigure(0, weight=1)
        hdr = tk.Canvas(parent, height=120, bg=BG, highlightthickness=0)
        hdr.grid(row=0, column=0, sticky="ew")
        hdr.bind("<Configure>",
            lambda e: draw_gradient_rect(hdr, 0, 0, e.width, 120, "#1a2a3a", BG))
        tk.Label(parent, text="Sync Library",
                 font=("Helvetica Neue",32,"bold"), fg=TEXT, bg=BG,
                 anchor="w").grid(row=0, column=0, sticky="sw", padx=28, pady=(0,16))

        info = tk.Frame(parent, bg=BG)
        info.grid(row=1, column=0, sticky="ew", padx=28, pady=(0,12))
        tk.Label(info, text="Compare your local library against a YouTube playlist or Spotify CSV.\nNew tracks that aren't downloaded yet will be listed — then download them in one click.",
                 font=("Helvetica Neue",11), fg=TEXT2, bg=BG, justify="left",
                 wraplength=700).pack(anchor="w")

        # Source input
        src_f = tk.Frame(parent, bg=BG)
        src_f.grid(row=2, column=0, sticky="ew", padx=28, pady=(0,12))

        tk.Label(src_f, text="SOURCE", font=("Helvetica Neue",9,"bold"),
                 fg=TEXT3, bg=BG).pack(anchor="w", pady=(0,6))

        yt_row = tk.Frame(src_f, bg=BG3, highlightbackground=BG4, highlightthickness=1)
        yt_row.pack(fill="x", pady=(0,6))
        tk.Label(yt_row, text="YT URL", font=("Helvetica Neue",9,"bold"),
                 fg=TEXT3, bg=BG3, padx=10, pady=8).pack(side="left")
        self._sync_yt_var = tk.StringVar()
        tk.Entry(yt_row, textvariable=self._sync_yt_var, bg=BG3, fg=TEXT,
                 insertbackground=TEXT, relief="flat",
                 font=("Helvetica Neue",10), bd=8).pack(side="left", fill="x", expand=True)

        csv_row = tk.Frame(src_f, bg=BG)
        csv_row.pack(fill="x", pady=(0,6))
        tk.Label(csv_row, text="OR", font=("Helvetica Neue",9,"bold"),
                 fg=TEXT3, bg=BG, padx=(0)).pack(side="left", padx=(0,12))
        self._sync_csv_var = tk.StringVar(value="No CSV loaded")
        tk.Label(csv_row, textvariable=self._sync_csv_var,
                 font=("Helvetica Neue",10), fg=TEXT3, bg=BG).pack(side="left", expand=True)
        load_csv = tk.Label(csv_row, text="Load Spotify CSV",
                            font=("Helvetica Neue",10,"bold"),
                            fg=SIDEBAR, bg=GREEN, padx=12, pady=6, cursor="hand2")
        load_csv.pack(side="right")
        load_csv.bind("<Button-1>", lambda e: self._sync_load_csv())
        load_csv.bind("<Enter>", lambda e: load_csv.config(bg=GREEN_HOVER))
        load_csv.bind("<Leave>", lambda e: load_csv.config(bg=GREEN))

        self._sync_tracks = []   # list of {title, artist} not yet downloaded

        btn_row = tk.Frame(src_f, bg=BG)
        btn_row.pack(fill="x", pady=(8,0))
        check_btn = tk.Label(btn_row, text="🔍  Check for New Tracks",
                             font=("Helvetica Neue",11,"bold"),
                             fg=SIDEBAR, bg=GREEN, padx=18, pady=10, cursor="hand2")
        check_btn.pack(side="left")
        check_btn.bind("<Button-1>", lambda e: self._sync_check())
        check_btn.bind("<Enter>", lambda e: check_btn.config(bg=GREEN_HOVER))
        check_btn.bind("<Leave>", lambda e: check_btn.config(bg=GREEN))

        self._sync_dl_btn = tk.Label(btn_row, text="⬇  Download All New",
                                      font=("Helvetica Neue",11,"bold"),
                                      fg=TEXT3, bg=BG4, padx=18, pady=10, cursor="hand2")
        self._sync_dl_btn.pack(side="left", padx=(10,0))
        self._sync_dl_btn.bind("<Button-1>", lambda e: self._sync_download())

        self._sync_status = tk.Label(btn_row, text="",
                                      font=("Helvetica Neue",10), fg=TEXT3, bg=BG)
        self._sync_status.pack(side="right")

        # Results list
        res_f = tk.Frame(parent, bg=BG)
        res_f.grid(row=3, column=0, sticky="nsew", padx=28, pady=(0,16))
        res_f.grid_rowconfigure(0, weight=1); res_f.grid_columnconfigure(0, weight=1)

        self._sync_tree = ttk.Treeview(res_f, style="Results.Treeview",
                                        columns=("status","title","artist"),
                                        show="headings")
        self._sync_tree.heading("status", text="STATUS")
        self._sync_tree.heading("title",  text="TITLE")
        self._sync_tree.heading("artist", text="ARTIST")
        self._sync_tree.column("status", width=100, stretch=False)
        self._sync_tree.column("title",  width=320, stretch=True)
        self._sync_tree.column("artist", width=220, stretch=True)
        syn_sb = tk.Scrollbar(res_f, command=self._sync_tree.yview,
                              bg=BG, troughcolor=BG, highlightthickness=0, bd=0, width=6)
        self._sync_tree.config(yscrollcommand=syn_sb.set)
        syn_sb.grid(row=0, column=1, sticky="ns")
        self._sync_tree.grid(row=0, column=0, sticky="nsew")
        self._sync_tree.tag_configure("new",   foreground=GREEN)
        self._sync_tree.tag_configure("have",  foreground=TEXT3)

    def _sync_load_csv(self):
        path = filedialog.askopenfilename(title="Select Exportify CSV",
                                          filetypes=[("CSV","*.csv"),("All","*.*")])
        if not path: return
        tracks, err = parse_exportify_csv(path)
        if err or not tracks:
            messagebox.showerror("Error", f"Could not read CSV: {err}"); return
        self._sync_csv_tracks = tracks
        self._sync_csv_var.set(f"{os.path.basename(path)}  ({len(tracks)} tracks)")

    def _sync_check(self):
        if self._sync_running:
            messagebox.showinfo("Running","Sync check already in progress."); return

        # Build set of known tracks (normalized)
        known = set()
        for tracks in self._library.values():
            for path in tracks:
                title, artist = split_track_name(path)
                key = re.sub(r'[\W_]','', f"{artist}{title}").lower()
                known.add(key)

        self._sync_status.config(text="Checking…", fg=TEXT3)
        self._sync_tree.delete(*self._sync_tree.get_children())
        self._sync_tracks = []
        self._sync_running = True

        def run():
            source_tracks = []
            # From YouTube URL
            yt_url = self._sync_yt_var.get().strip()
            if yt_url:
                try:
                    with yt_dlp.YoutubeDL({"quiet":True,"extract_flat":"in_playlist","ignoreerrors":True}) as ydl:
                        info = ydl.extract_info(yt_url, download=False)
                    for e in (info.get("entries") or []):
                        t = e.get("title","")
                        video_url = e.get("url") or e.get("webpage_url") or (f"https://www.youtube.com/watch?v={e.get('id')}" if e.get("id") else None)
                        if " - " in t:
                            p = t.split(" - ",1)
                            source_tracks.append({"artist":p[0].strip(),"title":p[1].strip(),"url":video_url})
                        else:
                            source_tracks.append({"artist":"","title":t,"url":video_url})
                except Exception as ex:
                    self.after(0, lambda: self._sync_status.config(text=f"YT error: {ex}", fg=RED))

            # From CSV
            if hasattr(self, '_sync_csv_tracks'):
                source_tracks.extend(self._sync_csv_tracks)

            def _update_sync_ui(new_t_count, t_objs):
                for t, key in t_objs:
                    if key in known:
                        self._sync_tree.insert("","end",tags=("have",),
                            values=("✓ Have", t['title'], t['artist']))
                    else:
                        self._sync_tree.insert("","end",tags=("new",),
                            values=("⬇ New", t['title'], t['artist']))
                        self._sync_tracks.append(t)
                
                if new_t_count > 0:
                    self._sync_status.config(text=f"{new_t_count} new tracks found!", fg=SUCCESS)
                    self._sync_dl_btn.config(fg=SIDEBAR, bg=GREEN)
                else:
                    self._sync_status.config(text="Library is up to date ✓", fg=SUCCESS)
                self._sync_running = False

            new_count = 0
            t_objs = []
            for t in source_tracks:
                key = re.sub(r'[\W_]','', f"{t['artist']}{t['title']}").lower()
                t_objs.append((t, key))
                if key not in known:
                    new_count += 1
            
            self.after(0, lambda: _update_sync_ui(new_count, t_objs))

        threading.Thread(target=run, daemon=True).start()

    def _sync_download(self):
        if not self._sync_tracks:
            messagebox.showinfo("Nothing to download", "Run a sync check first."); return
        dest = self.dl_dest_var.get() or MUSIC_ROOT
        os.makedirs(dest, exist_ok=True)
        self._sync_status.config(text=f"Downloading {len(self._sync_tracks)} tracks…", fg=WARNING)
        tracks = list(self._sync_tracks)

        def run():
            for i, t in enumerate(tracks):
                q = f"{t['artist']} {t['title']}"
                self.after(0, lambda idx=i, title=t['title']: self._sync_status.config(
                    text=f"[{idx+1}/{len(tracks)}] {title}", fg=WARNING))

                use_url = t.get("url")
                if self.sc_var.get():
                    sc_url, _ = probe_soundcloud(q)
                    if sc_url: use_url = sc_url
                if not use_url:
                    use_url = f"ytsearch1:{q}"

                safe = re.sub(r'[<>:"/\\|?*]', '', f"{t['artist']} - {t['title']}").strip()
                opts = {
                    "format": "bestaudio/best",
                    "outtmpl": os.path.join(dest, f"{safe}.%(ext)s"),
                    "postprocessors": [{
                        "key": "FFmpegExtractAudio",
                        "preferredcodec": "mp3",
                        "preferredquality": "192",
                    }],
                    "postprocessor_args": {
                        "FFmpegExtractAudio": [
                            "-ar", "44100",
                            "-ac", "2",
                            "-b:a", "192k",
                        ]
                    },
                    "writethumbnail": False,
                }
                try:
                    with yt_dlp.YoutubeDL(opts) as ydl: ydl.download([use_url])
                except: pass

            self.after(0, lambda: self._sync_status.config(
                text=f"Done! {len(tracks)} tracks downloaded.", fg=SUCCESS))
            self.after(0, self._refresh_library)
            self._sync_tracks = []

        threading.Thread(target=run, daemon=True).start()

    def _change_master_library(self):
        new_path = filedialog.askdirectory(title="Select New Master Music Folder")
        if not new_path: return
        global MUSIC_ROOT
        MUSIC_ROOT = new_path
        self._watch_roots = [MUSIC_ROOT]
        self.dl_dest_var.set(MUSIC_ROOT)
        self.master_path_lbl.config(text=MUSIC_ROOT)
        self._refresh_library()
        messagebox.showinfo("Success", f"Library moved to {new_path}")

    def _ctx_clear_playlist_contents(self, pl_name):
        """Deletes files inside the folder but keeps the folder."""
        tracks = self._library.get(pl_name, [])
        if not tracks: return
        confirm = messagebox.askyesno("Empty Playlist", f"Delete all {len(tracks)} songs in '{pl_name}'?\nThis cannot be undone.")
        if confirm:
            for path in tracks:
                try: 
                    if os.path.exists(path): os.remove(path)
                except: pass
            self._refresh_library()

    def _ctx_delete_playlist_folder(self, pl_name):
        """Deletes the files AND the folder."""
        folder_path = self._find_playlist_dir(pl_name)
        if not folder_path or pl_name in ["All Music", "Unsorted"]: return
        confirm = messagebox.askyesno("Delete Folder", f"Permanently delete folder '{pl_name}' and all songs inside?")
        if confirm:
            try:
                import shutil
                shutil.rmtree(folder_path)
                self._select_playlist_card(0, "All Music")
                self._refresh_library()
            except Exception as e:
                messagebox.showerror("Error", f"Failed: {e}")

    def _ctx_dump_playlist(self, source_pl_name):
        """Copies all songs from one playlist folder to another."""
        paths = self._library.get(source_pl_name, [])
        if not paths: return
        self._show_add_to_playlist_dialog(paths, f"Dump all from {source_pl_name}")   
        
    def _init_watchdog(self):
        """Starts the automatic background refresh loop."""
        # Only refresh if we aren't currently in the middle of a download
        if not self._is_dl:
            self._refresh_library()
        
        # Schedule the next run in 30 seconds
        self.after(30000, self._init_watchdog)
        


import signal
def signal_handler(sig, frame):
    pygame.mixer.quit()
    sys.exit(0)
signal.signal(signal.SIGINT, signal_handler)

if __name__ == "__main__":
    VibeMusicPlayer().mainloop()
