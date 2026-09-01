//! Complex combo batch 84 — cl-loop deep: every accumulator, every
//! iteration clause, conditional clauses, finally/initially, with = vs
//! then, into variables, and aggressive combination.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx84_cl_loop_accumulators_sum_count_min_max() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((nums '(3 1 4 1 5 9 2 6 5 3 5)))
  (list
   (cl-loop for n in nums sum n)
   (cl-loop for n in nums count (cl-oddp n))
   (cl-loop for n in nums minimize n)
   (cl-loop for n in nums maximize n)
   (cl-loop for n in nums sum (* n n) into squares finally (return squares))))
"##,
        expect,
    );
}

#[test]
fn div_cx84_cl_loop_iteration_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (cl-loop for i from 1 to 5 collect i)
 (cl-loop for i from 5 downto 1 collect i)
 (cl-loop for i from 0 to 10 by 2 collect i)
 (cl-loop for i from 10 above 0 by 3 collect i)
 (cl-loop for i below 5 collect i)
 (cl-loop for i upto 5 collect i)
 (cl-loop for i in '(a b c d) collect i)
 (cl-loop for i on '(a b c d) collect i)
 (cl-loop for i in-string "hello" collect i)
 (cl-loop for i across [10 20 30] collect i))
"##,
        expect,
    );
}

#[test]
fn div_cx84_cl_loop_with_multiple_for_clauses() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (cl-loop for x in '(1 2 3)
          for y in '(10 20 30)
          collect (+ x y))
 (cl-loop for x in '(1 2 3)
          for y = (* x 10)
          collect y)
 (cl-loop for x in '(1 2 3)
          for y = (* x 10) then (+ y 100)
          collect y)
 (cl-loop for x in '(1 2 3 4 5)
          for y in '(10 20)
          collect (list x y)))
"##,
        expect,
    );
}

#[test]
fn div_cx84_cl_loop_conditional_clauses_if_when_unless() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((nums '(1 2 3 4 5 6 7 8 9 10)))
  (list
   (cl-loop for n in nums if (cl-evenp n) collect n)
   (cl-loop for n in nums when (cl-evenp n) collect n)
   (cl-loop for n in nums unless (cl-evenp n) collect n)
   (cl-loop for n in nums
            if (cl-evenp n) collect n into evens
            else collect n into odds
            finally (return (list evens odds)))))
"##,
        expect,
    );
}

#[test]
fn div_cx84_cl_loop_finally_initially_with_return() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((trace nil))
  (list
   (cl-loop initially (push :init trace)
            for i from 1 to 3
            do (push i trace)
            finally (push :end trace)
            (return (nreverse trace)))))
"##,
        expect,
    );
}

#[test]
fn div_cx84_cl_loop_with_destructuring_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((alist '((1 . "a") (2 . "b") (3 . "c"))))
  (list
   (cl-loop for (k . v) in alist collect (list k v))
   (cl-loop for (k . v) in alist sum k)
   (cl-loop for (k . v) in alist collect v into vs finally (return vs))
   (cl-loop for (k . v) in alist
            for i from 1
            collect (list i k v))))
"##,
        expect,
    );
}

#[test]
fn div_cx84_cl_loop_while_until_termination() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (cl-loop for i from 0
          while (< i 5)
          collect i)
 (cl-loop for i from 0
          until (>= i 5)
          collect i)
 (cl-loop for i in '(1 2 3 4 stop 5 6)
          until (eq i 'stop)
          collect i)
 (cl-loop for i from 0
          while (< i 100)
          when (= i 5) return :stopped
          finally (return :done)))
"##,
        expect,
    );
}

#[test]
fn div_cx84_cl_loop_always_never_thereis() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (cl-loop for n in '(2 4 6 8) always (cl-evenp n))
 (cl-loop for n in '(2 4 5 8) always (cl-evenp n))
 (cl-loop for n in '(1 3 5) never (cl-evenp n))
 (cl-loop for n in '(1 3 4 5) never (cl-evenp n))
 (cl-loop for n in '(1 3 5 6 7) thereis (cl-evenp n))
 (cl-loop for n in '(1 3 5) thereis (cl-evenp n)))
"##,
        expect,
    );
}

#[test]
fn div_cx84_cl_loop_append_nconc_into_accumulator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lists '((1 2) (3 4) (5 6))))
  (list
   (cl-loop for l in lists append l)
   (cl-loop for l in lists nconc l)
   (cl-loop for l in lists append l into all finally (return all))
   (cl-loop for l in lists
            for i from 1
            nconc (mapcar (lambda (x) (* x i)) l))))
"##,
        expect,
    );
}

#[test]
fn div_cx84_cl_loop_maximize_minimize_with_into() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((nums '(5 3 8 1 9 2 7 4 6)))
  (list
   (cl-loop for n in nums maximize n into m finally (return m))
   (cl-loop for n in nums minimize n into m finally (return m))
   (cl-loop for n in nums
            if (cl-evenp n) maximize n into max-even
            else minimize n into min-odd
            finally (return (list max-even min-odd)))))
"##,
        expect,
    );
}

#[test]
fn div_cx84_cl_loop_hash_iteration_and_destructuring() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash "a" 1 ht)
  (puthash "b" 2 ht)
  (puthash "c" 3 ht)
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
fn div_cx84_cl_loop_do_side_effects_with_acc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (side)
  (let ((result (cl-loop for i from 1 to 5
                         do (push (* i 10) side)
                         collect i)))
    (list result (nreverse side))))
"##,
        expect,
    );
}

#[test]
fn div_cx84_cl_loop_complex_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert (mapconcat #'identity
                     (cl-loop for i from 1 to 5 collect (format "item-%d" i))
                     "\n"))
  (put-text-property 1 6 'face 'bold)
  (let ((m (set-marker (make-marker) 10))
        (ov (make-overlay 3 20)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 2 25)
    (let* ((result
            (cl-loop for line in (split-string (buffer-string) "\n")
                     for i from 1
                     when (> (length line) 0)
                     collect (cons i (length line))))
           (state (list result
                        (marker-position m)
                        (overlay-start ov) (overlay-end ov)
                        (buffer-string)
                        (text-properties-at 1))))
      (undo)
      (widen)
      (list state (buffer-string) (marker-position m)
            (overlay-start ov) (overlay-end ov)
            (text-properties-at 1)))))
"##,
        expect,
    );
}
