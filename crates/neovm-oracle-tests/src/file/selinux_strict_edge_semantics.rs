//! Oracle parity tests for SELinux file operations without native SELinux.
//!
//! GNU expands file names and dispatches file-name handlers for
//! `set-file-selinux-context` before the native SELinux implementation block.
//! A no-SELinux build still returns nil for local files, but handlers must see
//! the expanded file name and original context argument.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_set_file_selinux_context_no_selinux_handler_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (defun neomacs--oracle-selinux-handler (operation &rest args)
    (if (eq operation 'expand-file-name)
        (car args)
      (list operation args)))
  (unwind-protect
      (let ((file-name-handler-alist
             '(("^/tmp/neomacs-oracle-selinux-handler"
                . neomacs--oracle-selinux-handler))))
        (list
         (set-file-selinux-context "/tmp/neomacs-oracle-selinux-handler-file"
                                   '(user role type range))
         (set-file-selinux-context "/tmp/neomacs-oracle-local-selinux-file"
                                   42)
         (condition-case err
             (set-file-selinux-context)
           (error (list (car err) (cdr err))))
         (condition-case err
             (set-file-selinux-context 42 nil)
           (error (list (car err) (cdr err))))))
    (fmakunbound 'neomacs--oracle-selinux-handler)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((set-file-selinux-context (\"/tmp/neomacs-oracle-selinux-handler-file\" (user role type range))) nil (wrong-number-of-arguments (set-file-selinux-context 0)) (wrong-type-argument (stringp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
