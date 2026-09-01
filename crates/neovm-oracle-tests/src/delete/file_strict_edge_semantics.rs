//! Oracle parity tests for GNU `delete-file` wrapper semantics.
//!
//! GNU implements the public wrapper in `lisp/files.el`; it expands names,
//! refuses directories before calling the primitive, removes symlinks without
//! touching their targets, treats a missing file as nil, and delegates to
//! `move-file-to-trash` only when both trash flags are active.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_delete_file_wrapper_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dir (make-temp-file "neomacs-oracle-delete-file-" t))
       (default-directory (file-name-as-directory dir))
       (file (expand-file-name "plain.txt" dir))
       (link (expand-file-name "link.txt" dir))
       (subdir (expand-file-name "subdir" dir))
       (missing (expand-file-name "missing.txt" dir)))
  (unwind-protect
      (progn
        (write-region "plain" nil file nil 'silent)
        (make-symbolic-link "plain.txt" link)
        (make-directory subdir)
        (list
         (delete-file file)
         (file-exists-p file)
         (file-exists-p link)
         (file-symlink-p link)
         (delete-file link)
         (file-exists-p link)
         (file-exists-p (expand-file-name "plain.txt" dir))
         (condition-case err
             (delete-file subdir)
           (error (list (car err) (cdr err))))
         (delete-file missing)
         (condition-case err
             (delete-file 42)
           (error (list (car err) (cdr err))))
         (let ((delete-by-moving-to-trash nil))
           (write-region "trash-off" nil file nil 'silent)
           (list (delete-file file t)
                 (file-exists-p file)))
         (let ((delete-by-moving-to-trash t)
               (calls nil))
           (require 'cl-lib)
           (write-region "trash-on" nil file nil 'silent)
           (cl-letf (((symbol-function 'move-file-to-trash)
                      (lambda (name)
                        (push (file-relative-name name dir) calls)
                        'moved-to-trash)))
             (list (delete-file "plain.txt" t)
                   calls
                   (file-exists-p file)))))))
    (ignore-errors (delete-file link))
    (ignore-errors (delete-file file))
    (ignore-errors (delete-directory subdir))
    (ignore-errors (delete-directory dir t))))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 46 46)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
