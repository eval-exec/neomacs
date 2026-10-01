//! Completion annotations: what an `annotation-function` adds to a candidate.
//!
//! The suite's prelude defines the annotation function -- it is a plain
//! `completion-category-overrides` entry for the `buffer` category, which is
//! how a user attaches one to `C-x b` without any package of its own -- and it
//! pads every candidate to a fixed column before writing its annotation, so the
//! screen says where the annotation starts, not just that it is there. The
//! annotation is drawn in the `completions-annotations` face, which is the
//! standard one for it, and the candidates keep their own face for the part
//! that is the candidate.
//!
//! The scenario freezes the screen from GNU and checks the column the
//! annotation starts at and the candidates' rows against it. It is ordinary
//! coverage rather than a known divergence: if the two editors disagree, that
//! is a finding of its own and the failure stands.

use std::time::Duration;

use expect_test::expect_file;

use super::harness::*;
use super::prelude::*;

/// The fixture's buffer prefix, which filters `C-x b` down to those buffers.
const PREFIX: &str = "note-";

/// The `C-x b` prompt, whose row holds the count indicator and the input.
const PROMPT: &str = "Switch to buffer";

/// The fixture's candidate count.
const TOTAL: usize = 4;

/// The column the annotation function pads every candidate to, and the text it
/// writes there.
const ANNOTATION_COLUMN: usize = 20;
const ANNOTATION: &str = "[annotated]";

/// The terminal this scenario renders on.
const ROWS: u16 = 24;
const COLUMNS: u16 = 80;

pub(super) fn run() {
    let prelude = format!(
        "{}(setq completion-category-overrides \
         '((buffer (annotation-function . vertico-tui-annotate))))\n",
        default_vertico(&fixture_buffers(&numbered_names("note", TOTAL)))
    );
    let prelude = format!("{}{}", annotated_prelude(), prelude);
    let mut pair = spawn_pair("vertico-annotations", &prelude, ROWS, COLUMNS);
    let outcome =
        catch_phase("Vertico annotations scenario", || run_body(&mut pair)).and_then(|r| r);
    assert!(outcome.is_ok(), "{}", outcome.err().unwrap_or_default());
}

/// The annotation function itself, defined where the scenario can read it.
///
/// It pads the candidate out to [`ANNOTATION_COLUMN`] and then writes its
/// annotation, so a screen says where the annotation starts as well as what it
/// says.
fn annotated_prelude() -> String {
    format!(
        "(defun vertico-tui-annotate (cand)\n  \
         (concat (make-string (max 1 (- {ANNOTATION_COLUMN} (length cand))) ?\\s) \
         \"{ANNOTATION}\"))\n"
    )
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
        session.send(b"note-");
        wait_for(
            session,
            Duration::from_secs(8),
            "the fixture's candidates",
            |grid| candidate_rows(grid, PREFIX).len() == TOTAL,
        );
    })?;

    // Every candidate is annotated, and every annotation starts at the column
    // the annotation function pads to.
    let grid = pair.gnu.text_grid();
    for row in candidate_rows(&grid, PREFIX) {
        let contents = &grid[usize::from(row)];
        assert!(
            contents.contains(ANNOTATION),
            "GNU's candidate row {row} carries no annotation: {contents:?}"
        );
        let column = contents
            .find(ANNOTATION)
            .expect("the annotation was just found");
        assert_eq!(
            column, ANNOTATION_COLUMN,
            "GNU's candidate row {row} does not start its annotation where the \
             annotation function pads to: {contents:?}"
        );
    }

    record_checkpoint(
        pair,
        "Vertico annotations",
        expect_file!["snapshots/annotations_ansi_grid.ansi"],
        expect_file!["snapshots/annotations_plain_grid.plain"],
    );
    Ok(())
}
