use crate::emacs_core::format_eval_result;

/// GNU's display iterator elides invisible runs before laying them out
/// (`handle_invisible_prop`, src/xdisp.c), so `buffer-text-pixel-size` counts
/// neither the hidden characters nor their columns; a `(PROP . t)` element in
/// `buffer-invisibility-spec` contributes one ellipsis glyph instead.
///
/// The shared org Lisp depends on this: `org-string-width` measures
/// `org-clocktable` cells with bracketed links whose destination text is
/// fontified `invisible`, so a measurement that counted the raw markup would
/// widen every clocktable column to the link text.
#[test]
fn buffer_text_pixel_size_elides_invisible_text_and_overlays() {
    let mut ev = crate::test_utils::runtime_startup_context();
    let result = ev.eval_str(
        r#"(progn
             (defun tm-inv-text (spec)
               (with-temp-buffer
                 (insert "abcdef")
                 (put-text-property 2 5 'invisible 'inv)
                 (setq-local buffer-invisibility-spec spec)
                 (car (buffer-text-pixel-size nil nil t))))
             (defun tm-inv-overlay (spec)
               (with-temp-buffer
                 (insert "abcdef")
                 (let ((ov (make-overlay 2 5)))
                   (overlay-put ov 'invisible 'inv))
                 (setq-local buffer-invisibility-spec spec)
                 (car (buffer-text-pixel-size nil nil t))))
             (vector (tm-inv-text t)
                     (tm-inv-text '((inv . t)))
                     (tm-inv-text '(inv))
                     (tm-inv-overlay t)
                     (tm-inv-overlay '((inv . t)))))"#,
    );
    // GNU 31.1: text t -> 3 (aef), text ellipsis -> 6 (a...ef),
    // text listed -> 3, overlay t -> 3, overlay ellipsis -> 6.
    assert_eq!(format_eval_result(&result), "OK [3 6 3 3 6]");
}
