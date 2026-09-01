//! Strict combo oracle probes, batch 252: binary heap. heap-create with a
//! compare function, add/root/delete-root, size, and empty check.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_heap_min_priority_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'heap)
(let ((h (heap-create (lambda (a b) (< a b)) 0)))
  (heap-add h 5)
  (heap-add h 1)
  (heap-add h 3)
  (heap-add h 8)
  (heap-add h 2)
  (list (heap-size h)
        (heap-root h)
        (heap-empty h)
        (heap-delete-root h)
        (heap-root h)))
"##;
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"heap\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_heap_drain_sorted_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'heap)
(let ((h (heap-create (lambda (a b) (< a b)) 0)))
  (dolist (n '(7 2 9 1 5 3 8)) (heap-add h n))
  (let ((collected nil))
    (while (not (heap-empty h))
      (push (heap-delete-root h) collected))
    (list (nreverse collected)
          (heap-empty h)
          (heap-size h))))
"##;
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"heap\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_heap_max_priority_and_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'heap)
(let ((h (heap-create (lambda (a b) (> a b)) 0)))
  (dolist (n '(3 1 4 1 5 9 2 6)) (heap-add h n))
  (list (heap-root h)
        (heap-delete-root h)
        (heap-root h)
        (heap-size h)))
"##;
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"heap\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
