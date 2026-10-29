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
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
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
    let mut full = crate::font::metrics::test_catalog::layout_engine();
    full.layout_frame_rust(eval, frame);
    let reference = full.last_frame_display_state.as_ref().unwrap();
    let actual_paint = paint(&incremental);
    let expected_paint = paint(reference);
    for (index, (actual, expected)) in actual_paint.iter().zip(&expected_paint).enumerate() {
        assert_eq!(actual, expected, "{label}: painted glyph/face {index}");
    }
    assert_eq!(
        actual_paint.len(),
        expected_paint.len(),
        "{label}: glyph count"
    );
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
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
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
                  (if (and fontify-arm (= pos fontify-target))
                    (progn
                      (setq fontify-arm nil)
                      (put-text-property pos (1+ pos) 'face '(:foreground "#ff0000")))))))
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
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
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
    assert!(
        eval.eval_str("fontify-arm").unwrap().is_nil(),
        "the callback must install a face"
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

/// `NEOMACS_LAYOUT_EDIT_SYNC=prove` must disable synchronized reuse here too.
#[test]
fn face_overlay_replay_declines_in_prove_mode() {
    use crate::incremental_layout::edit_sync::{EditSyncMode, set_edit_sync_mode_for_test};

    struct RestoreSyncMode;
    impl Drop for RestoreSyncMode {
        fn drop(&mut self) {
            set_edit_sync_mode_for_test(None);
        }
    }

    set_edit_sync_mode_for_test(Some(EditSyncMode::Prove));
    let _restore = RestoreSyncMode;
    exercise(
        "nil",
        &[(
            "(progn (forward-line 1) (move-overlay moving-highlight (point) (line-beginning-position 2)))",
            0,
        )],
    );
}

fn exercise_face_overlay_scroll(setup: &str, changes: &[(usize, &str, bool)]) {
    let text = "session 会话 abc אבג\tready\n".repeat(100);
    let (mut eval, frame, _, _) = incr_editing_frame(&text, 800, 600);
    realize_test_gui_frame(&mut eval, frame);
    eval.eval_str(
        r##"(progn
          (setq truncate-lines t line-spacing 0.2
                char-property-alias-alist '((face font-lock-face))
                mode-line-format '("MODE " "%p"))
          (save-excursion
            (goto-char (point-min))
            (while (search-forward "\t" nil t)
              (put-text-property (1- (point)) (point) 'display '(space :align-to 45))))
          (goto-char (point-min)) (forward-line 12)
          (setq moving-highlight (make-overlay (point) (line-beginning-position 2)))
          (overlay-put moving-highlight 'window (selected-window))
          (overlay-put moving-highlight 'priority -50)
          (overlay-put moving-highlight 'face '(:background "#123456" :extend t)))"##,
    )
    .unwrap();
    eval.eval_str(setup).unwrap();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.layout_frame_rust(&mut eval, frame);
    for (start_line, change, should_reuse) in changes {
        eval.eval_str(change).unwrap();
        eval.eval_str(&format!(
            "(set-window-start nil (save-excursion (goto-char (point-min)) (forward-line {start_line}) (point)) t)"
        )).unwrap();
        engine.layout_frame_rust(&mut eval, frame);
        let stats = engine.last_layout_stats();
        if *should_reuse {
            assert_eq!(stats.scroll_windows, 1, "{change}: {stats:?}");
            assert!(stats.reused_shifted_rows >= 5, "{change}: {stats:?}");
        } else {
            assert_eq!(stats.scroll_windows, 0, "{change}: {stats:?}");
        }
        assert_matches_full_layout(&mut eval, &mut engine, frame, change);
    }
}

#[test]
fn face_overlay_scroll_reuses_clean_prefix_and_remeasures_changed_faces() {
    exercise_face_overlay_scroll(
        "nil",
        &[
            (
                1,
                "(progn (forward-line 1) (move-overlay moving-highlight (point) (line-beginning-position 2)))",
                true,
            ),
            (
                2,
                "(progn (forward-line 1) (move-overlay moving-highlight (point) (line-beginning-position 2)) (overlay-put moving-highlight 'face '(:height 1.5 :box t :background \"#abcdef\")))",
                true,
            ),
            (3, "(delete-overlay moving-highlight)", true),
            (
                4,
                "(progn (forward-line 1) (move-overlay moving-highlight (point) (line-beginning-position 2)))",
                true,
            ),
        ],
    );
}

#[test]
fn face_overlay_scroll_with_damage_at_new_top_declines() {
    exercise_face_overlay_scroll(
        "nil",
        &[(
            1,
            "(progn (goto-char (point-min)) (forward-line 1) (move-overlay moving-highlight (point) (line-beginning-position 2)))",
            false,
        )],
    );
}

#[test]
fn face_overlay_scroll_keeps_structural_and_alias_changes_on_full_layout() {
    for setup in [
        "(overlay-put moving-highlight 'before-string \"prefix\")",
        "(overlay-put moving-highlight 'display \"replacement\")",
        "(overlay-put moving-highlight 'invisible t)",
        "(setq char-property-alias-alist '((mouse-face face)))",
    ] {
        exercise_face_overlay_scroll(
            setup,
            &[(
                1,
                "(progn (forward-line 1) (move-overlay moving-highlight (point) (line-beginning-position 2)))",
                false,
            )],
        );
    }
}

#[test]
fn face_overlay_scroll_with_simultaneous_text_edit_declines() {
    exercise_face_overlay_scroll(
        "nil",
        &[(
            1,
            "(progn (insert \"new\") (move-overlay moving-highlight (point) (line-beginning-position 2)))",
            false,
        )],
    );
}

#[test]
fn face_overlay_scroll_outside_viewport_matches_full_layout() {
    exercise_face_overlay_scroll(
        "(save-excursion (goto-char (point-min)) (forward-line 70) (move-overlay moving-highlight (point) (line-beginning-position 2)))",
        &[(
            1,
            "(save-excursion (goto-char (point-min)) (forward-line 71) (move-overlay moving-highlight (point) (line-beginning-position 2)))",
            true,
        )],
    );
}

#[test]
fn face_overlay_scroll_backward_remeasures_damage_before_reusing_suffix() {
    exercise_face_overlay_scroll(
        "(set-window-start nil (save-excursion (goto-char (point-min)) (forward-line 6) (point)) t)",
        &[
            (
                5,
                "(progn (forward-line -1) (move-overlay moving-highlight (point) (line-beginning-position 2)))",
                true,
            ),
            (
                4,
                "(progn (forward-line -1) (move-overlay moving-highlight (point) (line-beginning-position 2)) (overlay-put moving-highlight 'face '(:height 1.5 :box t :background \"#abcdef\")))",
                true,
            ),
            (3, "(delete-overlay moving-highlight)", true),
        ],
    );
}

#[test]
fn face_overlay_scroll_respects_sync_policy() {
    use crate::incremental_layout::edit_sync::{EditSyncMode, set_edit_sync_mode_for_test};
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            set_edit_sync_mode_for_test(None);
        }
    }
    set_edit_sync_mode_for_test(Some(EditSyncMode::Prove));
    let _restore = Restore;
    exercise_face_overlay_scroll(
        "nil",
        &[(
            1,
            "(progn (forward-line 1) (move-overlay moving-highlight (point) (line-beginning-position 2)))",
            false,
        )],
    );
}

#[test]
fn face_overlay_scroll_revalidates_fontification_outside_overlay_damage() {
    let text = "session ready\n".repeat(100);
    let (mut eval, frame, _, _) = incr_editing_frame(&text, 800, 600);
    realize_test_gui_frame(&mut eval, frame);
    eval.eval_str(
        r##"(progn
      (defvar scroll-fontify-arm nil)
      (setq truncate-lines t line-spacing 0.2 mode-line-format "MODE" scroll-fontify-calls nil)
      (goto-char (point-min)) (forward-line 12)
      (setq moving-highlight (make-overlay (point) (line-beginning-position 2)))
      (overlay-put moving-highlight 'face '(:background "#123456"))
      (put-text-property (point-min) (point-max) 'fontified t)
      (save-excursion (goto-char (point-min)) (forward-line 3)
        (setq fontify-target (point))
        (put-text-property (point) (1+ (point)) 'fontified nil))
      (setq fontification-functions
        (list (lambda (pos)
          (setq scroll-fontify-calls (cons pos scroll-fontify-calls))
          (if (and scroll-fontify-arm (= pos fontify-target))
            (progn
              (setq scroll-fontify-arm nil)
              (put-text-property pos (1+ pos) 'face '(:foreground "#ff0000"))))))))"##,
    )
    .unwrap();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.layout_frame_rust(&mut eval, frame);
    eval.eval_str(
        "(progn (setq scroll-fontify-arm t) (forward-line 1)
      (move-overlay moving-highlight (point) (line-beginning-position 2))
      (set-window-start nil (save-excursion (goto-char (point-min)) (forward-line 1) (point)) t))",
    )
    .unwrap();
    engine.layout_frame_rust(&mut eval, frame);
    assert!(
        eval.eval_str("scroll-fontify-arm").unwrap().is_nil(),
        "fontification must run: {:?}",
        printed_eval_result(
            &mut eval,
            "(prin1-to-string (list scroll-fontify-calls fontify-target (get-text-property fontify-target 'fontified) (window-start)))"
        )
    );
    assert_matches_full_layout(
        &mut eval,
        &mut engine,
        frame,
        "scroll fontification outside highlight",
    );
}

#[test]
fn face_overlay_scroll_after_automatic_viewport_retry_matches_full_layout() {
    exercise_automatic_face_overlay_scroll(&"session 会话 ready\n".repeat(120), "nil", true);
}

#[test]
fn face_overlay_scroll_automatic_mixed_rows_and_visible_cursor_match_full_layout() {
    exercise_automatic_face_overlay_scroll(
        &"session 会话 abc אבג\tready\n\n".repeat(120),
        r##"(progn
            (setq cursor-type 'box header-line-format "HEADER")
            (save-excursion
              (goto-char (point-min))
              (while (search-forward "\t" nil t)
                (put-text-property (1- (point)) (point) 'display '(space :align-to 45)))))"##,
        false,
    );
}

fn exercise_automatic_face_overlay_scroll(text: &str, setup: &str, check_measurements: bool) {
    let (mut eval, frame, _, _) = incr_editing_frame(text, 800, 180);
    realize_test_gui_frame(&mut eval, frame);
    eval.eval_str(
        r##"(progn
        (setq truncate-lines t line-spacing 0.2 cursor-type nil
              scroll-step 1 scroll-conservatively 101 mode-line-format '("MODE " "%p"))
        (goto-char (point-min))
        (setq moving-highlight (make-overlay (point) (line-beginning-position 2)))
        (overlay-put moving-highlight 'face '(:background "#123456" :extend t)))"##,
    )
    .unwrap();
    eval.eval_str(setup).unwrap();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.layout_frame_rust(&mut eval, frame);
    let mut reused_scrolls = 0;
    for direction in [1, -1] {
        for step in 0..24 {
            eval.eval_str(&format!("(progn (forward-line {direction}) (move-overlay moving-highlight (point) (line-beginning-position 2)))")).unwrap();
            engine.layout_frame_rust(&mut eval, frame);
            reused_scrolls += engine.last_layout_stats().scroll_windows;
            if check_measurements && direction == 1 && engine.last_layout_stats().scroll_windows > 0
            {
                assert!(
                    engine.last_layout_stats().buffer_snapshots_built <= 3,
                    "automatic scroll must avoid an extra full measurement: {:?}",
                    engine.last_layout_stats()
                );
            }
            let label = format!(
                "automatic viewport retry direction={direction} step={step} stats={:?}",
                engine.last_layout_stats()
            );
            assert_matches_full_layout(&mut eval, &mut engine, frame, &label);
        }
    }
    assert!(
        reused_scrolls >= if check_measurements { 10 } else { 5 },
        "automatic scrolling must reuse rows: {reused_scrolls}"
    );
}

#[test]
fn face_overlay_scroll_after_automatic_viewport_retry_with_distinct_faces() {
    exercise_automatic_face_overlay_scroll(
        &(0..120)
            .map(|i| format!("session {i} ready\n"))
            .collect::<String>(),
        r##"(save-excursion
            (goto-char (point-min))
            (let ((i 0))
              (while (search-forward "ready" nil t)
                (put-text-property (line-beginning-position) (line-end-position)
                                   'face (nth (% i 3) '((:foreground "#ff0000") (:foreground "#00ff00") (:foreground "#0000ff"))))
                (setq i (1+ i)))))"##,
        true,
    );
}

