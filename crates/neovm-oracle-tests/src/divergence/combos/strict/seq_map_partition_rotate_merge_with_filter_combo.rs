//! Strict combo oracle probes, batch 167: seq.el and map.el combinators.
//! seq-partition/rotate/drop-while/take-while/keep/max-by/min-by/group-by-deep,
//! map-merge-with/map-filter/map-keys/map-values/map-pairs/map-apply, and
//! map-elt with default over list and hash-table maps.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_seq_partition_rotate_take_drop_keep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'seq)
(list (seq-partition '(1 2 3 4 5 6 7) 3)
      (seq-rotate '(1 2 3 4) 1)
      (seq-rotate '(1 2 3 4) -1)
      (seq-drop-while (lambda (x) (< x 3)) '(1 2 3 4 5))
      (seq-take-while (lambda (x) (< x 3)) '(1 2 3 4 5))
      (seq-keep (lambda (x) (and (> x 2) x)) '(1 2 3 4 5))
      (seq-max-by '(1 9 3 7) #'>)
      (seq-min-by '(1 9 3 7) #'<)
      (seq-group-by (lambda (x) (% x 2)) '(1 2 3 4 5 6))
      (seq-sort-by '("ccc" "a" "bb") #'length #'<)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function seq-rotate)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_map_merge_with_filter_keys_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'map)
(let ((m1 (list (cons 'a 1) (cons 'b 2)))
      (m2 (list (cons 'b 3) (cons 'c 4)))
      (h (make-hash-table :test 'equal)))
  (puthash 'x 10 h)
  (puthash 'y 20 h)
  (list (map-merge-with #'+ m1 m2)
        (map-filter (lambda (k v) (> v 1)) m1)
        (map-keys m2)
        (map-values m1)
        (map-elt m1 'b)
        (map-elt m1 'z 'default)
        (map-elt h 'x)
        (map-contains-key m1 'a)
        (map-contains-key m1 'z)
        (map-length m2)))
"##;
    let expect =
        expect_test::expect![[r#""ERR (cl-no-applicable-method map-into ((b . 3) (c . 4)) +)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_map_apply_pairs_do_map_resize() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'map)
(let ((m (list (cons 'a 1) (cons 'b 2) (cons 'c 3))))
  (list (sort (map-pairs (lambda (k v) (cons k (* v 10))) m)
              (lambda (p q) (string< (symbol-name (car p)) (symbol-name (car q)))))
        (let ((acc nil))
          (map-do (lambda (k v) (push (cons k v) acc)) m)
          (sort acc (lambda (p q) (string< (symbol-name (car p)) (symbol-name (car q))))))
        (map-apply (lambda (k v) (+ v 100)) m)
        (sort (map-apply (lambda (k v) (list k v)) m)
              (lambda (p q) (string< (symbol-name (car p)) (symbol-name (car q)))))))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
