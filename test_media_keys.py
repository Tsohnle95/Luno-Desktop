import types
import queue
import unittest

from music_player import LunoMusicPlayer


class FakeEngine:
    def __init__(self, playing=False, paused=False, queue=None, mixer_ready=True):
        self.playing = playing
        self.paused = paused
        self.queue = list(queue or [])
        self.mixer_ready = mixer_ready
        self.toggle_calls = 0
        self.next_calls = 0
        self.prev_calls = 0

    def toggle_play(self):
        self.toggle_calls += 1

    def next_track(self):
        self.next_calls += 1

    def prev_track(self):
        self.prev_calls += 1

    def stop(self):
        pass


class MediaKeyLogicTest(unittest.TestCase):
    def make_app(self, engine):
        app = types.SimpleNamespace()
        app.engine = engine
        app.play_all_calls = 0
        app._closing = False
        app._np_poll_counter = 0
        app._play_all = lambda: setattr(app, "play_all_calls", app.play_all_calls + 1)
        app._media_ready = types.MethodType(LunoMusicPlayer._media_ready, app)
        return app

    def test_play_starts_current_view_when_queue_empty(self):
        app = self.make_app(FakeEngine(playing=False, paused=False, queue=[]))

        LunoMusicPlayer._media_play(app)

        self.assertEqual(app.play_all_calls, 1)
        self.assertEqual(app.engine.toggle_calls, 0)

    def test_play_resumes_when_paused(self):
        app = self.make_app(FakeEngine(playing=True, paused=True, queue=["track.mp3"]))

        LunoMusicPlayer._media_play(app)

        self.assertEqual(app.play_all_calls, 0)
        self.assertEqual(app.engine.toggle_calls, 1)

    def test_pause_only_pauses_when_currently_playing(self):
        app = self.make_app(FakeEngine(playing=True, paused=False, queue=["track.mp3"]))

        LunoMusicPlayer._media_pause(app)

        self.assertEqual(app.engine.toggle_calls, 1)

        paused_app = self.make_app(FakeEngine(playing=True, paused=True, queue=["track.mp3"]))
        LunoMusicPlayer._media_pause(paused_app)

        self.assertEqual(paused_app.engine.toggle_calls, 0)

    def test_toggle_starts_current_view_when_queue_empty(self):
        app = self.make_app(FakeEngine(playing=False, paused=False, queue=[]))

        LunoMusicPlayer._media_toggle_play(app)

        self.assertEqual(app.play_all_calls, 1)
        self.assertEqual(app.engine.toggle_calls, 0)

    def test_media_play_does_nothing_when_closing(self):
        app = self.make_app(FakeEngine(playing=False, paused=False, queue=[]))
        app._closing = True

        LunoMusicPlayer._media_play(app)

        self.assertEqual(app.play_all_calls, 0)
        self.assertEqual(app.engine.toggle_calls, 0)

    def test_media_play_does_nothing_without_engine(self):
        app = self.make_app(None)

        LunoMusicPlayer._media_play(app)

        self.assertEqual(app.play_all_calls, 0)

    def test_media_toggle_play_does_nothing_when_mixer_not_ready(self):
        engine = FakeEngine(playing=False, paused=False, queue=["track.mp3"], mixer_ready=False)
        app = self.make_app(engine)

        LunoMusicPlayer._media_toggle_play(app)

        self.assertEqual(app.play_all_calls, 0)
        self.assertEqual(app.engine.toggle_calls, 0)

    def test_enqueue_media_command_skips_when_closing(self):
        app = self.make_app(FakeEngine())
        app._media_command_queue = queue.SimpleQueue()
        app._closing = True

        LunoMusicPlayer._enqueue_media_command(app, "toggle")

        self.assertTrue(app._media_command_queue.empty())

    def test_enqueue_media_command_skips_without_engine(self):
        app = self.make_app(None)
        app._media_command_queue = queue.SimpleQueue()

        LunoMusicPlayer._enqueue_media_command(app, "toggle")

        self.assertTrue(app._media_command_queue.empty())

    def test_media_key_listener_skips_without_engine(self):
        app = self.make_app(None)
        app._should_handle_media_keys = lambda: True
        app._mp_registered = False

        LunoMusicPlayer._start_media_key_listener(app)

        self.assertFalse(app._mp_registered)

    def test_media_command_poll_dispatches_on_main_thread(self):
        app = self.make_app(FakeEngine(playing=False, paused=False, queue=[]))
        app._media_command_queue = queue.SimpleQueue()
        app._log_error = lambda *args: None
        app.after_calls = []
        app.after = lambda delay, fn: app.after_calls.append((delay, fn))
        app._media_play = types.MethodType(LunoMusicPlayer._media_play, app)
        app._media_pause = types.MethodType(LunoMusicPlayer._media_pause, app)
        app._media_toggle_play = types.MethodType(LunoMusicPlayer._media_toggle_play, app)
        app._media_ready = types.MethodType(LunoMusicPlayer._media_ready, app)
        app._poll_media_commands = types.MethodType(LunoMusicPlayer._poll_media_commands, app)

        app._media_command_queue.put("toggle")
        app._media_command_queue.put("next")

        LunoMusicPlayer._poll_media_commands(app)

        self.assertEqual(app.play_all_calls, 1)
        self.assertEqual(app.engine.next_calls, 1)
        self.assertEqual(len(app.after_calls), 1)
        self.assertEqual(app.after_calls[0][0], 50)


if __name__ == "__main__":
    unittest.main()
