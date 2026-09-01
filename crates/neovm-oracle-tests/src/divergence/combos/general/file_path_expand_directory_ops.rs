//! Deep combo: file-name operations + expand-file-name + directory + path handling.
//! Tests path manipulation and file system introspection functions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_expand_file_name_relative() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"[ORACLE-PROJECT-ROOT]/crates/neovm-oracle-tests/foo/bar\" \"[ORACLE-PROJECT-ROOT]/crates/neovm-oracle-tests/bar\" \"[ORACLE-HOME]/test\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (list (expand-file-name \"foo/bar\")\n\
         (expand-file-name \"./foo/../bar\")\n\
         (expand-file-name \"~/test\")))",
        expect,
    );
}

#[test]
fn deficiency_file_name_directory_and_nondirectory() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"/home/user/\" \"file.txt\" nil \"file.txt\" \"/home/user/\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (list (file-name-directory \"/home/user/file.txt\")\n\
         (file-name-nondirectory \"/home/user/file.txt\")\n\
         (file-name-directory \"file.txt\")\n\
         (file-name-nondirectory \"file.txt\")\n\
         (file-name-directory \"/home/user/\")))",
        expect,
    );
}

#[test]
fn deficiency_file_name_extension_and_sans() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"el\" \"elc\" nil \"gz\" \"test\" \"test.tar\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (list (file-name-extension \"test.el\")\n\
         (file-name-extension \"test.elc\")\n\
         (file-name-extension \"test\")\n\
         (file-name-extension \"test.tar.gz\")\n\
         (file-name-sans-extension \"test.el\")\n\
         (file-name-sans-extension \"test.tar.gz\")))",
        expect,
    );
}

#[test]
fn deficiency_file_name_as_directory() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"/home/user/\" \"/home/user\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (list (file-name-as-directory \"/home/user\")\n\
         (directory-file-name \"/home/user/\")))",
        expect,
    );
}

#[test]
fn deficiency_concat_and_expand_path_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"/tmp/sub/dir/file.txt\" \"/tmp/sub/dir/\" \"file.txt\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((base \"/tmp\")\n\
         (rel \"sub/dir/file.txt\"))\n\
         (let ((full (expand-file-name rel base)))\n\
         (list full\n\
         (file-name-directory full)\n\
         (file-name-nondirectory full)))))",
        expect,
    );
}

#[test]
fn deficiency_substitute_in_file_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"[ORACLE-HOME]/test\" \"~/exec\" \"plain/path\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (list (substitute-in-file-name \"$HOME/test\")\n\
         (substitute-in-file-name \"~/$USER\")\n\
         (substitute-in-file-name \"plain/path\")))",
        expect,
    );
}

#[test]
fn deficiency_file_truename_vs_expand() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"/var/log\" \"/var/\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((expanded (expand-file-name \"/tmp/../var/./log\")))\n\
         (list expanded\n\
         (file-name-directory expanded))))",
        expect,
    );
}

#[test]
fn deficiency_file_name_absolute_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (list (file-name-absolute-p \"/absolute/path\")\n\
         (file-name-absolute-p \"relative/path\")\n\
         (file-name-absolute-p \"~/home/path\")\n\
         (file-name-absolute-p \"./path\")))",
        expect,
    );
}

#[test]
fn deficiency_make_temp_file_and_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((tmp (make-temp-file \"neovm-test-\")))\n\
         (let ((exists (file-exists-p tmp)))\n\
         (delete-file tmp)\n\
         (list exists\n\
         (file-exists-p tmp)))))",
        expect,
    );
}

#[test]
fn deficiency_directory_files_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"a.txt\" \"b.txt\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((tmp (make-temp-file \"neovm-dir-test-\")))\n\
         (delete-file tmp)\n\
         (make-directory tmp)\n\
         (let ((f1 (expand-file-name \"a.txt\" tmp))\n\
         (f2 (expand-file-name \"b.txt\" tmp)))\n\
         (write-region \"\" nil f1)\n\
         (write-region \"\" nil f2)\n\
         (let ((files (directory-files tmp nil \"\\\\.txt$\")))\n\
         (delete-file f1)\n\
         (delete-file f2)\n\
         (delete-directory tmp)\n\
         (sort files #'string<)))))",
        expect,
    );
}
