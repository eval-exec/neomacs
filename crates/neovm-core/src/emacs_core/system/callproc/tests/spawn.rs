//! Pipe-stdio child creation: session isolation (issue #132), descriptor
//! wiring, environment and working-directory semantics, output draining,
//! and the `posix_spawn` engine selection.

use super::{ChildCommand, ChildStdio};
use std::io::{Read, Write};

fn sh(script: &str) -> ChildCommand {
    let mut command = ChildCommand::new("sh");
    command.arg("-c").arg(script);
    command
}

/// Regression test for issue #132: every spawned pipe-stdio child must live
/// in its own *session* (`setsid`) -- its own process group AND no
/// controlling terminal.  The process group stops a child's SIGTSTP/SIGTTOU
/// from suspending the editor (the suspend); the lack of a controlling
/// terminal stops an interactive `bash -i` from being SIGTTOU/SIGTTIN-stopped
/// as a background process group, which would wedge a synchronous
/// `call-process` forever (the hang).
#[test]
fn child_runs_in_its_own_session() {
    let parent_pgid = unsafe { libc::getpgrp() };
    let parent_sid = unsafe { libc::getsid(0) };
    let mut child = sh("sleep 1")
        .stdin(ChildStdio::Null)
        .stdout(ChildStdio::Null)
        .stderr(ChildStdio::Null)
        .spawn()
        .expect("spawn child");
    let pid = child.id() as libc::pid_t;
    // Read the child's process group + session while it is still alive.
    let child_pgid = unsafe { libc::getpgid(pid) };
    let child_sid = unsafe { libc::getsid(pid) };
    let _ = child.kill();
    let _ = child.wait();
    assert!(child_pgid > 0, "getpgid failed for live child");
    assert_ne!(
        child_pgid, parent_pgid,
        "child shares the editor's process group; its SIGTSTP/SIGTTOU could suspend neomacs (#132 suspend)"
    );
    assert_eq!(
        child_pgid, pid,
        "isolated child should lead its own process group"
    );
    assert!(child_sid > 0, "getsid failed for live child");
    assert_eq!(
        child_sid, pid,
        "isolated child should lead its own session (setsid)"
    );
    assert_ne!(
        child_sid, parent_sid,
        "child shares the editor's session/controlling terminal (#132 hang)"
    );
}

/// The `fork` fallback used for pty children carries the same isolation.
#[test]
fn forking_command_keeps_the_session_isolation() {
    let parent_sid = unsafe { libc::getsid(0) };
    let mut command = sh("sleep 1");
    command
        .stdin(ChildStdio::Null)
        .stdout(ChildStdio::Null)
        .stderr(ChildStdio::Null);
    let mut child = command
        .into_forking_command()
        .expect("retain child exit status")
        .spawn()
        .expect("spawn forked child");
    let pid = child.id() as libc::pid_t;
    let child_sid = unsafe { libc::getsid(pid) };
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(child_sid, pid, "forked child should lead its own session");
    assert_ne!(child_sid, parent_sid);
}

#[test]
fn glibc_linux_spawns_without_forking() {
    assert_eq!(
        ChildCommand::uses_posix_spawn(),
        cfg!(all(target_os = "linux", target_env = "gnu")),
        "posix_spawn engine selection must follow the platform gate"
    );
}

#[test]
fn output_captures_both_streams_and_the_exit_status() {
    let output = sh("printf out; printf err >&2; exit 3")
        .output()
        .expect("run child");
    assert_eq!(output.stdout, b"out");
    assert_eq!(output.stderr, b"err");
    assert_eq!(output.status.code(), Some(3));
}

#[test]
#[cfg(unix)]
fn externally_reaped_child_permanently_revokes_signal_authority() {
    // A competing exact-PID reaper violates the owner contract. Subsequent
    // waits must not revive a handle whose loss has already been observed.
    for fork in [false, true] {
        let mut command = sh("exit 0");
        let mut child = if fork {
            super::super::SpawnedChild::from_std(
                command.into_forking_command().unwrap().spawn().unwrap(),
            )
        } else {
            command.spawn().unwrap()
        };
        let mut status = 0;
        assert_eq!(
            unsafe { libc::waitpid(child.id() as i32, &mut status, 0) },
            child.id() as i32
        );
        assert_eq!(
            child.try_wait().unwrap_err().raw_os_error(),
            Some(libc::ECHILD)
        );
        assert_eq!(child.kill().unwrap_err().raw_os_error(), Some(libc::ECHILD));
        assert_eq!(child.wait().unwrap_err().raw_os_error(), Some(libc::ECHILD));
        assert_eq!(child.kill().unwrap_err().raw_os_error(), Some(libc::ECHILD));
    }
}

