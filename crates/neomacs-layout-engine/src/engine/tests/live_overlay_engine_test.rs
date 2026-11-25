//! P4b/P4c/P4d engine-level tests for `LiveOverlayView`:
//! - mid-attempt fontification callbacks that move, overlay-put, delete, and GC
//!   produce the exact same frame as captured mode;
//! - an always-mutating mode-line callback converges cleanly via the bounded
//!   live-retry -> captured fallback without looping.

use super::*;
use crate::neovm_bridge::OverlaySourceMode;

struct ScopedTempDir(std::path::PathBuf);

impl ScopedTempDir {
    fn new(prefix: &str) -> Self {
        let base = std::env::var_os("CARGO_TARGET_TMPDIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = base.join(format!("{prefix}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scoped test tempdir");
        Self(dir)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for ScopedTempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn paint_frame(state: &neomacs_display_protocol::glyph_matrix::FrameDisplayState) -> Vec<String> {
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

fn run_fontification_overlay_mutation(mode: OverlaySourceMode) -> Vec<String> {
    let text = "line-01 alpha\nline-02 beta\nline-03 gamma\nline-04 delta\n";
    let (mut eval, frame, _, _) = incr_editing_frame(text, 800, 300);
    realize_test_gui_frame(&mut eval, frame);
    eval.eval_str(
        r##"(progn
              (setq mode-line-format '("MODE"))
              (setq live-test-ran nil)
              (setq ov-move (make-overlay 1 8))
              (overlay-put ov-move 'face '(:background "#112233"))
              (setq ov-put (make-overlay 15 22))
              (overlay-put ov-put 'face '(:background "#445566"))
              (setq ov-del (make-overlay 28 35))
              (overlay-put ov-del 'face '(:background "#778899"))
              (put-text-property (point-min) (point-max) 'fontified nil)
              (setq fontification-functions
                (list (lambda (pos)
                        (if live-test-ran
                            nil
                          (setq live-test-ran t)
                          (move-overlay ov-move 42 49)
                          (overlay-put ov-put 'face '(:background "#AABBCC"))
                          (delete-overlay ov-del)
                          (setq ov-del nil)
                          (garbage-collect))
                        (put-text-property pos (point-max) 'fontified t)))))"##,
    )
    .unwrap();
    let before_gc = eval.eval_str("gcs-done").unwrap().as_int().unwrap();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.set_overlay_source_mode_for_test(mode);
    engine.layout_frame_rust(&mut eval, frame);
    assert_eq!(
        eval.eval_str("live-test-ran").unwrap(),
        Value::T,
        "fontification callback must have run"
    );
    assert!(
        eval.eval_str("gcs-done").unwrap().as_int().unwrap() > before_gc,
        "fontification callback must have completed GC"
    );
    paint_frame(
        engine
            .last_frame_display_state
            .as_ref()
            .expect("layout frame must complete"),
    )
}

#[test]
fn live_overlay_fontification_move_put_delete_gc_matches_captured_frame() {
    let captured = run_fontification_overlay_mutation(OverlaySourceMode::Captured);
    let live = run_fontification_overlay_mutation(OverlaySourceMode::Live);
    assert_eq!(
        live, captured,
        "live overlay view must produce the exact same frame as captured after mid-attempt move/put/delete/GC"
    );
}

fn run_chrome_one_shot_overlay_mutation(
    mode: OverlaySourceMode,
) -> (Vec<String>, Vec<String>, i64) {
    let text = "line-01 alpha\nline-02 beta\nline-03 gamma\nline-04 delta\n";
    let (mut eval, frame, _, _) = incr_editing_frame(text, 800, 300);
    realize_test_gui_frame(&mut eval, frame);
    eval.eval_str(
        r##"(progn
              (setq chrome-mut-count 0)
              (setq ov-move (make-overlay 1 8))
              (overlay-put ov-move 'face '(:background "#112233"))
              (setq ov-put (make-overlay 15 22))
              (overlay-put ov-put 'face '(:background "#445566"))
              (setq ov-del (make-overlay 28 35))
              (overlay-put ov-del 'face '(:background "#778899"))
              (setq mode-line-format
                '((:eval (progn
                           (setq chrome-mut-count (1+ chrome-mut-count))
                           (if (= chrome-mut-count 1)
                               (progn
                                 (move-overlay ov-move 42 49)
                                 (overlay-put ov-put 'face '(:background "#AABBCC"))
                                 (delete-overlay ov-del)
                                 (setq ov-del nil)
                                 (garbage-collect)))
                           "MODE")))))"##,
    )
    .unwrap();
    let before_gc = eval.eval_str("gcs-done").unwrap().as_int().unwrap();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.set_overlay_source_mode_for_test(mode);
    engine.layout_frame_rust(&mut eval, frame);
    assert!(
        eval.eval_str("gcs-done").unwrap().as_int().unwrap() > before_gc,
        "chrome callback must have completed GC"
    );
    let calls_frame1 = eval.eval_str("chrome-mut-count").unwrap().as_int().unwrap();
    let frame1 = paint_frame(
        engine
            .last_frame_display_state
            .as_ref()
            .expect("first layout frame must complete"),
    );
    engine.layout_frame_rust(&mut eval, frame);
    let frame2 = paint_frame(
        engine
            .last_frame_display_state
            .as_ref()
            .expect("second layout frame must complete"),
    );
    (frame1, frame2, calls_frame1)
}

#[test]
fn live_overlay_chrome_mutation_matches_captured_and_preserves_body() {
    let (cap_f1, cap_f2, cap_calls) =
        run_chrome_one_shot_overlay_mutation(OverlaySourceMode::Captured);
    let (live_f1, live_f2, live_calls) =
        run_chrome_one_shot_overlay_mutation(OverlaySourceMode::Live);
    assert_eq!(
        live_calls, cap_calls,
        "late chrome overlay mutation must evaluate chrome the same number of times as captured"
    );
    assert_eq!(
        live_calls, 1,
        "late chrome overlay mutation must preserve sealed body without rewinding"
    );
    assert_eq!(
        live_f1, cap_f1,
        "frame 1 after late chrome overlay mutation must match captured"
    );
    assert_eq!(
        live_f2, cap_f2,
        "frame 2 picking up mutated overlays must match captured"
    );
    assert_ne!(
        live_f1, live_f2,
        "frame 2 must observe the overlays mutated by frame 1 chrome"
    );
}

fn parse_stat_field(line: &str, key: &str) -> usize {
    line.split_whitespace()
        .find_map(|tok| tok.strip_prefix(key)?.strip_prefix('='))
        .and_then(|val| val.parse::<usize>().ok())
        .unwrap_or_else(|| panic!("missing {key}=... in layout stats line: {line:?}"))
}

struct CompositionStaleFontHost;

impl neovm_core::emacs_core::eval::DisplayHost for CompositionStaleFontHost {
    fn realize_gui_frame(
        &mut self,
        _request: neovm_core::emacs_core::eval::GuiFrameHostRequest,
    ) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(
        &mut self,
        _request: neovm_core::emacs_core::eval::GuiFrameHostRequest,
    ) -> Result<(), String> {
        Ok(())
    }

    fn resolve_font_for_char(
        &mut self,
        _request: neovm_core::emacs_core::display_host::FontResolveRequest,
    ) -> Result<Option<neovm_core::emacs_core::eval::ResolvedFontMatch>, String> {
        use neomacs_display_protocol::font::{
            FontBackendKind, FontMemoryAsset, FontOutlineAsset, FontReplay, FontSlantKind,
            ResolvedFont, ResolvedFontId, ResolvedFontIdentity,
        };
        let metrics = neovm_core::emacs_core::eval::FontPxProbeResult {
            pixel_size: 14,
            height: 16,
            ascent: 12,
            descent: 4,
            max_width: 8,
            space_width: 8,
            average_width: 8,
        };
        Ok(Some(neovm_core::emacs_core::eval::ResolvedFontMatch {
            font: neovm_core::emacs_core::eval::ResolvedOpenedFont {
                resolved: ResolvedFont {
                    id: ResolvedFontId(0),
                    identity: ResolvedFontIdentity::from_memory(
                        FontBackendKind::Fontconfig,
                        "test:composition-stale".to_owned(),
                        0,
                        None,
                    ),
                    replay: FontReplay::Swash {
                        asset: FontOutlineAsset::Memory(
                            FontMemoryAsset::new(
                                "test:composition-stale".to_owned(),
                                std::sync::Arc::new(vec![0, 1, 0, 0]),
                                0,
                            )
                            .expect("non-empty test font"),
                        ),
                    },
                    family: "Monospace".to_owned(),
                    full_name: None,
                    postscript_name: None,
                    weight: 400,
                    slant: FontSlantKind::Normal,
                    width: 5,
                    pixel_size: 14.0,
                    ascent_px: 12.0,
                    descent_px: 4.0,
                    space_advance_px: 8.0,
                    glyph_advance: Default::default(),
                    spacing: None,
                },
                foundry: None,
                slant: neovm_core::face::FontSlant::Normal,
                metrics,
                capability: None,
            },
            glyph_code: Some(1),
        }))
    }
}

fn run_composition_stale_scenario(
    mode: OverlaySourceMode,
    mutate_limit: i64,
    stats_path: &std::path::Path,
) -> (Vec<String>, i64, usize, usize) {
    let text = "xy hello live overlay fallback\nsecond line\n";
    let (mut eval, frame, _, _) = incr_editing_frame(text, 800, 300);
    realize_test_gui_frame(&mut eval, frame);
    eval.set_display_host(Box::new(CompositionStaleFontHost));
    eval.eval_str(&format!(
        r##"(progn
              (fset 'save-match-data
                (cons 'macro (lambda (&rest body) (cons 'progn body))))
              (setq comp-mut-count 0)
              (setq comp-ov (make-overlay 1 6))
              (overlay-put comp-ov 'face '(:background "#123456"))
              (fset 'auto-compose-chars
                (lambda (&rest _args)
                  (setq comp-mut-count (1+ comp-mut-count))
                  (if (<= comp-mut-count {mutate_limit})
                      (overlay-put comp-ov 'live-tick comp-mut-count))
                  nil))
              (setq auto-composition-mode t
                    auto-composition-function 'auto-compose-chars
                    composition-function-table (make-char-table nil))
              (aset composition-function-table ?x
                    (list (vector "xy" 0 'font-shape-gstring))))"##
    ))
    .unwrap();

    let _ = std::fs::remove_file(stats_path);
    neovm_core::buffer::overlay::reset_overlay_layout_stats();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.set_overlay_source_mode_for_test(mode);
    engine.layout_frame_rust(&mut eval, frame);
    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("composition stale scenario must converge");
    assert!(
        !state.window_matrices.is_empty(),
        "converged frame must contain rendered window matrices"
    );
    let stats_text = std::fs::read_to_string(stats_path).expect("layout stats line written");
    let last_line = stats_text.lines().last().expect("non-empty stats line");
    let copies = parse_stat_field(last_line, "overlay_copies");
    let live_records = parse_stat_field(last_line, "overlay_live_records");
    let calls = eval.eval_str("comp-mut-count").unwrap().as_int().unwrap();
    (paint_frame(state), calls, copies, live_records)
}

#[test]
fn live_overlay_composition_stale_retries_live_then_falls_back_to_captured() {
    let dir = ScopedTempDir::new("neomacs-live-fallback");
    let stats_path = dir.path().join("stats");
    // SAFETY: nextest runs each test in its own process, before OnceLocks initialize.
    unsafe {
        std::env::set_var("NEOMACS_LAYOUT_STATS_FILE", &stats_path);
    }

    // 1st Stale retries once in Live mode (0 overlay copies, nonzero live records)
    // and matches Captured.
    let (cap1, _, _, _) =
        run_composition_stale_scenario(OverlaySourceMode::Captured, 1, &stats_path);
    let (live1, calls1, copies1, live_records1) =
        run_composition_stale_scenario(OverlaySourceMode::Live, 1, &stats_path);
    assert_eq!(
        calls1, 2,
        "one-shot mid-attempt mutation must take 1 initial live attempt + 1 live retry"
    );
    assert_eq!(
        copies1, 0,
        "single Stale retry must stay in Live mode without copying overlays"
    );
    assert!(
        live_records1 > 0,
        "live retry must read live overlay records, got {live_records1}"
    );
    assert_eq!(live1, cap1, "live retry frame must match captured");

    // 2nd Stale falls back to whole-buffer Captured (copies > 0) and converges.
    let (cap2, _, _, _) =
        run_composition_stale_scenario(OverlaySourceMode::Captured, 2, &stats_path);
    let (live2, calls2, copies2, _) =
        run_composition_stale_scenario(OverlaySourceMode::Live, 2, &stats_path);
    assert_eq!(
        calls2, 3,
        "two consecutive Stales must take initial live + live retry + captured fallback"
    );
    assert!(
        copies2 > 0,
        "second Stale must activate captured fallback (overlay_copies > 0), got {copies2}"
    );
    assert_eq!(live2, cap2, "captured fallback frame must match captured");
}
