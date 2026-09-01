//! Oracle parity tests for GNU file access and ownership helper semantics.
//!
//! GNU implements `access-file` and `file-accessible-directory-p` in
//! `src/fileio.c`, while `file-ownership-preserved-p` is Lisp in
//! `lisp/files.el` layered on `file-attributes`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_file_accessible_access_file_and_ownership_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((dir (make-temp-file "neomacs-oracle-file-access-" t)))
  (unwind-protect
      (progn
        (make-directory (expand-file-name "sub" dir))
        (with-temp-file (expand-file-name "alpha.txt" dir)
          (insert "alpha"))
        (let ((file (expand-file-name "alpha.txt" dir))
              (sub (expand-file-name "sub" dir))
              (missing (expand-file-name "missing" dir)))
          (list
           (access-file file "Reading alpha")
           (file-accessible-directory-p dir)
           (file-accessible-directory-p (file-name-as-directory dir))
           (file-accessible-directory-p sub)
           (file-accessible-directory-p file)
           (file-accessible-directory-p missing)
           (file-accessible-directory-p "")
           (file-ownership-preserved-p file)
           (file-ownership-preserved-p file t)
           (file-ownership-preserved-p missing)
           (condition-case err
               (access-file missing "Reading missing")
             (error (list (car err) (cadr err))))
           (condition-case err
               (access-file file 42)
             (error (list (car err) (cdr err))))
           (condition-case err
               (file-accessible-directory-p 42)
             (error (list (car err) (cdr err))))
           (condition-case err
               (file-ownership-preserved-p 42)
             (error (list (car err) (cdr err)))))))
    (delete-directory dir t)))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil t t t nil nil t t t t (file-missing \"Reading missing\") (wrong-type-argument (stringp 42)) (wrong-type-argument (stringp 42)) (wrong-type-argument (stringp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_access_file_handler_and_argument_order_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (setq neomacs--oracle-access-file-calls nil)
  (defun neomacs--oracle-access-file-handler (operation &rest args)
    (push (cons operation args) neomacs--oracle-access-file-calls)
    'handler-result)
  (unwind-protect
      (let ((file-name-handler-alist
             '(("\\`/oracle-access-root/" . neomacs--oracle-access-file-handler)))
            (default-directory "/oracle-access-root/"))
        (list
         ;; GNU checks FILENAME and calls `expand-file-name' before checking
         ;; STRING.  Without an `operations' property, this handler is eligible
         ;; for `expand-file-name'; a non-string handler result is rejected
         ;; before `access-file' reaches its own handler dispatch.
         (condition-case err
             (access-file "child" "Reading child")
           (error (list (car err) (cdr err))))
         neomacs--oracle-access-file-calls
         (setq neomacs--oracle-access-file-calls nil)
         (put 'neomacs--oracle-access-file-handler 'operations '(access-file))
         ;; With `operations' restricted to `access-file', expansion skips the
         ;; handler and `access-file' dispatches the expanded filename.  The
         ;; handler's return value is returned directly.
         (access-file "child" "Reading child")
         neomacs--oracle-access-file-calls
         (setq neomacs--oracle-access-file-calls nil)
         (condition-case err
             (access-file "child" 42)
           (error (list (car err) (cdr err))))
         neomacs--oracle-access-file-calls
         (setq neomacs--oracle-access-file-calls nil)
         (condition-case err
             (access-file 42 "Reading child")
           (error (list (car err) (cdr err))))
         neomacs--oracle-access-file-calls))
    (fmakunbound 'neomacs--oracle-access-file-handler)
    (put 'neomacs--oracle-access-file-handler 'operations nil)
    (makunbound 'neomacs--oracle-access-file-calls)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((error (\"Invalid handler in ‘file-name-handler-alist’\")) ((expand-file-name \"child\" \"/oracle-access-root/\")) nil (access-file) handler-result ((access-file \"/oracle-access-root/child\" \"Reading child\")) nil (wrong-type-argument (stringp 42)) nil nil (wrong-type-argument (stringp 42)) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
