//! Divergence tests: match-data + registers + syntax + abbrev combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_match_data_with_registers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 7 \"foo\" \"123\" t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "foo123bar456baz789")
  (goto-char 1)
  (re-search-forward "\\([a-z]+\\)\\([0-9]+\\)" nil t)
  (let ((md (match-data t))
        (g1 (match-string 1))
        (g2 (match-string 2)))
    (set-register ?a g1)
    (set-register ?b g2)
    (list (car md) (cadr md) g1 g2
          (string= g1 "foo")
          (string= g2 "123")
          (string= (get-register ?a) "foo")
          (string= (get-register ?b) "123")))) "#,
        expect,
    );
}

#[test]
fn divergence_nested_regex_match_data_save() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"AAA\" \"BBBB\" t t nil \"AAA\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAA-BBBB-CCCC-DDDD-EEEE")
  (goto-char 1)
  (re-search-forward "\\([A-Z]+\\)" nil t)
  (let ((outer (match-string 1))
        (outer-md (match-data)))
    (re-search-forward "\\([A-Z]+\\)" nil t)
    (let ((inner (match-string 1))
          (inner-md (match-data)))
      (list outer inner
            (string= outer "AAA")
            (string= inner "BBBB")
            (set-match-data outer-md)
            (match-string 1)
            (string= (match-string 1) "AAA"))))) "#,
        expect,
    );
}

#[test]
fn divergence_syntax_table_modify_regex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "aaa_bbb ccc-ddd")
  (let ((saved-syntax (copy-syntax-table (syntax-table))))
    (modify-syntax-entry ?_ "w")
    (goto-char 1)
    (let ((w1 (progn (forward-word 1) (buffer-substring 1 (point)))))
      (modify-syntax-entry ?- "w")
      (goto-char 1)
      (let ((w2 (progn (forward-word 1) (buffer-substring 1 (point)))))
        (set-syntax-table saved-syntax)
        (goto-char 1)
        (let ((w3 (progn (forward-word 1) (buffer-substring 1 (point)))))
          (list w1 w2 w3
                (string= w1 "aaa_bbb")
                (string= w2 "aaa_bbb")
                (string= w3 "aaa")))))) "#,
        expect,
    );
}

#[test]
fn divergence_abbrev_expand_with_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (wrong-type-argument obarrayp test-abbrev-tbl-xxx)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (define-abbrev-table 'test-abbrev-tbl-xxx nil)
  (define-abbrev 'test-abbrev-tbl-xxx "xyz" "expanded-xyz")
  (insert "xyz ")
  (let ((abbrev-mode t)
        (local-abbrev-table 'test-abbrev-tbl-xxx))
    (undo-boundary)
    (goto-char 4)
    (let ((expanded (expand-abbrev)))
      (list expanded
            (when expanded (buffer-string))
            (when expanded (string= (buffer-string) "expanded-xyz ")))))) "#,
        expect,
    );
}

#[test]
fn divergence_match_data_overlay_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil 1 30)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "match1 match2 match3 match4 match5")
  (let ((ov (make-overlay 1 30)))
    (overlay-put ov 'test t)
    (goto-char 1)
    (let ((matches nil))
      (while (re-search-forward "match[0-9]" nil t)
        (let ((s (match-string 0))
              (beg (match-beginning 0))
              (end (match-end 0)))
          (push (list s beg end) matches)))
      (list (= (length matches) 5)
            (string= (caar (last matches)) "match5")
            (overlay-start ov) (overlay-end ov))))) "#,
        expect,
    );
}

#[test]
fn divergence_register_point_marker_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (12 27 \"AAAAXX-BBBB-CCCC-DDDD-EEEE\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
  (let ((m (point-marker)))
    (goto-char 10)
    (set-register ?r (point-marker))
    (narrow-to-region 5 20)
    (goto-char (point-min))
    (insert "XX")
    (let ((reg-pos (marker-position (get-register ?r))))
      (widen)
      (list reg-pos
            (marker-position m)
            (buffer-string)
            (>= reg-pos 10))))) "#,
        expect,
    );
}

#[test]
fn divergence_syntax_forward_backward_word() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (14 5 8 t nil nil \"two\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "one two three four five")
  (goto-char 1)
  (forward-word 3)
  (let ((p1 (point)))
    (backward-word 2)
    (let ((p2 (point)))
      (forward-word 1)
      (let ((p3 (point)))
        (list p1 p2 p3
              (= p1 14)
              (= p2 4)
              (= p3 9)
              (buffer-substring p2 p3)))))) "#,
        expect,
    );
}

#[test]
fn divergence_re_search_with_match_data_swap() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK ((\"abc\" \"123\") (\"def\" \"456\") (\"abc\" \"123\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "abc123def456ghi789")
  (let ((saved nil))
    (goto-char 1)
    (re-search-forward "\\([a-z]+\\)\\([0-9]+\\)" nil t)
    (push (list (match-string 1) (match-string 2)) saved)
    (let ((md (match-data)))
      (re-search-forward "\\([a-z]+\\)\\([0-9]+\\)" nil t)
      (push (list (match-string 1) (match-string 2)) saved)
      (set-match-data md)
      (push (list (match-string 1) (match-string 2)) saved))
    (nreverse saved))) "#,
        expect,
    );
}

#[test]
fn divergence_thing_at_point_bounds_with_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"foo\" (13 . 16) \"hello\" (1 . 6) t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "hello world foo bar baz")
  (goto-char 13)
  (let ((word (thing-at-point 'word))
        (bounds (bounds-of-thing-at-point 'word)))
    (goto-char 1)
    (let ((w2 (thing-at-point 'word))
          (b2 (bounds-of-thing-at-point 'word)))
      (list word bounds w2 b2
            (string= word "foo")
            (equal bounds '(13 . 16))
            (string= w2 "hello")
            (equal b2 '(1 . 6)))))) "#,
        expect,
    );
}

#[test]
fn divergence_kill_ring_save_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"AAA\" t \"test-kill\" t \"AAA\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "match-AAA match-BBB match-CCC")
  (goto-char 1)
  (re-search-forward "match-\\([A-Z]+\\)")
  (let ((group (match-string 1)))
    (kill-new "test-kill")
    (list group
          (string= group "AAA")
          (current-kill 0)
          (string= (current-kill 0) "test-kill")
          (match-string 1)
          (string= (match-string 1) "AAA")))) "#,
        expect,
    );
}
