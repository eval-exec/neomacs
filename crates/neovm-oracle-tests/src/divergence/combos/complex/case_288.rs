//! Complex combo batch 288 — `string` comparison edge cases, `char`
//! operations, `compare-buffer-substrings`, `replace-regexp-in-string`
//! with function substitution, `match-data` save/restore/set.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx288_string_comparison_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil t nil t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-lessp "abc" "abd")
      (string-lessp "abc" "abc")
      (string-lessp "abc" "ab")
      (string-version-lessp "file2.txt" "file10.txt")
      (string-version-lessp "file10.txt" "file2.txt")
      (string< "abc" "abd")
      (string= "abc" "abc")
      (string-equal "ABC" "abc"))
"##,
        expect,
    )
}

#[test]
fn div_cx288_char_comparison_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-lessp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (char-equal ?a ?a)
      (char-equal ?a ?A)
      (char-equal ?A ?a)
      (char-equal ?a ?b)
      (char-lessp ?a ?b)
      (char-lessp ?b ?a)
      (char< ?A ?B)
      (char-equal ?à ?À))
"##,
        expect,
    )
}

#[test]
fn div_cx288_compare_buffer_substrings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 -1 0 -1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "Hello World Hello")
  (list (compare-buffer-substrings nil 1 6 nil 1 6)
        (compare-buffer-substrings nil 1 6 nil 7 12)
        (compare-buffer-substrings nil 1 6 nil 13 18)
        (compare-buffer-substrings nil 1 5 nil 7 11)))
"##,
        expect,
    )
}

#[test]
fn div_cx288_replace_regexp_in_string_with_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"abc # def #\" \"HELLO WORLD\" \"beta alpha\" \"a_b_c\" \"*lph*b*t\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (replace-regexp-in-string "[0-9]+" "#" "abc 123 def 456")
      (replace-regexp-in-string "\\b\\w+\\b" (lambda (m) (upcase m)) "hello world")
      (replace-regexp-in-string "\\(\\w+\\) \\(\\w+\\)" "\\2 \\1" "alpha beta")
      (replace-regexp-in-string " +" "_" "a  b   c")
      (replace-regexp-in-string "[aeiou]" "*" "alphabet" t))
"##,
        expect,
    )
}

#[test]
fn div_cx288_match_data_save_set_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<killed buffer> 0 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (saved)
  (with-temp-buffer
    (insert "alpha beta gamma")
    (string-match "\\(\\w+\\) \\(\\w+\\) \\(\\w+\\)" (buffer-string))
    (setq saved (match-data))
    (string-match "no-match" "different")
    (set-match-data saved)
    (list (match-data)
          (match-string 1)
          (match-string 2)
          (match-string 3))))
"##,
        expect,
    )
}

#[test]
fn div_cx288_looking_back_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "Hello World Foo")
  (goto-char 6)
  (list (looking-back "Hello" 1)
        (looking-back "Hello" (- (point) 5))
        (looking-back "World" 1)
        (looking-at "World")))
"##,
        expect,
    )
}

#[test]
fn div_cx288_skip_syntax_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 15 16 19 \"hello_world\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "   hello_world 123 rest")
  (goto-char 1)
  (skip-syntax-forward " ")
  (let ((p1 (point)))
    (skip-syntax-forward "w_")
    (let ((p2 (point)))
      (skip-syntax-forward " ")
      (let ((p3 (point)))
        (skip-syntax-forward "w")
        (list p1 p2 p3 (point) (buffer-substring p1 p2))))))
"##,
        expect,
    )
}

#[test]
fn div_cx288_char_before_following_preceding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (67 68 67 68)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "ABCDEF")
  (goto-char 4)
  (list (char-before)
        (following-char)
        (preceding-char)
        (char-after)))
"##,
        expect,
    )
}

#[test]
fn div_cx288_bolp_eolp_bobp_eobp_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "line1\nline2\n")
  (goto-char 1)
  (list (bobp) (bolp) (eolp) (eobp))
  (end-of-line)
  (list (eolp) (eobp))
  (goto-char (point-max))
  (list (eobp) (eolp)))
"##,
        expect,
    )
}

#[test]
fn div_cx288_string_ops_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s1 "alpha beta gamma")
      (s2 "ALPHA BETA GAMMA")
      (s3 "file10.txt"))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (format "%s vs %s vs %s" s1 s2 s3))
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 10))
          (ov (make-overlay 4 20)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 28)
      (let ((state (list (string-lessp s1 s2)
                         (string-version-lessp s3 "file2.txt")
                         (replace-regexp-in-string "[A-Z]+" (lambda (m) (downcase m)) s2)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen()
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1)))))))
"##,
        expect,
    )
}
