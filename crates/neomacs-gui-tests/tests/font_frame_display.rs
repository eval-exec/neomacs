//! Font listing with simultaneous graphical and terminal frames.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    CommandSpec, DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions,
    ProcessGuiCommandRunner,
};
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

enum Engine {
    Neomacs,
    Gnu,
}

/// Keep both PTY ends alive and drain output while the editor owns a TTY frame.
/// All descriptors are typed owners; no personal terminal is opened.
struct PrivateTerminal {
    path: PathBuf,
    _slave: File,
    stop: Arc<AtomicBool>,
    reader: Option<JoinHandle<std::io::Result<()>>>,
}

impl PrivateTerminal {
    fn new(log: &Path) -> Self {
        use rustix::pty::{OpenptFlags, grantpt, ioctl_tiocgptpeer, openpt, ptsname, unlockpt};
        let flags = OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC;
        let master = openpt(flags).expect("create private PTY");
        grantpt(&master).unwrap();
        unlockpt(&master).unwrap();
        let path = PathBuf::from(ptsname(&master, Vec::new()).unwrap().to_str().unwrap());
        let slave = ioctl_tiocgptpeer(&master, flags).unwrap();
        rustix::termios::tcsetwinsize(
            &slave,
            rustix::termios::Winsize {
                ws_row: 32,
                ws_col: 100,
                ws_xpixel: 0,
                ws_ypixel: 0,
            },
        )
        .unwrap();
        let flags = rustix::fs::fcntl_getfl(&master).unwrap();
        rustix::fs::fcntl_setfl(&master, flags | rustix::fs::OFlags::NONBLOCK).unwrap();
        let mut master = File::from(master);
        let mut log = File::create(log).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let reader = thread::spawn(move || {
            let mut buffer = [0; 8192];
            let mut recorded = 0;
            while !thread_stop.load(Ordering::Relaxed) {
                match master.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        // Keep draining after the diagnostic log reaches 2MB.
                        let keep = count.min(2_000_000 - recorded);
                        log.write_all(&buffer[..keep])?;
                        recorded += keep;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(error) => return Err(error),
                }
            }
            Ok(())
        });
        Self {
            path,
            _slave: File::from(slave),
            stop,
            reader: Some(reader),
        }
    }

    fn finish(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.reader.take().unwrap().join().unwrap().unwrap();
    }
}

