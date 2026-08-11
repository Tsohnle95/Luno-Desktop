import requests


class DiscoveryService:
    """Fetch similar tracks from Last.fm with a bounded in-memory cache."""

    def __init__(self, api_key=None, logger=None):
        self.api_key = api_key
        self.logger = logger
        self.cache = {}

    def get_similar(self, artist, title, limit=20):
        if not self.api_key or not artist or not title:
            return []

        cache_key = (artist.lower().strip(), title.lower().strip())
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
                        })

            if len(self.cache) > 10:
                self.cache.clear()
            self.cache[cache_key] = results
            return results
        except Exception as exc:
            if self.logger:
                self.logger("Discovery", f"Request Failed: {exc}")
            return []
