//! Complex combo batch 186 — `cl-loop` / `cl-do` / `cl-destructuring-bind`
//! / `cl-multiple-value-bind` edge cases with deep nesting.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx186_cl_loop_for_hash_collect_sum_average() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash "a" 10 ht)
  (puthash "b" 20 ht)
  (puthash "c" 30 ht)
  (puthash "d" 40 ht)
  (let* ((vals (cl-loop for v being the hash-values of ht collect v))
         (sum (cl-loop for v being the hash-values of ht sum v))
         (avg (/ sum (length vals))))
    (list (sort vals #'<) sum avg)))
"##,
        expect,
    );
}

#[test]
fn div_cx186_cl_loop_nested_with_for_in_and_across() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lists '((1 2 3) (4 5) (6 7 8 9)))
      (vec [10 20 30]))
  (list
   (cl-loop for lst in lists
            append (cl-loop for x in lst collect (* x 2)))
   (cl-loop for v across vec
            for i from 0
            collect (cons i v))
   (cl-loop for lst in lists
            for i from 0
            sum (length lst) into total
            collect i into indices
            finally (return (list :total total :indices indices)))))
"##,
        expect,
    );
}

#[test]
fn div_cx186_cl_destructuring_bind_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-destructuring-bind)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (cl-destructuring-bind ((a b) (c d)) '((1 2) (3 4))
   (list a b c d))
 (cl-destructuring-bind (&whole whole (a . b) c) '((1 . 2) 3)
   (list whole a b c))
 (cl-destructuring-bind (a (b (c (d)))) '(1 (2 (3 (4))))
   (list a b c d)))
"##,
        expect,
    );
}

#[test]
fn div_cx186_cl_multiple_value_bind_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-multiple-value-bind)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (cl-multiple-value-bind (a b c) (cl-values 1 2 3) (list a b c))
 (cl-multiple-value-bind (a b c) (cl-values 1 2) (list a b c))
 (cl-multiple-value-bind (a b) (cl-values 1 2 3 4 5) (list a b))
 (cl-multiple-value-bind () (cl-values) :no-bindings)
 (cl-multiple-value-bind (a) (cl-values :single) a))
"##,
        expect,
    );
}

#[test]
fn div_cx186_cl_loop_with_complex_conditionals() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((data '((1 . "a") (2 . "b") (3 . "c") (4 . "d") (5 . "e"))))
  (cl-loop for (k . v) in data
           if (and (> k 1) (cl-evenp k)) collect (cons k v) into evens
           else if (> k 3) collect (cons k v) into big-odds
           else collect (cons k v) into small-odds
           finally (return (list :evens evens
                                 :big-odds big-odds
                                 :small-odds small-odds))))
"##,
        expect,
    );
}

#[test]
fn div_cx186_cl_loop_minimize_maximize_with_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((strings '("apple" "kiwi" "banana" "fig" "cherry")))
  (cl-loop for s in strings
           minimize (length s) into min-len
           maximize (length s) into max-len
           finally (return (list :min min-len :max max-len))))
"##,
        expect,
    );
}

#[test]
fn div_cx186_cl_loop_with_initially_and_finally_side_effects() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (trace)
  (let ((result
         (cl-loop initially (push :start trace)
                  for i from 1 to 3
                  do (push (format "iter-%d" i) trace)
                  collect i into acc
                  finally (push (format "end-%d" (length acc)) trace)
                  (return (nreverse acc)))))
    (list result (nreverse trace))))
"##,
        expect,
    );
}

#[test]
fn div_cx186_cl_do_with_result_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-do)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((result
       (cl-do ((i 0 (1+ i))
               (acc nil (cons (* i i) acc)))
           ((>= i 5) (nreverse acc)))))
  result)
"##,
        expect,
    );
}

#[test]
fn div_cx186_cl_loop_always_never_thereis_short_circuit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (cl-loop for n in '(2 4 6 8 10) always (cl-evenp n))
 (cl-loop for n in '(2 4 5 8) always (cl-evenp n))
 (cl-loop for n in '(1 3 5) never (cl-evenp n))
 (cl-loop for n in '(1 3 4 5) never (cl-evenp n))
 (cl-loop for n in '(1 3 5 6 7) thereis (and (cl-evenp n) n))
 (cl-loop for n in '(1 3 5) thereis (and (cl-evenp n) n)))
"##,
        expect,
    );
}

#[test]
fn div_cx186_cl_loop_sum_with_float_into_var() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-loop for i from 1 to 10
         sum (* i 0.1) into total
         finally (return total))
"##,
        expect,
    );
}

#[test]
fn div_cx186_cl_loop_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((data '((1 . "alpha") (2 . "beta") (3 . "gamma"))))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (cl-loop for (k . v) in data concat (format "%d=%s " k v)))
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((state (list (cl-loop for (k . v) in data sum k)
                         (cl-loop for (k . v) in data collect v)
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
