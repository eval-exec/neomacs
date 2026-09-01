//! Oracle parity tests for GNU `directory-files-recursively` semantics.
//!
//! GNU implements this in `lisp/files.el`.  Results are depth-first, sorted
//! within each directory, absolute on return, optionally include directories,
//! skip symlinked directories unless FOLLOW-SYMLINKS is non-nil, and can filter
//! descent with PREDICATE.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_directory_files_recursively_order_predicate_symlink_and_error_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dir (make-temp-file "neomacs-oracle-recursive-files-" t))
       (a (expand-file-name "a" dir))
       (b (expand-file-name "b" dir))
       (skip (expand-file-name "skip" dir))
       (alink (expand-file-name "alink" dir)))
  (unwind-protect
      (progn
        (make-directory a)
        (make-directory b)
        (make-directory skip)
        (write-region "" nil (expand-file-name "root.el" dir) nil 'silent)
        (write-region "" nil (expand-file-name "a.el" a) nil 'silent)
        (write-region "" nil (expand-file-name "b.txt" b) nil 'silent)
        (write-region "" nil (expand-file-name "skip.el" skip) nil 'silent)
        (make-symbolic-link a alink)
        (let ((rel (lambda (files)
                     (mapcar (lambda (file) (file-relative-name file dir))
                             files))))
          (list
           (funcall rel (directory-files-recursively dir "\\.el\\'"))
           (funcall rel (directory-files-recursively dir "" t))
           (funcall rel
                    (directory-files-recursively
                     dir "\\.el\\'" nil
                     (lambda (subdir)
                       (not (string-equal (file-name-nondirectory subdir)
                                          "skip")))))
           (funcall rel
                    (directory-files-recursively dir "\\.el\\'" nil nil t))
           (condition-case err
               (directory-files-recursively 42 "")
             (error (list (car err) (cdr err))))
           (condition-case err
               (directory-files-recursively dir 42)
             (error (list (car err) (cdr err))))
           (condition-case err
               (directory-files-recursively)
             (error (list (car err) (cdr err)))))))
    (ignore-errors (delete-file alink))
    (ignore-errors (delete-file (expand-file-name "skip.el" skip)))
    (ignore-errors (delete-file (expand-file-name "b.txt" b)))
    (ignore-errors (delete-file (expand-file-name "a.el" a)))
    (ignore-errors (delete-file (expand-file-name "root.el" dir)))
    (ignore-errors (delete-directory skip))
    (ignore-errors (delete-directory b))
    (ignore-errors (delete-directory a))
    (ignore-errors (delete-directory dir))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((\"a/a.el\" \"skip/skip.el\" \"root.el\") (\"a/a.el\" \"a\" \"alink\" \"b/b.txt\" \"b\" \"skip/skip.el\" \"skip\" \"root.el\") (\"a/a.el\" \"root.el\") (\"a/a.el\" \"alink/a.el\" \"skip/skip.el\" \"root.el\") (wrong-type-argument (stringp 42)) (wrong-type-argument (stringp 42)) (wrong-number-of-arguments ((2 . 5) 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
