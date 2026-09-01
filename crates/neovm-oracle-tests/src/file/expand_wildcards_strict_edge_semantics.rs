//! Oracle parity tests for GNU `file-expand-wildcards` semantics.
//!
//! GNU implements this in `lisp/files.el`.  It expands each path component
//! separately, filters `.' and `..', special-cases trailing slash patterns, and
//! returns relative names unless the input pattern is absolute or FULL is
//! non-nil.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_file_expand_wildcards_nested_full_regexp_and_error_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dir (make-temp-file "neomacs-oracle-wildcards-" t))
       (default-directory (file-name-as-directory dir))
       (aa (expand-file-name "aa" dir))
       (bb (expand-file-name "bb" dir))
       (space-dir (expand-file-name "space dir" dir)))
  (unwind-protect
      (progn
        (make-directory aa)
        (make-directory bb)
        (make-directory space-dir)
        (write-region "" nil (expand-file-name "alpha.el" dir) nil 'silent)
        (write-region "" nil (expand-file-name "beta.txt" dir) nil 'silent)
        (write-region "" nil (expand-file-name ".hidden.el" dir) nil 'silent)
        (write-region "" nil (expand-file-name "zeta.el" aa) nil 'silent)
        (write-region "" nil (expand-file-name "eta.el" bb) nil 'silent)
        (write-region "" nil (expand-file-name "two words.el" space-dir)
                      nil 'silent)
        (list
         ;; Relative wildcard output is sorted and remains relative.
         (file-expand-wildcards "*.el")
         (file-expand-wildcards "*/")
         (file-expand-wildcards "*/*.el")
         ;; FULL returns absolute names; normalize them back to DIR to keep the
         ;; oracle independent of temporary directory names.
         (mapcar (lambda (name) (file-relative-name name dir))
                 (file-expand-wildcards "*.el" t))
         (mapcar (lambda (name) (file-relative-name name dir))
                 (file-expand-wildcards "*/*.el" t))
         ;; Absolute input patterns return absolute names even without FULL.
         (mapcar (lambda (name) (file-relative-name name dir))
                 (file-expand-wildcards (expand-file-name "*/*.el" dir)))
         ;; REGEXP mode matches one path component at a time.
         (file-expand-wildcards ".*\\.el" nil t)
         (file-expand-wildcards ".hidden\\.el" nil t)
         ;; Dot and dot-dot directory entries are filtered.
         (file-expand-wildcards "aa/.")
         (file-expand-wildcards "aa/..")
         ;; Missing matches return nil.
         (file-expand-wildcards "missing*")
         (condition-case err
             (file-expand-wildcards 42)
           (error (list (car err) (cdr err))))
         (condition-case err
             (file-expand-wildcards "*" nil nil nil)
           (error (list (car err) (cdr err)))))))
    (ignore-errors (delete-file (expand-file-name "two words.el" space-dir)))
    (ignore-errors (delete-file (expand-file-name "eta.el" bb)))
    (ignore-errors (delete-file (expand-file-name "zeta.el" aa)))
    (ignore-errors (delete-file (expand-file-name ".hidden.el" dir)))
    (ignore-errors (delete-file (expand-file-name "beta.txt" dir)))
    (ignore-errors (delete-file (expand-file-name "alpha.el" dir)))
    (ignore-errors (delete-directory space-dir))
    (ignore-errors (delete-directory bb))
    (ignore-errors (delete-directory aa))
    (ignore-errors (delete-directory dir))))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 56 44)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
