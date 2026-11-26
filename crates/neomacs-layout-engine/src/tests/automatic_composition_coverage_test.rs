//! Coverage proof for N-orphan A3: every automatic composition the row walk
//! painted must lie in the range whose gstring widths the evaluator resolved.
//! A miss is reported so the leaf retries with a widened range instead of
//! letting the row writer measure the raw text (which is exactly the pre-fix
//! full-character-cell advance for a lone combining mark).

use crate::engine::uncovered_automatic_composition_span_range;

#[test]
fn resolved_range_covers_spans_the_walk_painted() {
    // resolve [0, 32); the walk consumed characters 0..20 from 0.
    assert_eq!(
        uncovered_automatic_composition_span_range([3usize, 17].into_iter(), 0, 20, &(0..32)),
        None,
        "spans before the walk end inside the resolved range are covered"
    );
    // A span at or after the walk end was not painted and needs no width.
    assert_eq!(
        uncovered_automatic_composition_span_range([40usize].into_iter(), 0, 20, &(0..32)),
        None,
        "spans the walk never reached are not covered by this check"
    );
}

#[test]
fn spans_before_the_window_start_are_not_painted() {
    // The snapshot's composition scan is wider than the window (it starts a
    // screenful before `window_start`).  Those spans are never painted by
    // this walk, so they must not force a wider-resolution retry.
    assert_eq!(
        uncovered_automatic_composition_span_range([2usize, 5].into_iter(), 10, 40, &(10..32)),
        None,
        "spans before the walk start are not painted and need no resolution"
    );
}

#[test]
fn lone_mark_on_the_partial_last_line_widens_the_range() {
    // The visible-row bound ends at 32, but the walk reached the partial last
    // line and painted the lone U+0301 starting at 32.  GNU's width for that
    // glyph comes from `composition_gstring_width`, not from measuring the
    // raw text, so the leaf must retry with a wider range.
    assert_eq!(
        uncovered_automatic_composition_span_range([32usize].into_iter(), 0, 33, &(0..32)),
        Some(32..33)
    );
}

#[test]
fn composition_after_window_start_leaves_the_old_bounds() {
    // After scrolling, the walk crossed the old bounds on both sides: a
    // composition now sits before the resolved start (7, still at or after
    // the walk start 5) and one beyond the old end (48).  Both are reported so
    // the retry widens at both ends; the span after the walk end (70) is not
    // painted and is not reported.
    assert_eq!(
        uncovered_automatic_composition_span_range([7usize, 48, 70].into_iter(), 5, 60, &(10..48)),
        Some(7..49)
    );
}
