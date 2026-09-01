//! Oracle parity tests for complex functional programming patterns:
//! map/filter/reduce pipelines, zip/unzip, group-by/partition, transducer-like
//! composition, monadic bind (Maybe), and lazy thunk evaluation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Map/filter/reduce pipeline with complex transformations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fp_map_filter_reduce_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Pipeline: take numbers 1..20, keep evens, square them, sum those > 50
    let form = "(let* ((nums (let ((r nil))
                       (dotimes (i 20) (setq r (cons (1+ i) r)))
                       (nreverse r)))
                ;; Filter evens
                (evens (let ((r nil))
                         (dolist (x nums) (when (= 0 (mod x 2)) (setq r (cons x r))))
                         (nreverse r)))
                ;; Square each
                (squared (mapcar (lambda (x) (* x x)) evens))
                ;; Filter > 50
                (big (let ((r nil))
                       (dolist (x squared) (when (> x 50) (setq r (cons x r))))
                       (nreverse r)))
                ;; Sum
                (total (let ((s 0)) (dolist (x big) (setq s (+ s x))) s)))
           (list evens squared big total))";
    let expect = expect_test::expect![[
        r#""OK ((2 4 6 8 10 12 14 16 18 20) (4 16 36 64 100 144 196 256 324 400) (64 100 144 196 256 324 400) 1484)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_fp_nested_map_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Nested mapcar: matrix operations
    let form = "(let ((matrix '((1 2 3) (4 5 6) (7 8 9))))
                  (let* (;; Double each element
                         (doubled (mapcar (lambda (row)
                                           (mapcar (lambda (x) (* x 2)) row))
                                         matrix))
                         ;; Row sums
                         (row-sums (mapcar (lambda (row)
                                            (let ((s 0))
                                              (dolist (x row) (setq s (+ s x)))
                                              s))
                                          doubled))
                         ;; Column sums (transpose then sum rows)
                         (transposed
                          (let ((ncols (length (car matrix)))
                                (result nil))
                            (dotimes (j ncols)
                              (let ((col nil))
                                (dolist (row doubled)
                                  (setq col (cons (nth j row) col)))
                                (setq result (cons (nreverse col) result))))
                            (nreverse result)))
                         (col-sums (mapcar (lambda (col)
                                            (let ((s 0))
                                              (dolist (x col) (setq s (+ s x)))
                                              s))
                                          transposed)))
                    (list doubled row-sums col-sums)))";
    let expect =
        expect_test::expect![[r#""OK (((2 4 6) (8 10 12) (14 16 18)) (12 30 48) (24 30 36))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Zip and unzip operations on lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fp_zip_unzip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((zip (lambda (a b)
                    (let ((result nil))
                      (while (and a b)
                        (setq result (cons (cons (car a) (car b)) result))
                        (setq a (cdr a))
                        (setq b (cdr b)))
                      (nreverse result))))
                  (zip3 (lambda (a b c)
                    (let ((result nil))
                      (while (and a b c)
                        (setq result (cons (list (car a) (car b) (car c)) result))
                        (setq a (cdr a))
                        (setq b (cdr b))
                        (setq c (cdr c)))
                      (nreverse result))))
                  (unzip (lambda (pairs)
                    (let ((lefts nil) (rights nil))
                      (dolist (p pairs)
                        (setq lefts (cons (car p) lefts))
                        (setq rights (cons (cdr p) rights)))
                      (list (nreverse lefts) (nreverse rights))))))
              (let* ((names '(alice bob carol dave))
                     (ages '(30 25 35 28))
                     (scores '(85 92 78 95))
                     (zipped (funcall zip names ages))
                     (zipped3 (funcall zip3 names ages scores))
                     (unzipped (funcall unzip zipped)))
                (list zipped zipped3 unzipped
                      ;; Roundtrip: unzip(zip(a,b)) == (a, b)
                      (equal (car unzipped) names)
                      (equal (cadr unzipped) ages))))";
    let expect = expect_test::expect![[
        r#""OK (((alice . 30) (bob . 25) (carol . 35) (dave . 28)) ((alice 30 85) (bob 25 92) (carol 35 78) (dave 28 95)) ((alice bob carol dave) (30 25 35 28)) t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Group-by and partition operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fp_group_by_partition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((partition-n
                       (lambda (pred lst)
                         ;; Returns (satisfying . not-satisfying)
                         (let ((yes nil) (no nil))
                           (dolist (x lst)
                             (if (funcall pred x)
                                 (setq yes (cons x yes))
                               (setq no (cons x no))))
                           (cons (nreverse yes) (nreverse no)))))
                      (group-by
                       (lambda (key-fn lst)
                         ;; Group into alist sorted by key
                         (let ((table (make-hash-table :test 'equal)))
                           (dolist (x lst)
                             (let ((k (funcall key-fn x)))
                               (puthash k (cons x (gethash k table nil)) table)))
                           (let ((result nil))
                             (maphash (lambda (k v)
                                        (setq result (cons (cons k (nreverse v)) result)))
                                      table)
                             (sort result (lambda (a b) (< (car a) (car b)))))))))
                  (let ((nums '(1 2 3 4 5 6 7 8 9 10 11 12)))
                    (list
                      ;; Partition: evens vs odds
                      (funcall partition-n #'evenp nums)
                      ;; Partition: > 6 vs <= 6
                      (funcall partition-n (lambda (x) (> x 6)) nums)
                      ;; Group by mod 4
                      (funcall group-by (lambda (x) (mod x 4)) nums)
                      ;; Group by magnitude: small(1-4) medium(5-8) large(9-12)
                      (funcall group-by
                               (lambda (x) (cond ((< x 5) 1) ((< x 9) 2) (t 3)))
                               nums))))";
    let expect = expect_test::expect![[
        r#""OK (((2 4 6 8 10 12) 1 3 5 7 9 11) ((7 8 9 10 11 12) 1 2 3 4 5 6) ((0 4 8 12) (1 1 5 9) (2 2 6 10) (3 3 7 11)) ((1 1 2 3 4) (2 5 6 7 8) (3 9 10 11 12)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Transducer-like composition (map+filter without intermediate lists)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fp_transducer_compose() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build composed transformers that produce a single reducing function
    let form = "(let ((mapping (lambda (f)
                    ;; Returns a transducer: (rf -> rf')
                    (lambda (rf)
                      (lambda (acc x) (funcall rf acc (funcall f x))))))
                  (filtering (lambda (pred)
                    (lambda (rf)
                      (lambda (acc x)
                        (if (funcall pred x)
                            (funcall rf acc x)
                          acc)))))
                  (compose-xf (lambda (xf1 xf2)
                    ;; compose transducers: xf1 applied first
                    (lambda (rf) (funcall xf1 (funcall xf2 rf)))))
                  (transduce (lambda (xf rf init lst)
                    (let ((xrf (funcall xf rf))
                          (acc init))
                      (dolist (x lst)
                        (setq acc (funcall xrf acc x)))
                      acc))))
              (let* (;; Transform: square numbers, keep those > 10, sum them
                     (xf-square (funcall mapping (lambda (x) (* x x))))
                     (xf-big (funcall filtering (lambda (x) (> x 10))))
                     (xf-combined (funcall compose-xf xf-square xf-big))
                     (result1 (funcall transduce xf-combined #'+ 0
                                       '(1 2 3 4 5 6)))
                     ;; Transform: add 10, keep evens, collect into list
                     (xf-add10 (funcall mapping (lambda (x) (+ x 10))))
                     (xf-even (funcall filtering #'evenp))
                     (xf-combined2 (funcall compose-xf xf-add10 xf-even))
                     (result2 (funcall transduce xf-combined2
                                       (lambda (acc x) (cons x acc))
                                       nil '(1 2 3 4 5 6 7 8))))
                (list result1 (nreverse result2))))";
    let expect = expect_test::expect![[r#""OK (77 (12 14 16 18))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Monadic bind pattern (Maybe monad with nil as Nothing)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fp_maybe_monad() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Maybe monad: nil = Nothing, non-nil = Just(value)
    let form = "(let ((maybe-return (lambda (x) x))
                      (maybe-bind (lambda (mv f)
                        ;; If mv is nil (Nothing), short-circuit
                        (if (null mv) nil (funcall f mv))))
                      (safe-div (lambda (a b)
                        (if (= b 0) nil (/ a b))))
                      (safe-sqrt (lambda (x)
                        (if (< x 0) nil (sqrt (float x)))))
                      (safe-head (lambda (lst)
                        (if (null lst) nil (car lst)))))
                  (let ((chain (lambda (mv &rest fns)
                        (let ((result mv))
                          (dolist (f fns)
                            (setq result (funcall maybe-bind result f)))
                          result))))
                    (list
                      ;; Successful chain: 100 / 4 -> sqrt -> 5.0
                      (funcall chain 100
                               (lambda (x) (funcall safe-div x 4))
                               (lambda (x) (funcall safe-sqrt x)))
                      ;; Fails at division by zero
                      (funcall chain 100
                               (lambda (x) (funcall safe-div x 0))
                               (lambda (x) (funcall safe-sqrt x)))
                      ;; Fails at sqrt of negative
                      (funcall chain -16
                               (lambda (x) (funcall safe-sqrt x)))
                      ;; Nested safe-head on list of lists
                      (funcall chain '((1 2) (3 4))
                               safe-head
                               safe-head)
                      ;; Safe-head on empty list -> nil
                      (funcall chain nil
                               safe-head)
                      ;; Complex: lookup in alist, safe-divide result
                      (funcall chain '((a . 100) (b . 0) (c . 25))
                               (lambda (alist) (cdr (assq 'a alist)))
                               (lambda (x) (funcall safe-div x 5))))))";
    let expect = expect_test::expect![[r#""OK (5.0 nil nil 1 nil 20)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Lazy evaluation simulation with thunks
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fp_lazy_thunks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate lazy sequences using thunks (lambda () ...)
    let form = "(let ((lazy-cons (lambda (head tail-thunk)
                    (cons head tail-thunk)))
                  (lazy-head (lambda (s) (car s)))
                  (lazy-tail (lambda (s) (funcall (cdr s))))
                  (lazy-take (lambda (n s)
                    (let ((result nil) (current s))
                      (dotimes (_ n)
                        (when current
                          (setq result (cons (car current) result))
                          (setq current (funcall (cdr current)))))
                      (nreverse result)))))
              ;; Build infinite sequence of natural numbers
              (progn
                (fset 'neovm--test-naturals-from
                  (lambda (n)
                    (funcall lazy-cons n
                             (lambda () (funcall 'neovm--test-naturals-from (1+ n))))))
                ;; Build fibonacci lazy sequence
                (fset 'neovm--test-fibs-from
                  (lambda (a b)
                    (funcall lazy-cons a
                             (lambda () (funcall 'neovm--test-fibs-from b (+ a b))))))
                (unwind-protect
                    (let ((nats (funcall 'neovm--test-naturals-from 1))
                          (fibs (funcall 'neovm--test-fibs-from 0 1)))
                      (list
                        ;; First 10 naturals
                        (funcall lazy-take 10 nats)
                        ;; First 12 fibonacci numbers
                        (funcall lazy-take 12 fibs)
                        ;; Lazy map: square first 8 naturals
                        (let ((result nil)
                              (current nats))
                          (dotimes (_ 8)
                            (setq result (cons (* (car current) (car current)) result))
                            (setq current (funcall (cdr current))))
                          (nreverse result))))
                  (fmakunbound 'neovm--test-naturals-from)
                  (fmakunbound 'neovm--test-fibs-from))))";
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7 8 9 10) (0 1 1 2 3 5 8 13 21 34 55 89) (1 4 9 16 25 36 49 64))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_fp_lazy_filtered_stream() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Lazy filter: build a stream of primes via sieve
    let form = "(progn
  (fset 'neovm--test-ints-from
    (lambda (n) (cons n (lambda () (funcall 'neovm--test-ints-from (1+ n))))))

  (fset 'neovm--test-sieve
    (lambda (s)
      (let ((p (car s)))
        (cons p
              (lambda ()
                (funcall 'neovm--test-sieve
                         (funcall 'neovm--test-stream-filter
                                  (lambda (x) (/= 0 (mod x p)))
                                  (funcall (cdr s)))))))))

  (fset 'neovm--test-stream-filter
    (lambda (pred s)
      (if (null s) nil
        (if (funcall pred (car s))
            (cons (car s)
                  (lambda ()
                    (funcall 'neovm--test-stream-filter pred (funcall (cdr s)))))
          (funcall 'neovm--test-stream-filter pred (funcall (cdr s)))))))

  (fset 'neovm--test-stream-take
    (lambda (n s)
      (let ((result nil) (current s))
        (dotimes (_ n)
          (when current
            (setq result (cons (car current) result))
            (setq current (funcall (cdr current)))))
        (nreverse result))))

  (unwind-protect
      (let ((primes (funcall 'neovm--test-sieve
                              (funcall 'neovm--test-ints-from 2))))
        (funcall 'neovm--test-stream-take 15 primes))
    (fmakunbound 'neovm--test-ints-from)
    (fmakunbound 'neovm--test-sieve)
    (fmakunbound 'neovm--test-stream-filter)
    (fmakunbound 'neovm--test-stream-take)))";
    let expect = expect_test::expect![[r#""OK (2 3 5 7 11 13 17 19 23 29 31 37 41 43 47)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: church encoding of natural numbers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fp_church_numerals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Church numerals: numbers as higher-order functions
    let form = "(let ((church-zero (lambda (f) (lambda (x) x)))
                      (church-succ (lambda (n)
                        (lambda (f) (lambda (x) (funcall f (funcall (funcall n f) x))))))
                      (church-plus (lambda (m n)
                        (lambda (f) (lambda (x) (funcall (funcall m f) (funcall (funcall n f) x))))))
                      (church-mult (lambda (m n)
                        (lambda (f) (funcall m (funcall n f)))))
                      (church-to-int (lambda (n)
                        (funcall (funcall n (lambda (x) (1+ x))) 0))))
              (let* ((c0 church-zero)
                     (c1 (funcall church-succ c0))
                     (c2 (funcall church-succ c1))
                     (c3 (funcall church-succ c2))
                     (c5 (funcall church-plus c2 c3))
                     (c6 (funcall church-mult c2 c3)))
                (list
                  (funcall church-to-int c0)
                  (funcall church-to-int c1)
                  (funcall church-to-int c2)
                  (funcall church-to-int c3)
                  (funcall church-to-int c5)
                  (funcall church-to-int c6))))";
    let expect = expect_test::expect![[r#""OK (0 1 2 3 5 6)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(0 1 2 3 5 6)", &o, &n);
}
