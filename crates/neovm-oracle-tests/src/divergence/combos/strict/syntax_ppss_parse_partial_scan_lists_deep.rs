//! Strict combo oracle probes, batch 147: the C-level syntax engine via
//! parse-partial-sexp state vectors (string/comment/quote/nesting states),
//! scan-lists / scan-sexps forward+backward navigation, and syntax-ppss cache
//! results at anchor points inside docstrings, comments, nested lists, and
//! quoted forms.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_parse_partial_sexp_state_vectors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun foo (a b)\n  \"a doc string\"\n  ;; line comment\n  (let ((x (+ a 1)))\n    `(q ,@(list x b) . ,a)))\n")
  (let ((probe (lambda (target)
                 (save-excursion
                   (goto-char (point-min))
                   (search-forward target)
                   (parse-partial-sexp (point-min) (point))))))
    (list (funcall probe "doc string")
          (funcall probe "line comment")
          (funcall probe "defun")
          (funcall probe "(+ a 1)")
          (funcall probe "@(list")
          (funcall probe ". ,a")
          (funcall probe ")))"))))
"##;
    let expect = expect_test::expect![[
        r#""OK ((1 1 12 34 nil nil 0 nil 20 (1) nil) (1 1 20 nil t nil 0 nil 37 (1) nil) (1 1 2 nil nil nil 0 nil nil (1) nil) (4 61 64 nil nil nil 0 nil nil (1 55 60 61) nil) (4 84 85 nil nil nil 0 nil nil (1 55 79 84) nil) (3 79 98 nil nil nil 0 nil nil (1 55 79) nil) (2 55 60 nil nil nil 0 nil nil (1 55) nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_scan_lists_scan_sexps_forward_backward() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(a (b (c (d) e) f) g) (h i) \"str ) not paren\" ;; c1\n")
  (let ((at-open (save-excursion (goto-char (point-min)) (search-forward "(b") (1- (point))))
        (scan-fwd (lambda (depth) (save-excursion (scan-lists (point) depth 0))))
        (scan-fwd-sexp (lambda (n) (save-excursion (scan-sexps (point) n)))))
    (list (save-excursion (goto-char at-open) (scan-lists (point) 1 0))
          (save-excursion (goto-char at-open) (scan-lists (point) 1 -1))
          (save-excursion (goto-char at-open) (scan-lists (point) 1 1))
          (save-excursion (goto-char (point-min)) (scan-sexps (point-min) 1))
          (condition-case err (scan-lists (point-min) 99 0) (scan-error (cdr err)))
          (condition-case err (save-excursion (goto-char (point-min)) (forward-char 1) (scan-sexps (point) -1)) (scan-error (cadr err)))
          (save-excursion (goto-char at-open) (backward-prefix-chars) (point)))))
"##;
    let expect = expect_test::expect![[
        r#""OK (16 8 19 22 nil \"Containing expression ends prematurely\" 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_syntax_ppss_cache_anchors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun probe-fn ()\n  \"doc\"\n  ;; comment with (parens)\n  (list 'a \"b\" [1 2 3]))\n")
  (let ((anchor (lambda (target)
                  (save-excursion
                    (goto-char (point-min))
                    (search-forward target)
                    (point)))))
    (list (syntax-ppss (funcall anchor "doc"))
          (syntax-ppss (funcall anchor "(parens)"))
          (syntax-ppss (funcall anchor "[1 2 3]"))
          (syntax-ppss (point-max))
          (save-excursion (goto-char (funcall anchor "doc")) (syntax-ppss)))))
"##;
    let expect = expect_test::expect![[
        r#""OK ((1 1 17 34 nil nil 0 nil 22 (1) nil) (1 1 nil nil t nil 1 nil 30 (1) nil) (2 57 70 nil nil nil 1 nil nil (1 57) nil) (0 nil 1 nil nil nil 0 nil nil nil nil) (1 1 nil 34 nil nil 1 nil 22 (1) nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
