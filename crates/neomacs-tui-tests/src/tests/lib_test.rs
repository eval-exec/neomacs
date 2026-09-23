use super::{
    TuiLaunch, TuiProcessOutcome, TuiSession, TuiTempDirectory, TuiTerminalConfig, emacs_key,
    neomacs_binary_path_from_override,
    recording::{RecordingIdentity, RecordingPolicy},
};
use std::ffi::OsString;
use std::io::{Read as _, Write as _};

#[test]
fn private_parent_temp_directory_exposes_a_nested_owned_directory() {
    let directory = TuiTempDirectory::new_with_private_parent("tui-private-parent-", "listing");
    let exposed = directory.path().to_path_buf();
    let owner = exposed
        .parent()
        .expect("nested fixture directory should have a private parent")
        .to_path_buf();

    assert_eq!(
        exposed.file_name().and_then(|name| name.to_str()),
        Some("listing")
    );
    assert!(exposed.is_dir());
    assert!(owner.is_dir());

    drop(directory);

    assert!(!owner.exists(), "private parent survived fixture drop");
}
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[test]
fn structured_launch_preserves_spaces_in_arguments_and_environment() {
    let launch = TuiLaunch::new("sh")
        .args(["-c", "printf '%s' \"$NEOMACS_TUI_STRUCTURED_VALUE\""])
        .env("NEOMACS_TUI_STRUCTURED_VALUE", "alpha beta");
    let mut session = TuiSession::spawn_launch(launch, "STRUCTURED");

    session.read_until(Duration::from_secs(2), |grid| {
        grid.iter().any(|row| row.contains("alpha beta"))
    });

    assert!(
        session
            .text_grid()
            .iter()
            .any(|row| row.contains("alpha beta"))
    );
}

const NONBLOCKING_PTY_FIXTURE: &str = "NEOMACS_TUI_NONBLOCKING_PTY_FIXTURE";

