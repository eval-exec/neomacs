//! Oracle parity tests for GNU `rect.el` rectangle editing semantics.
//!
//! Rectangle commands depend on buffer columns, short lines, tabs, fill
//! behavior, point/mark placement, and canonical returned strings.  These tests
//! pin programmatic rectangle APIs against GNU Emacs.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_rectangle_extract_delete_and_bounds() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'rect)
  (with-temp-buffer
    (insert "abcdef\n012345\nxy\n")
    (let* ((start (progn (goto-char (point-min)) (move-to-column 2) (point)))
           (end (progn (forward-line 2) (move-to-column 5 t) (point)))
           (extracted (extract-rectangle start end))
           (bounds (extract-rectangle-bounds start end))
           (deleted (delete-extract-rectangle start end))
           (after-delete (buffer-string)))
      (list extracted bounds deleted after-delete (point)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((\"cde\" \"234\" \"   \") ((3 . 6) (10 . 13) (17 . 20)) (\"cde\" \"234\" \"   \") \"abf\\n015\\nxy\\n\" 11)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_rectangle_fill_open_clear_string_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'rect)
  (list
   (with-temp-buffer
     (insert "abc\nx\n12345\n")
     (let* ((start (progn (goto-char (point-min)) (move-to-column 2) (point)))
            (end (progn (forward-line 2) (move-to-column 4) (point))))
       (open-rectangle start end t)
       (buffer-string)))
   (with-temp-buffer
     (insert "abcdef\n012345\n")
     (let* ((start (progn (goto-char (point-min)) (move-to-column 1) (point)))
            (end (progn (forward-line 1) (move-to-column 4) (point))))
       (clear-rectangle start end)
       (buffer-string)))
   (with-temp-buffer
     (insert "abcdef\n012345\n")
     (let* ((start (progn (goto-char (point-min)) (move-to-column 1) (point)))
            (end (progn (forward-line 1) (move-to-column 4) (point))))
       (string-rectangle start end "XX")
       (buffer-string)))
   (with-temp-buffer
     (insert "abcdef\n012345\n")
     (let* ((start (progn (goto-char (point-min)) (move-to-column 2) (point)))
            (end (progn (forward-line 1) (move-to-column 2) (point))))
       (string-insert-rectangle start end "|")
       (buffer-string)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"ab  c\\nx   \\n12  345\\n\" \"a   ef\\n0   45\\n\" \"aXXef\\n0XX45\\n\" \"ab|cdef\\n01|2345\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_rectangle_tabs_short_lines_and_dimensions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'rect)
  (let ((tab-width 8)
        (indent-tabs-mode nil))
    (with-temp-buffer
      (insert "a\tbcd\nshort\n\tz\n")
      (let* ((start (progn (goto-char (point-min)) (move-to-column 1) (point)))
             (end (progn (forward-line 2) (move-to-column 9) (point))))
        (list
         (extract-rectangle start end)
         (rectangle-dimensions start end)
         (rectangle-position-as-coordinates start)
         (rectangle-position-as-coordinates end)
         (progn
           (delete-rectangle start end t)
           (buffer-string)))))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((\"       b\" \"hort    \" \"       z\") (8 . 3) (1 . 1) (9 . 3) \"acd\\ns\\n \\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_rectangle_insert_number_and_intersection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'rect)
  (list
   (with-temp-buffer
     (insert "left\nright\n")
     (goto-char (point-min))
     (move-to-column 2)
     (insert-rectangle '("AA" "BBBB" "C"))
     (list (buffer-string) (mark) (point)))
   (with-temp-buffer
     (insert "aaa\nbbb\nccc\n")
     (let* ((start (progn (goto-char (point-min)) (point)))
            (end (progn (forward-line 2) (point))))
       (rectangle-number-lines start end 7 "[%02d] ")
       (buffer-string)))
   (rectangle-intersect-p '(0 . 0) '(3 . 2) '(2 . 1) '(4 . 3))
   (rectangle-intersect-p '(0 . 0) '(2 . 2) '(2 . 0) '(1 . 1))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((\"leAAft\\nriBBBBght\\n  C\" 3 21) \"[07] aaa\\n[08] bbb\\n[09] ccc\\n\" t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
