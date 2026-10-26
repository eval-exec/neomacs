//! Compare actual initialized display iterators; cold batch uses another engine.
#![cfg(unix)]
use neomacs_tui_tests::{
    TuiLaunch, TuiProcessOutcome, TuiSession, TuiTempDirectory, TuiTerminalConfig,
};
use std::{path::PathBuf, time::Duration};

fn probe(binary: PathBuf, origin: &str) -> serde_json::Value {
    let directory = TuiTempDirectory::new("eob-overlay-motion-");
    let state = directory.path().join("state.json");
    let fixture = neomacs_infra::workspace_root()
        .join("crates/neomacs-gui-tests/fixtures/eob-overlay-motion.el");
    let launch = TuiLaunch::new(binary.as_os_str())
        .arg("-nw")
        .arg("-Q")
        .arg("--load")
        .arg(fixture)
        .env("HOME", directory.path())
        .env("TMPDIR", directory.path())
        .env("NEOMACS_GUI_STATE_JSON", &state)
        .env("NEOMACS_MOTION_PROBE_ORIGIN", origin);
    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        "EOB overlay motion",
        TuiTerminalConfig::new("xterm-256color", 30, 100),
    );
    assert_eq!(
        session.run_to_completion(Duration::from_secs(30)),
        TuiProcessOutcome::Exited
    );
    serde_json::from_slice(&std::fs::read(state).expect("motion fixture wrote results")).unwrap()
}

fn compare(origin: &str) {
    let gnu = std::env::var_os("NEOMACS_GNU_EMACS_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| "emacs".into());
    let expected = probe(gnu, origin);
    assert_eq!(expected["observations"].as_array().unwrap().len(), 16);
    assert!(
        expected["observations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| !v.as_str().unwrap().contains("(error ")),
        "GNU: {expected}"
    );
    assert_eq!(probe(neomacs_tui_tests::neomacs_binary(), origin), expected);
}

#[test]
fn motion_from_accessible_eob_with_virtual_overlay_rows_matches_gnu() {
    compare("eob");
}

#[test]
fn motion_into_eob_overlay_from_source_line_preserves_gnu_distance() {
    compare("bol");
}
