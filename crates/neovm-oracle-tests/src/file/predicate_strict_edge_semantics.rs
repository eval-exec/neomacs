//! Oracle parity tests for GNU file predicate symlink semantics.
//!
//! GNU implements these predicates in `src/fileio.c`.  The important split is
//! that `file-symlink-p` reads the link itself and returns the raw target
//! string, while existence/readability/directory/regular predicates follow
//! symlinks and therefore treat dangling links as missing targets.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_file_predicates_symlink_and_missing_targets() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dir (make-temp-file "neomacs-oracle-file-pred-" t))
       (file (expand-file-name "plain" dir))
       (subdir (expand-file-name "subdir" dir))
       (file-link (expand-file-name "file-link" dir))
       (dir-link (expand-file-name "dir-link" dir))
       (dangling-link (expand-file-name "dangling-link" dir))
       (missing (expand-file-name "missing" dir)))
  (unwind-protect
      (progn
        (write-region "x" nil file nil 'silent)
        (make-directory subdir)
        (make-symbolic-link "plain" file-link)
        (make-symbolic-link "subdir" dir-link)
        (make-symbolic-link "missing" dangling-link)
        (list
         (file-exists-p file)
         (file-readable-p file)
         (file-writable-p file)
         (file-executable-p subdir)
         (file-directory-p subdir)
         (file-regular-p file)
         (file-symlink-p file)
         (file-symlink-p file-link)
         (file-symlink-p dir-link)
         (file-symlink-p dangling-link)
         (file-exists-p file-link)
         (file-readable-p file-link)
         (file-regular-p file-link)
         (file-directory-p dir-link)
         (file-regular-p dir-link)
         ;; GNU follows the target for existence/readability/regular tests,
         ;; so a dangling symlink is still a symlink but not an existing file.
         (file-exists-p dangling-link)
         (file-readable-p dangling-link)
         (file-regular-p dangling-link)
         (file-directory-p dangling-link)
         (file-exists-p missing)
         (file-symlink-p missing)
         (file-directory-p "")
         (condition-case err
             (file-exists-p)
           (error (list (car err) (cdr err))))
         (condition-case err
             (file-symlink-p 42)
           (error (list (car err) (cdr err))))))
    (ignore-errors (delete-file file-link))
    (ignore-errors (delete-file dir-link))
    (ignore-errors (delete-file dangling-link))
    (ignore-errors (delete-file file))
    (ignore-errors (delete-directory subdir))
    (ignore-errors (delete-directory dir))))
"#;

    let expect = expect_test::expect![[
        r#""OK (t t t t t t nil \"plain\" \"subdir\" \"missing\" t t t t nil nil nil nil nil nil nil t (wrong-number-of-arguments (file-exists-p 0)) (wrong-type-argument (stringp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_file_predicates_dispatch_after_default_directory_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (setq neomacs--oracle-file-predicate-calls nil)
  (defun neomacs--oracle-file-predicate-handler (operation &rest args)
    (cond
     ((memq operation '(file-exists-p file-readable-p file-writable-p
                        file-executable-p file-accessible-directory-p
                        file-directory-p file-regular-p))
      (push (cons operation args) neomacs--oracle-file-predicate-calls)
      t)
     ((eq operation 'file-symlink-p)
      (push (cons operation args) neomacs--oracle-file-predicate-calls)
      "target")
     (t
      (let ((file-name-handler-alist nil))
        (apply operation args)))))
  (unwind-protect
      (let ((file-name-handler-alist
             '(("\\`/oracle-predicate-root/" . neomacs--oracle-file-predicate-handler)))
            (default-directory "/oracle-predicate-root/"))
        (list
         (file-exists-p "child")
         neomacs--oracle-file-predicate-calls
         (setq neomacs--oracle-file-predicate-calls nil)
         (file-readable-p "child")
         neomacs--oracle-file-predicate-calls
         (setq neomacs--oracle-file-predicate-calls nil)
         (file-writable-p "child")
         neomacs--oracle-file-predicate-calls
         (setq neomacs--oracle-file-predicate-calls nil)
         (file-executable-p "child")
         neomacs--oracle-file-predicate-calls
         (setq neomacs--oracle-file-predicate-calls nil)
         (file-accessible-directory-p "child/")
         neomacs--oracle-file-predicate-calls
         (setq neomacs--oracle-file-predicate-calls nil)
         (file-directory-p "child/")
         neomacs--oracle-file-predicate-calls
         (setq neomacs--oracle-file-predicate-calls nil)
         (file-regular-p "child/")
         neomacs--oracle-file-predicate-calls
         (setq neomacs--oracle-file-predicate-calls nil)
         (file-symlink-p "child")
         neomacs--oracle-file-predicate-calls))
    (fmakunbound 'neomacs--oracle-file-predicate-handler)
    (makunbound 'neomacs--oracle-file-predicate-calls)))
"#;

    let expect = expect_test::expect![[
        r#""OK (t ((file-exists-p \"/oracle-predicate-root/child\")) nil t ((file-readable-p \"/oracle-predicate-root/child\")) nil t ((file-writable-p \"/oracle-predicate-root/child\")) nil t ((file-executable-p \"/oracle-predicate-root/child\")) nil t ((file-accessible-directory-p \"/oracle-predicate-root/child/\")) nil t ((file-directory-p \"/oracle-predicate-root/child\")) nil t ((file-regular-p \"/oracle-predicate-root/child\")) nil \"target\" ((file-symlink-p \"/oracle-predicate-root/child\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
