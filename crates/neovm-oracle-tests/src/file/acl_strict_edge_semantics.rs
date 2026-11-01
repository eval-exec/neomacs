//! Oracle parity tests for GNU `file-acl` no-native-ACL build semantics.
//!
//! GNU `src/fileio.c:Ffile_acl` wraps filename expansion and file-name-handler
//! dispatch in `#if USE_ACL`.  When GNU is built without native ACL support,
//! `file-acl` only enforces arity and otherwise returns nil for any argument.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

crate::common::platform_twin_test!(TwinHost::Linux, oracle_file_acl_no_acl_build_edges, {
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
});

// The macos-batch reference is built with USE_ACL=1 (Darwin ACLs): file-acl
// expands its argument and asks the kernel.  An existing file with no
// extended ACL answers nil; a non-string argument and a wrong arity are the
// errors below.  The same probe answers identically under Neo on the mac
// (measured live).  The Linux no-ACL test keeps its expectation under
// cfg(linux).
crate::common::platform_twin_test!(TwinHost::Macos, oracle_file_acl_acl_enabled_build_edges, {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((path (make-temp-file "neo-b3-acl-")))
  (unwind-protect
      (list
       (condition-case err (file-acl path) (error (list (car err) (cdr err))))
       (condition-case err (file-acl 42) (error (list (car err) (cdr err))))
       (condition-case err (file-acl nil) (error (list (car err) (cdr err))))
       (condition-case err (file-acl '(not-a-file-name)) (error (list (car err) (cdr err))))
       (condition-case err (file-acl) (error (list (car err) (cdr err))))
       (condition-case err (file-acl "one" "two") (error (list (car err) (cdr err)))))
    (delete-file path)))
"#;
    let expect = expect_test::expect![[
        r#""OK (nil (wrong-type-argument (stringp 42)) (wrong-type-argument (stringp nil)) (wrong-type-argument (stringp (not-a-file-name))) (wrong-number-of-arguments (file-acl 0)) (wrong-number-of-arguments (file-acl 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
});
