//! Strict combo oracle probes, batch 68: character composition (compose-region,
//! find-composition, compose-string, decompose-string) and standard-output
//! printing (prin1/princ/terpri to a buffer).
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_o2_compose_region_find_composition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 3 t) (1 3 t) nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (compose-region 1 3)
  (list (find-composition 1)
        (find-composition 2)
        (find-composition 4)
        (not (null (get-text-property 1 'composition)))
        (null (get-text-property 4 'composition))))
"##,
        expect,
    );
}

#[test]
fn div_o2_compose_string_decompose() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s1 (compose-string "ab"))
       (comp (get-text-property 0 'composition s1))
       (s2 (decompose-string s1)))
  (list (not (null comp))
        (stringp s1)
        (stringp s2)
        (null (get-text-property 0 'composition s2))))
"##,
        expect,
    );
}

#[test]
fn div_o2_print_to_standard_output_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"foo bar 42\\n\\nlist\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((standard-output (current-buffer)))
    (prin1 'foo)
    (princ " bar ")
    (princ 42)
    (terpri)
    (print 'list))
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_o2_compose_with_components() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument arrayp nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc")
  (compose-region 1 4 (string #x2014))
  (list (find-composition 1)
        (not (null (get-text-property 1 'composition)))
        (char-to-string (aref (cdr (get-text-property 1 'composition)) 1))))
"##,
        expect,
    );
}
