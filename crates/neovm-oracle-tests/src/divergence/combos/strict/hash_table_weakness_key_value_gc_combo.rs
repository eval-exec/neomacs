//! Strict combo oracle probes, batch 334: hash-table weakness. make-hash-table
//! with :weakness t/k/v/kv, hash-table-weakness query, and key/value GC behavior.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_hash_table_weakness_key_value_setup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (hash-table-weakness (make-hash-table :weakness t))
      (hash-table-weakness (make-hash-table :weakness 'key))
      (hash-table-weakness (make-hash-table :weakness 'value))
      (hash-table-weakness (make-hash-table :weakness 'key-or-value))
      (hash-table-weakness (make-hash-table))
      (let ((h (make-hash-table :test 'equal :weakness t)))
        (puthash "key" 'val h)
        (list (hash-table-count h) (hash-table-weakness h))))
"##;
    let expect = expect_test::expect![[
        r#""OK (key-and-value key value key-or-value nil (1 key-and-value))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_hash_table_weak_garbage_collect_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((h (make-hash-table :test 'eq :weakness t))
       (k (cons 1 2))
       (other (cons 3 4)))
  (puthash k 'weak-val h)
  (puthash 'stable-key 'stable-val h)
  (let ((c1 (hash-table-count h)))
    (garbage-collect)
    (let ((c2 (hash-table-count h)))
      (list c1
            c2
            (gethash k h 'gone)
            (gethash 'stable-key h)
            (>= c2 1)))))
"##;
    let expect = expect_test::expect![[r#""OK (2 2 weak-val stable-val t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_hash_table_rehash_size_threshold_growth() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((h (make-hash-table :test 'equal :size 4 :rehash-size 2.0)))
  (dotimes (i 10) (puthash (intern (format "k%d" i)) i h))
  (list (hash-table-count h)
        (hash-table-size h)
        (hash-table-rehash-size h)
        (hash-table-test h)
        (gethash 'k5 h)
        (hash-table-empty-p h)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function hash-table-empty-p)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
