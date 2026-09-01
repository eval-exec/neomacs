//! Strict combo oracle probes, batch 289: ring CORE insert/remove/ref/length
//! combo. Any nil-in-Neomacs/t-in-GNU is a missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_ring_make_insert_remove_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((ring (make-ring 4)))
  (ring-insert ring 'a)
  (ring-insert ring 'b)
  (ring-insert ring 'c)
  (list (ring-length ring)
        (ring-ref ring 0)
        (ring-ref ring 1)
        (ring-ref ring 2)
        (ring-remove ring 1)
        (ring-ref ring 0)
        (ring-ref ring 1)
        (ring-length ring)))
"##;
    let expect = expect_test::expect![[r#""OK (3 c b a b c a 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_ring_empty_resize_elements_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((ring (make-ring 3)))
  (list (ring-empty-p ring)
        (ring-p ring)
        (progn
          (ring-insert ring 1)
          (ring-insert ring 2)
          (ring-insert ring 3)
          (ring-insert ring 4)
          (ring-length ring))
        (ring-elements ring)
        (progn
          (ring-resize ring 5)
          (ring-insert ring 5)
          (ring-length ring))
        (ring-elements ring)
        (ring-empty-p ring)))
"##;
    let expect = expect_test::expect![[r#""OK (t t 3 (4 3 2) 4 (5 4 3 2) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_ring_insert_at_extend_plus_remove_plus_plus_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((ring (make-ring 3)))
  (ring-insert+extend ring 'x nil)
  (ring-insert+extend ring 'y nil)
  (ring-insert+extend ring 'z nil)
  (ring-insert+extend ring 'w 'grow)
  (list (ring-length ring)
        (ring-size ring)
        (ring-elements ring)
        (ring-remove+extend ring 0 nil)
        (ring-length ring)
        (ring-ref ring 0)
        (ring-ref ring 1)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function ring-remove+extend)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
