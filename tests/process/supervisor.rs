use luno_desktop_lib::process::executables::{discover, ExecutableKind};
use luno_desktop_lib::process::supervisor::{ProcessRegistry, ProcessSpec, SuperviseError, Supervisor};
use std::time::Duration;

#[test]
fn missing_binary_is_explicit() {
    let path = std::env::temp_dir().join(format!("luno-no-such-{}", std::process::id()));
    assert!(discover(ExecutableKind::Ffmpeg, Some(&path)).is_err());
}

#[cfg(unix)]
#[test]
fn captures_with_bound_and_times_out() {
    let supervisor = Supervisor::new(ProcessRegistry::new());
    let mut spec = ProcessSpec::new("/bin/sh", vec!["-c".into(), "printf abc; sleep 5".into()], Duration::from_millis(50));
    spec.output_limit = 2;
    assert!(matches!(supervisor.run(spec), Err(SuperviseError::Timeout)));
}

#[cfg(unix)]
#[test]
fn cancellation_terminates_process() {
    let registry = ProcessRegistry::new();
    let supervisor = Supervisor::new(registry.clone());
    let thread = std::thread::spawn(move || supervisor.run(ProcessSpec::new("/bin/sh", vec!["-c".into(), "sleep 5".into()], Duration::from_secs(10))));
    std::thread::sleep(Duration::from_millis(30));
    registry.cancel_all();
    assert!(matches!(thread.join().unwrap(), Err(SuperviseError::Cancelled)));
    assert_eq!(registry.active_count(), 0);
}

#[cfg(unix)]
#[test]
fn admitted_process_is_rejected_after_app_shutdown_begins() {
    let state = luno_desktop_lib::state::AppState::new(1);
    state.tasks.begin_shutdown();
    let result = state.run_process(ProcessSpec::new("/bin/sh", vec!["-c".into(), "exit 0".into()], Duration::from_secs(1)));
    assert!(matches!(result, Err(SuperviseError::Cancelled)));
    assert_eq!(state.process_count(), 0);
}
