//! The specialized producer is admitted only by the original bounded proof.
//! Every forced arm compares complete painted output/matrix placement/point
//! snapshots/window-end against a full render, and compares observable counts
//! across arms. Numeric witnesses establish admitted versus rejected proofs.

use super::*;
use crate::incremental_layout::edit_sync::{
    EditSyncMode, ProveFirstCounts, prove_first_counts_for_test, reset_prove_first_counts_for_test,
    set_edit_sync_mode_for_test, set_prove_first_for_test,
};
use crate::incremental_layout::mode_line_gate::{ModeLineGate, set_mode_line_gate_for_test};
use neomacs_display_protocol::face::Face;
use neomacs_display_protocol::glyph_matrix::FrameDisplayState;

struct ProducerGuard;
impl ProducerGuard {
    fn set(enabled: bool) -> Self {
        set_edit_sync_mode_for_test(Some(EditSyncMode::Sync));
        set_mode_line_gate_for_test(Some(ModeLineGate::Gnu));
        set_prove_first_for_test(Some(enabled));
        reset_prove_first_counts_for_test();
        Self
    }
}
impl Drop for ProducerGuard {
    fn drop(&mut self) {
        set_prove_first_for_test(None);
        set_mode_line_gate_for_test(None);
        set_edit_sync_mode_for_test(None);
        reset_prove_first_counts_for_test();
    }
}

fn normalize_face(mut face: Face) -> Face {
    face.id = FaceId::new(0);
    face.default_resolved_font_id = face
        .default_resolved_font_id
        .map(|_| neomacs_display_protocol::font::ResolvedFontId(0));
    face
}
fn frame_paint(state: &FrameDisplayState) -> Vec<(FrameGlyph, Option<Face>)> {
    let mut paint = Vec::new();
    state.for_each_glyph(|mut glyph| {
        let face = match &mut glyph {
            FrameGlyph::Char { face_id, .. }
            | FrameGlyph::Stretch { face_id, .. }
            | FrameGlyph::Image { face_id, .. }
            | FrameGlyph::Video { face_id, .. }
            | FrameGlyph::Xwidget { face_id, .. }
            | FrameGlyph::Surface { face_id, .. }
            | FrameGlyph::FringeBitmap { face_id, .. } => {
                let face = state.faces.get(face_id).cloned().map(normalize_face);
                *face_id = FaceId::new(0);
                face
            }
            _ => None,
        };
        paint.push((glyph, face));
    });
    paint
}

fn complete_frame_rows(state: &FrameDisplayState) -> Vec<(i64, Vec<(usize, RowTrace)>)> {
    state
        .window_matrices
        .iter()
        .map(|entry| {
            (
                entry.window_id.get(),
                entry
                    .matrix
                    .rows
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| row.enabled)
                    .map(|(index, row)| (index, RowTrace::from_row(row, &state.faces)))
                    .collect(),
            )
        })
        .collect()
}

fn complete_window_ends(
    eval: &Context,
    frame_id: neovm_core::window::FrameId,
    state: &FrameDisplayState,
) -> Vec<(i64, neovm_core::window::WindowEndState)> {
    let frame = eval.frame_manager().get(frame_id).unwrap();
    state
        .window_matrices
        .iter()
        .filter_map(|entry| {
            match frame.find_window(neovm_core::window::WindowId(entry.window_id.get() as u64)) {
                Some(neovm_core::window::Window::Leaf { window_end, .. }) => {
                    Some((entry.window_id.get(), *window_end))
                }
                _ => None,
            }
        })
        .collect()
}

fn source() -> String {
    "plain ascii line for typing\n".repeat(60)
}
fn line_end(text: &str, line: usize) -> usize {
    text.split_inclusive('\n')
        .take(line + 1)
        .map(|line| line.chars().count())
        .sum::<usize>()
        - 1
}

/// Full-layout comparison uses the same Context to preserve source-object
/// identities. Only allocation-local face/font IDs are normalized, retaining
/// their values. Observable numeric Lisp results are compared between arms.
fn run(
    enabled: bool,
    text: &str,
    point: usize,
    forms: &[&str],
) -> (Vec<String>, ProveFirstCounts, Vec<LayoutStats>) {
    run_with_policy(enabled, EditSyncMode::Sync, true, text, point, forms)
}

