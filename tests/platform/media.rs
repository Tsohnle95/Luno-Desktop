use luno_desktop_lib::platform::media_session::*;
use luno_desktop_lib::playback::sleep_timer::*;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Default)]
struct Fake { registrations: Arc<Mutex<usize>>, handler: Arc<Mutex<Option<Handler>>>, published: Arc<Mutex<Vec<NowPlaying>>>, cleared: Arc<Mutex<usize>> }
impl MediaSessionRegistrar for Fake {
    fn register(&mut self, handler: Handler) -> Result<(), String> { *self.registrations.lock().unwrap() += 1; *self.handler.lock().unwrap() = Some(handler); Ok(()) }
    fn unregister(&mut self) { *self.registrations.lock().unwrap() -= 1; }
    fn publish(&mut self, info: &NowPlaying) -> Result<(), String> { self.published.lock().unwrap().push(info.clone()); Ok(()) }
    fn clear(&mut self) { *self.cleared.lock().unwrap() += 1; }
}

#[test]
fn timer_options_cancel_fire_and_reset() {
    let start = Instant::now(); let mut timer = SleepTimerState::new();
    timer.set(SleepTimer::Minutes(5), start); assert_eq!(timer.remaining(start), Some(Duration::from_secs(300)));
    timer.reset(SleepTimer::Minutes(15), start); assert!(!timer.poll(start + Duration::from_secs(301)));
    assert!(timer.poll(start + Duration::from_secs(900))); assert!(!timer.poll(start + Duration::from_secs(901)));
    timer.set(SleepTimer::Minutes(5), start); timer.cancel(); assert!(!timer.active());
}

#[test]
fn media_session_guards_duplicates_routes_metadata_and_shutdown() {
    let fake = Fake::default(); let handler_ref = fake.handler.clone(); let regs = fake.registrations.clone(); let cleared = fake.cleared.clone();
    let mut session = MediaSession::new(fake); assert_eq!(session.register().unwrap(), false);
    session.set_ready(true); assert_eq!(session.register().unwrap(), true); assert_eq!(session.register().unwrap(), false); assert_eq!(*regs.lock().unwrap(), 1);
    handler_ref.lock().unwrap().as_ref().unwrap()(MediaCommand::Next); assert_eq!(session.drain_commands(), vec![MediaCommand::Next]);
    assert!(session.publish(NowPlaying { title: "Song".into(), artist: "Artist".into(), ..Default::default() }).unwrap());
    session.shutdown(); assert_eq!(*cleared.lock().unwrap(), 1); assert_eq!(*regs.lock().unwrap(), 0); assert!(!session.is_registered());
    assert_eq!(xf86_command("XF86AudioPlay"), Some(MediaCommand::Toggle)); assert_eq!(xf86_command("other"), None);
}
