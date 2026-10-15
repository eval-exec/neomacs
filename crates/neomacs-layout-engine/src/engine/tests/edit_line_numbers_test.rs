use super::*;
use neomacs_display_protocol::glyph_matrix::{FrameDisplayState, GlyphArea, GlyphType};
use neovm_core::window::FrameId;

fn redisplay(engine: &mut LayoutEngine, ctx: &mut Context, frame: FrameId) -> FrameDisplayState {
    match engine.redisplay_frame_attempt(ctx, frame) {
        FrameLayoutAttempt::Prepared(state) => state.into_state(),
        FrameLayoutAttempt::Aborted => panic!("CPU frame aborted"),
    }
}

fn gutter(
    state: &FrameDisplayState,
    window: neovm_core::window::WindowId,
    count: usize,
) -> Vec<String> {
    state
        .window_matrices
        .iter()
        .find(|m| m.window_id.get() == window.0 as i64)
        .unwrap()
        .matrix
        .rows
        .iter()
        .filter(|r| {
            r.enabled && r.role == neomacs_display_protocol::frame_glyphs::GlyphRowRole::Text
        })
        .take(count)
        .map(|r| {
            r.glyphs[GlyphArea::Text.index()]
                .iter()
                .filter_map(|g| match &g.glyph_type {
                    GlyphType::Char { ch } => Some(*ch),
                    _ => None,
                })
                .collect::<String>()
                .trim()
                .to_owned()
        })
        .collect()
}
fn materialized_gutter(
    state: &FrameDisplayState,
    window: neovm_core::window::WindowId,
    count: usize,
) -> Vec<String> {
    let mut rows = vec![String::new(); count];
    for glyph in state.materialize().glyphs.iter() {
        if let neomacs_display_protocol::frame_glyphs::FrameGlyph::Char {
            window_id,
            slot_id,
            char: ch,
            ..
        } = glyph
        {
            if window_id.get() == window.0 as i64 && (slot_id.row as usize) < count {
                rows[slot_id.row as usize].push(*ch);
            }
        }
    }
    rows.into_iter().map(|r| r.trim().to_owned()).collect()
}

