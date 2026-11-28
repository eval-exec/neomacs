//! #672: Doom-style font-name resolution must return usable resize names.
//!
//! Linux Fontconfig/X11 coverage, with an isolated pinned Hack catalog and the
//! attested GNU 31.1 oracle. No user font installation or Doom checkout is used.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions, GuiRunStatus,
    GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use serde_json::Value;
use std::{fs, path::PathBuf, time::Duration};

#[derive(Clone, Copy)]
enum Engine {
    Neomacs,
    Gnu,
}

#[test]
fn native_hack_font_patterns_resolve_and_resize_frame() {
    run(Engine::Neomacs);
}

#[test]
fn gnu_hack_font_patterns_resolve_and_resize_frame() {
    run(Engine::Gnu);
}

#[test]
fn face_font_fallback_xlfd_resize_matches_gnu_on_x11() {
    let gnu = run_fallback_resize(Engine::Gnu);
    let neo = run_fallback_resize(Engine::Neomacs);
    assert_eq!(
        neo["startup-dumped-faces-present"], true,
        "dumped non-bootstrap faces must survive startup rehydration: {neo}"
    );
    assert_eq!(
        gnu["startup-dumped-faces-present"], true,
        "GNU control must have dumped non-bootstrap faces: {gnu}"
    );
    assert_eq!(
        neo["startup-face-count"], gnu["startup-face-count"],
        "stock -Q face-list count must match GNU before alist mutation: neo={neo} gnu={gnu}"
    );
    assert_eq!(
        neo["startup-completions-id"], gnu["startup-completions-id"],
        "stock -Q completions-common-part face-id must match GNU: neo={neo} gnu={gnu}"
    );
    assert_eq!(
        neo["startup-help-key-id"], gnu["startup-help-key-id"],
        "stock -Q help-key-binding face-id must match GNU: neo={neo} gnu={gnu}"
    );
    assert_eq!(
        neo["before-alist-face-count"], gnu["before-alist-face-count"],
        "face-list count after loading dynamic faces must match GNU: neo={neo} gnu={gnu}"
    );
    assert_eq!(
        neo["after-alist-face-count"], neo["before-alist-face-count"],
        "internal-set-alternative-font-family-alist must preserve dynamic face-list count: {neo}"
    );
    assert_eq!(
        neo["after-alist-face-count"], gnu["after-alist-face-count"],
        "face-list count after internal-set-alternative-font-family-alist must match GNU: neo={neo} gnu={gnu}"
    );
    assert_eq!(
        neo["before-alist-ansi-red-id"], gnu["before-alist-ansi-red-id"],
        "dynamic ansi-color-red face-id must match GNU: neo={neo} gnu={gnu}"
    );
    assert_eq!(
        neo["after-alist-ansi-red-id"], neo["before-alist-ansi-red-id"],
        "internal-set-alternative-font-family-alist must preserve dynamic ansi-color-red face-id: {neo}"
    );
    assert_eq!(
        neo["after-alist-ansi-red-listed"], true,
        "internal-set-alternative-font-family-alist must keep ansi-color-red in face-list: {neo}"
    );
    // In this isolated two-font Fontconfig catalog neither Hack nor DejaVu Sans
    // Mono is in stock `face-font-family-alternatives` for "Monospace Serif":
    // GNU's legacy X11 core xfont driver can still match bitmap `fixed`, while
    // Neomacs resolves via Fontconfig to Hack. Both must return a real XLFD
    // (never leaking "Monospace Serif") that resizes cleanly through
    // `x-resolve-font-name`.
    let startup_fps = neo["startup-fps"]
        .as_str()
        .expect("stock -Q fixed-pitch-serif face-font must be a string");
    assert!(
        !startup_fps.is_empty() && !startup_fps.contains("Monospace Serif"),
        "stock -Q fixed-pitch-serif must not leak unmapped family name: {neo}"
    );
    let startup_fps_resolved = neo["startup-fps-resolved"]
        .as_str()
        .expect("stock -Q resized fixed-pitch-serif XLFD must resolve");
    assert!(
        startup_fps_resolved.contains("-13-"),
        "stock -Q resized fixed-pitch-serif XLFD must reflect requested pixel size 13: {neo}"
    );
    assert_eq!(
        neo["faces"], gnu["faces"],
        "Neomacs face-font fallback resize diverged from GNU 31.1: neo={neo} gnu={gnu}"
    );
    assert_eq!(
        neo["direct-missing-resolve"], gnu["direct-missing-resolve"],
        "direct missing-family x-resolve-font-name must match GNU: neo={neo} gnu={gnu}"
    );
    assert_eq!(
        neo["direct-missing-set-frame"], gnu["direct-missing-set-frame"],
        "direct missing-family set-frame-font must match GNU: neo={neo} gnu={gnu}"
    );
    assert_eq!(
        neo["faces"]["fixed-pitch-serif"]["face-font"],
        "-PfEd-DejaVu Sans Mono-regular-normal-normal-*-14-*-*-*-m-0-iso10646-1",
        "fixed-pitch-serif must resolve through face-font-family-alternatives: {neo}"
    );
    assert_eq!(
        neo["faces"]["fixed-pitch-serif"]["resolved-13"],
        "-PfEd-DejaVu Sans Mono-regular-normal-normal-*-13-*-*-*-m-0-iso10646-1",
        "fixed-pitch-serif must resize to pixel size 13: {neo}"
    );
    assert_eq!(
        neo["faces"]["fixed-pitch-serif"]["resolved-21"],
        "-PfEd-DejaVu Sans Mono-regular-normal-normal-*-21-*-*-*-m-0-iso10646-1",
        "fixed-pitch-serif must resize to pixel size 21: {neo}"
    );
    assert_eq!(
        neo["faces"]["neo-672-missing-face"]["face-font"],
        "-SRC-Hack-regular-normal-normal-*-14-*-*-*-m-0-iso10646-1",
        "unmapped missing family must fall back via inherited SRC foundry to Hack: {neo}"
    );
}

