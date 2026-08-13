import unittest
from unittest.mock import patch

import video_preview


class _FakeYDL:
    def __init__(self, options):
        self.options = options

    def __enter__(self):
        return self

    def __exit__(self, *args):
        return False

    def extract_info(self, query, download=False):
        return {
            "entries": [
                {"id": "wrong", "title": "Palm Tree (unrelated remix)",
                 "uploader": "Other Artist"},
                {"id": "right", "title": "Prod by the Architect - Palm Tree",
                 "uploader": "Palm Tree"},
            ]
        }


class VideoPreviewTests(unittest.TestCase):
    def test_resolver_rejects_unrelated_artist_and_uses_canonical_url(self):
        fake_module = type("YtDlp", (), {"YoutubeDL": _FakeYDL})
        with patch.object(video_preview, "yt_dlp", fake_module):
            result = video_preview.resolve_video_source(
                "Palm Tree", "Prod by the Architect")
        self.assertEqual(result["url"], "https://www.youtube.com/watch?v=right")

    def test_resolver_rejects_results_without_title_match(self):
        class NoMatchYDL(_FakeYDL):
            def extract_info(self, query, download=False):
                return {"entries": [{"id": "wrong", "title": "Another Song",
                                      "uploader": "Palm Tree"}]}

        fake_module = type("YtDlp", (), {"YoutubeDL": NoMatchYDL})
        with patch.object(video_preview, "yt_dlp", fake_module):
            with self.assertRaises(video_preview.VideoPreviewError):
                video_preview.resolve_video_source("Palm Tree", "Prod by the Architect")


if __name__ == "__main__":
    unittest.main()
