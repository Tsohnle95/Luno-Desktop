"""Recommendation row artwork support (mobile RecommendationArtworkService +
RecommendationArtwork parity): cached Deezer cover lookups for recommendations
without usable Last.fm artwork, plus 75 deterministic code-drawn abstract
designs for loading/missing/error states.

The design index is derived from a stable hash of the normalized
artist - title key (mobile: hashCode(normalizedKey) mod 75), so every
recommendation identity always renders the same cover art.
"""

import io
import threading
import zlib
from collections import OrderedDict

import requests

DESIGN_COUNT = 75
_DEEZER_TIMEOUT_S = 8
_IMAGE_TIMEOUT_S = 8
_UA = "Luno/1.0 (desktop)"
_MAX_DEZ_CACHE = 200
_DEZ_CACHE = OrderedDict()
_DEZ_LOCK = threading.Lock()
_MAX_IMAGE_PIXELS = 4 * 1024 * 1024
_MAX_IMAGE_BYTES = 4 * 1024 * 1024

# Deterministic index-derived palettes (dark base, accent pair).
_DESIGN_PALETTES = [
    ((0x12, 0x18, 0x16), (0x1E, 0xD7, 0x60)),
    ((0x1A, 0x12, 0x18), (0x8A, 0x5C, 0xE8)),
    ((0x12, 0x16, 0x1E), (0x4F, 0xA8, 0xFF)),
    ((0x1C, 0x16, 0x10), (0xFF, 0x9F, 0x43)),
    ((0x16, 0x1A, 0x1E), (0x4F, 0xD8, 0xD0)),
    ((0x1E, 0x14, 0x14), (0xFF, 0x6B, 0x6B)),
    ((0x14, 0x1A, 0x14), (0x9B, 0xE8, 0x5C)),
    ((0x16, 0x14, 0x1C), (0xD8, 0x8F, 0xFF)),
    ((0x14, 0x1A, 0x1C), (0x6B, 0xD0, 0xFF)),
    ((0x1C, 0x1A, 0x14), (0xFF, 0xD0, 0x6B)),
    ((0x18, 0x12, 0x18), (0xFF, 0x77, 0xC8)),
    ((0x12, 0x18, 0x1A), (0x77, 0xFF, 0xC8)),
]


def design_index(key):
    """Stable (cross-process) deterministic index for a normalized key."""
    return zlib.crc32(key.encode("utf-8")) % DESIGN_COUNT