#[test]
#[cfg(unix)]
fn interruptible_output_reaps_the_exact_child_even_without_open_output_pipes() {
    for script in ["exec sleep 3600", "exec 1>&- 2>&-; exec sleep 3600"] {
        for piped in [false, true] {
            let mut command = sh(script);
            command
                .stdin(ChildStdio::Null)
                .stdout(if piped {
                    ChildStdio::Piped
                } else {
                    ChildStdio::Null
                })
                .stderr(if piped {
                    ChildStdio::Piped
                } else {
                    ChildStdio::Null
                });
            let child = command.spawn().unwrap();
            let pid = child.id() as libc::pid_t;
            let mut polls = 0;
            let start = std::time::Instant::now();
            let error = child
                .wait_with_output_interruptible(|| {
                    polls += 1;
                    if polls < 2 {
                        Ok(())
                    } else {
                        Err(std::io::ErrorKind::Interrupted.into())
                    }
                })
                .unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
            assert!(start.elapsed() < std::time::Duration::from_secs(5));
            let mut status = 0;
            // SAFETY: this read-only WNOHANG probe must find no waitable child;
            // the synchronous owner already killed AND reaped it.
            assert_eq!(
                unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) },
                -1
            );
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::ECHILD)
            );
        }
    }
}

#[test]
fn output_defaults_stdin_to_closed() {
    // `cat` sees immediate EOF on a closed stdin instead of blocking.
    let output = ChildCommand::new("cat").output().expect("run cat");
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn piped_stdin_reaches_the_child() {
    let mut child = ChildCommand::new("cat")
        .stdin(ChildStdio::Piped)
        .stdout(ChildStdio::Piped)
        .stderr(ChildStdio::Null)
        .spawn()
        .expect("spawn cat");
    let mut stdin = child.stdin.take().expect("piped stdin");
    stdin
        .write_all(b"hello through the pipe")
        .expect("write stdin");
    drop(stdin);
    let output = child.wait_with_output().expect("collect output");
    assert_eq!(output.stdout, b"hello through the pipe");
    assert!(output.status.success());
}

/// Both pipes are drained concurrently: a child that fills stderr past the
/// pipe buffer while stdout is still being read must not deadlock.
#[test]
fn wait_with_output_drains_both_pipes_past_the_pipe_buffer() {
    let chunk = "x".repeat(4096);
    let script = format!(
        "i=0; while [ $i -lt 64 ]; do printf '{chunk}' >&2; printf '{chunk}'; i=$((i+1)); done"
    );
    let output = sh(&script).output().expect("run child");
    assert_eq!(output.stdout.len(), 64 * 4096);
    assert_eq!(output.stderr.len(), 64 * 4096);
    assert!(output.status.success());
}

#[test]
fn stdout_can_be_redirected_to_a_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("captured");
    let file = std::fs::File::create(&path).expect("create capture file");
    let status = sh("printf into-file")
        .stdout(ChildStdio::from(file))
        .stderr(ChildStdio::Null)
        .spawn()
        .expect("spawn")
        .wait()
        .expect("wait");
    assert!(status.success());
    assert_eq!(
        std::fs::read(&path).expect("read capture file"),
        b"into-file"
    );
}

#[test]
fn stdout_can_be_a_pipe_writer_shared_with_stderr() {
    let (mut reader, writer) = os_pipe::pipe().expect("pipe");
    let stderr_writer = writer.try_clone().expect("dup writer");
    let mut child = sh("printf one; printf two >&2")
        .stdout(writer)
        .stderr(stderr_writer)
        .spawn()
        .expect("spawn");
    // The child holds the only remaining write ends; EOF follows its exit.
    let mut collected = String::new();
    reader
        .read_to_string(&mut collected)
        .expect("read shared pipe");
    assert!(child.wait().expect("wait").success());
    assert_eq!(collected, "onetwo");
}

#[test]
fn working_directory_is_honored() {
    let dir = tempfile::tempdir().expect("tempdir");
    let canonical = dir.path().canonicalize().expect("canonical tempdir");
    let output = sh("pwd").current_dir(&canonical).output().expect("run pwd");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        canonical.to_string_lossy()
    );
}

