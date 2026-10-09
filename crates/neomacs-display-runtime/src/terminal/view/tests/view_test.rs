use super::*;

#[test]
fn test_portable_pty_explicit_cmd() {
    use std::io::Read;

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("create pty");
    let mut cmd = CommandBuilder::new("/bin/sh");
    cmd.args(["-c", "echo PORTABLE_PTY_OK; sleep 1"]);
    let mut child = pair.slave.spawn_command(cmd).expect("spawn child");
    let mut reader = pair.master.try_clone_reader().expect("clone");
    let mut buf = [0u8; 4096];
    std::thread::sleep(std::time::Duration::from_millis(500));

    match reader.read(&mut buf) {
        Ok(n) if n > 0 => {
            let output = String::from_utf8_lossy(&buf[..n]);
            assert!(output.contains("PORTABLE_PTY_OK"));
        }
        Ok(_) => panic!("EOF"),
        Err(e) => panic!("Read error: {}", e),
    }

    let _ = child.wait();
}

#[cfg(target_os = "linux")]
fn process_exists(pid: u32) -> bool {
    // SAFETY: signal 0 performs existence/permission checking only.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(target_os = "linux")]
fn reader_thread_exists(name: &str) -> bool {
    std::fs::read_dir("/proc/self/task")
        .expect("read process task directory")
        .filter_map(Result::ok)
        .any(|task| {
            std::fs::read_to_string(task.path().join("comm")).is_ok_and(|comm| comm.trim() == name)
        })
}

/// Naming a thread is the thread's own first act, so the `comm` entry appears
/// some time after `spawn` returns -- on a loaded machine, long after.  Waiting
/// for the precondition keeps the test measuring what it is about (destroy
/// reaps) instead of how promptly the scheduler ran a new thread.  Only the
/// precondition waits: the post-destroy check stays instantaneous, because
/// `destroy` joins the thread and a joined thread is gone.
#[cfg(target_os = "linux")]
fn wait_for_reader_thread(name: &str) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if reader_thread_exists(name) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    reader_thread_exists(name)
}

#[test]
#[cfg(target_os = "linux")]
fn destroying_terminal_reaps_child_and_joins_reader_thread() {
    let id = TerminalId::new(77).expect("nonzero terminal id");
    let size = TerminalGridSize::new(20, 5).expect("positive terminal grid");
    let view = TerminalView::new(
        id,
        size,
        TerminalDisplayTarget::Window {
            buffer: neovm_core::buffer::BufferId(9),
        },
        Some("/bin/sh"),
    )
    .expect("create real PTY shell");
    let pid = view.child_process_id().expect("shell process id");
    let thread_name = format!("neo-term-{id}-pty");
    let mut manager = TerminalManager::new();
    manager.terminals.insert(id, view);

    assert!(process_exists(pid));
    assert!(
        wait_for_reader_thread(&thread_name),
        "reader thread {thread_name} never started"
    );
    assert!(manager.destroy(id).expect("destroy terminal"));

    let process_leaked = process_exists(pid);
    let thread_leaked = reader_thread_exists(&thread_name);
    if process_leaked {
        // Leave no child behind when this regression intentionally fails.
        // SAFETY: PID came from the child spawned immediately above.
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGKILL);
        }
    }

    assert!(!process_leaked, "destroy left PTY child {pid} alive");
    assert!(
        !thread_leaked,
        "destroy left reader thread {thread_name} alive"
    );
}
