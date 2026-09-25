//! GNU's optimization-1 guard (`decide_after_edit`) clause by clause, on
//! hand-built retained rows. Each case flips exactly one clause of an
//! otherwise keepable frame.

use super::*;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::glyph_matrix::GlyphRow;

struct Probe {
    text: Vec<char>,
    overlay_edges: Vec<i64>,
}

impl OneLineBufferProbe for Probe {
    fn overlay_touches(&self, pos: i64) -> bool {
        self.overlay_edges.contains(&pos)
    }
    fn char_at(&self, pos: i64) -> Option<char> {
        usize::try_from(pos)
            .ok()
            .and_then(|pos| self.text.get(pos).copied())
    }
    fn accessible(&self) -> (i64, i64) {
        (0, self.text.len() as i64)
    }
}

/// "aaaa\nbbbb\ncccc\n" after inserting one char into "bbbb" (line 2 starts
/// at 5); the retained row is the OLD line 2, [5, 9) with its newline at 9.
fn probe() -> Probe {
    Probe {
        text: "aaaa\nbbbbx\ncccc\n".chars().collect(),
        overlay_edges: Vec::new(),
    }
}

fn row(start: usize, end: usize) -> GlyphRow {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.enabled = true;
    row.start_charpos = start;
    row.end_charpos = end;
    row.height_px = 16.0;
    row
}

/// A keepable edit: one char typed at the end of line 2 (old Z 15, new Z 16).
fn facts() -> OneLineFacts {
    OneLineFacts {
        chrome_dirty: false,
        modified_star_flipped: false,
        column_displayed: false,
        other_windows_changed: false,
        selected_window: true,
        shows_current_buffer: true,
        window_start_moved: false,
        line_numbers_displayed: false,
        bidi_auto_paragraph: false,
        selective_display: false,
        old_z: 15,
        new_z: 16,
        point: 10,
        beg_unchanged: 9,
        end_unchanged: 6,
    }
}

fn decide(facts: OneLineFacts, line: &GlyphRow, previous: Option<&GlyphRow>) -> ModeLineDecision {
    decide_after_edit(
        facts,
        Some(RecordedLine {
            row_index: 1,
            row: line,
            previous,
        }),
        &probe(),
    )
}

fn evaluates(decision: ModeLineDecision) -> Option<ModeLineEvaluateReason> {
    match decision {
        ModeLineDecision::Evaluate(reason) => Some(reason),
        ModeLineDecision::KeepRetained(_) => None,
    }
}

#[test]
fn an_in_line_edit_keeps_the_mode_line_with_the_shifted_line_as_contract() {
    let line = row(5, 9);
    let ModeLineDecision::KeepRetained(contract) = decide(facts(), &line, None) else {
        panic!("an in-line edit is GNU's optimization 1");
    };
    assert_eq!(
        contract,
        OneLineContract {
            row_index: 1,
            start_charpos: 5,
            end_charpos: 10,
            height_px: 16.0,
            point: 10,
        }
    );
}

#[test]
fn a_property_change_from_the_line_start_counts_one_char_early() {
    // jit-lock marks `fontified nil` from the line start: GNU records the
    // change at start - 1, which is before the line.
    let line = row(5, 9);
    let mut facts = facts();
    facts.beg_unchanged = 4;
    assert_eq!(
        evaluates(decide(facts, &line, None)),
        Some(ModeLineEvaluateReason::TextOutsideLineChanged)
    );
    // A change starting exactly at the line start is inside it.
    facts.beg_unchanged = 5;
    assert_eq!(evaluates(decide(facts, &line, None)), None);
}

#[test]
fn a_change_past_the_line_end_evaluates() {
    let line = row(5, 9);
    let mut facts = facts();
    // tlendpos = old Z - (9 + 1) = 5; the change must leave >= 5 chars.
    facts.end_unchanged = 4;
    assert_eq!(
        evaluates(decide(facts, &line, None)),
        Some(ModeLineEvaluateReason::TextOutsideLineChanged)
    );
    facts.end_unchanged = 5;
    assert_eq!(evaluates(decide(facts, &line, None)), None);
}

#[test]
fn point_must_stay_on_the_recorded_line() {
    let line = row(5, 9);
    let mut facts = facts();
    facts.point = 4;
    assert_eq!(
        evaluates(decide(facts, &line, None)),
        Some(ModeLineEvaluateReason::PointLeftLine)
    );
    // PT <= Z - tlendpos admits the next line's start (display_line's
    // cursor check is what refuses it, after the walk).
    facts.point = 11;
    assert_eq!(evaluates(decide(facts, &line, None)), None);
    facts.point = 12;
    assert_eq!(
        evaluates(decide(facts, &line, None)),
        Some(ModeLineEvaluateReason::PointLeftLine)
    );
}

