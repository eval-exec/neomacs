//! Oracle parity tests for GNU `locate-dominating-file`.
//!
//! GNU implements this helper in `lisp/files.el`.  It starts from either a
//! file or directory, expands and abbreviates that name, walks parent
//! directories, returns a directory name with trailing slash, and accepts either
//! a witness filename or a predicate function.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_locate_dominating_file_parent_predicate_and_error_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((root (make-temp-file "neomacs-oracle-locate-dominating-" t))
       (a (expand-file-name "a" root))
       (b (expand-file-name "b" a))
       (leaf (expand-file-name "leaf.txt" b))
       (stop (expand-file-name "stop" root)))
  (unwind-protect
      (progn
        (make-directory a)
        (make-directory b)
        (make-directory stop)
        (write-region "witness" nil (expand-file-name ".project" a) nil 'silent)
        (write-region "leaf" nil leaf nil 'silent)
        (let ((rel (lambda (file)
                     (and file (file-relative-name file root)))))
          (list
           ;; Starting from a file searches its containing directory first.
           (funcall rel (locate-dominating-file leaf ".project"))
           ;; Starting from an existing directory without a slash still treats
           ;; that path as the starting directory.
           (funcall rel (locate-dominating-file b ".project"))
           (locate-dominating-file b "missing-neomacs-oracle-witness")
           ;; Predicate NAME is called with each candidate directory.
           (funcall rel
                    (locate-dominating-file
                     leaf
                     (lambda (dir)
                       (string= (file-name-nondirectory
                                 (directory-file-name dir))
                                "b"))))
           ;; A custom stop regexp prevents walking past STOP.
           (let ((locate-dominating-stop-dir-regexp
                  (concat "\\`" (regexp-quote (file-name-as-directory stop)) "\\'")))
             (locate-dominating-file (expand-file-name "child.txt" stop)
                                     ".project"))
           (condition-case err
               (locate-dominating-file)
             (error (list (car err) (cdr err))))
           (condition-case err
               (locate-dominating-file 42 ".project")
             (error (list (car err) (cdr err))))
           (condition-case err
               (locate-dominating-file leaf 42)
             (error (list (car err) (cdr err)))))))
    (delete-directory root t)))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"a/\" \"a/\" nil \"a/b/\" nil (wrong-number-of-arguments ((2 . 2) 0)) (wrong-type-argument (stringp 42)) (invalid-function (42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
