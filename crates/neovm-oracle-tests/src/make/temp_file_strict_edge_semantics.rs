//! Oracle parity tests for GNU temporary file helpers.
//!
//! GNU implements `make-temp-file` and `make-nearby-temp-file` in
//! `lisp/files.el`, with name creation delegated to `make-temp-file-internal`
//! and `make-temp-name` in `src/fileio.c`.  The random suffix is intentionally
//! not compared directly; these tests compare deterministic properties of the
//! created paths and contents.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_make_temp_file_prefix_suffix_text_directory_and_error_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((root (make-temp-file "neomacs-oracle-temp-root-" t))
       (temporary-file-directory (file-name-as-directory root))
       (near-dir (expand-file-name "near" root)))
  (unwind-protect
      (progn
        (make-directory near-dir)
        (let* ((file (make-temp-file "alpha-" nil ".txt" "payload"))
               (dir (make-temp-file "beta-" t ".dir"))
               (empty-prefix (make-temp-file "" nil ".empty"))
               (dot-prefix (make-temp-file "." nil ".dot"))
               (name-only (make-temp-name (expand-file-name "name-" root)))
               (near-file (make-nearby-temp-file
                           (expand-file-name "nearby-" near-dir)
                           nil ".near")))
          (list
           (and (file-exists-p file)
                (not (file-directory-p file))
                (string-prefix-p (expand-file-name "alpha-" root) file)
                (string-suffix-p ".txt" file)
                (with-temp-buffer
                  (insert-file-contents file)
                  (buffer-string)))
           (and (file-directory-p dir)
                (string-prefix-p (expand-file-name "beta-" root) dir)
                (string-suffix-p ".dir" dir))
           (and (file-exists-p empty-prefix)
                (string-prefix-p root empty-prefix)
                (string-suffix-p ".empty" empty-prefix))
           (and (file-exists-p dot-prefix)
                (string-prefix-p (expand-file-name "." root) dot-prefix)
                (string-suffix-p ".dot" dot-prefix))
           (and (stringp name-only)
                (not (file-exists-p name-only))
                (string-prefix-p (expand-file-name "name-" root) name-only))
           (and (file-exists-p near-file)
                (string-prefix-p (expand-file-name "nearby-" near-dir)
                                 near-file)
                (string-suffix-p ".near" near-file))
           (condition-case err
               (make-temp-file)
             (error (list (car err) (cdr err))))
           (condition-case err
               (make-temp-file 42)
             (error (list (car err) (cdr err))))
           (condition-case err
               (make-temp-file "x" nil 42)
             (error (list (car err) (cdr err))))
           (condition-case err
               (make-nearby-temp-file "x" nil nil nil)
             (error (list (car err) (cdr err)))))))
    (delete-directory root t)))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"payload\" t t t t t (wrong-number-of-arguments ((1 . 4) 0)) (wrong-type-argument (sequencep 42)) (wrong-type-argument (stringp 42)) (wrong-number-of-arguments ((1 . 3) 4)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
