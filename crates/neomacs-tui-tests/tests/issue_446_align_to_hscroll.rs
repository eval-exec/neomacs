#![cfg(unix)]
//! TUI comparison tests: issue #446 — horizontal scrolling over text that
//! carries `(space :align-to N)` display specifications.
//!
//! The reporter's recipe is replayed here: a `*align-test*` buffer whose rows
//! place five labelled fields on fixed columns with `:align-to`, truncation
//! turned on, and a frame narrower than the finished line, so moving to the
//! end of a line takes the window past its right edge. The report is that the
//! window then scrolls only part of the way — "the first 3 columns move to the
//! left, but the other right columns are fixed" — leaving `:align-to` fields
//! pinned to screen columns while the text around them moves. Where each field
//! lands after the window scrolls is exactly what a paired display comparison
//! measures, so these cases drive the recipe with `C-e`, `scroll-left`
//! (`C-x <`) and `scroll-right` (`C-x >`) and compare the whole screen.
//!
//! Scope note: the same report describes a stray `$` at the left edge of the
//! scrolled line. The reporter calls that behaviour correct under `nw` and
//! wrong only under a window system; this harness drives `nw` sessions only,
//! so that half is not reachable here and remains with the GUI suite. The
//! cases below still compare the left edge exactly — under `nw` both editors
//! are expected to agree on it — but they are not a reproduction of that half
//! of the report.
//!
//! Fixture rules, which the whole file depends on:
//!
//! * the fixture buffer is created by the fixture with a literal name and
//!   literal contents: nothing here reads the clock, the locale, the user name
//!   or any installed tool, and no buffer visits a file (so no per-session
//!   path can reach the screen);
//! * the buffer is created but *not* displayed: `pop-to-buffer` in the
//!   reporter's recipe is free to split the frame, which would make the window
//!   geometry depend on layout thresholds rather than on this fixture. The
//!   cases enter it with `C-x b`, which is the recipe's "move to the buffer"
//!   step in any case;
//! * the terminal is booted at the harness default size and narrowed
//!   afterwards, because a narrow boot can wrap the startup echo-area message
//!   and break the geometry the grid is compared on;
//! * `truncate-lines` is turned on through the recipe's own
//!   `M-x toggle-truncate-lines`, and the scrolls use the recipe's own keys.

use crate::support;
use neomacs_tui_tests::*;
use std::time::Duration;
use support::*;

// ── Fixtures ───────────────────────────────────────────────

/// Rows in the fixture buffer: the recipe's own fifty, far more than one
/// screen, so the window is always full and the first screen is fixed.
const ALIGN_ROWS: usize = 50;

/// The frame the recipe's "adjust frame size" narrows to.
///
/// The fixture's lines are ninety-five columns wide, so every row runs off
/// this frame's right edge — the condition the recipe sets up before it moves
/// to the end of the line.
const FRAME_ROWS: u16 = 24;
const FRAME_COLUMNS: u16 = 80;

/// The reporter's `test.el`, with the buffer created but not displayed.
fn align_fixture_source() -> String {
    format!(
        ";;; issue-446-align.el --- deterministic `:align-to' fixture  -*- lexical-binding: t; -*-\n\
         (progn\n\
         \x20 ;; The recipe scrolls with `C-x <' and `C-x >', which GNU ships\n\
         \x20 ;; disabled: enable the two commands so the keys reach them without\n\
         \x20 ;; a disabled-command prompt splitting the frame mid-scenario.\n\
         \x20 (put 'scroll-left 'disabled nil)\n\
         \x20 (put 'scroll-right 'disabled nil)\n\
         \x20 (with-current-buffer (get-buffer-create \"*align-test*\")\n\
         \x20   (let ((inhibit-read-only t))\n\
         \x20     (erase-buffer)\n\
         \x20     (dotimes (i {ALIGN_ROWS})\n\
         \x20       (insert (format \"ROW-%03d\" i))\n\
         \x20       (insert (propertize \" \" 'display '(space :align-to 11)))\n\
         \x20       (insert (format \"AAAA-%03d\" i))\n\
         \x20       (insert (propertize \" \" 'display '(space :align-to 21)))\n\
         \x20       (insert (format \"BBBB-%03d\" i))\n\
         \x20       (insert (propertize \" \" 'display '(space :align-to 31)))\n\
         \x20       (insert (format \"CCCC-%03d\" i))\n\
         \x20       (insert (propertize \" \" 'display '(space :align-to 41)))\n\
         \x20       (insert (format \"DDDD-%03d\" i))\n\
         \x20       (insert (propertize \" \" 'display '(space :align-to 69)))\n\
         \x20       (insert (format \"EEEE-%03d\" i))\n\
         \x20       (insert (propertize \" \" 'display '(space :align-to 87)))\n\
         \x20       (insert (format \"FFFF-%03d\" i))\n\
         \x20       (insert \"\\n\"))\n\
         \x20     (goto-char (point-min))\n\
         \x20     (setq-local buffer-read-only t))))\n"
    )
}