fn run_fallback_resize(engine: Engine) -> Value {
    let root = neomacs_infra::workspace_root();
    let (name, binary) = match engine {
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
            ("native", binary)
        }
        Engine::Gnu => {
            let oracle = neomacs_test_oracle::gnu::gnu_oracle();
            assert!(
                oracle.is_attested(),
                "GNU control requires the pinned reference"
            );
            let binary = oracle.executable_path();
            let prefix = binary.parent().unwrap().parent().unwrap();
            let check = std::process::Command::new("bash")
                .arg(root.join("scripts/ci/check-gnu-oracle.sh"))
                .arg(&binary)
                .arg("31.1")
                .arg(prefix.join("share/gnu-emacs-oracle/build-manifest"))
                .arg("a360712c9d272d950d8d8255ef74570f7e90b7d9")
                .output()
                .expect("run prescribed GNU oracle check");
            assert!(
                check.status.success(),
                "GNU oracle check failed:\n{}\n{}",
                String::from_utf8_lossy(&check.stdout),
                String::from_utf8_lossy(&check.stderr)
            );
            ("gnu", binary)
        }
    };
    let artifacts = root.join(format!(
        "target/neomacs-gui-tests/face-font-fallback-resize-{name}-{}-{}",
        std::process::id(),
        neomacs_gui_tests::unique_nonce()
    ));
    let fonts = artifacts.join("fonts");
    fs::create_dir_all(&fonts).unwrap();
    fs::copy(
        neomacs_test_fonts::hack_regular_3_003(),
        fonts.join("Hack-Regular.ttf"),
    )
    .unwrap();
    fs::copy(
        neomacs_test_fonts::dejavu_2_37().mono_regular(),
        fonts.join("DejaVuSansMono.ttf"),
    )
    .unwrap();
    let config = artifacts.join("fonts.conf");
    fs::write(
        &config,
        format!(
            "<fontconfig><dir>{}</dir><cachedir>{}</cachedir><alias><family>monospace</family><prefer><family>Hack</family></prefer></alias><alias><family>sans-serif</family><prefer><family>Hack</family></prefer></alias></fontconfig>",
            fonts.display(),
            artifacts.join("fontcache").display()
        ),
    )
    .unwrap();
    let session = DisplayHarness::Xvfb.start_session(&artifacts).unwrap();
    let mut plan = GuiTestPlan::new(
        GuiBackend::LinuxX11,
        &root,
        &artifacts,
        GuiScenario::new(
            "face-font-fallback-resize",
            root.join("crates/neomacs-gui-tests/fixtures/face-font-fallback-resize.el"),
        ),
    )
    .with_program(binary)
    .with_env("FONTCONFIG_FILE", config.display().to_string())
    .with_env("GDK_BACKEND", "x11")
    .with_env("WINIT_UNIX_BACKEND", "x11");
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    let options = GuiRunOptions::with_timeout(Duration::from_secs(30));
    let (artifacts, exit_code, timed_out) = match engine {
        Engine::Neomacs => {
            let result = plan
                .run_with(&mut ProcessGuiCommandRunner, options)
                .unwrap();
            assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
            (result.artifacts, result.exit_code, result.timed_out)
        }
        Engine::Gnu => {
            let artifacts = GuiArtifactSet::new(
                &artifacts,
                GuiBackend::LinuxX11,
                "face-font-fallback-resize",
            );
            fs::create_dir_all(artifacts.json.parent().unwrap()).unwrap();
            let mut command = plan.command_spec();
            command
                .env
                .retain(|(key, _)| !key.starts_with("NEOMACS_") || key == "NEOMACS_GUI_STATE_JSON");
            let output = ProcessGuiCommandRunner
                .run(&command, &artifacts, &options)
                .unwrap();
            fs::write(&artifacts.stdout, &output.stdout).unwrap();
            fs::write(&artifacts.stderr, &output.stderr).unwrap();
            (artifacts, output.exit_code, output.timed_out)
        }
    };
    let state = fs::read(&artifacts.gui_state)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    assert!(!timed_out, "artifacts={artifacts:#?}; state={state:?}");
    assert_eq!(
        exit_code,
        Some(0),
        "artifacts={artifacts:#?}; state={state:?}"
    );
    let state = state.expect("fixture must publish its face-font observations");
    assert_eq!(state["passed"], true, "{state}");
    state
}

