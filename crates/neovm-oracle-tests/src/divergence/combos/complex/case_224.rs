//! Complex combo batch 224 — `thing-at-point` comprehensive matrix across
//! all thing types: word, symbol, list, sexp, line, sentence, paragraph,
//! page, defun, filename, url, email, whitespace, number.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx224_thing_at_point_word_and_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"my\" \"my_symbol_name\" (1 . 3) (1 . 15))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "my_symbol_name and word_here")
  (goto-char 3)
  (list (thing-at-point 'word)
        (thing-at-point 'symbol)
        (bounds-of-thing-at-point 'word)
        (bounds-of-thing-at-point 'symbol)))
"##,
        expect,
    );
}

#[test]
fn div_cx224_thing_at_point_line_and_sentence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"First sentence.\" \"First sentence.  Second sentence.  Third.\\n\" (1 . 16))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "First sentence.  Second sentence.  Third.\nSecond line.\n")
  (goto-char 5)
  (list (thing-at-point 'sentence)
        (thing-at-point 'line)
        (bounds-of-thing-at-point 'sentence)))
"##,
        expect,
    );
}

#[test]
fn div_cx224_thing_at_point_list_and_sexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"(gamma delta)\" \"gamma\" (13 . 26))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(alpha beta (gamma delta) epsilon)")
  (goto-char 15)
  (list (thing-at-point 'list)
        (thing-at-point 'sexp)
        (bounds-of-thing-at-point 'list)))
"##,
        expect,
    );
}

#[test]
fn div_cx224_thing_at_point_filename_and_url() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"/home/user/file.txt\" \"https://example.com/path?q=1\" (29 . 53) (29 . 57))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "see /home/user/file.txt and https://example.com/path?q=1 for details")
  (goto-char 6)
  (let ((fname (thing-at-point 'filename)))
    (goto-char 35)
    (let ((url (thing-at-point 'url)))
      (list fname url
            (bounds-of-thing-at-point 'filename)
            (bounds-of-thing-at-point 'url)))))
"##,
        expect,
    );
}

#[test]
fn div_cx224_thing_at_point_email() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"user@example.com\" (9 . 25))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "contact user@example.com for info")
  (goto-char 10)
  (list (thing-at-point 'email)
        (bounds-of-thing-at-point 'email)))
"##,
        expect,
    );
}

#[test]
fn div_cx224_thing_at_point_paragraph_and_page() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"Para one line A.\\nPara one line B.\\n\" (1 . 35))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "Para one line A.\nPara one line B.\n\nPara two line A.\nPara two line B.\n")
  (goto-char 5)
  (list (thing-at-point 'paragraph)
        (bounds-of-thing-at-point 'paragraph)))
"##,
        expect,
    );
}

#[test]
fn div_cx224_thing_at_point_defun_in_elisp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "(defun neo-cx224-foo ()\n  \"docstring\"\n  (+ 1 2))\n\n(next form)\n")
      (emacs-lisp-mode)
      (goto-char 10)
      (let ((defun-str (thing-at-point 'defun)))
        (list (stringp defun-str)
              (> (length (or defun-str "")) 0))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx224_thing_at_point_whitespace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"    \t   \" (6 . 14))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "word1    \t   word2")
  (goto-char 7)
  (list (thing-at-point 'whitespace)
        (bounds-of-thing-at-point 'whitespace)))
"##,
        expect,
    );
}

#[test]
fn div_cx224_thing_at_point_number() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (42 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "value = 42 end")
      (goto-char 10)
      (list (thing-at-point 'number)
            (thing-at-point 'whitespace)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx224_thing_at_point_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "my_symbol_1 (call arg) /home/user/file.txt https://example.com end")
  (put-text-property 1 5 'face 'bold)
  (let ((m (set-marker (make-marker) 10))
        (ov (make-overlay 4 20)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 2 50)
    (goto-char 3)
    (let ((word-tp (thing-at-point 'symbol))
          (word-bounds (bounds-of-thing-at-point 'symbol)))
      (goto-char 20)
      (let ((list-tp (thing-at-point 'list)))
        (goto-char 35)
        (let ((fname-tp (thing-at-point 'filename)))
          (goto-char 50)
          (let ((url-tp (thing-at-point 'url)))
            (let ((state (list word-tp word-bounds list-tp fname-tp url-tp
                               (buffer-string)
                               (marker-position m)
                               (overlay-start ov) (overlay-end ov)
                               (text-properties-at 1))))
              (undo)
              (widen)
              (list state (buffer-string) (marker-position m)
                    (overlay-start ov) (overlay-end ov)
                    (text-properties-at 1))))))))
"##,
        expect,
    );
}
