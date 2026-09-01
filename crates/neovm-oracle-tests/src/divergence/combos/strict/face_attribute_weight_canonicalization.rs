//! Strict combo oracle probes, batch 92: face weight canonicalization via
//! set-face-attribute/face-attribute (not just font-face-attributes). Tests
//! whether setting :weight 'normal/'heavy/'ultra-bold on a face and reading
//! it back also canonicalizes in GNU but not Neomacs.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_q6_face_attribute_weight_set_and_read() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((normal . normal) (bold . bold) (heavy . heavy) (light . light) (ultra-bold . ultra-bold) (semi-bold . semi-bold) (medium . medium))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (result)
  (make-face 'probe-wt-face-q6)
  (dolist (w '(normal bold heavy light ultra-bold semi-bold medium))
    (set-face-attribute 'probe-wt-face-q6 nil :weight w)
    (push (cons w (face-attribute 'probe-wt-face-q6 :weight nil 'default)) result))
  (nreverse result))
"##,
        expect,
    );
}

#[test]
fn div_q6_face_attribute_slant_set_and_read() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((normal . normal) (italic . italic) (oblique . oblique))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (result)
  (make-face 'probe-slant-face-q6)
  (dolist (s '(normal italic oblique))
    (set-face-attribute 'probe-slant-face-q6 nil :slant s)
    (push (cons s (face-attribute 'probe-slant-face-q6 :slant nil 'default)) result))
  (nreverse result))
"##,
        expect,
    );
}

#[test]
fn div_q6_face_attribute_numeric_weight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument symbolp 100)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (result)
  (make-face 'probe-numwt-face-q6)
  (dolist (w '(100 200 300 400 500 600 700 800 900))
    (set-face-attribute 'probe-numwt-face-q6 nil :weight w)
    (push (cons w (face-attribute 'probe-numwt-face-q6 :weight nil 'default)) result))
  (nreverse result))
"##,
        expect,
    );
}
