import unittest
import threading
import types
from collections import OrderedDict, deque
from unittest import mock

import music_player
from music_player import ArtCache, LunoMusicPlayer
from views.downloader import DownloadView


class ArtCacheTest(unittest.TestCase):
    def _cache(self, max_entries=10, max_pixels=100):
        cache = ArtCache.__new__(ArtCache)
        cache.cache = OrderedDict()
        cache._cache_pixels = 0
        cache._pending = OrderedDict()
        cache._callbacks = {}
        cache.MAX_ENTRIES = max_entries
        cache.MAX_PIXELS = max_pixels
        return cache

    def test_cache_evicts_old_art_by_pixel_budget(self):
        cache = self._cache()
        cache._store(("old.mp3", 10), "old", 100)
        cache._store(("new.mp3", 10), "new", 100)

        self.assertNotIn(("old.mp3", 10), cache.cache)
        self.assertEqual(cache.cache[("new.mp3", 10)][0], "new")
        self.assertEqual(cache._cache_pixels, 100)

    def test_cache_refreshes_lru_order_on_read(self):
        cache = self._cache(max_entries=2, max_pixels=1000)
        cache._store(("a.mp3", 10), "a", 100)
        cache._store(("b.mp3", 10), "b", 100)
        self.assertEqual(cache.get("a.mp3", 10, None), "a")
        cache._store(("c.mp3", 10), "c", 100)

        self.assertIn(("a.mp3", 10), cache.cache)
        self.assertNotIn(("b.mp3", 10), cache.cache)

    def test_search_art_cache_is_bounded(self):
        view = DownloadView.__new__(DownloadView)
        view._search_art_cache = OrderedDict()
        view.MAX_SEARCH_ART_CACHE = 2

        view._store_search_artwork("a", "a")
        view._store_search_artwork("b", "b")
        view._store_search_artwork("a", "a-new")
        view._store_search_artwork("c", "c")

        self.assertEqual(list(view._search_art_cache), ["a", "c"])
        self.assertEqual(view._search_art_cache["a"], "a-new")

    def test_art_requests_queue_at_inflight_limit(self):
        cache = self._cache(max_entries=10, max_pixels=1000)
        cache.MAX_INFLIGHT = 1
        cache.fetching = {("busy.mp3", 10)}
        cache.executor = mock.Mock()
        cache.app = types.SimpleNamespace(after=lambda delay, callback: None)

        self.assertIsNone(cache.get("queued.mp3", 10, lambda _: None))
        self.assertIn(("queued.mp3", 10), cache._pending)
        cache.executor.submit.assert_not_called()

        cache.fetching.clear()
        cache._drain()
        cache.executor.submit.assert_called_once_with(
            cache._fetch, "queued.mp3", 10
        )

    def test_prune_paths_releases_stale_artwork_and_colors(self):
        cache = self._cache(max_entries=10, max_pixels=1000)
        cache._color_cache = {
            "keep.mp3": "#111111",
            "gone.mp3": "#222222",
        }
        cache._store(("keep.mp3", 10), "keep", 100)
        cache._store(("gone.mp3", 10), "gone", 100)

        cache.prune_paths(["keep.mp3"])

        self.assertIn(("keep.mp3", 10), cache.cache)
        self.assertNotIn(("gone.mp3", 10), cache.cache)
        self.assertEqual(cache._cache_pixels, 100)
        self.assertEqual(cache._color_cache, {"keep.mp3": "#111111"})


class DownloadViewRetentionTest(unittest.TestCase):
    class Label:
        def __init__(self):
            self.values = {}

        def config(self, **kwargs):
            self.values.update(kwargs)

        def winfo_ismapped(self):
            return True

    class Text:
        def __init__(self):
            self.inserts = []
            self.deletes = []

        def config(self, **kwargs):
            pass

        def insert(self, *args):
            self.inserts.append(args)

        def delete(self, *args):
            self.deletes.append(args)

        def see(self, *args):
            pass

    def make_view(self):
        view = DownloadView.__new__(DownloadView)
        view._ui_thread = threading.current_thread()
        view._dl_status_after_id = None
        view._pending_dl_status = None
        view._progress_after_id = None
        view._pending_progress = None
        view._log_line_count = 0
        view._log_lock = threading.Lock()
        view._pending_logs = deque()
        view._log_flush_scheduled = False
        view._closing = False
        view.MAX_LOG_LINES = 3
        view.callbacks = []
        view.after = lambda delay, callback: (view.callbacks.append(callback) or len(view.callbacks))
        view.dl_status_lbl = self.Label()
        view._activity_row = self.Label()
        view._activity_lbl = self.Label()
        view.log_text = self.Text()
        view.controller = types.SimpleNamespace(
            prog_var=types.SimpleNamespace(values=[]),
        )
        view.controller.prog_var.set = view.controller.prog_var.values.append
        return view

    def test_status_and_progress_updates_are_coalesced(self):
        view = self.make_view()

        DownloadView._set_dl_status(view, "first")
        DownloadView._set_dl_status(view, "latest")
        self.assertEqual(len(view.callbacks), 1)
        view.callbacks.pop(0)()
        self.assertEqual(view.dl_status_lbl.values["text"], "latest")

        DownloadView._set_progress_value(view, 10)
        DownloadView._set_progress_value(view, 90)
        self.assertEqual(len(view.callbacks), 1)
        view.callbacks.pop(0)()
        self.assertEqual(view.controller.prog_var.values, [90])

    def test_download_log_keeps_only_recent_lines(self):
        view = self.make_view()

        for index in range(5):
            DownloadView._log(view, f"line {index}")

        self.assertEqual(view._log_line_count, 3)
        self.assertTrue(view.log_text.deletes)


class LibraryScanSafetyTest(unittest.TestCase):
    def test_refresh_requests_coalesce_while_scan_is_active(self):
        app = types.SimpleNamespace(
            _library_scan_state_lock=threading.Lock(),
            _library_scan_generation=0,
            _library_scan_active=False,
            _library_scan_pending=False,
            _library_scan_pending_reset=False,
            _watch_roots=["/tmp/music"],
            _run_library_scan_bg=lambda *args: None,
        )
        starts = []

        class FakeThread:
            def __init__(self, **kwargs):
                starts.append(kwargs)

            def start(self):
                pass

        with mock.patch.object(music_player.threading, "Thread", FakeThread):
            LunoMusicPlayer._refresh_library(app)
            LunoMusicPlayer._refresh_library(app, reset_track_scroll=True)

        self.assertEqual(len(starts), 1)
        self.assertTrue(app._library_scan_pending)
        self.assertTrue(app._library_scan_pending_reset)


if __name__ == "__main__":
    unittest.main()
