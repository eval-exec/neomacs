//! GNU parity at the text line index's edit and narrowing boundaries.
//!
//! GNU's `forward-line` scans LF only (`src/cmds.c`), and its non-empty
//! final line counts as moved even when point reaches ZV. In contrast,
//! `line-number-at-pos` calls `display_count_lines` (`src/xdisp.c`), which
//! counts CR when selective-display is non-nil and not an integer.
//! `count-lines` is the Lisp implementation in `lisp/simple.el`.
//!
//! Run these with the line index in verify mode and its buffer/query
//! thresholds at zero to exercise small texts and every query. Expectations
//! are populated by GNU with NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_text_index_forward_line_endpoints_and_shortages() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(mapcar
 (lambda (text)
   (with-temp-buffer
     (insert text)
     (line-number-at-pos (point-max))
     (mapcar
      (lambda (start)
        (mapcar (lambda (n)
                  (goto-char start)
                  (list n (forward-line n) (point)))
                '(-100000 -2 -1 0 1 2 100000)))
      (list (point-min) (point-max)))))
 '("" "tail" "a\n" "\n\n" "é\n中\rz\n尾"))
"#;
    let expect = expect_test::expect![[
        r#""OK ((((-100000 -100000 1) (-2 -2 1) (-1 -1 1) (0 0 1) (1 1 1) (2 2 1) (100000 100000 1)) ((-100000 -100000 1) (-2 -2 1) (-1 -1 1) (0 0 1) (1 1 1) (2 2 1) (100000 100000 1))) (((-100000 -100000 1) (-2 -2 1) (-1 -1 1) (0 0 1) (1 0 5) (2 1 5) (100000 99999 5)) ((-100000 -100000 1) (-2 -2 1) (-1 -1 1) (0 0 1) (1 1 5) (2 2 5) (100000 100000 5))) (((-100000 -100000 1) (-2 -2 1) (-1 -1 1) (0 0 1) (1 0 3) (2 1 3) (100000 99999 3)) ((-100000 -99999 1) (-2 -1 1) (-1 0 1) (0 0 3) (1 1 3) (2 2 3) (100000 100000 3))) (((-100000 -100000 1) (-2 -2 1) (-1 -1 1) (0 0 1) (1 0 2) (2 0 3) (100000 99998 3)) ((-100000 -99998 1) (-2 0 1) (-1 0 2) (0 0 3) (1 1 3) (2 2 3) (100000 100000 3))) (((-100000 -100000 1) (-2 -2 1) (-1 -1 1) (0 0 1) (1 0 3) (2 0 7) (100000 99997 8)) ((-100000 -99998 1) (-2 0 1) (-1 0 3) (0 0 7) (1 1 8) (2 2 8) (100000 100000 8))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_text_index_selective_counts_narrowed_multibyte_and_unibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(mapcar
 (lambda (multibyte)
   (with-temp-buffer
     (set-buffer-multibyte multibyte)
     (insert (if multibyte "é\r中\n尾\r\nfin"
               (unibyte-string 233 13 255 10 254 13 10 102 105 110)))
     (line-number-at-pos (point-max))
     (mapcar
      (lambda (selective)
        (setq selective-display selective)
        (save-restriction
          (narrow-to-region 3 9)
          (list selective
                (mapcar (lambda (p)
                          (list (line-number-at-pos p)
                                (if (<= p (point-max))
                                    (line-number-at-pos p t)
                                  'outside)))
                        '(1 3 4 7 9 11))
                (mapcar (lambda (ends)
                          (count-lines (car ends) (cadr ends)))
                        '((3 3) (3 4) (3 7) (3 9) (9 3) (4 9)))
                (mapcar (lambda (start)
                          (mapcar (lambda (n)
                                    (goto-char start)
                                    (list (forward-line n) (point)))
                                  '(-20 0 1 20)))
                        (list (point-min) (point-max))))))
      '(nil t 2 hide))))
 '(t nil))
"#;
    let expect = expect_test::expect![[
        r#""OK (((nil ((1 1) (1 1) (1 1) (2 2) (3 3) (3 outside)) (0 1 2 3 3 3) (((-20 3) (0 3) (0 5) (17 9)) ((-18 3) (0 8) (1 9) (20 9)))) (t ((1 1) (1 2) (1 2) (3 4) (4 5) (4 outside)) (0 1 2 3 3 3) (((-20 3) (0 3) (0 5) (17 9)) ((-18 3) (0 8) (1 9) (20 9)))) (2 ((1 1) (1 1) (1 1) (2 2) (3 3) (3 outside)) (0 1 2 3 3 3) (((-20 3) (0 3) (0 5) (17 9)) ((-18 3) (0 8) (1 9) (20 9)))) (hide ((1 1) (1 2) (1 2) (3 4) (4 5) (4 outside)) (0 1 3 4 4 4) (((-20 3) (0 3) (0 5) (17 9)) ((-18 3) (0 8) (1 9) (20 9))))) ((nil ((1 1) (1 1) (1 1) (2 2) (3 3) (3 outside)) (0 1 2 3 3 3) (((-20 3) (0 3) (0 5) (17 9)) ((-18 3) (0 8) (1 9) (20 9)))) (t ((1 1) (1 2) (1 2) (3 4) (4 5) (4 outside)) (0 1 2 3 3 3) (((-20 3) (0 3) (0 5) (17 9)) ((-18 3) (0 8) (1 9) (20 9)))) (2 ((1 1) (1 1) (1 1) (2 2) (3 3) (3 outside)) (0 1 2 3 3 3) (((-20 3) (0 3) (0 5) (17 9)) ((-18 3) (0 8) (1 9) (20 9)))) (hide ((1 1) (1 2) (1 2) (3 4) (4 5) (4 outside)) (0 1 3 4 4 4) (((-20 3) (0 3) (0 5) (17 9)) ((-18 3) (0 8) (1 9) (20 9))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_text_index_queries_after_insertion_and_deletion_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (dotimes (_ 80) (insert "é\n中\rx\n"))
  (insert "tail")
  (buffer-enable-undo)
  (setq buffer-undo-list nil)
  (let ((probe (lambda ()
                 (list (buffer-size)
                       (line-number-at-pos (point-max))
                       (count-lines 1 (point-max))
                       (save-excursion
                         (goto-char 1)
                         (list (forward-line 100000) (point)))
                       (save-excursion
                         (goto-char (point-max))
                         (list (forward-line -100000) (point))))))
        results)
    (setq results (list (funcall probe)))
    (goto-char 100)
    (insert "u\nv\rw\n")
    (push (funcall probe) results)
    (primitive-undo 1 buffer-undo-list)
    (push (funcall probe) results)
    (setq buffer-undo-list nil)
    (delete-region 91 123)
    (push (funcall probe) results)
    (primitive-undo 1 buffer-undo-list)
    (push (funcall probe) results)
    (nreverse results)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((484 161 161 (99839 485) (-99840 1)) (490 163 163 (99837 491) (-99838 1)) (484 161 161 (99839 485) (-99840 1)) (452 150 150 (99850 453) (-99851 1)) (484 161 161 (99839 485) (-99840 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_text_index_queries_after_insert_file_contents_and_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((file (make-temp-file "neovm-line-index-gnu")))
  (unwind-protect
      (progn
        (let ((coding-system-for-write 'utf-8-unix))
          (with-temp-file file (insert "é\r\n中\n尾\nlast")))
        (with-temp-buffer
          (dotimes (_ 80) (insert "é\n中\rx\n"))
          (insert "tail")
          (let ((probe (lambda ()
                         (list (buffer-size)
                               (line-number-at-pos (point-max))
                               (count-lines (point-min) (point-max))
                               (save-excursion
                                 (goto-char (point-min))
                                 (list (forward-line 100000) (point)))
                               (save-excursion
                                 (goto-char (point-max))
                                 (list (forward-line -100000) (point))))))
                (coding-system-for-read 'utf-8-unix)
                results)
            (setq results (list (funcall probe)))
            (goto-char 100)
            (push (cadr (insert-file-contents file)) results)
            (push (funcall probe) results)
            (goto-char 200)
            (push (cadr (insert-file-contents file nil 2 11)) results)
            (push (funcall probe) results)
            (narrow-to-region 50 300)
            (funcall probe)
            (push (cadr (insert-file-contents file nil nil nil t)) results)
            (push (funcall probe) results)
            (widen)
            (push (funcall probe) results)
            (nreverse results))))
    (delete-file file)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((484 161 161 (99839 485) (-99840 1)) 11 (495 164 164 (99836 496) (-99837 1)) 5 (500 166 166 (99834 501) (-99835 1)) 11 (261 4 4 (99996 61) (-99997 50)) (261 86 86 (99914 262) (-99915 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_text_index_queries_after_replace_buffer_contents() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((source (generate-new-buffer " *neovm-line-index-source*")))
  (unwind-protect
      (with-temp-buffer
        (dotimes (_ 80) (insert "é\n中\rx\n"))
        (insert "tail")
        (let ((probe (lambda ()
                       (list (buffer-size)
                             (line-number-at-pos (point-max))
                             (line-number-at-pos (point-max) t)
                             (count-lines (point-min) (point-max))
                             (save-excursion
                               (goto-char (point-min))
                               (list (forward-line 100000) (point)))
                             (save-excursion
                               (goto-char (point-max))
                               (list (forward-line -100000) (point))))))
              results)
          (setq results (list (funcall probe)))
          (let ((text (buffer-substring-no-properties 1 (point-max))))
            (with-current-buffer source
              (insert text)
              (goto-char 100)
              (insert "u\nv\rw\n")
              (delete-region 200 223)))
          (push (replace-buffer-contents source) results)
          (push (funcall probe) results)
          (narrow-to-region 50 300)
          (funcall probe)
          (with-current-buffer source
            (erase-buffer)
            (insert "prefix\né\r\n中\n尾\nlast\nsuffix")
            (narrow-to-region 8 (- (point-max) 7)))
          (push (replace-buffer-contents source nil 0) results)
          (push (funcall probe) results)
          (widen)
          (push (funcall probe) results)
          (nreverse results)))
    (kill-buffer source)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((484 161 161 161 (99839 485) (-99840 1)) t (467 155 155 155 (99845 468) (-99846 1)) t (228 4 20 4 (99996 61) (-99997 50)) (228 75 75 75 (99925 229) (-99926 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
