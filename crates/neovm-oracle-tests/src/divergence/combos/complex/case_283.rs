//! Complex combo batch 283 — `syntax-ppss` cache consistency after
//! insert/delete, `parse-partial-sexp` with complex nesting,
//! `scan-sexps`/`scan-lists` error handling at buffer boundaries.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx283_syntax_ppss_after_insert_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(defun foo ()\n  \"doc\"\n  (+ 1 2))")
  (let ((before (syntax-ppss 20)))
    (goto-char 10)
    (insert "XXX")
    (let ((after-insert (syntax-ppss 20)))
      (delete-region 10 13)
      (let ((after-delete (syntax-ppss 20)))
        (list before after-insert after-delete
              (equal before after-delete)))))
"##,
        expect,
    )
}

#[test]
fn div_cx283_parse_partial_sexp_complex_nesting() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1 4 1 34 20 3 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(defun foo (a b)\n  \"docstring\"\n  (let ((x (+ a 1)))\n    (* x b)))")
  (list (nth 0 (parse-partial-sexp 1 10))
        (nth 0 (parse-partial-sexp 1 30))
        (nth 0 (parse-partial-sexp 1 50))
        (nth 0 (parse-partial-sexp 1 65))
        (nth 3 (parse-partial-sexp 1 30))
        (nth 8 (parse-partial-sexp 1 30))
        (nth 0 (parse-partial-sexp 1 60))
        (nth 1 (parse-partial-sexp 1 60))))
"##,
        expect,
    )
}

#[test]
fn div_cx283_scan_lists_at_buffer_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (scan-error \"Unbalanced parentheses\" 1 16)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(a (b (c) d) e)")
  (goto-char 1)
  (list (scan-lists (point) 1 0)
        (scan-lists (point) 1 1)
        (scan-lists (point) 2 0)
        (condition-case e (scan-lists (point) 99 0) (error (car e)))
        (scan-lists 5 -1 0)
        (condition-case e (scan-lists 1 -1 0) (error (car e)))))
"##,
        expect,
    )
}

#[test]
fn div_cx283_scan_sexps_error_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 12 18 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(a b) (c d) (e f)")
  (goto-char 1)
  (list (scan-sexps (point) 1)
        (scan-sexps (point) 2)
        (scan-sexps (point) 3)
        (condition-case e (scan-sexps (point) 99) (error (car e)))))
"##,
        expect,
    )
}

#[test]
fn div_cx283_syntax_ppss_string_with_escapes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 34 34 nil 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "before \"str with \\\"esc\\\"\" after")
  (list (nth 3 (syntax-ppss 8))
        (nth 3 (syntax-ppss 15))
        (nth 3 (syntax-ppss 25))
        (nth 3 (syntax-ppss 30))
        (nth 8 (syntax-ppss 15))))
"##,
        expect,
    )
}

#[test]
fn div_cx283_syntax_ppss_comment_line_and_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "code1\n; line comment\ncode2\n/* block\nmulti */ after")
  (list (nth 4 (syntax-ppss 10))
        (nth 4 (syntax-ppss 20))
        (nth 4 (syntax-ppss 35))
        (nth 4 (syntax-ppss 50))))
"##,
        expect,
    )
}

#[test]
fn div_cx283_forward_comment_complex_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (7 7 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "code1 ; comment1\ncode2 /* block */ code3")
  (goto-char 7)
  (forward-comment 1)
  (let ((p1 (point)))
    (forward-comment 1)
    (let ((p2 (point)))
      (forward-comment -1)
      (list p1 p2 (point)))))
"##,
        expect,
    )
}

#[test]
fn div_cx283_up_list_down_list_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (16 19 22)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(a (b (c (d) e) f) g)")
  (goto-char 14)
  (condition-case e
      (progn
        (up-list 1)
        (let ((p1 (point)))
          (up-list 1)
          (let ((p2 (point)))
            (up-list 1)
            (list p1 p2 (point)))))
    (error (list :error (car e)))))
"##,
        expect,
    )
}

#[test]
fn div_cx283_backward_up_list_at_top_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8 7 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(a (b (c)))")
  (goto-char 1)
  (down-list 1)
  (down-list 1)
  (down-list 1)
  (let ((p1 (point)))
    (backward-up-list 1)
    (let ((p2 (point)))
      (backward-up-list 1)
      (list p1 p2 (point)))))
"##,
        expect,
    )
}

#[test]
fn div_cx283_syntax_ppss_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<killed buffer> 1 30)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "(defun mega ()\n  \"docstring\"\n  (let ((x 1))\n    (+ x 1)))")
  (put-text-property 1 8 'face 'bold)
  (let ((m (set-marker (make-marker) 20))
        (ov (make-overlay 4 30)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 2 35)
    (let ((state (list (parse-partial-sexp 1 30)
                       (nth 3 (syntax-ppss 25))
                       (nth 8 (syntax-ppss 25))
                       (scan-lists 1 1 0)
                       (buffer-string)
                       (marker-position m)
                       (overlay-start ov) (overlay-end ov)
                       (text-properties-at 1))))
      (undo)
      (widen)
      (list state (buffer-string) (marker-position m)
            (overlay-start ov) (overlay-end ov)
            (text-properties-at 1)))))
"##,
        expect,
    )
}
