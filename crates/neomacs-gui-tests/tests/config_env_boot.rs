//! Config-environment GUI boots: Doom and Spacemacs mount their sealed
//! fixture through the [`ConfigEnvironment`] contract and boot in a real
//! GUI session.
//!
//! The host-backend tests assert that Neomacs boots the config and renders
//! a frame; they run on every scenario-tier backend, including a locked
//! macOS console (the fixture's success condition is config-visible text,
//! not a presented pixel). The `*_matches_gnu` tests additionally compare
//! the visible home-buffer text and frame geometry against GNU in an X11
//! session and therefore stay Linux-only until a macOS GUI GNU oracle
//! profile exists.
//!
//! Isolation contract (the reason this test exists in this shape): the
//! editor subprocesses get HOME and every XDG directory pinned inside
//! their per-session state directories — never the operator's — and the
//! display session owns its runtime dirs, locale, and X authority.
//! Nothing a config framework writes can reach a real home.
//!
//! Prerequisites: materialized fixtures
//! (`cargo run -p xtask -- infra materialize doom` /
//!  `... spacemacs`) and a release binary.  Skips where absent.

use neomacs_gui_tests::DisplayHarness;
use neomacs_gui_tests::{
    Capability, GuiBackend, GuiRunOptions, GuiRunResult, GuiRunStatus, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use neomacs_infra::workspace_root;
use std::path::PathBuf;
use std::time::Duration;

fn run_config_boot(
    env: &dyn neomacs_infra::config_env::ConfigEnvironment,
    session_state: &std::path::Path,
    program: PathBuf,
    needle: &str,
    backend: GuiBackend,
    variant: &str,
) -> GuiRunResult {
    let root = workspace_root();
    let artifacts = root.join("target/neomacs-gui-tests").join(format!(
        "config-boot-{}-{}",
        env.name(),
        backend.slug()
    ));
    std::fs::create_dir_all(&artifacts).unwrap();
    // Xvfb, not weston, on Linux: the GTK3 GNU oracle is an X11 build (no
    // PGTK), so it cannot mount a Wayland-only session at all. Neomacs
    // speaks X11 through the same winit backend; other backends use their
    // own session (the logged-in desktop on macOS).
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .expect("start the test display");

    let scenario_script = root.join("crates/neomacs-gui-tests/fixtures/config-env-boot.el");
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(
            format!("{}-{}-gui-boot", env.name(), variant),
            &scenario_script,
        ),
    )
    .with_program(program)
    .with_env("NEOMACS_GUI_CONFIG_HOME_NEEDLE", needle)
    .with_env("RUST_LOG", "warn")
    .require(Capability::Display)
    .require(Capability::ScenarioDriver);
    // No -Q: the config framework must load from the pinned HOME / mount
    // args.  The harness's default args assume -Q, so set ours explicitly,
    // mount args first so the fixture probes a fully-booted config.
    let mut args = env.session_args();
    args.extend([
        "-l".to_owned(),
        scenario_script.to_string_lossy().into_owned(),
    ]);
    plan = plan.with_args(args);
    for (key, value) in env.session_env(session_state) {
        plan = plan.with_env(
            key.to_string_lossy().into_owned(),
            value.to_string_lossy().into_owned(),
        );
    }
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    plan.run_with(
        &mut ProcessGuiCommandRunner,
        GuiRunOptions::with_timeout(Duration::from_secs(300)),
    )
    .unwrap()
}

#[cfg(target_os = "linux")]
fn compare_config_boot(env: &dyn neomacs_infra::config_env::ConfigEnvironment, needle: &str) {
    let root = workspace_root();
    let gnu_state = root
        .join("target/neomacs-gui-tests")
        .join(format!("config-boot-{}-gnu-state", env.name()));
    let neo_state = root
        .join("target/neomacs-gui-tests")
        .join(format!("config-boot-{}-neo-state", env.name()));
    for state in [&gnu_state, &neo_state] {
        let _ = std::fs::remove_dir_all(state);
        std::fs::create_dir_all(state).unwrap();
        env.prepare_session_state(state)
            .expect("seed session state");
    }
    let neo_program = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));

    let gnu_program = std::env::var_os("NEOMACS_GUI_TEST_GNU_EMACS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("emacs"));
    let gnu = run_config_boot(
        env,
        &gnu_state,
        gnu_program,
        needle,
        GuiBackend::LinuxX11,
        "gnu",
    );
    assert!(
        !gnu.timed_out,
        "GNU {} boot timed out: {gnu:#?}",
        env.name()
    );
    assert_eq!(
        gnu.exit_code,
        Some(0),
        "GNU {} boot failed: {gnu:#?}",
        env.name()
    );

    let neo = run_config_boot(
        env,
        &neo_state,
        neo_program,
        needle,
        GuiBackend::LinuxX11,
        "neo",
    );
    assert!(
        !neo.timed_out,
        "Neomacs {} boot timed out: {neo:#?}",
        env.name()
    );
    assert_eq!(
        neo.exit_code,
        Some(0),
        "Neomacs {} boot failed: {neo:#?}",
        env.name()
    );

    let read = |result: &GuiRunResult| {
        std::fs::read_to_string(&result.artifacts.gui_state)
            .unwrap_or_else(|error| panic!("read {} gui-state: {error}", env.name()))
    };
    let gnu_state = read(&gnu);
    let neo_state = read(&neo);
    assert_eq!(
        gnu_state,
        neo_state,
        "{} GUI boot state diverged (see target/neomacs-gui-tests/config-boot-{}/)",
        env.name(),
        env.name()
    );
}

