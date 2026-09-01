//! Oracle parity tests for iterator pattern implementations in Elisp.
//!
//! Tests range iterators, filter/map/take/drop/chain/zip iterators,
//! and complex lazy evaluation pipelines using composed iterator closures.
//! Each iterator is a closure returning the next value or 'done.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Range iterator: generates (from, from+step, ...) up to (exclusive) to
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_iter_range_iterator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--iter-range
    (lambda (from to step)
      "Return an iterator closure that yields FROM, FROM+STEP, ... < TO."
      (let ((current from))
        (lambda ()
          (if (< current to)
              (let ((val current))
                (setq current (+ current step))
                val)
            'done)))))

  (fset 'neovm--iter-collect
    (lambda (iter)
      "Collect all values from ITER into a list."
      (let ((result nil)
            (val (funcall iter)))
        (while (not (eq val 'done))
          (setq result (cons val result))
          (setq val (funcall iter)))
        (nreverse result))))

  (unwind-protect
      (list
       ;; Basic range 0..10 step 1
       (funcall 'neovm--iter-collect (funcall 'neovm--iter-range 0 10 1))
       ;; Range 0..20 step 3
       (funcall 'neovm--iter-collect (funcall 'neovm--iter-range 0 20 3))
       ;; Range 5..5 step 1 (empty)
       (funcall 'neovm--iter-collect (funcall 'neovm--iter-range 5 5 1))
       ;; Range -10..5 step 4
       (funcall 'neovm--iter-collect (funcall 'neovm--iter-range -10 5 4))
       ;; Range with large step (only one element)
       (funcall 'neovm--iter-collect (funcall 'neovm--iter-range 0 100 200))
       ;; Range 1..1000 step 100
       (funcall 'neovm--iter-collect (funcall 'neovm--iter-range 1 1000 100)))
    (fmakunbound 'neovm--iter-range)
    (fmakunbound 'neovm--iter-collect)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 1 2 3 4 5 6 7 8 9) (0 3 6 9 12 15 18) nil (-10 -6 -2 2) (0) (1 101 201 301 401 501 601 701 801 901))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Filter iterator: wraps another iterator, only yields matching values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_iter_filter_iterator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--iter-range
    (lambda (from to step)
      (let ((current from))
        (lambda ()
          (if (< current to)
              (let ((val current))
                (setq current (+ current step))
                val)
            'done)))))

  (fset 'neovm--iter-filter
    (lambda (pred iter)
      "Return an iterator that only yields values where (PRED val) is non-nil."
      (lambda ()
        (let ((val (funcall iter))
              (found nil))
          (while (and (not (eq val 'done)) (not found))
            (if (funcall pred val)
                (setq found t)
              (setq val (funcall iter))))
          val))))

  (fset 'neovm--iter-collect
    (lambda (iter)
      (let ((result nil) (val (funcall iter)))
        (while (not (eq val 'done))
          (setq result (cons val result))
          (setq val (funcall iter)))
        (nreverse result))))

  (unwind-protect
      (list
       ;; Filter evens from 0..20
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-filter
                         (lambda (x) (= 0 (mod x 2)))
                         (funcall 'neovm--iter-range 0 20 1)))
       ;; Filter multiples of 3 from 1..30
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-filter
                         (lambda (x) (= 0 (mod x 3)))
                         (funcall 'neovm--iter-range 1 30 1)))
       ;; Filter primes from 2..50 (trial division)
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-filter
                         (lambda (n)
                           (and (> n 1)
                                (let ((is-prime t) (d 2))
                                  (while (and is-prime (<= (* d d) n))
                                    (when (= 0 (mod n d))
                                      (setq is-prime nil))
                                    (setq d (1+ d)))
                                  is-prime)))
                         (funcall 'neovm--iter-range 2 50 1)))
       ;; Filter from empty range
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-filter
                         (lambda (x) t)
                         (funcall 'neovm--iter-range 5 5 1)))
       ;; Filter that rejects everything
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-filter
                         (lambda (x) nil)
                         (funcall 'neovm--iter-range 0 10 1))))
    (fmakunbound 'neovm--iter-range)
    (fmakunbound 'neovm--iter-filter)
    (fmakunbound 'neovm--iter-collect)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 2 4 6 8 10 12 14 16 18) (3 6 9 12 15 18 21 24 27) (2 3 5 7 11 13 17 19 23 29 31 37 41 43 47) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Map iterator: transforms yielded values through a function
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_iter_map_iterator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--iter-range
    (lambda (from to step)
      (let ((current from))
        (lambda ()
          (if (< current to)
              (let ((val current))
                (setq current (+ current step))
                val)
            'done)))))

  (fset 'neovm--iter-map
    (lambda (fn iter)
      "Return an iterator that applies FN to each value from ITER."
      (lambda ()
        (let ((val (funcall iter)))
          (if (eq val 'done)
              'done
            (funcall fn val))))))

  (fset 'neovm--iter-collect
    (lambda (iter)
      (let ((result nil) (val (funcall iter)))
        (while (not (eq val 'done))
          (setq result (cons val result))
          (setq val (funcall iter)))
        (nreverse result))))

  (unwind-protect
      (list
       ;; Square each element
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-map
                         (lambda (x) (* x x))
                         (funcall 'neovm--iter-range 1 8 1)))
       ;; Convert to string
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-map
                         #'number-to-string
                         (funcall 'neovm--iter-range 10 15 1)))
       ;; Negate
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-map
                         (lambda (x) (- x))
                         (funcall 'neovm--iter-range 1 6 1)))
       ;; Map over empty iterator
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-map
                         (lambda (x) (* x 100))
                         (funcall 'neovm--iter-range 0 0 1)))
       ;; Chained maps: double then add 1
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-map
                         (lambda (x) (1+ x))
                         (funcall 'neovm--iter-map
                                  (lambda (x) (* x 2))
                                  (funcall 'neovm--iter-range 1 6 1)))))
    (fmakunbound 'neovm--iter-range)
    (fmakunbound 'neovm--iter-map)
    (fmakunbound 'neovm--iter-collect)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 4 9 16 25 36 49) (\"10\" \"11\" \"12\" \"13\" \"14\") (-1 -2 -3 -4 -5) nil (3 5 7 9 11))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Take/drop iterators: limit or skip elements
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_iter_take_drop_iterators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--iter-range
    (lambda (from to step)
      (let ((current from))
        (lambda ()
          (if (< current to)
              (let ((val current))
                (setq current (+ current step))
                val)
            'done)))))

  (fset 'neovm--iter-take
    (lambda (n iter)
      "Return an iterator that yields at most N values from ITER."
      (let ((remaining n))
        (lambda ()
          (if (<= remaining 0)
              'done
            (setq remaining (1- remaining))
            (funcall iter))))))

  (fset 'neovm--iter-drop
    (lambda (n iter)
      "Return an iterator that skips the first N values from ITER."
      (let ((skipped 0))
        (lambda ()
          (while (< skipped n)
            (funcall iter)
            (setq skipped (1+ skipped)))
          (funcall iter)))))

  (fset 'neovm--iter-collect
    (lambda (iter)
      (let ((result nil) (val (funcall iter)))
        (while (not (eq val 'done))
          (setq result (cons val result))
          (setq val (funcall iter)))
        (nreverse result))))

  (unwind-protect
      (list
       ;; Take 5 from 0..100
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-take 5
                         (funcall 'neovm--iter-range 0 100 1)))
       ;; Take 0 (empty)
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-take 0
                         (funcall 'neovm--iter-range 0 100 1)))
       ;; Take more than available
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-take 100
                         (funcall 'neovm--iter-range 0 5 1)))
       ;; Drop 3 from 0..10
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-drop 3
                         (funcall 'neovm--iter-range 0 10 1)))
       ;; Drop all
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-drop 10
                         (funcall 'neovm--iter-range 0 10 1)))
       ;; Drop 0 (no change)
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-drop 0
                         (funcall 'neovm--iter-range 0 5 1)))
       ;; Take after drop: skip 5, take 3 from 0..20
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-take 3
                         (funcall 'neovm--iter-drop 5
                                  (funcall 'neovm--iter-range 0 20 1)))))
    (fmakunbound 'neovm--iter-range)
    (fmakunbound 'neovm--iter-take)
    (fmakunbound 'neovm--iter-drop)
    (fmakunbound 'neovm--iter-collect)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 1 2 3 4) nil (0 1 2 3 4) (3 4 5 6 7 8 9) nil (0 1 2 3 4) (5 6 7))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Chain iterator: concatenate two iterators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_iter_chain_iterator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--iter-range
    (lambda (from to step)
      (let ((current from))
        (lambda ()
          (if (< current to)
              (let ((val current))
                (setq current (+ current step))
                val)
            'done)))))

  (fset 'neovm--iter-chain
    (lambda (iter1 iter2)
      "Return an iterator that yields all of ITER1 then all of ITER2."
      (let ((on-first t))
        (lambda ()
          (if on-first
              (let ((val (funcall iter1)))
                (if (eq val 'done)
                    (progn
                      (setq on-first nil)
                      (funcall iter2))
                  val))
            (funcall iter2))))))

  (fset 'neovm--iter-collect
    (lambda (iter)
      (let ((result nil) (val (funcall iter)))
        (while (not (eq val 'done))
          (setq result (cons val result))
          (setq val (funcall iter)))
        (nreverse result))))

  (unwind-protect
      (list
       ;; Chain 0..5 and 10..15
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-chain
                         (funcall 'neovm--iter-range 0 5 1)
                         (funcall 'neovm--iter-range 10 15 1)))
       ;; Chain empty with non-empty
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-chain
                         (funcall 'neovm--iter-range 0 0 1)
                         (funcall 'neovm--iter-range 1 4 1)))
       ;; Chain non-empty with empty
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-chain
                         (funcall 'neovm--iter-range 1 4 1)
                         (funcall 'neovm--iter-range 0 0 1)))
       ;; Chain empty with empty
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-chain
                         (funcall 'neovm--iter-range 0 0 1)
                         (funcall 'neovm--iter-range 0 0 1)))
       ;; Triple chain via nesting
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-chain
                         (funcall 'neovm--iter-chain
                                  (funcall 'neovm--iter-range 0 3 1)
                                  (funcall 'neovm--iter-range 10 13 1))
                         (funcall 'neovm--iter-range 100 103 1))))
    (fmakunbound 'neovm--iter-range)
    (fmakunbound 'neovm--iter-chain)
    (fmakunbound 'neovm--iter-collect)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 1 2 3 4 10 11 12 13 14) (1 2 3) (1 2 3) nil (0 1 2 10 11 12 100 101 102))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Zip iterator: pair elements from two iterators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_iter_zip_iterator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--iter-range
    (lambda (from to step)
      (let ((current from))
        (lambda ()
          (if (< current to)
              (let ((val current))
                (setq current (+ current step))
                val)
            'done)))))

  (fset 'neovm--iter-zip
    (lambda (iter1 iter2)
      "Return an iterator that yields (val1 . val2) pairs until either is done."
      (lambda ()
        (let ((v1 (funcall iter1))
              (v2 (funcall iter2)))
          (if (or (eq v1 'done) (eq v2 'done))
              'done
            (cons v1 v2))))))

  (fset 'neovm--iter-collect
    (lambda (iter)
      (let ((result nil) (val (funcall iter)))
        (while (not (eq val 'done))
          (setq result (cons val result))
          (setq val (funcall iter)))
        (nreverse result))))

  (unwind-protect
      (list
       ;; Zip equal-length ranges
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-zip
                         (funcall 'neovm--iter-range 0 5 1)
                         (funcall 'neovm--iter-range 100 105 1)))
       ;; Zip unequal lengths: shorter determines length
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-zip
                         (funcall 'neovm--iter-range 0 3 1)
                         (funcall 'neovm--iter-range 100 200 1)))
       ;; Zip with empty
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-zip
                         (funcall 'neovm--iter-range 0 0 1)
                         (funcall 'neovm--iter-range 0 10 1)))
       ;; Zip indices with squares
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-zip
                         (funcall 'neovm--iter-range 0 8 1)
                         ;; Build a squares iterator via closure
                         (let ((i 0))
                           (lambda ()
                             (if (>= i 8)
                                 'done
                               (let ((val (* i i)))
                                 (setq i (1+ i))
                                 val)))))))
    (fmakunbound 'neovm--iter-range)
    (fmakunbound 'neovm--iter-zip)
    (fmakunbound 'neovm--iter-collect)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 . 100) (1 . 101) (2 . 102) (3 . 103) (4 . 104)) ((0 . 100) (1 . 101) (2 . 102)) nil ((0 . 0) (1 . 1) (2 . 4) (3 . 9) (4 . 16) (5 . 25) (6 . 36) (7 . 49)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: lazy evaluation pipeline using composed iterators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_iter_lazy_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compose all iterator types into a complex lazy pipeline:
    // Generate 1..100, filter to primes, map to (p, p^2), take first 8,
    // chain with [(0 . 0)], then collect.
    let form = r#"(progn
  (fset 'neovm--iter-range
    (lambda (from to step)
      (let ((current from))
        (lambda ()
          (if (< current to)
              (let ((val current))
                (setq current (+ current step))
                val)
            'done)))))

  (fset 'neovm--iter-filter
    (lambda (pred iter)
      (lambda ()
        (let ((val (funcall iter)) (found nil))
          (while (and (not (eq val 'done)) (not found))
            (if (funcall pred val)
                (setq found t)
              (setq val (funcall iter))))
          val))))

  (fset 'neovm--iter-map
    (lambda (fn iter)
      (lambda ()
        (let ((val (funcall iter)))
          (if (eq val 'done) 'done (funcall fn val))))))

  (fset 'neovm--iter-take
    (lambda (n iter)
      (let ((remaining n))
        (lambda ()
          (if (<= remaining 0) 'done
            (setq remaining (1- remaining))
            (funcall iter))))))

  (fset 'neovm--iter-drop
    (lambda (n iter)
      (let ((skipped 0))
        (lambda ()
          (while (< skipped n)
            (funcall iter)
            (setq skipped (1+ skipped)))
          (funcall iter)))))

  (fset 'neovm--iter-chain
    (lambda (iter1 iter2)
      (let ((on-first t))
        (lambda ()
          (if on-first
              (let ((val (funcall iter1)))
                (if (eq val 'done)
                    (progn (setq on-first nil) (funcall iter2))
                  val))
            (funcall iter2))))))

  (fset 'neovm--iter-zip
    (lambda (iter1 iter2)
      (lambda ()
        (let ((v1 (funcall iter1)) (v2 (funcall iter2)))
          (if (or (eq v1 'done) (eq v2 'done)) 'done
            (cons v1 v2))))))

  (fset 'neovm--iter-collect
    (lambda (iter)
      (let ((result nil) (val (funcall iter)))
        (while (not (eq val 'done))
          (setq result (cons val result))
          (setq val (funcall iter)))
        (nreverse result))))

  (fset 'neovm--iter-is-prime
    (lambda (n)
      (and (> n 1)
           (let ((is-prime t) (d 2))
             (while (and is-prime (<= (* d d) n))
               (when (= 0 (mod n d)) (setq is-prime nil))
               (setq d (1+ d)))
             is-prime))))

  (unwind-protect
      (let* (;; Pipeline 1: primes from 1..100, map to (p . p^2), take 8
             (pipeline1
              (funcall 'neovm--iter-collect
                       (funcall 'neovm--iter-take 8
                                (funcall 'neovm--iter-map
                                         (lambda (p) (cons p (* p p)))
                                         (funcall 'neovm--iter-filter
                                                  'neovm--iter-is-prime
                                                  (funcall 'neovm--iter-range 1 100 1))))))
             ;; Pipeline 2: range 0..20, drop 5, filter evens, map to triple, take 4
             (pipeline2
              (funcall 'neovm--iter-collect
                       (funcall 'neovm--iter-take 4
                                (funcall 'neovm--iter-map
                                         (lambda (x) (* x 3))
                                         (funcall 'neovm--iter-filter
                                                  (lambda (x) (= 0 (mod x 2)))
                                                  (funcall 'neovm--iter-drop 5
                                                           (funcall 'neovm--iter-range 0 20 1)))))))
             ;; Pipeline 3: zip two filtered ranges
             (pipeline3
              (funcall 'neovm--iter-collect
                       (funcall 'neovm--iter-zip
                                (funcall 'neovm--iter-filter
                                         (lambda (x) (= 0 (mod x 2)))
                                         (funcall 'neovm--iter-range 0 20 1))
                                (funcall 'neovm--iter-filter
                                         (lambda (x) (= 0 (mod x 3)))
                                         (funcall 'neovm--iter-range 0 30 1)))))
             ;; Pipeline 4: chain two take-limited ranges
             (pipeline4
              (funcall 'neovm--iter-collect
                       (funcall 'neovm--iter-chain
                                (funcall 'neovm--iter-take 3
                                         (funcall 'neovm--iter-range 0 100 1))
                                (funcall 'neovm--iter-take 3
                                         (funcall 'neovm--iter-range 100 200 1)))))
             ;; Verify: sum of prime squares from pipeline1
             (prime-square-sum
              (let ((s 0))
                (dolist (pair pipeline1)
                  (setq s (+ s (cdr pair))))
                s)))
        (list
         :pipeline1 pipeline1
         :pipeline2 pipeline2
         :pipeline3 pipeline3
         :pipeline4 pipeline4
         :prime-square-sum prime-square-sum
         :p1-length (length pipeline1)
         :p2-length (length pipeline2)
         :p3-length (length pipeline3)
         :p4-length (length pipeline4)))
    (fmakunbound 'neovm--iter-range)
    (fmakunbound 'neovm--iter-filter)
    (fmakunbound 'neovm--iter-map)
    (fmakunbound 'neovm--iter-take)
    (fmakunbound 'neovm--iter-drop)
    (fmakunbound 'neovm--iter-chain)
    (fmakunbound 'neovm--iter-zip)
    (fmakunbound 'neovm--iter-collect)
    (fmakunbound 'neovm--iter-is-prime)))"#;
    let expect = expect_test::expect![[
        r#""OK (:pipeline1 ((2 . 4) (3 . 9) (5 . 25) (7 . 49) (11 . 121) (13 . 169) (17 . 289) (19 . 361)) :pipeline2 (18 24 30 36) :pipeline3 ((0 . 0) (2 . 3) (4 . 6) (6 . 9) (8 . 12) (10 . 15) (12 . 18) (14 . 21) (16 . 24) (18 . 27)) :pipeline4 (0 1 2 100 101 102) :prime-square-sum 1027 :p1-length 8 :p2-length 4 :p3-length 10 :p4-length 6)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: enumerate and reduce combinators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_iter_enumerate_reduce() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build enumerate (index-value pairs) and reduce (fold) combinators
    let form = r#"(progn
  (fset 'neovm--iter-range
    (lambda (from to step)
      (let ((current from))
        (lambda ()
          (if (< current to)
              (let ((val current))
                (setq current (+ current step))
                val)
            'done)))))

  (fset 'neovm--iter-enumerate
    (lambda (iter)
      "Return an iterator that yields (index . value) pairs."
      (let ((idx 0))
        (lambda ()
          (let ((val (funcall iter)))
            (if (eq val 'done)
                'done
              (let ((pair (cons idx val)))
                (setq idx (1+ idx))
                pair)))))))

  (fset 'neovm--iter-reduce
    (lambda (fn init iter)
      "Reduce ITER with FN and initial value INIT. Returns final accumulator."
      (let ((acc init)
            (val (funcall iter)))
        (while (not (eq val 'done))
          (setq acc (funcall fn acc val))
          (setq val (funcall iter)))
        acc)))

  (fset 'neovm--iter-filter
    (lambda (pred iter)
      (lambda ()
        (let ((val (funcall iter)) (found nil))
          (while (and (not (eq val 'done)) (not found))
            (if (funcall pred val) (setq found t)
              (setq val (funcall iter))))
          val))))

  (fset 'neovm--iter-map
    (lambda (fn iter)
      (lambda ()
        (let ((val (funcall iter)))
          (if (eq val 'done) 'done (funcall fn val))))))

  (fset 'neovm--iter-collect
    (lambda (iter)
      (let ((result nil) (val (funcall iter)))
        (while (not (eq val 'done))
          (setq result (cons val result))
          (setq val (funcall iter)))
        (nreverse result))))

  (unwind-protect
      (list
       ;; Enumerate 10..15
       (funcall 'neovm--iter-collect
                (funcall 'neovm--iter-enumerate
                         (funcall 'neovm--iter-range 10 15 1)))
       ;; Reduce: sum of 1..100
       (funcall 'neovm--iter-reduce
                (lambda (acc x) (+ acc x))
                0
                (funcall 'neovm--iter-range 1 101 1))
       ;; Reduce: product of 1..10
       (funcall 'neovm--iter-reduce
                (lambda (acc x) (* acc x))
                1
                (funcall 'neovm--iter-range 1 11 1))
       ;; Reduce: max of squares of odds from 1..20
       (funcall 'neovm--iter-reduce
                (lambda (acc x) (max acc x))
                0
                (funcall 'neovm--iter-map
                         (lambda (x) (* x x))
                         (funcall 'neovm--iter-filter
                                  (lambda (x) (= 1 (mod x 2)))
                                  (funcall 'neovm--iter-range 1 20 1))))
       ;; Reduce: build string from chars via enumerate
       (funcall 'neovm--iter-reduce
                (lambda (acc pair)
                  (concat acc (format "%d:%d " (car pair) (cdr pair))))
                ""
                (funcall 'neovm--iter-enumerate
                         (funcall 'neovm--iter-range 100 105 1))))
    (fmakunbound 'neovm--iter-range)
    (fmakunbound 'neovm--iter-enumerate)
    (fmakunbound 'neovm--iter-reduce)
    (fmakunbound 'neovm--iter-filter)
    (fmakunbound 'neovm--iter-map)
    (fmakunbound 'neovm--iter-collect)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 . 10) (1 . 11) (2 . 12) (3 . 13) (4 . 14)) 5050 3628800 361 \"0:100 1:101 2:102 3:103 4:104 \")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
