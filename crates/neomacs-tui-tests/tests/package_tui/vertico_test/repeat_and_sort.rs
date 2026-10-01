//! `vertico-repeat` replaying the last session, and the order the package's
//! default sort function puts the candidates in.
//!
//! The fixture's three buffer names are chosen so the default sort function's
//! order is not the alphabetical one: `vertico-sort-history-length-alpha` sorts
//! by history, then by length, then alphabetically, and one of the names is
//! longer than the other two. That order is pinned on the first screen and
//! again on the replayed one.
//!
//! Setting `vertico-sort-function` to another sort function is *not* covered
//! here: with `vertico-sort-alpha` set at the keyboard, GNU orders the same
//! candidates alphabetically and Neomacs keeps ordering them by history first,
//! so the two screens cannot be compared. That is a finding of its own,
//! reported rather than frozen.
//!
//! The replay is the session `C-x b` left behind: `vertico-repeat` reopens the
//! command it was and restores the input that was typed. Its candidate list is
//! the one `switch-to-buffer` builds, which excludes the buffer the session
//! would switch away from -- and by then that is the buffer the first session
//! switched *to*, so the replayed session has one candidate fewer than the
//! session it replays. The count indicator reports that, and so does the saved
//! selection: the candidate that was current is no longer in the list.

use std::time::Duration;

use expect_test::expect_file;

use super::harness::*;
use super::prelude::*;

/// The fixture's buffer prefix, which filters `C-x b` down to those buffers.
const PREFIX: &str = "quartz-";

/// The `C-x b` prompt, whose row holds the count indicator and the input.
const PROMPT: &str = "Switch to buffer";

/// The fixture: two of the names are the same length and the third is longer,
/// so the default sort function's order is not the names' own order.
const NAMES: [&str; 3] = ["quartz-zz", "quartz-aaa", "quartz-mm"];

/// What the package's default sort function makes of them.
const DEFAULT_ORDER: [&str; 3] = ["quartz-mm", "quartz-zz", "quartz-aaa"];

pub(super) fn run() {
    let prelude = repeat_vertico(&fixture_buffers(&NAMES.map(str::to_owned)));
    let mut pair = spawn_pair("vertico-repeat", &prelude, 24, 80);
    let outcome =
        catch_phase("Vertico repeat scenario", || run_body(&mut pair)).and_then(|result| result);
    assert!(outcome.is_ok(), "{}", outcome.err().unwrap_or_default());
}

fn run_body(pair: &mut PackageTuiPair) -> Result<(), String> {
    // The session this scenario replays: type the prefix, move the selection to
    // the second candidate, and accept it.
    both(pair, "open switch-to-buffer", |session| {
        session.send_keys("C-x b");
        wait_for(
            session,
            Duration::from_secs(8),
            "the switch-to-buffer prompt",
            |grid| grid.iter().any(|row| row.contains(PROMPT)),
        );
    })?;
    both(pair, "type the fixture prefix", |session| {
        session.send(b"quartz-");
        wait_for(
            session,
            Duration::from_secs(8),
            "the fixture's three candidates",
            |grid| candidate_rows(grid, PREFIX).len() == NAMES.len(),
        );
    })?;
    assert_order(pair, &DEFAULT_ORDER, "the package's default sort function");
    record_checkpoint(
        pair,
        "Vertico repeat default sort",
        expect_file!["snapshots/repeat_default_sort_ansi_grid.ansi"],
        expect_file!["snapshots/repeat_default_sort_plain_grid.plain"],
    );

    press_and_wait_for_count(pair, "C-n", PROMPT, PREFIX, "2/3", Some("quartz-zz"))?;
    both(pair, "accept the second candidate", |session| {
        session.send_key("RET");
        wait_for(
            session,
            Duration::from_secs(8),
            "the accepted buffer",
            |grid| grid.iter().any(|row| row.contains("quartz-zz")),
        );
    })?;
    assert_eq!(
        mode_line(&pair.neo),
        mode_line(&pair.gnu),
        "the accepted buffer's mode line differs from GNU"
    );

    // `vertico-repeat` reopens it: the same command, the same input, and the
    // candidate that was current when the session ended.
    both(pair, "replay the session", |session| {
        invoke(session, "vertico-repeat", PROMPT);
    })?;
    assert_eq!(
        minibuffer_input(&pair.neo, PROMPT),
        minibuffer_input(&pair.gnu, PROMPT),
        "the replayed session's input differs from GNU's"
    );
    assert_eq!(
        minibuffer_input(&pair.gnu, PROMPT),
        PREFIX,
        "the replayed session did not restore the input"
    );
    // The buffer the first session switched to is not among them any more, so
    // the replayed session has two candidates and the first one is current.
    assert_candidate_state(pair, PREFIX, PROMPT, "1/2", Some("quartz-mm"));
    record_checkpoint(
        pair,
        "Vertico repeat replayed session",
        expect_file!["snapshots/repeat_replayed_session_ansi_grid.ansi"],
        expect_file!["snapshots/repeat_replayed_session_plain_grid.plain"],
    );

    // Abandon it, and the buffer the first session switched to is still the
    // current one: `switch-to-buffer` leaves that buffer out of its candidates,
    // so a fresh session over the same prefix has one candidate fewer.
    both(pair, "abort the replayed session", |session| {
        session.send_key("C-g");
        wait_for(
            session,
            Duration::from_secs(8),
            "the minibuffer to close",
            |grid| !grid.iter().any(|row| row.contains(PROMPT)),
        );
    })?;
    both(pair, "ask for the prefix again", |session| {
        session.send_keys("C-x b");
        wait_for(
            session,
            Duration::from_secs(8),
            "the switch-to-buffer prompt",
            |grid| grid.iter().any(|row| row.contains(PROMPT)),
        );
        session.send(b"quartz-");
        wait_for(
            session,
            Duration::from_secs(8),
            "the remaining candidates",
            |grid| count_indicator_in(grid, PROMPT) != "1/3",
        );
    })?;
    assert_candidate_state(pair, PREFIX, PROMPT, "1/2", Some("quartz-mm"));
    Ok(())
}

/// The candidate order the screen shows is `expected` in both peers.
fn assert_order(pair: &PackageTuiPair, expected: &[&str], description: &str) {
    assert_eq!(
        candidate_texts(&pair.neo.text_grid(), PREFIX),
        candidate_texts(&pair.gnu.text_grid(), PREFIX),
        "{description}: the candidate order differs from GNU's"
    );
    assert_eq!(
        candidate_texts(&pair.gnu.text_grid(), PREFIX),
        expected,
        "{description}: the candidate order is not the expected one"
    );
}
