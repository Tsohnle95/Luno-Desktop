import os
import tempfile
import threading
import unittest
from unittest import mock

import downloader
from downloader import Downloader


class FakeYoutubeDL:
    playlist_info = {
        "title": "Synthwave",
        "entries": [
            {
                "id": "obsidian123",
                "title": "Obsidian",
                "url": "https://www.youtube.com/watch?v=obsidian123",
            }
        ],
    }

    def __init__(self, opts):
        self.opts = opts

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, tb):
        return False

    def extract_info(self, url, download=False):
        return self.playlist_info


class FakeYtDlp:
    YoutubeDL = FakeYoutubeDL


class PlaylistSyncRegressionTest(unittest.TestCase):
    def run_sync(self, out_folder, download_calls, create_file=True, download_error=None):
        done = threading.Event()
        logs = []
        messages = []

        def fake_download(opts, url, timeout=120):
            download_calls.append(url)
            if download_error:
                raise download_error
            if create_file:
                outtmpl = opts["outtmpl"]
                path = outtmpl.replace("%(ext)s", "mp3")
                with open(path, "w", encoding="utf-8") as f:
                    f.write("fake audio")

        with mock.patch.object(downloader, "yt_dlp", FakeYtDlp), \
             mock.patch.object(downloader, "_download_with_timeout", fake_download):
            Downloader.download_playlist_sync(
                "https://example.test/playlist",
                out_folder,
                skip_existing=True,
                try_sc=False,
                on_progress=lambda idx, total, title: None,
                on_log=lambda title, is_success, err: logs.append((title, is_success, err)),
                on_complete=done.set,
                check_cancel=lambda: False,
                on_playlist_msg=messages.append,
            )
            self.assertTrue(done.wait(2), "sync did not complete")

        return logs, messages

    def test_second_sync_excludes_already_downloaded_items(self):
        with tempfile.TemporaryDirectory() as tmp:
            download_calls = []

            first_logs, first_messages = self.run_sync(tmp, download_calls)
            self.assertEqual(download_calls, ["https://www.youtube.com/watch?v=obsidian123"])
            self.assertEqual(first_logs, [("Obsidian", True, None)])
            self.assertIn("Found 1 new track(s) out of 1 total", first_messages)
            self.assertIn("Downloading 1/1: Obsidian", first_messages)
            self.assertTrue(os.path.exists(os.path.join(tmp, ".downloaded_vids")))

            second_logs, second_messages = self.run_sync(tmp, download_calls)
            self.assertEqual(download_calls, ["https://www.youtube.com/watch?v=obsidian123"])
            self.assertEqual(second_logs, [])
            self.assertIn("Searched 1 track(s) — up to date", second_messages)
            self.assertNotIn("Downloading 1/1: Obsidian", second_messages)

    def test_legacy_download_archive_excludes_items(self):
        with tempfile.TemporaryDirectory() as tmp:
            with open(os.path.join(tmp, ".downloaded.txt"), "w", encoding="utf-8") as f:
                f.write("youtube obsidian123\n")

            download_calls = []
            logs, messages = self.run_sync(tmp, download_calls)

            self.assertEqual(download_calls, [])
            self.assertEqual(logs, [])
            self.assertIn("Searched 1 track(s) — up to date", messages)

    def test_existing_sibling_playlist_track_is_not_redownloaded(self):
        with tempfile.TemporaryDirectory() as tmp:
            out_folder = os.path.join(tmp, "Vibin")
            sibling = os.path.join(tmp, "Mixes")
            os.makedirs(out_folder)
            os.makedirs(sibling)
            with open(os.path.join(sibling, "Obsidian.mp3"), "w", encoding="utf-8") as f:
                f.write("fake audio")

            download_calls = []
            logs, messages = self.run_sync(out_folder, download_calls)

            self.assertEqual(download_calls, [])
            self.assertEqual(logs, [])
            self.assertIn("Searched 1 track(s) — up to date", messages)
            self.assertTrue(os.path.exists(os.path.join(out_folder, ".downloaded_vids")))

    def test_success_requires_created_audio_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            download_calls = []
            logs, messages = self.run_sync(tmp, download_calls, create_file=False)

            self.assertEqual(download_calls, ["https://www.youtube.com/watch?v=obsidian123"])
            self.assertEqual(len(logs), 1)
            self.assertEqual(logs[0][0], "Obsidian")
            self.assertFalse(logs[0][1])
            self.assertIn("no audio file was created", logs[0][2])
            self.assertFalse(os.path.exists(os.path.join(tmp, ".downloaded_vids")))

    def test_permanent_download_failure_is_not_retried(self):
        with tempfile.TemporaryDirectory() as tmp:
            download_calls = []
            err = Exception("ERROR: [youtube] obsidian123: Video unavailable. This video is not available")

            first_logs, first_messages = self.run_sync(tmp, download_calls, download_error=err)
            self.assertEqual(download_calls, ["https://www.youtube.com/watch?v=obsidian123"])
            self.assertEqual(len(first_logs), 1)
            self.assertFalse(first_logs[0][1])
            self.assertIn("Video unavailable", first_logs[0][2])
            self.assertTrue(os.path.exists(os.path.join(tmp, ".sync_failed_vids")))

            second_logs, second_messages = self.run_sync(tmp, download_calls)
            self.assertEqual(download_calls, ["https://www.youtube.com/watch?v=obsidian123"])
            self.assertEqual(second_logs, [])
            self.assertIn("Skipped 1 previously failed unavailable item(s)", second_messages)
            self.assertIn("Searched 1 track(s) — up to date", second_messages)
            self.assertNotIn("Downloading 1/1: Obsidian", second_messages)


if __name__ == "__main__":
    unittest.main()