#[test]
fn display_string_newlines_keep_buffer_numbers_and_current_line_face() {
    let mut ctx = Context::new();
    let buffer = ctx.buffer_manager().current_buffer().unwrap().id();
    let frame = ctx
        .frame_manager_mut()
        .create_frame("display-gutter", 1024, 768, buffer);
    assert!(ctx.frame_manager_mut().select_frame(frame));
    let window = ctx.frame_manager().get(frame).unwrap().selected_window;
    ctx.eval_str(r##"(progn
      (setq mode-line-format nil header-line-format nil tab-line-format nil display-line-numbers t)
      (internal-set-lisp-face-attribute 'line-number :background "#101010" (selected-frame))
      (internal-set-lisp-face-attribute 'line-number-current-line :background "#ffff00" (selected-frame))
      (insert "line 1\nline 2\nline 3\nline 4\nline 5\n")
      (goto-char (point-min))
      (let ((eol (line-end-position)))
        (overlay-put (make-overlay eol (1+ eol)) 'display "  [A]\n  [B]\n"))
      (forward-line 2))"##).unwrap();
    let mut engine = LayoutEngine::new_without_font_metrics();
    let expected = [
        "1 line 1  [A]",
        "[B]",
        "2 line 2",
        "3 line 3",
        "4 line 4",
        "5 line 5",
    ];
    // Repeat to cover retained output as well as the initial complete layout.
    for _ in 0..2 {
        let state = redisplay(&mut engine, &mut ctx, frame);
        assert_eq!(gutter(&state, window, 6), expected);
        assert_eq!(materialized_gutter(&state, window, 6), expected);
        let rows = &state
            .window_matrices
            .iter()
            .find(|m| m.window_id.get() == window.0 as i64)
            .unwrap()
            .matrix
            .rows;
        let backgrounds: Vec<_> = rows
            .iter()
            .take(6)
            .map(|row| state.faces[&row.glyphs[GlyphArea::Text.index()][0].face_id].background)
            .collect();
        assert_eq!(
            backgrounds[3],
            neomacs_display_protocol::types::Color::rgb(1.0, 1.0, 0.0)
        );
        assert_ne!(backgrounds[2], backgrounds[3]);
    }
}

#[test]
fn display_replacements_count_hidden_buffer_lines_for_every_replacement_kind() {
    for replacement in [r#""[hidden] ""#, "'(space :width 2)", r#""""#] {
        let mut ctx = Context::new();
        let buffer = ctx.buffer_manager().current_buffer().unwrap().id();
        let frame = ctx
            .frame_manager_mut()
            .create_frame("hidden-gutter", 1024, 768, buffer);
        assert!(ctx.frame_manager_mut().select_frame(frame));
        let window = ctx.frame_manager().get(frame).unwrap().selected_window;
        ctx.eval_str(&format!(r#"(progn
          (setq mode-line-format nil header-line-format nil tab-line-format nil display-line-numbers t)
          (insert "line 1\nline 2\nline 3\nline 4\nline 5\n")
          (goto-char (point-min))
          (forward-line 1)
          (put-text-property (point) (progn (forward-line 2) (point)) 'display {replacement})
          (goto-char (point-max)) (forward-line -1))"#)).unwrap();
        let mut engine = LayoutEngine::new_without_font_metrics();
        let state = redisplay(&mut engine, &mut ctx, frame);
        let rows = gutter(&state, window, 3);
        let numbers: Vec<_> = rows
            .iter()
            .map(|row| row.split_whitespace().next().unwrap())
            .collect();
        assert_eq!(
            numbers,
            ["1", "2", "5"],
            "replacement {replacement}: {rows:?}"
        );
    }
}

#[test]
fn display_table_newline_glyphs_still_advance_buffer_line_numbers() {
    let mut ctx = Context::new();
    let buffer = ctx.buffer_manager().current_buffer().unwrap().id();
    let frame = ctx
        .frame_manager_mut()
        .create_frame("table-gutter", 1024, 768, buffer);
    assert!(ctx.frame_manager_mut().select_frame(frame));
    let window = ctx.frame_manager().get(frame).unwrap().selected_window;
    ctx.eval_str(
        r#"(progn
      (setq mode-line-format nil header-line-format nil tab-line-format nil display-line-numbers t)
      (insert "a\nb\nc\n")
      (setq buffer-display-table (make-char-table 'display-table nil))
      (aset buffer-display-table ?\n [?$ ?\n])
      (goto-char (point-min)))"#,
    )
    .unwrap();
    let mut engine = LayoutEngine::new_without_font_metrics();
    let state = redisplay(&mut engine, &mut ctx, frame);
    assert_eq!(gutter(&state, window, 3), ["1 a$", "2 b$", "3 c$"]);
}

fn newline_edits(mode: &str, count: usize) {
    let mut ctx = Context::new();
    ctx.eval_str("(neomacs-set-buffer-text-backend 'gap-buffer)")
        .unwrap();
    let buffer = ctx.buffer_manager().current_buffer().unwrap().id();
    let frame = ctx
        .frame_manager_mut()
        .create_frame("gutter-cpu", 1024, 768, buffer);
    let mini = ctx.buffers.create_buffer(" *Minibuf-0*");
    ctx.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .minibuffer_leaf
        .as_mut()
        .unwrap()
        .set_buffer(mini);
    let window = ctx.frame_manager().get(frame).unwrap().selected_window;
    {
        let frame = ctx.frame_manager_mut().get_mut(frame).unwrap();
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.install_gnu_gui_default_parameters();
    }
    assert!(ctx.frame_manager_mut().select_frame(frame));
    ctx.eval_str("(internal-set-lisp-face-attribute 'default :height 120 (selected-frame))")
        .unwrap();
    ctx.eval_str("(setq mode-line-format nil header-line-format nil tab-line-format nil display-line-numbers-current-absolute t)").unwrap();
    ctx.eval_str(&format!("(setq display-line-numbers {mode})"))
        .unwrap();
    let mut engine = LayoutEngine::new_without_font_metrics();
    redisplay(&mut engine, &mut ctx, frame);
    for edit in 1..=count {
        ctx.eval_str("(insert \"\\n\")").unwrap();
        let incremental = redisplay(&mut engine, &mut ctx, frame);
        let stats = engine.last_layout_stats();
        let mut fresh = LayoutEngine::new_without_font_metrics();
        let full = redisplay(&mut fresh, &mut ctx, frame);
        let got = gutter(&incremental, window, edit + 1);
        assert_eq!(got, materialized_gutter(&incremental, window, edit + 1));
        assert_eq!(
            got,
            gutter(&full, window, edit + 1),
            "{mode}, newline {edit}"
        );
        assert_eq!(got, materialized_gutter(&full, window, edit + 1));
        assert_eq!(got[edit], "", "empty EOB prefix stays blank");
        let body = incremental
            .window_matrices
            .iter()
            .find(|m| m.window_id.get() == window.0 as i64)
            .unwrap();
        let cursor_row = &body.matrix.rows[edit];
        assert!(
            cursor_row.cursor_col.is_some(),
            "caret remains at empty EOB"
        );
        assert_eq!(
            cursor_row.cursor_col,
            full.window_matrices
                .iter()
                .find(|m| m.window_id.get() == window.0 as i64)
                .unwrap()
                .matrix
                .rows[edit]
                .cursor_col
        );
        if edit >= 3 {
            if matches!(mode, "'relative" | "'visual") {
                assert_eq!(
                    stats.edit_windows, 0,
                    "whole-window line numbers decline localized replay"
                );
            } else {
                assert!(
                    stats.edit_windows > 0,
                    "Off/Absolute keep localized edit reuse"
                );
            }
        }
    }
    let stable = redisplay(&mut engine, &mut ctx, frame);
    assert!(
        engine.last_layout_stats().reused_rows > 0,
        "unchanged-frame reuse remains available"
    );
    assert_eq!(
        gutter(&stable, window, count + 1),
        materialized_gutter(&stable, window, count + 1)
    );
}

#[test]
fn relative_newline_edits_keep_incremental_full_and_materialized_gutters_coherent() {
    for count in [2, 3, 30] {
        newline_edits("'relative", count);
    }
}
#[test]
fn visual_newline_edits_decline_localized_replay() {
    newline_edits("'visual", 3);
}
#[test]
fn absolute_and_off_newline_edits_preserve_localized_reuse() {
    for mode in ["t", "nil"] {
        newline_edits(mode, 3);
    }
}
