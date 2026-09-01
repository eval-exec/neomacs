//! Divergence tests: file local variables, dir-locals, safe-local-vars.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_file_local_variables() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'hack-local-variables)
  (fboundp 'hack-local-variables-filter)
  (boundp 'enable-local-variables)
  (boundp 'safe-local-variable-values)
  (fboundp 'safe-local-variable-p))"#,
        expect,
    );
}

#[test]
fn divergence_dir_locals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'dir-locals-set-directory-class)
  (fboundp 'dir-locals-set-class-variables)
  (fboundp 'dir-locals-find-file)
  (featurep 'files))"#,
        expect,
    );
}

#[test]
fn divergence_file_handlers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'file-name-handler-alist)
  (listp file-name-handler-alist)
  (fboundp 'find-file-name-handler))"#,
        expect,
    );
}

#[test]
fn divergence_file_attributes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'file-attributes)
  (fboundp 'file-attributes-lessp)
  (fboundp 'file-newer-than-file-p)
  (fboundp 'file-exists-p)
  (fboundp 'file-readable-p)
  (fboundp 'file-writable-p))"#,
        expect,
    );
}

#[test]
fn divergence_file_expand() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'expand-file-name)
  (fboundp 'abbreviate-file-name)
  (fboundp 'file-name-directory)
  (fboundp 'file-name-nondirectory)
  (fboundp 'file-name-sans-versions))"#,
        expect,
    );
}

#[test]
fn divergence_path_names() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'directory-file-name)
  (fboundp 'file-name-as-directory)
  (fboundp 'file-relative-name)
  (fboundp 'file-name-absolute-p)
  (= (length (file-name-split "/foo/bar/baz")) 4)) "#,
        expect,
    );
}

#[test]
fn divergence_directory_listing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'directory-files)
  (fboundp 'directory-files-and-attributes)
  (fboundp 'file-expand-wildcards)
  (fboundp 'file-all-completions))"#,
        expect,
    );
}

#[test]
fn divergence_make_dir() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'make-directory)
  (fboundp 'make-nearby-directory)
  (fboundp 'delete-directory)
  (fboundp 'copy-directory)
  (fboundp 'rename-file)
  (fboundp 'copy-file))"#,
        expect,
    );
}

#[test]
fn divergence_file_symlink() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'make-symbolic-link)
  (fboundp 'file-symlink-p)
  (fboundp 'file-truename)
  (fboundp 'file-chase-links))"#,
        expect,
    );
}

#[test]
fn divergence_temp_files() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'make-temp-file)
  (fboundp 'make-temp-name)
  (fboundp 'temporary-file-directory)
  (stringp (temporary-file-directory))
  (fboundp 'small-temporary-file-directory))"#,
        expect,
    );
}
