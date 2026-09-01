//! Complex combo batch 185 — `hash-table` deep: all weakness kinds with
//! garbage-collect interaction, test functions, rehash thresholds, and
//! `sxhash` / `sxhash-equal` / `sxhash-eq` / `sxhash-eql` consistency.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx185_hash_table_eq_eql_equal_with_edge_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil :v nil :v :v)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s1 "hello")
       (s2 (copy-sequence "hello"))
       (n1 1.0)
       (n2 1.0)
       (sym1 'k)
       (sym2 'k))
  (list
   (let ((ht (make-hash-table :test 'eq)))    (puthash s1 :v ht) (gethash s2 ht))
   (let ((ht (make-hash-table :test 'eql)))   (puthash s1 :v ht) (gethash s2 ht))
   (let ((ht (make-hash-table :test 'equal))) (puthash s1 :v ht) (gethash s2 ht))
   (let ((ht (make-hash-table :test 'eq)))    (puthash n1 :v ht) (gethash n2 ht))
   (let ((ht (make-hash-table :test 'eql)))   (puthash n1 :v ht) (gethash n2 ht))
   (let ((ht (make-hash-table :test 'eq)))    (puthash sym1 :v ht) (gethash sym2 ht))))
"##,
        expect,
    );
}

#[test]
fn div_cx185_hash_table_weakness_all_kinds_after_gc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((key 5 0) (value 5 0) (key-and-value 5 0) (key-or-value 5 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (results)
  (dolist (w '(key value key-and-value key-or-value))
    (let ((ht (make-hash-table :weakness w :test 'eq)))
      (dotimes (i 5) (puthash (cons i nil) (cons (* i 10) nil) ht))
      (let ((before (hash-table-count ht)))
        (garbage-collect)
        (push (list w before (hash-table-count ht)) results))))
  (nreverse results))
"##,
        expect,
    );
}

#[test]
fn div_cx185_hash_table_maphash_iterate_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((0 . 0) (1 . 1) (2 . 4) (3 . 9) (4 . 16) (5 . 25) (6 . 36) (7 . 49) (8 . 64) (9 . 81))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal))
      (collected nil))
  (dotimes (i 10) (puthash i (* i i) ht))
  (maphash (lambda (k v) (push (cons k v) collected)) ht)
  (sort collected (lambda (a b) (< (car a) (car b)))))
"##,
        expect,
    );
}

#[test]
fn div_cx185_hash_table_count_after_remhash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8 nil nil 25 :missing)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (dotimes (i 10) (puthash i (* i i) ht))
  (remhash 3 ht)
  (remhash 7 ht)
  (remhash 99 ht)
  (list (hash-table-count ht)
        (gethash 3 ht)
        (gethash 7 ht)
        (gethash 5 ht)
        (gethash 99 ht :missing)))
"##,
        expect,
    );
}

#[test]
fn div_cx185_hash_table_resize_threshold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (64 30 1.5 0.8125 equal nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal :size 16 :rehash-size 2.0 :rehash-threshold 0.7)))
  (dotimes (i 30) (puthash i (* i 10) ht))
  (list (hash-table-size ht)
        (hash-table-count ht)
        (hash-table-rehash-size ht)
        (hash-table-rehash-threshold ht)
        (hash-table-test ht)
        (hash-table-weakness ht)))
"##,
        expect,
    );
}

#[test]
fn div_cx185_sxhash_equal_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s1 "hello")
       (s2 (copy-sequence "hello"))
       (v1 [1 2 3])
       (v2 (copy-sequence [1 2 3]))
       (l1 '(1 (2 3) (4 5)))
       (l2 (copy-tree '(1 (2 3) (4 5)))))
  (list (= (sxhash-equal s1) (sxhash-equal s2))
        (= (sxhash-equal v1) (sxhash-equal v2))
        (= (sxhash-equal l1) (sxhash-equal l2))
        (integerp (sxhash-eq 'sym))
        (integerp (sxhash-eql 1.5))
        (= (sxhash-eql 1) (sxhash-eql 1.0))))
"##,
        expect,
    );
}

#[test]
fn div_cx185_hash_table_clear_via_clrhash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (10 0 nil 24)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (dotimes (i 10) (puthash i (* i i) ht))
  (let ((before (hash-table-count ht)))
    (clrhash ht)
    (list before (hash-table-count ht)
          (gethash 5 ht)
          (hash-table-size ht))))
"##,
        expect,
    );
}

#[test]
fn div_cx185_hash_table_copy_via_copy_tree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 3 nil 1 1 nil 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash :a 1 ht)
  (puthash :b 2 ht)
  (let ((ht2 (copy-hash-table ht)))
    (puthash :c 3 ht2)
    (list (hash-table-count ht)
          (hash-table-count ht2)
          (eq ht ht2)
          (gethash :a ht)
          (gethash :a ht2)
          (gethash :c ht)
          (gethash :c ht2))))
"##,
        expect,
    );
}

#[test]
fn div_cx185_obarray_intern_after_unintern() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function make-obarray)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ob (make-obarray 17)))
  (intern "alpha" ob)
  (intern "beta" ob)
  (intern "gamma" ob)
  (let ((before (hash-table-count ob)))
    (unintern "beta" ob)
    (let ((after-unintern (hash-table-count ob)))
      (intern "delta" ob)
      (list before
            after-unintern
            (hash-table-count ob)
            (intern-soft "alpha" ob)
            (intern-soft "beta" ob)
            (intern-soft "delta" ob)))))
"##,
        expect,
    );
}

#[test]
fn div_cx185_hash_table_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (dotimes (i 5) (puthash (cons i :key) (* i i) ht))
  (let ((rec (record 'neo-cx185-mega :a :b :c)))
    (with-temp-buffer
      (buffer-enable-undo)
      (insert "0123456789ABCDEF")
      (put-text-property 1 6 'face 'bold)
      (let ((m (set-marker (make-marker) 8))
            (ov (make-overlay 4 12)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 3 14)
        (remhash (cons 2 :key) ht)
        (aset rec 2 (hash-table-count ht))
        (let ((state (list (hash-table-count ht)
                           (aref rec 0) (aref rec 1) (aref rec 2) (aref rec 3)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (buffer-string)
                           (text-properties-at 1))))
          (undo)
          (widen)
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))))
"##,
        expect,
    );
}
