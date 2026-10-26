//! Source coverage at EOB is distinct from the virtual rows of an overlay string.

use super::*;
use neovm_core::emacs_core::print::print_value;
use neovm_core::window::{
    DisplayRowEndSource, DisplayRowSnapshot, FrameId, WindowId, WindowLayoutQueryOutcome,
    WindowLayoutQueryScope,
};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn overlay_eob_fixture(
    text: &str,
    property: &str,
    tail: &str,
    restriction: Option<(i64, i64)>,
    width: u32,
) -> (Context, FrameId, WindowId) {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().expect("buffer").id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .expect("buffer")
        .insert(text);
    let frame = eval
        .frame_manager_mut()
        .create_frame("motion-overlay-eob", width, 240, buffer);
    {
        let frame = eval.frame_manager_mut().get_mut(frame).expect("frame");
        frame.window_system = Some(Value::symbol("neomacs"));
        frame.char_width = 8.0;
        frame.char_height = 16.0;
        frame.font_pixel_size = 16.0;
    }
    let window = eval
        .frame_manager()
        .get(frame)
        .expect("frame")
        .selected_window;
    if let Some((start, end)) = restriction {
        eval.eval_str(&format!("(narrow-to-region {start} {end})"))
            .expect("interior accessible region");
    }
    let tail = print_value(&Value::string(tail));
    eval.eval_str(&format!(
        r#"(progn
          (setq mode-line-format nil header-line-format nil tab-line-format nil
                truncate-lines nil word-wrap nil)
          (let ((overlay (make-overlay (point-max) (point-max))))
            (overlay-put overlay '{property} {tail}))
          (goto-char (point-max)))"#,
    ))
    .expect("EOB overlay");
    (eval, frame, window)
}

fn query_rows(
    eval: &mut Context,
    frame: FrameId,
    window: WindowId,
    start: i64,
    end: i64,
) -> Vec<DisplayRowSnapshot> {
    let mut engine = WindowLayoutQueryEngine::new_without_font_metrics();
    let query = engine
        .query_window_layout(
            eval,
            frame,
            window,
            WindowLayoutQueryScope::Rows {
                start: LispCharPos1::new(start),
                count: NonZeroUsize::new(64).expect("positive row count"),
            },
        )
        .expect("canonical EOB rows");
    assert_eq!(query.end(), LispCharPos1::new(end));
    query.geometry().expect("query geometry").rows.clone()
}

fn install_motion_query(eval: &mut Context) -> Arc<AtomicUsize> {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed_calls = Arc::clone(&calls);
    let mut engine = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        observed_calls.fetch_add(1, Ordering::Relaxed);
        match engine.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    calls
}

#[test]
fn canonical_rows_at_eob_keep_the_overlay_tail_sourceless() {
    let (mut eval, frame, window) = overlay_eob_fixture("abc", "after-string", "\nX", None, 400);
    let rows = query_rows(&mut eval, frame, window, 1, 4);
    assert_eq!(
        rows.len(),
        2,
        "one source row and one virtual row: {rows:?}"
    );
    assert_eq!(rows[0].start_buffer_pos, Some(LispCharPos1::ONE));
    assert_eq!(rows[0].end_buffer_pos, Some(LispCharPos1::new(4)));
    assert_eq!(rows[0].end_source, DisplayRowEndSource::OverlayAfterString);
    // EOB's insertion boundary was emitted before the overlay's newline. The
    // trailing X has Lisp-string provenance, not a later buffer source row.
    assert_eq!(rows[1].start_buffer_pos, None);
    assert_eq!(rows[1].end_buffer_pos, None);
    assert_eq!(rows[1].end_source, DisplayRowEndSource::Buffer);
    assert!(
        rows[1].end_x > rows[1].start_x,
        "the virtual row contains X"
    );
}

#[test]
fn positive_motion_from_eol_settles_at_eob_with_a_virtual_overlay_tail() {
    let (mut eval, _, _) = overlay_eob_fixture("abc", "after-string", "\nX", None, 400);
    let calls = install_motion_query(&mut eval);
    let result = eval
        .eval_str("(let ((noninteractive nil)) (list (vertical-motion 1) (point)))")
        .expect("covered EOB must not signal an out-of-coverage motion origin");
    assert_eq!(print_value(&result), "(0 4)");
    assert!(
        calls.load(Ordering::Relaxed) > 0,
        "motion uses the real row query"
    );
}

const OVERLAY_KINDS: [(&str, DisplayRowEndSource); 2] = [
    ("before-string", DisplayRowEndSource::OverlayBeforeString),
    ("after-string", DisplayRowEndSource::OverlayAfterString),
];

// The accessible text is always "abc\ndef"; narrowing leaves backing text on
// both sides, so confusing accessible EOB with physical EOB cannot pass.
fn two_line_fixture(
    property: &str,
    tail: &str,
    narrowed: bool,
    width: u32,
) -> (Context, FrameId, WindowId, i64) {
    let (text, restriction, start) = if narrowed {
        ("PRE\nabc\ndef\nPOST", Some((5, 12)), 5)
    } else {
        ("abc\ndef", None, 1)
    };
    let (eval, frame, window) = overlay_eob_fixture(text, property, tail, restriction, width);
    (eval, frame, window, start)
}

fn assert_two_line_motion(eval: &mut Context, start: i64, following_rows: i64, case: &str) {
    let calls = install_motion_query(eval);
    let last_start = start + 4;
    let end = start + 7;
    for origin in [start, last_start, end] {
        for lines in [-10, -2, -1, 0, 1, 2, 10] {
            let (moved, target) = if lines < 0 {
                (if origin == start { 0 } else { -1 }, start)
            } else if lines == 0 {
                (0, if origin == start { start } else { last_start })
            } else if origin == start && lines == 1 {
                (1, last_start)
            } else if origin == start {
                (1 + following_rows, end)
            } else if origin == end {
                (0, end)
            } else {
                (following_rows, end)
            };
            let previous_calls = calls.load(Ordering::Relaxed);
            let result = eval
                .eval_str(&format!(
                    "(let ((noninteractive nil))
                       (goto-char {origin})
                       (list (vertical-motion {lines}) (point) (point-min) (point-max)))"
                ))
                .unwrap_or_else(|error| {
                    panic!("{case}, origin={origin}, lines={lines}: {error:?}")
                });
            assert_eq!(
                print_value(&result),
                format!("({moved} {target} {start} {end})"),
                "{case}, origin={origin}, lines={lines}"
            );
            assert!(
                calls.load(Ordering::Relaxed) > previous_calls,
                "{case}: every motion must use the real row query"
            );
        }
    }
}

