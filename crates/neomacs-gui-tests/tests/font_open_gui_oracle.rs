//! GNU `open-font' contract, checked against the attested oracle.
//!
//! Both engines open the same pinned DejaVu Sans Mono entity on the same
//! Xvfb display at nil/0/20/12.0 sizes. The fixture publishes the observable
//! contract (opened pixel sizes, object properties and metrics, per-entity
//! same-size identity, and error symbols/predicates); this test requires the
//! native and GNU states to agree key by key.
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
fn gnu_and_native_agree_on_open_font() {
    let native = run(Engine::Neomacs);
    let gnu = run(Engine::Gnu);
    assert_eq!(gnu["passed"], true, "GNU contract failed: {gnu}");
    assert_eq!(native["passed"], true, "Neomacs contract failed: {native}");
    let keys = [
        "entity-family",
        "entity-weight",
        "open-nil-type",
        "open-nil-size",
        "open-nil-get-size",
        "open-20-size",
        "open-20-max-width",
        "open-20-ascent",
        "open-20-descent",
        "open-20-space-width",
        "open-20-average-width",
        "open-20-family",
        "open-20-weight",
        "open-20-get-size",
        "open-20-info-pixel",
        "open-20-info-space",
        "open-20-xlfd",
        "open-20-full-name",
        "open-20-get-name",
        "open-20-get-file",
        "open-20-info-full",
        "open-0-size",
        "open-0-get-size",
        "open-float-size",
        "open-float-get-size",
        "open-float0-size",
        "open-float0-get-size",
        "open-float-tiny-size",
        "open-float-tiny-get-size",
        "open-float-neg5-size",
        "open-float-neg5-get-size",
        "open-neg5-size",
        "open-neg5-get-size",
        "open-neg16-nil",
        "eq-same-size",
        "close-returns-nil",
        "err-size",
        "err-spec",
        "err-frame",
    ];
    for key in keys {
        assert_eq!(
            native[key], gnu[key],
            "open-font key {key:?} diverges: native={native:#} gnu={gnu:#}"
        );
    }
}

fn run(engine: Engine) -> Value {
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
        "target/neomacs-gui-tests/font-open-{name}-{}-{}",
        std::process::id(),
        neomacs_gui_tests::unique_nonce()
    ));
    let fonts = artifacts.join("fonts");
    fs::create_dir_all(&fonts).unwrap();
    let desktop = neomacs_test_fonts::desktop_fonts_cached().directory();
    for face in ["DejaVuSans.ttf", "DejaVuSansMono.ttf"] {
        fs::copy(desktop.join(face), fonts.join(face)).unwrap();
    }
    let cache = artifacts.join("fontconfig-cache");
    fs::create_dir_all(&cache).unwrap();
    let config = artifacts.join("fonts.conf");
    fs::write(
        &config,
        format!(
            "<fontconfig><dir>{}</dir><cachedir>{}</cachedir></fontconfig>",
            fonts.display(),
            cache.display()
        ),
    )
    .unwrap();
    let session = DisplayHarness::Xvfb.start_session(&artifacts).unwrap();
    let mut plan = GuiTestPlan::new(
        GuiBackend::LinuxX11,
        &root,
        &artifacts,
        GuiScenario::new(
            "font-open",
            root.join("crates/neomacs-gui-tests/fixtures/font-open.el"),
        ),
    )
    .with_program(binary)
    .with_env("FONTCONFIG_FILE", config.display().to_string())
    .with_env("GDK_BACKEND", "x11")
    .with_env("WINIT_UNIX_BACKEND", "x11");
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    let options = GuiRunOptions::with_timeout(Duration::from_secs(60));
    let state = match engine {
        Engine::Neomacs => {
            let result = plan
                .run_with(&mut ProcessGuiCommandRunner, options)
                .unwrap();
            assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
            assert_eq!(result.exit_code, Some(0), "{result:#?}");
            assert!(!result.timed_out, "{result:#?}");
            read_state(&result.artifacts.gui_state)
        }
        Engine::Gnu => {
            let artifacts = GuiArtifactSet::new(&artifacts, GuiBackend::LinuxX11, "font-open");
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
            assert!(!output.timed_out, "GNU open-font probe timed out");
            assert_eq!(
                output.exit_code,
                Some(0),
                "GNU open-font probe failed:\n{}\n{}",
                output.stdout,
                output.stderr
            );
            read_state(&artifacts.gui_state)
        }
    };
    state
}

fn read_state(path: &std::path::Path) -> Value {
    let state: Value = serde_json::from_slice(
        &fs::read(path).unwrap_or_else(|error| panic!("missing {}: {error}", path.display())),
    )
    .unwrap();
    state
}
