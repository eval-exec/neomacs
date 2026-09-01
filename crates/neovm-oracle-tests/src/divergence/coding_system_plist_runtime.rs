//! coding-system-plist / coding-system-get / -category / -base parity for
//! utf-8 and its eol variants, latin-1, us-ascii; plus the no-conversion
//! plist :eol-type gap.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn cs_category_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (coding-category-utf-8 coding-category-charset coding-category-raw-text)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (coding-system-category 'utf-8)
        (coding-system-category 'iso-8859-1)
        (coding-system-category 'raw-text))"##,
        expect,
    );
}

#[test]
fn cs_get_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (utf-8 iso-8859-1 big \"UTF-8 (no signature (BOM))\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (coding-system-get 'utf-8 :mime-charset)
        (coding-system-get 'iso-8859-1 :mime-charset)
        (coding-system-get 'utf-16 :endian)
        (coding-system-doc-string 'utf-8))"##,
        expect,
    );
}

#[test]
fn csplist_utf8() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (utf-8 utf-8 t coding-category-utf-8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((p (coding-system-plist 'utf-8)))
  (list (plist-get p :name) (plist-get p :mime-charset) (plist-get p :ascii-compatible-p)
        (plist-get p :category)))"##,
        expect,
    );
}

#[test]
fn csplist_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil iso-8859-1 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (plist-get (coding-system-plist 'utf-8-unix) :eol-type)
        (plist-get (coding-system-plist 'utf-8-dos) :eol-type)
        (plist-get (coding-system-plist 'latin-1) :mime-charset)
        (plist-get (coding-system-plist 'us-ascii) :ascii-compatible-p))"##,
        expect,
    );
}

#[test]
fn divergence_coding_plist_no_conversion_eol_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (unix (:eol-type unix) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (plist-get (coding-system-plist 'no-conversion) :eol-type)
      (plist-member (coding-system-plist 'no-conversion) :eol-type)
      (plist-get (coding-system-plist 'raw-text) :eol-type))"##,
        expect,
    );
}
