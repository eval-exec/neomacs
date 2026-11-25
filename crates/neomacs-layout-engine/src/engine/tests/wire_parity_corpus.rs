//! MILESTONES M2 `wire_parity_real_frames`: evaluator-driven frames survive
//! the NDP/1 display wire exactly.
//!
//! Each scenario builds a real frame the way the engine's own tests do (an
//! evaluator `Context`, a frame, `layout_frame_rust`, then
//! `last_frame_display_state`), sends it through FrameSync and the wire codec
//! as a keyframe, edits it, and sends the delta. After each, the canonical
//! encoding of `materialize(seal(decode(encode(s))))` must equal that of
//! `materialize(seal(s))` with the hit index's text source stripped (D4).
//!
//! Measured sizes are appended to the file `NEOMACS_WIRE_SIZES_LOG` names.

use super::*;
use crate::presentation::spatial::{
    PresentedTextPositionsMode, set_presented_text_positions_mode_for_test,
};
use neomacs_display_protocol::{LiveDisplayFrameId, SealedFramePresentation};
use neomacs_display_wire::present::{FrameSyncDecoder, FrameSyncEncoder, StoreLimits};
use neomacs_display_wire::testing::{
    WireSize, assert_same_materialized, round_trip, without_text_positions,
};
use neovm_core::window::{FrameId, WindowId};

/// One wire session's two ends.
struct Link {
    encoder: FrameSyncEncoder,
    decoder: FrameSyncDecoder,
    sizes: Vec<(&'static str, WireSize)>,
}

impl Link {
    fn new() -> Self {
        Self {
            encoder: FrameSyncEncoder::new(StoreLimits::default()),
            decoder: FrameSyncDecoder::new(StoreLimits::default()),
            sizes: Vec::new(),
        }
    }

