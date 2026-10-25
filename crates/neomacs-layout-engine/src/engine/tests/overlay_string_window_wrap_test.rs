use super::*;
use crate::types::LineWrapMode;

fn overlay_rows(payload: &str, wrap_mode: LineWrapMode) -> Vec<String> {
    let payload = payload.to_owned();
    let (_eval, _buffer, _frame, rows, _cw, _ch) =
        layout_main_text_rows_with("Z\n", move |eval, buffer| {
            eval.buffer_manager_mut().set_current(buffer);
            eval.eval_str(match wrap_mode {
                LineWrapMode::Truncate => "(setq truncate-lines t)",
                LineWrapMode::Wrap => "(setq truncate-lines nil)",
            })
            .expect("window wrap policy");
            eval.eval_form(Value::list(vec![
                Value::symbol("overlay-put"),
                Value::list(vec![
                    Value::symbol("make-overlay"),
                    Value::fixnum(1),
                    Value::fixnum(2),
                ]),
                Value::list(vec![Value::symbol("quote"), Value::symbol("before-string")]),
                Value::string(payload),
            ]))
            .expect("overlay insertion source");
        });
    rows.iter()
        .map(|row| glyphs_logical_text(&row.glyphs[1]))
        .collect()
}

#[test]
fn wrapping_overlay_string_keeps_its_ascii_tail() {
    let payload = "0123456789".repeat(12);
    let rows = overlay_rows(&payload, LineWrapMode::Wrap);
    let actual: String = rows
        .iter()
        .flat_map(|row| row.chars())
        .filter(char::is_ascii_digit)
        .collect();
    assert_eq!(
        actual, payload,
        "GNU WINDOW_WRAP continues the same overlay string instead of discarding its clipped tail: {rows:?}"
    );
    assert!(
        rows[0].ends_with('\\'),
        "the broken TTY row needs its continuation glyph: {rows:?}"
    );
}

#[test]
fn wrapping_overlay_string_keeps_its_wide_tail() {
    let payload = "日本語".repeat(20);
    let rows = overlay_rows(&payload, LineWrapMode::Wrap);
    let actual: String = rows
        .iter()
        .flat_map(|row| row.chars())
        .filter(|ch| matches!(ch, '日' | '本' | '語'))
        .collect();
    assert_eq!(
        actual, payload,
        "a wide overlay element continues on the next display row: {rows:?}"
    );
    assert!(
        rows[0].ends_with('\\'),
        "the TTY continuation marker must survive the wide cut: {rows:?}"
    );
}

#[test]
fn truncating_overlay_string_still_discards_its_tail() {
    let payload = "0123456789".repeat(12);
    let rows = overlay_rows(&payload, LineWrapMode::Truncate);
    let actual: String = rows
        .iter()
        .flat_map(|row| row.chars())
        .filter(char::is_ascii_digit)
        .collect();
    assert!(
        actual.len() < payload.len(),
        "GNU TRUNCATE must not manufacture a continuation row: {rows:?}"
    );
}

fn install_before_string(eval: &mut Context, payload: Value) {
    eval.eval_form(Value::list(vec![
        Value::symbol("overlay-put"),
        Value::list(vec![
            Value::symbol("make-overlay"),
            Value::fixnum(1),
            Value::fixnum(1),
        ]),
        Value::list(vec![Value::symbol("quote"), Value::symbol("before-string")]),
        payload,
    ]))
    .expect("before-string at the source anchor");
}

#[test]
fn wrapping_overlay_string_emits_wrap_prefix_before_its_retained_tail() {
    let payload = "0123456789".repeat(12);
    let expected = payload.clone();
    let (_, _, _, rows, _, _) =
        layout_main_text_rows_with_width("Z\n", 632, move |eval, buffer| {
            let buffer = eval.buffer_manager_mut().get_mut(buffer).expect("buffer");
            buffer.set_buffer_local("wrap-prefix", Value::string(">>"));
            install_before_string(eval, Value::string(payload));
        });
    let text: Vec<_> = rows
        .iter()
        .map(|row| glyphs_logical_text(&row.glyphs[1]))
        .collect();
    assert!(
        !text[0].starts_with(">>"),
        "first row uses no wrap prefix: {text:?}"
    );
    assert!(
        text[1].starts_with(">>"),
        "prefix precedes the pending string tail: {text:?}"
    );
    assert_eq!(
        text[1].matches(">>").count(),
        1,
        "outer buffer prelude must not repeat the consumed prefix"
    );
    let digits: String = text
        .iter()
        .flat_map(|row| row.chars())
        .filter(char::is_ascii_digit)
        .collect();
    assert_eq!(digits, expected);
    assert!(
        text[1].trim_end().ends_with('Z'),
        "following buffer source remains on the continued row: {text:?}"
    );
}

