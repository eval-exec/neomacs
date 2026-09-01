//! Complex combo batch 169 — `cl-loop` extreme variants: hash iteration
//! with destructive mutation, multiple accumulators, finally return.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx169_cl_loop_with_multiple_accumulators() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((nums '(1 2 3 4 5)))
  (cl-loop for n in nums
           sum n into total
           count (cl-evenp n) into even-count
           maximize n into max-val
           finally (return (list :total total
                                 :even-count even-count
                                 :max max-val))))
"##,
        expect,
    );
}

#[test]
fn div_cx169_cl_loop_with_destructuring_hash_iteration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash "alpha" 1 ht)
  (puthash "beta" 2 ht)
  (puthash "gamma" 3 ht)
  (sort (cl-loop for k being the hash-keys of ht using (hash-values v)
                 collect (cons k v))
        (lambda (a b) (string< (car a) (car b)))))
"##,
        expect,
    );
}

#[test]
fn div_cx169_cl_loop_for_with_destructuring_and_multiple_into() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((data '(((1 . "a") (2 . "b"))
              ((3 . "c") (4 . "d")))))
  (cl-loop for sublist in data
           append sublist into all
           for i from 1
           count (> (caar sublist) 1) into big-first
           finally (return (list :all all :count i :big-first big-first))))
"##,
        expect,
    );
}

#[test]
fn div_cx169_cl_loop_with_for_hash_and_collect_with_destructuring() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash "a" 1 ht)
  (puthash "b" 2 ht)
  (puthash "c" 3 ht)
  (cl-loop for k being the hash-keys of ht using (hash-values v)
           when (> v 1)
           collect (cons k v) into filtered
           finally (return filtered)))
"##,
        expect,
    );
}

#[test]
fn div_cx169_cl_loop_with_iterate_through_vector_indices() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((v [10 20 30 40 50]))
  (cl-loop for x across v
           for i from 0
           collect (cons i x)))
"##,
        expect,
    );
}

#[test]
fn div_cx169_cl_loop_with_for_in_string_iteration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "hello"))
  (cl-loop for c in-string s
           for i from 0
           collect (cons i c)))
"##,
        expect,
    );
}

#[test]
fn div_cx169_cl_loop_with_complex_initially_finally() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (trace)
  (prog1
      (cl-loop initially (push :init trace)
               for i from 1 to 3
               do (push (format "iter-%d" i) trace)
               sum i into total
               finally (push (format "final-%d" total) trace)
               (return :done))
    trace))
"##,
        expect,
    );
}

#[test]
fn div_cx169_cl_loop_with_while_until_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((data '(1 2 3 4 5 stop 6 7 8))
      (i 0)
      (acc nil))
  (cl-loop while (< i (length data))
           for x = (nth i data)
           until (eq x 'stop)
           do (push x acc)
           do (cl-incf i)
           finally (return (list :count i :acc (nreverse acc)))))
"##,
        expect,
    );
}

#[test]
fn div_cx169_cl_loop_maximize_minimize_with_into_and_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((nums '(5 3 8 1 9 2 7 4 6 0 -1 -5 100)))
  (cl-loop for n in nums
           if (cl-evenp n) maximize n into max-even
           else minimize n into min-odd
           finally (return (list :max-even max-even
                                 :min-odd min-odd))))
"##,
        expect,
    );
}

#[test]
fn div_cx169_cl_loop_with_complex_when_unless_branching() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((items '((1 . "a") (2 . "b") (3 . "c") (4 . "d"))))
  (cl-loop for (k . v) in items
           when (cl-evenp k) collect v into even-strs
           unless (cl-evenp k) collect v into odd-strs
           finally (return (list :even even-strs :odd odd-strs))))
"##,
        expect,
    );
}

#[test]
fn div_cx169_cl_loop_with_marker_overlay_undo_narrow_mega() {
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
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((state (list (hash-table-count ht)
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
