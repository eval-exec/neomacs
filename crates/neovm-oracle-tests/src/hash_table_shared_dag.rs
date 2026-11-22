//! Oracle parity for an `equal` hash table keyed by a shared cons DAG.
//!
//! A shared DAG's unfolded tree is exponential in its depth.  GNU's equal
//! hash is bounded (`SXHASH_MAX_DEPTH` 3, `SXHASH_MAX_LEN` 7, src/fns.c
//! `sxhash_obj`/`sxhash_list`), and a table entry stores only hash, key and
//! value (src/lisp.h `Lisp_Hash_Table`), so `puthash`/`gethash`/`remhash`
//! compare live key objects with a bounded hash.  Neomacs also keeps a
//! structural key snapshot beside each entry for iteration, dumping and
//! switch plans; this pins that budget-truncating that snapshot leaves
//! membership, replacement, count, iteration and removal identical to GNU.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity_expect;

/// Build a `2^30`-node unfolded DAG out of 30 shared conses.
const SHARED_DAG: &str = r#"(let ((v 1))
                              (dotimes (_ 30) (setq v (cons v v)))
                              v)"#;

#[test]
fn oracle_prop_hash_table_equal_shared_dag_membership() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((make (lambda ()
                                (let ((v 1))
                                  (dotimes (_ 30) (setq v (cons v v)))
                                  v)))
                         (h (make-hash-table :test 'equal)))
                     (let ((a (funcall make))
                           (b (funcall make)))
                       (puthash a 'first h)
                       (let ((after-first (list (gethash a h)
                                                (gethash b h)
                                                (hash-table-count h))))
                         (puthash b 'second h)
                         (let ((seen 0))
                           (maphash (lambda (_key _value) (setq seen (1+ seen))) h)
                           (list after-first
                                 (list (gethash a h)
                                       (gethash b h)
                                       (hash-table-count h)
                                       seen)
                                 (progn
                                   (remhash a h)
                                   (list (gethash b h) (hash-table-count h))))))))"#;
    let expect = expect_test::expect![[r#""OK ((first first 1) (second second 1 1) (nil 0))""#]];
    assert_oracle_parity_expect(&form, expect);
}

#[test]
fn oracle_prop_hash_table_equal_shared_dag_snapshot_helpers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The same object graph through the iteration helpers that walk every
    // stored entry, plus the table's reported test.
    let form = format!(
        r#"(let ((dag {SHARED_DAG})
                 (h (make-hash-table :test #'equal)))
             (let ((seen nil))
               (puthash dag 42 h)
               (puthash dag 43 h)
               (maphash (lambda (_key value) (push value seen)) h)
               (list (gethash dag h)
                     (hash-table-count h)
                     (nreverse seen)
                     (hash-table-test h))))"#
    );
    let expect = expect_test::expect![[r#""OK (43 1 (43) equal)""#]];
    assert_oracle_parity_expect(&form, expect);
}