/// Boot both editors with the fixture loaded.
fn boot_align_fixture() -> (TuiSession, TuiSession, TuiTempFile) {
    let fixture = TuiTempFile::new(
        "neomacs-issue-446-",
        "issue-446-align.el",
        align_fixture_source(),
    );
    let extra = format!("-l {}", fixture.display());
    let (gnu, neo) = boot_pair(&extra);
    (gnu, neo, fixture)
}

// ── Pair helpers ───────────────────────────────────────────

/// Wait for `predicate` on both editors, then drain the frame it paints.
fn step(gnu: &mut TuiSession, neo: &mut TuiSession, predicate: impl Fn(&[String]) -> bool + Copy) {
    gnu.read_until(Duration::from_secs(8), predicate);
    neo.read_until(Duration::from_secs(12), predicate);
    read_both(gnu, neo, Duration::from_millis(500));
}

/// Boot, narrow the frame the way the recipe's "adjust frame size" does, and
/// enter the fixture buffer.
fn open_align_fixture() -> (TuiSession, TuiSession, TuiTempFile) {
    let (mut gnu, mut neo, fixture) = boot_align_fixture();
    resize_both(&mut gnu, &mut neo, FRAME_ROWS, FRAME_COLUMNS);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x b");
    let prompt = |grid: &[String]| {
        grid.last()
            .is_some_and(|row| row.contains("Switch to buffer"))
    };
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    for session in [&mut gnu, &mut neo] {
        session.send(b"*align-test*");
    }
    send_both(&mut gnu, &mut neo, "RET");
    step(&mut gnu, &mut neo, |grid| {
        grid.iter().any(|row| row.contains("ROW-000"))
    });

    (gnu, neo, fixture)
}

/// The recipe's `M-x toggle truncate lines`, waited out through its own report.
fn toggle_truncate_lines(gnu: &mut TuiSession, neo: &mut TuiSession) {
    invoke_mx_command(gnu, neo, "toggle-truncate-lines");
    step(gnu, neo, |grid| {
        grid.iter()
            .any(|row| row.contains("Truncate long lines enabled"))
    });
}

/// Assert `predicate` against each editor's own grid, naming it on failure.
fn assert_both(
    what: &str,
    gnu: &TuiSession,
    neo: &TuiSession,
    predicate: impl Fn(&[String]) -> bool,
) {
    for (label, session) in [("GNU", gnu), ("Neomacs", neo)] {
        let grid = session.text_grid();
        assert!(
            predicate(&grid),
            "{label} should {what}; its grid was:\n{}",
            grid.join("\n")
        );
    }
}

/// Assert that both editors render fixture row `index` as exactly `expected`.
///
/// The screen row is matched against the whole eighty-column line, so the
/// assertion fixes both the text and the column every field of it sits on --
/// which is what the report is about.
fn assert_row_both(what: &str, gnu: &TuiSession, neo: &TuiSession, expected: &str) {
    assert_both(what, gnu, neo, |grid| {
        grid.iter().any(|row| row.as_str() == expected)
    });
}

