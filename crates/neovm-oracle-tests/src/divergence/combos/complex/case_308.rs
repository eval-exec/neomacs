//! Complex combo batch 308 — `hash-table` ultimate deep: `clrhash` /
//! `copy-hash-table` / `hash-table-test` custom / `sxhash-equal`
//! distribution / `maphash` vs `map-keys` consistency.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx308_hash_table_clrhash_preserves_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (20 24 0 24)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (dotimes (i 20) (puthash i (* i i) ht))
  (let ((before-count (hash-table-count ht))
        (before-size (hash-table-size ht)))
    (clrhash ht)
    (list before-count before-size
          (hash-table-count ht)
          (hash-table-size ht))))
"##,
        expect,
    )
}

#[test]
fn div_cx308_hash_table_copy_independence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 3 :missing 3 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash :a 1 ht)
  (puthash :b 2 ht)
  (let ((ht2 (copy-hash-table ht)))
    (puthash :c 3 ht2)
    (list (hash-table-count ht)
          (hash-table-count ht2)
          (gethash :c ht :missing)
          (gethash :c ht2)
          (eq ht ht2))))
"##,
        expect,
    )
}

#[test]
fn div_cx308_hash_table_custom_test_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((ht (make-hash-table :test (lambda (a b) (eq (car-safe a) (car-safe b)))
                                  :hash (lambda (k) (sxhash (car-safe k))))))
      (puthash '(:a) :v1 ht)
      (puthash '(:b) :v2 ht)
      (puthash '(:a 99) :v3 ht)
      (list (hash-table-count ht)
            (gethash '(:a) ht)
            (gethash '(:a 99) ht)
            (gethash '(:b) ht)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx308_hash_table_weakness_all_kinds_gc() {
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
    )
}

#[test]
fn div_cx308_hash_table_maphash_vs_map_keys_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function map-keys)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (dotimes (i 10) (puthash i (* i i) ht))
  (let ((maphash-keys nil)
        (map-keys (sort (map-keys ht) #'<)))
    (maphash (lambda (k v) (push k maphash-keys)) ht)
    (setq maphash-keys (sort maphash-keys #'<))
    (list (equal maphash-keys map-keys)
          (length maphash-keys)
          (length map-keys))))
"##,
        expect,
    )
}

#[test]
fn div_cx308_sxhash_equal_consistency_across_types() {
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
    )
}

#[test]
fn div_cx308_hash_table_resize_threshold_query() {
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
    )
}

#[test]
fn div_cx308_obarray_intern_unintern_reuse() {
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
    )
}

#[test]
fn div_cx308_hash_table_count_after_remhash_many() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (16 nil nil 9 :missing)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (dotimes (i 20) (puthash i (* i i) ht))
  (remhash 0 ht)
  (remhash 5 ht)
  (remhash 10 ht)
  (remhash 15 ht)
  (remhash 99 ht)
  (list (hash-table-count ht)
        (gethash 0 ht)
        (gethash 5 ht)
        (gethash 3 ht)
        (gethash 99 ht :missing)))
"##,
        expect,
    )
}

#[test]
fn div_cx308_hash_table_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (dotimes (i 5) (puthash (cons i :key) (* i i) ht))
  (let ((rec (record 'neo-cx308-mega :a :b :c)))
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
    )
}