#[test]
fn continued_and_continuation_rows_record_no_line() {
    let mut line = row(5, 9);
    line.continued = true;
    assert_eq!(
        evaluates(decide(facts(), &line, None)),
        Some(ModeLineEvaluateReason::NoRecordedLine)
    );
    let line = row(5, 9);
    let mut previous = row(0, 5);
    previous.continued = true;
    assert_eq!(
        evaluates(decide(facts(), &line, Some(&previous))),
        Some(ModeLineEvaluateReason::NoRecordedLine)
    );
    assert_eq!(
        evaluates(decide_after_edit(facts(), None, &probe())),
        Some(ModeLineEvaluateReason::NoRecordedLine)
    );
}

#[test]
fn an_overlay_at_a_boundary_the_change_reaches_evaluates() {
    let line = row(5, 9);
    let mut facts = facts();
    facts.beg_unchanged = 5;
    let mut buffer = probe();
    buffer.overlay_edges = vec![5];
    let recorded = || RecordedLine {
        row_index: 1,
        row: &line,
        previous: None,
    };
    assert_eq!(
        evaluates(decide_after_edit(facts, Some(recorded()), &buffer)),
        Some(ModeLineEvaluateReason::OverlayAtLineEdge)
    );
    // The same overlay is harmless when the change starts later.
    facts.beg_unchanged = 6;
    assert_eq!(
        evaluates(decide_after_edit(facts, Some(recorded()), &buffer)),
        None
    );
    // END_UNCHANGED == tlendpos (5): the overlay at Z - end = 16 - 5 = 11.
    facts.end_unchanged = 5;
    buffer.overlay_edges = vec![11];
    assert_eq!(
        evaluates(decide_after_edit(facts, Some(recorded()), &buffer)),
        Some(ModeLineEvaluateReason::OverlayAtLineEdge)
    );
}

#[test]
fn every_window_level_clause_names_itself() {
    let line = row(5, 9);
    let cases: [(fn(&mut OneLineFacts), ModeLineEvaluateReason); 10] = [
        (
            |f| f.other_windows_changed = true,
            ModeLineEvaluateReason::OtherWindowsChanged,
        ),
        (
            |f| f.selected_window = false,
            ModeLineEvaluateReason::NotSelectedWindow,
        ),
        (
            |f| f.chrome_dirty = true,
            ModeLineEvaluateReason::ChromeDirty,
        ),
        (
            |f| f.modified_star_flipped = true,
            ModeLineEvaluateReason::ModifiedStarFlipped,
        ),
        (
            |f| f.column_displayed = true,
            ModeLineEvaluateReason::ColumnDisplayed,
        ),
        (
            |f| f.shows_current_buffer = false,
            ModeLineEvaluateReason::NotCurrentBuffer,
        ),
        (
            |f| f.window_start_moved = true,
            ModeLineEvaluateReason::WindowStartMoved,
        ),
        (
            |f| f.line_numbers_displayed = true,
            ModeLineEvaluateReason::LineNumbersDisplayed,
        ),
        (
            |f| f.bidi_auto_paragraph = true,
            ModeLineEvaluateReason::BidiAutoParagraph,
        ),
        (
            |f| {
                f.selective_display = true;
                f.beg_unchanged = 5;
            },
            ModeLineEvaluateReason::SelectiveDisplay,
        ),
    ];
    for (flip, reason) in cases {
        let mut facts = facts();
        flip(&mut facts);
        assert_eq!(evaluates(decide(facts, &line, None)), Some(reason));
    }
}

#[test]
fn a_vanished_continuation_line_evaluates() {
    // The line start no longer follows a newline and now holds one.
    let buffer = Probe {
        text: "aaaab\nccc\n".chars().collect(),
        overlay_edges: Vec::new(),
    };
    let line = row(5, 5);
    let mut facts = facts();
    facts.old_z = 11;
    facts.new_z = 10;
    facts.point = 5;
    facts.beg_unchanged = 5;
    facts.end_unchanged = 5;
    assert_eq!(
        evaluates(decide_after_edit(
            facts,
            Some(RecordedLine {
                row_index: 1,
                row: &line,
                previous: None,
            }),
            &buffer,
        )),
        Some(ModeLineEvaluateReason::ContinuationLineVanished)
    );
}

#[test]
fn the_contract_rejects_a_wrapped_moved_or_resized_line() {
    let contract = OneLineContract {
        row_index: 1,
        start_charpos: 5,
        end_charpos: 10,
        height_px: 16.0,
        point: 10,
    };
    assert!(contract.holds_for(Some(&row(5, 10))));
    assert!(!contract.holds_for(None));
    let mut wrapped = row(5, 10);
    wrapped.continued = true;
    assert!(!contract.holds_for(Some(&wrapped)));
    assert!(!contract.holds_for(Some(&row(5, 9))));
    let mut taller = row(5, 10);
    taller.height_px = 20.0;
    assert!(!contract.holds_for(Some(&taller)));
    let point_elsewhere = OneLineContract {
        point: 11,
        ..contract
    };
    assert!(!point_elsewhere.holds_for(Some(&row(5, 10))));
}
