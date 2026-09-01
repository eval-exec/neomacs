/// Batch 519: file-name-case-insensitivity, file-truename, expand-file-name edge.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx519_expand_file_name_tilde() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"[ORACLE-HOME]/test\" \"/root/test\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (expand-file-name "~/test") (expand-file-name "~root/test"))
"##,
        expect,
    );
}

#[test]
fn div_cx519_expand_file_name_dots() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"/a/c\" \"/a/b\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (expand-file-name "/a/b/../c") (expand-file-name "/a/./b"))
"##,
        expect,
    );
}

#[test]
fn div_cx519_expand_file_name_slashes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"/a/b\" \"/a/b\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (expand-file-name "///a///b") (expand-file-name "/a//b"))
"##,
        expect,
    );
}

#[test]
fn div_cx519_file_truename_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"/tmp\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(file-truename "/tmp")
"##,
        expect,
    );
}

#[test]
fn div_cx519_file_name_absolute() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (file-name-absolute-p "/a/b") (file-name-absolute-p "a/b"))
"##,
        expect,
    );
}

#[test]
fn div_cx519_file_name_directory() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"/a/b/\" \"a/b/\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (file-name-directory "/a/b/c") (file-name-directory "a/b/" ))
"##,
        expect,
    );
}

#[test]
fn div_cx519_file_name_as_directory() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"/a/b/\" \"/a/b\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (file-name-as-directory "/a/b") (directory-file-name "/a/b/"))
"##,
        expect,
    );
}

#[test]
fn div_cx519_file_name_sans_ext() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (file-name-sans-extension "foo.tar.gz")
      (file-name-sans-extension "foo.tar.gz" t)
      (file-name-extension "foo.tar.gz")
      (file-name-extension "foo.tar.gz" t))
"##,
        expect,
    );
}

#[test]
fn div_cx519_file_size_human_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"1k\" \"1M\" \"1G\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (file-size-human-readable 1024)
      (file-size-human-readable 1048576)
      (file-size-human-readable 1073741824))
"##,
        expect,
    );
}

#[test]
fn div_cx519_directory_folder_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((d (make-temp-file "cx519-dir-" t)))
  (unwind-protect (file-directory-p d) (delete-directory d t)))
"##,
        expect,
    );
}

#[test]
fn div_cx519_file_regular_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (make-temp-file "cx519-reg-")))
  (unwind-protect (file-regular-p f) (delete-file f)))
"##,
        expect,
    );
}

#[test]
fn div_cx519_file_accessible_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (make-temp-file "cx519-acc-")))
  (unwind-protect (file-accessible-directory-p f) (delete-file f)))
"##,
        expect,
    );
}

#[test]
fn div_cx519_file_selinux_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (make-temp-file "cx519-sel-")))
  (unwind-protect (condition-case e (file-selinux-context f) (error (car e))) (delete-file f)))
"##,
        expect,
    );
}

#[test]
fn div_cx519_file_acl_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (make-temp-file "cx519-acl-")))
  (unwind-protect (condition-case e (file-acl f) (error (car e))) (delete-file f)))
"##,
        expect,
    );
}

#[test]
fn div_cx519_file_modes_symbolic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Parse error in modes near ‘rw-r--r--’\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (file-modes-number-to-symbolic #o644)
      (file-modes-number-to-symbolic #o755)
      (file-modes-symbolic-to-number "rw-r--r--"))
"##,
        expect,
    );
}
