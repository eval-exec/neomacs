//! Complex combo batch 310 — `string` operations ultimate: `string-replace`,
//! `string-trim` variants, `split-string` with TRIM, `string-pad`, `string-limit`,
//! `string-lines`, `string-join` with separators.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx310_string_replace_all_occurrences() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-length-argument 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-replace "o" "0" "hello world foo")
      (string-replace "world" "Emacs" "hello world")
      (string-replace "" "X" "abc")
      (string-replace "a" "" "banana")
      (string-replace " " "-" "a b c d e"))
"##,
        expect,
    )
}

#[test]
fn div_cx310_string_trim_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"hello\" \"hello\" \"hello\" \"hello\" \"hello\" \"hello\" \"hello\" \"hello\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-trim "   hello   ")
      (string-trim-left "   hello")
      (string-trim-right "hello   ")
      (string-trim "\n\nhello\n\n")
      (string-trim "xxhelloxx" "x+" "x+")
      (string-trim "  hello  " "[ ]+" "[ ]+")
      (string-trim-left "-----hello" "-+")
      (string-trim-right "hello-----" "-+"))
"##,
        expect,
    )
}

#[test]
fn div_cx310_split_string_with_trim_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"hello\" \"world\") (\"a\" \"b\" \"c\") (\"\" \"hello\" \"world\" \"\") (\"alpha\" \"beta\" \"gamma\") (\"a\" \"b\" \"c\") (\"\") (\"no delimiters here\") (\"alpha\" \"beta\" \"gamma\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (split-string "  hello  world  " "[ \t]+" t)
      (split-string ",a,b,c," "," t)
      (split-string "  hello  world  " "[ \t]+" nil)
      (split-string "alpha,beta,gamma," "," t)
      (split-string "a\nb\nc\n" "\n" t)
      (split-string "" ",")
      (split-string "no delimiters here" ",")
      (split-string "alpha beta gamma"))
"##,
        expect,
    )
}

#[test]
fn div_cx310_string_pad_and_limit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"hello     \" \"hello-----\" \"hello\" \"hello\" \"hello\" \"hello\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (string-pad "hello" 10)
          (string-pad "hello" 10 ?-)
          (string-pad "hello" 3)
          (string-pad "hello" 5)
          (string-pad "hello" 0)
          (when (fboundp 'string-limit) (string-limit "hello world" 5)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx310_string_lines_and_join() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"line1\" \"line2\" \"line3\") 4 \"alpha,beta,gamma\" \"alpha -> beta -> gamma\" \"single\" \"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (when (fboundp 'string-lines) (string-lines "line1\nline2\nline3"))
          (when (fboundp 'string-lines) (length (string-lines "a\nb\nc\nd")))
          (string-join '("alpha" "beta" "gamma") ",")
          (string-join '("alpha" "beta" "gamma") " -> ")
          (string-join '("single"))
          (string-join '()))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx310_string_distance_levenshtein_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 2 0 0 1 1 3 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-distance "kitten" "sitting")
      (string-distance "flaw" "lawn")
      (string-distance "same" "same")
      (string-distance "" "")
      (string-distance "a" "")
      (string-distance "" "a")
      (string-distance "abc" "xyz")
      (string-distance "café" "cafe"))
"##,
        expect,
    )
}

#[test]
fn div_cx310_string_version_lessp_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t nil t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-version-lessp "file2.txt" "file10.txt")
      (string-version-lessp "file10.txt" "file2.txt")
      (string-version-lessp "file1.0" "file1.1")
      (string-version-lessp "file1.10" "file1.2")
      (string-version-lessp "1" "2")
      (string-version-lessp "2" "10")
      (string-version-lessp "v1.0.0" "v1.0.1")
      (string-version-lessp "v1.0.9" "v1.0.10"))
"##,
        expect,
    )
}

#[test]
fn div_cx310_string_prefix_suffix_predicates_full() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t nil t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-prefix-p "hello" "hello world")
      (string-prefix-p "world" "hello world")
      (string-prefix-p "HELLO" "hello world" t)
      (string-prefix-p "HELLO" "hello world" nil)
      (string-suffix-p "world" "hello world")
      (string-suffix-p "hello" "hello world")
      (string-suffix-p "WORLD" "hello world" t))
"##,
        expect,
    )
}

#[test]
fn div_cx310_compare_strings_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t -3 -4 3 t 1 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (compare-strings "abc" nil nil "abc" nil nil)
      (compare-strings "abc" nil nil "abd" nil nil)
      (compare-strings "abc" nil nil "abcd" nil nil)
      (compare-strings "abc" nil nil "ab" nil nil)
      (compare-strings "abc" nil nil "ABC" nil nil t)
      (compare-strings "abc" nil nil "ABC" nil nil nil)
      (compare-strings "abc" 0 3 "xabc" 1 4)
      (compare-strings "abc" 0 2 "abx" 0 2))
"##,
        expect,
    )
}

#[test]
fn div_cx310_string_ops_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s1 "  hello world  ")
       (s2 "alpha,beta,gamma")
       (parts (split-string s2 "," t))
       (trimmed (string-trim s1)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (format "%s | %s | %S" trimmed s2 parts))
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 10))
          (ov (make-overlay 4 20)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 28)
      (let ((state (list trimmed s2 parts
                         (string-distance "kitten" "sitting")
                         (string-version-lessp "file2" "file10")
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    )
}
