//! #672: Doom-style font-name resolution must return usable resize names.
//!
//! Linux Fontconfig/X11 coverage, with an isolated pinned Hack catalog and the
//! attested GNU 31.1 oracle. No user font installation or Doom checkout is used.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

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

fn run(engine: Engine) {
    let root = neomacs_infra::workspace_root();
    let (name, binary) = match engine {
        Engine::Neomacs => (
            "native",
            std::env::var_os("NEOMACS_GUI_TEST_BINARY")
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("target/release/neomacs")),
        ),
        Engine::Gnu => {
            let oracle = neomacs_test_oracle::gnu::gnu_oracle();
            assert!(
                oracle.is_attested(),
                "GNU control requires the pinned reference"
            );
            eprintln!("{}", oracle.stamp());
            ("gnu", oracle.executable_path())
        }
    };
    let artifacts = root.join(format!(
        "target/neomacs-gui-tests/font-pattern-resize-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
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
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .unwrap();
    let state = fs::read(&result.artifacts.gui_state)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    assert!(!result.timed_out, "{result:#?}; state={state:?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}; state={state:?}");
    let state = state.expect("fixture must publish its font-resolution observations");
    assert_eq!(state["passed"], true, "{state}");
    assert_eq!(state["sizes"], serde_json::json!([13, 21]));
    for field in ["case-insensitive", "maximum-zero", "missing-foundry"] {
        assert_eq!(state[field], true, "{field}: {state}");
    }
    assert_eq!(state["negative-condition"], "wrong-type-argument");
    assert!(state["large-height"].as_i64().unwrap() > state["small-height"].as_i64().unwrap());
    assert!(state["large-width"].as_i64().unwrap() > state["small-width"].as_i64().unwrap());
}