// ── The fixture's first line under the three scroll states ──

/// Before anything scrolls it: the five aligned fields on their `:align-to`
/// columns out to `EEEE-000` at column 69, with the last `:align-to` (87)
/// already past the right edge of the eighty-column frame, so the row ends in
/// the truncation glyph.
fn align_to_row(row: usize) -> String {
    format!(
        "ROW-{row:03}    AAAA-{row:03}  BBBB-{row:03}  CCCC-{row:03}  DDDD-{row:03}{:20}EEEE-{row:03}  $",
        ""
    )
}

/// After `C-e` on the first line: point is on the last display column of the
/// line, so the window has scrolled nineteen columns and the row now opens
/// inside the space that carries `:align-to 11` (its first cell spent on the
/// left truncation glyph) and ends with the last field fully visible.
///
/// The row stops at that field: the scrolled line now ends before the window
/// does, so GNU paints nothing in the four columns that follow and the row's
/// text ends there (the paired display comparison still checks those cells
/// against Neomacs's, this string is only the painted text).
fn align_to_row_scrolled_to_eol(row: usize) -> String {
    format!(
        "$ BBBB-{row:03}  CCCC-{row:03}  DDDD-{row:03}{:20}EEEE-{row:03}{:10}FFFF-{row:03}",
        "", ""
    )
}

/// After `C-x <` scrolls twelve columns left: every field has moved twelve
/// columns left with the text, the row opens inside `AAAA-000`, and the last
/// field is cut off at the right edge.
fn align_to_row_after_scroll_left_twelve(row: usize) -> String {
    format!(
        "$AA-{row:03}  BBBB-{row:03}  CCCC-{row:03}  DDDD-{row:03}{:20}EEEE-{row:03}{:10}FFFF$",
        "", ""
    )
}

/// The rows each scroll state is asserted on.
const CHECKED_ROWS: std::ops::Range<usize> = 0..4;

// ── Tests ──────────────────────────────────────────────────

/// The layout half of the report: with truncation on and the frame narrower
/// than the line, each `:align-to` field must still land on its own column,
/// and the row must be cut off at the right edge rather than reflowed.
///
/// This is the state the recipe scrolls *from*, so a divergence here would
/// make every scroll assertion afterwards ambiguous — and Neomacs does diverge
/// in it, at the right edge. GNU clips the `:align-to 87` stretch at the
/// window and paints the truncation glyph in the last column, leaving the text
/// that follows the spec undrawn (`... EEEE-000  $'); Neomacs pulls the
/// stretch back into the frame and then draws the first two characters of the
/// field that should have started at column 87 (`... EEEE-000FF$'), differing
/// from GNU from column 77 on.
///
/// This case is meant to stay RED: it is a real divergence and it is not to be
/// blessed from Neomacs, weakened, or skipped.
#[test]
fn issue_446_align_to_columns_land_on_their_exact_columns() {
    let (mut gnu, mut neo, _fixture) = open_align_fixture();
    toggle_truncate_lines(&mut gnu, &mut neo);

    for row in CHECKED_ROWS {
        assert_row_both(
            "place every aligned field on its :align-to column",
            &gnu,
            &neo,
            &align_to_row(row),
        );
    }
    // The alignment past column 80 (87) cannot be honoured in an
    // eighty-column frame, so the last field is off the screen entirely.
    assert_both(
        "keep the field aligned past the right edge off the screen",
        &gnu,
        &neo,
        |grid| !grid.iter().any(|row| row.contains("FFFF-000")),
    );

    dump_pair_grids(
        "issue_446_align_to_columns_land_on_their_exact_columns",
        &gnu,
        &neo,
    );
    assert_pair_exact_display(
        "issue_446_align_to_columns_land_on_their_exact_columns",
        &gnu,
        &neo,
    );
}

