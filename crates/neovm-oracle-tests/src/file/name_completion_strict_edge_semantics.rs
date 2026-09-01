//! Oracle parity tests for GNU file-name completion semantics.
//!
//! GNU implements `file-name-completion` and `file-name-all-completions` in
//! `src/dired.c`.  The completion path appends "/" for directories, filters
//! `completion-ignored-extensions` only for `file-name-completion`, applies
//! `completion-regexp-list` to both APIs, and calls predicates with relative
//! names while dynamically binding `default-directory` to DIRECTORY.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_file_name_completion_prefix_ignored_case_regex_and_predicate_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((dir (make-temp-file "neomacs-oracle-fncomp-" t)))
  (unwind-protect
      (progn
        (make-directory (expand-file-name "subdir" dir))
        (dolist (pair '(("alpha" . "a")
                        ("alphabet" . "abc")
                        ("beta.el" . "el")
                        ("beta.elc" . "elc")
                        ("case.TXT" . "case")))
          (with-temp-file (expand-file-name (car pair) dir)
            (insert (cdr pair))))
        (let ((dirslash (file-name-as-directory dir)))
          (list
           (file-name-completion "alp" dir)
           (file-name-completion "alpha" dir)
           (file-name-completion "alphabet" dir)
           (file-name-completion "sub" dir)
           (file-name-completion "missing" dir)
           (sort (file-name-all-completions "alp" dir) #'string<)
           (let ((completion-ignored-extensions '(".elc")))
             (list
              (file-name-completion "beta.e" dir)
              (file-name-completion "beta.el" dir)
              (sort (file-name-all-completions "beta.e" dir) #'string<)))
           (let ((completion-regexp-list '("\\.TXT\\'")))
             (list
              (file-name-completion "c" dir)
              (sort (file-name-all-completions "c" dir) #'string<)))
           (let ((completion-ignore-case t))
             (list
              (file-name-completion "CASE" dir)
              (sort (file-name-all-completions "case" dir) #'string<)))
           (let ((seen nil))
             (list
              (file-name-completion
               "sub" dir
               (lambda (name)
                 (push (list (equal default-directory dirslash)
                             name
                             (file-directory-p name))
                       seen)
                 (file-directory-p name)))
              (nreverse seen)))
           (condition-case err
               (file-name-completion 42 dir)
             (error (list (car err) (cdr err))))
           (condition-case err
               (file-name-all-completions "a" 42)
             (error (list (car err) (cdr err)))))))
    (delete-directory dir t)))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"alpha\" \"alpha\" t \"subdir/\" nil (\"alpha\" \"alphabet\") (\"beta.el\" t (\"beta.el\" \"beta.elc\")) (\"case.TXT\" (\"case.TXT\")) (\"case.TXT\" (\"case.TXT\")) (\"subdir/\" ((nil \"subdir/\" t))) (wrong-type-argument (stringp 42)) (wrong-type-argument (stringp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
