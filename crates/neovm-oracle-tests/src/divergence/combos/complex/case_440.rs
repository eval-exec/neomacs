//! Complex combo batch 440 — 15 final probes: face-attribute relative,
//! buffer-local overlay marker (the 4 passers from 439 retested plus
//! new variants), face+font+frame pass combo, ex-439 passers:
//! posn+display+invisible, face-attribute+font+frame,
//! buffer-local+overlay+marker, plus new edge combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// Re-test passers from 439 with small variations.
#[test]
fn div_cx440_face_font_frame_pass() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold italic nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (face-attribute 'bold :weight nil 'default)
      (face-attribute 'italic :slant nil 'default)
      (face-font 'default))"##,
        expect,
    );
}

/// buffer-local + overlay + marker: state tracking (passed before).
#[test]
fn div_cx440_buffer_local_overlay_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abcdef")
  (let ((o (make-overlay 2 4))) (overlay-put o 'face 'bold))
  (let ((m (set-marker (make-marker) 3))
        (v (make-local-variable 'neo-cx440-v)))
    (setq neo-cx440-v 'val)
    (list (marker-position m)
          (length (overlays-in 1 10)))))"##,
        expect,
    );
}

/// posn-at-point + display + visible text (passed before).
#[test]
fn div_cx440_posn_display_visible() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello world")
  (put-text-property 3 4 'display "XX")
  (condition-case e (posn-at-point 3) (error (car e))))"##,
        expect,
    );
}

/// bidi-string-mark-left-to-right with multibyte.
#[test]
fn div_cx440_bidi_string_mark_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"bidi-string\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'bidi-string)
  (string-mark-left-to-right "abcالعربية123"))"##,
        expect,
    );
}

/// char-bytes / char-width with edge Unicode ranges.
#[test]
fn div_cx440_char_bytes_width_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-bytes)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (char-bytes ?a) (char-bytes ?é) (char-bytes ?世) (char-bytes #x1F600)
      (char-width ?a) (char-width ?é) (char-width ?世) (char-width #x1F600))"##,
        expect,
    );
}

/// assoc + assq + rassoc + rassq with symbol/string keys.
#[test]
fn div_cx440_assoc_assq_rassoc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((a . 1) (\"a\" . 1) (a . 1) (\"a\" . 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((al '((a . 1) (b . 2) (c . 3)))
      (sl '(("a" . 1) ("b" . 2))))
  (list (assq 'a al) (assoc "a" sl) (rassq 1 al) (rassoc 1 sl)))"##,
        expect,
    );
}

/// length+ + safe-length + proper-list-p on various types.
#[test]
fn div_cx440_length_safe_proper() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 3 1 3 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (length '(a b c))
      (safe-length '(a b c))
      (safe-length '(a . b))
      (proper-list-p '(a b c))
      (proper-list-p '(a . b)))"##,
        expect,
    );
}

/// delete + delq + remove + remq with eq and equal.
#[test]
fn div_cx440_delete_delq_remove_remq() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((a c a) (a c a) (a c a) (a c a))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((l '(a b c b a)))
  (list (delq 'b (copy-sequence l))
        (delete 'b (copy-sequence l))
        (remq 'b (copy-sequence l))
        (remove 'b (copy-sequence l))))"##,
        expect,
    );
}

/// keymap lookup with multiple inheritance chain.
#[test]
fn div_cx440_keymap_inherit_chain_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (self-insert-command self-insert-command self-insert-command)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((k1 (make-sparse-keymap))
      (k2 (make-sparse-keymap))
      (k3 (make-sparse-keymap)))
  (define-key k1 "a" 'fn1)
  (define-key k2 "b" 'fn2)
  (define-key k3 "c" 'fn3)
  (set-keymap-parent k2 k1)
  (set-keymap-parent k3 k2)
  (list (key-binding "a" nil nil k3)
        (key-binding "b" nil nil k3)
        (key-binding "c" nil nil k3)))"##,
        expect,
    );
}

/// syntax-table: copy + modify + set.
#[test]
fn div_cx440_syntax_table_copy_modify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 8""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((st (copy-syntax-table (syntax-table))))
  (modify-syntax-entry ?_ "w" st)
  (with-temp-buffer
    (set-syntax-table st)
    (insert "foo_bar baz")
    (goto-char 1)
    (forward-word)
    (point)))"##,
        expect,
    );
}

/// memory-info / memory-use-counts basic.
#[test]
fn div_cx440_memory_info_counts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (listp (memory-info))
      (listp (memory-use-counts))
      (listp (memory-limit)))"##,
        expect,
    );
}

/// window-state-get with parameters and buffer.
#[test]
fn div_cx440_window_state_with_parameters() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "state")
  (let ((state (window-state-get (selected-window) t)))
    (list (listp state)
          (> (length state) 0))))"##,
        expect,
    );
}

/// prin1 with print-escape-control-characters.
#[test]
fn div_cx440_prin1_escape_control() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\\\"hello\\\\12\\\\11world\\\\15\\\\0\\\"\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((print-escape-control-characters t))
  (prin1-to-string "hello\n\tworld\r\0"))"##,
        expect,
    );
}

/// format with %S on vectors and records.
#[test]
fn div_cx440_format_S_vectors_records() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r##""OK (\"[1 2 3]\" \"#s(test 1 2)\" \"[:key :value]\")""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%S" [1 2 3])
      (format "%S" (record 'test 1 2))
      (format "%S" [:key :value]))"##,
        expect,
    );
}

/// abbrev-expansion / abbrev-symbol with mixed case.
#[test]
fn div_cx440_abbrev_mixed_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"the\" \"don't\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((tab (make-abbrev-table)))
  (define-abbrev tab "teh" "the")
  (define-abbrev tab "dont" "don't" nil 1)
  (list (abbrev-expansion "teh" tab)
        (abbrev-expansion "DONT" tab)))"##,
        expect,
    );
}