#[test]
fn long_session_list_navigation_converges_without_extra_scrolls() {
    let text = (0..999)
        .map(|i| match i % 4 {
            0 => format!("session {i} 会话 abc אבג\tready\n"),
            1 => format!("session {i} done\n"),
            2 => format!("session {i} running 运行中\n"),
            _ => format!("session {i}\n"),
        })
        .collect::<String>();
    let (mut eval, frame, _, _) = incr_editing_frame(&text, 800, 420);
    realize_test_gui_frame(&mut eval, frame);
    eval.eval_str(
        r##"(progn
        (setq truncate-lines t line-spacing 0.2 cursor-type nil
              scroll-step 1 scroll-conservatively 101
              char-property-alias-alist '((face font-lock-face))
              mode-line-format '("MODE " "%p"))
        (save-excursion
          (goto-char (point-min))
          (let ((i 0))
            (while (search-forward "ready" nil t)
              (put-text-property (line-beginning-position) (line-end-position)
                                 'face (nth (% i 5) '((:foreground "#c0c0c0")
                                                      (:foreground "#00ff00")
                                                      (:foreground "#ffaa00")
                                                      (:foreground "#4488ff")
                                                      (:foreground "#ff44aa"))))
              (setq i (1+ i)))))
        (goto-char (point-min))
        (setq moving-highlight (make-overlay (point) (line-beginning-position 2)))
        (overlay-put moving-highlight 'priority 5)
        (overlay-put moving-highlight 'face '(:background "#123456" :extend t)))"##,
    )
    .unwrap();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.layout_frame_rust(&mut eval, frame);

    let mut stats = Vec::new();
    let mut scrolls = 0;
    let mut fulls = 0;
    for step in 0..60 {
        eval.eval_str(
            "(progn (forward-line 1) (move-overlay moving-highlight (point) (line-beginning-position 2)))",
        )
        .unwrap();
        engine.layout_frame_rust(&mut eval, frame);
        let first = engine.last_layout_stats().clone();
        if first.scroll_windows > 0 {
            scrolls += 1;
        }
        if first.full_windows > 0 {
            fulls += 1;
        }
        let start: i64 = eval
            .eval_str("(window-start)")
            .unwrap()
            .as_fixnum()
            .map(i64::from)
            .unwrap_or(-1);
        // No Lisp ran between the two layouts: a converged frame must not
        // scroll, re-edit, or lay the window out from scratch again.
        engine.layout_frame_rust(&mut eval, frame);
        let second = engine.last_layout_stats().clone();
        let start_again: i64 = eval
            .eval_str("(window-start)")
            .unwrap()
            .as_fixnum()
            .map(i64::from)
            .unwrap_or(-1);
        if second.scroll_windows != 0
            || second.full_windows != 0
            || second.edit_windows != 0
            || start_again != start
        {
            panic!(
                "step {step} did not converge: start {start} -> {start_again}, first {:?}, second {:?}",
                first, second
            );
        }
        stats.push((step, start, first));
    }
    let scrolled: Vec<_> = stats.iter().filter(|(_, start, _)| *start > 1).collect();
    eprintln!(
        "scrolling steps={} full={} scroll_windows={} last={:?}",
        scrolled.len(),
        fulls,
        scrolls,
        stats.last().map(|(_, _, s)| s)
    );
    assert!(
        scrolled.len() > 5,
        "navigation must scroll the viewport: {:?}",
        stats
    );
}

