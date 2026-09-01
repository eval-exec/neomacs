//! Complex combo batch 376 — `map` library ultimate: map-merge/merge-with,
//! map-filter, map-keys/values/length/do/elt/put!/contains-p across
//! hash-table/alist/plist.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx376_map_keys_values_length_across_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function map-keys)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash "alpha" 1 ht)
  (puthash "beta" 2 ht)
  (puthash "gamma" 3 ht)
  (list (sort (map-keys ht) #'string<)
        (sort (map-values ht) #'<)
        (map-length ht)
        (map-keys '((a . 1) (b . 2)))
        (map-values '((a . 1) (b . 2)))
        (map-length '((a . 1) (b . 2)))
        (map-keys '(:a 1 :b 2))
        (map-values '(:a 1 :b 2))
        (map-length '(:a 1 :b 2))))
"##,
        expect,
    )
}

#[test]
fn div_cx376_map_merge_with_collision_fn() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function map-merge)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht1 (make-hash-table :test 'equal))
      (ht2 (make-hash-table :test 'equal)))
  (puthash "a" 1 ht1) (puthash "b" 2 ht1)
  (puthash "c" 3 ht2) (puthash "d" 4 ht2)
  (let ((merged (map-merge ht1 ht2)))
    (list (map-length merged)
          (map-elt merged "a") (map-elt merged "c") (map-elt merged "z" :missing))))
"##,
        expect,
    )
}

#[test]
fn div_cx376_map_merge_with_collision_resolution() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function map-merge-with)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht1 (make-hash-table :test 'equal))
      (ht2 (make-hash-table :test 'equal)))
  (puthash "a" 1 ht1)
  (puthash "a" 10 ht2)
  (let ((merged (map-merge-with #'+ ht1 ht2)))
    (list (map-elt merged "a") (map-length merged))))
"##,
        expect,
    )
}

#[test]
fn div_cx376_map_do_and_apply_across_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function map-do)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal))
      (collected nil))
  (puthash "x" 10 ht)
  (puthash "y" 20 ht)
  (map-do (lambda (k v) (push (cons k v) collected)) ht)
  (let ((ht-collected (sort collected (lambda (a b) (string< (car a) (car b))))))
    (let ((alist '((a . 1) (b . 2))))
      (let ((result (map-apply (lambda (k v) (cons k (* v 10))) alist)))
        (list ht-collected result)))))
"##,
        expect,
    )
}

#[test]
fn div_cx376_map_elt_with_default_and_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function map-elt)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((plist '(:a 1 :b 2 :c (:d 3 :e 4))))
  (list (map-elt plist :a)
        (map-elt plist :b)
        (map-elt plist :c)
        (map-elt plist :missing)
        (map-elt plist :missing :default)))
"##,
        expect,
    )
}

#[test]
fn div_cx376_map_put_set_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((ht (make-hash-table :test 'equal)))
      (map-put! ht "key" :val)
      (list (map-elt ht "key") (map-length ht)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx376_map_contains_p_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function map-contains-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash "alpha" 1 ht)
  (list (map-contains-p ht "alpha")
        (map-contains-p ht "missing")))
"##,
        expect,
    )
}

#[test]
fn div_cx376_map_copy_with_merge_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function map-merge)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p1 '(:a 1 :b 2))
      (p2 '(:c 3)))
  (let ((merged (map-merge p1 p2)))
    (list merged (map-length merged)
          (map-elt merged :a) (map-elt merged :c))))
"##,
        expect,
    )
}

#[test]
fn div_cx376_map_into_different_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function map-into)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((alist '((a . 1) (b . 2))))
  (list (map-into alist 'vector)
        (map-into alist 'hash-table)))
"##,
        expect,
    )
}

#[test]
fn div_cx376_map_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash "alpha" 1 ht)
  (puthash "beta" 2 ht)
  (puthash "gamma" 3 ht)
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (mapconcat #'identity (map-keys ht) ", "))
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 18)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 25)
      (let ((state (list (map-length ht)
                         (sort (map-values ht) #'<)
                         (map-contains-p ht "alpha")
                         (map-elt ht "alpha")
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen()
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    )
}