/// Model GNU's TTY descriptor setup: stdin/stdout are dup'd from one PTY
/// slave open-file description, so setting O_NONBLOCK while polling stdin
/// also makes terminal output nonblocking (`src/keyboard.c:8256`).
#[test]
fn nonblocking_pty_output_fixture() {
    if std::env::var_os(NONBLOCKING_PTY_FIXTURE).is_none() {
        return;
    }

    println!("fixture-ready");
    std::io::stdout().flush().expect("flush fixture readiness");
    let mut input = [0_u8; 1];
    std::io::stdin()
        .read_exact(&mut input)
        .expect("read fixture trigger");

    let flags = unsafe { libc::fcntl(libc::STDIN_FILENO, libc::F_GETFL) };
    assert!(flags >= 0, "read fixture descriptor flags");
    assert_eq!(
        unsafe { libc::fcntl(libc::STDIN_FILENO, libc::F_SETFL, flags | libc::O_NONBLOCK,) },
        0,
        "make the shared slave description nonblocking",
    );

    let block = [b'x'; 512];
    for _ in 0..128 {
        // Deliberately mirror GNU `tty_write_glyphs_1`: output is attempted
        // once and a short/EAGAIN write is not retried.
        unsafe {
            libc::write(libc::STDOUT_FILENO, block.as_ptr().cast(), block.len());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    unsafe {
        libc::write(
            libc::STDOUT_FILENO,
            b"fixture-complete\n".as_ptr().cast(),
            b"fixture-complete\n".len(),
        );
    }
}

#[test]
fn tui_session_drains_nonblocking_output_between_client_observations() {
    let launch = TuiLaunch::new(std::env::current_exe().expect("current test executable"))
        .args([
            "--exact",
            "tests::nonblocking_pty_output_fixture",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(NONBLOCKING_PTY_FIXTURE, "1");
    let mut session = TuiSession::spawn_launch(launch, "NONBLOCKING-OUTPUT");
    session.read_until(Duration::from_secs(2), |grid| {
        grid.iter().any(|row| row.contains("fixture-ready"))
    });
    session.clear_recent_output();

    session.send(b"x\n");
    // Test clients do real work between observations. The transport must
    // keep draining independently during that interval, like a terminal.
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(
        session.run_to_completion(Duration::from_secs(2)),
        TuiProcessOutcome::Exited,
    );

    assert!(
        session
            .recent_output()
            .windows(b"fixture-complete".len())
            .any(|window| window == b"fixture-complete"),
        "the PTY queue filled while the client was not calling read; the nonblocking child lost its output tail",
    );
}

#[test]
fn tui_session_records_the_pty_interaction_at_its_public_artifact_path() {
    let artifacts = tempfile::tempdir().expect("create recording root");
    let launch = TuiLaunch::new("sh").args([
        "-c",
        "printf ready; IFS= read -r line; printf 'done:%s' \"$line\"",
    ]);
    let mut session = TuiSession::spawn_launch_for_recording_test(
        launch,
        "GNU",
        TuiTerminalConfig::new("xterm-256color", 24, 80),
        RecordingPolicy::On,
        artifacts.path(),
        RecordingIdentity::new("neomacs-tui-tests", "pty interaction", "GNU"),
    );
    session.read(Duration::from_secs(1));
    session.send(b"go\n");
    session.resize(30, 90);
    session.mark_recording("command complete");
    assert_eq!(
        session.run_to_completion(Duration::from_secs(1)),
        TuiProcessOutcome::Exited
    );
    let path = session
        .recording_path()
        .expect("recording path")
        .to_path_buf();

    drop(session);

    let lines = std::fs::read_to_string(path)
        .expect("read session cast")
        .lines()
        .map(|line| serde_json::from_str(line).expect("valid event"))
        .collect::<Vec<serde_json::Value>>();
    assert_eq!(
        lines[0]["term"],
        serde_json::json!({"cols": 80, "rows": 24, "type": "xterm-256color"})
    );
    let events = &lines[1..];
    let output = events
        .iter()
        .filter(|event| event[1] == "o")
        .filter_map(|event| event[2].as_str())
        .collect::<String>();

    assert!(output.contains("ready"));
    assert!(output.contains("done:go"));
    assert!(
        events
            .iter()
            .any(|event| event[1] == "i" && event[2] == "go\n")
    );
    assert!(
        events
            .iter()
            .any(|event| event[1] == "r" && event[2] == "90x30")
    );
    assert!(
        events
            .iter()
            .any(|event| event[1] == "m" && event[2] == "command complete")
    );
    assert!(events.iter().any(|event| event[1] == "x"));
}

#[test]
fn tui_session_recording_is_disabled_by_default() {
    let artifacts = tempfile::tempdir().expect("create recording root");
    let session = TuiSession::spawn_launch_for_recording_test(
        TuiLaunch::new("sh").args(["-c", "printf ignored"]),
        "NEO",
        TuiTerminalConfig::default(),
        RecordingPolicy::default(),
        artifacts.path(),
        RecordingIdentity::new("neomacs-tui-tests", "recording off", "NEO"),
    );

    assert_eq!(session.recording_path(), None);
    drop(session);
    assert!(
        std::fs::read_dir(artifacts.path())
            .expect("read recording root")
            .next()
            .is_none()
    );
}

#[test]
fn structured_launch_never_deletes_a_caller_owned_home() {
    let external_home = tempfile::tempdir().expect("create caller-owned HOME");
    let sentinel = external_home.path().join("keep-me");
    std::fs::write(&sentinel, "owned by caller").expect("write HOME sentinel");
    let launch = TuiLaunch::new("sh")
        .args(["-c", "printf done"])
        .env("HOME", external_home.path().as_os_str());

    let mut session = TuiSession::spawn_launch(launch, "EXTERNAL-HOME");
    session.read(Duration::from_secs(1));
    drop(session);

    assert!(sentinel.is_file(), "TUI session deleted caller-owned HOME");
}

#[test]
fn structured_launch_never_deletes_a_caller_owned_tmpdir() {
    let external_tmp = tempfile::tempdir().expect("create caller-owned TMPDIR");
    let sentinel = external_tmp.path().join("keep-me");
    std::fs::write(&sentinel, "owned by caller").expect("write TMPDIR sentinel");
    let launch = TuiLaunch::new("sh")
        .args(["-c", "printf done"])
        .env("TMPDIR", external_tmp.path().as_os_str());

    let mut session = TuiSession::spawn_launch(launch, "EXTERNAL-TMPDIR");
    session.read(Duration::from_secs(1));
    drop(session);

    assert!(
        sentinel.is_file(),
        "TUI session deleted caller-owned TMPDIR"
    );
}

#[test]
fn structured_launch_removes_harness_owned_directories() {
    let mut session = TuiSession::spawn_launch(
        TuiLaunch::new("sh").args(["-c", "printf done"]),
        "OWNED-DIRECTORIES",
    );
    session.read(Duration::from_secs(1));
    let home = session.home.path().to_path_buf();
    let tmp = session._tmp.path().to_path_buf();

    drop(session);

    assert!(!home.exists(), "harness-owned HOME survived session drop");
    assert!(!tmp.exists(), "harness-owned TMPDIR survived session drop");
}

#[test]
fn neomacs_binary_path_prefers_explicit_override() {
    let workspace = Path::new("/repo");
    let path =
        neomacs_binary_path_from_override(workspace, Some(OsString::from("/tmp/custom-neomacs")));

    assert_eq!(path, PathBuf::from("/tmp/custom-neomacs"));
}

#[test]
fn neomacs_binary_path_defaults_to_release_binary() {
    let workspace = Path::new("/repo");
    let path = neomacs_binary_path_from_override(workspace, None);

    assert_eq!(
        path,
        PathBuf::from("/repo")
            .join("target")
            .join("release")
            .join("neomacs")
    );
}

#[test]
fn emacs_key_maps_control_space_to_terminal_nul() {
    assert_eq!(emacs_key("C-SPC"), vec![0x00]);
    assert_eq!(emacs_key("C-@"), vec![0x00]);
    assert_eq!(emacs_key("C-M-SPC"), vec![0x1b, 0x00]);
    assert_eq!(emacs_key("C-M-@"), vec![0x1b, 0x00]);
    assert_eq!(emacs_key("C-/"), vec![0x1f]);
    assert_eq!(emacs_key("C-_"), vec![0x1f]);
    assert_eq!(emacs_key("C-M-/"), vec![0x1b, 0x1f]);
    assert_eq!(emacs_key("C-M-_"), vec![0x1b, 0x1f]);
}

#[test]
fn emacs_key_maps_control_semicolon_to_modify_other_keys() {
    assert_eq!(emacs_key("C-;"), b"\x1b[27;5;59~".to_vec());
}

#[test]
fn emacs_key_maps_meta_space_as_a_complete_named_key() {
    assert_eq!(emacs_key("M-SPC"), vec![0x1b, b' ']);
}

#[test]
fn emacs_key_maps_f10_to_screen_terminfo_sequence() {
    assert_eq!(emacs_key("F10"), b"\x1b[21~".to_vec());
    assert_eq!(emacs_key("f10"), b"\x1b[21~".to_vec());
}

#[test]
fn emacs_key_maps_arrow_keys_to_cursor_sequences() {
    assert_eq!(emacs_key("UP"), b"\x1b[A".to_vec());
    assert_eq!(emacs_key("DOWN"), b"\x1b[B".to_vec());
    assert_eq!(emacs_key("RIGHT"), b"\x1b[C".to_vec());
    assert_eq!(emacs_key("LEFT"), b"\x1b[D".to_vec());
}

#[test]
fn vt100_parser_does_not_render_decscusr_cursor_shape_as_text() {
    let mut parser = vt100::Parser::new(2, 40, 0);
    parser.process(b"\x1b[1;1HList lines matching regexp: \x1b[6 q\x1b[?25h");

    let row = parser.screen().contents_between(0, 0, 0, 40);
    let trimmed = row.trim_end();
    if trimmed != "List lines matching regexp:" {
        let mut bytes = String::new();
        for byte in b"\x1b[1;1HList lines matching regexp: \x1b[6 q\x1b[?25h" {
            let _ = write!(&mut bytes, "{byte:02x} ");
        }
        panic!("unexpected row {trimmed:?} for bytes [{bytes}]");
    }
}