/// The dsh-sessions navigation shape: point walks down with NO overlay or text
/// change. Once it reaches the last visible row, cursor-only declines
/// (`PointMoveMayScrollDown`) and the leaf discovers the new window start. That
/// continuation must reuse the retained rows instead of running a full layout
/// for every scrolling step, and must not admit retained faces too late.
#[test]
fn point_only_automatic_scroll_reuses_retained_rows() {
    let text = (0..400)
        .map(|i| format!("session {i} 会话 abc אבג\tready\n"))
        .collect::<String>();
    let (mut eval, frame, _, _) = incr_editing_frame(&text, 800, 420);
    realize_test_gui_frame(&mut eval, frame);
    eval.eval_str(
        r##"(progn
        (setq truncate-lines t line-spacing 0.2 cursor-type nil
              scroll-step 1 scroll-conservatively 101
              mode-line-format '("MODE " "%p"))
        (goto-char (point-min)))"##,
    )
    .unwrap();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.layout_frame_rust(&mut eval, frame);
    let mut scrolls = 0;
    for step in 0..60 {
        eval.eval_str("(forward-line 1)").unwrap();
        engine.layout_frame_rust(&mut eval, frame);
        let stats = engine.last_layout_stats().clone();
        if stats.scroll_windows > 0 {
            scrolls += 1;
        }
        assert_eq!(
            stats.full_windows, 0,
            "step {step} fell back to a full layout: {stats:?}"
        );
        let start: i64 = eval
            .eval_str("(window-start)")
            .unwrap()
            .as_fixnum()
            .map(i64::from)
            .unwrap_or(-1);
        // Nothing ran between the two layouts: the accepted frame must stand.
        engine.layout_frame_rust(&mut eval, frame);
        let again = engine.last_layout_stats().clone();
        let start_again: i64 = eval
            .eval_str("(window-start)")
            .unwrap()
            .as_fixnum()
            .map(i64::from)
            .unwrap_or(-1);
        assert!(
            again.scroll_windows == 0
                && again.full_windows == 0
                && again.edit_windows == 0
                && start_again == start,
            "step {step} did not converge: {start} -> {start_again}, {stats:?} then {again:?}"
        );
        assert_matches_full_layout(
            &mut eval,
            &mut engine,
            frame,
            &format!("point-only automatic scroll step {step}"),
        );
    }
    assert!(
        scrolls >= 20,
        "automatic scrolling must reuse retained rows: {scrolls}"
    );
}

