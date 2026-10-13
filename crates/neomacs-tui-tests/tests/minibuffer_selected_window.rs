#![cfg(unix)]
//! Issue #560: completion survives deleting its invoking window.
use neomacs_tui_tests::{TuiLaunch, TuiSession, TuiTempDirectory};
use std::time::Duration;

#[test]
fn completion_renders_and_accepts_after_the_invoking_window_is_deleted() {
    let root = neomacs_infra::workspace_root();
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/minibuffer-selected-window.el");
    for (label, program) in [
        ("GNU", "emacs".into()),
        ("Neomacs", neomacs_tui_tests::neomacs_binary()),
    ] {
        let directory = TuiTempDirectory::new("issue560-completion-");
        let ready = directory.path().join("ready");
        let launch = TuiLaunch::new(program.as_os_str())
            .args(["-nw", "-Q", "-no-comp-spawn", "-l"])
            .arg(fixture.as_os_str())
            .env("NEOMACS_TEST_COMPLETION_READY", ready.as_os_str());
        let mut session = TuiSession::spawn_launch(launch, label);
        session.read_until(Duration::from_secs(30), |grid| {
            ready.exists()
                && grid.iter().any(|row| row.contains("Choose: "))
                && grid
                    .iter()
                    .any(|row| row.contains("Alpha") && row.contains("checked"))
                && grid
                    .iter()
                    .any(|row| row.contains("Beta") && row.contains("checked"))
        });
        let grid = session.text_grid();
        assert!(
            ready.exists(),
            "{label}: deletion did not complete: {}",
            grid.join("\n")
        );
        for candidate in ["Alpha", "Beta"] {
            assert!(
                grid.iter()
                    .any(|row| row.contains(candidate) && row.contains("checked")),
                "{label}: missing annotated candidate {candidate}: {}",
                grid.join("\n")
            );
        }
        session.send(b"Beta\r");
        session.read_until(Duration::from_secs(8), |grid| {
            grid.iter().any(|row| row.starts_with("Selected Beta"))
        });
        assert!(
            session
                .text_grid()
                .iter()
                .any(|row| row.starts_with("Selected Beta")),
            "{label}: could not accept completion: {}",
            session.text_grid().join("\n")
        );
    }
}
