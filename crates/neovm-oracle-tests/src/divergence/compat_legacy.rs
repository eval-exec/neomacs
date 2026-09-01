//! Divergence tests: subr-x, compat, legacy compat functions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_subr_x() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'string-join)
  (fboundp 'string-replace)
  (fboundp 'string-trim)
  (fboundp 'string-pad)
  (fboundp 'string-lines)
  (fboundp 'string-chop-newline)
  (fboundp 'hash-table-keys)
  (fboundp 'hash-table-values)
  (featurep 'subr-x)) "#,
        expect,
    );
}

#[test]
fn divergence_string_functions_modern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"a, b, c\" \"bar baz bar\" \"hello\" \"hi   \" \"hello\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (string-join '("a" "b" "c") ", ")
  (string-replace "foo" "bar" "foo baz foo")
  (string-trim "  hello  ")
  (string-pad "hi" 5)
  (string-chop-newline "hello\n")) "#,
        expect,
    );
}

#[test]
fn divergence_hash_table_modern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function hash-table-keys)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ht (make-hash-table :test 'equal)))
  (puthash "a" 1 ht)
  (puthash "b" 2 ht)
  (puthash "c" 3 ht)
  (list (sort (hash-table-keys ht) 'string<)
        (sort (hash-table-values ht) '<)
        (hash-table-count ht))) "#,
        expect,
    );
}

#[test]
fn divergence_compat_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'compat-function)
  (fboundp 'compat-version)
  (featurep 'compat)) "#,
        expect,
    );
}

#[test]
fn divergence_legacy_aliases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'string-to-number)
  (fboundp 'number-to-string)
  (fboundp 'int-to-string)
  (fboundp 'string-to-int)
  (= (string-to-number "42") 42)
  (string= (number-to-string 42) "42")) "#,
        expect,
    );
}

#[test]
fn divergence_buffer_compat() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'buffer-name)
  (fboundp 'buffer-file-name)
  (fboundp 'buffer-size)
  (fboundp 'buffer-string)
  (fboundp 'buffer-substring)
  (fboundp 'buffer-substring-no-properties)
  (stringp (buffer-name))
  (integerp (buffer-size))) "#,
        expect,
    );
}

#[test]
fn divergence_file_name_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'file-name-split)
  (fboundp 'file-name-concat)
  (fboundp 'file-name-with-extension)
  (fboundp 'file-name-extension)
  (fboundp 'file-name-sans-extension)
  (string= (file-name-extension "foo/bar.txt") "txt")
  (string= (file-name-sans-extension "foo/bar.txt") "foo/bar")) "#,
        expect,
    );
}

#[test]
fn divergence_path_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'directory-name-p)
  (fboundp 'file-name-absolute-p)
  (file-name-absolute-p "/foo/bar")
  (not (file-name-absolute-p "foo/bar"))
  (fboundp 'expand-file-name)) "#,
        expect,
    );
}

#[test]
fn divergence_internal_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'internal--scratch-buffer)
  (fboundp 'internal-show-call-stack)
  (fboundp 'internal--format-call-stack)
  (fboundp 'internal--diagnostics)) "#,
        expect,
    );
}

#[test]
fn divergence_comp_warn_vars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable warning-suppress-log-types)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'warning-suppress-types)
  (listp warning-suppress-types)
  (boundp 'warning-suppress-log-types)
  (listp warning-suppress-log-types)
  (fboundp 'display-warning)
  (fboundp 'warn)
  (fboundp 'lwarn)) "#,
        expect,
    );
}
