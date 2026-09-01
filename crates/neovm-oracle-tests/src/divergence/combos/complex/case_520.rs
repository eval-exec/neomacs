/// Batch 520: regexp replace with count, string-trim with predicate, string-join deep.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx520_regexp_replace_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"XX XXX XXX\" \"X XXX XXX\" \"XX XXX\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (replace-regexp-in-string "a" "X" "aaa aaa aaa" nil nil nil 1)
      (replace-regexp-in-string "a" "X" "aaa aaa aaa" nil nil nil 2)
      (replace-regexp-in-string "a" "X" "aaa aaa aaa" nil nil nil 5))
"##,
        expect,
    );
}

#[test]
fn div_cx520_regexp_replace_subexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"bacbac\" \"bcbac\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (replace-regexp-in-string "\\(a\\)\\(b\\)" "\\2\\1" "abcabc")
      (replace-regexp-in-string "\\(a\\)\\(b\\)" "\\2\\1" "abcabc" nil nil nil 1))
"##,
        expect,
    );
}

#[test]
fn div_cx520_regexp_replace_literal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"XXX\" \"XXX\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (replace-regexp-in-string "." "X" "abc" nil t)
      (replace-regexp-in-string "." "X" "abc" nil nil))
"##,
        expect,
    );
}

#[test]
fn div_cx520_string_trim_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"hello\" \"hello\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-trim "  hello  ")
      (string-trim "xxhelloxx" "x+" "x+"))
"##,
        expect,
    );
}

#[test]
fn div_cx520_string_join_multi() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a, b, c\" \"x\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-join '("a" "b" "c") ", ")
      (string-join '("x") ", ")
      (string-join '()))
"##,
        expect,
    );
}

#[test]
fn div_cx520_string_fill() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"hello\\nworld\" \"hello world\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-fill "hello world" 5)
      (string-fill "hello world" 20))
"##,
        expect,
    );
}

#[test]
fn div_cx520_string_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"hel\" \"llo\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-limit "hello" 3) (string-limit "hello" 3 t))
"##,
        expect,
    );
}

#[test]
fn div_cx520_string_title_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function string-titleize)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'subr-x)
  (string-titleize "hello world"))
"##,
        expect,
    );
}

#[test]
fn div_cx520_string_ellipsis() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function string-ellipsis)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'subr-x)
  (string-ellipsis "hello world" 5))
"##,
        expect,
    );
}

#[test]
fn div_cx520_string_pad_left_right() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"hello     \" \"     hello\" \"hello\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-pad "hello" 10)
      (string-pad "hello" 10 nil t)
      (string-pad "hello" 3))
"##,
        expect,
    );
}

#[test]
fn div_cx520_string_truncate_left() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'subr-x)
  (list (string-truncate-left 5 "hello world")
        (string-truncate-left 20 "short")))
"##,
        expect,
    );
}

#[test]
fn div_cx520_string_search_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 nil 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-search "world" "hello world")
      (string-search "xyz" "hello world")
      (string-search "o" "hello world"))
"##,
        expect,
    );
}

#[test]
fn div_cx520_string_repeat_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function string-repeat)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-repeat "ab" 3) (string-repeat "x" 0) (string-repeat "" 5))
"##,
        expect,
    );
}

#[test]
fn div_cx520_format_field_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"   hi\" \"hi   \" \"00042\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%5s" "hi") (format "%-5s" "hi") (format "%05d" 42))
"##,
        expect,
    );
}

#[test]
fn div_cx520_format_decimal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"42\" \"-42\" \"377\" \"ff\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%d" 42) (format "%d" -42) (format "%o" 255) (format "%x" 255))
"##,
        expect,
    );
}
