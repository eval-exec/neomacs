//! Oracle parity tests for GNU `directory-empty-p` semantics.
//!
//! GNU implements this in `lisp/files.el` by composing `file-directory-p` with
//! `directory-files` using `directory-files-no-dot-files-regexp` and COUNT=1.
//! Symlinks to directories count as directories.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_directory_empty_p_regular_missing_symlink_and_type_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dir (make-temp-file "neomacs-oracle-directory-empty-" t))
       (empty (expand-file-name "empty" dir))
       (nonempty (expand-file-name "nonempty" dir))
       (file (expand-file-name "file.txt" dir))
       (link (expand-file-name "empty-link" dir))
       (missing (expand-file-name "missing" dir)))
  (unwind-protect
      (progn
        (make-directory empty)
        (make-directory nonempty)
        (write-region "child" nil (expand-file-name "child.txt" nonempty)
                      nil 'silent)
        (write-region "file" nil file nil 'silent)
        (make-symbolic-link empty link)
        (list
         (directory-empty-p empty)
         (directory-empty-p nonempty)
         (directory-empty-p file)
         (directory-empty-p missing)
         (directory-empty-p link)
         (condition-case err
             (directory-empty-p 42)
           (error (list (car err) (cdr err))))
         (condition-case err
             (directory-empty-p)
           (error (list (car err) (cdr err))))))
    (ignore-errors (delete-file link))
    (ignore-errors (delete-file file))
    (ignore-errors (delete-file (expand-file-name "child.txt" nonempty)))
    (ignore-errors (delete-directory nonempty))
    (ignore-errors (delete-directory empty))
    (ignore-errors (delete-directory dir))))
"#;

    let expect = expect_test::expect![[
        r#""OK (t nil nil nil t (wrong-type-argument (stringp 42)) (wrong-number-of-arguments ((1 . 1) 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
