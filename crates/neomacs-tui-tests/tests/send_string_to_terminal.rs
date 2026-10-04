#![cfg(unix)]
//! Issue #453: `send-string-to-terminal` must reach the real terminal.
//!
//! KKP (`kkp.el`) negotiates with the terminal exclusively through this
//! primitive: the support query, the device-attributes request and the
//! `CSI > flags u` enable sequence all leave Lisp as bytes handed to
//! `send-string-to-terminal`.  GNU's `Fsend_string_to_terminal`
//! (src/dispnew.c:6808) writes those bytes to the tty; a stub that drops
//! them leaves the terminal silent and raw ESC as prefix input.  This
//! regression drives real PTYs and requires the marker bytes on BOTH
//! sides, exactly as a terminal would observe them.

use crate::support::{boot_pair, eval_expression_one, settle_session};
use neomacs_tui_tests::TuiSession;
use std::time::{Duration, Instant};

/// Unlikely enough that no startup, mode-line or dialog text contains it.
const MARKER: &[u8] = b"KKP-PROBE-4Q3F-MARKER";

/// The marker is raw PTY output, not screen content -- bytes that corrupt
/// the grid are exactly the interesting ones here -- so poll the transport
/// instead of waiting on a rendered predicate.
fn send_marker_and_read(session: &mut TuiSession, label: &str) {
    session.clear_recent_output();
    eval_expression_one(
        session,
        &format!(
            "(send-string-to-terminal \"{}\")",
            std::str::from_utf8(MARKER).unwrap()
        ),
    );
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        session.read(Duration::from_millis(100));
        if session
            .recent_output()
            .windows(MARKER.len())
            .any(|window| window == MARKER)
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{label}: send-string-to-terminal bytes never reached the PTY; \
             recent output: {:?}",
            String::from_utf8_lossy(session.recent_output())
        );
    }
}

#[test]
fn send_string_to_terminal_reaches_the_pty_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    settle_session(&mut gnu);
    settle_session(&mut neo);
    send_marker_and_read(&mut gnu, "GNU");
    send_marker_and_read(&mut neo, "NEO");
}
