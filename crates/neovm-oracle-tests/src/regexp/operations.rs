//! Oracle parity tests for `regexp-quote`, `replace-regexp-in-string`,
//! `looking-at`, `replace-match`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// regexp-quote
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_quote_special_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"foo\\\\.bar\"""#]];
    // regexp-quote escapes special regex characters
    crate::common::assert_oracle_parity_expect(r#"(regexp-quote "foo.bar")"#, expect);
    let expect = expect_test::expect![[r#""OK \"a\\\\*b\\\\+c\\\\?\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(regexp-quote "a*b+c?")"#, expect);
    let expect = expect_test::expect![[r#""OK \"\\\\[test]\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(regexp-quote "[test]")"#, expect);
    let expect = expect_test::expect![[r#""OK \"a\\\\\\\\b\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(regexp-quote "a\\b")"#, expect);
    let expect = expect_test::expect![[r#""OK \"\\\\^start\\\\$\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(regexp-quote "^start$")"#, expect);
    let expect = expect_test::expect![[r#""OK \"(group)\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(regexp-quote "(group)")"#, expect);
}

#[test]
fn oracle_prop_regexp_quote_plain_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    // Plain strings should pass through unchanged
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(regexp-quote "hello")"#, expect);
    assert_ok_eq(r#""hello""#, &o, &n);
}

#[test]
fn oracle_prop_regexp_quote_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // After quoting, the string should match literally
    let form = r####"(let ((literal "foo.bar*baz"))
                    (string-match-p (regexp-quote literal) literal))"####;
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("0", &o, &n);
}

#[test]
fn oracle_prop_regexp_quote_used_in_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Without quoting, "." matches any char; with quoting, only literal "."
    let form = r####"(list
                    (string-match-p "foo.bar" "fooXbar")
                    (string-match-p (regexp-quote "foo.bar") "fooXbar")
                    (string-match-p (regexp-quote "foo.bar") "foo.bar"))"####;
    let expect = expect_test::expect![[r#""OK (0 nil 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// replace-regexp-in-string (5 params)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_regexp_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"fooNUMbarNUM\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(replace-regexp-in-string "[0-9]+" "NUM" "foo123bar456")"#,
        expect,
    );
    assert_ok_eq(r#""fooNUMbarNUM""#, &o, &n);
}

#[test]
fn oracle_prop_replace_regexp_no_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"hello world\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(replace-regexp-in-string "xyz" "ABC" "hello world")"#,
        expect,
    );
    assert_ok_eq(r#""hello world""#, &o, &n);
}

#[test]
fn oracle_prop_replace_regexp_with_backreference() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use \1 backreference in replacement
    let form = r####"(replace-regexp-in-string
                    "\\([a-z]+\\)-\\([0-9]+\\)"
                    "\\2-\\1"
                    "foo-123 bar-456")"####;
    let expect = expect_test::expect![[r#""OK \"123-foo 456-bar\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_replace_regexp_with_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // REP can be a function that receives the matched string
    let form = r####"(replace-regexp-in-string
                    "[0-9]+"
                    (lambda (match)
                      (number-to-string (* 2 (string-to-number match))))
                    "price: 10, qty: 5")"####;
    let expect = expect_test::expect![[r#""OK \"price: 20, qty: 10\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_replace_regexp_fixedcase() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // FIXEDCASE parameter (4th arg)
    let form = r####"(replace-regexp-in-string
                    "hello" "world" "Hello hello HELLO" t)"####;
    let expect = expect_test::expect![[r#""OK \"world world world\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_replace_regexp_literal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // LITERAL parameter (5th arg) — don't interpret \ in replacement
    let form = r####"(replace-regexp-in-string
                    "foo" "\\&bar" "foo" nil t)"####;
    let expect = expect_test::expect![[r#""OK \"\\\\&bar\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_replace_regexp_start_pos() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // START parameter (6th arg)
    let form = r####"(replace-regexp-in-string
                    "[0-9]+" "X" "a1b2c3d4" nil nil 4)"####;
    let expect =
        expect_test::expect![[r#""ERR (error \"replace-match subexpression does not exist\" 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_replace_regexp_complex_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Complex: strip HTML-like tags
    let form = r####"(replace-regexp-in-string
                    "<[^>]+>" "" "<b>bold</b> and <i>italic</i>")"####;
    let expect = expect_test::expect![[r#""OK \"bold and italic\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(r#""bold and italic""#, &o, &n);
}

// ---------------------------------------------------------------------------
// looking-at (buffer regex)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_looking_at_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "hello world")
                    (goto-char (point-min))
                    (looking-at "hello"))"####;
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_looking_at_at_middle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "hello world")
                    (goto-char 7)
                    (looking-at "world"))"####;
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_looking_at_no_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "hello world")
                    (goto-char (point-min))
                    (looking-at "world"))"####;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_looking_at_sets_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "abc-123-def")
                    (goto-char (point-min))
                    (looking-at "\\([a-z]+\\)-\\([0-9]+\\)")
                    (list (match-string 0)
                          (match-string 1)
                          (match-string 2)))"####;
    let expect = expect_test::expect![[r#""OK (\"abc-123\" \"abc\" \"123\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// replace-match (buffer modification)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_match_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "hello world")
                    (goto-char (point-min))
                    (re-search-forward "world")
                    (replace-match "emacs")
                    (buffer-string))"####;
    let expect = expect_test::expect![[r#""OK \"hello emacs\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(r#""hello emacs""#, &o, &n);
}

#[test]
fn oracle_prop_replace_match_with_backreference() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "foo-123")
                    (goto-char (point-min))
                    (re-search-forward "\\([a-z]+\\)-\\([0-9]+\\)")
                    (replace-match "\\2-\\1")
                    (buffer-string))"####;
    let expect = expect_test::expect![[r#""OK \"123-foo\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_replace_match_fixedcase_and_literal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // replace-match FIXEDCASE (2nd arg), LITERAL (3rd arg)
    let form = r####"(with-temp-buffer
                    (insert "Hello World")
                    (goto-char (point-min))
                    (re-search-forward "Hello")
                    (replace-match "goodbye" t)
                    (buffer-string))"####;
    let expect = expect_test::expect![[r#""OK \"goodbye World\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_replace_match_on_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // replace-match can operate on a string (4th arg)
    let form = r####"(progn
                    (string-match "\\([a-z]+\\)" "hello world")
                    (replace-match "REPLACED" nil nil "hello world"))"####;
    let expect = expect_test::expect![[r#""OK \"REPLACED world\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex combination: search-and-replace pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_search_replace_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multi-pass search and replace in a buffer
    let form = r####"(with-temp-buffer
                    (insert "price: $10, discount: $3, total: $7")
                    (goto-char (point-min))
                    (let ((sum 0))
                      (while (re-search-forward "\\$\\([0-9]+\\)" nil t)
                        (setq sum (+ sum (string-to-number
                                          (match-string 1)))))
                      (goto-char (point-max))
                      (insert (format " [sum=$%d]" sum))
                      (buffer-string)))"####;
    let expect =
        expect_test::expect![[r#""OK \"price: $10, discount: $3, total: $7 [sum=$20]\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_regexp_global_replace_in_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "cat sat on the mat with a cat")
                    (goto-char (point-min))
                    (let ((count 0))
                      (while (re-search-forward "cat" nil t)
                        (replace-match "dog")
                        (setq count (1+ count)))
                      (list (buffer-string) count)))"####;
    let expect = expect_test::expect![[r#""OK (\"dog sat on the mat with a dog\" 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
