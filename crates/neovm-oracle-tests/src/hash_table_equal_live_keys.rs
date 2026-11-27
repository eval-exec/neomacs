//! Oracle parity for `equal` tables that keep the key object a `puthash`
//! was given.  GNU stores the key itself in the entry (src/lisp.h
//! `Lisp_Hash_Table`, src/fns.c `hash_put`), so `maphash` hands back that
//! object, an equal copy finds it, and a key mutated after insertion stays
//! filed under the hash it had when it was put.  The last case is the table
//! a dump must round-trip: two string keys, one mutated to equal the other,
//! keep two entries and both values.

use crate::common::assert_oracle_parity_expect;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_hash_table_equal_keeps_the_key_object() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((h (make-hash-table :test 'equal))
                         (k (list 1 "two" 3.5))
                         (s (propertize "key" 'face 'bold))
                         got)
                     (puthash k 'v h)
                     (puthash s 'p h)
                     (puthash (list 1 "two" 3.5) 'w h)
                     (maphash (lambda (key _) (push key got)) h)
                     (list (hash-table-count h)
                           (gethash (list 1 "two" 3.5) h)
                           (gethash "key" h)
                           (and (memq k got) t)
                           (and (memq s got) t)
                           (get-text-property 0 'face s)))"#;
    assert_oracle_parity_expect(form, expect_test::expect![[r#""OK (2 w p t t bold)""#]]);
}

#[test]
fn oracle_prop_hash_table_equal_key_mutated_after_insertion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((h (make-hash-table :test 'equal))
                         (k (list 1 2 3)))
                     (puthash k 'v h)
                     (setcar k 9)
                     (list (gethash k h)
                           (gethash (list 1 2 3) h)
                           (gethash (list 9 2 3) h)
                           (progn (puthash (list 9 2 3) 'w h)
                                  (hash-table-count h))))"#;
    assert_oracle_parity_expect(form, expect_test::expect![[r#""OK (nil nil nil 2)""#]]);
}

#[test]
fn oracle_prop_hash_table_equal_string_keys_mutated_to_collide() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((h (make-hash-table :test 'equal))
                         (a (copy-sequence "a"))
                         (b (copy-sequence "b"))
                         out)
                     (puthash a 1 h)
                     (puthash b 2 h)
                     (aset b 0 ?a)
                     (maphash (lambda (key value) (push (cons key value) out)) h)
                     (list (hash-table-count h)
                           (sort out (lambda (x y) (< (cdr x) (cdr y))))
                           (gethash "a" h)
                           (let ((c (copy-hash-table h)) seen)
                             (maphash (lambda (_ value) (push value seen)) c)
                             (list (hash-table-count c) (sort seen #'<)))))"#;
    assert_oracle_parity_expect(
        form,
        expect_test::expect![[r#""OK (2 ((\"a\" . 1) (\"a\" . 2)) 1 (2 (1 2)))""#]],
    );
}
