//! Oracle parity tests for GNU public directory wrapper semantics.
//!
//! GNU implements `make-directory` and `delete-directory` in `lisp/files.el`
//! over the internal primitives.  The wrappers add recursive parent creation,
//! existing-directory return values, recursive deletion, and trash delegation.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_directory_wrapper_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((root (make-temp-file "neomacs-oracle-dir-wrapper-" t))
       (a (expand-file-name "a" root))
       (b (expand-file-name "b/c" root))
       (nonempty (expand-file-name "nonempty" root))
       (leaf (expand-file-name "leaf.txt" nonempty))
       (trashdir (expand-file-name "trashme" root)))
  (unwind-protect
      (progn
        (make-directory nonempty)
        (write-region "x" nil leaf nil 'silent)
        (make-directory trashdir)
        (list
         (make-directory a)
         (file-directory-p a)
         (make-directory a t)
         (condition-case err
             (make-directory a)
           (error (list (car err) (cdr err))))
         (make-directory b t)
         (file-directory-p b)
         (condition-case err
             (make-directory (expand-file-name "missing/child" root))
           (error (list (car err) (cdr err))))

         (condition-case err
             (delete-directory nonempty)
           (error (list (car err) (cdr err))))
         (file-directory-p nonempty)
         (delete-directory nonempty t)
         (file-exists-p nonempty)

         (let ((delete-by-moving-to-trash t)
               (calls nil))
           (require 'cl-lib)
           (make-directory trashdir t)
           (write-region "y" nil (expand-file-name "x" trashdir) nil 'silent)
           (cl-letf (((symbol-function 'move-file-to-trash)
                      (lambda (name)
                        (push (file-relative-name name root) calls)
                        'moved)))
             (list
              (condition-case err
                  (delete-directory trashdir nil t)
                (error (list (car err) (cdr err))))
              (file-directory-p trashdir)
              (delete-directory trashdir t t)
              calls
              (file-directory-p trashdir))))

         (condition-case err
             (make-directory)
           (error (list (car err) (cdr err))))
         (condition-case err
             (delete-directory 42)
           (error (list (car err) (cdr err)))))))
    (ignore-errors (delete-directory root t))))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 57 47)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