#[test]
fn multiline_eob_overlay_rows_preserve_repeated_anchors_and_a_sourceless_tail() {
    for (property, end_source) in OVERLAY_KINDS {
        for tail in ["\nX", "\nX\nY", "\nX\n"] {
            for narrowed in [false, true] {
                let (mut eval, frame, window, start) =
                    two_line_fixture(property, tail, narrowed, 400);
                let end = start + 7;
                let rows = query_rows(&mut eval, frame, window, start, end);
                let breaks = tail.bytes().filter(|&byte| byte == b'\n').count();
                let case = format!("{property}, tail={tail:?}, narrowed={narrowed}");
                // The row query omits an empty, sourceless row after a final
                // newline, but publishes a visible tail such as X or Y.
                let has_tail = !tail.ends_with('\n');
                assert_eq!(
                    rows.len(),
                    breaks + 1 + usize::from(has_tail),
                    "{case}: {rows:?}"
                );
                assert_eq!(
                    rows[0].start_buffer_pos,
                    Some(LispCharPos1::new(start)),
                    "{case}"
                );
                assert_eq!(
                    rows[0].end_buffer_pos,
                    Some(LispCharPos1::new(start + 3)),
                    "{case}"
                );
                assert_eq!(rows[0].end_source, DisplayRowEndSource::Buffer, "{case}");
                for (index, row) in rows[1..=breaks].iter().enumerate() {
                    let row_start = if index == 0 { start + 4 } else { end };
                    assert_eq!(
                        row.start_buffer_pos,
                        Some(LispCharPos1::new(row_start)),
                        "{case}"
                    );
                    assert_eq!(row.end_buffer_pos, Some(LispCharPos1::new(end)), "{case}");
                    assert_eq!(row.end_source, end_source, "{case}");
                }
                if has_tail {
                    let last = rows.last().expect("virtual tail row");
                    assert_eq!(last.start_buffer_pos, None, "{case}");
                    assert_eq!(last.end_buffer_pos, None, "{case}");
                    assert_eq!(last.end_source, DisplayRowEndSource::Buffer, "{case}");
                    assert!(last.end_x > last.start_x, "{case}: visible overlay tail");
                }
            }
        }
    }
}

#[test]
fn multiline_eob_overlay_motion_preserves_physical_distance_and_source_landings() {
    for (property, _) in OVERLAY_KINDS {
        // Initialized GNU's display iterator returns the physical distance
        // through the tail when motion starts before EOB, unlike a cold batch
        // process with `noninteractive` merely rebound to nil.
        for (tail, following_rows) in [("\nX", 1), ("\nX\nY", 2), ("\nX\n", 2)] {
            for narrowed in [false, true] {
                let (mut eval, _, _, start) = two_line_fixture(property, tail, narrowed, 400);
                assert_two_line_motion(
                    &mut eval,
                    start,
                    following_rows,
                    &format!("{property}, tail={tail:?}, narrowed={narrowed}"),
                );
            }
        }
    }
}

