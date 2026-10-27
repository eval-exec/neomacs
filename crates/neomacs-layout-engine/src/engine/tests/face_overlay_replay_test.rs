use super::*;

fn paint(state: &neomacs_display_protocol::glyph_matrix::FrameDisplayState) -> Vec<String> {
    let mut result = Vec::new();
    state.for_each_glyph(|mut glyph| {
        let face = match &mut glyph {
            FrameGlyph::Char { face_id, .. }
            | FrameGlyph::Stretch { face_id, .. }
            | FrameGlyph::Image { face_id, .. }
            | FrameGlyph::Video { face_id, .. }
            | FrameGlyph::Xwidget { face_id, .. }
            | FrameGlyph::Surface { face_id, .. }
            | FrameGlyph::FringeBitmap { face_id, .. } => {
                let mut face = state.faces.get(face_id).cloned();
                *face_id = FaceId::new(0);
                if let Some(face) = face.as_mut() {
                    face.id = FaceId::new(0);
                    face.default_resolved_font_id = None;
                }
                face
            }
            _ => None,
        };
        result.push(format!("{glyph:?} {face:?}"));
    });
    result
}

fn exercise(setup: &str, forms: &[(&str, usize)]) {
    // Non-ASCII text and fractional line spacing are the session-list case;
    // leaving bidi direction automatic exercises unchanged paragraph input.
    let text = "session 会话 abc אבג\tready\n".repeat(80);
    let (mut eval, frame, _, _) = incr_editing_frame(&text, 800, 600);
    realize_test_gui_frame(&mut eval, frame);
    eval.eval_str(
        r##"(progn
        (setq line-spacing 0.2 truncate-lines t face-overlay-evals 0)
        (setq char-property-alias-alist '((face font-lock-face)))
        (setq mode-line-format '((:eval (progn
            (setq face-overlay-evals (1+ face-overlay-evals)) "MODE"))))
        (goto-char (point-min)) (forward-line 5)
        (setq moving-highlight (make-overlay (point) (line-beginning-position 2)))
        (overlay-put moving-highlight 'priority -50)
        (overlay-put moving-highlight 'face '(:background "#123456" :extend t)))"##,
    )
    .unwrap();
    eval.eval_str(setup).unwrap();
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    for (form, minimum_reused_rows) in forms {
        eval.eval_str("(setq face-overlay-evals 0)").unwrap();
        eval.eval_str(form).unwrap();
        engine.layout_frame_rust(&mut eval, frame);
        let stats = engine.last_layout_stats();
        if *minimum_reused_rows > 0 {
            assert!(
                stats.edit_windows > 0
                    && stats.reused_rows + stats.reused_shifted_rows >= *minimum_reused_rows,
                "{setup} => {form}: {stats:?}"
            );
        } else {
            assert_eq!(stats.edit_windows, 0, "{setup} => {form}: {stats:?}");
        }
        assert_eq!(
            eval.eval_str("face-overlay-evals").unwrap().as_fixnum(),
            Some(1),
            "mode-line Lisp still runs on an overlay update"
        );
        assert_matches_full_layout(&mut eval, &mut engine, frame, form);
    }
}

/// Compare the incremental frame against a fresh full layout of the same
/// inputs: every glyph, its resolved face, row geometry, window end and the
/// physical cursor. A replay that reuses a row the new inputs invalidated shows
/// up here.
fn assert_matches_full_layout(
    eval: &mut Context,
    engine: &mut LayoutEngine,
    frame: neovm_core::window::FrameId,
    label: &str,
) {
    let incremental = engine.last_frame_display_state.as_ref().unwrap().clone();
    let trace = selected_window_layout_trace(eval, engine, frame);
    let mut full = LayoutEngine::new();
    full.layout_frame_rust(eval, frame);
    let reference = full.last_frame_display_state.as_ref().unwrap();
    let actual_paint = paint(&incremental);
    let expected_paint = paint(reference);
    assert_eq!(
        actual_paint.len(),
        expected_paint.len(),
        "{label}: glyph count"
    );
    for (index, (actual, expected)) in actual_paint.iter().zip(&expected_paint).enumerate() {
        assert_eq!(actual, expected, "{label}: painted glyph/face {index}");
    }
    assert_eq!(
        trace,
        selected_window_layout_trace(eval, &full, frame),
        "{label}: row geometry, point mapping and window end"
    );
    assert_eq!(
        incremental.phys_cursor, reference.phys_cursor,
        "{label}: cursor"
    );
}