#[test]
fn wrapping_overlay_and_following_buffer_tab_share_the_physical_line_grid() {
    let (_, _, _, rows, cw, _) = layout_main_text_rows_with_width("\tZ\n", 632, |eval, buffer| {
        eval.buffer_manager_mut()
            .get_mut(buffer)
            .expect("buffer")
            .set_buffer_local("wrap-prefix", Value::string(">\t"));
        install_before_string(eval, Value::string("x".repeat(100)));
    });
    let row = rows.get(1).expect("continued overlay row");
    let glyphs = &row.glyphs[GlyphArea::Text.index()];
    let prefix_tab = glyphs
        .iter()
        .find(|glyph| {
            matches!(glyph.glyph_type, GlyphType::Stretch { .. })
                && matches!(glyph.provenance, GlyphProvenance::Str { .. })
        })
        .expect("prefix tab");
    assert_eq!(
        prefix_tab.pixel_width,
        7.0 * cw,
        "prefix TAB uses screen column one"
    );
    let buffer_tab = glyphs
        .iter()
        .find(|glyph| {
            matches!(glyph.glyph_type, GlyphType::Stretch { .. })
                && glyph.provenance == GlyphProvenance::buffer(0)
        })
        .expect("following buffer tab");
    assert_eq!(
        buffer_tab.pixel_width,
        4.0 * cw,
        "100 overlay columns advance to physical column 104; prefix width cancels"
    );
    assert_eq!(
        glyphs
            .iter()
            .filter(|glyph| glyph.provenance == GlyphProvenance::buffer(1))
            .count(),
        1,
        "following buffer character has one source slot"
    );
}

#[test]
fn overlay_wrap_prefix_alone_does_not_keep_retrying_an_oversized_pending_glyph() {
    let (_, _, _, rows, _, _) =
        layout_main_text_rows_with_width("Z\nNEXT\n", 632, |eval, buffer| {
            eval.buffer_manager_mut()
                .get_mut(buffer)
                .expect("buffer")
                .set_buffer_local("wrap-prefix", Value::string("p".repeat(77)));
            install_before_string(eval, Value::string("日".repeat(60)));
        });
    let text: Vec<_> = rows
        .iter()
        .map(|row| glyphs_logical_text(&row.glyphs[1]))
        .collect();
    assert_eq!(
        text.iter().filter(|row| row.starts_with('p')).count(),
        1,
        "a prefix is decoration, not newly consumed source: {text:?}"
    );
    assert!(
        text.iter().any(|row| row.contains("NEXT")),
        "a refused wide tail cannot exhaust the viewport with prefix-only rows: {text:?}"
    );
}

#[test]
fn end_of_buffer_overlay_wrap_uses_the_same_prefix_transition() {
    let (_, _, _, rows, _, _) = layout_main_text_rows_with_width("", 632, |eval, buffer| {
        eval.buffer_manager_mut()
            .get_mut(buffer)
            .expect("buffer")
            .set_buffer_local("wrap-prefix", Value::string(">>"));
        install_before_string(eval, Value::string("x".repeat(100)));
    });
    let text: Vec<_> = rows
        .iter()
        .map(|row| glyphs_logical_text(&row.glyphs[1]))
        .collect();
    assert!(
        text[1].starts_with(">>"),
        "EOB strings use the common continuation prelude: {text:?}"
    );
    assert_eq!(
        text.iter()
            .flat_map(|row| row.chars())
            .filter(|ch| *ch == 'x')
            .count(),
        100
    );
}

