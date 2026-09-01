//! Complex combo batch 196 — `cl-loop` final extreme: `for` being the
//! `elements` of various sequence types, `across` vectors, `in-string`,
//! hash iteration, and `with` pre-declarations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx196_cl_loop_for_being_elements_of_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (cl-loop for x being the elements of [1 2 3 4] collect x)
 (cl-loop for x being the elements of "abcd" collect x)
 (cl-loop for x being the elements of '(a b c d) collect x))
"##,
        expect,
    );
}

#[test]
fn div_cx196_cl_loop_with_pre_declaration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (cl-loop with a = 10
          with b = 20
          for i from 1 to 3
          collect (+ a b i))
 (cl-loop with (a b) = '(1 2)
          for i from 1 to 3
          collect (list a b i)))
"##,
        expect,
    );
}

#[test]
fn div_cx196_cl_loop_across_vector_and_in_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((v [10 20 30 40 50])
      (s "hello"))
  (list
   (cl-loop for x across v collect (* x x))
   (cl-loop for x across v for i from 0 collect (cons i x))
   (cl-loop for c in-string s collect c)
   (cl-loop for c in-string s for i from 0 collect (cons i c))))
"##,
        expect,
    );
}

#[test]
fn div_cx196_cl_loop_hash_keys_values_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash "alpha" 1 ht)
  (puthash "beta" 2 ht)
  (puthash "gamma" 3 ht)
  (list
   (sort (cl-loop for k being the hash-keys of ht collect k) #'string<)
   (cl-loop for v being the hash-values of ht sum v)
   (sort (cl-loop for k being the hash-keys of ht using (hash-values v)
                  collect (cons k v))
         (lambda (a b) (string< (car a) (car b))))))
"##,
        expect,
    );
}

#[test]
fn div_cx196_cl_loop_complex_sum_into_with_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-loop for n in '(1 2 3 4 5 6 7 8 9 10)
         if (cl-evenp n) sum n into even-sum
         else sum n into odd-sum
         count (cl-evenp n) into even-count
         finally (return (list :even-sum even-sum
                               :odd-sum odd-sum
                               :even-count even-count)))
"##,
        expect,
    );
}

#[test]
fn div_cx196_cl_loop_return_from_inside_body() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (cl-loop for n in '(1 2 3 4 5)
          when (> n 3) return n
          finally (return :not-found))
 (cl-loop for n in '(1 2 3 4 5)
          when (> n 99) return n
          finally (return :not-found))
 (cl-loop for n in '(1 2 3 4 5)
          thereis (and (> n 3) n)))
"##,
        expect,
    );
}

#[test]
fn div_cx196_cl_loop_destructuring_with_complex_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((data '(((1 . "a") (2 . "b")) ((3 . "c")) ((4 . "d") (5 . "e")))))
  (list
   (cl-loop for sublist in data
            append (cl-loop for (k . v) in sublist collect (list k v)))
   (cl-loop for sublist in data
            for i from 1
            sum (length sublist) into total
            collect i into indices
            finally (return (list :total total :indices indices)))))
"##,
        expect,
    );
}

#[test]
fn div_cx196_cl_loop_always_never_thereis_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((nums '(2 4 6 8 10))
      (mixed '(2 4 5 6)))
  (list
   (cl-loop for n in nums always (cl-evenp n))
   (cl-loop for n in mixed always (cl-evenp n))
   (cl-loop for n in nums never (cl-oddp n))
   (cl-loop for n in mixed never (cl-oddp n))
   (cl-loop for n in mixed thereis n)
   (cl-loop for n in nums thereis (and (cl-oddp n) n))))
"##,
        expect,
    );
}

#[test]
fn div_cx196_cl_loop_append_nconc_into_accumulator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lists '((1 2) (3 4) (5 6))))
  (list
   (cl-loop for l in lists append l)
   (cl-loop for l in lists nconc l)
   (cl-loop for l in lists append l into all finally (return all))))
"##,
        expect,
    );
}

#[test]
fn div_cx196_cl_loop_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash "alpha" 1 ht)
  (puthash "beta" 2 ht)
  (puthash "gamma" 3 ht)
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (cl-loop for k being the hash-keys of ht using (hash-values v)
                     concat (format "%s=%d\n" k v)))
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 10))
          (ov (make-overlay 4 18)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 22)
      (let ((state (list (hash-table-count ht)
                         (cl-loop for v being the hash-values of ht sum v)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    );
}
