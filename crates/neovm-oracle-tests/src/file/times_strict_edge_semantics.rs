//! Oracle parity tests for GNU `set-file-times` handler and argument order.
//!
//! GNU `src/fileio.c:Fset_file_times` converts the optional timestamp before
//! expanding the file name or dispatching a file-name handler.  Handler calls
//! always receive the expanded name plus both optional arguments, with omitted
//! optionals passed as nil.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_set_file_times_handler_and_timestamp_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (defun neomacs--oracle-times-handler (operation &rest args)
    (if (eq operation 'expand-file-name)
        (car args)
      (list operation args)))
  (unwind-protect
      (let ((file-name-handler-alist
             '(("^/tmp/neomacs-oracle-times-handler"
                . neomacs--oracle-times-handler))))
        (list
         (set-file-times "/tmp/neomacs-oracle-times-handler-file")
         (set-file-times "/tmp/neomacs-oracle-times-handler-file" 42)
         (set-file-times "/tmp/neomacs-oracle-times-handler-file" 42 'nofollow)
         (condition-case err
             (set-file-times "/tmp/neomacs-oracle-times-handler-file" 'bad-time)
           (error (list (car err) (cdr err))))
         (condition-case err
             (set-file-times 42 'bad-time)
           (error (list (car err) (cdr err))))
         (condition-case err
             (set-file-times)
           (error (list (car err) (cdr err))))
         (condition-case err
             (set-file-times "a" nil nil nil)
           (error (list (car err) (cdr err)))))))
    (fmakunbound 'neomacs--oracle-times-handler)))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 27 50)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
