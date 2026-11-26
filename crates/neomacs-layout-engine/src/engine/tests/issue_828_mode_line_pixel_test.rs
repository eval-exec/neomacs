//! Issue #828: a mode-line `:eval` that measures `string-pixel-width` must
//! publish the enclosing frame.
//!
//! GNU `Fbuffer_text_pixel_size` (src/xdisp.c, emacs-31.1) swaps the window
//! onto the measured buffer with `wset_buffer`, walks that text with its own
//! iterator, then `unbind_to` restores the window and puts `base_line_pos`
//! and `window_end_valid` back. The comment there says this restore matters
//! when the call comes from a `:eval` form in the middle of redisplay.
//! A 1x1 cell reports `(string-pixel-width "hello")` as 5.

use std::cell::Cell;
use std::rc::Rc;

use super::*;
use neovm_core::window::WindowLayoutQueryOutcome;

fn rendered_body_text(engine: &LayoutEngine) -> String {
    engine
        .last_frame_display_state
        .as_ref()
        .map(|state| {
            state
                .window_matrices
                .iter()
                .flat_map(|window| window.matrix.rows.iter())
                .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
                .flat_map(|row| row.glyphs[1].iter())
                .filter_map(|glyph| match &glyph.glyph_type {
                    neomacs_display_protocol::glyph_matrix::GlyphType::Char { ch } => Some(*ch),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn mode_line_string_pixel_width_publishes_gnu_width() {
    let (mut eval, frame_id, buf, window) = incr_editing_frame("line one\nline two\n", 80, 24);
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 1.0;
        frame.set_window_system(None);
        frame.set_window_layout_text_size(80, 24);
        if let Some(mini) = frame.minibuffer_leaf.as_mut() {
            let mut bounds = *mini.bounds();
            bounds.height = 1.0;
            mini.set_bounds(bounds);
        }
    }
    // Production redisplay owns one engine. The measurement takes the other,
    // and a second entry while that engine is borrowed is LayoutBusy.
    let query_calls = Rc::new(Cell::new(0u32));
    let query_outcome = Rc::new(Cell::new("none"));
    let query_engine = Rc::new(std::cell::RefCell::new(
        LayoutEngine::new_without_font_metrics(),
    ));
    eval.install_window_layout_query({
        let query_calls = Rc::clone(&query_calls);
        let query_outcome = Rc::clone(&query_outcome);
        let query_engine = Rc::clone(&query_engine);
        move |eval, frame, window, scope| {
            query_calls.set(query_calls.get().saturating_add(1));
            let Ok(mut engine) = query_engine.try_borrow_mut() else {
                query_outcome.set("busy");
                return WindowLayoutQueryOutcome::LayoutBusy;
            };
            match engine.query_window_layout(eval, frame, window, scope) {
                Ok(query) => {
                    query_outcome.set("ready");
                    WindowLayoutQueryOutcome::Ready(query)
                }
                Err(error) => {
                    query_outcome.set("failed");
                    WindowLayoutQueryOutcome::Failed(error)
                }
            }
        }
    })
    .expect("install the layout-query handler");

    // The shown buffer carries the canonical empty string as `line-prefix`
    // and `wrap-prefix`, the same objects `work-buffer--prepare-pixelwise`
    // installs (`subr-x.el`). The body walk observes that string. The `:eval`
    // returns a fixnum, which the display walker drops, so the formatted
    // mode line is empty. Clearing that string's absent interval tree must
    // not move `source_capture`. Align the window point first, the way the
    // command loop does.
    eval.eval_str(
        r#"(progn
            (put-text-property (point-min) (point-max) 'line-prefix "")
            (put-text-property (point-min) (point-max) 'wrap-prefix "")
            (goto-char 4)
            (set-window-point nil (point)))"#,
    )
    .expect("show the canonical empty string on the buffer the body walks");

    // A bare context has no `defun` or `with-work-buffer`. This is the
    // `string-pixel-width` body (`subr-x.el`): one reused work buffer,
    // `insert` under `deactivate-mark`, the pixel-prep text properties,
    // `buffer-text-pixel-size`, then `work-buffer--release`. `set-default`
    // is what `setq-default` expands to, so a new work buffer inherits the
    // meter. The form returns the width, and also stores it.
    eval.eval_str(
        r#"(progn
            (setq neo-828-seen nil)
            (setq neo-828-seq 0)
            (setq neo-828-work (get-buffer-create " *neo-828-px*"))
            (set-default 'mode-line-format
              '("%e"
                (:eval (let ((origin (current-buffer))
                             (buf neo-828-work))
                         (setq neo-828-seq (1+ neo-828-seq))
                         (unwind-protect
                             (progn
                               (set-buffer buf)
                               (erase-buffer)
                               (let (deactivate-mark)
                                 (insert "hello"))
                               (add-text-properties
                                (point-min) (point-max)
                                '(display-line-numbers-disable t
                                  line-prefix ""
                                  wrap-prefix ""))
                               (setq neo-828-seen
                                     (car (buffer-text-pixel-size nil nil t)))
                               neo-828-seen)
                           (progn
                             (set-buffer buf)
                             (erase-buffer)
                             (delete-all-overlays)
                             (kill-all-local-variables t)
                             (set-buffer origin)))))))
            (setq mode-line-format (default-value 'mode-line-format)))"#,
    )
    .expect("install the mode-line meter");

    let mut engine = LayoutEngine::new_without_font_metrics();
    engine.layout_frame_rust(&mut eval, frame_id);
    let published = engine.last_frame_display_state.is_some();
    let rendered = rendered_mode_line_text(&engine);
    let body = rendered_body_text(&engine);
    engine.layout_frame_rust(&mut eval, frame_id);
    let published_again = engine.last_frame_display_state.is_some();

    let seen = eval
        .eval_str("neo-828-seen")
        .expect("read the captured width");
    let seq = eval.eval_str("neo-828-seq").expect("read the meter depth");
    let shown = eval
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.find_window(window))
        .and_then(|window| window.buffer_id());
    let report = format!(
        "published={published} again={published_again} calls={} outcome={} seen={seen:?} seq={seq:?} shown={shown:?} body={body:?} mode_line={rendered:?}",
        query_calls.get(),
        query_outcome.get(),
    );
    assert!(
        published && published_again,
        "the frame must publish after a mode-line pixel measurement; {report}"
    );
    assert!(
        query_calls.get() > 0,
        "the measurement must enter the layout query; {report}"
    );
    assert_eq!(
        seen,
        Value::fixnum(5),
        "GNU 31.1 batch/TTY answers 5 for (string-pixel-width \"hello\") on a 1x1 cell; {report}"
    );
    assert_eq!(
        shown,
        Some(buf),
        "the published window must still show the original buffer; {report}"
    );
    assert!(
        body.contains("line"),
        "the published body must still be the original text; {report}"
    );
}
