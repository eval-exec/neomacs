//! Strict combo oracle probes, batch 33: loaded-library coverage.
//! These libraries are NOT preloaded in -Q, so they are loaded via
//! assert_oracle_parity_with_load (both engines load the same .el source from
//! lisp/). Covers seq.el (generic sequence ops) and map.el (generic map ops).
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_h0_seq_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((2 3 4) (2 3 4) (2 4) 10 2 3 3 [1 2 3])""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (seq-map #'1+ '(1 2 3))
      (seq-map #'1+ [1 2 3])
      (seq-filter (lambda (x) (= (% x 2) 0)) '(1 2 3 4 5))
      (seq-reduce #'+ '(1 2 3 4) 0)
      (seq-elt '(1 2 3) 1)
      (seq-length '(1 2 3))
      (seq-length "abc")
      (seq-into '(1 2 3) 'vector))
"##,
        &["emacs-lisp/seq.el"],
        expect,
    );
}

#[test]
fn div_h0_seq_search_order_minmax() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function seq-containsp)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (seq-position '(1 2 3 2) 2)
      (seq-containsp '(1 2 3) 2)
      (seq-containsp '(1 2 3) 9)
      (seq-min '(3 1 2))
      (seq-max '(3 1 2))
      (seq-sort #'< '(3 1 2))
      (seq-uniq '(1 2 1 3 2)))
"##,
        &["emacs-lisp/seq.el"],
        expect,
    );
}

#[test]
fn div_h0_seq_group_partition_concat() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function seq-split-at)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (seq-group-by (lambda (x) (= (% x 2) 0)) '(1 2 3 4))
      (seq-partition '(1 2 3 4 5) 2)
      (seq-split-at '(1 2 3 4 5) 2)
      (seq-reverse '(1 2 3))
      (seq-concatenate 'list '(1 2) '(3 4)))
"##,
        &["emacs-lisp/seq.el"],
        expect,
    );
}

#[test]
fn div_h0_map_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function symbol<)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (sort (map-keys '((a . 1) (b . 2))) #'symbol<)
      (sort (map-values '((a . 1) (b . 2))) #'<)
      (map-length '((a . 1) (b . 2)))
      (map-elt '((a . 1)) 'a)
      (map-contains-key '((a . 1)) 'a)
      (map-contains-key '((a . 1)) 'z))
"##,
        &["emacs-lisp/map.el"],
        expect,
    );
}

#[test]
fn div_h0_map_merge_and_do() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (((a . 1) (b . 2)) ((a . 1) (b . 2)))""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (map-merge 'alist '((a . 1)) '((b . 2)))
      (let (out)
        (map-do (lambda (k v) (setq out (cons (cons k v) out))) '((a . 1) (b . 2)))
        (sort out (lambda (x y) (string< (symbol-name (car x)) (symbol-name (car y)))))))
"##,
        &["emacs-lisp/map.el"],
        expect,
    );
}

#[test]
fn div_h0_hash_table_via_seq_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function symbol<)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(let ((h (make-hash-table :test 'equal)))
  (puthash 'a 1 h)
  (puthash 'b 2 h)
  (puthash 'c 3 h)
  (list (sort (map-keys h) #'symbol<)
        (sort (map-values h) #'<)
        (map-length h)
        (map-elt h 'b)
        (map-elt h 'z 'missing)))
"##,
        &["emacs-lisp/map.el"],
        expect,
    );
}
