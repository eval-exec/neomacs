//! Regex edge-case divergence probes (regex-emacs.c vs neovm-core regex_emacs.rs).
//!
//! Classic regex-engine divergence points: backreferences, shy/explicit-numbered
//! groups, interval operators, word boundaries, case-fold over special casing
//! (ß, Σ/σ), non-greedy quantifiers, match-data subexpressions, and replace-match
//! backrefs with case directives (\u \l \U \L \E) and \,(lisp) eval.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_ar_backreference_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-match "\(a\)\1" "xaa") (string-match "\(a\)\1" "xab"))"##,
        expect,
    );
}

#[test]
fn div_ar_shy_group_numbering() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    // Shy group (?:ab) does NOT capture; group 1 is "c".
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (string-match "\(?:ab\)\\(c\\)" "abc")
       (list (match-beginning 1) (match-end 1) (match-beginning 2)))
"##,
        expect,
    );
}

#[test]
fn div_ar_shy_then_backref() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 3 28)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (string-match "\(?:a\)\\(b\\)\1" "abb")
       (match-beginning 1)))
"##,
        expect,
    );
}

#[test]
fn div_ar_interval_operator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 3 49)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (string-match "a\\{2,3\\}" "aaaaa")
       (list (match-beginning 0) (match-end 0))))
"##,
        expect,
    );
}

#[test]
fn div_ar_interval_brace_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 3 22)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (string-match "a\\{2\\}" "aaaa")
       (match-end 0)))
"##,
        expect,
    );
}

#[test]
fn div_ar_non_greedy_plus() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 3 49)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (string-match "a+?" "aaa")
       (list (match-beginning 0) (match-end 0))))
"##,
        expect,
    );
}

#[test]
fn div_ar_word_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(string-match "\\bword\\b" "a word here")"##,
        expect,
    );
}

#[test]
fn div_ar_angle_word_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 nil 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-match "\\<foo\\>" "foo bar")
      (string-match "\\<foo\\>" "foobar")
      (progn (string-match "\\<foo\\>" "x foo y") (match-beginning 0)))
"##,
        expect,
    );
}

#[test]
fn div_ar_case_fold_sigma() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 3)""#]];
    // σ (U+03C3) should match Σ (U+03A3) under case-fold-search.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (list (string-match "σ" "abcΣdef")
        (string-match "Σ" "abcσdef")))
"##,
        expect,
    );
}

#[test]
fn div_ar_case_fold_sharp_s() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (list (string-match "ß" "STRASSE")
        (string-match "ß" "straße")))
"##,
        expect,
    );
}

#[test]
fn div_ar_match_data_subexpressions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 4 1 2 2 3 3 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (string-match "\\(a\\)\\(b\\)\\(c\\)" "xabc")
       (match-data))
"##,
        expect,
    );
}

#[test]
fn div_ar_replace_match_backref_swap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"ba cd ba\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(replace-regexp-in-string "\\(a\\)\\(b\\)" "\\2\\1" "ab cd ab")
"##,
        expect,
    );
}

#[test]
fn div_ar_replace_match_amp_and_n() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"[aaa] bb [aaa]\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(replace-regexp-in-string "a+" "[\\&]" "aaa bb aaa")
"##,
        expect,
    );
}

#[test]
fn div_ar_replace_match_upper_directive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Invalid use of ‘\\\\’ in replacement text\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(replace-regexp-in-string "\\(a\\)" "\\u\\1" "abc"))
"##,
        expect,
    );
}

#[test]
fn div_ar_replace_match_upper_all_directive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Invalid use of ‘\\\\’ in replacement text\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (replace-regexp-in-string "\\(ab\\)" "\\U\\1" "ab cd")
      (replace-regexp-in-string "\\(AB\\)" "\\L\\1" "AB CD"))
"##,
        expect,
    );
}

#[test]
fn div_ar_replace_match_lisp_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"a2b23c334\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(replace-regexp-in-string "[0-9]+"
  (lambda (m) (number-to-string (1+ (string-to-number m))))
  "a1b22c333")
"##,
        expect,
    );
}

#[test]
fn div_ar_looking_back() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "foo bar")
  (goto-char 7)
  (list (looking-back "bar") (looking-back "foo" 3)))
"##,
        expect,
    );
}

#[test]
fn div_ar_alternation_first_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    // Emacs regex returns FIRST alternative match (a), not longest (ab).
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (string-match "a\\|ab" "ab") (match-end 0))
"##,
        expect,
    );
}

#[test]
fn div_ar_regexp_quote_special() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"\\\\.\\\\*\\\\+\\\\?\\\\[](){}\\\\^\\\\$\\\\\\\\|\"""#]];
    crate::common::assert_oracle_parity_expect(r##"(regexp-quote ".*+?[](){}^$\\|")"##, expect);
}

#[test]
fn div_ar_nongreedy_optional_prefers_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // Non-greedy `??` prefers the empty match; it must NOT behave greedily.
    // Found by the differential proptest (regex_parity_proptest.rs) and fixed
    // in regex_emacs.rs compile_repetition (the `??` split now skips the body
    // first, matching GNU regex-emacs.c:2009-2015).
    let expect = expect_test::expect![[r#""OK (0 (0 0) 0 (0 0) 0 (0 2) (0 2 0 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-match "a??" "aaa") (match-data)
      (string-match ".??" "xy")  (match-data)
      (string-match "a??b" "ab")  (match-data)
      (progn (string-match "\\(a??\\)b" "ab") (match-data)))
"##,
        expect,
    );
}

#[test]
fn div_ar_word_boundary_string_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // GNU treats the beginning/end of the searched region as unconditional word
    // boundaries (regex-emacs.c `case wordbound`, Case 1): `\b` succeeds and
    // `\B` fails at an edge regardless of the adjacent char's syntax.  Found by
    // the differential proptest and fixed in regex_emacs.rs assert_word_boundary.
    let expect = expect_test::expect![[r#""OK (0 0 nil nil 1 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-match "\\b" "")   (string-match "\\b" ".")
      (string-match "\\B" ".")  (string-match "\\B" "")
      (string-match "\\B" "ab") (string-match "\\W\\b" ","))
"##,
        expect,
    );
}
