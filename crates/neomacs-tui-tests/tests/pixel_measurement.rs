//! Pixel APIs preserve terminal cells even when face fonts differ.
#![cfg(unix)]
use neomacs_tui_tests::{
    TuiLaunch, TuiProcessOutcome, TuiSession, TuiTempDirectory, TuiTerminalConfig,
};
use std::{path::PathBuf, time::Duration};

fn probe(binary: PathBuf) -> serde_json::Value {
    let directory = TuiTempDirectory::new("pixel-measurement-");
    let state = directory.path().join("state.json");
    let fixture = neomacs_infra::workspace_root()
        .join("crates/neomacs-gui-tests/fixtures/pixel-measurement/init.el");
    let launch = TuiLaunch::new(binary.as_os_str())
        .arg("-nw")
        .arg("-Q")
        .arg("--load")
        .arg(fixture)
        .env("NEOMACS_GUI_STATE_JSON", &state)
        .env("HOME", directory.path());
    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        "pixel measurement",
        TuiTerminalConfig::new("xterm-256color", 30, 100),
    );
    assert_eq!(
        session.run_to_completion(Duration::from_secs(30)),
        TuiProcessOutcome::Exited
    );
    serde_json::from_slice(&std::fs::read(state).unwrap()).unwrap()
}
#[test]
fn pixel_measurements_match_gnu_terminal_glyph_positions() {
    let gnu_binary = std::env::var_os("NEOMACS_GNU_EMACS_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("emacs"));
    let mut gnu = probe(gnu_binary);
    assert_eq!(
        gnu["native-engine"][0], false,
        "GNU probe must run GNU Emacs"
    );
    gnu.as_object_mut().unwrap().remove("native-engine");
    assert_eq!(
        gnu,
        serde_json::json!({"variable-pitch":[10,10,10], "height":[3,3,3], "pixel-space":[50,50,50], "emoji":[6,6,6], "long-string":[100], "window-preserved":[true]})
    );
    let mut native = probe(neomacs_tui_tests::neomacs_binary());
    assert_eq!(native["native-engine"][0], true);
    native.as_object_mut().unwrap().remove("native-engine");
    assert_eq!(native, gnu);
}
