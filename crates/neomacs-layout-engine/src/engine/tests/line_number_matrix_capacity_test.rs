//! GNU keeps the desired matrix allocation until redisplay reallocates it.
use super::*;
use crate::font::resolver::FontEntityQuery;
use crate::font_backend::FontFamilyName;
use neomacs_display_protocol::glyph_matrix_capacity::required_gui_glyph_matrix_rows;
use neomacs_display_protocol::{GraphicalBackend, GraphicalDisplayIdentity};
use neovm_core::window::FrameDisplayIdentity;

fn redisplay(
    engine: &mut LayoutEngine,
    ctx: &mut Context,
    frame: neovm_core::window::FrameId,
) -> neomacs_display_protocol::glyph_matrix::FrameDisplayState {
    match engine.redisplay_frame_attempt(ctx, frame) {
        FrameLayoutAttempt::Prepared(state) => state.into_state(),
        FrameLayoutAttempt::Aborted => panic!("the real GUI frame must prepare successfully"),
    }
}

#[test]
fn unused_font_open_keeps_width_until_desired_matrix_reallocation() {
    // GNU 31.1/a360: xdisp.c uses desired_matrix->nrows for this getter.
    // The attested linux-gtk3 temporal probe observed widths 2, 2, 3 after
    // initial redisplay, an unused small font-info, and explicit redisplay.
    let text = "body line\n".repeat(1100);
    let (mut ctx, frame_id, buffer_id, window_id) = incr_editing_frame(&text, 800, 800);
    realize_test_gui_frame(&mut ctx, frame_id);
    let identity = GraphicalDisplayIdentity::anonymous_connection(GraphicalBackend::X11);
    ctx.frame_manager_mut()
        .get_mut(frame_id)
        .unwrap()
        .set_display_identity(FrameDisplayIdentity::Graphical(identity.clone()));
    ctx.eval_str(
        r#"(progn
      (setq mode-line-format nil header-line-format nil tab-line-format nil
            display-line-numbers t display-line-numbers-width nil
            display-line-numbers-widen nil display-line-numbers-offset 0
            display-line-numbers-current-absolute t)
      (internal-set-lisp-face-attribute 'default :family "Noto Sans Mono" (selected-frame))
      (internal-set-lisp-face-attribute 'default :height 120 (selected-frame))
      (goto-char (point-min)))"#,
    )
    .unwrap();
    install_test_gnu_line_number_face_inheritance(&mut ctx);
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.set_display_identity(identity.clone());
    let normal =
        engine
            .font_metrics
            .as_mut()
            .unwrap()
            .font_metrics("Noto Sans Mono", 400, false, 16.0);
    assert!(normal.ascent > 0.0 && normal.descent >= 0.0);
    assert!(identity.smallest_opened_font_height() > 1);

    let initial = redisplay(&mut engine, &mut ctx, frame_id);
    let initial_rows = initial
        .window_matrices
        .iter()
        .find(|matrix| matrix.window_id.get() == window_id.0 as i64)
        .expect("selected window has a real desired matrix")
        .matrix
        .nrows;
    assert!(
        (10..100).contains(&initial_rows),
        "normal matrix rows: {initial_rows}"
    );
    assert_eq!(
        ctx.eval_str("(line-number-display-width)").unwrap(),
        Value::fixnum(2)
    );
    let normal_min = identity.smallest_opened_font_height();
    let body_before = ctx.eval_str("(window-body-height nil t)").unwrap();
    assert_eq!(ctx.eval_str("(point)").unwrap(), Value::fixnum(1));
    let (default_before, scale) = {
        let frame = ctx.frame_manager().get(frame_id).unwrap();
        let window = frame.find_window(window_id).unwrap();
        assert_eq!(window.window_start(), Some(LispCharPos1::ONE));
        assert_eq!(window.buffer_id(), Some(buffer_id));
        (
            (frame.font_pixel_size, frame.char_width, frame.char_height),
            window.display().unwrap().glyph_matrix_row_scale.current(),
        )
    };

    // Open a real font entity without assigning it to any face or text.
    let query = FontEntityQuery::new(Some(FontFamilyName::new("Noto Sans Mono").unwrap()));
    let small = engine
        .font_metrics
        .as_ref()
        .unwrap()
        .open_font_entity(&query, 2)
        .expect("the immutable test catalog must open its small primary font");
    assert!(small.metrics.ascent + small.metrics.descent > 0);
    let small_min = identity.smallest_opened_font_height();
    assert!(
        small_min < normal_min,
        "normal={normal_min}, small={small_min}"
    );
    let body_after = ctx.eval_str("(window-body-height nil t)").unwrap();
    assert_eq!(
        body_after, body_before,
        "opening an unused font cannot resize the body"
    );
    let body_pixels = body_after
        .as_int()
        .expect("pixel body height is an integer");
    let newly_required = required_gui_glyph_matrix_rows(body_pixels, 0, small_min, scale);
    assert!(
        newly_required >= 100,
        "small-font required matrix rows: {newly_required}"
    );
    let frame = ctx.frame_manager().get(frame_id).unwrap();
    assert_eq!(
        (frame.font_pixel_size, frame.char_width, frame.char_height),
        default_before,
        "opening an unused font cannot replace the frame's default font"
    );

    // This is the temporal regression: current font minima alone must not
    // replace the desired allocation before redisplay has run.
    assert_eq!(
        ctx.eval_str("(line-number-display-width)").unwrap(),
        Value::fixnum(2),
        "unused font opening must retain the previously allocated width until redisplay"
    );
    let reallocated = redisplay(&mut engine, &mut ctx, frame_id);
    let reallocated_rows = reallocated
        .window_matrices
        .iter()
        .find(|matrix| matrix.window_id.get() == window_id.0 as i64)
        .unwrap()
        .matrix
        .nrows;
    assert!(
        reallocated_rows >= 100,
        "reallocated matrix rows: {reallocated_rows}"
    );
    assert_eq!(
        ctx.eval_str("(line-number-display-width)").unwrap(),
        Value::fixnum(3)
    );
}

#[test]
fn hidden_basic_tab_line_font_opens_before_gui_matrix_allocation() {
    // GNU 31.1/a360 xfaces.c:5944 realizes every basic GUI face, including
    // invisible tab-line faces, with remapping disabled. faces.el's tab-line
    // inherits variable-pitch at relative height 0.9. Their real font openings
    // lower the connection minimum before dispnew.c allocates desired rows.
    let text = "body line\n".repeat(1100);
    let (mut ctx, frame_id, buffer_id, window_id) = incr_editing_frame(&text, 800, 800);
    realize_test_gui_frame(&mut ctx, frame_id);
    let identity = GraphicalDisplayIdentity::anonymous_connection(GraphicalBackend::X11);
    ctx.frame_manager_mut()
        .get_mut(frame_id)
        .unwrap()
        .set_display_identity(FrameDisplayIdentity::Graphical(identity.clone()));
    ctx.eval_str(
        r#"(progn
      (setq mode-line-format nil header-line-format nil tab-line-format nil
            display-line-numbers t display-line-numbers-width nil
            display-line-numbers-widen nil display-line-numbers-offset 0
            display-line-numbers-current-absolute t)
      (modify-frame-parameters nil '((menu-bar-lines . 0) (tool-bar-lines . 0)
                                     (tab-bar-lines . 0) (compact-bar-lines . 0)))
      (internal-set-lisp-face-attribute 'default :family "Noto Sans Mono" (selected-frame))
      (internal-set-lisp-face-attribute 'default :height 120 (selected-frame))
      (internal-set-lisp-face-attribute 'variable-pitch :family "Noto Sans" (selected-frame))
      (internal-set-lisp-face-attribute 'tab-line :inherit 'variable-pitch (selected-frame))
      (internal-set-lisp-face-attribute 'tab-line :height 1.0 (selected-frame))
      (internal-set-lisp-face-attribute 'tab-line-active :inherit 'tab-line (selected-frame))
      (internal-set-lisp-face-attribute 'tab-line-inactive :inherit 'tab-line (selected-frame))
      (goto-char (point-min)))"#,
    )
    .unwrap();
    install_test_gnu_line_number_face_inheritance(&mut ctx);
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.set_display_identity(identity.clone());

    // All font openings in the tested connection must come from ordinary
    // redisplay: no explicit font probe or observer seeds its minimum.
    let initial = redisplay(&mut engine, &mut ctx, frame_id);
    assert!(initial.frame_chrome.bands().is_empty());
    let initial_rows = initial
        .window_matrices
        .iter()
        .find(|matrix| matrix.window_id.get() == window_id.0 as i64)
        .expect("selected window has a real desired matrix")
        .matrix
        .nrows;
    assert!(
        (10..100).contains(&initial_rows),
        "initial rows: {initial_rows}"
    );
    let initial_min = identity.smallest_opened_font_height();
    assert!(initial_min > 1, "redisplay must open a real GUI font");

    // Pick the last two-digit case using GNU's zero-based max_lnum rule:
    // (start_line - 1) + desired_nrows - 1 = 99. This selects the stimulus,
    // while the assertions below use the real matrix and public Lisp getter.
    let start_line = 101 - initial_rows;
    ctx.eval_str(&format!(
        "(progn (goto-char (point-min)) (forward-line {}) \
         (set-window-start (selected-window) (point)))",
        start_line - 1
    ))
    .unwrap();
    let baseline = redisplay(&mut engine, &mut ctx, frame_id);
    assert!(baseline.frame_chrome.bands().is_empty());
    assert_eq!(identity.smallest_opened_font_height(), initial_min);
    let baseline_rows = baseline
        .window_matrices
        .iter()
        .find(|matrix| matrix.window_id.get() == window_id.0 as i64)
        .unwrap()
        .matrix
        .nrows;
    assert_eq!(baseline_rows, initial_rows);
    assert_eq!(
        ctx.eval_str("(line-number-display-width)").unwrap(),
        Value::fixnum(2)
    );
    let body_before = ctx.eval_str("(window-body-height nil t)").unwrap();
    let point_before = ctx.eval_str("(point)").unwrap();
    let default_before = {
        let frame = ctx.frame_manager().get(frame_id).unwrap();
        (frame.font_pixel_size, frame.char_width, frame.char_height)
    };

    // The changed font is never displayed: tab-line-format and every frame
    // bar stay nil/zero. Normal GUI basic-face realization must still open it.
    ctx.eval_str("(internal-set-lisp-face-attribute 'tab-line :height 0.9 (selected-frame))")
        .unwrap();
    let smaller = redisplay(&mut engine, &mut ctx, frame_id);
    let smaller_min = identity.smallest_opened_font_height();
    assert!(
        smaller_min < initial_min,
        "hidden basic-face redisplay must lower the opened-font minimum: before={initial_min}, after={smaller_min}"
    );
    let smaller_rows = smaller
        .window_matrices
        .iter()
        .find(|matrix| matrix.window_id.get() == window_id.0 as i64)
        .unwrap()
        .matrix
        .nrows;
    assert!(
        smaller_rows > baseline_rows,
        "hidden basic-face font must grow the real desired matrix: before={baseline_rows}, after={smaller_rows}"
    );
    assert_eq!(
        ctx.eval_str("(line-number-display-width)").unwrap(),
        Value::fixnum(3)
    );
    assert!(smaller.frame_chrome.bands().is_empty());
    assert_eq!(ctx.eval_str("tab-line-format").unwrap(), Value::NIL);
    assert_eq!(
        ctx.eval_str("(window-body-height nil t)").unwrap(),
        body_before
    );
    assert_eq!(ctx.eval_str("(point)").unwrap(), point_before);
    let frame = ctx.frame_manager().get(frame_id).unwrap();
    assert_eq!(
        (frame.font_pixel_size, frame.char_width, frame.char_height),
        default_before,
        "opening a hidden basic font cannot replace the frame's default font"
    );
    let window = frame.find_window(window_id).unwrap();
    assert_eq!(window.buffer_id(), Some(buffer_id));
    assert_eq!(
        window.window_start(),
        Some(
            ctx.buffer_manager()
                .get(buffer_id)
                .unwrap()
                .point_lisp_char_pos()
        )
    );
}
