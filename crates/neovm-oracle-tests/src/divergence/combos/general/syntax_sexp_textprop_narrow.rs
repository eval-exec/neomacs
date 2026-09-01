//! Divergence tests: syntax + sexp nav + text property syntax + narrow combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_forward_sexp_across_syntax_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 13 18 t nil \"(foo bar|baz quux)\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(foo bar|baz quux)")
  (put-text-property 9 10 'syntax-table '(1))
  (goto-char 2)
  (forward-sexp 1)
  (let ((p1 (point)))
    (forward-sexp 1)
    (let ((p2 (point)))
      (forward-sexp 1)
      (list p1 p2 (point)
            (= p1 5)
            (= p2 8)
            (buffer-string)
            (= (buffer-size) 17))))) "#,
        expect,
    );
}

#[test]
fn divergence_scan_lists_with_textprop_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 9 35)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(a (b (c) d) e)")
  (put-text-property 5 6 'syntax-table '(15))
  (let ((p1 (scan-lists 1 1 0))
        (p2 (scan-lists 1 2 0)))
    (list p1 p2
          (= p1 17)
          (buffer-string)
          (= (buffer-size) 15)))) #"#,
        expect,
    );
}

#[test]
fn divergence_narrowed_forward_sexp_with_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(alpha (beta gamma) delta)")
  (let ((ov (make-overlay 8 12)))
    (overlay-put ov 'face 'bold)
    (narrow-to-region 2 22)
    (goto-char (point-min))
    (forward-sexp 1)
    (let ((p1 (point)))
      (forward-sexp 1)
      (let ((p2 (point)))
        (widen)
        (list p1 p2
              (buffer-string)
              (overlay-start ov) (overlay-end ov)
              (overlay-get ov 'face)
              (= (buffer-size) 24))))) "#,
        expect,
    );
}

#[test]
fn divergence_backward_sexp_from_mid() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 6 2 t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(foo bar baz quux)")
  (goto-char 14)
  (backward-sexp 1)
  (let ((p1 (point)))
    (backward-sexp 1)
    (let ((p2 (point)))
      (backward-sexp 1)
      (list p1 p2 (point)
            (= p1 10)
            (= p2 6)
            (= (point) 2))))) "#,
        expect,
    );
}

#[test]
fn divergence_kill_sexp_with_undo_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"( beta gamma delta)\" 2 15 \"(alpha beta gamma delta)\" t 2 20)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(alpha beta gamma delta)")
  (let ((m1 (copy-marker 2 t))
        (m2 (copy-marker 20)))
    (undo-boundary)
    (goto-char 2)
    (kill-sexp 1)
    (let ((s1 (buffer-string))
          (p1 (marker-position m1))
          (p2 (marker-position m2)))
      (primitive-undo 1 buffer-undo-list)
      (list s1 p1 p2
            (buffer-string)
            (= (buffer-size) 24)
            (marker-position m1)
            (marker-position m2))))) "#,
        expect,
    );
}

#[test]
fn divergence_transpose_sexps_with_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 15 35)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(aaa bbb ccc ddd)")
  (let ((ov (make-overlay 6 9)))
    (overlay-put ov 'tag 'bbb)
    (put-text-property 2 5 'group 'aaa)
    (put-text-property 6 9 'group 'bbb)
    (goto-char 10)
    (transpose-sexps 1)
    (list (buffer-string)
          (overlay-start ov) (overlay-end ov)
          (overlay-get ov 'tag)
          (get-text-property 2 'group)
          (eq (get-text-property 2 'group) 'aaa)
          (get-text-property 6 'group)
          (= (buffer-size) 19)))) #"#,
        expect,
    );
}

#[test]
fn divergence_mark_sexp_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"oneXXX two three four fiv\" 9 \"(one two three four five)\" nil 6)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(one two three four five)")
  (let ((m (copy-marker 6 t)))
    (narrow-to-region 2 24)
    (undo-boundary)
    (goto-char (point-min))
    (forward-sexp 1)
    (insert "XXX")
    (let ((s1 (buffer-string))
          (mp (marker-position m)))
      (primitive-undo 1 buffer-undo-list)
      (widen)
      (list s1 mp
            (buffer-string)
            (= (buffer-size) 26)
            (marker-position m))))) "#,
        expect,
    );
}

#[test]
fn divergence_parse_partial_sexp_depth() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 10 35)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(a (b (c (d) e) f) g)")
  (let ((p1 (parse-partial-sexp 1 5))
        (p2 (parse-partial-sexp 1 10))
        (p3 (parse-partial-sexp 1 20)))
    (list (nth 0 p1) (nth 0 p2) (nth 0 p3)
          (>= (nth 0 p1) 0)
          (>= (nth 0 p2) (nth 0 p1))
          (>= (nth 0 p3) (nth 0 p2))
          (= (buffer-size) 21)))) #"#,
        expect,
    );
}

#[test]
fn divergence_syntax_property_string_fence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 12 33)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "before\"quoted text\"after")
  (put-text-property 7 19 'syntax-table '(15))
  (goto-char 7)
  (forward-sexp 1)
  (let ((p1 (point)))
    (forward-sexp 1)
    (let ((p2 (point)))
      (list p1 p2
            (= p1 20)
            (= p2 25)
            (buffer-string))))) #"#,
        expect,
    );
}

#[test]
fn divergence_insert_parentheses_balancing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 14 51)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "alpha beta gamma")
  (goto-char 7)
  (insert-parentheses nil)
  (let ((s1 (buffer-string)))
    (goto-char 1)
    (insert-parentheses 3)
    (list s1
          (buffer-string)
          (= (buffer-size) 23)
          (char-after 1)
          (= (char-after 1) 40)
          (char-after (1- (point-max)))
          (= (char-after (1- (point-max))) 41)))) #"#,
        expect,
    );
}