#[test]
fn face_overlay_movement_creation_and_deletion_match_full_layout() {
    exercise(
        "nil",
        &[
            (
                "(progn (forward-line 1) (move-overlay moving-highlight (point) (line-beginning-position 2)))",
                10,
            ),
            (
                "(progn (forward-line -1) (move-overlay moving-highlight (point) (line-beginning-position 2)))",
                10,
            ),
            ("(delete-overlay moving-highlight)", 10),
            (
                "(move-overlay moving-highlight (point) (line-beginning-position 2))",
                10,
            ),
            (
                "(overlay-put moving-highlight 'face '(:foreground \"#abcdef\" :background \"#987654\" :extend t))",
                10,
            ),
        ],
    );
}

#[test]
fn face_overlay_box_and_height_changes_match_full_layout() {
    exercise(
        "nil",
        &[
            (
                "(overlay-put moving-highlight 'face '(:box t :height 1.5 :background \"#123456\"))",
                1,
            ),
            (
                "(progn (forward-line 1) (move-overlay moving-highlight (point) (line-beginning-position 2)))",
                1,
            ),
            (
                "(overlay-put moving-highlight 'face '(:background \"#123456\" :extend t))",
                1,
            ),
        ],
    );
}

#[test]
fn face_overlay_structural_properties_and_simultaneous_edits_decline() {
    for setup in [
        "(overlay-put moving-highlight 'before-string \"prefix\")",
        "(overlay-put moving-highlight 'display \"replacement\")",
        "(overlay-put moving-highlight 'invisible t)",
        "(setq char-property-alias-alist '((mouse-face face)))",
        "(progn (put 'overlay-category 'face 'highlight) (overlay-put moving-highlight 'category 'overlay-category))",
    ] {
        exercise(
            setup,
            &[(
                "(progn (forward-line 1) (move-overlay moving-highlight (point) (line-beginning-position 2)))",
                0,
            )],
        );
    }
    exercise(
        "nil",
        &[(
            "(progn (insert \"new\") (move-overlay moving-highlight (point) (line-beginning-position 2)))",
            0,
        )],
    );
}

#[test]
fn face_overlay_multiple_highlights_repaint_their_combined_coverage() {
    exercise(
        "(progn (setq second-highlight (make-overlay 1 25)) (overlay-put second-highlight 'face '(:background \"#fedcba\")))",
        &[
            (
                "(progn (forward-line 1) (move-overlay moving-highlight (point) (line-beginning-position 2)) (move-overlay second-highlight (point) (line-beginning-position 2)))",
                10,
            ),
            ("(delete-overlay second-highlight)", 10),
        ],
    );
}

#[test]
fn face_overlay_with_session_style_aligned_columns_matches_full_layout() {
    exercise(
        "(progn (overlay-put moving-highlight 'window (selected-window))
           (save-excursion (goto-char (point-min))
             (while (search-forward \"\\t\" nil t)
               (put-text-property (1- (point)) (point) 'display '(space :align-to 45)))))",
        &[
            (
                "(progn (forward-line 1) (move-overlay moving-highlight (point) (line-beginning-position 2)))",
                10,
            ),
            ("(delete-overlay moving-highlight)", 10),
        ],
    );
}

