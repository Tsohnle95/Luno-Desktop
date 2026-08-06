"""
Deterministic unit tests for LunoEngine playback state, heartbeat rescheduling,
completion detection, repeat, seek, pause/resume, and multi-track transitions.

Uses mocked pygame.mixer, time.monotonic, and controller so no audio device or
Tk display is required.
"""

import sys
import types
import time
import unittest
from unittest.mock import MagicMock, patch, PropertyMock


# ---------------------------------------------------------------------------
# Fake/mock pygame.mixer module
# ---------------------------------------------------------------------------
class FakeMusic:
    def __init__(self):
        self._loaded = None
        self._volume = 0.7
        self._playing = False
        self._busy = True  # True until explicitly stopped or end reached
        self._paused = False
        self._start_pos = 0.0
        self._play_mono = 0.0
        self._get_pos_override_ms = None

    def load(self, path):
        self._loaded = path
        self._busy = True
        self._playing = True

    def play(self, start=0.0):
        self._start_pos = start
        self._play_mono = time.monotonic()
        self._get_pos_override_ms = None
        self._busy = True
        self._playing = True
        self._paused = False

    def stop(self):
        self._busy = False
        self._playing = False
        self._paused = False

    def pause(self):
        self._paused = True

    def unpause(self):
        self._paused = False

    def get_pos(self):
        if self._get_pos_override_ms is not None:
            return self._get_pos_override_ms
        if not self._playing or self._paused:
            return -1
        return int(max(0.0, time.monotonic() - self._play_mono) * 1000)

    def set_get_pos_ms(self, ms):
        self._get_pos_override_ms = ms

    def get_busy(self):
        return self._busy

    def set_volume(self, vol):
        self._volume = vol

    def get_volume(self):
        return self._volume


class FakeMixer:
    def __init__(self):
        self.music = FakeMusic()
        self._inited = False
        self._quited = False

    def pre_init(self, *a, **kw):
        pass

    def init(self):
        self._inited = True

    def quit(self):
        self._quited = True
        self._inited = False


# ---------------------------------------------------------------------------
# Fake time.monotonic — controlled by test
# ---------------------------------------------------------------------------
class MonotonicClock:
    def __init__(self, start=1000.0):
        self._now = start

    def advance(self, delta):
        self._now += delta

    def set(self, t):
        self._now = t

    def __call__(self):
        return self._now


# ---------------------------------------------------------------------------
# Fake controller (minimal Tk-like surface)
# ---------------------------------------------------------------------------
def make_fake_controller():
    """Build a SimpleNamespace that quacks like LunoMusicPlayer for engine use."""
    scrub = types.SimpleNamespace()
    scrub._val = 0
    def scrub_set(v):
        scrub._val = v
    scrub.set = scrub_set

    time_cur = types.SimpleNamespace()
    time_cur._text = "0:00"
    def time_cur_set(t):
        time_cur._text = t
    time_cur.config = lambda **kw: setattr(time_cur, '_text', kw.get('text', time_cur._text))

    time_tot = types.SimpleNamespace()
    time_tot._text = "0:00"
    def time_tot_set(t):
        time_tot._text = t
    time_tot.config = lambda **kw: setattr(time_tot, '_text', kw.get('text', time_tot._text))

    play_btn = types.SimpleNamespace()
    play_btn._text = "▶"
    def play_btn_set(t):
        play_btn._text = t
    play_btn.set_text = play_btn_set

    now_title = types.SimpleNamespace()
    now_title.config = lambda **kw: None
    now_artist = types.SimpleNamespace()
    now_artist.config = lambda **kw: None

    _frames = {"song_page": None, "library": types.SimpleNamespace()}
    _frames["library"]._redraw_track_list = lambda: None

    c = types.SimpleNamespace()
    c.MUSIC_ROOT = "/tmp/fake_music_root"
    c.scrub = scrub
    c.time_cur = time_cur
    c.time_tot = time_tot
    c.play_btn = play_btn
    c.now_title = now_title
    c.now_artist = now_artist
    c._frames = _frames
    c._closing = False
    c._recently_played = []
    c._queue_playlist_name = None
    c._queue_playlist_paths = []
    c._library = {}
    c._log_error = lambda area, msg: None
    c._update_queue = lambda: None
    c._update_now_playing_info = lambda *a, **kw: None
    c._on_track_changed_discovery = lambda *a: None
    c._broadcast_shuffle_state = lambda: None
    c.update_player_art = lambda *a: None
    c._clear_now_playing = lambda: None
    # Fake Tk after — we capture scheduled callbacks
    c._after_queue = []
    def fake_after(delay, fn):
        c._after_queue.append((delay, fn))
        return id(fn)
    c.after = fake_after
    c.after_cancel = lambda *a: None
    return c


