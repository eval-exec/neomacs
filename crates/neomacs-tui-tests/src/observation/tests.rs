use std::time::{Duration, Instant};

use super::{TuiObservationError, TuiSession};
use crate::{ObservedTransportState, TuiLaunch};

#[test]
fn fenced_terminal_observes_the_shell_screen_through_the_existing_projection() {
    let launch = TuiLaunch::new("sh").args([
        "-c",
        r"printf '\033[31mQuery replace alpha with: omega\033[0m'; IFS= read -r line",
    ]);
    let mut session = TuiSession::spawn_launch(launch, "FENCED-SHELL");
    // Waiting is driven by real PTY observations. The fake owner's canonical
    // controls separately prove queued-before-request ordering deterministically.
    session.read_until(Duration::from_secs(2), |grid| {
        grid.iter()
            .any(|row| row.contains("Query replace alpha with: omega"))
    });
    {
        let fenced = session
            .observe_at_fence()
            .expect("live ordered observation");
        assert!(
            fenced
                .text_grid()
                .iter()
                .any(|row| row.contains("Query replace alpha with: omega"))
        );
        assert!(fenced.screen().contents().contains("with: omega"));
        assert!(format!("{fenced:?}").contains("FENCED-SHELL"));
    }
    session
        .finish()
        .expect("finish shell and ordered transport");
}

#[test]
fn a_fenced_interactive_view_is_refused_after_output_eof() {
    let launch = TuiLaunch::new("sh").args(["-c", "printf final-output"]);
    let mut session = TuiSession::spawn_launch(launch, "FENCED-EOF");
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match session.terminal.transport_state {
            ObservedTransportState::OutputClosed => break,
            ObservedTransportState::OwnerClosed => panic!("owner closed before explicit finish"),
            ObservedTransportState::Open => {}
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "shell output EOF was not observed");
        session.read(remaining.min(Duration::from_millis(50)));
    }
    assert!(
        session
            .text_grid()
            .iter()
            .any(|row| row.contains("final-output"))
    );
    assert!(matches!(
        session.observe_at_fence(),
        Err(TuiObservationError::OutputClosed)
    ));
    session.finish().expect("reap exited shell and close owner");
}

#[test]
fn a_fenced_interactive_view_is_refused_after_explicit_final_close() {
    let launch = TuiLaunch::new("sh").args(["-c", "IFS= read -r line"]);
    let mut session = TuiSession::spawn_launch(launch, "FENCED-CLOSED");
    session.finish().expect("close shell before observation");
    assert!(matches!(
        session.observe_at_fence(),
        Err(TuiObservationError::OwnerClosed)
    ));
}
