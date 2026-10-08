#![cfg(unix)]
//! The GUI repair must preserve the TTY frame-local parameter contract.
use neomacs_tui_tests::{
    TuiLaunch, TuiProcessOutcome, TuiSession, TuiTempDirectory, TuiTerminalConfig,
};
use std::{path::PathBuf, time::Duration};

fn probe(name: &str, binary: PathBuf) -> serde_json::Value {
    let directory = TuiTempDirectory::new("frame-bar-parameters-");
    let output = directory.path().join("state.json");
    let fixture = neomacs_infra::crate_root!()
        .join("../neomacs-gui-tests/fixtures/frame-bar-parameters/init.el");
    let launch = TuiLaunch::new(binary.as_os_str())
        .arg("-nw")
        .arg("-Q")
        .arg("--eval")
        .arg("(modify-frame-parameters nil '((menu-bar-lines . 0) (tool-bar-lines . 0)))")
        .arg("--load")
        .arg(fixture)
        .env("NEOMACS_GUI_STATE_JSON", &output)
        .env("HOME", directory.path());
    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        name,
        TuiTerminalConfig::new("xterm-256color", 30, 100),
    );
    assert_eq!(
        session.run_to_completion(Duration::from_secs(25)),
        TuiProcessOutcome::Exited
    );
    serde_json::from_slice(&std::fs::read(output).expect("fixture produced a trace")).unwrap()
}

#[test]
fn frame_bar_parameters_and_root_geometry_match_gnu_on_tty() {
    let gnu = probe("GNU", PathBuf::from("emacs"));
    let neomacs = probe("Neomacs", neomacs_tui_tests::neomacs_binary());
    assert_eq!(neomacs, gnu);
    assert_eq!(neomacs[1]["menu"], 0);
    assert_eq!(neomacs[3]["menu"], 0);
    assert_eq!(neomacs[4]["menu"], 1);
    assert_eq!(neomacs[4]["mode"], false);
}
