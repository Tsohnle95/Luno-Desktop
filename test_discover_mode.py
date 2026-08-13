"""Deterministic unit tests for desktop Discover mode / recommendation
previews (mobile RecommendationPreviewManager parity): temporary preview
staging (never the library), passive first-five prefetch, tap-to-preview
sessions with rolling-window refill, resolve/download states and timeouts,
Next/Previous session routing, Discover deferred block + auto-extend,
promotion ("Add to Library") with queued-save observation, and session
teardown/handoff cleanup.

These tests bind the controller methods onto a lightweight stub so no Tk
window is needed; the resolve/download pipeline is stubbed synchronously
(real pipeline = previews.resolve_preview_source / download_preview) and
previews are staged in a per-test temp directory standing in for PREVIEW_DIR.
"""
import os
import shutil
import tempfile
import threading
import time
import types
import unittest
from unittest import mock
from concurrent.futures import ThreadPoolExecutor

import previews
from music_player import LunoMusicPlayer

SEED = "/tmp/fake_lib/Seed.mp3"
CONT1 = "/tmp/fake_lib/Continuation 1.mp3"
CONT2 = "/tmp/fake_lib/Continuation 2.mp3"

RECS = [
    {"artist": f"Artist{i}", "title": f"Title{i}"}
    for i in range(10)
]

DISCOVER_METHODS = (
    "_discover_set",
    "_discover_prefetch",
    "_discover_key",
    "_discover_chain_start",
    "_discover_chain_stop",
    "_discover_notify_ui",
    "_discover_refill",
    "_discover_append_recs",
    "_discover_retry",
    "_discover_existing_download",
    "_discover_prepare_next",
    "_discover_mark_waiting_band",
    "_discover_fail_preview",
    "_discover_start_resolve",
    "_discover_start_preview_download",
    "_discover_download_done",
    "_discover_track_ready",
    "_discover_start_session",
    "_discover_restart_session",
    "_discover_jump_into_preview",
    "_discover_leave_block",
    "_discover_restart_window_after_current",
    "_rec_seed_for",
    "_discover_adopt_ready",
    "_discover_insert_path",
    "_discover_consumed",
    "_discover_consider_handoff",
    "_discover_end_session",
    "_discover_teardown",
    "_discover_cleanup_files",
    "_discover_delete_files",
    "_discover_deferred_flush",
    "_discover_promote",
    "_discover_promote_staged",
    "_discover_promote_done",
    "_discover_download_new",
    "_discover_download_observed",
    "_discover_play_rec",
    "_discover_row_state",
    "_discover_ready_count",
    "_discover_busy_count",
    "_discover_chain_done",
    "_discover_session_seed",
    "_is_discover_path",
    "_discover_current_path",
    "_discover_key_for_path",
    "_discover_current_key",
    "_next_queued_is_preview",
    "_discover_preparing",
    "_route_session_next",
    "_route_session_prev",
    "_user_next",
    "_user_prev",
    "_on_playback_failed",
    "_discover_maybe_auto_extend",
    "_find_downloaded_audio",
    "_on_track_changed_discovery",
    "_download_track",
)


class FakeStatus:
    def __init__(self):
        self.value = ""

    def set(self, value):
        self.value = value


class FakeEngine:
    def __init__(self, queue, queue_idx):
        self.queue = list(queue)
        self.queue_idx = queue_idx
        self.playing = True
        self.paused = False
        self.mixer_ready = True

    def play_current(self):
        if 0 <= self.queue_idx < len(self.queue):
            self.playing = True

    def next_track(self):
        self.queue_idx = min(self.queue_idx + 1, max(len(self.queue) - 1, 0))
        self.play_current()

    def prev_track(self):
        self.queue_idx = max(self.queue_idx - 1, 0)
        self.play_current()