#[test]
fn missing_working_directory_is_a_spawn_error() {
    let error = sh("true")
        .current_dir("/nonexistent/neomacs-spawn-test")
        .output()
        .expect_err("spawn into a missing directory must fail");
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn environment_is_inherited_unless_cleared() {
    // SAFETY: test-only; no other thread reads the environment concurrently.
    unsafe { std::env::set_var("NEOMACS_SPAWN_TEST_INHERITED", "yes") };
    let output = sh("printf \"$NEOMACS_SPAWN_TEST_INHERITED\"")
        .output()
        .expect("run child");
    assert_eq!(output.stdout, b"yes");

    let output = sh("printf \"[$NEOMACS_SPAWN_TEST_INHERITED]\"")
        .env_clear()
        .output()
        .expect("run child");
    assert_eq!(output.stdout, b"[]");
}

#[test]
fn environment_edits_apply_over_the_inherited_environment() {
    // SAFETY: test-only; no other thread reads the environment concurrently.
    unsafe { std::env::set_var("NEOMACS_SPAWN_TEST_REMOVED", "still-here") };
    let output = sh("printf \"$NEOMACS_SPAWN_TEST_SET|$NEOMACS_SPAWN_TEST_REMOVED\"")
        .env("NEOMACS_SPAWN_TEST_SET", "set-value")
        .env_remove("NEOMACS_SPAWN_TEST_REMOVED")
        .output()
        .expect("run child");
    assert_eq!(output.stdout, b"set-value|");
}

#[test]
fn a_cleared_environment_only_contains_the_edits() {
    let output = ChildCommand::new("env")
        .env_clear()
        .env("ONLY_ONE", "1")
        .env("ONLY_TWO", "2")
        .output()
        .expect("run env");
    let mut lines: Vec<&str> = std::str::from_utf8(&output.stdout)
        .expect("utf8")
        .lines()
        .collect();
    lines.sort_unstable();
    assert_eq!(lines, ["ONLY_ONE=1", "ONLY_TWO=2"]);
}

/// A `PATH` set for the child resolves a bare program name, as it does with
/// `std::process::Command`.
#[test]
fn a_child_path_resolves_a_bare_program_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    let script = dir.path().join("neomacs-spawn-probe");
    std::fs::write(&script, "#!/bin/sh\nprintf found-on-child-path\n").expect("write script");
    let mut permissions = std::fs::metadata(&script).expect("metadata").permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    std::fs::set_permissions(&script, permissions).expect("chmod");

    let output = ChildCommand::new("neomacs-spawn-probe")
        .env("PATH", dir.path())
        .output()
        .expect("run probe via child PATH");
    assert_eq!(output.stdout, b"found-on-child-path");
}

#[test]
fn a_missing_program_is_not_found_and_leaves_no_child() {
    let error = ChildCommand::new("/nonexistent/neomacs-spawn-missing")
        .output()
        .expect_err("missing program must fail to spawn");
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn an_argument_with_an_interior_nul_is_rejected() {
    let error = ChildCommand::new("sh")
        .arg("-c")
        .arg("printf a\0b")
        .output()
        .expect_err("interior NUL must be rejected");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
}

#[test]
fn try_wait_reports_running_then_exited() {
    let mut child = ChildCommand::new("cat")
        .stdin(ChildStdio::Piped)
        .stdout(ChildStdio::Null)
        .stderr(ChildStdio::Null)
        .spawn()
        .expect("spawn cat");
    assert!(
        child.try_wait().expect("try_wait").is_none(),
        "cat waiting on stdin must still be running"
    );
    drop(child.stdin.take());
    let status = child.wait().expect("wait");
    assert!(status.success());
    assert_eq!(
        child.try_wait().expect("try_wait after wait"),
        Some(status),
        "a reaped child reports its cached status"
    );
}

#[test]
fn kill_after_reaping_is_a_no_op() {
    let mut child = sh("exit 0")
        .stdin(ChildStdio::Null)
        .stdout(ChildStdio::Null)
        .stderr(ChildStdio::Null)
        .spawn()
        .expect("spawn");
    child.wait().expect("wait");
    child
        .kill()
        .expect("kill after wait must not signal a possibly reused pid");
}

#[test]
fn signals_the_editor_ignores_are_default_in_the_child() {
    // The editor ignores SIGPIPE (Rust's runtime does so at startup); the
    // child must not inherit that or `yes | head` style pipelines never end.
    let output = sh("trap - PIPE; yes | head -c 1 >/dev/null; printf done")
        .output()
        .expect("run child");
    assert_eq!(output.stdout, b"done");
    assert!(output.status.success());
}