#[test]
fn overlay_pending_string_wrap_prefix_overrides_the_buffer_default() {
    let (_, _, _, rows, _, _) = layout_main_text_rows_with_width("Z\n", 632, |eval, buffer| {
        eval.buffer_manager_mut()
            .get_mut(buffer)
            .expect("buffer")
            .set_buffer_local("wrap-prefix", Value::string("BUFFER"));
        let payload = eval
            .eval_str("(propertize (make-string 100 ?x) 'wrap-prefix \"STRING\")")
            .expect("string prefix property");
        install_before_string(eval, payload);
    });
    let text: Vec<_> = rows
        .iter()
        .map(|row| glyphs_logical_text(&row.glyphs[1]))
        .collect();
    assert!(
        text[1].starts_with("STRING"),
        "pending string property wins: {text:?}"
    );
    assert!(!text[1].contains("BUFFER"));
}

#[test]
fn overlay_visual_wrap_keeps_its_unconsumed_buffer_anchor() {
    let (eval, _, frame_id, rows, _, _) = layout_main_text_rows_with("Z\n", |eval, _| {
        install_before_string(eval, Value::string("x".repeat(120)));
    });
    assert!(rows[0].continued, "the string crossed a visual row");
    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(frame.selected_window)
        .expect("window snapshot");
    let row = &snapshot.rows[0];
    assert_eq!(
        row.end_source,
        neovm_core::window::DisplayRowEndSource::OverlayBeforeString
    );
    assert_eq!(row.end_buffer_pos, Some(LispCharPos1::ONE));
    assert_eq!(
        crate::display_row::walk_state::row_next_window_start_charpos(row),
        Some(0),
        "continuing a string must not invent consumption of its buffer anchor"
    );
}

