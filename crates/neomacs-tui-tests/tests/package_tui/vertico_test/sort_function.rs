//! `vertico-sort-function` set at the keyboard, and the order it puts the
//! candidates in.
//!
//! The fixture's three names all have different lengths, and the scenario walks
//! the same completion through three orders:
//!
//! * the package's default, `vertico-sort-history-length-alpha`, over an empty
//!   minibuffer history -- by length, then alphabetically;
//! * the same sort function once one of the candidates has been accepted, which
//!   puts it at the head of `buffer-name-history` and therefore at the head of
//!   the candidate list;
//! * `vertico-sort-alpha`, evaluated at the keyboard -- alphabetical, whatever
//!   the history says.
//!
//! The third screen is a divergence: GNU orders the candidates alphabetically,
//! ignoring the history that ordered the second screen, while Neomacs keeps
//! putting the history's candidate first. The fixture accepts a candidate that
//! is neither the first nor the last alphabetically, so that the two orders
//! disagree in a row the screen can see.
//!
//! Between the second and third sessions the scenario visits two buffers that
//! are not candidates, because `C-x b` hands the minibuffer a default value --
//! the most recently displayed other buffer -- and vertico moves the default to
//! the front of the list whatever the sort function says. That is the package's
//! own behaviour, in both editors, and it would otherwise hide the history the
//! third screen is about.
//!
//! The frozen files are blessed from GNU's screen, because GNU's order is the
//! behaviour the scenario asserts, and the comparison against Neomacs is left
//! red until `vertico-sort-function` is honoured. That failure is the point of
//! this scenario: it is not to be re-blessed away, and the third screen must
//! not be re-blessed from Neomacs.

use std::time::Duration;

use expect_test::expect_file;

use super::harness::*;
use super::prelude::*;

/// The fixture's buffer prefix, which filters `C-x b` down to those buffers.
const PREFIX: &str = "quartz-";

/// The `C-x b` prompt, whose row holds the count indicator and the input.
const PROMPT: &str = "Switch to buffer";

/// The buffer the scenario starts in and returns to between sessions.
const HOME: &str = "*scratch*";

/// The fixture: one name per length, so the orders below are different orders.
const NAMES: [&str; 3] = ["quartz-bbb", "quartz-aaaa", "quartz-cc"];

/// What `vertico-sort-history-length-alpha` makes of them with an empty
/// history: by length, then alphabetically.
const LENGTH_ALPHA_ORDER: [&str; 3] = ["quartz-cc", "quartz-bbb", "quartz-aaaa"];

/// The candidate the first session accepts. Accepting it is what puts it into
/// `buffer-name-history`, which the default sort function's first key reads.
const ACCEPTED: &str = "quartz-bbb";

/// What the default sort function makes of them once [`ACCEPTED`] is in the
/// history: the history's candidate first, the rest by length and alphabet.
const HISTORY_ORDER: [&str; 3] = ["quartz-bbb", "quartz-cc", "quartz-aaaa"];

/// What `vertico-sort-alpha` makes of them: alphabetical, history or no
/// history.
const ALPHA_ORDER: [&str; 3] = ["quartz-aaaa", "quartz-bbb", "quartz-cc"];

/// The form the scenario evaluates at the keyboard before the last session.
const ALPHA_SORT: &str = "(setq vertico-sort-function #'vertico-sort-alpha)";

pub(super) fn run() {
    let prelude = default_vertico(&format!(
        "{}{}",
        fixture_buffers(&NAMES.map(str::to_owned)),
        VERTICO_HELD_PRELUDE
    ));
    let mut pair = spawn_pair("vertico-sort-function", &prelude, 24, 80);
    let outcome =
        catch_phase("Vertico sort-function scenario", || run_body(&mut pair)).and_then(|r| r);
    assert!(outcome.is_ok(), "{}", outcome.err().unwrap_or_default());
}

