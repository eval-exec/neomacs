//! Complex combo batch 193 — `string` operations deep: `string-match-p`,
//! `string-search`, `compare-strings`, `string-distance`, `assoc-string`,
//! `split-string` with TRIM, `string-replace` (Emacs 28+).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx193_string_match_p_casefold_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 4 4 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-match-p "hello" "say hello world")
      (string-match-p "HELLO" "say hello world")
      (let ((case-fold-search t)) (string-match-p "HELLO" "say hello world"))
      (let ((case-fold-search nil)) (string-match-p "hello" "say Hello world")))
"##,
        expect,
    );
}

#[test]
fn div_cx193_compare_strings_with_casefold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t -3 t 1 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (compare-strings "abc" nil nil "abc" nil nil)
      (compare-strings "abc" nil nil "abd" nil nil)
      (compare-strings "abc" nil nil "ABC" nil nil t)
      (compare-strings "abc" nil nil "ABC" nil nil nil)
      (compare-strings "abc" 0 3 "xabc" 1 4)
      (compare-strings "abc" 0 2 "abx" 0 2))
"##,
        expect,
    );
}

#[test]
fn div_cx193_string_distance_levenshtein() {
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
    );
}

#[test]
fn div_cx193_split_string_trim_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"hello\" \"world\") (\"a\" \"b\" \"c\") (\"\" \"hello\" \"world\" \"\") (\"alpha\" \"beta\" \"gamma\") (\"a\" \"b\" \"c\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (split-string "  hello  world  " "[ \t]+" t)
      (split-string ",a,b,c," "," t)
      (split-string "  hello  world  " "[ \t]+" nil)
      (split-string "alpha,beta,gamma," "," t)
      (split-string "a\nb\nc\n" "\n" t))
"##,
        expect,
    );
}

#[test]
fn div_cx193_string_replace_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-length-argument)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (string-replace "o" "0" "hello world")
          (string-replace "world" "Emacs" "hello world")
          (string-replace "" "X" "abc")
          (string-replace "a" "" "banana"))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx193_string_prefix_suffix_predicates() {
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
    );
}

#[test]
fn div_cx193_string_lines_split() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"line1\" \"line2\" \"line3\") (\"line1\" \"line2\" \"line3\") 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (string-lines "line1\nline2\nline3")
          (string-lines "line1\nline2\nline3\n")
          (length (string-lines "a\nb\nc\nd")))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx193_string_pad_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"hello     \" \"hello-----\" \"hello\" \"hello\" \"hello\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (string-pad "hello" 10)
          (string-pad "hello" 10 ?-)
          (string-pad "hello" 3)
          (string-pad "hello" 5)
          (string-pad "hello" 0))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx193_string_version_lessp_with_suffixes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-version-lessp "file2.txt" "file10.txt")
      (string-version-lessp "file10.txt" "file2.txt")
      (string-version-lessp "file1.0" "file1.1")
      (string-version-lessp "file1.10" "file1.2")
      (string-version-lessp "1" "2")
      (string-version-lessp "2" "10"))
"##,
        expect,
    );
}

#[test]
fn div_cx193_string_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s1 "kitten")
       (s2 "sitting")
       (dist (string-distance s1 s2))
       (parts (split-string "  hello  world  " "[ \t]+" t)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (format "dist(%s,%s)=%d parts=%S" s1 s2 dist parts))
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 10))
          (ov (make-overlay 4 18)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 25)
      (let ((state (list dist parts
                         (string-prefix-p "dist" (buffer-string))
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
    );
}
