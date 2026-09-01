//! Oracle parity tests for GNU directory/file name conversion helpers.
//!
//! GNU implements `file-name-as-directory`, `directory-file-name`, and
//! `directory-name-p` in `src/fileio.c`.  These are syntactic operations with
//! specific Unix root and empty-string behavior.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_directory_name_transform_root_and_empty_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (file-name-as-directory "")
 (file-name-as-directory ".")
 (file-name-as-directory "a")
 (file-name-as-directory "a/")
 (file-name-as-directory "/")
 (file-name-as-directory "//")
 (file-name-as-directory "///")
 (directory-file-name "")
 (directory-file-name ".")
 (directory-file-name "a")
 (directory-file-name "a/")
 (directory-file-name "a///")
 (directory-file-name "/")
 (directory-file-name "//")
 (directory-file-name "///")
 (directory-name-p "")
 (directory-name-p ".")
 (directory-name-p "a")
 (directory-name-p "a/")
 (directory-name-p "/")
 (directory-name-p "//")
 (condition-case err
     (file-name-as-directory)
   (error (list (car err) (cdr err))))
 (condition-case err
     (directory-file-name 42)
   (error (list (car err) (cdr err))))
 (condition-case err
     (directory-name-p 42)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"./\" \"./\" \"a/\" \"a/\" \"/\" \"//\" \"///\" \"\" \".\" \"a\" \"a\" \"a\" \"/\" \"//\" \"/\" nil nil nil t t t (wrong-number-of-arguments (file-name-as-directory 0)) (wrong-type-argument (stringp 42)) (wrong-type-argument (stringp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_directory_name_transform_handler_result_contract_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (defun neomacs--oracle-directory-name-bad-handler (operation &rest args)
    42)
  (unwind-protect
      (let ((file-name-handler-alist
             '(("\\`/bad-dir-transform:" . neomacs--oracle-directory-name-bad-handler))))
        (list
         ;; GNU validates handler results per operation in src/fileio.c.
         (condition-case err
             (file-name-as-directory "/bad-dir-transform:path")
           (error (list (car err) (cdr err))))
         (condition-case err
             (directory-file-name "/bad-dir-transform:path/")
           (error (list (car err) (cdr err))))
         ;; `unhandled-file-name-directory' is permissive and maps a
         ;; non-string handler result to nil.
         (unhandled-file-name-directory "/bad-dir-transform:path")))
    (fmakunbound 'neomacs--oracle-directory-name-bad-handler)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((error (\"Invalid handler in ‘file-name-handler-alist’\")) (error (\"Invalid handler in ‘file-name-handler-alist’\")) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_unhandled_file_name_directory_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (unhandled-file-name-directory "")
 (unhandled-file-name-directory ".")
 (unhandled-file-name-directory "plain")
 (unhandled-file-name-directory "plain/")
 (unhandled-file-name-directory "/")
 (unhandled-file-name-directory "//")
 (unhandled-file-name-directory "///")
 (unhandled-file-name-directory "/tmp/file")
 (let ((file-name-handler-alist nil))
   (unhandled-file-name-directory "/tmp/no-handler"))
 (condition-case err
     (unhandled-file-name-directory)
   (error (list (car err) (cdr err))))
 (condition-case err
     (unhandled-file-name-directory 42)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"./\" \"./\" \"plain/\" \"plain/\" \"/\" \"//\" \"///\" \"/tmp/file/\" \"/tmp/no-handler/\" (wrong-number-of-arguments (unhandled-file-name-directory 0)) (wrong-type-argument (stringp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