def make_controller(queue, queue_idx, music_root, preview_dir):
    c = types.SimpleNamespace()
    c.engine = FakeEngine(queue, queue_idx)
    c.status_var = FakeStatus()
    c._closing = False
    c._discovery_downloads = 0
    c._discover_active = False
    c._discover_seed = None
    c._discover_chain_running = False
    c._discover_chain_gen = 0
    c._discover_physical_queue = None
    c._discover_physical_idx = None
    c._discover_paths = []
    c._discover_recs = []
    c._discover_rec_pos = 0
    c._discover_states = {}
    c._discover_retry_queue = []
    c._discover_insert_pos = 0
    c._discover_dest = None
    c._discover_downloading = False
    c._discover_deferred_delete = []
    c._discover_discover_mode = False
    c._discover_window_start = 0
    c._discover_window_end = 0
    c._discover_first_play_pending = False
    c._discover_advance_armed = False
    c._discover_saving_keys = set()
    c._discover_saved_uris = {}
    c._discover_queued_keys = {}
    c._discover_save_failures = {}
    c._preview_resolver_pool = ThreadPoolExecutor(max_workers=2)
    c._frames = {}
    c._recently_played = []
    c.dl_dest_var = types.SimpleNamespace(get=lambda: "")
    c.MUSIC_ROOT = music_root
    c.skip_var = types.SimpleNamespace(get=lambda: True)
    c.sc_var = types.SimpleNamespace(get=lambda: True)
    c.auto_discovery_var = types.SimpleNamespace(get=lambda: False)
    c.DISCOVER_IMMEDIATE_COUNT = LunoMusicPlayer.DISCOVER_IMMEDIATE_COUNT
    c.PREVIEW_EXTS = LunoMusicPlayer.PREVIEW_EXTS
    c.PREVIEW_DIR = preview_dir
    c._update_queue = lambda: None
    c._log_error = lambda *a, **kw: None
    c._refresh_library = lambda **kw: None
    c._trigger_discovery = lambda *a, **kw: None
    c._after_queue = []
    c.after = lambda delay, fn: c._after_queue.append((delay, fn))
    import inspect
    for name in DISCOVER_METHODS:
        raw = inspect.getattr_static(LunoMusicPlayer, name)
        if isinstance(raw, staticmethod):
            setattr(c, name, raw.__func__)
        else:
            setattr(c, name, raw.__get__(c, LunoMusicPlayer))
    return c


def safe_name(artist, title):
    import re
    return re.sub(r'[<>:"/\\|?*]', '', f"{artist} - {title}").strip() or "Unknown"


def staged_path(preview_dir, artist, title, ext=".mp3"):
    return os.path.join(preview_dir, f"{safe_name(artist, title)}{ext}")


def make_resolve_stub(c, resolve_err=None):
    """Synchronous stand-in for _discover_start_resolve: flips the row to
    preparing and hands off to the (also stubbed) download step."""
    def do_resolve(key, st, gen):
        if resolve_err is not None:
            st["status"] = "failed"
            st["error"] = str(resolve_err)
            c._discover_downloading = False
            c._discover_notify_ui()
            c._discover_refill()
            return
        st["source"] = {"url": "https://example.com/watch?v=abc",
                        "thumbnail": ""}
        st["status"] = "preparing"
        c._discover_notify_ui()
        c._discover_start_preview_download(key, st, gen)
    c._discover_start_resolve = do_resolve


def make_download_stub(c, fail_title=None, fail_err=None, hold=None):
    """Synchronous stand-in for _discover_start_preview_download. Writes the
    staged mp3 and completes through _discover_download_done, optionally
    failing one title or holding the download in flight."""
    def do_download(key, st, gen):
        if hold is not None:
            hold.update(key=key, st=st, gen=gen)
            return
        artist = st["rec"].get("artist", "")
        title = st["rec"].get("title", "")
        if fail_title is not None and title == fail_title:
            c._discover_download_done(
                key, st, gen, None,
                fail_err if fail_err is not None
                else previews.PreviewError("no audio"))
            return
        path = staged_path(c.PREVIEW_DIR, artist, title)
        with open(path, "wb") as fh:
            fh.write(b"fake")
        c._discover_download_done(key, st, gen, path, None)
    c._discover_start_preview_download = do_download


def wire_stubs(c, fail_title=None, fail_err=None, hold=None, resolve_err=None):
    make_resolve_stub(c, resolve_err=resolve_err)
    make_download_stub(c, fail_title=fail_title, fail_err=fail_err, hold=hold)