/// The same exclusively owned Context/reference-render fixture with explicit
/// producer policy. It returns only owned strings/numeric counts after Drop.
fn run_with_policy(
    enabled: bool,
    mode: EditSyncMode,
    allow_below_reuse: bool,
    text: &str,
    point: usize,
    forms: &[&str],
) -> (Vec<String>, ProveFirstCounts, Vec<LayoutStats>) {
    let _guard = ProducerGuard::set(enabled);
    set_edit_sync_mode_for_test(Some(mode));
    let (mut eval, frame_id, _buffer, _window) = incr_editing_frame(text, 800, 600);
    eval.eval_str(&format!(
        "(progn (setq bidi-paragraph-direction 'left-to-right) \
        (setq prove-first-mode-count 0 prove-first-font-count 0) \
        (setq mode-line-format '((:eval (progn \
             (setq prove-first-mode-count (1+ prove-first-mode-count)) \"ML\")))) \
        (goto-char {}))",
        point + 1
    ))
    .unwrap();
    let mut engine = LayoutEngine::new();
    engine.allow_below_reuse = allow_below_reuse;
    engine.layout_frame_rust(&mut eval, frame_id);
    activate_last_engine_presentation(&mut eval, &engine, frame_id);
    let mut observations = Vec::new();
    let mut statistics = Vec::new();
    reset_prove_first_counts_for_test();
    for form in forms {
        eval.eval_str("(setq prove-first-mode-count 0 prove-first-font-count 0)")
            .unwrap();
        eval.eval_str(form).unwrap();
        engine.layout_frame_rust(&mut eval, frame_id);
        activate_last_engine_presentation(&mut eval, &engine, frame_id);
        let incremental = engine.last_frame_display_state.as_ref().unwrap().clone();
        let ends = complete_window_ends(&eval, frame_id, &incremental);
        assert!(
            ends.iter().all(|(_, end)| end.is_current()),
            "{form}: {ends:?}"
        );
        let trace = selected_window_layout_trace(&eval, &engine, frame_id);
        statistics.push(engine.last_layout_stats().clone());
        let observation = eval
            .eval_str(
                r#"(prin1-to-string
            (list prove-first-mode-count prove-first-font-count
                  (window-end nil t)
                  (let ((p (posn-at-point)))
                    (and p (list (nth 1 p) (nth 2 p) (nth 8 p))))))"#,
            )
            .unwrap();
        observations.push(observation.as_str_owned().unwrap());
        // The reference render must not add callback effects to the measured
        // attempt; temporarily remove fontification callbacks for this check.
        eval.eval_str("(setq prove-first-saved-fontifiers fontification-functions fontification-functions nil)").unwrap();
        let mut fresh = LayoutEngine::new();
        fresh.layout_frame_rust(&mut eval, frame_id);
        let full = fresh.last_frame_display_state.as_ref().unwrap();
        assert_eq!(
            complete_frame_rows(&incremental),
            complete_frame_rows(full),
            "{form}: matrix rows"
        );
        assert_eq!(
            frame_paint(&incremental),
            frame_paint(full),
            "{form}: complete paint"
        );
        assert_eq!(
            trace,
            selected_window_layout_trace(&eval, &fresh, frame_id),
            "{form}: all display points"
        );
        assert_eq!(
            ends,
            complete_window_ends(&eval, frame_id, full),
            "{form}: complete window ends"
        );
        assert_eq!(
            incremental.phys_cursor, full.phys_cursor,
            "{form}: frame cursor"
        );
        assert_eq!(
            incremental.cursor_effects_by_window,
            full.cursor_effects_by_window
        );
        assert_eq!(incremental.backgrounds.len(), full.backgrounds.len());
        assert_eq!(incremental.face_fills.len(), full.face_fills.len());
        assert_eq!(incremental.borders.len(), full.borders.len());
        assert_eq!(incremental.scroll_bars.len(), full.scroll_bars.len());
        assert_eq!(incremental.window_infos.len(), full.window_infos.len());
        eval.eval_str("(setq fontification-functions prove-first-saved-fontifiers)")
            .unwrap();
    }
    (observations, prove_first_counts_for_test(), statistics)
}

