//! GNU trailing-whitespace classification must inspect source, not stretch shape.
use super::*;

/// Lay out real source and optional display replacements, returning the buffer
/// positions painted with the trailing-whitespace face. Default point is zero
/// so positive controls do not accidentally suppress their own endpoint.
fn highlighted(text: &str, replacement: Option<&str>, point: Option<usize>) -> Vec<usize> {
    let trace = layout_trace_with_buffer_setup(text, 800, 240, |buffer, _, _| {
        buffer.set_buffer_local("show-trailing-whitespace", Value::T);
        if let Some(replacement) = replacement {
            let display = if replacement == "stretch" {
                table_bar()
            } else {
                Value::string(replacement)
            };
            let bar = text.rfind('|').unwrap();
            assert!(buffer.put_text_property(bar, bar + 1, Value::symbol("display"), display));
        }
        {
            let point = point.unwrap_or(0);
            buffer.set_point_anchor(neovm_core::buffer::TextPositionAnchor::new(
                neovm_core::buffer::CharPos0::new(point),
                neovm_core::buffer::EmacsBytePos::new(point),
            ));
        }
    });
    trace
        .matrix_rows
        .iter()
        .filter(|row| row.role == GlyphRowRole::Text && !row.mode_line)
        .flat_map(|row| row.glyph_areas[1].iter())
        .filter(|glyph| glyph.face.contains("trailing-whitespace"))
        .map(|glyph| glyph.charpos)
        .collect()
}

/// Source classification uses character coordinates, accessible bounds and GNU
/// CR/ZV termination, rather than treating missing bytes or display ends as EOF.
#[test]
fn trailing_source_predicate_respects_cr_zv_narrowing_and_character_coordinates() {
    use crate::display_row::trailing_whitespace::buffer_trailing_whitespace_p;
    use neovm_core::buffer::CharPos0;
    for (text, narrowed, from, point, expected) in [
        ("é  \rhidden\n", false, 1, 0, true),
        ("é  \rhidden\n", false, 1, 3, false),
        ("é  \rhidden\n", false, 0, 0, false),
        ("x  ", false, 1, 0, true),
        ("x  ", false, 1, 3, false),
        ("x  hidden", true, 1, 0, true),
        ("x  hidden", false, 1, 0, false),
    ] {
        let mut eval = Context::new();
        eval.buffer_manager_mut()
            .current_buffer_mut()
            .unwrap()
            .insert(text);
        if narrowed {
            eval.eval_str("(narrow-to-region 1 4)").unwrap();
        }
        let buffer = eval.buffer_manager().current_buffer().unwrap();
        assert_eq!(
            buffer_trailing_whitespace_p(buffer, point, CharPos0::new(from)),
            expected,
            "text={text:?} narrowed={narrowed} from={from} point={point}"
        );
    }
}

/// Both cursor-only and scrolling reuse must reproduce fresh endpoint-dependent
/// faces after point leaves or enters a whitespace run.
#[test]
fn trailing_whitespace_point_motion_keeps_reused_rows_equal_to_fresh() {
    let positions = |trace: BackendLayoutTrace| -> Vec<usize> {
        trace
            .matrix_rows
            .iter()
            .filter(|row| row.role == GlyphRowRole::Text && !row.mode_line)
            .flat_map(|row| row.glyph_areas[1].iter())
            .filter(|glyph| glyph.face.contains("trailing-whitespace"))
            .map(|glyph| glyph.charpos)
            .collect()
    };
    for scroll in [false, true] {
        let (mut eval, frame, _, _) = incr_editing_frame(&"x  \n".repeat(30), 800, 240);
        eval.buffer_manager_mut()
            .current_buffer_mut()
            .unwrap()
            .set_buffer_local("show-trailing-whitespace", Value::T);
        eval.eval_str("(goto-char 1)").unwrap();
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        engine.layout_frame_rust(&mut eval, frame);
        for point in if scroll { [44, 41] } else { [4, 1] } {
            eval.eval_str(&format!("(set-window-point (selected-window) {point})"))
                .unwrap();
            if scroll {
                eval.eval_str("(set-window-start (selected-window) 5 t)")
                    .unwrap();
            }
            engine.layout_frame_rust(&mut eval, frame);
            let actual = positions(selected_window_layout_trace(&eval, &engine, frame));
            let mut fresh = crate::font::metrics::test_catalog::layout_engine();
            fresh.layout_frame_rust(&mut eval, frame);
            let expected = positions(selected_window_layout_trace(&eval, &fresh, frame));
            assert_eq!(actual, expected, "scroll={scroll} point={point}");
        }
    }
}

