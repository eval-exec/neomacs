//! Complex combo batch 67 — hash tables (eq/eql/equal/custom tests, all
//! weakness kinds), records, char-tables (subtypes, ranges, extra-slots),
//! bool-vectors, and `sxhash` collisions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx67_hash_table_eq_vs_eql_vs_equal_with_floats_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil :v nil :v :v)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((str1 "hello")
       (str2 (copy-sequence "hello"))
       (n1 1.0)
       (n2 1.0)
       (sym1 'k)
       (sym2 'k))
  (list
   (let ((ht (make-hash-table :test 'eq)))    (puthash str1 :v ht) (gethash str2 ht))
   (let ((ht (make-hash-table :test 'eql)))   (puthash str1 :v ht) (gethash str2 ht))
   (let ((ht (make-hash-table :test 'equal))) (puthash str1 :v ht) (gethash str2 ht))
   (let ((ht (make-hash-table :test 'eq)))    (puthash n1 :v ht) (gethash n2 ht))
   (let ((ht (make-hash-table :test 'eql)))   (puthash n1 :v ht) (gethash n2 ht))
   (let ((ht (make-hash-table :test 'eq)))    (puthash sym1 :v ht) (gethash sym2 ht))))
"##,
        expect,
    );
}

#[test]
fn div_cx67_hash_table_custom_test_function() {
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
    );
}

#[test]
fn div_cx67_hash_table_weakness_kinds_after_gc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((key 3 0) (value 3 0) (key-and-value 3 0) (key-or-value 3 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (results)
  (dolist (w '(key value key-and-value key-or-value))
    (let ((ht (make-hash-table :weakness w :test 'eq)))
      (dotimes (i 3) (puthash (cons i nil) (cons (* i 10) nil) ht))
      (let ((before (hash-table-count ht)))
        (garbage-collect)
        (push (list w before (hash-table-count ht)) results))))
  (nreverse results))
"##,
        expect,
    );
}

#[test]
fn div_cx67_hash_table_maphash_remhash_iteration_order_independence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (8 ((0 . 0) (1 . 1) (2 . 4) (4 . 16) (5 . 25) (6 . 36) (8 . 64) (9 . 81)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal))
      (collected nil)
      (count-after-remhash 0))
  (dotimes (i 10) (puthash i (* i i) ht))
  (remhash 3 ht)
  (remhash 7 ht)
  (maphash (lambda (k v) (push (cons k v) collected)) ht)
  (setq count-after-remhash (hash-table-count ht))
  (list count-after-remhash
        (sort collected (lambda (a b) (< (car a) (car b))))))
"##,
        expect,
    );
}

#[test]
fn div_cx67_record_type_create_access_and_setf() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function record-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((r1 (record 'neo-cx67-tag 1 2 3))
      (r2 (record 'neo-cx67-tag :a :b)))
  (list (record-p r1)
        (record-type r1)
        (aref r1 0) (aref r1 1) (aref r1 2) (aref r1 3)
        (length r1)
        (setf (aref r1 2) 99)
        (aref r1 2)
        (record-length r2)))
"##,
        expect,
    );
}

#[test]
fn div_cx67_char_table_subtype_range_extra_slots() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (args-out-of-range #^[:default nil neo-cx67-subtype :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default :default] 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-char-table 'neo-cx67-subtype :default)))
  (set-char-table-extra-slot ct 0 :extra0)
  (set-char-table-extra-slot ct 1 :extra1)
  (aset ct ?A :letter)
  (aset ct ?9 :digit)
  (set-char-table-range ct '(?a . ?z) :lowercase)
  (list (char-table-subtype ct)
        (aref ct ?A) (aref ct ?a) (aref ct ?z) (aref ct ?9) (aref ct ?!)
        (char-table-extra-slot ct 0)
        (char-table-extra-slot ct 1)
        (char-table-range ct '(?a . ?z))
        (char-table-range ct nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx67_bool_vector_bit_ops_and_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function bool-vector-intersection-and)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((bv1 (make-bool-vector 16 nil))
      (bv2 (make-bool-vector 16 t)))
  (dotimes (i 16) (aset bv1 i (= (% i 2) 0)))
  (let ((and-v (bool-vector-intersection-and bv1 bv2 nil))
        (or-v (bool-vector-union bv1 bv2 nil))
        (xor-v (bool-vector-xor bv1 bv2 nil))
        (not-v (bool-vector-not bv1 nil))
        (subset (bool-vector-subsetp bv1 bv2)))
    (list (bool-vector-count-consecutive bv1 t 0)
          (bool-vector-count-population bv1)
          (bool-vector-p bv1)
          subset
          (length bv1))))
"##,
        expect,
    );
}

#[test]
fn div_cx67_sxhash_equal_p_equal_hash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 13 46)""#]];
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
        (sxhash-eq 'sym)
        (sxhash-eql 1.5)
        (= (sxhash-eql 1) (sxhash-eql 1.0)))))
"##,
        expect,
    );
}

#[test]
fn div_cx67_hash_table_size_resize_threshold() {
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
fn div_cx67_obarray_intern_after_unintern_reuse_slot() {
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
    (let ((after (hash-table-count ob)))
      (intern "delta" ob)
      (list before
            after
            (hash-table-count ob)
            (intern-soft "alpha" ob)
            (intern-soft "beta" ob)
            (intern-soft "delta" ob)))))
"##,
        expect,
    );
}

#[test]
fn div_cx67_char_table_map_optimized_with_overrides() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:digit . 1) (:word . 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-char-table 'syntax-table nil)))
  (set-char-table-range ct '(?a . ?z) :word)
  (set-char-table-range ct '(?A . ?Z) :word)
  (set-char-table-range ct '(?0 . ?9) :digit)
  (let (counts)
    (map-char-table
     (lambda (key val)
       (when val
         (let ((entry (assq val counts)))
           (if entry (setcdr entry (1+ (cdr entry)))
             (push (cons val 1) counts)))))
     ct)
    (sort counts (lambda (a b) (string< (symbol-name (car a)) (symbol-name (car b)))))))
"##,
        expect,
    );
}

#[test]
fn div_cx67_hash_table_record_marker_overlay_undo_textprop_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (dotimes (i 5) (puthash (cons i :key) (* i i) ht))
  (let ((rec (record 'neo-cx67-mega :a :b :c)))
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
          (list state
                (hash-table-count ht)
                (buffer-string) (marker-position m)
                (overlayp ov) (overlay-start ov)
                (text-properties-at 1)))))))
"##,
        expect,
    );
}
