/// Batch 514: localized environment tests - locale, language, charset deep.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx514_set_locale() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (set-locale-environment "en_US.UTF-8")
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx514_set_language() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (set-language-environment "English")
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx514_current_language() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function current-language-environment)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(current-language-environment)
"##,
        expect,
    );
}

#[test]
fn div_cx514_language_info() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"English\" (documentation . \"Nothing special is needed to handle English.\") (sample-text . \"Hello!, Hi!, How are you?\") (charset ascii) (tutorial . \"TUTORIAL\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(assoc "English" language-info-alist)
"##,
        expect,
    );
}

#[test]
fn div_cx514_charset_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function charset-list)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((charsets (charset-list)))
  (list (listp charsets) (> (length charsets) 10)))
"##,
        expect,
    );
}

#[test]
fn div_cx514_charset_description() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"ASCII (ISO646 IRV)\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (charset-description 'ascii)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx514_charset_iso_final() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 66""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (charset-iso-final-char 'ascii)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx514_charset_short_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK wrong-type-argument""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (charset-short-name 'ascii)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx514_charset_long_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK wrong-type-argument""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (charset-long-name 'ascii)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx514_charset_dimension() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (charset-dimension 'ascii)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx514_charset_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 128""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (charset-chars 'ascii)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx514_charset_max_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (charset-max-char 'ascii)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx514_charset_min_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (charset-min-char 'ascii)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx514_charset_coding_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (charset-coding-system 'ascii)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx514_charset_plist_complete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (ascii [0 127 0 0 0 0 0 0])""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (let ((pl (charset-plist 'ascii)))
      (list (plist-get pl :name) (plist-get pl :code-space)))
  (error (car e)))
"##,
        expect,
    );
}
