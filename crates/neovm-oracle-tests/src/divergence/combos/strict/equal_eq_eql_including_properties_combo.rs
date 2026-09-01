//! Strict combo oracle probes, batch 197: equality predicates deep. equal on
//! lists/vectors/strings/numbers/bool-vectors, eq/eql numeric edges, equal-
//! including-properties on propertized strings, and identity vs structural
//! equality of conses.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_equal_structural_nested_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (equal '(1 2 3) '(1 2 3))
      (equal '(1 2 3) '(1 2 4))
      (equal [1 2 3] [1 2 3])
      (equal [1 2 3] (vector 1 2 3))
      (equal "abc" "abc")
      (equal "abc" (copy-sequence "abc"))
      (equal '(1 (2 3) (4 (5))) '(1 (2 3) (4 (5))))
      (equal #*0101 #*0101)
      (equal (bool-vector t nil t) (bool-vector t nil t))
      (equal (cons 1 2) (cons 1 2))
      (not (equal '(1 2) '(1 2 3))))
"##;
    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#*\" 9 15)""##]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_eq_eql_identity_numeric_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (eq 'a 'a)
      (eq (cons 1 2) (cons 1 2))
      (eq "abc" "abc")
      (let ((s "abc")) (eq s s))
      (eql 1.0 1.0)
      (eql 1 1)
      (eql 1 1.0)
      (eq 1 1)
      (eq most-positive-fixnum most-positive-fixnum)
      (eq (1+ most-positive-fixnum) (1+ most-positive-fixnum))
      (eql ?a ?a)
      (eq ?a ?a)
      (eql 1/2 1/2)
      (eq 1/2 1/2))
"##;
    let expect = expect_test::expect![[r#""ERR (void-variable 1/2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_equal_including_properties_identity_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (equal-including-properties (propertize "x" 'face 'bold)
                                  (propertize "x" 'face 'bold))
      (equal-including-properties "x" (propertize "x" 'face 'bold))
      (equal-including-properties "abc" "abc")
      (equal-including-properties (propertize "abc" 'a 1 'b 2)
                                  (propertize "abc" 'a 1 'b 2))
      (equal-including-properties (propertize "abc" 'a 1)
                                  (propertize "abc" 'b 1))
      (equal (propertize "x" 'face 'bold)
             (propertize "x" 'face 'bold)))
"##;
    let expect = expect_test::expect![[r#""OK (t nil t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