#[test]
fn wrapping_overlay_string_preserves_automatic_combining_clusters() {
    let payload = format!("truncation-{}", "a\u{301}".repeat(34));
    let (_, _, _, rows, _, _) =
        layout_main_text_rows_with_width("Z\n", 320, move |eval, _buffer| {
            eval.eval_str(
                r#"(setq auto-composition-mode t
                         auto-composition-function 'auto-compose-chars
                         composition-function-table (make-char-table nil))
                    (aset composition-function-table #x301
                          (list (vector ".́" 1 'font-shape-gstring)))"#,
            )
            .expect("explicit automatic composition rule");
            install_before_string(eval, Value::string(payload));
        });
    let composed_marks = rows
        .iter()
        .flat_map(|row| row.glyphs[1].iter())
        .filter_map(|glyph| match &glyph.glyph_type {
            neomacs_display_protocol::glyph_matrix::GlyphType::Composite { text }
            | neomacs_display_protocol::glyph_matrix::GlyphType::AutomaticComposite {
                text, ..
            } => Some(text),
            _ => None,
        })
        .map(|text| text.chars().filter(|ch| *ch == '\u{301}').count())
        .sum::<usize>();
    assert_eq!(
        composed_marks, 34,
        "TTY automatic compositions must retain the mark in its base cell across overlay wraps"
    );
}

fn install_default_combining_rules(eval: &mut Context) {
    eval.eval_str(
        r#"(setq auto-composition-mode t
                 auto-composition-function 'auto-compose-chars
                 composition-function-table (make-char-table nil))
            (aset composition-function-table #x301
                  (list (vector "\\c.\\c^+" 1 'compose-gstring-for-graphic)
                        (vector nil 0 'compose-gstring-for-graphic)))
            (let ((table (make-category-table)))
              (define-category ?. "Graphic base" table)
              (define-category ?^ "Nonspacing mark" table)
              (modify-category-entry ?a ?. table)
              (modify-category-entry #x301 ?^ table)
              (set-category-table table))"#,
    )
    .expect("normal combining rule and its explicit category inputs");
}

fn assert_all_overlay_combining_pairs_are_automatic(rows: &[MatrixRow], expected: usize) {
    let pairs: Vec<_> = rows
        .iter()
        .flat_map(|row| row.glyphs[GlyphArea::Text.index()].iter())
        .filter_map(|glyph| match &glyph.glyph_type {
            GlyphType::AutomaticComposite { text, terminal } if text.contains('\u{301}') => {
                Some((text.as_ref(), terminal.width_cols))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        pairs.len(),
        expected,
        "every base+mark pair remains an automatic composition, including across property boundaries: {pairs:?}"
    );
    assert!(
        pairs
            .iter()
            .all(|(text, width)| *text == "a\u{301}" && *width == 1),
        "the default rule must preserve each mark in its own one-column base cell: {pairs:?}"
    );
}

#[test]
fn wrapping_overlay_string_preserves_default_combining_rules() {
    let payload = format!("truncation-{}", "a\u{301}".repeat(32));
    let (_, _, _, rows, _, _) =
        layout_main_text_rows_with_width("Z\n", 320, move |eval, buffer| {
            eval.buffer_manager_mut().set_current(buffer);
            install_default_combining_rules(eval);
            install_before_string(eval, Value::string(payload));
        });
    assert_all_overlay_combining_pairs_are_automatic(&rows, 32);
}

#[test]
fn wrapping_overlay_string_preserves_default_composition_across_vertico_face_intervals() {
    // Exact bounded all-candidate string and property intervals observed in
    // both normally loaded peers. The last candidate's first base has the
    // first-difference face; its U+0301 starts the following property interval.
    let payload = format!(
        " \ntruncation-日本語日本語日本語日本語日本語\ntruncation-abcdefghijklmnopqrstuvwxyz-abcdefghijklmnopqrstuvwxyz\ntruncation-{}\n",
        "a\u{301}".repeat(32)
    );
    assert_eq!(payload.chars().count(), 170);
    let (_, _, _, rows, _, _) = layout_main_text_rows_with_width(
        "Z\n",
        320,
        move |eval, buffer| {
            eval.buffer_manager_mut().set_current(buffer);
            install_default_combining_rules(eval);
            eval.eval_form(Value::list(vec![
                Value::symbol("setq"),
                Value::symbol("overlay-combining-candidate"),
                Value::string(payload),
            ]))
            .expect("actual formatted candidate object");
            eval.eval_str(
                r#"(put-text-property 0 1 'cursor t overlay-combining-candidate)
                    (put-text-property 2 13 'face '(completions-common-part vertico-current) overlay-combining-candidate)
                    (put-text-property 13 14 'face '(completions-first-difference vertico-current) overlay-combining-candidate)
                    (put-text-property 14 29 'face 'vertico-current overlay-combining-candidate)
                    (put-text-property 29 40 'face 'completions-common-part overlay-combining-candidate)
                    (put-text-property 40 41 'face 'completions-first-difference overlay-combining-candidate)
                    (put-text-property 94 105 'face 'completions-common-part overlay-combining-candidate)
                    (put-text-property 105 106 'face 'completions-first-difference overlay-combining-candidate)"#,
            )
            .expect("actual Vertico property boundaries");
            let candidate = eval
                .eval_str("overlay-combining-candidate")
                .expect("formatted candidate");
            install_before_string(eval, candidate);
        },
    );
    assert_all_overlay_combining_pairs_are_automatic(&rows, 32);
}

#[test]
fn wrapping_overlay_string_composes_after_leading_newline() {
    let payload = format!(" \ntruncation-{}\n", "a\u{301}".repeat(32));
    let (_, _, _, rows, _, _) =
        layout_main_text_rows_with_width("Z\n", 320, move |eval, buffer| {
            eval.buffer_manager_mut().set_current(buffer);
            install_default_combining_rules(eval);
            install_before_string(eval, Value::string(payload));
        });
    assert_all_overlay_combining_pairs_are_automatic(&rows, 32);
}

#[test]
fn automatic_overlay_composition_preserves_display_replacement_boundaries() {
    let (_, _, _, rows, _, _) = layout_main_text_rows_with_width("Z\n", 320, |eval, buffer| {
        eval.buffer_manager_mut().set_current(buffer);
        install_default_combining_rules(eval);
        let candidate = eval
            .eval_str(
                r#"(let ((candidate "á"))
                         (put-text-property 1 2 'display "Q" candidate)
                         candidate)"#,
            )
            .expect("display replacement on the mark");
        install_before_string(eval, candidate);
    });
    let text: String = rows
        .iter()
        .map(|row| glyphs_logical_text(&row.glyphs[GlyphArea::Text.index()]))
        .collect();
    assert!(
        text.contains("aQZ"),
        "a composition must not consume the mark's higher-priority display replacement: {text:?}"
    );
    assert!(
        rows.iter().all(|row| row.glyphs[GlyphArea::Text.index()]
            .iter()
            .all(|glyph| !matches!(&glyph.glyph_type, GlyphType::AutomaticComposite { text, .. } if text.contains('\u{301}')))),
        "the replaced mark cannot also be emitted as an automatic extender"
    );
}