#[test]
fn overlong_eob_overlay_rows_preserve_provenance_and_physical_motion_distance() {
    let tail = format!("\n{}\nY", "X".repeat(80));
    for (property, end_source) in OVERLAY_KINDS {
        for truncate in [false, true] {
            let (mut eval, frame, window, start) = two_line_fixture(property, &tail, false, 96);
            eval.eval_str(if truncate {
                "(setq truncate-lines t)"
            } else {
                "(setq truncate-lines nil)"
            })
            .expect("wrapping policy");
            let rows = query_rows(&mut eval, frame, window, start, start + 7);
            let case = format!("{property}, truncate={truncate}");
            assert!(
                rows.len() >= 4,
                "{case}: overlay continues after the overlong line: {rows:?}"
            );
            assert_eq!(
                rows[1].start_buffer_pos,
                Some(LispCharPos1::new(start + 4)),
                "{case}"
            );
            assert_eq!(
                rows[1].end_buffer_pos,
                Some(LispCharPos1::new(start + 7)),
                "{case}"
            );
            assert_eq!(rows[1].end_source, end_source, "{case}");
            for row in &rows[2..rows.len() - 1] {
                assert_eq!(
                    row.start_buffer_pos,
                    Some(LispCharPos1::new(start + 7)),
                    "{case}"
                );
                assert_eq!(
                    row.end_buffer_pos,
                    Some(LispCharPos1::new(start + 7)),
                    "{case}"
                );
                assert_eq!(row.end_source, end_source, "{case}");
            }
            let last = rows.last().expect("trailing Y row");
            assert_eq!(last.start_buffer_pos, None, "{case}");
            assert_eq!(last.end_buffer_pos, None, "{case}");
            assert_eq!(last.end_source, DisplayRowEndSource::Buffer, "{case}");
            assert!(last.end_x > last.start_x, "{case}: Y is rendered");
            // This fontless row producer test checks transport against its
            // independently published geometry. Live TUI/GUI comparisons
            // establish the actual GNU distance with initialized displays.
            let following_rows = last.row - rows[1].row;
            assert_two_line_motion(&mut eval, start, following_rows, &case);
        }
    }
}

#[test]
fn positive_motion_through_a_large_eob_overlay_expands_measured_coverage() {
    let tail = "\nX".repeat(70);
    for (property, _) in OVERLAY_KINDS {
        let (mut eval, _, _) = overlay_eob_fixture("abc", property, &tail, None, 400);
        let calls = install_motion_query(&mut eval);
        let result = eval
            .eval_str(
                "(let ((noninteractive nil)) (goto-char 1) (list (vertical-motion 1) (point)))",
            )
            .expect("the complete tail is measured beyond the initial row budget");
        assert_eq!(print_value(&result), "(70 4)", "{property}");
        assert!(
            calls.load(Ordering::Relaxed) > 1,
            "{property}: partial coverage must grow"
        );
    }
}

#[test]
fn truncated_source_at_the_right_edge_still_completes_its_eob_overlay() {
    use neovm_core::window::EobOverlayMotionCoverage;

    for (property, length) in OVERLAY_KINDS
        .into_iter()
        .flat_map(|(property, _)| (0..=20).chain([100]).map(move |length| (property, length)))
    {
        let (mut eval, frame, window) =
            overlay_eob_fixture(&"a".repeat(length), property, "\nX", None, 80);
        eval.eval_str("(setq truncate-lines t)").unwrap();
        let mut engine = WindowLayoutQueryEngine::new_without_font_metrics();
        let query = engine
            .query_window_layout(
                &mut eval,
                frame,
                window,
                WindowLayoutQueryScope::Rows {
                    start: LispCharPos1::ONE,
                    count: NonZeroUsize::new(64).unwrap(),
                },
            )
            .unwrap();
        let snapshot = query.geometry().unwrap();
        assert!(
            matches!(
                snapshot.eob_overlay_motion,
                EobOverlayMotionCoverage::Complete(_)
            ),
            "{property}, length={length}: finite truncated strings must be consumed: {:?}",
            snapshot.eob_overlay_motion
        );
    }
}

#[test]
fn eob_overlay_completion_waits_for_every_ordered_string() {
    use neovm_core::window::EobOverlayMotionCoverage;

    let (mut eval, frame, window) =
        overlay_eob_fixture("abc", "after-string", &"\nX".repeat(70), None, 400);
    eval.eval_str(
        "(let ((overlay (make-overlay (point-max) (point-max))))
           (overlay-put overlay 'before-string \"\\nA\"))",
    )
    .unwrap();
    let mut engine = WindowLayoutQueryEngine::new_without_font_metrics();
    let partial = engine
        .query_window_layout(
            &mut eval,
            frame,
            window,
            WindowLayoutQueryScope::Rows {
                start: LispCharPos1::ONE,
                count: NonZeroUsize::new(4).unwrap(),
            },
        )
        .unwrap();
    assert_eq!(
        partial.geometry().unwrap().eob_overlay_motion,
        EobOverlayMotionCoverage::Incomplete {
            anchor: LispCharPos1::new(4)
        },
        "consuming one string cannot certify the whole EOB stream"
    );
    let calls = install_motion_query(&mut eval);
    let result = eval
        .eval_str("(let ((noninteractive nil)) (goto-char 1) (list (vertical-motion 1) (point)))")
        .unwrap();
    assert_eq!(print_value(&result), "(71 4)");
    assert!(calls.load(Ordering::Relaxed) > 1);
}
