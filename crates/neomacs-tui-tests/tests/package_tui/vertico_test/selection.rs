//! C-n/C-p moving the highlight across the candidate window, and TAB/RET
//! accepting what is selected.
//!
//! Vertico's selection is the package's own state, and it is on screen twice:
//! in which candidate row carries the current-candidate highlight, and in the
//! count indicator ahead of the prompt. Both are compared with GNU's at every
//! step, and each step is a keystroke -- nothing is read back from the editor
//! and nothing is set up in the minibuffer for the scenario to observe.
//!
//! The keys are the C- ones because those are the navigation keys in this
//! configuration. Vertico remaps the *commands* Emacs binds to them
//! (`next-line`, `next-line-or-history-element`), and GNU binds the terminal's
//! arrow keys and C-n/C-p to those; M-n/M-p are `next-history-element` in the
//! minibuffer and stay history keys, in both editors.
//!
//! The fixture is the three `project-` buffers the suite's original scenario
//! uses, with `vertico-count 5` and `vertico-cycle t`, so the walk below covers
//! the whole cycle: the cycle includes the prompt, which this completion leaves
//! selectable (`C-x b` does not require a match).

use std::time::Duration;

use expect_test::expect_file;

use super::harness::*;
use super::prelude::VERTICO_TUI_PRELUDE;

/// The fixture's buffer prefix, which filters `C-x b` down to those buffers.
const PREFIX: &str = "project-";

/// The `C-x b` prompt, whose row holds the count indicator and the input.
const PROMPT: &str = "Switch to buffer";

pub(super) fn run() {
    let mut pair = spawn_pair("vertico-selection", VERTICO_TUI_PRELUDE, 24, 80);
    let outcome =
        catch_phase("Vertico selection scenario", || run_body(&mut pair)).and_then(|result| result);
    assert!(outcome.is_ok(), "{}", outcome.err().unwrap_or_default());
}

fn run_body(pair: &mut PackageTuiPair) -> Result<(), String> {
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
        session.send(b"project-");
        wait_for(
            session,
            Duration::from_secs(8),
            "the fixture's three candidates",
            |grid| candidate_rows(grid, PREFIX).len() == 3,
        );
    })?;

    // Sorted by the package's default sort function, and the first candidate is
    // current before any key moves it.
    assert_candidate_state(pair, PREFIX, PROMPT, "1/3", Some("project-beta"));
    record_checkpoint(
        pair,
        "Vertico selection first candidate",
        expect_file!["snapshots/selection_first_candidate_ansi_grid.ansi"],
        expect_file!["snapshots/selection_first_candidate_plain_grid.plain"],
    );

    // C-n walks forward through the candidates and then through the prompt:
    // `vertico-next` cycles through it too while it is selectable, which `C-x b`
    // leaves it (the command does not require a match). The indicator switches
    // to `*` for the prompt, and the candidate highlight leaves the rows.
    press_and_wait_for_count(pair, "C-n", PROMPT, PREFIX, "2/3", Some("project-alpha"))?;
    press_and_wait_for_count(pair, "C-n", PROMPT, PREFIX, "3/3", Some("project-notes"))?;
    record_checkpoint(
        pair,
        "Vertico selection last candidate",
        expect_file!["snapshots/selection_last_candidate_ansi_grid.ansi"],
        expect_file!["snapshots/selection_last_candidate_plain_grid.plain"],
    );
    press_and_wait_for_count(pair, "C-n", PROMPT, PREFIX, "*/3", None)?;
    record_checkpoint(
        pair,
        "Vertico selection prompt",
        expect_file!["snapshots/selection_prompt_ansi_grid.ansi"],
        expect_file!["snapshots/selection_prompt_plain_grid.plain"],
    );
    // Out of the prompt again, and the cycle is closed: the candidate after the
    // prompt is the first one.
    press_and_wait_for_count(pair, "C-n", PROMPT, PREFIX, "1/3", Some("project-beta"))?;

    // C-p is the same walk backwards. It reaches the prompt from the first
    // candidate, and the last candidate from the prompt.
    press_and_wait_for_count(pair, "C-p", PROMPT, PREFIX, "*/3", None)?;
    press_and_wait_for_count(pair, "C-p", PROMPT, PREFIX, "3/3", Some("project-notes"))?;
    press_and_wait_for_count(pair, "C-p", PROMPT, PREFIX, "2/3", Some("project-alpha"))?;
    press_and_wait_for_count(pair, "C-p", PROMPT, PREFIX, "1/3", Some("project-beta"))?;
    press_and_wait_for_count(pair, "C-n", PROMPT, PREFIX, "2/3", Some("project-alpha"))?;

    // TAB inserts the current candidate into the minibuffer. The sole remaining
    // match is that candidate, so the window shrinks to it.
    both(pair, "TAB inserts the current candidate", |session| {
        session.send_key("TAB");
        wait_for(
            session,
            Duration::from_secs(8),
            "the inserted candidate",
            |grid| count_indicator_in(grid, PROMPT) == "1/1",
        );
    })?;
    assert_eq!(
        minibuffer_input(&pair.neo, PROMPT),
        minibuffer_input(&pair.gnu, PROMPT),
        "Vertico's minibuffer input differs from GNU"
    );
    assert_eq!(
        minibuffer_input(&pair.gnu, PROMPT),
        "project-alpha",
        "TAB did not insert the current candidate"
    );
    assert_candidate_state(pair, PREFIX, PROMPT, "1/1", Some("project-alpha"));
    record_checkpoint(
        pair,
        "Vertico selection inserted candidate",
        expect_file!["snapshots/selection_inserted_ansi_grid.ansi"],
        expect_file!["snapshots/selection_inserted_plain_grid.plain"],
    );

    // RET accepts it: the buffer that is displayed is the one that was
    // selected, not the one the session started from.
    both(pair, "RET accepts the candidate", |session| {
        session.send_key("RET");
        wait_for(
            session,
            Duration::from_secs(8),
            "the accepted buffer's contents",
            |grid| grid.iter().any(|row| row.contains("ALPHA BUFFER")),
        );
    })?;
    assert_eq!(
        mode_line(&pair.neo),
        mode_line(&pair.gnu),
        "the accepted buffer's mode line differs from GNU"
    );
    assert!(
        mode_line(&pair.gnu).contains("project-alpha"),
        "the accepted buffer is not the selected one: {}",
        mode_line(&pair.gnu)
    );
    Ok(())
}
