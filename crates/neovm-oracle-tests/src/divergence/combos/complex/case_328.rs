//! Complex combo batch 328 — `cl-loop` ultimate: all accumulators (sum/count/
//! maximize/minimize/collect/append/nconc), all iteration variants (for in/
//! across/in-string/being hash), all conditional clauses (if/when/unless),
//! finally/initially, always/never/thereis.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx328_cl_loop_accumulators_full_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((nums '(3 1 4 1 5 9 2 6)))
  (list (cl-loop for n in nums sum n)
        (cl-loop for n in nums count (cl-evenp n))
        (cl-loop for n in nums maximize n)
        (cl-loop for n in nums minimize n)
        (cl-loop for n in nums collect (* n n))
        (cl-loop for n in nums append (list n n))
        (cl-loop for n in nums nconc (list n n))))
"##,
        expect,
    )
}

#[test]
fn div_cx328_cl_loop_iteration_variants_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-loop for i from 1 to 5 collect i)
      (cl-loop for i from 5 downto 1 collect i)
      (cl-loop for i from 0 to 10 by 2 collect i)
      (cl-loop for i below 5 collect i)
      (cl-loop for i in '(a b c) collect i)
      (cl-loop for i on '(a b c) collect i)
      (cl-loop for i in-string "abc" collect i)
      (cl-loop for i across [10 20 30] collect i))
"##,
        expect,
    )
}

#[test]
fn div_cx328_cl_loop_conditional_clauses_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((nums '(1 2 3 4 5 6)))
  (list (cl-loop for n in nums if (cl-evenp n) collect n)
        (cl-loop for n in nums when (> n 3) collect n)
        (cl-loop for n in nums unless (cl-evenp n) collect n)
        (cl-loop for n in nums
                 if (cl-evenp n) collect n into evens
                 else collect n into odds
                 finally (return (list :evens evens :odds odds)))))
"##,
        expect,
    )
}

#[test]
fn div_cx328_cl_loop_finally_initially_side_effects() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (trace)
  (prog1
      (cl-loop initially (push :init trace)
               for i from 1 to 3
               do (push i trace)
               sum i into total
               finally (push (format "total=%d" total) trace)
               (return :done))
    trace))
"##,
        expect,
    )
}

#[test]
fn div_cx328_cl_loop_always_never_thereis() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-loop for n in '(2 4 6 8) always (cl-evenp n))
      (cl-loop for n in '(2 4 5 8) always (cl-evenp n))
      (cl-loop for n in '(1 3 5) never (cl-evenp n))
      (cl-loop for n in '(1 3 4 5) never (cl-evenp n))
      (cl-loop for n in '(1 3 5 6 7) thereis (and (cl-evenp n) n))
      (cl-loop for n in '(1 3 5) thereis (and (cl-evenp n) n)))
"##,
        expect,
    )
}

#[test]
fn div_cx328_cl_loop_hash_iteration_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash "alpha" 1 ht)
  (puthash "beta" 2 ht)
  (puthash "gamma" 3 ht)
  (list (sort (cl-loop for k being the hash-keys of ht collect k) #'string<)
        (cl-loop for v being the hash-values of ht sum v)
        (sort (cl-loop for k being the hash-keys of ht using (hash-values v)
                       collect (cons k v))
              (lambda (a b) (string< (car a) (car b))))))
"##,
        expect,
    )
}

#[test]
fn div_cx328_cl_loop_while_until_termination() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-loop for i from 0 while (< i 5) collect i)
      (cl-loop for i from 0 until (>= i 5) collect i)
      (cl-loop for i in '(1 2 3 4 stop 5 6)
               until (eq i 'stop) collect i)
      (cl-loop for i from 0 while (< i 100)
               when (= i 5) return :stopped
               finally (return :done)))
"##,
        expect,
    )
}

#[test]
fn div_cx328_cl_loop_destructuring_with_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((data '(((1 . "a") (2 . "b")) ((3 . "c")) ((4 . "d") (5 . "e")))))
  (list (cl-loop for sublist in data
                append (cl-loop for (k . v) in sublist collect (list k v)))
        (cl-loop for sublist in data
                 for i from 1
                 sum (length sublist) into total
                 collect i into indices
                 finally (return (list :total total :indices indices)))))
"##,
        expect,
    )
}

#[test]
fn div_cx328_cl_loop_multiple_for_clauses_parallel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-loop for x in '(1 2 3) for y in '(10 20 30) collect (+ x y))
      (cl-loop for x in '(1 2 3) for y = (* x 10) collect y)
      (cl-loop for x in '(1 2 3) for y = (* x 10) then (+ y 100) collect y))
"##,
        expect,
    )
}

#[test]
fn div_cx328_cl_loop_with_marker_overlay_undo_narrow_mega() {
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
        (widen()
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1)))))))
"##,
        expect,
    )
}
