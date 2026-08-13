import requests

# Mobile LastfmService parity: reject Last.fm's default/no-image placeholder
# URLs so rows fall through to the Deezer lookup / abstract designs.
_LASTFM_PLACEHOLDER_MARKERS = (
    "/default",
    "noimage",
    "no_image",
)


def _pick_image(track):
    """Mobile LastfmService.pickLargestImage parity: prefer the largest usable
    image entry, normalized to https, dropping known placeholder artwork."""
    images = track.get("image") if isinstance(track, dict) else None
    if not isinstance(images, list):
        return ""
    by_size = {}
    first = ""
    for image in images:
        if not isinstance(image, dict):
            continue
        raw = (image.get("#text") or image.get("url") or "").strip()
        url = _normalize_image_url(raw)
        if not url:
            continue
        first = first or url
        by_size[image.get("size", "")] = url
    for size in ("mega", "extralarge", "large", "medium", "small"):
        if by_size.get(size):
            return by_size[size]
    return first


def _normalize_image_url(raw_url):
    url = raw_url.strip()
    if url.startswith("http://", 0, 7):
        url = "https://" + url[7:]
    elif not url.startswith("https://", 0, 8):
        return ""
    lowered = url.lower()
    is_lastfm_host = "last.fm" in lowered or "lastfm.freetls.fastly.net" in lowered
    if is_lastfm_host and any(marker in lowered for marker in _LASTFM_PLACEHOLDER_MARKERS):
        return ""
    return url


class DiscoveryService:
    """Fetch similar tracks from Last.fm with a bounded in-memory cache."""

    def __init__(self, api_key=None, logger=None):
        self.api_key = api_key
        self.logger = logger
        self.cache = {}

    def get_similar(self, artist, title, limit=20):
        if not self.api_key or not artist or not title:
            return []

        # Keyed by seed + requested result count (mobile parity: the Android
        # cache also stores per (seed, limit), so "load 25 more" can grow the
        # list instead of returning the earlier smaller page).
        cache_key = (artist.lower().strip(), title.lower().strip(), limit)
        if cache_key in self.cache:
            return self.cache[cache_key]

        url = "https://ws.audioscrobbler.com/2.0/"
        params = {
            "method": "track.getsimilar",
            "artist": artist,
            "track": title,
            "api_key": self.api_key,
            "format": "json",
            "limit": limit,
        }
        results = []
        try:
            resp = requests.get(url, params=params, timeout=8)
            resp.raise_for_status()
            data = resp.json()

            if "error" not in data:
                tracks_data = data.get("similartracks", {}).get("track", [])
                if not isinstance(tracks_data, list):
                    tracks_data = [tracks_data] if tracks_data else []
                for track in tracks_data:
                    results.append({
                        "artist": track.get("artist", {}).get("name")
                        if isinstance(track.get("artist"), dict)
                        else track.get("artist"),
                        "title": track.get("name"),
                        "match": float(track.get("match", 0)),
                        "image": _pick_image(track),
                    })

            if not results:
                if self.logger:
                    self.logger(
                        "Discovery",
                        f"No similar tracks for '{title}'. Fetching popular tracks by {artist}...",
                    )
                params["method"] = "artist.gettoptracks"
                del params["track"]
                resp = requests.get(url, params=params, timeout=8)
                resp.raise_for_status()
                data = resp.json()
                if "error" not in data:
                    tracks_data = data.get("toptracks", {}).get("track", [])
                    if not isinstance(tracks_data, list):
                        tracks_data = [tracks_data] if tracks_data else []
                    for track in tracks_data:
                        results.append({
                            "artist": track.get("artist", {}).get("name")
                            if isinstance(track.get("artist"), dict)
                            else track.get("artist"),
                            "title": track.get("name"),
                            "match": 0.8,
                            "image": _pick_image(track),
                        })

            if len(self.cache) > 10:
                self.cache.clear()
            self.cache[cache_key] = results
            return results
        except Exception as exc:
            if self.logger:
                self.logger("Discovery", f"Request Failed: {exc}")
            return []
