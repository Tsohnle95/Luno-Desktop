use luno_desktop_lib::contracts::TrackId;
use luno_desktop_lib::playback::queue::{QueueState, RepeatMode, HISTORY_LIMIT};

fn id(value: &str) -> TrackId { TrackId::new(value).unwrap() }

#[test]
fn context_and_up_next_preserve_manual_order() {
    let mut q = QueueState::new(); q.play_with_context(id("b"), vec![id("a"), id("b"), id("c"), id("d")]);
    q.play_next(vec![id("x"), id("y")]); q.add_to_queue(vec![id("z")]);
    assert_eq!(q.queue(), &[id("a"), id("b"), id("x"), id("y"), id("z"), id("c"), id("d")]); assert_eq!(q.user_queue_count(), 3);
}

#[test]
fn shuffle_restores_order_and_keeps_current() {
    let mut q = QueueState::new(); q.play_with_context(id("c"), vec![id("a"), id("b"), id("c"), id("d")]); q.toggle_shuffle();
    assert_eq!(q.current(), Some(&id("c"))); q.toggle_shuffle(); assert_eq!(q.queue(), &[id("a"), id("b"), id("c"), id("d")]);
}

#[test]
fn repeat_modes_obey_boundaries() {
    let mut q = QueueState::new(); q.play_with_context(id("b"), vec![id("a"), id("b")]); q.set_repeat(RepeatMode::One); assert_eq!(q.next(), Some(&id("b")));
    q.set_repeat(RepeatMode::Off); assert_eq!(q.next(), None); q.set_repeat(RepeatMode::All); assert_eq!(q.next(), Some(&id("a")));
}

#[test]
fn history_is_bounded_and_preview_is_not_physical_queue() {
    let mut q = QueueState::new(); q.play_with_context(id("seed"), vec![id("seed")]); q.start_preview(vec![id("p1"), id("p2")]); q.record_started(id("p1"), true);
    for i in 0..(HISTORY_LIMIT + 1) { q.record_started(id(&format!("{i}")), false); }
    assert_eq!(q.previews(), &[id("p1"), id("p2")]); assert_eq!(q.queue(), &[id("seed")]); assert_eq!(q.history().len(), HISTORY_LIMIT); q.clear(); assert!(q.queue().is_empty() && q.previews().is_empty());
}