/// Boot one config on the host's backend and require a rendered frame.
///
/// The fixture exits 0 only after the config's finish line is visible in a
/// window or in `*Messages*`, so exit status is the boot contract. The frame
/// snapshot is the display oracle: a zero-frame or zero-row snapshot means
/// the config loaded but redisplay produced nothing.
fn assert_config_boot_renders(
    env: &dyn neomacs_infra::config_env::ConfigEnvironment,
    needle: &str,
) {
    let Some(backend) = neomacs_gui_tests::requested_backend() else {
        eprintln!("no GUI backend configured for {}", env.name());
        return;
    };
    let root = workspace_root();
    let state = root.join("target/neomacs-gui-tests").join(format!(
        "config-boot-{}-{}-neo-state",
        env.name(),
        backend.slug()
    ));
    let _ = std::fs::remove_dir_all(&state);
    std::fs::create_dir_all(&state).unwrap();
    env.prepare_session_state(&state)
        .expect("seed session state");
    let program = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let result = run_config_boot(env, &state, program, needle, backend, "neo");
    if result.status == GuiRunStatus::Unsupported {
        eprintln!(
            "skipping {} GUI boot on {}: {}",
            env.name(),
            backend.slug(),
            result
                .failure_reason
                .as_deref()
                .unwrap_or("host lacks a required capability")
        );
        return;
    }
    assert!(
        !result.timed_out,
        "{} GUI boot timed out on {}: {result:#?}",
        env.name(),
        backend.slug()
    );
    assert_eq!(
        result.exit_code,
        Some(0),
        "{} GUI boot failed on {}: {result:#?}",
        env.name(),
        backend.slug()
    );
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    let snapshot = std::fs::read_to_string(&result.artifacts.frame_snapshot_json)
        .unwrap_or_else(|error| panic!("read {} frame snapshot: {error}", env.name()));
    let doc: serde_json::Value =
        serde_json::from_str(&snapshot).expect("frame snapshot JSON parses");
    let frames = doc["frames"].as_array().expect("frames array");
    assert!(
        !frames.is_empty(),
        "{} boot produced no rendered frame: {doc:.800}",
        env.name()
    );
    let rows = frames[0]["window_infos"]
        .as_array()
        .map(|windows| windows.len())
        .unwrap_or(0);
    assert!(
        rows > 0,
        "{} boot rendered a frame with no windows: {doc:.800}",
        env.name()
    );
}

#[test]
fn doom_gui_boot_renders_on_the_host_backend() {
    match neomacs_infra::DoomEnvironment::open() {
        Some(env) => assert_config_boot_renders(&env, "Doom loaded"),
        None => eprintln!(
            "skipping: no sealed Doom fixture; run \
             `cargo run -p xtask -- infra materialize doom`"
        ),
    }
}

#[test]
fn spacemacs_gui_boot_renders_on_the_host_backend() {
    match neomacs_infra::SpacemacsEnvironment::open() {
        Some(env) => assert_config_boot_renders(&env, "Homepage"),
        None => eprintln!(
            "skipping: no sealed Spacemacs fixture; run \
             `cargo run -p xtask -- infra materialize spacemacs`"
        ),
    }
}

#[cfg(target_os = "linux")]
#[test]
fn doom_gui_boot_home_buffer_matches_gnu() {
    match neomacs_infra::DoomEnvironment::open() {
        Some(env) => compare_config_boot(&env, "Doom loaded"),
        None => eprintln!(
            "skipping: no sealed Doom fixture; run \
             `cargo run -p xtask -- infra materialize doom`"
        ),
    }
}

// Same probe-design blocker, one step earlier: GNU's spacemacs GUI init
// under Xvfb had not completed within 300s in the last harness run
// (machine load ~35 during the chase; TUI boots in seconds).  Re-run
// on a quiet machine before concluding anything about spacemacs GUI.
#[cfg(target_os = "linux")]
#[test]
fn spacemacs_gui_boot_home_buffer_matches_gnu() {
    match neomacs_infra::SpacemacsEnvironment::open() {
        Some(env) => compare_config_boot(&env, "Homepage"),
        None => eprintln!(
            "skipping: no sealed Spacemacs fixture; run \
             `cargo run -p xtask -- infra materialize spacemacs`"
        ),
    }
}