fn run(engine: Engine) {
    let root = neomacs_infra::workspace_root();
    let (name, binary) = match engine {
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
            eprintln!("{stamp:?}");
            ("native", binary)
        }
        Engine::Gnu => {
            let oracle = neomacs_test_oracle::gnu::gnu_oracle();
            assert!(
                oracle.is_attested(),
                "GNU control requires the pinned reference"
            );
            let binary = oracle.executable_path();
            let prefix = binary.parent().unwrap().parent().unwrap();
            let check = std::process::Command::new("bash")
                .arg(root.join("scripts/ci/check-gnu-oracle.sh"))
                .arg(&binary)
                .arg("31.1")
                .arg(prefix.join("share/gnu-emacs-oracle/build-manifest"))
                .arg("a360712c9d272d950d8d8255ef74570f7e90b7d9")
                .output()
                .expect("run prescribed GNU oracle check");
            assert!(
                check.status.success(),
                "GNU oracle check failed:\n{}\n{}",
                String::from_utf8_lossy(&check.stdout),
                String::from_utf8_lossy(&check.stderr)
            );
            eprintln!("{}", String::from_utf8_lossy(&check.stdout));
            eprintln!("{}", oracle.stamp());
            ("gnu", binary)
        }
    };
    let artifacts = root.join(format!(
        "target/neomacs-gui-tests/font-pattern-resize-{name}-{}-{}",
        std::process::id(),
        neomacs_gui_tests::unique_nonce()
    ));
    let fonts = artifacts.join("fonts");
    fs::create_dir_all(&fonts).unwrap();
    fs::copy(
        neomacs_test_fonts::hack_regular_3_003(),
        fonts.join("Hack-Regular.ttf"),
    )
    .unwrap();
    let config = artifacts.join("fonts.conf");
    fs::write(
        &config,
        format!(
            "<fontconfig><dir>{}</dir><cachedir>{}</cachedir><alias><family>monospace</family><prefer><family>Hack</family></prefer></alias><alias><family>sans-serif</family><prefer><family>Hack</family></prefer></alias></fontconfig>",
            fonts.display(), artifacts.join("fontcache").display()
        ),
    ).unwrap();
    let session = DisplayHarness::Xvfb.start_session(&artifacts).unwrap();
    let mut plan = GuiTestPlan::new(
        GuiBackend::LinuxX11,
        &root,
        &artifacts,
        GuiScenario::new(
            "font-name-pattern-resize",
            root.join("crates/neomacs-gui-tests/fixtures/font-name-pattern-resize.el"),
        ),
    )
    .with_program(binary)
    .with_env("FONTCONFIG_FILE", config.display().to_string())
    .with_env("GDK_BACKEND", "x11")
    .with_env("WINIT_UNIX_BACKEND", "x11");
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    let options = GuiRunOptions::with_timeout(Duration::from_secs(30));
    let (artifacts, exit_code, timed_out) = match engine {
        Engine::Neomacs => {
            let result = plan
                .run_with(&mut ProcessGuiCommandRunner, options)
                .unwrap();
            assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
            (result.artifacts, result.exit_code, result.timed_out)
        }
        Engine::Gnu => {
            // GNU does not implement Neomacs GPU readback. Use the shared
            // process/deadline runner with only the artifacts GNU produces.
            let artifacts =
                GuiArtifactSet::new(&artifacts, GuiBackend::LinuxX11, "font-name-pattern-resize");
            fs::create_dir_all(artifacts.json.parent().unwrap()).unwrap();
            let mut command = plan.command_spec();
            command
                .env
                .retain(|(key, _)| !key.starts_with("NEOMACS_") || key == "NEOMACS_GUI_STATE_JSON");
            let output = ProcessGuiCommandRunner
                .run(&command, &artifacts, &options)
                .unwrap();
            fs::write(&artifacts.stdout, &output.stdout).unwrap();
            fs::write(&artifacts.stderr, &output.stderr).unwrap();
            let state = fs::read(&artifacts.gui_state)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
            let status = if output.timed_out {
                GuiRunStatus::TimedOut
            } else if output.exit_code == Some(0)
                && state.as_ref().is_some_and(|state| state["passed"] == true)
            {
                GuiRunStatus::Passed
            } else {
                GuiRunStatus::Failed
            };
            let manifest = serde_json::json!({
                "engine": "gnu-emacs",
                "backend": "linux-x11",
                "scenario": "font-name-pattern-resize",
                "status": status.slug(),
                "command": {
                    "program": command.program,
                    "args": command.args,
                    "env": command.env.into_iter().collect::<std::collections::BTreeMap<_, _>>(),
                },
                "process": {"exit_code": output.exit_code, "timed_out": output.timed_out},
                "expected_artifacts": {
                    "gui_state": artifacts.gui_state,
                    "stdout": artifacts.stdout,
                    "stderr": artifacts.stderr,
                },
                "gui_state": state,
            });
            fs::write(&artifacts.json, format!("{manifest}\n")).unwrap();
            assert_eq!(status, GuiRunStatus::Passed, "{manifest}");
            (artifacts, output.exit_code, output.timed_out)
        }
    };
    let state = fs::read(&artifacts.gui_state)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    assert!(!timed_out, "artifacts={artifacts:#?}; state={state:?}");
    assert_eq!(
        exit_code,
        Some(0),
        "artifacts={artifacts:#?}; state={state:?}"
    );
    let manifest: Value = serde_json::from_slice(&fs::read(&artifacts.json).unwrap()).unwrap();
    assert_eq!(manifest["status"], "passed", "{manifest}");
    let state = state.expect("fixture must publish its font-resolution observations");
    assert_eq!(state["passed"], true, "{state}");
    assert_eq!(state["sizes"], serde_json::json!([13, 21, 13]));
    for field in ["case-insensitive", "maximum-zero", "missing-foundry"] {
        assert_eq!(state[field], true, "{field}: {state}");
    }
    assert_eq!(state["negative-condition"], "wrong-type-argument");
    assert!(state["large-height"].as_i64().unwrap() > state["small-height"].as_i64().unwrap());
    assert!(state["large-width"].as_i64().unwrap() > state["small-width"].as_i64().unwrap());
    assert_eq!(state["restored-height"], state["small-height"], "{state}");
    assert_eq!(state["restored-width"], state["small-width"], "{state}");
}
