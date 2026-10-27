#![cfg(unix)]
//! Observable-behaviour comparison for the producer contracts C6 (combining
//! cluster classes) and C7 (invisible-text elision).
//!
//! The producer stream harness asserts internal ownership: today the producer
//! streams a combining extender as a plain scan step and does not elide
//! invisible text, because the renderer owns both decisions. These tests pin
//! what the user actually sees on a terminal, against GNU:
//!
//! * a base plus combining mark occupies the same cells GNU gives it (the mark
//!   adds no advance and never splits from its base), and
//! * text under an `invisible` property is absent from the grid GNU builds.
//!
//! If these pass, the harness contracts are future-refactor acceptance
//! criteria, not current-behaviour tests.

use crate::support;
use neomacs_tui_tests::*;
use std::time::Duration;
use support::*;

/// Both editors, started on a `-l` fixture that selects a named buffer and
/// fills it.  `-l` is parsed before the command loop, so the first displayed
/// frame already shows the fixture and no keystroke timing enters the
/// comparison.
fn pair_on_fixture(fixture: &TuiTempFile, needle: &str) -> (TuiSession, TuiSession) {
    let arg = format!("-l {}", fixture.path().display());
    let mut gnu = TuiSession::gnu_emacs(&arg);
    let mut neo = TuiSession::neomacs(&arg);
    let loaded = |rows: &[String]| rows.iter().any(|row| row.contains(needle));
    gnu.read_until(Duration::from_secs(10), loaded);
    neo.read_until(Duration::from_secs(15), loaded);
    // Let the first full redisplay settle on both sides.
    gnu.read(Duration::from_secs(1));
    neo.read(Duration::from_secs(1));
    (gnu, neo)
}

/// C6: "ae\u{301}z" must occupy exactly the cells GNU lays out.  The extender
/// is zero-advance and stays attached to its base; the user-visible grid is
/// three characters wide, not four.
#[test]
fn strict_combining_cluster_grid_matches_gnu() {
    let fixture = TuiTempFile::new(
        "neomacs-strict-c6-",
        "setup.el",
        "(switch-to-buffer (get-buffer-create \"strict-c6\"))\n\
         (erase-buffer)\n\
         (insert \"ae\u{301}z\\n\")\n\
         (goto-char (point-min))\n",
    );
    let (gnu, neo) = pair_on_fixture(&fixture, "z");

    // Non-vacuous: the fixture really is on screen, in both editors.  The
    // cluster is one row, base and mark together, with no gap.
    for (label, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.trim_end() == "ae\u{301}z"),
            "{label} did not display the combining cluster row: {grid:#?}"
        );
    }
    assert_pair_exact_display("combining cluster", &gnu, &neo);
}

/// C7: "abXXcd" with an `invisible` property over "XX" must display as "abcd"
/// in exactly the cells GNU uses.
#[test]
fn strict_invisible_span_grid_matches_gnu() {
    let fixture = TuiTempFile::new(
        "neomacs-strict-c7-",
        "setup.el",
        "(switch-to-buffer (get-buffer-create \"strict-c7\"))\n\
         (erase-buffer)\n\
         (insert \"abXXcd\\n\")\n\
         (add-text-properties 3 5 '(invisible t))\n\
         (goto-char (point-min))\n",
    );
    let (gnu, neo) = pair_on_fixture(&fixture, "abcd");

    // Non-vacuous: the visible text is exactly the two letters on either side
    // of the hidden run; the hidden characters are on no row and in no cell.
    for (label, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.trim_end() == "abcd"),
            "{label} did not display the elided row: {grid:#?}"
        );
        assert!(
            grid.iter().all(|row| !row.contains("XX")),
            "{label} displayed the invisible characters: {grid:#?}"
        );
    }
    assert_pair_exact_display("invisible span", &gnu, &neo);
}
