//! Oracle parity tests for GNU `make-empty-file` semantics.
//!
//! GNU implements `make-empty-file` in `lisp/files.el`.  It wraps
//! `write-region` with exact existing-file and parent-directory behavior.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_make_empty_file_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((root (make-temp-file "neomacs-oracle-make-empty-file-" t))
       (file (expand-file-name "plain.txt" root))
       (nested (expand-file-name "a/b/leaf.txt" root))
       (missing-parent (expand-file-name "x/y/leaf.txt" root)))
  (unwind-protect
      (cl-labels
          ((rel (value)
             (if (and (stringp value)
                      (string-prefix-p root value))
                 (file-relative-name value root)
               value))
           (rel-tree (value)
             (cond
              ((consp value) (cons (rel-tree (car value))
                                   (rel-tree (cdr value))))
              ((vectorp value) (mapcar #'rel-tree value))
              (t (rel value)))))
        (list
         (make-empty-file file)
         (file-exists-p file)
         (nth 7 (file-attributes file))
         (condition-case err
             (make-empty-file file)
           (error (list (car err) (rel-tree (cdr err)))))
         (write-region "old" nil file nil 'silent)
         (make-empty-file file t)
         (with-temp-buffer
           (insert-file-contents file)
           (buffer-string))
         (condition-case err
             (make-empty-file missing-parent)
           (error (list (car err) (rel-tree (cdr err)))))
         (make-empty-file nested t)
         (file-exists-p nested)
         (file-directory-p (file-name-directory nested))
         (condition-case err
             (make-empty-file)
           (error (list (car err) (rel-tree (cdr err)))))
         (condition-case err
             (make-empty-file 42)
           (error (list (car err) (rel-tree (cdr err)))))))
    (ignore-errors (delete-directory root t))))
"#;

    let expect = expect_test::expect![[r#""ERR (void-function cl-labels)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
