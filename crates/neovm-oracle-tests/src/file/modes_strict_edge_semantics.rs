//! Oracle parity tests for GNU file mode edge semantics.
//!
//! GNU implements `file-modes` and `set-file-modes` in `src/fileio.c`.
//! Their optional FLAG handling goes through `symlink_nofollow_flag`, where
//! any non-nil flag means nofollow; it is not restricted to the symbol
//! `nofollow`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_file_modes_non_nil_flag_means_nofollow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dir (make-temp-file "neomacs-oracle-file-modes-" t))
       (target (expand-file-name "target" dir))
       (link (expand-file-name "link" dir)))
  (unwind-protect
      (progn
        (write-region "x" nil target nil 'silent)
        (set-file-modes target #o600)
        (make-symbolic-link target link)
        (list
         (logand (file-modes target) #o7777)
         (file-modes (expand-file-name "missing" dir))
         ;; GNU fileio.c:symlink_nofollow_flag treats any non-nil flag
         ;; as nofollow, so t and an arbitrary symbol must match 'nofollow.
         (= (file-modes link t)
            (file-modes link 'nofollow))
         (= (file-modes link 'anything-non-nil)
            (file-modes link 'nofollow))
         ;; The ordinary follow path still sees the target mode.
         (logand (file-modes link nil) #o7777)
         (condition-case err
             (file-modes)
           (error (list (car err) (cdr err))))
         (condition-case err
             (file-modes target 'nofollow 'extra)
           (error (list (car err) (cdr err))))))
    (ignore-errors (delete-file link))
    (ignore-errors (delete-file target))
    (ignore-errors (delete-directory dir))))
"#;

    let expect = expect_test::expect![[
        r#""OK (384 nil t t 384 (wrong-number-of-arguments (file-modes 0)) (wrong-number-of-arguments (file-modes 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_file_modes_handler_expansion_and_argument_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (setq neomacs--oracle-file-modes-calls nil)
  (defun neomacs--oracle-file-modes-handler (operation &rest args)
    (push (cons operation args) neomacs--oracle-file-modes-calls)
    (cond
     ((eq operation 'file-modes) #o641)
     ((eq operation 'set-file-modes) 'set-mode-handler-result)
     (t (let ((file-name-handler-alist nil))
          (apply operation args)))))
  (unwind-protect
      (let ((file-name-handler-alist
             '(("\\`/oracle-mode-root/" . neomacs--oracle-file-modes-handler)))
            (default-directory "/oracle-mode-root/subdir/"))
        (list
         ;; GNU expands relative file names before handler dispatch and
         ;; passes the optional flag as nil when omitted.
         (file-modes "../child")
         neomacs--oracle-file-modes-calls
         (setq neomacs--oracle-file-modes-calls nil)
         (file-modes "../child/" 'nofollow)
         neomacs--oracle-file-modes-calls
         (setq neomacs--oracle-file-modes-calls nil)
         ;; `set-file-modes' checks MODE before filename expansion/handler
         ;; lookup, then calls handlers with expanded filename, mode, flag.
         (set-file-modes "../child" #o600)
         neomacs--oracle-file-modes-calls
         (setq neomacs--oracle-file-modes-calls nil)
         (set-file-modes "../child" #o644 'nofollow)
         neomacs--oracle-file-modes-calls
         (condition-case err
             (set-file-modes "../child" 'bad-mode)
           (error (list (car err) (cdr err))))
         neomacs--oracle-file-modes-calls
         (setq neomacs--oracle-file-modes-calls nil)
         (condition-case err
             (set-file-modes 99 'bad-mode)
           (error (list (car err) (cdr err))))
         neomacs--oracle-file-modes-calls))
    (fmakunbound 'neomacs--oracle-file-modes-handler)
    (makunbound 'neomacs--oracle-file-modes-calls)))
"#;

    let expect = expect_test::expect![[
        r#""OK (417 ((file-modes \"/oracle-mode-root/child\" nil) (expand-file-name \"../child\" \"/oracle-mode-root/subdir/\")) nil 417 ((file-modes \"/oracle-mode-root/child\" nofollow) (directory-file-name \"/oracle-mode-root/child/\") (expand-file-name \"../child/\" \"/oracle-mode-root/subdir/\")) nil set-mode-handler-result ((set-file-modes \"/oracle-mode-root/child\" 384 nil) (expand-file-name \"../child\" \"/oracle-mode-root/subdir/\")) nil set-mode-handler-result ((set-file-modes \"/oracle-mode-root/child\" 420 nofollow) (expand-file-name \"../child\" \"/oracle-mode-root/subdir/\")) (wrong-type-argument (fixnump bad-mode)) ((set-file-modes \"/oracle-mode-root/child\" 420 nofollow) (expand-file-name \"../child\" \"/oracle-mode-root/subdir/\")) nil (wrong-type-argument (fixnump bad-mode)) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
