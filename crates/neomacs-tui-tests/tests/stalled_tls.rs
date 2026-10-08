#![cfg(unix)]
use neomacs_tui_tests::{
    TuiLaunch, TuiProcessOutcome, TuiSession, TuiTempDirectory, TuiTerminalConfig,
};
use std::time::Duration;

#[test]
fn stalled_tls_keeps_timers_and_terminal_redisplay_alive() {
    let directory = TuiTempDirectory::new("stalled-tls-");
    let output = directory.path().join("state.json");
    let fixture = neomacs_infra::crate_root!().join("../neomacs-gui-tests/fixtures/stalled-tls.el");
    let launch = TuiLaunch::new(neomacs_tui_tests::neomacs_binary().as_os_str())
        .arg("-nw")
        .arg("-Q")
        .arg("--load")
        .arg(fixture)
        .env("NEOMACS_GUI_STATE_JSON", &output);
    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        "stalled TLS",
        TuiTerminalConfig::new("xterm-256color", 30, 100),
    );
    session.read_until(Duration::from_secs(12), |grid| {
        grid.iter().any(|row| row.contains("TLS-TIMEOUT-CLOSED"))
    });
    // Capture the alternate screen before normal editor exit restores the
    // terminal's startup screen.
    let painted = session.text_grid().join("\n");
    assert!(painted.contains("TIMER-DURING-TLS"), "{painted}");
    assert!(painted.contains("TLS-TIMEOUT-CLOSED"), "{painted}");
    assert_eq!(
        session.run_to_completion(Duration::from_secs(12)),
        TuiProcessOutcome::Exited
    );
    let state: serde_json::Value = serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
    assert_eq!(
        state,
        serde_json::json!({"passed": true, "timer": true, "outcome": "timeout", "closed": true})
    );
}
