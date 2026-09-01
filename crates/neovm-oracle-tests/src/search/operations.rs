//! Oracle parity tests for search operations: `search-forward`,
//! `search-backward`, `re-search-backward`, `looking-at-p`,
//! `posix-string-match`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// search-forward
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_search_forward_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "hello world hello emacs")
                    (goto-char (point-min))
                    (list (search-forward "hello" nil t)
                          (point)))"#;
    let expect = expect_test::expect![[r#""OK (6 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_search_forward_bound() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // BOUND parameter limits search range
    let form = r#"(with-temp-buffer
                    (insert "aaa bbb ccc ddd")
                    (goto-char (point-min))
                    (list (search-forward "ccc" 8 t)
                          (search-forward "ccc" nil t)))"#;
    let expect = expect_test::expect![[r#""OK (nil 12)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_search_forward_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // COUNT parameter: find Nth occurrence
    let form = r#"(with-temp-buffer
                    (insert "ab ab ab ab ab")
                    (goto-char (point-min))
                    (search-forward "ab" nil t 3)
                    (point))"#;
    let expect = expect_test::expect![[r#""OK 9""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_search_forward_not_found() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "hello world")
                    (goto-char (point-min))
                    (search-forward "xyz" nil t))"#;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// search-backward
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_search_backward_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "alpha beta gamma beta delta")
                    (goto-char (point-max))
                    (list (search-backward "beta" nil t)
                          (point)))"#;
    let expect = expect_test::expect![[r#""OK (18 18)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_search_backward_bound() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "aaa bbb ccc bbb ddd")
                    (goto-char (point-max))
                    ;; bound=10 means don't search before position 10
                    (list (search-backward "aaa" 10 t)
                          (search-backward "bbb" nil t)))"#;
    let expect = expect_test::expect![[r#""OK (nil 13)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_search_backward_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "xx xx xx xx xx")
                    (goto-char (point-max))
                    (search-backward "xx" nil t 2)
                    (point))"#;
    let expect = expect_test::expect![[r#""OK 10""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// re-search-backward
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_re_search_backward_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "foo-123 bar-456 baz-789")
                    (goto-char (point-max))
                    (re-search-backward "\\([a-z]+\\)-\\([0-9]+\\)" nil t)
                    (list (match-string 0)
                          (match-string 1)
                          (match-string 2)
                          (point)))"#;
    let expect = expect_test::expect![[r#""OK (\"z-789\" \"z\" \"789\" 19)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_re_search_backward_bound() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "aaa-111 bbb-222 ccc-333")
                    (goto-char (point-max))
                    ;; bound prevents finding first match
                    (list (re-search-backward "[a-z]+-[0-9]+" 10 t)
                          (when (match-string 0)
                            (match-string 0))))"#;
    let expect = expect_test::expect![[r#""OK (19 \"c-333\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_re_search_backward_collect_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Collect all matches searching backward
    let form = r#"(with-temp-buffer
                    (insert "cat sat on the mat with a bat")
                    (goto-char (point-max))
                    (let ((matches nil))
                      (while (re-search-backward "\\b[a-z]at\\b" nil t)
                        (setq matches (cons (match-string 0) matches)))
                      matches))"#;
    let expect = expect_test::expect![[r#""OK (\"cat\" \"sat\" \"mat\" \"bat\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// looking-at-p (non-match-data-modifying)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_looking_at_p_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "hello world")
                    (goto-char (point-min))
                    (list (looking-at-p "hello")
                          (looking-at-p "world")
                          (looking-at-p "hel.*")))"#;
    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_looking_at_p_preserves_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // looking-at-p should NOT modify match data
    let form = r#"(progn
                    (string-match "\\(foo\\)" "foobar")
                    (let ((before (match-beginning 1)))
                      (with-temp-buffer
                        (insert "test")
                        (goto-char (point-min))
                        (looking-at-p "test"))
                      (= before (match-beginning 1))))"#;
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: search-and-extract pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_search_extract_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extract structured data using forward and backward search
    let form = r#"(with-temp-buffer
                    (insert "BEGIN name=Alice END\n")
                    (insert "BEGIN name=Bob age=25 END\n")
                    (insert "BEGIN name=Carol role=dev END\n")
                    (goto-char (point-min))
                    (let ((records nil))
                      (while (search-forward "BEGIN " nil t)
                        (let ((start (point)))
                          (when (search-forward " END" nil t)
                            (let ((content (buffer-substring
                                            start (match-beginning 0))))
                              (let ((pairs nil)
                                    (pos 0))
                                (while (string-match
                                        "\\([a-z]+\\)=\\([^ ]+\\)"
                                        content pos)
                                  (setq pairs
                                        (cons (cons (match-string 1 content)
                                                    (match-string 2 content))
                                              pairs)
                                        pos (match-end 0)))
                                (setq records
                                      (cons (nreverse pairs)
                                            records)))))))
                      (nreverse records)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"name\" . \"Alice\")) ((\"name\" . \"Bob\") (\"age\" . \"25\")) ((\"name\" . \"Carol\") (\"role\" . \"dev\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_search_bidirectional_bracket_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Find matching brackets using forward/backward search
    let form = r#"(with-temp-buffer
                    (insert "(defun foo (x y) (+ x y))")
                    ;; Find the inner (x y) paren group
                    (goto-char (point-min))
                    (search-forward "(x y)" nil t)
                    (let ((end (point))
                          (start (match-beginning 0)))
                      ;; Now search backward from end for opening paren
                      (goto-char end)
                      (search-backward "(" start t)
                      (let ((inner-start (point)))
                        (list inner-start end
                              (buffer-substring inner-start end)))))"#;
    let expect = expect_test::expect![[r#""OK (12 17 \"(x y)\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
