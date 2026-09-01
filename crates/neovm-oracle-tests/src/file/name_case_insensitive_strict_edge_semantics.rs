//! Oracle parity tests for GNU file-name case-sensitivity probing.
//!
//! GNU implements `file-name-case-insensitive-p` in `src/fileio.c`.  Missing
//! paths are not immediate errors: GNU walks upward to an existing parent and
//! reports that filesystem's case behavior, returning nil if it cannot decide.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_file_name_case_insensitive_existing_and_missing_paths() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((root (make-temp-file "neomacs-oracle-file-name-case-" t))
       (sub (expand-file-name "SubDir" root))
       (file (expand-file-name "MixedName.txt" sub))
       (missing-child (expand-file-name "Missing/Child.txt" sub)))
  (unwind-protect
      (progn
        (make-directory sub)
        (write-region "x" nil file nil 'silent)
        (list
         ;; Existing files and directories should use the same filesystem
         ;; probe result.
         (file-name-case-insensitive-p root)
         (file-name-case-insensitive-p sub)
         (file-name-case-insensitive-p file)
         ;; Missing descendants walk upward to the existing parent.
         (file-name-case-insensitive-p missing-child)
         (equal (file-name-case-insensitive-p missing-child)
                (file-name-case-insensitive-p sub))
         ;; Relative names are expanded before probing.
         (let ((default-directory sub))
           (equal (file-name-case-insensitive-p "MixedName.txt")
                  (file-name-case-insensitive-p file)))
         (condition-case err
             (file-name-case-insensitive-p)
           (error (list (car err) (cdr err))))
         (condition-case err
             (file-name-case-insensitive-p file 'extra)
           (error (list (car err) (cdr err))))
         (condition-case err
             (file-name-case-insensitive-p 42)
           (error (list (car err) (cdr err))))))
    (ignore-errors (delete-directory root t))))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil nil nil nil t t (wrong-number-of-arguments (file-name-case-insensitive-p 0)) (wrong-number-of-arguments (file-name-case-insensitive-p 2)) (wrong-type-argument (stringp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_file_name_case_insensitive_handler_and_argument_order_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (setq neomacs--oracle-case-fold-calls nil)
  (defun neomacs--oracle-case-fold-handler (operation &rest args)
    (push (cons operation args) neomacs--oracle-case-fold-calls)
    (cond
     ((eq operation 'file-name-case-insensitive-p)
      'handled-case-fold)
     ((eq operation 'expand-file-name)
      17)
     (t
      (let ((file-name-handler-alist nil))
        (apply operation args)))))
  (unwind-protect
      (list
       ;; GNU validates arity and type before consulting file-name handlers.
       (condition-case err
           (let ((file-name-handler-alist
                  '((".*" . neomacs--oracle-case-fold-handler))))
             (file-name-case-insensitive-p))
         (error (list (car err) (cdr err))))
       neomacs--oracle-case-fold-calls
       (setq neomacs--oracle-case-fold-calls nil)
       (condition-case err
           (let ((file-name-handler-alist
                  '((".*" . neomacs--oracle-case-fold-handler))))
             (file-name-case-insensitive-p 42))
         (error (list (car err) (cdr err))))
       neomacs--oracle-case-fold-calls
       (setq neomacs--oracle-case-fold-calls nil)
       ;; Unrestricted handlers can be reached by expand-file-name first.
       (let ((file-name-handler-alist
              '(("\\`/oracle-case-fold-root/" . neomacs--oracle-case-fold-handler)))
             (default-directory "/oracle-case-fold-root/"))
         (condition-case err
             (file-name-case-insensitive-p "child")
           (error (list (car err) (cdr err)))))
       neomacs--oracle-case-fold-calls
       (setq neomacs--oracle-case-fold-calls nil)
       ;; Restricting operations skips expand-file-name and dispatches the
       ;; predicate with the expanded absolute name.
       (progn
         (put 'neomacs--oracle-case-fold-handler
              'operations
              '(file-name-case-insensitive-p))
         (let ((file-name-handler-alist
                '(("\\`/oracle-case-fold-root/" . neomacs--oracle-case-fold-handler)))
               (default-directory "/oracle-case-fold-root/"))
           (file-name-case-insensitive-p "child")))
       neomacs--oracle-case-fold-calls)
    (put 'neomacs--oracle-case-fold-handler 'operations nil)
    (fmakunbound 'neomacs--oracle-case-fold-handler)
    (makunbound 'neomacs--oracle-case-fold-calls)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-number-of-arguments (file-name-case-insensitive-p 0)) nil nil (wrong-type-argument (stringp 42)) nil nil (error (\"Invalid handler in ‘file-name-handler-alist’\")) ((expand-file-name \"child\" \"/oracle-case-fold-root/\")) nil handled-case-fold ((file-name-case-insensitive-p \"/oracle-case-fold-root/child\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
