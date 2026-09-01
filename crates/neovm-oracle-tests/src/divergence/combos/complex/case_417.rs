//! Complex combo batch 417 — 20 probes into regex engine edge cases:
//! non-greedy, shy groups, intervals, backreferences, word boundaries,
//! multibyte classes, alternation, lazy quantifiers, lookahead (if any),
//! bracket ranges with multibyte, and case-insensitive regex with
//! various character ranges.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// Non-greedy quantifiers: *? +? ?? with multibyte.
#[test]
fn div_cx417_regex_non_greedy_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 \"X\" 0 \"X\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (with-temp-buffer
    (insert "aαβγbαβγc")
    (list (string-match "a\\(.*?\\)b" "aXbYc")
          (match-string 1 "aXbYc")
          (string-match "a\\(.*\\)b" "aXbYc")
          (match-string 1 "aXbYc"))))
"##,
        expect,
    );
}

/// Shy (non-capturing) groups: \\(?: ... \\)
#[test]
fn div_cx417_regex_shy_group() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 \"foo42\" \"42\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-match "\\(?:foo\\|bar\\)\\([0-9]+\\)" "foo42")
      (match-string 0 "foo42")
      (match-string 1 "foo42"))
"##,
        expect,
    );
}

/// Interval quantifiers: \\{N,M\\} exact and bounded.
#[test]
fn div_cx417_regex_interval_quantifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 \"aaa\" 0 \"aaaa\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-match "a\\{3\\}" "aaab")
      (match-string 0 "aaab")
      (string-match "a\\{2,4\\}" "aaaaab")
      (match-string 0 "aaaaab"))
"##,
        expect,
    );
}

/// Backreferences: \\1 \\2 within the same regex.
#[test]
fn div_cx417_regex_backreference() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 \"hello hello\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-match "\\([a-z]+\\) \\1" "hello hello")
      (match-string 0 "hello hello")
      (string-match "\\([a-z]+\\) \\1" "hello world"))
"##,
        expect,
    );
}

/// Word boundaries: \\b \\B with multibyte characters.
#[test]
fn div_cx417_regex_word_boundary_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0 1 \"αβγ \")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (list (string-match "\\b" "a β γ")
        (string-match "\\b" "αβγ")
        (string-match "\\B" "abc")
        (and (string-match "\\b\\([α-ω]+\\).\\b" "αβγ δέ")
             (match-string 0 "αβγ δέ"))))
"##,
        expect,
    );
}

/// Character classes with multibyte ranges: [[:alpha:]] etc.
#[test]
fn div_cx417_regex_char_class_multibyte_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 \"αβγ\" 0 \"αβγ123\" 0 \"ΑΒΓabc\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (list (string-match "[[:alpha:]]+" "αβγ123")
        (match-string 0 "αβγ123")
        (string-match "[[:alnum:]]+" "αβγ123")
        (match-string 0 "αβγ123")
        (string-match "[[:upper:]]+" "ΑΒΓabc")
        (match-string 0 "ΑΒΓabc")))
"##,
        expect,
    );
}

/// Alternation with multibyte alternatives.
#[test]
fn div_cx417_regex_alternation_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 \"café\" 0 \"世界\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (list (string-match "café\\|世界" "café")
        (match-string 0 "café")
        (string-match "café\\|世界" "世界")
        (match-string 0 "世界")))
"##,
        expect,
    );
}

/// Bracket ranges with multibyte endpoints.
#[test]
fn div_cx417_regex_bracket_range_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0 nil 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (list (string-match "[α-ω]" "α")
        (string-match "[α-ω]" "ω")
        (string-match "[α-ω]" "a")
        (string-match "[α-ω]+" "αβγδε")))
"##,
        expect,
    );
}

/// Case-insensitive regex with multibyte bracketed ranges.
#[test]
fn div_cx417_regex_case_fold_bracket() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 \"ABCdef\" 0 \"abcDEF\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (list (string-match "[a-z]+" "ABCdef")
        (match-string 0 "ABCdef")
        (string-match "[A-Z]+" "abcDEF")
        (match-string 0 "abcDEF")))
"##,
        expect,
    );
}

/// Lazy quantifier vs greedy at end of string.
#[test]
fn div_cx417_regex_lazy_vs_greedy_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 \"aXbYb\" 0 \"aXb\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (list (string-match "a.*b" "aXbYb")
        (match-string 0 "aXbYb")
        (string-match "a.*?b" "aXbYb")
        (match-string 0 "aXbYb")))
"##,
        expect,
    );
}

/// Empty string matching: regex that matches zero-length.
#[test]
fn div_cx417_regex_empty_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0 \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-match "" "abc")
      (string-match "a*" "bc")
      (match-string 0 "bc"))
"##,
        expect,
    );
}

/// Nested groups with alternation.
#[test]
fn div_cx417_regex_nested_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 \"ac\" \"ac\" \"a\" \"c\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-match "\\(\\(a\\|b\\)\\(c\\|d\\)\\)" "ac")
      (match-string 0 "ac")
      (match-string 1 "ac")
      (match-string 2 "ac")
      (match-string 3 "ac"))
"##,
        expect,
    );
}

/// Anchored regex: ^ and $ with multibyte.
#[test]
fn div_cx417_regex_anchored_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 2 0 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-match "^αβ" "αβγ")
      (string-match "γ$" "αβγ")
      (string-match "^café" "café世界")
      (string-match "世界$" "café世界"))
"##,
        expect,
    );
}

/// Dot (.) with multibyte: should match any character including multibyte.
#[test]
fn div_cx417_regex_dot_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0 0 0 \"αβγ\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-match "." "a")
      (string-match "." "é")
      (string-match "." "世")
      (string-match "..." "αβγ")
      (match-string 0 "αβγ"))
"##,
        expect,
    );
}

/// Character range inversion: [^...] with multibyte.
#[test]
fn div_cx417_regex_inverted_range_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 \"12345\" 0 \"abc\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-match "[^a-z]+" "12345")
      (match-string 0 "12345")
      (string-match "[^α-ω]+" "abc")
      (match-string 0 "abc"))
"##,
        expect,
    );
}

/// Named character classes: [[:digit:]], [[:punct:]], etc.
#[test]
fn div_cx417_regex_named_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 \"123\" 5 \"!!!\" 1 \"   \")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-match "[[:digit:]]+" "abc123def")
      (match-string 0 "abc123def")
      (string-match "[[:punct:]]+" "hello!!!")
      (match-string 0 "hello!!!")
      (string-match "[[:space:]]+" "a   b")
      (match-string 0 "a   b"))
"##,
        expect,
    );
}

/// Multibyte character in bracket: single char char class.
#[test]
fn div_cx417_regex_single_char_class_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 \"é\" 0 \"世\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-match "[é]" "café")
      (match-string 0 "café")
      (string-match "[世]" "世界")
      (match-string 0 "世界"))
"##,
        expect,
    );
}

/// Regex with optional newline and dotall behavior.
#[test]
fn div_cx417_regex_dot_newline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 0 \"axb\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-match "a.b" "a\nb")
      (string-match "a.b" "axb")
      (match-string 0 "axb"))
"##,
        expect,
    );
}

/// Complex alternation with shared prefix.
#[test]
fn div_cx417_regex_complex_alternation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 \"forget\" 0 \"forgive\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-match "forever\\|forget\\|forgive" "forget")
      (match-string 0 "forget")
      (string-match "forever\\|forget\\|forgive" "forgive")
      (match-string 0 "forgive"))
"##,
        expect,
    );
}