fn run_body(pair: &mut PackageTuiPair) -> Result<(), String> {
    // An empty history: the default sort function has nothing to rank first.
    open_session(pair, LENGTH_ALPHA_ORDER[0])?;
    assert_order(
        pair,
        &LENGTH_ALPHA_ORDER,
        "the default sort over an empty history",
    );
    record_checkpoint(
        pair,
        "Vertico sort-function default order",
        expect_file!["snapshots/sort_function_default_ansi_grid.ansi"],
        expect_file!["snapshots/sort_function_default_plain_grid.plain"],
    );

    // Accept a candidate that is neither the first nor the last one, so the
    // history the next session reads differs from the order this session shows.
    select(pair, &LENGTH_ALPHA_ORDER, ACCEPTED)?;
    // `C-x b` leaves the current buffer out of its candidates, and hands the
    // minibuffer the most recently displayed other buffer as its default, which
    // vertico moves to the front of the list. Visiting the held buffer and then
    // coming back leaves a buffer that is no candidate of the fixture's own.
    switch_to(pair, VERTICO_HELD_BUFFER, "held buffer line")?;
    switch_to(pair, HOME, HOME)?;

    // The same completion, with the accepted candidate at the head of the
    // history and no other change.
    open_session(pair, HISTORY_ORDER[0])?;
    assert_order(pair, &HISTORY_ORDER, "the default sort over the history");
    record_checkpoint(
        pair,
        "Vertico sort-function history order",
        expect_file!["snapshots/sort_function_history_ansi_grid.ansi"],
        expect_file!["snapshots/sort_function_history_plain_grid.plain"],
    );
    abort_session(pair)?;

    // The sort function is changed the way a user changes it -- evaluated at
    // the keyboard -- rather than from the prelude, so the scenario is about
    // the package reading the variable and not about the fixture that set it.
    eval_expression(pair, ALPHA_SORT)?;

    // The same command over the same buffers and the same history, with the
    // sort function now saying the order is alphabetical. GNU's screen is the
    // alphabetical one; the comparison against Neomacs is what fails here.
    open_session(pair, ALPHA_ORDER[0])?;
    assert_gnu_order(pair, &ALPHA_ORDER, "the sort function set at the keyboard");
    record_checkpoint(
        pair,
        "Vertico sort-function alphabetical order",
        expect_file!["snapshots/sort_function_alpha_ansi_grid.ansi"],
        expect_file!["snapshots/sort_function_alpha_plain_grid.plain"],
    );
    Ok(())
}

/// Open `C-x b`, type the fixture's prefix and wait for its three candidates.
///
/// `current` is the candidate the order under test puts first, which is the one
/// the session starts on.
fn open_session(pair: &mut PackageTuiPair, current: &str) -> Result<(), String> {
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
    assert_candidate_state(pair, PREFIX, PROMPT, "1/3", Some(current));
    Ok(())
}

/// Walk the selection to `candidate`, which `order` puts in the list, and
/// accept it.
fn select(pair: &mut PackageTuiPair, order: &[&str], candidate: &str) -> Result<(), String> {
    let index = order
        .iter()
        .position(|name| *name == candidate)
        .unwrap_or_else(|| panic!("{candidate} is not in the fixture"));
    for step in 2..=index + 1 {
        let count = format!("{step}/3");
        press_and_wait_for_count(pair, "C-n", PROMPT, PREFIX, &count, Some(candidate))?;
    }
    both(pair, "accept the candidate", |session| {
        session.send_key("RET");
        wait_for(
            session,
            Duration::from_secs(8),
            "the accepted buffer",
            |grid| grid.iter().any(|row| row.contains(candidate)),
        );
    })
}

/// Switch to `buffer` by typing its name, and wait for `marker` to appear.
fn switch_to(pair: &mut PackageTuiPair, buffer: &str, marker: &str) -> Result<(), String> {
    both(pair, "switch to another buffer", |session| {
        session.send_keys("C-x b");
        wait_for(
            session,
            Duration::from_secs(8),
            "the switch-to-buffer prompt",
            |grid| grid.iter().any(|row| row.contains(PROMPT)),
        );
        session.send(buffer.as_bytes());
        wait_for(
            session,
            Duration::from_secs(8),
            "the named candidate",
            |grid| candidate_texts(grid, buffer) == vec![buffer.to_owned()],
        );
        session.send_key("RET");
        wait_for(
            session,
            Duration::from_secs(8),
            "the named buffer's screen",
            |grid| grid.iter().any(|row| row.contains(marker)),
        );
    })
}

/// Abandon the open session with `C-g`.
fn abort_session(pair: &mut PackageTuiPair) -> Result<(), String> {
    both(pair, "abort the session", |session| {
        session.send_key("C-g");
        wait_for(
            session,
            Duration::from_secs(8),
            "the minibuffer to close",
            |grid| !grid.iter().any(|row| row.contains(PROMPT)),
        );
    })
}

/// Evaluate `expression` at the keyboard in both peers.
///
/// The expression is typed into `eval-expression`'s own minibuffer, so the
/// variable is changed the way a user changes it rather than by the fixture
/// that boots the package.
fn eval_expression(pair: &mut PackageTuiPair, expression: &str) -> Result<(), String> {
    both(
        pair,
        "evaluate the sort function at the keyboard",
        |session| {
            invoke(session, "eval-expression", "Eval: ");
            session.send(expression.as_bytes());
            session.send_key("RET");
            wait_for(
                session,
                Duration::from_secs(8),
                "the eval minibuffer to close",
                |grid| !grid.iter().any(|row| row.contains("Eval: ")),
            );
        },
    )
}

/// GNU's candidate order is `expected`.
fn assert_gnu_order(pair: &PackageTuiPair, expected: &[&str], description: &str) {
    assert_eq!(
        candidate_texts(&pair.gnu.text_grid(), PREFIX),
        expected,
        "{description}: the order GNU renders is not the expected one"
    );
}

/// The candidate order both peers show is `expected`.
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
