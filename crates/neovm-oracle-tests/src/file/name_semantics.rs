//! Oracle parity tests for GNU file-name manipulation semantics.
//!
//! GNU Emacs implements the core primitives in `src/fileio.c`, with
//! `abbreviate-file-name` layered in `lisp/files.el`.  These tests avoid
//! filesystem existence checks and compare canonical string semantics only.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_file_name_directory_and_nondirectory_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (mapcar #'file-name-directory
         '("plain" "dir/file" "/root/file" "/root/dir/" "/" ""))
 (mapcar #'file-name-nondirectory
         '("plain" "dir/file" "/root/file" "/root/dir/" "/" "")))
"#;

    let expect = expect_test::expect![[
        r#""OK ((nil \"dir/\" \"/root/\" \"/root/dir/\" \"/\" nil) (\"plain\" \"file\" \"file\" \"\" \"\" \"\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_directory_file_name_and_file_name_as_directory_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (mapcar #'file-name-as-directory
         '("" "." "dir" "dir/" "/" "//" "///"))
 (mapcar #'directory-file-name
         '("" "." "dir" "dir/" "/" "//" "///" "/tmp///")))
"#;

    let expect = expect_test::expect![[
        r#""OK ((\"./\" \"./\" \"dir/\" \"dir/\" \"/\" \"//\" \"///\") (\"\" \".\" \"dir\" \"dir\" \"/\" \"//\" \"/\" \"/tmp\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_expand_file_name_canonicalizes_without_stat() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((default-directory "/tmp/base/dir/"))
  (list
   (expand-file-name "a/./b/../c")
   (expand-file-name "../sibling" "/tmp/base/dir")
   (expand-file-name "" "/tmp/base/dir")
   (expand-file-name "." "/")
   (expand-file-name ".." "/")
   (expand-file-name "/tmp//base///file" "/ignored")))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"/tmp/base/dir/a/c\" \"/tmp/base/sibling\" \"/tmp/base/dir\" \"/\" \"/..\" \"/tmp/base/file\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_substitute_in_file_name_env_and_embedded_absolute() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((process-environment (copy-sequence process-environment)))
  (setenv "NEOMACS_ORACLE_DIR" "/tmp/oracle-root")
  (setenv "NEOMACS_ORACLE_NAME" "leaf")
  (list
   (substitute-in-file-name "$NEOMACS_ORACLE_DIR/$NEOMACS_ORACLE_NAME")
   (substitute-in-file-name "${NEOMACS_ORACLE_DIR}/x")
   (substitute-in-file-name "$NEOMACS_ORACLE_MISSING/x")
   (substitute-in-file-name "/prefix//$NEOMACS_ORACLE_DIR/tail")
   (substitute-in-file-name "/prefix/~user-kept/tail")))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"/tmp/oracle-root/leaf\" \"/tmp/oracle-root/x\" \"$NEOMACS_ORACLE_MISSING/x\" \"/tmp/oracle-root/tail\" \"/prefix/~user-kept/tail\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_abbreviate_file_name_home_and_directory_abbrev() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((process-environment (copy-sequence process-environment))
      (abbreviated-home-dir nil)
      (directory-abbrev-alist '(("\\`/tmp/very-long-root/" . "/short/"))))
  (setenv "HOME" "/tmp/neomacs-home")
  (list
   (abbreviate-file-name "/tmp/neomacs-home/project/file.el")
   (abbreviate-file-name "/tmp/very-long-root/project/file.el")
   (abbreviate-file-name "/tmp/neomacs-home")
   (abbreviate-file-name "/")))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"~/project/file.el\" \"/short/project/file.el\" \"~\" \"/\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
