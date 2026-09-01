//! Oracle parity tests for GNU repository metadata helpers.
//!
//! GNU implements these in `lisp/version.el`.  In the studied GNU source,
//! `emacs-repository-get-version` and `emacs-repository-get-branch` accept
//! optional directory arguments, while `emacs-repository-get-dirty` is absent.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_repository_metadata_optional_args_and_dirty_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((dir temporary-file-directory))
  (list
   (condition-case err
       (emacs-repository-get-version dir)
     (error (cons (car err) (cdr err))))
   (condition-case err
       (emacs-repository-get-branch dir)
     (error (cons (car err) (cdr err))))
   (condition-case err
       (emacs-repository-get-version dir t)
     (error (cons (car err) (cdr err))))
   (fboundp 'emacs-repository-get-dirty)
   (condition-case err
       (emacs-repository-get-dirty dir)
     (error (cons (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil nil nil nil (void-function emacs-repository-get-dirty))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
