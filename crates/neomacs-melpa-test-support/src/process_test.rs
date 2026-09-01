#[cfg(unix)]
#[test]
fn timeout_kills_the_complete_child_process_group() {
    use std::process::Command;
    use std::time::{Duration, Instant};

    let mut command = Command::new("sh");
    command.args(["-c", "sleep 60 & child=$!; echo $child; wait"]);

    let started = Instant::now();
    let output = match super::group_output_with_timeout(&mut command, Duration::from_millis(100)) {
        Err(super::CommandError::TimedOut(output)) => output,
        result => panic!("child-spawning command must time out, got {result:?}"),
    };

    assert!(started.elapsed() < Duration::from_secs(5));
    let child_pid = String::from_utf8(output.stdout)
        .expect("child pid is UTF-8")
        .trim()
        .to_string();
    assert!(!child_pid.is_empty());
    assert!(
        !Command::new("kill")
            .args(["-0", &child_pid])
            .status()
            .expect("query child process")
            .success(),
        "timed-out descendant {child_pid} survived its process group"
    );
}

#[cfg(unix)]
#[test]
fn timeout_still_kills_the_group_when_its_leader_exits_first() {
    use std::process::Command;
    use std::time::{Duration, Instant};

    let mut command = Command::new("sh");
    command.args(["-c", "sleep 60 & echo $!"]);

    let started = Instant::now();
    let output = match super::group_output_with_timeout(&mut command, Duration::from_millis(100)) {
        Err(super::CommandError::TimedOut(output)) => output,
        result => panic!("leader-exits-first command must time out, got {result:?}"),
    };

    assert!(started.elapsed() < Duration::from_secs(5));
    let child_pid = String::from_utf8(output.stdout)
        .expect("child pid is UTF-8")
        .trim()
        .to_string();
    assert!(!child_pid.is_empty());
    assert!(
        !Command::new("kill")
            .args(["-0", &child_pid])
            .status()
            .expect("query child process")
            .success(),
        "descendant {child_pid} survived after its group leader exited"
    );
}

#[cfg(unix)]
#[test]
fn private_pty_timeout_kills_a_hup_ignoring_session_child() {
    use std::fs;
    use std::process::Command;
    use std::time::Duration;

    let workspace_root = super::workspace_root();
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-pty-timeout-")
        .tempdir_in(workspace_root.join("tmp"))
        .expect("create workspace-local process scratch directory");
    let child_pid_path = scratch.path().join("child.pid");
    let sentinel = scratch.path().join("never-written");
    let terminal_output = scratch.path().join("terminal.ansi");
    let child_script = format!(
        "trap '' HUP; echo $$ > {}; sleep 60",
        child_pid_path.display()
    );
    let mut command = Command::new("python3");
    command
        .arg(workspace_root.join("tools/bench/pty-run.py"))
        .args(["sh", "-c", &child_script])
        .env("SENTINEL", sentinel)
        .env("PTY_OUTPUT", terminal_output)
        .env("PTY_TIMEOUT", "0.1");

    let output = super::group_output_with_timeout(&mut command, Duration::from_secs(2))
        .expect("PTY runner must enforce its inner deadline");
    assert!(!output.status.success());
    let child_pid = fs::read_to_string(child_pid_path)
        .expect("PTY child published its pid")
        .trim()
        .to_string();
    assert!(
        !Command::new("kill")
            .args(["-0", &child_pid])
            .status()
            .expect("query PTY child")
            .success(),
        "HUP-ignoring PTY child {child_pid} survived its runner timeout"
    );
}
