//! Oracle parity tests for GNU `temporary-file-directory`.
//!
//! GNU implements this function in `lisp/files.el`.  Without a file-name
//! handler it returns `default-directory` when that directory matches
//! `mounted-file-systems`; otherwise it returns the variable
//! `temporary-file-directory`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_temporary_file_directory_default_and_mounted_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((root (make-temp-file "neomacs-oracle-temp-dir-fn-" t))
       (tmp (expand-file-name "tmp/" root))
       (mounted (expand-file-name "mounted/" root))
       (plain (expand-file-name "plain/" root)))
  (unwind-protect
      (progn
        (make-directory tmp)
        (make-directory mounted)
        (make-directory plain)
        (let ((temporary-file-directory tmp)
              (mounted-file-systems (concat "\\`" (regexp-quote mounted))))
          (list
           (let ((default-directory plain))
             (string= (temporary-file-directory) tmp))
           (let ((default-directory mounted))
             (string= (temporary-file-directory) mounted))
           (let ((default-directory mounted)
                 (mounted-file-systems "\\`never-match-neomacs-oracle\\'"))
             (string= (temporary-file-directory) tmp))
           (condition-case err
               (temporary-file-directory "extra")
             (error (list (car err) (cdr err))))
           (condition-case err
               (let ((default-directory 42))
                 (temporary-file-directory))
             (error (list (car err) (cdr err)))))))
    (delete-directory root t)))
"#;

    let expect = expect_test::expect![[
        r#""OK (t t t (wrong-number-of-arguments ((0 . 0) 1)) (wrong-type-argument (stringp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
