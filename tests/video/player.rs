use luno_desktop_lib::video::player::*;
use std::time::Duration;

#[derive(Default)] struct Fake { starts: Vec<Duration>, stops: usize, clears: usize }
impl Decoder for Fake {
    fn start(&mut self, position: Duration, _: VideoSize) -> Result<(), String> { self.starts.push(position); Ok(()) }
    fn stop(&mut self) { self.stops += 1; }
    fn clear_buffer(&mut self) { self.clears += 1; }
}
struct Factory { made: usize }
impl DecoderFactory for Factory { type Decoder = Fake; fn create(&mut self, _: VideoGeneration) -> Result<Fake, String> { self.made += 1; Ok(Fake::default()) } }

#[test]
fn pause_resume_and_seek_follow_audio_clock() {
    let mut player = VideoPlayer::new(Factory { made: 0 }, VideoSize { width: 640, height: 360 });
    let generation = player.load(Duration::from_secs(3));
    assert!(player.start(generation, Duration::from_secs(3)));
    player.pause(Duration::from_secs(4));
    assert_eq!(player.status(), VideoStatus::Paused);
    player.resume(Duration::from_secs(4));
    assert_eq!(player.status(), VideoStatus::Playing);
    let before = player.generation();
    player.seek(Duration::from_secs(9));
    assert!(player.generation().0 > before.0);
    assert!(player.take_events().iter().any(|e| matches!(e, VideoEvent::Restarted { position, .. } if *position == Duration::from_secs(9))));
}

#[test]
fn drift_and_resize_restart_without_independent_clock() {
    let mut player = VideoPlayer::new(Factory { made: 0 }, VideoSize { width: 640, height: 360 });
    let generation = player.load(Duration::ZERO); player.start(generation, Duration::ZERO);
    let old = player.generation(); player.sync(Duration::from_millis(250));
    assert!(player.generation().0 > old.0);
    let old = player.generation(); player.resize(VideoSize { width: 1280, height: 720 }, Duration::from_secs(1));
    assert!(player.generation().0 > old.0);
    assert_eq!(player.size(), VideoSize { width: 1280, height: 720 });
}

#[test]
fn eof_fullscreen_and_stale_generation_are_safe() {
    let mut player = VideoPlayer::new(Factory { made: 0 }, VideoSize { width: 1, height: 1 });
    let generation = player.load(Duration::ZERO);
    assert!(!player.start(VideoGeneration(generation.0 - 1), Duration::ZERO));
    player.start(generation, Duration::ZERO); player.set_fullscreen(true); player.decoder_eof();
    assert_eq!(player.status(), VideoStatus::Ended); assert!(player.is_fullscreen());
    player.set_fullscreen(false); player.shutdown(); assert_eq!(player.status(), VideoStatus::Idle);
}