def render_design(index, size):
    """Mobile RecommendationArtwork parity: a deterministic abstract design
    derived from the index — a two-tone gradient plus one of several simple
    geometric overlays — returned as a square PIL image."""
    from PIL import Image, ImageDraw

    c1, c2 = _DESIGN_PALETTES[index % len(_DESIGN_PALETTES)]
    img = Image.new("RGB", (size, size))
    px = img.load()
    denom = max(size - 1, 1)
    for y in range(size):
        t = y / denom
        r = int(c1[0] + (c2[0] - c1[0]) * t)
        g = int(c1[1] + (c2[1] - c1[1]) * t)
        b = int(c1[2] + (c2[2] - c1[2]) * t)
        for x in range(size):
            px[x, y] = (r, g, b)

    draw = ImageDraw.Draw(img, "RGBA")
    accent = c2 + (90,)
    dim = c1 + (120,)
    kind = index % 6
    if kind == 0:
        # Concentric rings
        step = max(size // 8, 2)
        for i in range(1, 9):
            radius = i * step
            draw.ellipse((size / 2 - radius, size / 2 - radius,
                          size / 2 + radius, size / 2 + radius),
                         outline=accent if i % 2 else dim, width=2)
    elif kind == 1:
        # Offset disc
        cx = size * ((index // 6) % 7) / 7
        cy = size * ((index // 42) % 5) / 5
        draw.ellipse((cx - size / 3, cy - size / 3, cx + size / 3, cy + size / 3),
                     fill=accent)
    elif kind == 2:
        # Horizontal bars
        bars = 3 + (index // 6) % 4
        thickness = max(size // (bars * 3), 2)
        for i in range(bars):
            y = (i + 0.5) * size / bars
            draw.rectangle((0, y - thickness / 2, size, y + thickness / 2),
                           fill=accent if i % 2 == 0 else dim)
    elif kind == 3:
        # Diagonal band
        width = max(size // 3, 4)
        draw.polygon([(size / 2 - width, 0), (size / 2 + width, 0),
                      (size / 2 - width, size), (size / 2 - 3 * width, size)],
                     fill=accent)
    elif kind == 4:
        # Dot grid
        dots = 3 + (index // 6) % 3
        for i in range(dots):
            for j in range(dots):
                cx = (i + 0.5) * size / dots
                cy = (j + 0.5) * size / dots
                radius = max(size / (dots * 6), 2)
                draw.ellipse((cx - radius, cy - radius, cx + radius, cy + radius),
                             fill=accent if (i + j) % 2 == 0 else dim)
    else:
        # Corner triangles
        tri = size // 2
        draw.polygon([(0, 0), (tri, 0), (0, tri)], fill=accent)
        draw.polygon([(size, size), (size - tri, size), (size, size - tri)],
                     fill=dim)
    return img


def find_artwork(artist, title):
    """Mobile RecommendationArtworkService.findArtwork parity: Deezer cover
    search with an in-memory cache (nulls cached too).

    The mobile 'artist:"X" track:"Y"' field query currently returns no
    results from the Deezer API, so the search uses quoted terms with a
    plain-text fallback attempt.
    """
    if not artist or not title:
        return None
    key = f"{artist.strip().lower()}|{title.strip().lower()}"
    with _DEZ_LOCK:
        if key in _DEZ_CACHE:
            _DEZ_CACHE.move_to_end(key)
            return _DEZ_CACHE[key]
    cover = None
    try:
        for query in (f'"{artist}" "{title}"', f"{artist} {title}"):
            resp = requests.get(
                "https://api.deezer.com/search",
                params={"q": query, "limit": 1},
                headers={"User-Agent": _UA},
                timeout=_DEEZER_TIMEOUT_S,
            )
            resp.raise_for_status()
            data = resp.json().get("data") or []
            if not data:
                continue
            album = (data[0].get("album") or {})
            cover = (album.get("cover_xl") or album.get("cover_big")
                     or album.get("cover_medium") or "").strip() or None
            if cover and cover.startswith("http://", 0, 7):
                cover = "https://" + cover[7:]
            break
    except Exception:
        cover = None
    with _DEZ_LOCK:
        _DEZ_CACHE[key] = cover
        _DEZ_CACHE.move_to_end(key)
        while len(_DEZ_CACHE) > _MAX_DEZ_CACHE:
            _DEZ_CACHE.popitem(last=False)
    return cover


def load_image(url, size=96):
    """Downloads an image URL, center-crops it to a square, and downsamples it
    to at most `size` pixels. Returns a PIL image or None (never raises)."""
    if not url:
        return None
    try:
        resp = requests.get(url, headers={"User-Agent": _UA},
                            timeout=_IMAGE_TIMEOUT_S)
        resp.raise_for_status()
        if len(resp.content) > _MAX_IMAGE_BYTES:
            return None
        from PIL import Image
        with Image.open(io.BytesIO(resp.content)) as src:
            src.load()
            if src.width * src.height > _MAX_IMAGE_PIXELS:
                return None
            image = src.convert("RGB")
        w, h = image.size
        side = min(w, h)
        left = (w - side) // 2
        top = (h - side) // 2
        cropped = image.crop((left, top, left + side, top + side))
        image.close()
        if max(cropped.size) > size:
            cropped.thumbnail((size, size), Image.Resampling.LANCZOS)
        return cropped
    except Exception:
        return None
