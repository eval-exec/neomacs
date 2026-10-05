//! GNU `set-buffer-multibyte` position map for `string-as-multibyte`.
//!
//! Flag `t` keeps a valid UTF-8 sequence and splits a `C0`/`C1` raw-byte
//! pair into two eight-bit characters (`buffer.c:2881`, `allow_8bit` false).
//! The numbers below are what GNU Emacs 31.1 printed for the same buffers.

use crate::emacs_core::format_eval_result;

fn convert(bytes: &str, flag: &str, point: i64) -> String {
    let mut eval = crate::emacs_core::eval::Context::new();
    format_eval_result(&eval.eval_str(&format!(
        r#"(progn
             (erase-buffer)
             (set-buffer-multibyte t)
             (set-buffer-multibyte nil)
             (insert (unibyte-string {bytes}))
             (if (and (> (point-max) 1) (< {point} (point-max)))
                 (put-text-property 1 {point} 'gdh 'yes))
             (goto-char {point})
             (let ((m (point-marker))
                   (ov (if (> (point-max) 1) (make-overlay 1 (point-max)) nil)))
               (let ((ret (set-buffer-multibyte {flag})))
                 (list ret
                       enable-multibyte-characters
                       (point)
                       (position-bytes (point))
                       (point-max)
                       (position-bytes (point-max))
                       (buffer-size)
                       (append (buffer-string) nil)
                       (marker-position m)
                       (and ov (overlay-start ov))
                       (and ov (overlay-end ov))
                       (get-text-property 1 'gdh)
                       (car buffer-undo-list)))))"#
    )))
}

#[test]
fn flag_t_splits_c0_80_and_keeps_utf8() {
    crate::test_utils::init_test_tracing();
    // Unibyte C0 80, point on the second byte. GNU: two eight-bit chars,
    // point between them, marker and overlay follow the expanded text.
    assert_eq!(
        convert("192 128", "t", 2),
        "OK (t t 2 3 3 5 2 (4194240 4194176) 2 1 3 yes (apply set-buffer-multibyte nil))"
    );
    assert_eq!(
        convert("192 128", "t", 1),
        "OK (t t 1 1 3 5 2 (4194240 4194176) 1 1 3 nil (apply set-buffer-multibyte nil))"
    );
    assert_eq!(
        convert("192 128", "t", 3),
        "OK (t t 3 5 3 5 2 (4194240 4194176) 3 1 3 nil (apply set-buffer-multibyte nil))"
    );
    // Unibyte C3 A9 is U+00E9. Flag t keeps that one character. Point was
    // between the two bytes, so it advances to the character end.
    assert_eq!(
        convert("195 169", "t", 2),
        "OK (t t 2 3 2 3 1 (233) 2 1 2 yes (apply set-buffer-multibyte nil))"
    );
}

#[test]
fn symbol_to_expands_every_high_byte() {
    crate::test_utils::init_test_tracing();
    // GNU treats the symbol `to` like every non-nil value other than `t`:
    // each high byte becomes an eight-bit character, even inside valid UTF-8.
    assert_eq!(
        convert("195 169", "'to", 2),
        "OK (to t 2 3 3 5 2 (4194243 4194217) 2 1 3 yes (apply set-buffer-multibyte nil))"
    );
}
