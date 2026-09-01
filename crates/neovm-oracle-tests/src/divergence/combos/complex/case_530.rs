/// Batch 530: font-get, font-face-attributes, font-xlfd-name, font-spec deep.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx530_font_spec_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t Monospace)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (fontp (font-spec :family "Monospace"))
      (font-get (font-spec :family "Monospace") :family))
"##,
        expect,
    );
}

#[test]
fn div_cx530_font_put() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 12""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (font-spec :family "Monospace")))
  (font-put f :size 12)
  (font-get f :size))
"##,
        expect,
    );
}

#[test]
fn div_cx530_font_face_attr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family \"Monospace\" :height 100) (:family \"Monospace\" :height 120 :weight bold))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (font-face-attributes "Monospace-10")
      (font-face-attributes "Monospace-12:bold"))
"##,
        expect,
    );
}

#[test]
fn div_cx530_font_xlfd() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"-*-Monospace-*-*-*-*-*-*-*-*-*-*-*-*\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (font-xlfd-name (font-spec :family "Monospace"))
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx530_font_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (font-match (font-spec :family "Monospace"))
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx530_font_list_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (length (font-list-fonts (font-spec :family "Monospace")))
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx530_font_family_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((families (font-family-list)))
  (list (listp families) (> (length families) 0)))
"##,
        expect,
    );
}

#[test]
fn div_cx530_font_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (font-type (font-spec :family "Monospace"))
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx530_font_style() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (font-style (font-spec :family "Monospace"))
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx530_font_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (font-width (font-spec :family "Monospace"))
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx530_font_weight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (font-weight (font-spec :family "Monospace"))
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx530_font_slant() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (font-slant (font-spec :family "Monospace"))
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx530_font_underline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (font-get (font-spec :family "Monospace") :underline)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx530_font_overstrike() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (font-get (font-spec :family "Monospace") :overstrike)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx530_font_pixel_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 12""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (font-get (font-spec :family "Monospace" :size 12) :size)
  (error (car e)))
"##,
        expect,
    );
}
