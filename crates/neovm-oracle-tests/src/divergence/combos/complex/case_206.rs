//! Complex combo batch 206 — `isearch` / `occur` / `replace` / `match-data`
//! deep with regex groups, backreferences, word boundaries across multibyte.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx206_regex_groups_full_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((0 24 0 5 6 9 10 14 15 18 19 24) \"alpha 123 beta 456 gamma\" \"alpha\" \"123\" \"beta\" \"456\" \"gamma\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "alpha 123 beta 456 gamma"))
  (string-match "\\(\\w+\\) \\([0-9]+\\) \\(\\w+\\) \\([0-9]+\\) \\(\\w+\\)" s)
  (list (match-data)
        (match-string 0 s) (match-string 1 s) (match-string 2 s)
        (match-string 3 s) (match-string 4 s) (match-string 5 s)))
"##,
        expect,
    );
}

#[test]
fn div_cx206_backreference_in_regex_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 \"hello\" nil 0 \"42\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-match "\\(\\w+\\) \\1" "hello hello")
      (match-string 1 "hello hello")
      (string-match "\\(\\w+\\) \\1" "hello world")
      (string-match "\\([0-9]+\\)-\\1" "42-42")
      (match-string 1 "42-42"))
"##,
        expect,
    );
}

#[test]
fn div_cx206_word_boundary_across_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 \"世界\" 9 \"café\" 17 \"123\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "hello 世界 café 世界 123"))
  (list (string-match "\\b世界\\b" s)
        (match-string 0 s)
        (string-match "\\bcafé\\b" s)
        (match-string 0 s)
        (string-match "\\b[0-9]+\\b" s)
        (match-string 0 s)))
"##,
        expect,
    );
}

#[test]
fn div_cx206_looking_at_chain_then_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \"First\" 6 \"First\" 13 \"Second\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "First Second Third")
  (goto-char 1)
  (list (looking-at "[A-Z][a-z]+")
        (match-string 0)
        (re-search-forward "[a-z]+" nil t)
        (match-string 0)
        (re-search-forward "[a-z]+" nil t)
        (match-string 0)))
"##,
        expect,
    );
}

#[test]
fn div_cx206_replace_match_with_backref_in_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"alpha:name age:42 city:Tokyo\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "name:alpha age:42 city:Tokyo")
  (goto-char 1)
  (re-search-forward "\\(\\w+\\):\\(\\w+\\)")
  (replace-match "\\2:\\1")
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx206_skip_chars_forward_backward_combination() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 12 15 23 \"hello123\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "   hello123   world456   ")
  (goto-char 1)
  (skip-chars-forward " \t")
  (let ((p1 (point)))
    (skip-chars-forward "a-zA-Z0-9")
    (let ((p2 (point)))
      (skip-chars-forward " \t")
      (let ((p3 (point)))
        (skip-chars-forward "a-zA-Z0-9")
        (list p1 p2 p3 (point) (buffer-substring p1 p2))))))
"##,
        expect,
    );
}

#[test]
fn div_cx206_match_data_save_restore_set() {
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
    );
}

#[test]
fn div_cx206_occur_with_multiple_matches() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"alpha line\\nbeta line\\nalpha again\\ngamma line\\nalpha third\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "alpha line\nbeta line\nalpha again\ngamma line\nalpha third\n")
      (goto-char 1)
      (occur "alpha")
      (let ((ob (get-buffer "*Occur*")))
        (prog1 (when ob (buffer-string))
          (when ob (kill-buffer ob)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx206_case_fold_search_affects_re_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 6 \"Hello\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "Hello WORLD Foo Bar")
  (list
   (let ((case-fold-search nil))
     (goto-char 1)
     (re-search-forward "hello" nil t))
   (let ((case-fold-search t))
     (goto-char 1)
     (re-search-forward "hello" nil t))
   (let ((case-fold-search t))
     (goto-char 1)
     (re-search-forward "[a-z]+" nil t)
     (match-string 0))))
"##,
        expect,
    );
}

#[test]
fn div_cx206_regex_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "name:alpha age:42 city:Tokyo name:beta age:30")
  (put-text-property 1 5 'face 'bold)
  (let ((m (set-marker (make-marker) 12))
        (ov (make-overlay 5 20)))
    (overlay-put ov 'face 'region)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 3 45)
    (goto-char 1)
    (let ((matches
           (cl-loop for i from 0 below 3
                    while (re-search-forward "\\(\\w+\\):\\(\\w+\\)" nil t)
                    collect (list (match-string 1) (match-string 2)))))
      (let ((state (list matches
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1)))))
"##,
        expect,
    );
}
