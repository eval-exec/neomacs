#![cfg(unix)]
//! TTY column geometry stays independent of GUI font classification.
use neomacs_tui_tests::{
    TuiLaunch, TuiProcessOutcome, TuiSession, TuiTempDirectory, TuiTerminalConfig,
};
use std::{path::PathBuf, time::Duration};
fn probe(binary: PathBuf) -> serde_json::Value {
    let directory = TuiTempDirectory::new("dual-width-text-");
    let state = directory.path().join("state.json");
    let fixture = neomacs_infra::crate_root!()
        .join("../neomacs-gui-tests/fixtures/dual-width-fixed-pitch/init.el");
    let launch = TuiLaunch::new(binary.as_os_str())
        .arg("-nw")
        .arg("-Q")
        .arg("--load")
        .arg(fixture)
        .env("NEOMACS_GUI_STATE_JSON", &state)
        .env("HOME", directory.path());
    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        "CJK",
        TuiTerminalConfig::new("xterm-256color", 30, 100),
    );
    assert_eq!(
        session.run_to_completion(Duration::from_secs(25)),
        TuiProcessOutcome::Exited
    );
    serde_json::from_slice(&std::fs::read(state).unwrap()).unwrap()
}
#[test]
fn cjk_position_advances_match_gnu_in_a_real_terminal() {
    let gnu = probe(PathBuf::from("emacs"));
    let neomacs = probe(neomacs_tui_tests::neomacs_binary());
    assert_eq!(
        gnu["positions"],
        serde_json::json!([0, 1, 2, 4, 6, 8, 10, 11, 12])
    );
    assert_eq!(neomacs, gnu);
}
