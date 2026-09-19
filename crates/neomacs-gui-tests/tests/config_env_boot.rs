//! Config-environment GUI comparisons: Doom and Spacemacs boot in real
//! GUI sessions (weston-headless), each editor mounting its sealed
//! fixture through the [`ConfigEnvironment`] contract, and the visible
//! home-buffer text plus frame geometry must agree.
//!
//! Isolation contract (the reason this test exists in this shape): the
//! editor subprocesses get HOME and every XDG directory pinned inside
//! their per-session state directories — never the operator's — and the
//! display session owns its XDG_RUNTIME_DIR/Wayland socket, locale, and
//! Xauthority.  Nothing a config framework writes can reach a real home.
//!
//! Prerequisites: materialized fixtures
//! (`cargo run -p xtask -- infra materialize doom` /
//!  `... spacemacs`), release binary, Weston.  Skips where absent.

#![cfg(target_os = "linux")]

use neomacs_gui_tests::DisplayHarness;
use neomacs_gui_tests::{
    GuiBackend, GuiRunOptions, GuiRunResult, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use neomacs_infra::config_env::ConfigEnvironment as _;
use neomacs_infra::display::WaylandOutput;
use neomacs_infra::workspace_root;
use std::path::PathBuf;
use std::time::Duration;

fn run_config_boot(
    env: &dyn neomacs_infra::config_env::ConfigEnvironment,
    session_state: &std::path::Path,
    program: PathBuf,
    needle: &str,
) -> GuiRunResult {
    let root = workspace_root();
    let artifacts = root
        .join("target/neomacs-gui-tests")
        .join(format!("config-boot-{}", env.name()));
    std::fs::create_dir_all(&artifacts).unwrap();
    // Xvfb, not weston: the GTK3 GNU oracle is an X11 build (no PGTK), so
    // it cannot mount a Wayland-only session at all -- GNU GUI under
    // weston has never actually run (part of the open GUI CI cluster).
    // Neomacs speaks X11 through the same winit backend the resize
    // scenarios use.
    let backend = GuiBackend::LinuxX11;
    let session = DisplayHarness::Xvfb
        .start_session(&artifacts)
        .expect("start isolated Xvfb");

    let scenario_script = root.join("crates/neomacs-gui-tests/fixtures/config-env-boot.el");
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(format!("{}-gui-boot", env.name()), &scenario_script),
    )
    .with_program(program)
    .with_env("NEOMACS_GUI_CONFIG_HOME_NEEDLE", needle)
    .with_env("RUST_LOG", "warn")
    .with_env("WINIT_UNIX_BACKEND", "x11");
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

    let gnu = run_config_boot(env, &gnu_state, PathBuf::from("emacs"), needle);
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

    let neo = run_config_boot(env, &neo_state, neo_program, needle);
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
// GNU-side only: during the harness run GNU spacemacs never opens its X
// connection (no socket fd; gmain/gdbus idle; the drawn screen observed
// on the shared Xvfb is the Neomacs side).  The doom sibling passes on
// the identical env, so this is spacemacs-specific pre-display blocking,
// not the harness.  TUI spacemacs parity is green.
#[test]
#[ignore = "GNU spacemacs GUI blocks before opening X: live investigation"]
fn spacemacs_gui_boot_home_buffer_matches_gnu() {
    match neomacs_infra::SpacemacsEnvironment::open() {
        Some(env) => compare_config_boot(&env, "Find File"),
        None => eprintln!(
            "skipping: no sealed Spacemacs fixture; run \
             `cargo run -p xtask -- infra materialize spacemacs`"
        ),
    }
}
