//! Oracle parity tests for GNU `file-attributes` edge semantics.
//!
//! GNU implements `file-attributes` in `src/dired.c`.  It condition-catches
//! `expand-file-name`, returns nil if expansion fails or the file does not
//! exist, follows GNU's ID-FORMAT rule for uid/gid representation, and
//! `file-attributes-lessp` compares the car strings of directory entries.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_file_attributes_shape_id_format_missing_bad_filename_and_lessp_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((dir (make-temp-file "neomacs-oracle-fileattrs-" t)))
  (unwind-protect
      (progn
        (make-directory (expand-file-name "sub" dir))
        (with-temp-file (expand-file-name "alpha.txt" dir)
          (insert "alpha"))
        (let* ((file (expand-file-name "alpha.txt" dir))
               (sub (expand-file-name "sub" dir))
               (missing (expand-file-name "missing" dir))
               (int-attrs (file-attributes file 'integer))
               (string-attrs (file-attributes file 'string))
               (dir-attrs (file-attributes sub 'integer)))
          (list
           (list
            (nth 0 int-attrs)
            (integerp (nth 1 int-attrs))
            (integerp (nth 2 int-attrs))
            (integerp (nth 3 int-attrs))
            (consp (nth 4 int-attrs))
            (consp (nth 5 int-attrs))
            (consp (nth 6 int-attrs))
            (nth 7 int-attrs)
            (substring (nth 8 int-attrs) 0 1)
            (nth 9 int-attrs)
            (integerp (nth 10 int-attrs))
            (integerp (nth 11 int-attrs))
            (length int-attrs))
           (list
            (or (stringp (nth 2 string-attrs)) (integerp (nth 2 string-attrs)))
            (or (stringp (nth 3 string-attrs)) (integerp (nth 3 string-attrs))))
           (list (nth 0 dir-attrs)
                 (substring (nth 8 dir-attrs) 0 1)
                 (length dir-attrs))
           (file-attributes missing 'integer)
           (file-attributes nil)
           (condition-case err
               (file-attributes 42)
             (error (list (car err) (cdr err))))
           (condition-case err
               (file-attributes (string ?a 0 ?b))
             (error (list (car err) (cdr err))))
           (file-attributes-lessp (cons "alpha" int-attrs)
                                  (cons "beta" int-attrs))
           (file-attributes-lessp (cons "beta" int-attrs)
                                  (cons "alpha" int-attrs))
           (condition-case err
               (file-attributes-lessp "alpha" "beta")
             (error (list (car err) (cdr err)))))))
    (delete-directory dir t)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((nil t t t t t t 5 \"-\" t t t 12) (t t) (t \"d\" 12) nil nil nil nil t nil (wrong-type-argument (listp \"beta\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_file_attributes_symlink_type_and_dangling_link_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dir (make-temp-file "neomacs-oracle-fileattrs-link-" t))
       (file (expand-file-name "target.txt" dir))
       (subdir (expand-file-name "target-dir" dir))
       (file-link (expand-file-name "file-link" dir))
       (dir-link (expand-file-name "dir-link" dir))
       (dangling-link (expand-file-name "dangling-link" dir)))
  (unwind-protect
      (progn
        (write-region "target" nil file nil 'silent)
        (make-directory subdir)
        (make-symbolic-link "target.txt" file-link)
        (make-symbolic-link "target-dir" dir-link)
        (make-symbolic-link "missing-target" dangling-link)
        (let ((file-attrs (file-attributes file-link 'integer))
              (dir-attrs (file-attributes dir-link 'integer))
              (dangling-attrs (file-attributes dangling-link 'integer))
              (target-attrs (file-attributes file 'integer)))
          (list
           ;; GNU `file-attributes' uses lstat/no-follow semantics for the
           ;; type field: symlinks return the raw link target string, including
           ;; dangling links whose targets do not exist.
           (nth 0 file-attrs)
           (nth 0 dir-attrs)
           (nth 0 dangling-attrs)
           (file-exists-p dangling-link)
           (file-symlink-p dangling-link)
           ;; Symlink attributes describe the link object, not the target.
           (stringp (nth 0 file-attrs))
           (stringp (nth 8 file-attrs))
           (substring (nth 8 file-attrs) 0 1)
           (equal (nth 10 file-attrs) (nth 10 target-attrs))
           (length file-attrs)
           (length dangling-attrs))))
    (ignore-errors (delete-file file-link))
    (ignore-errors (delete-file dir-link))
    (ignore-errors (delete-file dangling-link))
    (ignore-errors (delete-file file))
    (ignore-errors (delete-directory subdir))
    (ignore-errors (delete-directory dir))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"target.txt\" \"target-dir\" \"missing-target\" nil \"missing-target\" t t \"l\" nil 12 12)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_file_attributes_handler_and_expand_error_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (setq neomacs--oracle-fileattrs-calls nil)
  (defun neomacs--oracle-fileattrs-handler (operation &rest args)
    (push (cons operation args) neomacs--oracle-fileattrs-calls)
    (cond
     ((eq operation 'file-attributes)
      (list t 1 2 3 nil nil nil 0 "drwx------" nil 1 1))
     ((eq operation 'expand-file-name)
      17)
     (t
      (let ((file-name-handler-alist nil))
        (apply operation args)))))
  (unwind-protect
      (list
       ;; GNU catches expand-file-name errors in file-attributes and returns nil.
       (let ((file-name-handler-alist
              '(("\\`/oracle-fileattrs-root/" . neomacs--oracle-fileattrs-handler)))
             (default-directory "/oracle-fileattrs-root/"))
         (list
          (file-attributes "child")
          neomacs--oracle-fileattrs-calls))
       (setq neomacs--oracle-fileattrs-calls nil)
       ;; Restricting operations makes expand-file-name skip the handler; GNU
       ;; then dispatches file-attributes with the expanded absolute name.
       (progn
         (put 'neomacs--oracle-fileattrs-handler 'operations '(file-attributes))
         (let ((file-name-handler-alist
                '(("\\`/oracle-fileattrs-root/" . neomacs--oracle-fileattrs-handler)))
               (default-directory "/oracle-fileattrs-root/"))
           (list
            (file-attributes "child")
            neomacs--oracle-fileattrs-calls)))
       (setq neomacs--oracle-fileattrs-calls nil)
       ;; Non-nil ID-FORMAT is forwarded to modern handlers.
       (let ((file-name-handler-alist
              '(("\\`/oracle-fileattrs-root/" . neomacs--oracle-fileattrs-handler)))
             (default-directory "/oracle-fileattrs-root/"))
         (list
          (file-attributes "child" 'string)
          neomacs--oracle-fileattrs-calls)))
    (put 'neomacs--oracle-fileattrs-handler 'operations nil)
    (fmakunbound 'neomacs--oracle-fileattrs-handler)
    (makunbound 'neomacs--oracle-fileattrs-calls)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((nil ((expand-file-name \"child\" \"/oracle-fileattrs-root/\"))) nil ((t 1 2 3 nil nil nil 0 \"drwx------\" nil 1 1) ((file-attributes \"/oracle-fileattrs-root/child\"))) nil ((t 1 2 3 nil nil nil 0 \"drwx------\" nil 1 1) ((file-attributes \"/oracle-fileattrs-root/child\" string))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
