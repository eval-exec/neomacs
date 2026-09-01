//! Oracle parity tests for GNU `subr.el` buffer matching helpers.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_subr_buffer_match_p_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU subr.el:buffer-match-p implements a condition language over buffer
    // names, predicate functions, modes, display categories, this-command, and
    // recursive not/or/and forms.  match-buffers pushes matches, so explicit
    // buffer lists come back in reverse match order.
    let form = r#"(let ((b1 (generate-new-buffer "*neovm-match-alpha*"))
      (b2 (generate-new-buffer "*neovm-match-beta*")))
  (unwind-protect
      (progn
        (with-current-buffer b1 (setq major-mode 'text-mode))
        (with-current-buffer b2 (setq major-mode 'emacs-lisp-mode))
        (let ((this-command 'find-file)
              (buffer-match-p--past-warnings nil))
          (list
           (mapcar (lambda (cond)
                     (list cond
                           (buffer-match-p cond b1)
                           (buffer-match-p cond b2)))
                   (list t nil "alpha" "beta"
                         '(major-mode . text-mode)
                         '(major-mode . emacs-lisp-mode)
                         '(derived-mode . prog-mode)
                         '(this-command . find-file)
                         '(this-command . (save-buffer find-file))
                         '(not . ("alpha"))
                         '(or . ("nomatch" "beta"))
                         '(and . ("alpha" (major-mode . text-mode)))))
           (buffer-match-p '(category . demo) b1 '(nil (category . demo)))
           (buffer-match-p '(category . other) b1 '(nil (category . demo)))
           (buffer-match-p
            (lambda (buf extra) (list (buffer-name (get-buffer buf)) extra))
            b1 'payload)
           (buffer-match-p
            (lambda (buf) (string-match-p "alpha" (buffer-name (get-buffer buf))))
            b1)
           (mapcar #'buffer-name
                   (match-buffers '(or . ("alpha" "beta")) (list b1 b2))))))
    (kill-buffer b1)
    (kill-buffer b2)))"#;
    let expect = expect_test::expect![[
        r#""OK (((t t t) (nil nil nil) (\"alpha\" t nil) (\"beta\" nil t) ((major-mode . text-mode) t nil) ((major-mode . emacs-lisp-mode) nil t) ((derived-mode . prog-mode) nil t) ((this-command . find-file) t t) ((this-command save-buffer find-file) t t) ((not \"alpha\") nil t) ((or \"nomatch\" \"beta\") nil t) ((and \"alpha\" (major-mode . text-mode)) t nil)) t nil t t (\"*neovm-match-beta*\" \"*neovm-match-alpha*\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
