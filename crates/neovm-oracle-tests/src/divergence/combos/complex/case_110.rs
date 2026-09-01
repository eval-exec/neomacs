//! Complex combo batch 110 — search / re-search / match-data with
//! multi-byte, case-fold, syntax tables, and word boundaries across
//! narrow.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx110_search_forward_basic_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (12 7 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "Hello world from Emacs")
  (goto-char 1)
  (search-forward "world")
  (list (point) (match-beginning 0) (match-end 0)))
"##,
        expect,
    );
}

#[test]
fn div_cx110_search_forward_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (7 11 12 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "Hello café 世界 from Emacs")
  (goto-char 1)
  (search-forward "café")
  (let ((cafe-beg (match-beginning 0))
        (cafe-end (match-end 0)))
    (search-forward "世界")
    (list cafe-beg cafe-end
          (match-beginning 0) (match-end 0))))
"##,
        expect,
    );
}

#[test]
fn div_cx110_search_backward_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (18 18 22)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "alpha beta gamma beta delta")
  (goto-char (point-max))
  (search-backward "beta")
  (list (point) (match-beginning 0) (match-end 0)))
"##,
        expect,
    );
}

#[test]
fn div_cx110_word_search_forward_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 4) 21 24)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "the quick brown fox the theocratic")
  (goto-char 1)
  (word-search-forward "the")
  (let ((first (list (match-beginning 0) (match-end 0))))
    (word-search-forward "the")
    (list first (match-beginning 0) (match-end 0))))
"##,
        expect,
    );
}

#[test]
fn div_cx110_re_search_forward_with_groups_and_quantifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"name\" \"alpha\" \"age\" \"42\" (#<marker in no buffer> #<marker in no buffer> #<marker in no buffer> #<marker in no buffer> #<marker in no buffer> #<marker in no buffer>))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "name:alpha age:42 city:Tokyo name:beta age:30")
  (goto-char 1)
  (re-search-forward "\\(\\w+\\):\\(\\w+\\)")
  (let ((g1 (match-string 1))
        (g2 (match-string 2)))
    (re-search-forward "\\(\\w+\\):\\(\\w+\\)")
    (list g1 g2
          (match-string 1) (match-string 2)
          (match-data))))
"##,
        expect,
    );
}

#[test]
fn div_cx110_re_search_backward_with_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"e\" \"gamma\" 25 32)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "name:alpha name:beta name:gamma")
  (goto-char (point-max))
  (re-search-backward "\\(\\w+\\):\\(\\w+\\)")
  (list (match-string 1) (match-string 2)
        (match-beginning 0) (match-end 0)))
"##,
        expect,
    );
}

#[test]
fn div_cx110_looking_at_with_alternation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \"Hello\" t \"Hello\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "Hello world foo bar baz")
  (goto-char 1)
  (list (looking-at "\\(hello\\|world\\|foo\\)" )
        (match-string 0)
        (looking-at "[A-Z][a-z]+")
        (match-string 0)
        (looking-at "x")))
"##,
        expect,
    );
}

#[test]
fn div_cx110_match_data_save_restore_via_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<killed buffer> 0 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (saved)
  (with-temp-buffer
    (insert "first second third")
    (string-match "\\(\\w+\\) \\(\\w+\\)" (buffer-string))
    (setq saved (match-data))
    (string-match "no match" "different")
    (set-match-data saved)
    (list (match-string 1)
          (match-string 2)
          (match-data))))
"##,
        expect,
    );
}

#[test]
fn div_cx110_search_with_bound_and_noerror() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (12 nil 20 nil 20)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "aaa bbb ccc ddd eee")
  (goto-char 1)
  (list (search-forward "ccc" 12 t)
        (search-forward "eee" 12 t)
        (search-forward "eee" 20 t)
        (search-forward "zzz" nil t)
        (point)))
"##,
        expect,
    );
}

#[test]
fn div_cx110_search_with_narrow_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (19 nil 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "outside-BEG inside-content outside-END")
  (narrow-to-region 13 27)
  (goto-char (point-min))
  (let ((in-buf (search-forward "inside" nil t))
        (out-buf-1 (search-forward "outside" nil t)))
    (widen)
    (goto-char 1)
    (let ((out-buf-2 (search-forward "outside" nil t)))
      (list in-buf out-buf-1 out-buf-2))))
"##,
        expect,
    );
}

#[test]
fn div_cx110_case_fold_search_t_affects_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 6 nil 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "Hello WORLD Foo")
  (list
   (let ((case-fold-search nil)) (goto-char 1) (search-forward "hello" nil t))
   (let ((case-fold-search t))   (goto-char 1) (search-forward "hello" nil t))
   (let ((case-fold-search nil)) (goto-char 1) (search-forward "world" nil t))
   (let ((case-fold-search t))   (goto-char 1) (search-forward "world" nil t))))
"##,
        expect,
    );
}

#[test]
fn div_cx110_char_fold_search_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "café résumé naïve")
      (let ((char-fold-symmetric t))
        (goto-char 1)
        (let ((r1 (char-fold-search-forward "cafe" nil t)))
          (list r1))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx110_search_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "alpha 123 beta 456 gamma 789 delta 012 epsilon")
  (put-text-property 1 5 'face 'bold)
  (let ((m (set-marker (make-marker) 12))
        (ov (make-overlay 5 20)))
    (overlay-put ov 'face 'region)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 4 45)
    (undo-boundary)
    (let ((case-fold-search nil))
      (goto-char 1)
      (while (re-search-forward "\\b[a-z]+\\b" nil t)
        (replace-match (upcase (match-string 0))))
    (let ((state (list (buffer-string)
                       (marker-position m)
                       (overlay-start ov) (overlay-end ov)
                       (point-min) (point-max)
                       (text-properties-at 1))))
      (undo) (undo)
      (widen)
      (list state
            (buffer-string) (marker-position m)
            (overlay-start ov) (overlay-end ov)
            (text-properties-at 1)))))
"##,
        expect,
    );
}