/// Edits can invalidate an earlier suppressed run even if point's numeric
/// position stays unchanged; ordinary non-highlighting edits still reuse rows.
#[test]
fn trailing_whitespace_edit_replay_does_not_retain_point_dependent_faces() {
    let positions = |trace: BackendLayoutTrace| -> Vec<usize> {
        trace
            .matrix_rows
            .iter()
            .filter(|row| row.role == GlyphRowRole::Text && !row.mode_line)
            .flat_map(|row| row.glyph_areas[1].iter())
            .filter(|glyph| glyph.face.contains("trailing-whitespace"))
            .map(|glyph| glyph.charpos)
            .collect()
    };
    for show in [false, true] {
        for moved in [false, true] {
            let (mut eval, frame, _, _) = incr_editing_frame(&"x  \n".repeat(30), 800, 240);
            eval.buffer_manager_mut()
                .current_buffer_mut()
                .unwrap()
                .set_buffer_local(
                    "show-trailing-whitespace",
                    if show { Value::T } else { Value::NIL },
                );
            eval.eval_str(if moved {
                "(goto-char 4)"
            } else {
                "(goto-char 12)"
            })
            .unwrap();
            let mut engine = crate::font::metrics::test_catalog::layout_engine();
            engine.layout_frame_rust(&mut eval, frame);
            // Either leave the old endpoint and edit below it, or keep the same
            // numeric point while an insertion above shifts its former row.
            eval.eval_str(if moved {
                "(progn (goto-char 21) (insert \"z\"))"
            } else {
                "(progn (goto-char 1) (insert \"z\") (goto-char 12))"
            })
            .unwrap();
            engine.layout_frame_rust(&mut eval, frame);
            let actual = positions(selected_window_layout_trace(&eval, &engine, frame));
            let mut fresh = crate::font::metrics::test_catalog::layout_engine();
            fresh.layout_frame_rust(&mut eval, frame);
            let expected = positions(selected_window_layout_trace(&eval, &fresh, frame));
            assert_eq!(actual, expected, "show={show} moved={moved}");
            assert_eq!(
                engine.last_layout_stats().edit_windows,
                usize::from(!show),
                "show={show} moved={moved}"
            );
        }
    }
}

/// Model org-modern's visible border without depending on third-party packages.
fn table_bar() -> Value {
    Value::list(vec![
        Value::symbol("space"),
        Value::keyword("width"),
        Value::list(vec![Value::fixnum(3)]),
    ])
}

/// Display appearance is not source whitespace; string-owned glyphs block the
/// backward scan rather than behaving like nil-object synthetic padding.
#[test]
fn display_stretches_do_not_turn_org_table_padding_into_trailing_whitespace() {
    for text in ["| alpha    |\n", "| café     |\n", "x  |\n"] {
        assert!(
            highlighted(text, Some("stretch"), None).is_empty(),
            "{text:?}"
        );
    }
    // Unlike nil-object end padding, a displayed string must block the scan.
    for display in [" ", "word"] {
        assert!(
            highlighted("x  |\n", Some(display), None).is_empty(),
            "{display:?}"
        );
    }
}

/// Genuine spaces/tabs still highlight, including a preceding displayed stretch
/// licensed by GNU's single last-candidate source check.
#[test]
fn actual_trailing_spaces_and_tabs_keep_gnu_highlighting() {
    assert_eq!(highlighted("x  \n", None, None), vec![1, 2]);
    assert_eq!(highlighted("x \t\n", None, None), vec![1, 2]);
    // GNU checks the LAST candidate's source, then paints the whole glyph run.
    // A display stretch before genuine trailing spaces still belongs to it.
    assert_eq!(highlighted("x|  \n", Some("stretch"), None), vec![1, 2, 3]);
}

/// Only point at the end of the complete source run suppresses highlighting;
/// point inside that run must leave its face intact.
#[test]
fn trailing_whitespace_is_suppressed_when_point_is_at_its_endpoint() {
    assert!(highlighted("x  \n", None, Some(3)).is_empty());
    assert_eq!(highlighted("x  \n", None, Some(2)), vec![1, 2]);
}