#[test]
fn prove_first_ascii_typing_keeps_complete_geometry_and_gnu_chrome_counts() {
    let text = source();
    let forms = ["(insert \"x\")", "(delete-region (1- (point)) (point))"];
    let off = run(false, &text, line_end(&text, 8), &forms);
    let on = run(true, &text, line_end(&text, 8), &forms);
    assert_eq!(
        on.0, off.0,
        "mode-line callbacks and published point/window-end queries"
    );
    assert_eq!(off.1, ProveFirstCounts::default());
    assert!(
        on.1.preferred_prove > 0,
        "must enter certified producer: {:?}",
        on.1
    );
    assert!(on.2.iter().all(|stats| stats.edit_windows == 1));
}

#[test]
fn prove_first_rejected_newline_still_enters_general_sync_and_shifts_rows() {
    let text = source();
    let forms = ["(insert \"\\n\")"];
    let off = run(false, &text, line_end(&text, 8) - 3, &forms);
    let on = run(true, &text, line_end(&text, 8) - 3, &forms);
    assert_eq!(on.0, off.0);
    assert_eq!(on.1.preferred_prove, 0);
    assert!(
        on.1.sync_fallback > 0,
        "an above-only Some must be rejected: {:?}",
        on.1
    );
    assert!(
        on.2[0].reused_shifted_rows > 0,
        "general sync must still install moved tail"
    );
}

#[test]
fn prove_first_rejected_width_proof_keeps_general_sync_and_cursor_exact() {
    let text = source();
    let forms = ["(insert (make-string 100 ?w))"];
    let off = run(false, &text, line_end(&text, 8) - 3, &forms);
    let on = run(true, &text, line_end(&text, 8) - 3, &forms);
    assert_eq!(on.0, off.0);
    assert_eq!(on.1.preferred_prove, 0);
    assert!(
        on.1.sync_fallback > 0,
        "must reject stale one-row width proof"
    );
}

#[test]
fn prove_first_unproven_unicode_keeps_general_sync_without_a_prove_attempt() {
    let text = source();
    let forms = ["(insert \"中\")"];
    let off = run(false, &text, line_end(&text, 8), &forms);
    let on = run(true, &text, line_end(&text, 8), &forms);
    assert_eq!(on.0, off.0);
    assert_eq!(on.1, ProveFirstCounts::default());
    assert!(
        on.2[0].edit_windows > 0,
        "general unproven edit remains incremental"
    );
}

#[test]
fn prove_first_structure_property_uses_general_sync_and_preserves_points() {
    let text = source();
    let forms = ["(put-text-property (- (point) 4) (point) 'display \"<REPLACED>\")"];
    let off = run(false, &text, line_end(&text, 8), &forms);
    let on = run(true, &text, line_end(&text, 8), &forms);
    assert_eq!(on.0, off.0);
    assert_eq!(on.1, ProveFirstCounts::default());
    assert!(on.2[0].edit_windows > 0);
}

#[test]
fn prove_first_preserves_unmarking_fontification_callback_counts_on_fallback() {
    let text = source();
    let forms = ["(progn (setq fontification-functions \
                    (list (lambda (_p) (setq prove-first-font-count \
                          (1+ prove-first-font-count))))) \
                    (delete-region (point) (1+ (point))))"];
    let off = run(false, &text, line_end(&text, 8), &forms);
    let on = run(true, &text, line_end(&text, 8), &forms);
    assert_eq!(on.0, off.0, "numeric planning must not evaluate fontifiers");
    assert!(on.1.sync_fallback > 0);
    assert!(
        on.0[0]
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse::<i64>()
            .unwrap()
            > 0,
        "fontification callback must be exercised: {:?}",
        on.0
    );
}

#[test]
fn prove_first_preserves_sync_extra_reuse_at_a_line_start() {
    let text = source();
    let point = text
        .split_inclusive('\n')
        .take(8)
        .map(|line| line.len())
        .sum();
    let forms = ["(insert \"x\")"];
    let off = run(false, &text, point, &forms);
    let on = run(true, &text, point, &forms);
    assert_eq!(on.0, off.0);
    assert_eq!(
        on.1.preferred_prove, 0,
        "cannot restore a widened predecessor"
    );
    assert!(
        on.1.sync_fallback > 0,
        "proven but unequal spans must fall back"
    );
    assert_eq!(on.2[0].relaid_body_rows, off.2[0].relaid_body_rows);
    assert_eq!(on.2[0].reused_rows, off.2[0].reused_rows);
}

#[cfg(test)]
#[path = "edit_sync_lazy_proof_engine_test.rs"]
mod lazy_proof_tests;

#[cfg(test)]
#[path = "retained_face_gather_engine_test.rs"]
mod retained_face_gather_tests;