impl Drop for PrivateTerminal {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

#[test]
fn native_font_listing_uses_the_requested_frame_display() {
    run(Engine::Neomacs);
}

#[test]
fn gnu_font_listing_uses_the_requested_frame_display() {
    run(Engine::Gnu);
}

fn run(engine: Engine) {
    let root = neomacs_infra::workspace_root();
    let (name, binary, attestation) = match engine {
        Engine::Neomacs => {
            let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("target/release/neomacs"));
            let stamp = neomacs_test_env::fresh_build_stamp::verify(&binary, &root.join("lisp"))
                .expect("native GUI binary requires its matching fresh-build stamp");
            assert!(matches!(
                stamp,
                neomacs_test_env::fresh_build_stamp::StampVerdict::Verified { .. }
            ));
            ("native", binary, format!("{stamp:?}"))
        }
        Engine::Gnu => {
            let oracle = neomacs_test_oracle::gnu::gnu_oracle();
            assert!(
                oracle.is_attested(),
                "GNU control requires the pinned oracle"
            );
            ("gnu", oracle.executable_path(), oracle.stamp())
        }
    };
    eprintln!("{attestation}");
    let artifact_root = root.join(format!(
        "target/neomacs-gui-tests/font-frame-display-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let artifacts = GuiArtifactSet::new(&artifact_root, GuiBackend::LinuxX11, "font-frame-display");
    fs::create_dir_all(artifacts.json.parent().unwrap()).unwrap();
    let fonts = artifact_root.join("fonts");
    fs::create_dir(&fonts).unwrap();
    fs::copy(
        neomacs_test_fonts::hack_regular_3_003(),
        fonts.join("Hack-Regular.ttf"),
    )
    .unwrap();
    let config = artifact_root.join("fonts.conf");
    fs::write(
        &config,
        format!(
            "<fontconfig><dir>{}</dir><cachedir>{}</cachedir><alias><family>monospace</family><prefer><family>Hack</family></prefer></alias><alias><family>sans-serif</family><prefer><family>Hack</family></prefer></alias></fontconfig>",
            fonts.display(), artifact_root.join("fontcache").display()
        ),
    ).unwrap();
    let display = DisplayHarness::Xvfb.start_session(&artifact_root).unwrap();
    let tty_log = artifact_root.join("tty.log");
    let mut terminal = PrivateTerminal::new(&tty_log);
    let mut command = CommandSpec {
        program: binary,
        args: vec![
            "-Q".into(),
            "-l".into(),
            root.join("crates/neomacs-gui-tests/fixtures/font-frame-display.el")
                .display()
                .to_string(),
        ],
        env: vec![
            ("FONTCONFIG_FILE".into(), config.display().to_string()),
            ("WINIT_UNIX_BACKEND".into(), "x11".into()),
            ("GDK_BACKEND".into(), "x11".into()),
            ("TERM".into(), "xterm-256color".into()),
            (
                "NEOMACS_GUI_STATE_JSON".into(),
                artifacts.gui_state.display().to_string(),
            ),
            (
                "NEOMACS_FONT_FRAME_TTY".into(),
                terminal.path.display().to_string(),
            ),
        ],
    };
    command.env.extend(display.env().iter().cloned());
    let output = ProcessGuiCommandRunner
        .run(
            &command,
            &artifacts,
            &GuiRunOptions::with_timeout(Duration::from_secs(40)),
        )
        .unwrap();
    terminal.finish();
    fs::write(&artifacts.stdout, &output.stdout).unwrap();
    fs::write(&artifacts.stderr, &output.stderr).unwrap();
    let state = fs::read(&artifacts.gui_state)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    // Neither engine is required to produce a Neomacs GPU readback PNG.
    let passed = !output.timed_out
        && output.exit_code == Some(0)
        && state.as_ref().is_some_and(|state| state["passed"] == true);
    let manifest = json!({
        "engine": name, "scenario": "font-frame-display", "backend": "linux-x11",
        "attestation": attestation, "status": if passed { "passed" } else { "failed" },
        "process": {"exit_code": output.exit_code, "timed_out": output.timed_out},
        "command": {"program": command.program, "args": command.args,
                    "env": command.env.into_iter().collect::<std::collections::BTreeMap<_, _>>()},
        "expected_artifacts": {"gui_state": artifacts.gui_state, "stdout": artifacts.stdout,
                               "stderr": artifacts.stderr, "tty": tty_log},
        "gui_state": state,
    });
    fs::write(&artifacts.json, format!("{manifest}\n")).unwrap();
    assert!(passed, "{}: {manifest}", artifacts.json.display());
    let state = &manifest["gui_state"];
    assert_eq!(state["gui-graphic"], true);
    assert_eq!(state["tty-graphic"], false);
    assert_eq!(state["script-cases"], 9);
    let observations = state["observations"].as_array().unwrap();
    let expected = [
        "script-explicit-nil-han",
        "script-missing-property",
        "script-all-latin-and-han",
        "script-any-latin-or-han",
        "script-any-empty",
        "script-any-invalid",
        "script-unknown",
        "script-legacy-overrides-all",
        "script-legacy-overrides-any",
        "gui-selected-tty-default-face-negative-width",
        "gui-selected-tty-invalid-face-negative-width",
        "tty-selected-nil-frame",
        "tty-selected-tty-frame",
        "tty-selected-gui-frame-globals-nil",
    ];
    assert_eq!(observations.len(), expected.len());
    for (observation, label) in observations.iter().zip(expected) {
        assert_eq!(observation["label"], label);
        assert_eq!(observation["passed"], true, "{observation}");
    }
}
