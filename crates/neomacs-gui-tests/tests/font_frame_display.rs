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
    process::Command,
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

fn prepare_listing_review_fonts(fonts: &Path) -> String {
    let hack = neomacs_test_fonts::hack_regular_3_003();
    let dejavu = neomacs_test_fonts::dejavu_2_37().sans_regular();
    fs::write(
        fonts.join("review-layout.woff"),
        neomacs_test_fonts::uncompressed_woff(&fs::read(dejavu).unwrap()),
    )
    .unwrap();
    let bitmap = fs::read_to_string(neomacs_test_fonts::spleen_2_2_0().bdf()).unwrap();
    assert!(bitmap.contains("FAMILY_NAME \"Spleen\""));
    fs::write(
        fonts.join("review-bitmap.bdf"),
        bitmap
            .replace("FAMILY_NAME \"Spleen\"", "FAMILY_NAME \"FrameBitmap\"")
            .replace("FONT -misc-spleen-", "FONT -misc-framebitmap-"),
    )
    .unwrap();
    let mut rules = "<match target=\"scan\"><test name=\"file\" compare=\"contains\"><string>review-layout.woff</string></test><edit name=\"family\" mode=\"assign_replace\"><string>FrameLayout</string></edit></match>".to_owned();
    for (scenario, family) in [
        ("slant", "FrameSlant"),
        ("weight", "FrameWeight"),
        ("width", "FrameWidth"),
    ] {
        for (name, source) in [("hack", hack), ("dejavu", dejavu)] {
            let filename = format!("review-{scenario}-{name}.ttf");
            fs::copy(source, fonts.join(&filename)).unwrap();
            let slant = match (scenario, name) {
                ("slant", "hack") => 100,
                ("slant", _) => 110,
                _ => 0,
            };
            let weight = if scenario == "weight" && name == "hack" {
                "<edit name=\"weight\" mode=\"delete_all\"/>".to_owned()
            } else {
                format!(
                    "<edit name=\"weight\" mode=\"assign_replace\"><int>{}</int></edit>",
                    if scenario == "weight" { 180 } else { 80 }
                )
            };
            let width = if scenario == "width" && name == "hack" {
                "<edit name=\"width\" mode=\"delete_all\"/>".to_owned()
            } else {
                format!(
                    "<edit name=\"width\" mode=\"assign_replace\"><int>{}</int></edit>",
                    if scenario == "width" { 113 } else { 100 }
                )
            };
            rules.push_str(&format!("<match target=\"scan\"><test name=\"file\" compare=\"contains\"><string>{filename}</string></test><edit name=\"family\" mode=\"assign_replace\"><string>{family}</string></edit><edit name=\"slant\" mode=\"assign_replace\"><int>{slant}</int></edit>{weight}{width}</match>"));
        }
    }
    rules
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
            let binary = oracle.executable_path();
            let prefix = binary
                .parent()
                .and_then(Path::parent)
                .expect("canonical GNU executable must be installed in PREFIX/bin");
            let check = Command::new("bash")
                .arg(root.join("scripts/ci/check-gnu-oracle.sh"))
                .arg(&binary)
                .arg("31.1")
                .arg(prefix.join("share/gnu-emacs-oracle/build-manifest"))
                .arg("a360712c9d272d950d8d8255ef74570f7e90b7d9")
                .output()
                .expect("run canonical GNU build/profile check");
            let profile = String::from_utf8_lossy(&check.stdout);
            assert!(
                check.status.success(),
                "GNU build/profile check failed: {profile}{}",
                String::from_utf8_lossy(&check.stderr)
            );
            ("gnu", binary, format!("{}\n{profile}", oracle.stamp()))
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
    fs::copy(
        neomacs_test_fonts::spleen_2_2_0().otb(),
        fonts.join("spleen-8x16.otb"),
    )
    .unwrap();
    let scan_rules = prepare_listing_review_fonts(&fonts);
    let config = artifact_root.join("fonts.conf");
    fs::write(
        &config,
        format!(
            "<fontconfig><dir>{}</dir><cachedir>{}</cachedir>{scan_rules}<alias><family>monospace</family><prefer><family>Hack</family></prefer></alias><alias><family>sans-serif</family><prefer><family>Hack</family></prefer></alias></fontconfig>",
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
    assert_eq!(state["scalable-cases"], 7);
    assert_eq!(state["face-remapping-cases"], 4);
    assert_eq!(state["font-property-cases"], 4);
    assert_eq!(state["listing-review-cases"], 7);
    assert_eq!(state["registry-family-cases"], 13);
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
        "scalable-explicit-nil",
        "scalable-true",
        "scalable-absent",
        "scalable-name-false",
        "scalable-name-true",
        "scalable-name-mixed-case",
        "scalable-nil-average-width-zero",
        "face-remapping-default-height",
        "face-remapping-nil-face",
        "face-remapping-invalid-face",
        "face-remapping-restored",
        "otf-unsupported-hack-script",
        "font-extra-existing-slots-shared",
        "font-native-key-updates-primary-slot",
        "font-point-size-zero-dpi",
        "otf-woff-positive-and-missing-script",
        "bitmap-native-average-width",
        "bitmap-average-width-admission",
        "prefer-numeric-italic-oblique",
        "prefer-missing-weight-unscored",
        "prefer-missing-width-unscored",
        "xlfd-positioned-family",
        "family-omitted-unrestricted",
        "family-empty-restricted",
        "family-whitespace-restricted",
        "find-font-empty-family",
        "registry-substring",
        "registry-regexp-bracket",
        "registry-regexp-alternation",
        "registry-regexp-casefold",
        "registry-whitespace-not-trimmed",
        "registry-invalid-error-order",
        "registry-invalid-preference-ignored",
        "find-font-otf-uses-listing",
        "find-font-registry-error",
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
