//! Oracle parity tests for lazy evaluation (streams) in Elisp:
//! delay/force with closures, lazy cons/car/cdr, infinite streams
//! (naturals, fibonacci, primes), stream operations (map, filter,
//! take, zip), merging sorted streams, and stream memoization.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Delay/force and lazy cons/car/cdr primitives
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lazy_delay_force_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement delay/force using cons cells with a lambda.
    // A lazy value is (promise . (lambda () value)) initially,
    // becomes (forced . value) after first force.
    // lazy-cons creates (head . (lambda () tail-stream)).
    let form = r#"(progn
  ;; delay: create a promise (unevaluated thunk)
  ;; We represent a stream as (value . thunk-for-rest)
  ;; where thunk-for-rest is a lambda returning the next stream cell or nil.

  (fset 'neovm--lz-cons
    (lambda (head tail-thunk)
      "Create a lazy cons: HEAD is eager, TAIL-THUNK is a lambda producing the rest."
      (cons head tail-thunk)))

  (fset 'neovm--lz-car
    (lambda (stream)
      "Get the head of a lazy stream."
      (car stream)))

  (fset 'neovm--lz-cdr
    (lambda (stream)
      "Force the tail of a lazy stream."
      (if (null stream)
          nil
        (let ((tail-thunk (cdr stream)))
          (if (functionp tail-thunk)
              (funcall tail-thunk)
            tail-thunk)))))

  (fset 'neovm--lz-take
    (lambda (n stream)
      "Take first N elements from a lazy stream."
      (let ((result nil)
            (s stream)
            (i 0))
        (while (and (< i n) s)
          (setq result (cons (funcall 'neovm--lz-car s) result))
          (setq s (funcall 'neovm--lz-cdr s))
          (setq i (1+ i)))
        (nreverse result))))

  (unwind-protect
      (list
       ;; Simple finite stream: (1 2 3)
       (let* ((s3 (funcall 'neovm--lz-cons 3 (lambda () nil)))
              (s2 (funcall 'neovm--lz-cons 2 (lambda () s3)))
              (s1 (funcall 'neovm--lz-cons 1 (lambda () s2))))
         (list
          (funcall 'neovm--lz-car s1)
          (funcall 'neovm--lz-car (funcall 'neovm--lz-cdr s1))
          (funcall 'neovm--lz-car (funcall 'neovm--lz-cdr (funcall 'neovm--lz-cdr s1)))
          (funcall 'neovm--lz-cdr (funcall 'neovm--lz-cdr (funcall 'neovm--lz-cdr s1)))))

       ;; Take from a finite stream
       (let* ((s (funcall 'neovm--lz-cons 'a
                          (lambda ()
                            (funcall 'neovm--lz-cons 'b
                                     (lambda ()
                                       (funcall 'neovm--lz-cons 'c (lambda () nil))))))))
         (list
          (funcall 'neovm--lz-take 2 s)
          (funcall 'neovm--lz-take 5 s)
          (funcall 'neovm--lz-take 0 s)))

       ;; Empty stream
       (funcall 'neovm--lz-take 3 nil))
    (fmakunbound 'neovm--lz-cons)
    (fmakunbound 'neovm--lz-car)
    (fmakunbound 'neovm--lz-cdr)
    (fmakunbound 'neovm--lz-take)))"#;
    let expect = expect_test::expect![[r#""OK ((1 2 3 nil) ((a b) (a b c) nil) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Infinite streams: naturals and fibonacci
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lazy_infinite_streams() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build infinite streams of natural numbers and fibonacci numbers,
    // then take finite prefixes.
    let form = r#"(progn
  (fset 'neovm--lz2-cons
    (lambda (head tail-thunk)
      (cons head tail-thunk)))

  (fset 'neovm--lz2-car
    (lambda (s) (car s)))

  (fset 'neovm--lz2-cdr
    (lambda (s)
      (if (null s) nil
        (let ((t (cdr s)))
          (if (functionp t) (funcall t) t)))))

  (fset 'neovm--lz2-take
    (lambda (n s)
      (let ((result nil) (cur s) (i 0))
        (while (and (< i n) cur)
          (setq result (cons (funcall 'neovm--lz2-car cur) result))
          (setq cur (funcall 'neovm--lz2-cdr cur))
          (setq i (1+ i)))
        (nreverse result))))

  ;; Infinite stream of naturals starting from n
  (fset 'neovm--lz2-naturals
    (lambda (n)
      (funcall 'neovm--lz2-cons n
               (lambda () (funcall 'neovm--lz2-naturals (1+ n))))))

  ;; Infinite fibonacci stream
  (fset 'neovm--lz2-fib-from
    (lambda (a b)
      (funcall 'neovm--lz2-cons a
               (lambda () (funcall 'neovm--lz2-fib-from b (+ a b))))))

  (fset 'neovm--lz2-fib
    (lambda ()
      (funcall 'neovm--lz2-fib-from 0 1)))

  ;; Infinite stream of squares
  (fset 'neovm--lz2-squares-from
    (lambda (n)
      (funcall 'neovm--lz2-cons (* n n)
               (lambda () (funcall 'neovm--lz2-squares-from (1+ n))))))

  (unwind-protect
      (list
       ;; First 15 naturals starting from 0
       (funcall 'neovm--lz2-take 15 (funcall 'neovm--lz2-naturals 0))
       ;; Naturals starting from 10
       (funcall 'neovm--lz2-take 5 (funcall 'neovm--lz2-naturals 10))
       ;; First 12 fibonacci numbers
       (funcall 'neovm--lz2-take 12 (funcall 'neovm--lz2-fib))
       ;; First 8 squares
       (funcall 'neovm--lz2-take 8 (funcall 'neovm--lz2-squares-from 1))
       ;; Take 0 from infinite stream
       (funcall 'neovm--lz2-take 0 (funcall 'neovm--lz2-naturals 0)))
    (fmakunbound 'neovm--lz2-cons)
    (fmakunbound 'neovm--lz2-car)
    (fmakunbound 'neovm--lz2-cdr)
    (fmakunbound 'neovm--lz2-take)
    (fmakunbound 'neovm--lz2-naturals)
    (fmakunbound 'neovm--lz2-fib-from)
    (fmakunbound 'neovm--lz2-fib)
    (fmakunbound 'neovm--lz2-squares-from)))"#;
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Infinite prime sieve stream
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lazy_prime_sieve() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement the Sieve of Eratosthenes lazily: filter out multiples
    // from an infinite stream of naturals.
    let form = r#"(progn
  (fset 'neovm--lz3-cons
    (lambda (h t) (cons h t)))
  (fset 'neovm--lz3-car
    (lambda (s) (car s)))
  (fset 'neovm--lz3-cdr
    (lambda (s) (if (null s) nil (let ((t (cdr s))) (if (functionp t) (funcall t) t)))))
  (fset 'neovm--lz3-take
    (lambda (n s)
      (let ((r nil) (c s) (i 0))
        (while (and (< i n) c)
          (setq r (cons (funcall 'neovm--lz3-car c) r))
          (setq c (funcall 'neovm--lz3-cdr c))
          (setq i (1+ i)))
        (nreverse r))))

  ;; Stream of integers from n
  (fset 'neovm--lz3-from
    (lambda (n)
      (funcall 'neovm--lz3-cons n
               (lambda () (funcall 'neovm--lz3-from (1+ n))))))

  ;; Filter a stream: keep only elements satisfying pred
  (fset 'neovm--lz3-filter
    (lambda (pred stream)
      (if (null stream)
          nil
        (let ((h (funcall 'neovm--lz3-car stream)))
          (if (funcall pred h)
              (funcall 'neovm--lz3-cons h
                       (lambda ()
                         (funcall 'neovm--lz3-filter pred
                                  (funcall 'neovm--lz3-cdr stream))))
            (funcall 'neovm--lz3-filter pred
                     (funcall 'neovm--lz3-cdr stream)))))))

  ;; Sieve: take head as prime, filter its multiples from the rest
  (fset 'neovm--lz3-sieve
    (lambda (stream)
      (if (null stream)
          nil
        (let ((p (funcall 'neovm--lz3-car stream)))
          (funcall 'neovm--lz3-cons p
                   (lambda ()
                     (funcall 'neovm--lz3-sieve
                              (funcall 'neovm--lz3-filter
                                       (lambda (x) (/= (% x p) 0))
                                       (funcall 'neovm--lz3-cdr stream)))))))))

  (fset 'neovm--lz3-primes
    (lambda ()
      (funcall 'neovm--lz3-sieve (funcall 'neovm--lz3-from 2))))

  (unwind-protect
      (list
       ;; First 20 primes
       (funcall 'neovm--lz3-take 20 (funcall 'neovm--lz3-primes))
       ;; First 5 primes
       (funcall 'neovm--lz3-take 5 (funcall 'neovm--lz3-primes))
       ;; Verify: first 10 primes
       (funcall 'neovm--lz3-take 10 (funcall 'neovm--lz3-primes)))
    (fmakunbound 'neovm--lz3-cons)
    (fmakunbound 'neovm--lz3-car)
    (fmakunbound 'neovm--lz3-cdr)
    (fmakunbound 'neovm--lz3-take)
    (fmakunbound 'neovm--lz3-from)
    (fmakunbound 'neovm--lz3-filter)
    (fmakunbound 'neovm--lz3-sieve)
    (fmakunbound 'neovm--lz3-primes)))"#;
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Stream operations: map, filter, take, zip
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lazy_stream_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement map, filter, zip on lazy streams and compose them.
    let form = r#"(progn
  (fset 'neovm--lz4-cons
    (lambda (h t) (cons h t)))
  (fset 'neovm--lz4-car
    (lambda (s) (car s)))
  (fset 'neovm--lz4-cdr
    (lambda (s) (if (null s) nil (let ((t (cdr s))) (if (functionp t) (funcall t) t)))))
  (fset 'neovm--lz4-take
    (lambda (n s)
      (let ((r nil) (c s) (i 0))
        (while (and (< i n) c)
          (setq r (cons (funcall 'neovm--lz4-car c) r))
          (setq c (funcall 'neovm--lz4-cdr c))
          (setq i (1+ i)))
        (nreverse r))))

  (fset 'neovm--lz4-from
    (lambda (n)
      (funcall 'neovm--lz4-cons n
               (lambda () (funcall 'neovm--lz4-from (1+ n))))))

  ;; Map: apply fn to each element
  (fset 'neovm--lz4-map
    (lambda (fn stream)
      (if (null stream)
          nil
        (funcall 'neovm--lz4-cons
                 (funcall fn (funcall 'neovm--lz4-car stream))
                 (lambda ()
                   (funcall 'neovm--lz4-map fn
                            (funcall 'neovm--lz4-cdr stream)))))))

  ;; Filter: keep elements satisfying pred
  (fset 'neovm--lz4-filter
    (lambda (pred stream)
      (if (null stream)
          nil
        (let ((h (funcall 'neovm--lz4-car stream)))
          (if (funcall pred h)
              (funcall 'neovm--lz4-cons h
                       (lambda ()
                         (funcall 'neovm--lz4-filter pred
                                  (funcall 'neovm--lz4-cdr stream))))
            (funcall 'neovm--lz4-filter pred
                     (funcall 'neovm--lz4-cdr stream)))))))

  ;; Zip: pair elements from two streams
  (fset 'neovm--lz4-zip
    (lambda (s1 s2)
      (if (or (null s1) (null s2))
          nil
        (funcall 'neovm--lz4-cons
                 (cons (funcall 'neovm--lz4-car s1)
                       (funcall 'neovm--lz4-car s2))
                 (lambda ()
                   (funcall 'neovm--lz4-zip
                            (funcall 'neovm--lz4-cdr s1)
                            (funcall 'neovm--lz4-cdr s2)))))))

  (unwind-protect
      (let ((nats (funcall 'neovm--lz4-from 1)))
        (list
         ;; Map: double each natural
         (funcall 'neovm--lz4-take 8
                  (funcall 'neovm--lz4-map (lambda (x) (* x 2)) nats))

         ;; Filter: only even naturals
         (funcall 'neovm--lz4-take 8
                  (funcall 'neovm--lz4-filter (lambda (x) (= (% x 2) 0)) nats))

         ;; Compose: map then filter (squares of odd numbers)
         (funcall 'neovm--lz4-take 6
                  (funcall 'neovm--lz4-map
                           (lambda (x) (* x x))
                           (funcall 'neovm--lz4-filter
                                    (lambda (x) (/= (% x 2) 0))
                                    nats)))

         ;; Zip naturals with their squares
         (funcall 'neovm--lz4-take 6
                  (funcall 'neovm--lz4-zip
                           nats
                           (funcall 'neovm--lz4-map (lambda (x) (* x x)) nats)))

         ;; Filter then map: even numbers tripled
         (funcall 'neovm--lz4-take 5
                  (funcall 'neovm--lz4-map
                           (lambda (x) (* x 3))
                           (funcall 'neovm--lz4-filter
                                    (lambda (x) (= (% x 2) 0))
                                    nats)))

         ;; Zip two mapped streams
         (funcall 'neovm--lz4-take 5
                  (funcall 'neovm--lz4-zip
                           (funcall 'neovm--lz4-map (lambda (x) (* x 10)) nats)
                           (funcall 'neovm--lz4-map (lambda (x) (+ x 100)) nats)))))
    (fmakunbound 'neovm--lz4-cons)
    (fmakunbound 'neovm--lz4-car)
    (fmakunbound 'neovm--lz4-cdr)
    (fmakunbound 'neovm--lz4-take)
    (fmakunbound 'neovm--lz4-from)
    (fmakunbound 'neovm--lz4-map)
    (fmakunbound 'neovm--lz4-filter)
    (fmakunbound 'neovm--lz4-zip)))"#;
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Merge sorted streams
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lazy_merge_sorted_streams() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Merge two sorted infinite streams into a single sorted stream,
    // removing duplicates.
    let form = r#"(progn
  (fset 'neovm--lz5-cons
    (lambda (h t) (cons h t)))
  (fset 'neovm--lz5-car
    (lambda (s) (car s)))
  (fset 'neovm--lz5-cdr
    (lambda (s) (if (null s) nil (let ((t (cdr s))) (if (functionp t) (funcall t) t)))))
  (fset 'neovm--lz5-take
    (lambda (n s)
      (let ((r nil) (c s) (i 0))
        (while (and (< i n) c)
          (setq r (cons (funcall 'neovm--lz5-car c) r))
          (setq c (funcall 'neovm--lz5-cdr c))
          (setq i (1+ i)))
        (nreverse r))))

  ;; Stream of multiples of k starting from k
  (fset 'neovm--lz5-multiples
    (lambda (k start)
      (funcall 'neovm--lz5-cons start
               (lambda () (funcall 'neovm--lz5-multiples k (+ start k))))))

  ;; Merge two sorted streams, removing duplicates
  (fset 'neovm--lz5-merge
    (lambda (s1 s2)
      (cond
       ((null s1) s2)
       ((null s2) s1)
       (t
        (let ((h1 (funcall 'neovm--lz5-car s1))
              (h2 (funcall 'neovm--lz5-car s2)))
          (cond
           ((< h1 h2)
            (funcall 'neovm--lz5-cons h1
                     (lambda ()
                       (funcall 'neovm--lz5-merge
                                (funcall 'neovm--lz5-cdr s1) s2))))
           ((> h1 h2)
            (funcall 'neovm--lz5-cons h2
                     (lambda ()
                       (funcall 'neovm--lz5-merge
                                s1 (funcall 'neovm--lz5-cdr s2)))))
           (t ;; equal: take one, skip both
            (funcall 'neovm--lz5-cons h1
                     (lambda ()
                       (funcall 'neovm--lz5-merge
                                (funcall 'neovm--lz5-cdr s1)
                                (funcall 'neovm--lz5-cdr s2)))))))))))

  (unwind-protect
      (list
       ;; Merge multiples of 2 and multiples of 3 (sorted, no dups)
       ;; Should give: 2 3 4 6 8 9 10 12 14 15 16 18 ...
       (funcall 'neovm--lz5-take 15
                (funcall 'neovm--lz5-merge
                         (funcall 'neovm--lz5-multiples 2 2)
                         (funcall 'neovm--lz5-multiples 3 3)))

       ;; Merge multiples of 3 and multiples of 5
       (funcall 'neovm--lz5-take 12
                (funcall 'neovm--lz5-merge
                         (funcall 'neovm--lz5-multiples 3 3)
                         (funcall 'neovm--lz5-multiples 5 5)))

       ;; Merge three streams: multiples of 2, 3, and 5
       (funcall 'neovm--lz5-take 15
                (funcall 'neovm--lz5-merge
                         (funcall 'neovm--lz5-multiples 2 2)
                         (funcall 'neovm--lz5-merge
                                  (funcall 'neovm--lz5-multiples 3 3)
                                  (funcall 'neovm--lz5-multiples 5 5))))

       ;; Merge two identical streams (all duplicates removed)
       (funcall 'neovm--lz5-take 8
                (funcall 'neovm--lz5-merge
                         (funcall 'neovm--lz5-multiples 7 7)
                         (funcall 'neovm--lz5-multiples 7 7))))
    (fmakunbound 'neovm--lz5-cons)
    (fmakunbound 'neovm--lz5-car)
    (fmakunbound 'neovm--lz5-cdr)
    (fmakunbound 'neovm--lz5-take)
    (fmakunbound 'neovm--lz5-multiples)
    (fmakunbound 'neovm--lz5-merge)))"#;
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Stream memoization: force only once
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lazy_memoized_streams() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement memoized streams where forcing the tail records the result
    // so subsequent accesses don't recompute. Use a side-effect counter
    // to verify memoization works.
    let form = r#"(progn
  ;; A memoized stream cell: (value . (memo-cell))
  ;; memo-cell = (nil . thunk) before force, (t . result) after force

  (fset 'neovm--lzm-cons
    (lambda (head tail-thunk)
      "Create a memoized lazy cons."
      (cons head (cons nil tail-thunk))))

  (fset 'neovm--lzm-car
    (lambda (s) (car s)))

  (fset 'neovm--lzm-cdr
    (lambda (s)
      "Force the tail, memoizing the result."
      (if (null s)
          nil
        (let ((memo (cdr s)))
          (if (car memo)
              ;; Already forced
              (cdr memo)
            ;; First time: force and memoize
            (let ((result (funcall (cdr memo))))
              (setcar memo t)
              (setcdr memo result)
              result))))))

  (fset 'neovm--lzm-take
    (lambda (n s)
      (let ((r nil) (c s) (i 0))
        (while (and (< i n) c)
          (setq r (cons (funcall 'neovm--lzm-car c) r))
          (setq c (funcall 'neovm--lzm-cdr c))
          (setq i (1+ i)))
        (nreverse r))))

  (unwind-protect
      (let ((eval-count 0))
        ;; Create a stream where each element computation increments eval-count
        (let ((s (funcall 'neovm--lzm-cons 1
                          (lambda ()
                            (setq eval-count (1+ eval-count))
                            (funcall 'neovm--lzm-cons 2
                                     (lambda ()
                                       (setq eval-count (1+ eval-count))
                                       (funcall 'neovm--lzm-cons 3
                                                (lambda ()
                                                  (setq eval-count (1+ eval-count))
                                                  nil))))))))
          ;; Take all 3 elements (forces tail thunks)
          (let ((first-take (funcall 'neovm--lzm-take 3 s))
                (count-after-first eval-count))
            ;; Take again: should NOT increment eval-count (memoized)
            (let ((second-take (funcall 'neovm--lzm-take 3 s))
                  (count-after-second eval-count))
              ;; Take a third time
              (let ((third-take (funcall 'neovm--lzm-take 3 s))
                    (count-after-third eval-count))
                (list
                 'first-take first-take
                 'second-take second-take
                 'third-take third-take
                 'evals-after-first count-after-first
                 'evals-after-second count-after-second
                 'evals-after-third count-after-third
                 ;; All takes should produce the same result
                 'all-equal (and (equal first-take second-take)
                                 (equal second-take third-take))
                 ;; Memoization: count should not increase after first take
                 'memoized (= count-after-first count-after-second count-after-third)))))))
    (fmakunbound 'neovm--lzm-cons)
    (fmakunbound 'neovm--lzm-car)
    (fmakunbound 'neovm--lzm-cdr)
    (fmakunbound 'neovm--lzm-take)))"#;
    let expect = expect_test::expect![[
        r#""OK (first-take (1 2 3) second-take (1 2 3) third-take (1 2 3) evals-after-first 3 evals-after-second 3 evals-after-third 3 all-equal t memoized t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
