//! Oracle parity tests for GNU `file-acl` no-native-ACL build semantics.
//!
//! GNU `src/fileio.c:Ffile_acl` wraps filename expansion and file-name-handler
//! dispatch in `#if USE_ACL`.  When GNU is built without native ACL support,
//! `file-acl` only enforces arity and otherwise returns nil for any argument.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_file_acl_no_acl_build_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((file-name-handler-alist
       (list (cons "^/tmp/neomacs-oracle-acl-handler"
                   (lambda (op &rest args)
                     (list op args))))))
  (list
   (file-acl 42)
   (file-acl nil)
   (file-acl '(not-a-file-name))
   (file-acl "/tmp/neomacs-oracle-acl-handler-file")
   (condition-case err
       (file-acl)
     (error (list (car err) (cdr err))))
   (condition-case err
       (file-acl "one" "two")
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil nil nil nil (wrong-number-of-arguments (file-acl 0)) (wrong-number-of-arguments (file-acl 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