/// Which frame realization the face-change regression runs on.
#[derive(Clone, Copy, Debug)]
enum FaceChangeFrame {
    Gui,
    Tty,
}

/// A global face change after a laid-out frame moves `face_change_count`, so
/// no retained row of that frame can be reused -- and Phase A must not admit
/// those rows' face IDs for a scroll continuation either. The mode line's
/// basic `mode-line-active` face keeps its ID with new content; admitting the
/// retained, stale realization of that ID made the chrome walk panic with
/// `FrameFaceConflict` ("row rendering must preserve its realized face
/// identity"), which crashed every theme load and `set-face-attribute` on the
/// mode line in a running editor (CI TUI treesit/gruvbox/leuven, 2026-10-28).
fn mode_line_face_change_matches_full_layout(frame_kind: FaceChangeFrame) {
    let text = "line of text\n".repeat(80);
    let (mut eval, frame, _, _) = incr_editing_frame(&text, 800, 600);
    match frame_kind {
        FaceChangeFrame::Gui => realize_test_gui_frame(&mut eval, frame),
        FaceChangeFrame::Tty => install_window_fill_test_tty_palette(&mut eval),
    }
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.layout_frame_rust(&mut eval, frame);
    for (label, form) in [
        (
            "mode-line recolored",
            r#"(progn
                 (internal-set-lisp-face-attribute 'mode-line :foreground "black" (selected-frame))
                 (internal-set-lisp-face-attribute 'mode-line :background "white" (selected-frame))
                 (internal-set-lisp-face-attribute 'mode-line :box nil (selected-frame)))"#,
        ),
        (
            "mode-line boxed again",
            r#"(progn
                 (internal-set-lisp-face-attribute 'mode-line :foreground "white" (selected-frame))
                 (internal-set-lisp-face-attribute 'mode-line :background "black" (selected-frame))
                 (internal-set-lisp-face-attribute
                  'mode-line :box '(:line-width -1 :style released-button) (selected-frame)))"#,
        ),
        (
            "default recolored with point motion",
            r#"(progn
                 (internal-set-lisp-face-attribute 'default :foreground "red" (selected-frame))
                 (forward-line 3))"#,
        ),
    ] {
        eval.eval_str(form).unwrap();
        engine.layout_frame_rust(&mut eval, frame);
        assert_matches_full_layout(
            &mut eval,
            &mut engine,
            frame,
            &format!("{frame_kind:?}: {label}"),
        );
    }
}

#[test]
fn face_change_does_not_admit_stale_retained_faces_on_gui_frames() {
    mode_line_face_change_matches_full_layout(FaceChangeFrame::Gui);
}

#[test]
fn face_change_does_not_admit_stale_retained_faces_on_tty_frames() {
    mode_line_face_change_matches_full_layout(FaceChangeFrame::Tty);
}