# We need to replace pygame in the engine module's namespace before any
# LunoEngine instantiation.  We do this once at module level, then
# reset state per test via setUp.
import engine

_fake_mixer = FakeMixer()
engine.pygame = types.SimpleNamespace()
engine.pygame.mixer = _fake_mixer
engine.pygame.mixer.music = _fake_mixer.music
engine.pygame.mixer.pre_init = _fake_mixer.pre_init
engine.pygame.mixer.init = _fake_mixer.init
engine.pygame.mixer.quit = _fake_mixer.quit
engine.PYGAME_AVAILABLE = True
engine.MUTAGEN_AVAILABLE = False
engine.MutagenFile = None

# We also need to reset the mixer_ready state that was set at import time
# since engine module was already imported for the 'from engine import ...' in music_player


class LunoEnginePlaybackTest(unittest.TestCase):

    def setUp(self):
        """Create a fresh engine with mocked mixer, clock, and controller."""
        # Reset mixer state
        _fake_mixer.music = FakeMusic()
        _fake_mixer._inited = False
        engine.pygame.mixer.music = _fake_mixer.music
        
        # Install controlled monotonic clock
        self.clock = MonotonicClock()
        self._mono_patch = patch('time.monotonic', self.clock)
        self._mono_patch.start()
        
        self.controller = make_fake_controller()
        # Reset after queue
        self.controller._after_queue = []
        
        # Reinitialize mixer_ready for each test by creating engine
        # But LunoEngine.__init__ calls pygame.mixer.quit/init which our fake handles
        self.engine = engine.LunoEngine(self.controller)
        # Clear any pending after calls from __init__'s _poll
        self.controller._after_queue.clear()
        # Ensure mixer is ready
        self.engine.mixer_ready = True
        self.engine.playing = False
        self.engine.paused = False
        self.engine.duration = 0.0
        self.engine.track_start = 0.0
        self.engine.seek_pos = 0.0
        self.engine._completion_dispatched = False

    def tearDown(self):
        self._mono_patch.stop()
        # Clean up any pending after ids
        if self.engine._poll_after_id:
            self.engine._poll_after_id = None
        self.controller._closing = True

    # ------------------------------------------------------------------
    # Helpers
    # ------------------------------------------------------------------
    def _prime_playing_state(self, path="/fake/song.mp3", duration=200.0,
                              track_start_offset=0.0):
        """Set up engine state as if a track is actively playing."""
        self.engine.queue = [path]
        self.engine.queue_idx = 0
        self.engine.playing = True
        self.engine.paused = False
        self.engine.duration = duration
        self.engine.track_start = self.clock() - track_start_offset
        self.engine.seek_pos = 0.0
        self.engine._playback_base_pos = 0.0
        self.engine._playback_base_mono = self.engine.track_start
        self.engine._last_position = track_start_offset
        self.engine._completion_dispatched = False
        _fake_mixer.music._busy = True

    # ------------------------------------------------------------------
    # Shuffle
    # ------------------------------------------------------------------
    def test_original_queue_starts_empty_and_shuffle_can_be_disabled_safely(self):
        self.assertEqual(self.engine._original_queue, [])
        self.engine.queue = ["/fake/song1.mp3", "/fake/song2.mp3"]
        self.engine.queue_idx = 0
        self.engine.shuffle = True

        self.engine.toggle_shuffle()

        self.assertFalse(self.engine.shuffle)

    def test_shuffle_cycle_avoids_immediate_boundary_repeat(self):
        songs = ["/fake/song1.mp3", "/fake/song2.mp3", "/fake/song3.mp3"]
        self.engine.queue = list(songs)
        self.engine.queue_idx = len(songs) - 1
        self.engine.shuffle = True
        self.controller._queue_playlist_paths = list(songs)

        # Force the freshly shuffled order to put the previous final track
        # first, then verify the boundary guard swaps it out.
        with patch("engine.random.shuffle", side_effect=lambda items: items.reverse()), \
             patch("engine.random.choice", return_value=1), \
             patch.object(self.engine, "play_current"):
            self.engine.next_track()

        self.assertNotEqual(self.engine.queue[0], songs[-1])
        self.assertEqual(sorted(self.engine.queue), sorted(songs))
        self.assertEqual(self.engine.queue_idx, 0)

    # ------------------------------------------------------------------
    # get_position
    # ------------------------------------------------------------------
    def test_get_position_while_playing(self):
        self._prime_playing_state(track_start_offset=10.0)
        pos = self.engine.get_position()
        self.assertAlmostEqual(pos, 10.0, places=2)

        self.clock.advance(5.0)
        pos = self.engine.get_position()
        self.assertAlmostEqual(pos, 15.0, places=2)

    def test_get_position_prefers_mixer_clock_over_monotonic_drift(self):
        self._prime_playing_state(duration=200.0, track_start_offset=90.0)
        _fake_mixer.music._playing = True
        _fake_mixer.music.set_get_pos_ms(12_000)
        pos = self.engine.get_position()
        self.assertAlmostEqual(pos, 12.0, places=2)

        self.engine._poll()
        self.assertAlmostEqual(self.controller.scrub._val, 6.0, places=2)
        self.assertEqual(self.controller.time_cur._text, "0:12")

    def test_get_position_mixer_clock_clamps_to_duration(self):
        self._prime_playing_state(duration=30.0, track_start_offset=1.0)
        _fake_mixer.music._playing = True
        _fake_mixer.music.set_get_pos_ms(35_000)
        self.assertAlmostEqual(self.engine.get_position(), 30.0, places=2)

    def test_get_position_while_paused(self):
        self._prime_playing_state(track_start_offset=10.0)
        self.engine.paused = True
        self.engine.seek_pos = 12.5
        pos = self.engine.get_position()
        self.assertAlmostEqual(pos, 12.5, places=2)
        # Advancing clock should NOT change paused position
        self.clock.advance(10.0)
        pos = self.engine.get_position()
        self.assertAlmostEqual(pos, 12.5, places=2)

    def test_get_position_while_stopped(self):
        self.engine.playing = False
        self.engine.paused = False
        pos = self.engine.get_position()
        self.assertEqual(pos, 0.0)

    def test_get_position_clamps_to_duration(self):
        self._prime_playing_state(duration=30.0, track_start_offset=35.0)
        # elapsed would be 35s but duration is 30s
        pos = self.engine.get_position()
        self.assertAlmostEqual(pos, 30.0, places=2)

    def test_get_position_never_negative(self):
        self._prime_playing_state(track_start_offset=-5.0)
        # elapsed would be -5s → clamped to 0
        pos = self.engine.get_position()
        self.assertAlmostEqual(pos, 0.0, places=2)

    # ------------------------------------------------------------------
    # Play, pause, resume, seek
    # ------------------------------------------------------------------
    @patch('os.path.exists', return_value=True)
    def test_toggle_play_starts_playback(self, _mock_exists):
        self.engine.queue = ["/fake/song.mp3"]
        self.engine.queue_idx = 0
        self.engine.playing = False
        self.engine.toggle_play()
        self.assertTrue(self.engine.playing)
        self.assertFalse(self.engine.paused)

    def test_toggle_play_pauses_and_resumes(self):
        self._prime_playing_state()
        self.engine.toggle_play()  # pause
        self.assertTrue(self.engine.paused)
        paused_pos = self.engine.seek_pos

        self.clock.advance(5.0)
        self.engine.toggle_play()  # resume
        self.assertFalse(self.engine.paused)
        # After resume, track_start should be reconstructed from seek_pos
        expected_start = self.clock() - paused_pos
        self.assertAlmostEqual(self.engine.track_start, expected_start, places=2)

    def test_pause_resume_preserves_displayed_position_with_mixer_clock(self):
        self._prime_playing_state(duration=100.0)
        _fake_mixer.music._playing = True
        _fake_mixer.music.set_get_pos_ms(25_000)
        self.engine.toggle_play()
        self.assertTrue(self.engine.paused)
        self.assertAlmostEqual(self.engine.get_position(), 25.0, places=2)
        self.clock.advance(5.0)
        self.assertAlmostEqual(self.engine.get_position(), 25.0, places=2)
        self.engine.toggle_play()
        self.assertFalse(self.engine.paused)
        _fake_mixer.music.set_get_pos_ms(27_000)
        self.assertAlmostEqual(self.engine.get_position(), 27.0, places=2)

    def test_seek_updates_position(self):
        self._prime_playing_state(duration=100.0)
        self.engine.on_scrub_moved(50)  # 50% → 50s
        self.assertAlmostEqual(self.engine.seek_pos, 50.0, places=2)
        pos = self.engine.get_position()
        self.assertAlmostEqual(pos, 50.0, places=2)

    def test_seek_continues_from_requested_position_using_mixer_base(self):
        self._prime_playing_state(duration=100.0)
        self.engine.on_scrub_moved(50)  # base absolute position is 50s
        self.clock.advance(3.0)
        pos = self.engine.get_position()
        self.assertAlmostEqual(pos, 53.0, places=2)
        self.engine._poll()
        self.assertAlmostEqual(self.controller.scrub._val, 53.0, places=2)

    def test_paused_seek_updates_footer_and_stays_paused(self):
        self._prime_playing_state(duration=100.0, track_start_offset=10.0)
        self.engine.paused = True
        self.engine.seek_pos = 10.0
        self.engine.on_scrub_moved(75)
        self.assertTrue(self.engine.paused)
        self.assertTrue(_fake_mixer.music._paused)
        self.assertAlmostEqual(self.engine.get_position(), 75.0, places=2)
        self.assertAlmostEqual(self.controller.scrub._val, 75.0, places=2)
        self.assertEqual(self.controller.time_cur._text, "1:15")

    def test_seek_and_pause_no_backward_jump(self):
        """After seek+pause, poll uses seek_pos, not elapsed."""
        self._prime_playing_state(duration=100.0, track_start_offset=10.0)
        self.engine.on_scrub_moved(50)  # seek to 50s
        self.engine.paused = True
        self.engine.seek_pos = 50.0
        # Even if time advances, paused position should stay at seek_pos
        self.clock.advance(10.0)
        pos = self.engine.get_position()
        self.assertAlmostEqual(pos, 50.0, places=2)

    # ------------------------------------------------------------------
    # Heartbeat rescheduling
    # ------------------------------------------------------------------
    def test_poll_reschedules_while_playing(self):
        self._prime_playing_state()
        self.controller._after_queue.clear()
        self.engine._poll()
        # Should have one after(400, ...) scheduled
        self.assertGreaterEqual(len(self.controller._after_queue), 1)
        has_heartbeat = any(delay == 400 for delay, fn in self.controller._after_queue)
        self.assertTrue(has_heartbeat)

    def test_poll_reschedules_while_paused(self):
        self._prime_playing_state()
        self.engine.paused = True
        self.controller._after_queue.clear()
        self.engine._poll()
        self.assertEqual(len(self.controller._after_queue), 1)
        self.assertEqual(self.controller._after_queue[0][0], 400)

    def test_poll_reschedules_while_idle(self):
        self.engine.playing = False
        self.engine.paused = False
        self.controller._after_queue.clear()
        self.engine._poll()
        self.assertEqual(len(self.controller._after_queue), 1)
        self.assertEqual(self.controller._after_queue[0][0], 400)

    def test_poll_does_not_reschedule_when_closing(self):
        self._prime_playing_state()
        self.controller._closing = True
        self.controller._after_queue.clear()
        self.engine._poll()
        self.assertEqual(len(self.controller._after_queue), 0)

    # ------------------------------------------------------------------
    # Duration-based completion
    # ------------------------------------------------------------------
    def test_duration_completion_advances_to_next(self):
        self._prime_playing_state(duration=10.0, track_start_offset=9.5)
        self.engine.queue = ["/fake/song1.mp3", "/fake/song2.mp3"]
        self.engine.queue_idx = 0
        # Advance clock past duration
        self.clock.advance(1.0)  # elapsed = 10.5 > 10.0
        self.controller._after_queue.clear()
        self.engine._poll()
        # Should have advanced to next track
        self.assertEqual(self.engine.queue_idx, 1)
        self.assertTrue(self.engine.playing)
        # Should have rescheduled heartbeat (may be extra after calls from
        # play_current for song_page update, so check at least 1 exists)
        self.assertGreaterEqual(len(self.controller._after_queue), 1)
        # At least one entry should be the 400ms heartbeat reschedule
        has_heartbeat = any(delay == 400 for delay, fn in self.controller._after_queue)
        self.assertTrue(has_heartbeat)

    def test_repeat_restarts_same_track(self):
        self._prime_playing_state(duration=10.0, track_start_offset=9.5)
        self.engine.repeat = True
        self.engine.queue = ["/fake/song1.mp3"]
        self.engine.queue_idx = 0
        self.clock.advance(1.0)  # elapsed > duration
        self.controller._after_queue.clear()
        self.engine._poll()
        # Should have restarted same track (repeat)
        self.assertEqual(self.engine.queue_idx, 0)
        self.assertTrue(self.engine.playing)
        self.assertFalse(self.engine._completion_dispatched)
        self.assertGreaterEqual(len(self.controller._after_queue), 1)
        has_heartbeat = any(delay == 400 for delay, fn in self.controller._after_queue)
        self.assertTrue(has_heartbeat)

    def test_completion_latch_prevents_dual_advance(self):
        """Once _completion_dispatched is set, poll does not advance again."""
        self._prime_playing_state(duration=10.0, track_start_offset=9.5)
        self.engine._completion_dispatched = True  # already dispatched
        self.engine.queue = ["/fake/song1.mp3", "/fake/song2.mp3"]
        self.engine.queue_idx = 0
        self.clock.advance(1.0)
        self.engine._poll()
        # Should NOT advance because latch is set
        self.assertEqual(self.engine.queue_idx, 0)

    # ------------------------------------------------------------------
    # Mixer-idle completion (unknown-duration)
    # ------------------------------------------------------------------
    def test_mixer_idle_completion_with_known_duration(self):
        """get_busy false + elapsed > 0.5 should advance even when duration is known."""
        self._prime_playing_state(duration=200.0, track_start_offset=5.0)
        _fake_mixer.music._busy = False  # mixer idle
        self.clock.advance(1.0)  # elapsed = 6s > 0.5s startup grace
        self.engine.queue = ["/fake/song1.mp3", "/fake/song2.mp3"]
        self.engine.queue_idx = 0
        self.engine._poll()
        self.assertEqual(self.engine.queue_idx, 1)

    def test_mixer_idle_completion_unknown_duration(self):
        """Mixer-idle detection works even when duration is 0 (unknown)."""
        self._prime_playing_state(duration=0.0, track_start_offset=2.0)
        _fake_mixer.music._busy = False
        self.clock.advance(1.0)  # elapsed = 3s > 0.5s grace
        self.engine.queue = ["/fake/song1.mp3", "/fake/song2.mp3"]
        self.engine.queue_idx = 0
        self.engine._poll()
        self.assertEqual(self.engine.queue_idx, 1)

    def test_mixer_idle_does_not_advance_during_startup_grace(self):
        """Elapsed <= 0.5s after load should not trigger idle completion."""
        self._prime_playing_state(duration=0.0, track_start_offset=0.3)
        _fake_mixer.music._busy = False
        # elapsed = 0.3s < 0.5s grace
        self.engine.queue = ["/fake/song1.mp3", "/fake/song2.mp3"]
        self.engine.queue_idx = 0
        self.engine._poll()
        self.assertEqual(self.engine.queue_idx, 0)

    def test_mixer_idle_completion_with_repeat(self):
        """Mixer idle + repeat restarts the same track."""
        self._prime_playing_state(duration=0.0, track_start_offset=2.0)
        self.engine.repeat = True
        _fake_mixer.music._busy = False
        self.clock.advance(1.0)
        self.engine.queue = ["/fake/song1.mp3"]
        self.engine.queue_idx = 0
        self.controller._after_queue.clear()
        self.engine._poll()
        self.assertEqual(self.engine.queue_idx, 0)  # same track (repeat)
        self.assertTrue(self.engine.playing)

    # ------------------------------------------------------------------
    # Exception handling in _poll
    # ------------------------------------------------------------------
    def test_poll_handles_exception_and_reschedules(self):
        """Exception inside _poll should be caught and heartbeat continues."""
        self._prime_playing_state()
        # Force an exception by making scrub.set raise
        original_set = self.controller.scrub.set
        def broken_set(v):
            raise RuntimeError("boom")
        self.controller.scrub.set = broken_set
        self.controller._after_queue.clear()
        # Should not propagate
        self.engine._poll()
        self.assertEqual(len(self.controller._after_queue), 1)
        # Restore
        self.controller.scrub.set = original_set

    # ------------------------------------------------------------------
    # Multi-transition regression (3 songs auto-advance)
    # ------------------------------------------------------------------
    def test_multi_transition_auto_advance(self):
        """Three queued songs should auto-advance in sequence."""
        songs = ["/fake/song1.mp3", "/fake/song2.mp3", "/fake/song3.mp3"]
        self.engine.queue = list(songs)
        self.engine.queue_idx = 0
        self.engine.playing = True
        self.engine.paused = False
        self.engine.duration = 10.0
        self.engine.track_start = self.clock() - 9.5  # almost done

        # Advance past song 1
        self.clock.advance(1.0)  # elapsed = 10.5 > 10
        self.engine._completion_dispatched = False
        self.engine._poll()
        self.assertEqual(self.engine.queue_idx, 1)
        self.assertEqual(self.controller.scrub._val, 0)  # scrub reset on new track
        self.assertEqual(self.controller.time_cur._text, "0:00")
        # The _poll after play_current resets latch

        # Advance past song 2
        self.engine.track_start = self.clock() - 9.5
        self.clock.advance(1.0)
        self.engine._completion_dispatched = False
        self.engine._poll()
        self.assertEqual(self.engine.queue_idx, 2)

        # Advance past song 3
        self.engine.track_start = self.clock() - 9.5
        self.clock.advance(1.0)
        self.engine._completion_dispatched = False
        self.engine._poll()
        # Queue exhausted — next_track wraps around
        self.assertEqual(self.engine.queue_idx, 0)

    # ------------------------------------------------------------------
    # Wall-clock immunity (monotonic)
    # ------------------------------------------------------------------
    def test_wall_clock_change_does_not_affect_position(self):
        """Since we use time.monotonic, system wall-clock changes have no effect."""
        self._prime_playing_state(track_start_offset=10.0)
        pos_before = self.engine.get_position()
        # Simulate a wall-clock jump by changing time.time won't matter
        # because we use time.monotonic. The clock already advanced.
        self.clock.advance(5.0)
        pos_after = self.engine.get_position()
        self.assertAlmostEqual(pos_after, pos_before + 5.0, places=2)

    # ------------------------------------------------------------------
    # Footer scrub reset on new track
    # ------------------------------------------------------------------
    def test_scrub_reset_on_play_current(self):
        self.controller.scrub._val = 50  # stale value
        self.engine.queue = ["/fake/song.mp3"]
        self.engine.queue_idx = 0
        self.engine.play_current()
        self.assertEqual(self.controller.scrub._val, 0)

    # ------------------------------------------------------------------
    # Clear queue resets position
    # ------------------------------------------------------------------
    def test_clear_queue_resets_position(self):
        self._prime_playing_state()
        self.engine.clear_queue()
        self.assertFalse(self.engine.playing)
        self.assertEqual(self.engine.get_position(), 0.0)


if __name__ == "__main__":
    unittest.main()
