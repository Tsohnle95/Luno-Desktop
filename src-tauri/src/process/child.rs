//! Process-tree setup and termination shared by managed media helpers.

use std::process::{Child, Command};
use std::time::{Duration, Instant};

pub const DEFAULT_KILL_GRACE: Duration = Duration::from_millis(250);

/// Put a helper and its subprocesses in a private group so cancellation reaches
/// yt-dlp/ffmpeg descendants as well as the shell wrapper.
pub fn isolate_process_group(command: &mut Command) {
    #[cfg(unix)]
    unsafe {
        use std::os::unix::process::CommandExt;
        command.pre_exec(|| {
            if libc::setpgid(0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

/// Stop the complete process group, reap the direct child, and escalate if a
/// descendant ignores TERM. The group is checked after reaping the direct
/// child because the wrapper can exit before one of its own children does.
pub fn terminate_process_tree(child: &mut Child, grace: Duration) {
    #[cfg(unix)]
    {
        let group = -(child.id() as i32);
        unsafe {
            let _ = libc::kill(group, libc::SIGTERM);
        }
        let deadline = Instant::now() + grace;
        loop {
            let group_exists = unsafe { libc::kill(group, 0) == 0 };
            if !group_exists || Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        // Keep the direct child unreaped until all signals are sent: once its
        // PID is reaped, the OS can reuse that number for an unrelated group.
        if unsafe { libc::kill(group, 0) == 0 } {
            unsafe {
                let _ = libc::kill(group, libc::SIGKILL);
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = grace;
    }
    let _ = child.kill();
    let _ = child.wait();
}