/// jit-lock installs a buffer-local `fontification-functions` in every
/// font-locked buffer -- including the ones where hl-line and show-paren move.
/// The hook's presence alone must not disable the replay; the leaf revalidates
/// the attempt against the post-fontification inputs instead (see
/// `attempt_freshness`).
#[test]
fn face_overlay_replay_works_with_a_fontification_hook() {
    let text = "session 会话 abc אבג\tready\n".repeat(80);
    let (mut eval, frame, _, _) = incr_editing_frame(&text, 800, 600);
    realize_test_gui_frame(&mut eval, frame);
    eval.eval_str(
        r##"(progn
        (setq line-spacing 0.2 truncate-lines t
              char-property-alias-alist '((face font-lock-face))
              mode-line-format "MODE"
              fontify-calls 0)
        (goto-char (point-min)) (forward-line 5)
        (setq moving-highlight (make-overlay (point) (line-beginning-position 2)))
        (overlay-put moving-highlight 'priority -50)
        (overlay-put moving-highlight 'face '(:background "#123456" :extend t))
        (setq fontification-functions
          (list (lambda (pos)
                  (setq fontify-calls (1+ fontify-calls))
                  (put-text-property pos (min (point-max) (+ pos 40)) 'fontified t)))))"##,
    )
    .unwrap();
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    assert!(
        eval.eval_str("fontify-calls").unwrap().as_fixnum().unwrap() > 0,
        "the installed hook must actually fontify the first frame"
    );
    eval.eval_str(
        "(progn (forward-line 1) (move-overlay moving-highlight (point) (line-beginning-position 2)))",
    )
    .unwrap();
    engine.layout_frame_rust(&mut eval, frame);
    let stats = engine.last_layout_stats();
    assert!(
        stats.edit_windows > 0 && stats.reused_rows + stats.reused_shifted_rows >= 10,
        "a fontified buffer must still take the replay: {stats:?}"
    );
    assert_matches_full_layout(&mut eval, &mut engine, frame, "fontification hook");
}

/// A face installed by fontification in the SAME frame as the overlay update
/// must not be published from rows the replay measured before it. The property
/// tick moves inside the leaf, so the attempt is retried against the new
/// inputs; the next frame, with the hook idling, takes the replay again.
#[test]
fn face_overlay_replay_retries_when_fontification_installs_a_face() {
    let text = "session 会话 abc אבג\tready\n".repeat(80);
    let (mut eval, frame, _, _) = incr_editing_frame(&text, 800, 600);
    realize_test_gui_frame(&mut eval, frame);
    eval.eval_str(
        r##"(progn
        (setq line-spacing 0.2 truncate-lines t
              char-property-alias-alist '((face font-lock-face))
              mode-line-format "MODE"
              fontify-calls 0 fontify-arm nil fontify-target nil)
        (goto-char (point-min)) (forward-line 5)
        (setq moving-highlight (make-overlay (point) (line-beginning-position 2)))
        (overlay-put moving-highlight 'priority -50)
        (overlay-put moving-highlight 'face '(:background "#123456" :extend t))
        (setq fontification-functions
          (list (lambda (pos)
                  (setq fontify-calls (1+ fontify-calls))
                  (when (and fontify-arm (= pos fontify-target))
                    (setq fontify-arm nil)
                    (put-text-property pos (1+ pos) 'face '(:foreground "#ff0000"))))))
        ;; Everything is already fontified except one visible character outside
        ;; the highlight's row: the hook is entered once on every layout frame,
        ;; and only the armed frame changes a face.
        (put-text-property (point-min) (point-max) 'fontified t)
        (save-excursion
          (goto-char (point-min)) (forward-line 10)
          (setq fontify-target (point))
          (put-text-property fontify-target (1+ fontify-target) 'fontified nil)))"##,
    )
    .unwrap();
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    let calls_after_first = eval.eval_str("fontify-calls").unwrap().as_fixnum().unwrap();

    // The fontification callback installs a face outside the overlay's damage
    // span in the same frame that the overlay moves.
    eval.eval_str(
        "(progn (setq fontify-arm t) (forward-line 1)
                (move-overlay moving-highlight (point) (line-beginning-position 2)))",
    )
    .unwrap();
    engine.layout_frame_rust(&mut eval, frame);
    assert!(
        eval.eval_str("fontify-calls").unwrap().as_fixnum().unwrap() > calls_after_first,
        "the armed fontification callback must run during the replay frame"
    );
    assert_matches_full_layout(&mut eval, &mut engine, frame, "fontification face change");

    // With the same hook now idling, the next overlay update is a replay whose
    // face measurements are already final.
    eval.eval_str(
        "(progn (forward-line 1) (move-overlay moving-highlight (point) (line-beginning-position 2)))",
    )
    .unwrap();
    engine.layout_frame_rust(&mut eval, frame);
    let stats = engine.last_layout_stats();
    assert!(
        stats.edit_windows > 0 && stats.reused_rows + stats.reused_shifted_rows >= 10,
        "an idling fontification hook must not disable the replay: {stats:?}"
    );
    assert_matches_full_layout(&mut eval, &mut engine, frame, "idling fontification hook");
}

/// Structural refusals still fall back to the full walk: these add glyphs or
/// move rows in ways a face replay cannot reproduce, so declining is part of
/// the contract rather than an untested heuristic.
#[test]
fn face_overlay_replay_declines_under_structural_configurations() {
    let move_form = "(progn (forward-line 1) (move-overlay moving-highlight (point) (line-beginning-position 2)))";
    for setup in [
        "(setq truncate-lines nil word-wrap t)",
        "(setq show-trailing-whitespace t)",
        "(setq display-line-numbers t)",
        "(setq scroll-margin 2)",
        "(set-window-hscroll (selected-window) 3)",
    ] {
        exercise(setup, &[(move_form, 0)]);
    }
}