    /// Send the engine's last presentation of `frame_id` and check what the
    /// receiver rebuilds.
    fn send(
        &mut self,
        engine: &LayoutEngine,
        frame_id: FrameId,
        step: &'static str,
    ) -> SealedFramePresentation {
        let sealed = engine
            .last_frame_display_state
            .as_ref()
            .expect("a sealed presentation")
            .clone();
        let frame = LiveDisplayFrameId::new(
            std::num::NonZeroU64::new(frame_id.0).expect("Lisp frame ids start at 1"),
        );
        let (size, decoded) = round_trip(&mut self.encoder, &mut self.decoder, frame, &sealed);
        let decoded = decoded
            .unwrap_or_else(|reason| panic!("{step}: the receiver refused the frame: {reason}"));
        let expected = without_text_positions(&sealed).materialize();
        assert_same_materialized(&expected, &decoded.materialize(), step);
        self.sizes.push((step, size));
        decoded
    }
}

fn report(scenario: &str, link: &Link) {
    let line = link
        .sizes
        .iter()
        .map(|(step, size)| format!("{step} {} B in {} messages", size.bytes, size.messages))
        .collect::<Vec<_>>()
        .join("; ");
    println!("wire_parity {scenario}: {line}");
    if let Some(path) = std::env::var_os("NEOMACS_WIRE_SIZES_LOG") {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("open the sizes log");
        writeln!(file, "{scenario}: {line}").expect("append to the sizes log");
    }
}

fn layout(eval: &mut Context, engine: &mut LayoutEngine, frame_id: FrameId) {
    engine.layout_frame_rust(eval, frame_id);
    activate_last_engine_presentation(eval, engine, frame_id);
}

/// Every glyph row of the frame's windows.
fn rows(
    frame: &SealedFramePresentation,
) -> impl Iterator<Item = &neomacs_display_protocol::GlyphRow> {
    frame
        .window_matrices
        .iter()
        .flat_map(|entry| entry.matrix.rows.iter().map(|row| &**row))
        .filter(|row| row.enabled)
}

fn glyphs(
    frame: &SealedFramePresentation,
) -> impl Iterator<Item = &neomacs_display_protocol::Glyph> {
    rows(frame).flat_map(|row| row.glyphs.iter().flatten())
}

/// Keyframe, then one edit as a delta, through one link. `exercises` checks
/// that the keyframe really has the scenario's feature.
fn keyframe_and_edit(
    scenario: &str,
    eval: &mut Context,
    engine: &mut LayoutEngine,
    frame_id: FrameId,
    edit: &str,
    exercises: impl FnOnce(&SealedFramePresentation) -> bool,
) {
    let mut link = Link::new();
    layout(eval, engine, frame_id);
    let keyframe = link.send(engine, frame_id, "keyframe");
    assert!(
        exercises(&keyframe),
        "{scenario}: the frame does not exercise its feature"
    );
    eval.eval_str(edit).expect("the scenario's edit evaluates");
    layout(eval, engine, frame_id);
    link.send(engine, frame_id, "delta");
    report(scenario, &link);
}

fn frame_with(
    text: &str,
    width: u32,
    height: u32,
    setup: &str,
) -> (Context, FrameId, BufferId, WindowId) {
    let (mut eval, frame_id, buffer, window) = incr_editing_frame(text, width, height);
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .expect("frame")
        .window_system = Some(Value::symbol("neomacs"));
    if !setup.is_empty() {
        eval.eval_str(setup)
            .expect("the scenario's setup evaluates");
    }
    (eval, frame_id, buffer, window)
}

const INSERT_AT_START: &str = "(progn (goto-char (point-min)) (insert \"x\"))";

#[test]
fn wire_parity_plain_text() {
    let (mut eval, frame_id, _, _) = frame_with(&"plain text line\n".repeat(40), 800, 600, "");
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    keyframe_and_edit(
        "plain text",
        &mut eval,
        &mut engine,
        frame_id,
        INSERT_AT_START,
        |frame| glyphs(frame).count() > 400,
    );
}

#[test]
fn wire_parity_faces_and_overlays() {
    let (mut eval, frame_id, _, _) = frame_with(
        &"(defun f (a b) (+ a b))\n".repeat(30),
        800,
        600,
        "(progn
           (put-text-property 1 7 'face 'bold)
           (put-text-property 30 60 'face '(:foreground \"red\" :underline t))
           (let ((o (make-overlay 70 120)))
             (overlay-put o 'face '(:background \"#203040\"))
             (overlay-put o 'before-string (propertize \"[ov]\" 'face 'italic)))
           (overlay-put (make-overlay 130 140) 'mouse-face 'highlight))",
    );
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    keyframe_and_edit(
        "faces and overlays",
        &mut eval,
        &mut engine,
        frame_id,
        "(progn (goto-char 3) (insert \"y\"))",
        |frame| {
            let faces: std::collections::BTreeSet<_> =
                glyphs(frame).map(|glyph| glyph.face_id).collect();
            faces.len() >= 4 && rows(frame).any(|row| !row.pointer_appearances().is_empty())
        },
    );
}

#[test]
fn wire_parity_display_strings_and_properties() {
    let (mut eval, frame_id, _, _) = frame_with(
        &"display property source text\n".repeat(30),
        800,
        600,
        "(progn
           (put-text-property 1 8 'display \"SHOWN\")
           (put-text-property 31 33 'display '(space :width 6))
           (put-text-property 61 66 'display '(raise 0.3))
           (put-text-property 91 92 'display (propertize \"*\" 'face 'bold)))",
    );
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    keyframe_and_edit(
        "display strings and properties",
        &mut eval,
        &mut engine,
        frame_id,
        "(progn (goto-char (point-max)) (insert \"tail\"))",
        |frame| {
            glyphs(frame).any(|glyph| {
                matches!(
                    glyph.provenance,
                    neomacs_display_protocol::GlyphProvenance::Str { .. }
                )
            }) && glyphs(frame).any(|glyph| {
                matches!(
                    glyph.glyph_type,
                    neomacs_display_protocol::GlyphType::Stretch { .. }
                )
            })
        },
    );
}

#[test]
fn wire_parity_bidi_paragraph() {
    let (mut eval, frame_id, _, _) = frame_with(
        &"שלום עולם hello 123 مرحبا\n".repeat(20),
        800,
        600,
        "(setq bidi-paragraph-direction 'right-to-left)",
    );
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    keyframe_and_edit(
        "bidi paragraph",
        &mut eval,
        &mut engine,
        frame_id,
        INSERT_AT_START,
        |frame| {
            rows(frame).any(|row| row.reversed_p)
                && glyphs(frame).any(|glyph| glyph.bidi_level % 2 == 1)
        },
    );
}

#[test]
fn wire_parity_wide_and_composed_characters() {
    let (mut eval, frame_id, _, _) = frame_with(
        &"日本語のテキスト e\u{301} n\u{303} 한국어 ２２\n".repeat(20),
        800,
        600,
        "",
    );
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    keyframe_and_edit(
        "wide and composed characters",
        &mut eval,
        &mut engine,
        frame_id,
        "(progn (goto-char (point-min)) (insert \"字\"))",
        |frame| glyphs(frame).any(|glyph| glyph.wide) && glyphs(frame).any(|glyph| glyph.padding),
    );
}

#[test]
fn wire_parity_mode_header_and_tab_lines() {
    let (mut eval, frame_id, _, _) = frame_with(
        &"buffer with chrome lines\n".repeat(30),
        800,
        600,
        "(progn
           (setq header-line-format \"HEADER %b\")
           (setq tab-line-format '(\"TAB-ONE \" (:propertize \"TAB-TWO\" face bold)))
           (setq mode-line-format '(\"%b \" (:propertize \"MODE\" face italic) \" %l\")))",
    );
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    keyframe_and_edit(
        "mode, header and tab lines",
        &mut eval,
        &mut engine,
        frame_id,
        "(progn (setq header-line-format \"HEADER CHANGED\") (goto-char 5) (insert \"z\"))",
        |frame| {
            use neomacs_display_protocol::GlyphRowRole;
            [
                GlyphRowRole::HeaderLine,
                GlyphRowRole::TabLine,
                GlyphRowRole::ModeLine,
            ]
            .iter()
            .all(|role| rows(frame).any(|row| row.role == *role))
        },
    );
}

#[test]
fn wire_parity_margins_and_fringes() {
    let (mut eval, frame_id, _, _) = frame_with(
        &"margin and fringe text\n".repeat(30),
        800,
        600,
        "(progn
           (setq left-margin-width 3 right-margin-width 2)
           (put-text-property 1 2 'display '((margin left-margin) \"L>\"))
           (put-text-property 24 25 'display '((margin right-margin) \"<R\"))
           (put-text-property 47 48 'display '(left-fringe right-arrow))
           (setq indicate-empty-lines t))",
    );
    eval.eval_str("(set-window-margins (selected-window) 3 2)")
        .expect("set margins");
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    keyframe_and_edit(
        "margins and fringes",
        &mut eval,
        &mut engine,
        frame_id,
        INSERT_AT_START,
        |frame| {
            rows(frame).any(|row| !row.glyphs[0].is_empty())
                && rows(frame).any(|row| !row.glyphs[2].is_empty())
                && rows(frame).any(|row| row.left_fringe_bitmap.is_some())
        },
    );
}

#[test]
fn wire_parity_line_numbers() {
    let (mut eval, frame_id, _, _) = frame_with(
        &"numbered line\n".repeat(60),
        800,
        600,
        "(setq display-line-numbers t)",
    );
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    keyframe_and_edit(
        "line numbers",
        &mut eval,
        &mut engine,
        frame_id,
        "(progn (goto-char (point-min)) (insert \"new line\\n\"))",
        |frame| {
            frame
                .window_infos
                .iter()
                .any(|info| info.line_number_field.is_some())
        },
    );
}

#[test]
fn wire_parity_truncation_and_continuation() {
    let long = "a long line that is much wider than the window ".repeat(6);
    let (mut eval, frame_id, _, _) =
        frame_with(&format!("{long}\nshort\n").repeat(8), 400, 400, "");
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    let mut link = Link::new();
    // Continued lines first, then the same buffer truncated, then an edit.
    layout(&mut eval, &mut engine, frame_id);
    let continued = link.send(&engine, frame_id, "continued");
    assert!(rows(&continued).any(|row| row.continued), "continued rows");
    eval.eval_str("(setq truncate-lines t)").expect("truncate");
    layout(&mut eval, &mut engine, frame_id);
    let truncated = link.send(&engine, frame_id, "truncated");
    assert!(
        rows(&truncated).any(|row| row.right_fringe_bitmap.is_some()),
        "truncated rows show their fringe arrow"
    );
    eval.eval_str(INSERT_AT_START).expect("edit");
    layout(&mut eval, &mut engine, frame_id);
    link.send(&engine, frame_id, "delta");
    report("truncation and continuation", &link);
}

#[test]
fn wire_parity_split_windows() {
    let (mut eval, frame_id, buffer, window) =
        frame_with(&"split window text\n".repeat(60), 800, 600, "");
    let right = eval
        .frame_manager_mut()
        .split_window(
            frame_id,
            window,
            neovm_core::window::SplitDirection::Horizontal,
            buffer,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .expect("split right");
    eval.frame_manager_mut()
        .split_window(
            frame_id,
            right,
            neovm_core::window::SplitDirection::Vertical,
            buffer,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .expect("split below");
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    keyframe_and_edit(
        "split windows",
        &mut eval,
        &mut engine,
        frame_id,
        INSERT_AT_START,
        |frame| frame.window_matrices.len() >= 3 && !frame.borders.is_empty(),
    );
}

#[test]
fn wire_parity_child_frame() {
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    let parent_buffer = eval
        .buffer_manager()
        .current_buffer()
        .expect("parent buffer")
        .id();
    let child_buffer = eval.buffer_manager_mut().create_buffer("*child*");
    eval.buffer_manager_mut()
        .get_mut(child_buffer)
        .expect("child buffer")
        .insert("child frame line one\nchild frame line two\n");
    let parent = eval
        .frame_manager_mut()
        .create_frame("wire-parent", 1000, 700, parent_buffer);
    let child = eval
        .frame_manager_mut()
        .create_frame("wire-child", 400, 120, child_buffer);
    eval.create_window_markers_for_root(parent, parent_buffer);
    eval.create_window_markers_for_root(child, child_buffer);
    for frame in [parent, child] {
        eval.frame_manager_mut()
            .get_mut(frame)
            .expect("frame")
            .window_system = Some(Value::symbol("neomacs"));
    }
    eval.frame_manager_mut()
        .get_mut(child)
        .expect("child frame")
        .parent_frame = Value::make_frame(parent.0);
    assert!(eval.frame_manager_mut().select_frame(parent));
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    let mut link = Link::new();
    layout(&mut eval, &mut engine, parent);
    link.send(&engine, parent, "parent keyframe");
    layout(&mut eval, &mut engine, child);
    let child_presentation = link.send(&engine, child, "child keyframe");
    assert!(
        child_presentation.frame_placement.parent().is_some(),
        "the child frame is placed in its parent"
    );
    eval.buffer_manager_mut()
        .get_mut(child_buffer)
        .expect("child buffer")
        .insert("x");
    layout(&mut eval, &mut engine, child);
    link.send(&engine, child, "child delta");
    report("child frame", &link);
}

fn await_coverage(engine: &mut LayoutEngine) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !engine
        .scroll_coverage
        .drain(&mut engine.prepared_viewports)
        .expect("coverage drains")
    {
        assert!(std::time::Instant::now() < deadline, "coverage arrives");
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
fn wire_parity_scroll_coverage_with_offsets() {
    let line = "ordinary offscreen text\n";
    let (mut eval, frame_id, _, window) = frame_with(&line.repeat(300), 800, 600, "");
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    let mut link = Link::new();
    layout(&mut eval, &mut engine, frame_id);
    link.send(&engine, frame_id, "keyframe");
    let window_id = DisplayWindowId::new(window.0 as i64);
    let rows: Vec<_> = engine.retained_window_matrices[&window_id]
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
        .map(|row| row.start_charpos)
        .collect();
    let target = rows[rows.len() - 2];
    engine
        .request_scroll_coverage(&eval, frame_id, window, CharPos0::new(target))
        .expect("coverage request");
    await_coverage(&mut engine);
    layout(&mut eval, &mut engine, frame_id);
    let with_coverage = link.send(&engine, frame_id, "with coverage");
    assert_eq!(
        with_coverage.scroll_coverage.len(),
        1,
        "the frame carries scroll coverage"
    );
    assert_eq!(
        with_coverage.materialize().scroll_surfaces.len(),
        1,
        "the rebuilt coverage materializes"
    );
    report("scroll coverage with offsets", &link);
}

struct ModeGuard;

impl Drop for ModeGuard {
    fn drop(&mut self) {
        set_presented_text_positions_mode_for_test(None);
    }
}

#[test]
fn wire_parity_lazy_text_hit_source() {
    let _guard = ModeGuard;
    set_presented_text_positions_mode_for_test(Some(PresentedTextPositionsMode::Lazy));
    let text = "short\n\nx\n(defun a-longer-line (arg) (list arg arg arg))\n\tindented\n".repeat(8);
    let (mut eval, frame_id, _, _) = frame_with(&text, 800, 600, "");
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    layout(&mut eval, &mut engine, frame_id);
    let sealed = engine
        .last_frame_display_state
        .as_ref()
        .expect("sealed")
        .clone();
    assert!(
        sealed.presented_hit_index.text_deferred(),
        "the frame's text positions come from a lazy source"
    );
    let mut link = Link::new();
    let decoded = link.send(&engine, frame_id, "keyframe");
    assert!(
        decoded.presented_hit_index.text_positions().is_empty(),
        "no text position crosses the wire (D4)"
    );
    eval.eval_str("(forward-char 1)").expect("move point");
    layout(&mut eval, &mut engine, frame_id);
    link.send(&engine, frame_id, "delta");
    report("lazy text hit source", &link);
}
