//! Complex combo batch 243 — `keyboard-translate` / `function-key-map` /
//! `input-decode-map` / `local-function-key-map` / `key-translation-map`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx243_keyboard_translate_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'keyboard-translate-table)
      (or (null keyboard-translate-table)
          (char-table-p keyboard-translate-table))
      (fboundp 'keyboard-translate))
"##,
        expect,
    );
}

#[test]
fn div_cx243_function_key_map_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'function-key-map)
      (keymapp function-key-map)
      (boundp 'local-function-key-map)
      (keymapp local-function-key-map))
"##,
        expect,
    );
}

#[test]
fn div_cx243_input_decode_map_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'input-decode-map)
      (keymapp input-decode-map))
"##,
        expect,
    );
}

#[test]
fn div_cx243_key_translation_map_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'key-translation-map)
      (keymapp key-translation-map))
"##,
        expect,
    );
}

#[test]
fn div_cx243_define_key_in_input_decode_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ([24] t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((saved input-decode-map))
  (define-key input-decode-map [?\C-a] [?\C-x])
  (let ((result (lookup-key input-decode-map [?\C-a])))
    (define-key input-decode-map [?\C-a] nil)
    (list result (eq (lookup-key input-decode-map [?\C-a]) nil))))
"##,
        expect,
    );
}

#[test]
fn div_cx243_define_key_in_key_translation_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\u{3}\u{3}\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((saved key-translation-map))
  (define-key key-translation-map (kbd "C-x C-a") (kbd "C-c C-c"))
  (let ((result (lookup-key key-translation-map (kbd "C-x C-a"))))
    (define-key key-translation-map (kbd "C-x C-a") nil)
    (list result (null (lookup-key key-translation-map (kbd "C-x C-a"))))))
"##,
        expect,
    );
}

#[test]
fn div_cx243_local_function_key_map_per_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (help other)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf-a (get-buffer-create " *neo-cx243-a*"))
      (buf-b (get-buffer-create " *neo-cx243-b*")))
  (with-current-buffer buf-a
    (let ((local-map (make-sparse-keymap)))
      (define-key local-map [f1] 'help)
      (use-local-map local-map)))
  (with-current-buffer buf-b
    (let ((local-map (make-sparse-keymap)))
      (define-key local-map [f1] 'other)
      (use-local-map local-map)))
  (let ((a-fn (with-current-buffer buf-a (lookup-key (current-local-map) [f1])))
        (b-fn (with-current-buffer buf-b (lookup-key (current-local-map) [f1]))))
    (kill-buffer buf-a)
    (kill-buffer buf-b)
    (list a-fn b-fn)))
"##,
        expect,
    );
}

#[test]
fn div_cx243_listify_key_sequence_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((24 6) (134217848) (f5) (M-down))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (listify-key-sequence (kbd "C-x C-f"))
      (listify-key-sequence (kbd "M-x"))
      (listify-key-sequence [f5])
      (listify-key-sequence [M-down]))
"##,
        expect,
    )
}

#[test]
fn div_cx243_events_to_keys_and_back() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\u{3}\u{1}\u{2}\u{3}\" (3 1 2 3) 4 \"C-c C-a C-b C-c\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((keys (kbd "C-c C-a C-b C-c"))
       (events (listify-key-sequence keys)))
  (list keys events
        (length events)
        (key-description keys)
        (eq (aref keys 0) (car events))))
"##,
        expect,
    )
}

#[test]
fn div_cx243_keymap_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((saved-trans key-translation-map))
  (define-key key-translation-map (kbd "C-c C-t") (kbd "C-c C-a"))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Key translation mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((state (list (boundp 'key-translation-map)
                         (boundp 'input-decode-map)
                         (boundp 'function-key-map)
                         (lookup-key key-translation-map (kbd "C-c C-t"))
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (define-key key-translation-map (kbd "C-c C-t") nil)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    )
}