/// The reporter's own reproduction: point at the end of a line that runs off
/// the right edge, so redisplay scrolls the window horizontally.
///
/// At `C-e` point is on the last display column of the line, fifteen columns
/// past the frame, so the whole line has to move left — every `:align-to`
/// field with it. GNU scrolls nineteen columns and every field moves with the
/// text; Neomacs scrolls the plain text (twenty-three columns' worth of it)
/// but then lays the fields that follow an `:align-to` spec back down on their
/// unscrolled screen columns, so `CCCC-000`, `DDDD-000` and `EEEE-000` sit
/// where they did before the scroll and the row differs from GNU from column 1
/// on. This is the reporter's own reproduction.
///
/// This case is meant to stay RED: it is a real divergence and it is not to be
/// blessed from Neomacs, weakened, or skipped.
#[test]
fn issue_446_ce_scrolls_an_align_to_line_like_gnu() {
    let (mut gnu, mut neo, _fixture) = open_align_fixture();
    toggle_truncate_lines(&mut gnu, &mut neo);

    send_both(&mut gnu, &mut neo, "C-e");
    step(&mut gnu, &mut neo, |grid| {
        !grid.iter().any(|row| row.starts_with("ROW-000"))
    });

    // The end of the line is the last field, so the window must have moved
    // far enough left for it to be on screen at all.
    for row in CHECKED_ROWS {
        assert_row_both(
            "scroll the whole line, not just its leftmost fields, at the end of it",
            &gnu,
            &neo,
            &align_to_row_scrolled_to_eol(row),
        );
    }

    dump_pair_grids("issue_446_ce_scrolls_an_align_to_line_like_gnu", &gnu, &neo);
    assert_pair_exact_display("issue_446_ce_scrolls_an_align_to_line_like_gnu", &gnu, &neo);
}

/// Manual horizontal scrolling over the same text: `scroll-left` must move
/// every field by the same amount, and `scroll-right` must put them back.
///
/// The same divergence as the `C-e` case, reached through the recipe's own
/// `C-x <`/`C-x >` keys rather than through auto-scroll: after twelve columns
/// left, GNU parts with the row at column 1 (`$AA-000  BBBB-000 ...`), while
/// Neomacs leaves `BBBB-000` and everything after it on their unscrolled
/// columns. Scrolling back right restores the fields on both editors, so the
/// second checkpoint only keeps the right-edge cell difference of the case
/// above.
///
/// This case is meant to stay RED: it is a real divergence and it is not to be
/// blessed from Neomacs, weakened, or skipped.
#[test]
fn issue_446_scroll_left_and_right_move_every_aligned_field() {
    let (mut gnu, mut neo, _fixture) = open_align_fixture();
    toggle_truncate_lines(&mut gnu, &mut neo);

    // Twelve columns left: the first field is pushed under the left edge and
    // the rest follow it.
    send_both(&mut gnu, &mut neo, "C-u 12 C-x <");
    step(&mut gnu, &mut neo, |grid| {
        !grid.iter().any(|row| row.starts_with("ROW-000"))
    });
    for row in CHECKED_ROWS {
        assert_row_both(
            "move every field with the text on scroll-left",
            &gnu,
            &neo,
            &align_to_row_after_scroll_left_twelve(row),
        );
    }
    dump_pair_grids(
        "issue_446_scroll_left_and_right_move_every_aligned_field/left",
        &gnu,
        &neo,
    );
    assert_pair_exact_display(
        "issue_446_scroll_left_and_right_move_every_aligned_field/left",
        &gnu,
        &neo,
    );

    // Back to the left edge: the fields return to their `:align-to` columns.
    send_both(&mut gnu, &mut neo, "C-u 12 C-x >");
    step(&mut gnu, &mut neo, |grid| {
        grid.iter().any(|row| row.starts_with("ROW-000"))
    });
    for row in CHECKED_ROWS {
        assert_row_both(
            "return every field to its :align-to column on scroll-right",
            &gnu,
            &neo,
            &align_to_row(row),
        );
    }
    dump_pair_grids(
        "issue_446_scroll_left_and_right_move_every_aligned_field/right",
        &gnu,
        &neo,
    );
    assert_pair_exact_display(
        "issue_446_scroll_left_and_right_move_every_aligned_field/right",
        &gnu,
        &neo,
    );
}
