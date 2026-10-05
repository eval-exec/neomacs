//! GNU Emacs 31.1 parity for unibyte/multibyte string and buffer conversion.
//!
//! `string-as-multibyte` rejects a `C0`/`C1` raw-byte pair. `set-buffer-multibyte`
//! keeps a valid UTF-8 sequence only when the flag is exactly `t`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_gdh_unibyte_multibyte_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  (let ((s (string-as-multibyte (unibyte-string 192 128))))
    (list (length s) (string-bytes s) (append s nil)))
  (let ((s (string-as-multibyte (unibyte-string 193 191))))
    (list (length s) (string-bytes s) (append s nil)))
  (let ((s (string-as-multibyte (unibyte-string 195 169))))
    (list (length s) (string-bytes s) (append s nil)))
  (let ((s (string-to-multibyte (unibyte-string 195 169))))
    (list (length s) (string-bytes s) (append s nil)))
  (append (string-as-multibyte (unibyte-string 194 128)) nil)
  (let ((s (unibyte-string 65 66)))
    (eq (string-make-multibyte s) s))
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert (unibyte-string 192 128))
    (goto-char 2)
    (let ((m (point-marker)))
      (set-buffer-multibyte t)
      (list (point) (position-bytes (point)) (point-max)
            (position-bytes (point-max)) (buffer-size)
            (append (buffer-string) nil) (marker-position m))))
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert (unibyte-string 195 169))
    (goto-char 2)
    (set-buffer-multibyte t)
    (list (buffer-size) (append (buffer-string) nil)
          (point) (position-bytes (point))))
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert (unibyte-string 195 169))
    (goto-char 2)
    (set-buffer-multibyte 'foo)
    (list (buffer-size) (append (buffer-string) nil) (point)))
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert (unibyte-string 195 169))
    (goto-char 2)
    (set-buffer-multibyte 1)
    (list (buffer-size) (append (buffer-string) nil)))
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert (unibyte-string 195 169))
    (goto-char 2)
    (set-buffer-multibyte "t")
    (list (buffer-size) (append (buffer-string) nil)))
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert (unibyte-string 195 169))
    (goto-char 2)
    (set-buffer-multibyte 'to)
    (list (buffer-size) (append (buffer-string) nil)))
  (with-temp-buffer
    (set-buffer-multibyte t)
    (insert (string 233))
    (goto-char 2)
    (list (set-buffer-multibyte 'foo)
          enable-multibyte-characters
          (point)
          (append (buffer-string) nil)))
  (with-temp-buffer
    (insert "abc")
    (narrow-to-region 2 3)
    (condition-case err
        (set-buffer-multibyte nil)
      (error (error-message-string err)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((2 4 (4194240 4194176)) (2 4 (4194241 4194239)) (1 2 (233)) (2 4 (4194243 4194217)) (128) t (2 3 3 5 2 (4194240 4194176) 2) (1 (233) 2 3) (2 (4194243 4194217) 2) (2 (4194243 4194217)) (2 (4194243 4194217)) (2 (4194243 4194217)) (foo t 2 (233)) \"Changing multibyteness in a narrowed buffer\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