class DiscoverModeTest(unittest.TestCase):
    def setUp(self):
        self.dest = tempfile.mkdtemp(prefix="luno_lib_")
        self.preview = tempfile.mkdtemp(prefix="luno_preview_")
        self.download_calls = []

        def fake_download_sync(tracks, out_folder, skip_existing, try_sc,
                               on_progress, on_log, on_complete, check_cancel):
            self.download_calls.append(list(tracks))
            for t in tracks:
                name = safe_name(t.get("artist", ""), t.get("title", ""))
                with open(os.path.join(out_folder, f"{name}.mp3"), "wb") as f:
                    f.write(b"fake")
                on_progress(0, 1, t.get("title", ""))
                on_log(t.get("title", ""), True, None)
            on_complete()

        self._patch_dl = mock.patch.object(
            __import__("downloader").Downloader, "download_sync_tracks",
            side_effect=fake_download_sync,
        )
        self._patch_dl.start()

    def tearDown(self):
        self._patch_dl.stop()
        shutil.rmtree(self.dest, ignore_errors=True)
        shutil.rmtree(self.preview, ignore_errors=True)

    def _c(self, queue=(SEED,), idx=0):
        return make_controller(list(queue), idx, self.dest, self.preview)

    def _drain(self, c):
        while c._after_queue:
            delay, fn = c._after_queue.pop(0)
            if delay == 0:
                fn()

    def _prefetch(self, c, recs=None, seed=("Artist0", "Title0")):
        c._discover_prefetch(seed, recs if recs is not None else RECS)
        self._drain(c)

    def _toggle_on(self, c):
        c._discover_set(True, list(c._discover_recs))
        self._drain(c)

    def _toggle_off(self, c):
        c._discover_set(False)
        self._drain(c)

    def _staged_files(self):
        return sorted(os.listdir(self.preview))

    # ── Passive prefetch ────────────────────────────────────────────────────

    def test_prefetch_prepares_first_five_into_staging(self):
        c = self._c()
        wire_stubs(c)
        self._prefetch(c)

        self.assertEqual(c._discover_active, False)
        self.assertEqual(c.engine.queue, [SEED])
        self.assertEqual(c.engine.queue_idx, 0)
        self.assertEqual(len(self._staged_files()), 5)
        # Every row state resolves to ready with a staged path.
        for i in range(5):
            st = c._discover_row_state(c._discover_key(RECS[i]))
            self.assertEqual(st["status"], "ready")
            self.assertTrue(st["path"].startswith(self.preview))
        self.assertEqual(c._discover_ready_count(), 5)

    def test_prefetch_window_releases_watchdog(self):
        c = self._c()
        wire_stubs(c)
        self._prefetch(c)
        # Passive window full -> chain released (watchdog quiet again).
        self.assertEqual(c._discover_chain_running, False)
        self.assertEqual(c._discovery_downloads, 0)

    def test_prefetch_never_touches_the_library(self):
        c = self._c()
        wire_stubs(c)
        self._prefetch(c)
        self.assertEqual(os.listdir(self.dest), [])

    def test_prefetch_skips_already_staged_files(self):
        c = self._c()
        wire_stubs(c)
        self._prefetch(c)
        staged_before = len(self._staged_files())
        calls_before = len(self.download_calls)
        c._discover_prefetch(("Artist0", "Title0"), RECS)
        self._drain(c)
        self.assertEqual(len(self.download_calls), calls_before)
        self.assertEqual(len(self._staged_files()), staged_before)
        self.assertEqual(c._discover_ready_count(), 5)

    def test_prefetch_marks_waiting_rows_queued(self):
        c = self._c()
        wire_stubs(c)
        hold = {}
        make_download_stub(c, hold=hold)
        c._discover_prefetch(("Artist0", "Title0"), RECS)
        # While row 0 is downloading, rows 1-4 show "Queued for temporary
        # preview" (mobile prefetchQueuedKeys parity).
        st = c._discover_row_state(c._discover_key(RECS[1]))
        self.assertEqual(st["status"], "waiting")
        # Complete the held download -> the whole window fills. The chain
        # continues synchronously, so release the hold first.
        path = staged_path(self.preview, "Artist0", "Title0")
        with open(path, "wb") as fh:
            fh.write(b"fake")
        make_download_stub(c)
        c._discover_download_done(hold["key"], hold["st"], hold["gen"],
                                  path, None)
        self._drain(c)
        self.assertEqual(c._discover_ready_count(), 5)
        # Nothing waits behind a full window.
        self.assertIsNone(c._discover_row_state(c._discover_key(RECS[5])))

    def test_prefetch_resolve_failure_shows_retryable_message(self):
        c = self._c()
        wire_stubs(c, resolve_err=previews.PreviewTimeoutError(
            previews.PREVIEW_RESOLVE_TIMED_OUT_MESSAGE))
        c._discover_prefetch(("Artist0", "Title0"), RECS[:1])
        self._drain(c)
        st = c._discover_row_state(c._discover_key(RECS[0]))
        self.assertEqual(st["status"], "failed")
        self.assertEqual(st["error"], previews.PREVIEW_RESOLVE_TIMED_OUT_MESSAGE)

    def test_prefetch_download_timeout_maps_message(self):
        c = self._c()
        wire_stubs(c, fail_title="Title0", fail_err=previews.PreviewTimeoutError(
            previews.PREVIEW_DOWNLOAD_TIMED_OUT_MESSAGE))
        c._discover_prefetch(("Artist0", "Title0"), RECS[:5])
        self._drain(c)
        st = c._discover_row_state(c._discover_key(RECS[0]))
        self.assertEqual(st["status"], "failed")
        self.assertEqual(st["error"], previews.PREVIEW_DOWNLOAD_TIMED_OUT_MESSAGE)

    # ── Discover mode ON ─────────────────────────────────────────────────────

    def test_discover_on_adopts_ready_block_into_queue(self):
        c = self._c()
        wire_stubs(c)
        self._prefetch(c)
        self._toggle_on(c)

        self.assertEqual(c._discover_active, True)
        self.assertEqual(c._discover_discover_mode, True)
        self.assertEqual(len(c._discover_paths), 5)
        expected = [SEED] + c._discover_paths
        self.assertEqual(c.engine.queue, expected)
        self.assertEqual(c.engine.queue_idx, 0)
        # Window full: no extra downloads after adoption.
        self.assertEqual(len(self._staged_files()), 5)

    def test_discover_on_without_prefetch_starts_chain(self):
        c = self._c()
        wire_stubs(c)
        c._discover_set(True, RECS)
        self._drain(c)
        self.assertEqual(c._discover_active, True)
        self.assertEqual(len(c._discover_paths), 5)

    def test_rolling_window_refills_as_playback_advances(self):
        c = self._c()
        wire_stubs(c)
        self._prefetch(c)
        self._toggle_on(c)
        self.assertEqual(len(self._staged_files()), 5)

        # Block track 2 starts (consumed=2): two more downloads (2+5=7).
        c.engine.queue_idx = 2
        c._discover_consider_handoff()
        c._discover_refill()
        self._drain(c)
        self.assertEqual(len(c._discover_paths), 7)
        self.assertEqual(len(self._staged_files()), 7)
        self.assertEqual(c._discover_active, True)

        # Block track 4 (consumed=4): one more (4+5=9).
        c.engine.queue_idx = 4
        c._discover_consider_handoff()
        c._discover_refill()
        self._drain(c)
        self.assertEqual(len(c._discover_paths), 9)

    def test_exhausted_list_stops_and_reports(self):
        c = self._c()
        wire_stubs(c)
        self._prefetch(c, recs=RECS[:5])
        self._toggle_on(c)
        self.assertEqual(len(c._discover_paths), 5)
        self.assertIn("all recommendations downloaded", c.status_var.value)

    # ── Tap-to-preview sessions (non-Discover) ──────────────────────────────

    def test_tap_unprepared_row_resolves_on_demand_and_plays(self):
        c = self._c()
        wire_stubs(c)
        self._prefetch(c)
        # Row 7 sits beyond the five prefetched rows — it must resolve and
        # download on demand (mobile startPreview parity).
        key = c._discover_key(RECS[7])
        c._discover_play_rec(key)
        self._drain(c)

        self.assertEqual(c._discover_active, True)
        self.assertEqual(c._discover_discover_mode, False)
        st = c._discover_row_state(key)
        self.assertEqual(st["status"], "ready")
        # The tapped row jumped into playback immediately (mobile
        # playSequential parity).
        self.assertEqual(c.engine.queue_idx, 1)
        self.assertEqual(c.engine.queue[1], st["path"])
        # Window = tapped + the next five (capped by the list length).
        self.assertEqual(len(c._discover_paths), 3)
        self.assertEqual(len(self._staged_files()), 3)
        # Prefetched rows outside the new window were cleaned up.
        self.assertNotIn("Artist0 - Title0.mp3", self._staged_files())

    def test_tap_adopted_row_plays_instantly_without_download(self):
        c = self._c()
        wire_stubs(c)
        self._prefetch(c)
        staged_before = len(self._staged_files())
        key = c._discover_key(RECS[2])
        c._discover_play_rec(key)
        self._drain(c)

        self.assertEqual(c.engine.queue_idx, 1)
        self.assertEqual(c.engine.queue[1],
                         staged_path(self.preview, "Artist2", "Title2"))
        # Window = rows 2..7: rows 0..1 were outside it and cleaned up.
        self.assertEqual(len(c._discover_paths), 6)
        self.assertEqual(len(self._staged_files()), 6)
        self.assertNotIn("Artist0 - Title0.mp3", self._staged_files())

    def test_preview_window_capped_at_tapped_plus_five(self):
        c = self._c()
        wire_stubs(c)
        self._prefetch(c)
        key = c._discover_key(RECS[0])
        c._discover_play_rec(key)
        self._drain(c)
        self.assertEqual(len(self._staged_files()), 6)

        # Play through the whole block: no preparation beyond tapped + 5.
        c.engine.queue_idx = 6
        c._discover_consider_handoff()
        c._discover_refill()
        self._drain(c)
        self.assertEqual(len(self._staged_files()), 6)
        self.assertEqual(len(c._discover_paths), 6)

    def test_tap_restarts_session_at_row_during_discover(self):
        c = self._c()
        wire_stubs(c)
        self._prefetch(c)
        self._toggle_on(c)
        c.engine.queue_idx = 1  # first preview playing
        key = c._discover_key(RECS[8])
        c._discover_play_rec(key)
        self._drain(c)

        # Unplayed previews were dropped from the queue; the current one and
        # the new window's assets remain.
        self.assertEqual(c._discover_active, True)
        self.assertEqual(c.engine.queue[0], SEED)
        self.assertTrue(c._is_discover_path(c.engine.queue[1]))
        self.assertEqual(len(c._discover_paths), 3)  # current + rows 8,9
        st = c._discover_row_state(key)
        self.assertEqual(st["status"], "ready")

    def test_tap_failed_first_item_ends_session_seed_keeps_playing(self):
        c = self._c()
        wire_stubs(c)
        self._prefetch(c)
        # Row 7 (beyond the prefetched five) cannot be prepared: the
        # tap-to-preview session must end instead of hijacking playback
        # (mobile finishEmptyPreviewSession parity).
        make_download_stub(c, fail_title="Title7")
        key = c._discover_key(RECS[7])
        c._discover_play_rec(key)
        self._drain(c)

        self.assertEqual(c._discover_active, False)
        self.assertEqual(c.engine.queue_idx, 0)
        self.assertEqual(c.engine.queue, [SEED])
        st = c._discover_row_state(key)
        self.assertEqual(st["status"], "failed")

    def test_session_seed_pins_the_list_while_active(self):
        c = self._c()
        wire_stubs(c)
        self.assertEqual(c._discover_session_seed(), None)
        self._prefetch(c)
        # Prefetch alone does not pin the seed (no session).
        self.assertEqual(c._discover_session_seed(), None)
        key = c._discover_key(RECS[0])
        c._discover_play_rec(key)
        self._drain(c)
        self.assertEqual(c._discover_session_seed(), ("Artist0", "Title0"))

    # ── Next / Previous routing ─────────────────────────────────────────────

    def test_non_discover_next_leaves_block_to_physical_successor(self):
        physical = [SEED, CONT1, CONT2]
        c = self._c(physical)
        wire_stubs(c)
        self._prefetch(c)
        key = c._discover_key(RECS[0])
        c._discover_play_rec(key)
        self._drain(c)
        self.assertEqual(c.engine.queue_idx, 1)  # preview playing

        c._user_next()

        self.assertEqual(c._discover_active, False)
        self.assertEqual(c.engine.queue, physical)
        self.assertEqual(c.engine.queue_idx, 1)  # the seed's successor
        self.assertEqual(self._staged_files(), [])

    def test_non_discover_prev_returns_to_seed(self):
        physical = [SEED, CONT1, CONT2]
        c = self._c(physical)
        wire_stubs(c)
        self._prefetch(c)
        key = c._discover_key(RECS[0])
        c._discover_play_rec(key)
        self._drain(c)

        c._user_prev()

        self.assertEqual(c._discover_active, False)
        self.assertEqual(c.engine.queue, physical)
        self.assertEqual(c.engine.queue_idx, 0)  # the seed itself
        self.assertEqual(self._staged_files(), [])

    def test_discover_next_while_seed_current_arms_advance(self):
        c = self._c()
        wire_stubs(c)
        hold = {}
        make_download_stub(c, hold=hold)
        c._discover_set(True, RECS)
        self._drain(c)
        # The first preview is still "in flight" — nothing is queued yet.
        self.assertEqual(len(c._discover_paths), 0)
        self.assertEqual(c.engine.queue_idx, 0)

        # First Next press arms the auto-advance (mobile
        # advanceToDeferredRecommendation); the press is consumed.
        c._user_next()
        self.assertEqual(c._discover_advance_armed, True)
        self.assertEqual(c.engine.queue_idx, 0)

        # The held download completes -> the block is inserted and playback
        # jumps into it.
        path = staged_path(self.preview, "Artist0", "Title0")
        with open(path, "wb") as fh:
            fh.write(b"fake")
        make_download_stub(c)
        c._discover_download_done(hold["key"], hold["st"], hold["gen"],
                                  path, None)
        self._drain(c)

        self.assertEqual(c.engine.queue_idx, 1)
        self.assertEqual(c.engine.queue[1], path)
        self.assertEqual(c._discover_advance_armed, False)

    def test_discover_next_with_queued_block_walks_normally(self):
        c = self._c()
        wire_stubs(c)
        self._prefetch(c)
        self._toggle_on(c)
        # Seed current with the block queued: Next walks into the block
        # (mobile: normal Media3 skip into the queued preview).
        self.assertFalse(c._route_session_next())
        c.engine.next_track()
        self.assertEqual(c.engine.queue_idx, 1)
        self.assertTrue(c._is_discover_path(c.engine.queue[1]))

    # ── Session end: toggle off / handoff ───────────────────────────────────

    def test_toggle_off_inside_block_restores_queue_and_cleans(self):
        physical = [SEED, CONT1, CONT2]
        c = self._c(physical)
        wire_stubs(c)
        self._prefetch(c)
        self._toggle_on(c)
        self.assertEqual(c.engine.queue, physical[:1] + c._discover_paths + physical[1:])

        c.engine.queue_idx = 3  # a recommendation is playing
        self._toggle_off(c)

        self.assertEqual(c._discover_active, False)
        self.assertEqual(c.engine.queue, physical)
        self.assertEqual(c.engine.queue_idx, 0)
        # Staged previews wiped; nothing left in the library.
        self.assertEqual(self._staged_files(), [])
        self.assertEqual(os.listdir(self.dest), [])

    def test_handoff_ends_session_removes_block_and_cleans(self):
        physical = [SEED, CONT1, CONT2]
        c = self._c(physical)
        wire_stubs(c)
        self._prefetch(c)
        self._toggle_on(c)
        block_paths = list(c._discover_paths)

        # Playback walks past the block into the continuation.
        block_end = 0 + len(block_paths)
        c.engine.queue_idx = block_end + 1
        c._discover_consider_handoff()
        self._drain(c)

        self.assertEqual(c._discover_active, False)
        # Queue rebuilt from the physical capture at the shifted position.
        self.assertEqual(c.engine.queue, physical)
        self.assertEqual(c.engine.queue_idx, 1)
        self.assertEqual(self._staged_files(), [])
        self.assertEqual(os.listdir(self.dest), [])

    def test_toggle_off_past_block_does_not_rewind(self):
        physical = [SEED, CONT1, CONT2]
        c = self._c(physical)
        wire_stubs(c)
        self._prefetch(c)
        self._toggle_on(c)
        block_paths = list(c._discover_paths)

        block_end = 0 + len(block_paths)
        c.engine.queue_idx = block_end + 1
        self._toggle_off(c)

        self.assertEqual(c._discover_active, False)
        self.assertEqual(c.engine.queue, physical)
        self.assertEqual(c.engine.queue_idx, 1)  # continuation, not the seed

    def test_queue_replaced_ends_session_without_touching_queue(self):
        c = self._c()
        wire_stubs(c)
        self._prefetch(c)
        self._toggle_on(c)
        c.engine.queue = ["/tmp/fake_lib/Other.mp3"]
        c.engine.queue_idx = 0
        c._discover_consider_handoff()

        self.assertEqual(c._discover_active, False)
        self.assertEqual(c.engine.queue, ["/tmp/fake_lib/Other.mp3"])
        self.assertEqual(self._staged_files(), [])

    def test_playback_failure_marks_row_and_removes_item(self):
        c = self._c()
        wire_stubs(c)
        self._prefetch(c)
        self._toggle_on(c)
        path = c._discover_paths[0]
        c.engine.queue_idx = 1
        c._on_playback_failed(path)

        key = c._discover_key_for_path(path)
        self.assertIsNone(key)
        self.assertNotIn(path, c.engine.queue)
        self.assertNotIn(path, c._discover_paths)
        self.assertFalse(os.path.isfile(path))
        # The failed row shows the retryable error.
        failed = [k for k, s in c._discover_states.items()
                  if s["status"] == "failed"]
        self.assertEqual(len(failed), 1)

    # ── Discover infinite scroll ────────────────────────────────────────────

    def test_discover_next_at_block_end_falls_through_to_player(self):
        c = self._c()
        wire_stubs(c)
        self._prefetch(c, recs=RECS[:5])
        self._toggle_on(c)
        # Playback reaches the last loaded preview (with a physical successor
        # queued behind): Next must fall through to the player instead of
        # being swallowed (mobile: 'falls back to player skip').
        c.engine.queue_idx = len(c._discover_paths)
        self.assertFalse(c._route_session_next())

    def test_discover_auto_extend_at_last_loaded_recommendation(self):
        c = self._c()
        wire_stubs(c)
        loaded = []
        sp = types.SimpleNamespace()
        sp._load_more_recs = lambda: loaded.append(True)
        c._frames["song_page"] = sp
        self._prefetch(c, recs=RECS[:5])
        self._toggle_on(c)
        self.assertEqual(loaded, [])

        # Playback reaches the last loaded recommendation -> auto page fetch.
        c.engine.queue_idx = len(c._discover_paths)
        c._on_track_changed_discovery(c._discover_paths[-1])
        self._drain(c)
        self.assertEqual(loaded, [True])

        # A mid-list preview never triggers it.
        loaded.clear()
        c.engine.queue_idx = 1
        c._on_track_changed_discovery(c._discover_paths[0])
        self._drain(c)
        self.assertEqual(loaded, [])

    # ── Failures, retries, promotion, load-more ─────────────────────────────

    def test_failed_download_skips_rec_and_continues(self):
        c = self._c()
        wire_stubs(c, fail_title="Title0")
        self._prefetch(c)

        self.assertEqual(c._discover_row_state(c._discover_key(RECS[0]))["status"],
                         "failed")
        # The prefetch window is the fixed first five (mobile
        # PREFETCH_RECOMMENDATION_COUNT parity): the four successes stay
        # ready, the failure stays retryable.
        self.assertEqual(c._discover_ready_count(), 4)
        self.assertEqual(len(self._staged_files()), 4)

    def test_retry_reprepares_failed_rec(self):
        c = self._c()
        wire_stubs(c, fail_title="Title0")
        self._prefetch(c)
        key = c._discover_key(RECS[0])
        self.assertEqual(c._discover_row_state(key)["status"], "failed")

        # Now the retried download succeeds.
        make_download_stub(c)
        c._discover_retry(key)
        self._drain(c)

        self.assertEqual(c._discover_row_state(key)["status"], "ready")
        self.assertIn("Title0", " ".join(self._staged_files()))

    def test_promote_copies_into_chosen_playlist(self):
        c = self._c()
        wire_stubs(c)
        chosen = os.path.join(self.dest, "My Playlist")
        os.makedirs(chosen, exist_ok=True)

        def fake_picker(paths, title, on_transfer=None, subtitle=None):
            if on_transfer:
                on_transfer(chosen, "My Playlist")

        c._show_add_to_playlist_dialog = fake_picker
        self._prefetch(c)

        key = c._discover_key(RECS[0])
        c._discover_promote(key)
        self._drain(c)
        self._wait_save(c)

        st = c._discover_row_state(key)
        self.assertEqual(st["status"], "saved")
        self.assertEqual(st["promoted"], True)
        self.assertTrue(st["permanent"])
        # Copied into the chosen playlist folder; staged copy still present.
        self.assertTrue(os.path.isfile(
            os.path.join(chosen, "Artist0 - Title0.mp3")))
        self.assertIn("Artist0 - Title0.mp3", self._staged_files())
        # Nothing silently landed in the library root / Unsorted.
        self.assertEqual(os.listdir(self.dest), ["My Playlist"])

    def _wait_save(self, c, timeout=2.0):
        deadline = time.monotonic() + timeout
        while c._discover_saving_keys and time.monotonic() < deadline:
            self._drain(c)
            time.sleep(0.005)
        self._drain(c)

    def test_already_saved_links_without_redownload(self):
        c = self._c()
        wire_stubs(c)
        chosen = os.path.join(self.dest, "My Playlist")
        other = os.path.join(self.dest, "Other Playlist")
        os.makedirs(chosen, exist_ok=True)
        os.makedirs(other, exist_ok=True)
        picked = []
        c._show_add_to_playlist_dialog = (
            lambda paths, title, on_transfer=None, subtitle=None:
                picked.append(paths) or (
                    on_transfer(other, "Other Playlist")
                    if on_transfer else None))
        self._prefetch(c)

        key = c._discover_key(RECS[0])
        c._discover_promote(key)
        self._drain(c)
        self._wait_save(c)
        self.assertEqual(len(picked), 1)

        # Second save: no re-download, the saved copy is linked instead.
        staged_before = len(self._staged_files())
        c._discover_promote(key)
        self._drain(c)
        self.assertEqual(len(picked), 2)
        self.assertEqual(picked[1], [c._discover_saved_uris[key]])
        self.assertEqual(len(self._staged_files()), staged_before)
        self.assertTrue(os.path.isfile(
            os.path.join(other, "Artist0 - Title0.mp3")))

    def test_download_new_uses_chosen_destination(self):
        c = self._c()
        wire_stubs(c)
        chosen = os.path.join(self.dest, "Chosen Playlist")
        os.makedirs(chosen, exist_ok=True)
        picked = []
        c._show_add_to_playlist_dialog = (
            lambda paths, title, on_transfer=None, subtitle=None:
                picked.append((paths, title)) or
                (on_transfer(chosen, "Chosen Playlist") if on_transfer else None))

        c._discover_download_new("key9", "Artist9", "Title9")
        self._drain(c)

        self.assertEqual(picked[0][0], [])  # no staged file to copy
        self.assertTrue(os.path.isfile(
            os.path.join(chosen, "Artist9 - Title9.mp3")))
        self.assertEqual(os.listdir(self.dest), ["Chosen Playlist"])
        # The queued row observed the completion: permanent save recorded.
        self.assertEqual(c._discover_saved_uris["key9"],
                         os.path.join(chosen, "Artist9 - Title9.mp3"))
        self.assertEqual(c._discover_queued_keys, {})
        st = c._discover_row_state("key9")
        self.assertEqual(st["status"], "saved")

    def test_append_recs_grows_chain_when_window_has_room(self):
        c = self._c()
        wire_stubs(c)
        self._prefetch(c, recs=RECS[:5])
        self._toggle_on(c)
        self.assertEqual(len(self._staged_files()), 5)

        c.engine.queue_idx = 2
        extra = [{"artist": "More", "title": f"Extra{i}"} for i in range(25)]
        c._discover_append_recs(extra)
        self._drain(c)

        self.assertEqual(len(c._discover_recs), 30)
        self.assertEqual(len(c._discover_paths), 7)

    def test_new_seed_prefetch_ends_session_and_drops_stale_inflight(self):
        c = self._c()
        wire_stubs(c)
        c._discover_set(True, RECS)
        self._drain(c)
        self.assertEqual(c._discover_active, True)

        # A new song starts -> new seed delivered -> chain resets.
        c._discover_prefetch(("NewArtist", "NewTitle"), RECS)
        self._drain(c)

        self.assertEqual(c._discover_active, False)
        self.assertEqual(len(c._discover_recs), 10)
        # The old session's block was removed and the queue restored.
        self.assertEqual(len(c._discover_paths), 0)
        self.assertEqual(c.engine.queue, [SEED])


if __name__ == "__main__":
    unittest.main()
