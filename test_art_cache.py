import unittest
from collections import OrderedDict

from music_player import ArtCache
from views.downloader import DownloadView


class ArtCacheTest(unittest.TestCase):
    def _cache(self, max_entries=10, max_pixels=100):
        cache = ArtCache.__new__(ArtCache)
        cache.cache = OrderedDict()
        cache._cache_pixels = 0
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


if __name__ == "__main__":
    unittest.main()
