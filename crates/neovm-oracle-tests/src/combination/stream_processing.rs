//! Oracle parity tests for lazy stream processing in Elisp:
//! delay/force with closures, stream map/filter/take/drop/zip,
//! infinite streams (naturals, fibonacci), stream-based sieve of
//! Eratosthenes, stream accumulation/reduction, and stream interleaving.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Delay/force primitives with memoization and stream constructors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_stream_delay_force_memoized() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A promise is (forced-flag . thunk-or-value).
    // A stream is either nil (empty) or (value . promise-of-rest).
    let form = r#"(progn
  ;; Promise: (nil . thunk) -> (t . value) after first force
  (fset 'neovm--sp-delay
    (lambda (thunk)
      "Create a promise from THUNK (a zero-arg lambda)."
      (cons nil thunk)))

  (fset 'neovm--sp-force
    (lambda (promise)
      "Force a promise, memoizing the result."
      (if (car promise)
          (cdr promise)
        (let ((val (funcall (cdr promise))))
          (setcar promise t)
          (setcdr promise val)
          val))))

  ;; Stream constructors
  (fset 'neovm--sp-empty (lambda () nil))
  (fset 'neovm--sp-empty-p (lambda (s) (null s)))

  (fset 'neovm--sp-cons
    (lambda (head tail-thunk)
      "Lazy cons: HEAD is eager, TAIL-THUNK is delayed."
      (cons head (funcall 'neovm--sp-delay tail-thunk))))

  (fset 'neovm--sp-head (lambda (s) (car s)))
  (fset 'neovm--sp-tail
    (lambda (s) (funcall 'neovm--sp-force (cdr s))))

  (fset 'neovm--sp-take
    (lambda (n s)
      (let ((result nil) (cur s) (i 0))
        (while (and (< i n) (not (funcall 'neovm--sp-empty-p cur)))
          (push (funcall 'neovm--sp-head cur) result)
          (setq cur (funcall 'neovm--sp-tail cur)
                i (1+ i)))
        (nreverse result))))

  (unwind-protect
      (let ((eval-count 0))
        ;; Build a stream where each tail-thunk increments a counter
        (let ((s (funcall 'neovm--sp-cons 10
                          (lambda ()
                            (setq eval-count (1+ eval-count))
                            (funcall 'neovm--sp-cons 20
                                     (lambda ()
                                       (setq eval-count (1+ eval-count))
                                       (funcall 'neovm--sp-cons 30
                                                (lambda ()
                                                  (setq eval-count (1+ eval-count))
                                                  nil))))))))
          ;; First traversal forces all thunks
          (let ((take1 (funcall 'neovm--sp-take 3 s))
                (count1 eval-count))
            ;; Second traversal: memoized, no additional forces
            (let ((take2 (funcall 'neovm--sp-take 3 s))
                  (count2 eval-count))
              ;; Partial take: only forces first thunk
              (setq eval-count 0)
              (let ((s2 (funcall 'neovm--sp-cons 'a
                                 (lambda ()
                                   (setq eval-count (1+ eval-count))
                                   (funcall 'neovm--sp-cons 'b
                                            (lambda ()
                                              (setq eval-count (1+ eval-count))
                                              nil))))))
                (let ((take-1 (funcall 'neovm--sp-take 1 s2))
                      (count-after-1 eval-count))
                  (list take1 take2
                        (equal take1 take2)
                        count1 count2
                        (= count1 count2)   ;; memoization works
                        take-1 count-after-1)))))))
    (fmakunbound 'neovm--sp-delay)
    (fmakunbound 'neovm--sp-force)
    (fmakunbound 'neovm--sp-empty)
    (fmakunbound 'neovm--sp-empty-p)
    (fmakunbound 'neovm--sp-cons)
    (fmakunbound 'neovm--sp-head)
    (fmakunbound 'neovm--sp-tail)
    (fmakunbound 'neovm--sp-take)))"#;
    let expect = expect_test::expect![[r#""OK ((10 20 30) (10 20 30) t 3 3 t (a) 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Stream map, filter, take, drop
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_stream_map_filter_take_drop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--s2-delay (lambda (thunk) (cons nil thunk)))
  (fset 'neovm--s2-force
    (lambda (p) (if (car p) (cdr p) (let ((v (funcall (cdr p)))) (setcar p t) (setcdr p v) v))))
  (fset 'neovm--s2-cons
    (lambda (h tt) (cons h (funcall 'neovm--s2-delay tt))))
  (fset 'neovm--s2-head (lambda (s) (car s)))
  (fset 'neovm--s2-tail (lambda (s) (funcall 'neovm--s2-force (cdr s))))
  (fset 'neovm--s2-empty-p (lambda (s) (null s)))
  (fset 'neovm--s2-take
    (lambda (n s)
      (let ((r nil) (c s) (i 0))
        (while (and (< i n) (not (funcall 'neovm--s2-empty-p c)))
          (push (funcall 'neovm--s2-head c) r)
          (setq c (funcall 'neovm--s2-tail c) i (1+ i)))
        (nreverse r))))

  ;; Infinite naturals
  (fset 'neovm--s2-nats
    (lambda (n)
      (funcall 'neovm--s2-cons n (lambda () (funcall 'neovm--s2-nats (1+ n))))))

  ;; Map
  (fset 'neovm--s2-map
    (lambda (fn s)
      (if (funcall 'neovm--s2-empty-p s) nil
        (funcall 'neovm--s2-cons
                 (funcall fn (funcall 'neovm--s2-head s))
                 (lambda () (funcall 'neovm--s2-map fn (funcall 'neovm--s2-tail s)))))))

  ;; Filter
  (fset 'neovm--s2-filter
    (lambda (pred s)
      (if (funcall 'neovm--s2-empty-p s) nil
        (let ((h (funcall 'neovm--s2-head s)))
          (if (funcall pred h)
              (funcall 'neovm--s2-cons h
                       (lambda () (funcall 'neovm--s2-filter pred (funcall 'neovm--s2-tail s))))
            (funcall 'neovm--s2-filter pred (funcall 'neovm--s2-tail s)))))))

  ;; Drop first n elements
  (fset 'neovm--s2-drop
    (lambda (n s)
      (let ((cur s) (i 0))
        (while (and (< i n) (not (funcall 'neovm--s2-empty-p cur)))
          (setq cur (funcall 'neovm--s2-tail cur) i (1+ i)))
        cur)))

  (unwind-protect
      (let ((nats (funcall 'neovm--s2-nats 1)))
        (list
         ;; map: cube each natural
         (funcall 'neovm--s2-take 8
                  (funcall 'neovm--s2-map (lambda (x) (* x x x)) nats))
         ;; filter: multiples of 3
         (funcall 'neovm--s2-take 7
                  (funcall 'neovm--s2-filter (lambda (x) (= (% x 3) 0)) nats))
         ;; map then filter: squares that are even
         (funcall 'neovm--s2-take 5
                  (funcall 'neovm--s2-filter
                           (lambda (x) (= (% x 2) 0))
                           (funcall 'neovm--s2-map (lambda (x) (* x x)) nats)))
         ;; filter then map: double the primes (simple primality)
         (funcall 'neovm--s2-take 6
                  (funcall 'neovm--s2-map
                           (lambda (x) (* x 2))
                           (funcall 'neovm--s2-filter
                                    (lambda (n)
                                      (and (> n 1)
                                           (let ((div 2) (prime t))
                                             (while (and (<= (* div div) n) prime)
                                               (when (= (% n div) 0) (setq prime nil))
                                               (setq div (1+ div)))
                                             prime)))
                                    nats)))
         ;; drop first 5, take next 5
         (funcall 'neovm--s2-take 5 (funcall 'neovm--s2-drop 5 nats))
         ;; drop then filter
         (funcall 'neovm--s2-take 4
                  (funcall 'neovm--s2-filter
                           (lambda (x) (= (% x 7) 0))
                           (funcall 'neovm--s2-drop 10 nats)))
         ;; Compose: drop, map, filter, take
         (funcall 'neovm--s2-take 5
                  (funcall 'neovm--s2-filter
                           (lambda (x) (> x 50))
                           (funcall 'neovm--s2-map
                                    (lambda (x) (* x x))
                                    (funcall 'neovm--s2-drop 3 nats))))))
    (fmakunbound 'neovm--s2-delay)
    (fmakunbound 'neovm--s2-force)
    (fmakunbound 'neovm--s2-cons)
    (fmakunbound 'neovm--s2-head)
    (fmakunbound 'neovm--s2-tail)
    (fmakunbound 'neovm--s2-empty-p)
    (fmakunbound 'neovm--s2-take)
    (fmakunbound 'neovm--s2-nats)
    (fmakunbound 'neovm--s2-map)
    (fmakunbound 'neovm--s2-filter)
    (fmakunbound 'neovm--s2-drop)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 8 27 64 125 216 343 512) (3 6 9 12 15 18 21) (4 16 36 64 100) (4 6 10 14 22 26) (6 7 8 9 10) (14 21 28 35) (64 81 100 121 144))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Zip and zipWith on infinite streams
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_stream_zip_and_zipwith() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--s3-delay (lambda (thunk) (cons nil thunk)))
  (fset 'neovm--s3-force
    (lambda (p) (if (car p) (cdr p) (let ((v (funcall (cdr p)))) (setcar p t) (setcdr p v) v))))
  (fset 'neovm--s3-cons
    (lambda (h tt) (cons h (funcall 'neovm--s3-delay tt))))
  (fset 'neovm--s3-head (lambda (s) (car s)))
  (fset 'neovm--s3-tail (lambda (s) (funcall 'neovm--s3-force (cdr s))))
  (fset 'neovm--s3-empty-p (lambda (s) (null s)))
  (fset 'neovm--s3-take
    (lambda (n s)
      (let ((r nil) (c s) (i 0))
        (while (and (< i n) (not (funcall 'neovm--s3-empty-p c)))
          (push (funcall 'neovm--s3-head c) r)
          (setq c (funcall 'neovm--s3-tail c) i (1+ i)))
        (nreverse r))))
  (fset 'neovm--s3-nats
    (lambda (n) (funcall 'neovm--s3-cons n (lambda () (funcall 'neovm--s3-nats (1+ n))))))
  (fset 'neovm--s3-map
    (lambda (fn s)
      (if (funcall 'neovm--s3-empty-p s) nil
        (funcall 'neovm--s3-cons
                 (funcall fn (funcall 'neovm--s3-head s))
                 (lambda () (funcall 'neovm--s3-map fn (funcall 'neovm--s3-tail s)))))))

  ;; Zip: pair elements
  (fset 'neovm--s3-zip
    (lambda (s1 s2)
      (if (or (funcall 'neovm--s3-empty-p s1) (funcall 'neovm--s3-empty-p s2))
          nil
        (funcall 'neovm--s3-cons
                 (cons (funcall 'neovm--s3-head s1) (funcall 'neovm--s3-head s2))
                 (lambda ()
                   (funcall 'neovm--s3-zip
                            (funcall 'neovm--s3-tail s1)
                            (funcall 'neovm--s3-tail s2)))))))

  ;; ZipWith: combine with a function
  (fset 'neovm--s3-zipwith
    (lambda (fn s1 s2)
      (if (or (funcall 'neovm--s3-empty-p s1) (funcall 'neovm--s3-empty-p s2))
          nil
        (funcall 'neovm--s3-cons
                 (funcall fn (funcall 'neovm--s3-head s1)
                          (funcall 'neovm--s3-head s2))
                 (lambda ()
                   (funcall 'neovm--s3-zipwith fn
                            (funcall 'neovm--s3-tail s1)
                            (funcall 'neovm--s3-tail s2)))))))

  ;; Fibonacci using zipWith (fibs = 0 : 1 : zipWith + fibs (tail fibs))
  (fset 'neovm--s3-fibs
    (lambda ()
      (let ((fibs nil))
        (setq fibs (funcall 'neovm--s3-cons 0
                            (lambda ()
                              (funcall 'neovm--s3-cons 1
                                       (lambda ()
                                         (funcall 'neovm--s3-zipwith
                                                  #'+
                                                  fibs
                                                  (funcall 'neovm--s3-tail fibs)))))))
        fibs)))

  (unwind-protect
      (let ((nats (funcall 'neovm--s3-nats 1)))
        (list
         ;; Zip naturals with their squares
         (funcall 'neovm--s3-take 6
                  (funcall 'neovm--s3-zip
                           nats
                           (funcall 'neovm--s3-map (lambda (x) (* x x)) nats)))
         ;; ZipWith +: sum of naturals and naturals*10
         (funcall 'neovm--s3-take 6
                  (funcall 'neovm--s3-zipwith
                           #'+
                           nats
                           (funcall 'neovm--s3-map (lambda (x) (* x 10)) nats)))
         ;; ZipWith *: product of consecutive naturals
         (funcall 'neovm--s3-take 7
                  (funcall 'neovm--s3-zipwith
                           #'*
                           nats
                           (funcall 'neovm--s3-nats 2)))
         ;; Fibonacci stream via zipWith
         (funcall 'neovm--s3-take 15 (funcall 'neovm--s3-fibs))
         ;; Zip three streams using nested zip
         (funcall 'neovm--s3-take 5
                  (funcall 'neovm--s3-map
                           (lambda (pair)
                             (list (car (car pair)) (cdr (car pair)) (cdr pair)))
                           (funcall 'neovm--s3-zip
                                    (funcall 'neovm--s3-zip nats
                                             (funcall 'neovm--s3-map (lambda (x) (* x x)) nats))
                                    (funcall 'neovm--s3-map (lambda (x) (* x x x)) nats))))
         ;; ZipWith on finite streams: shorter one terminates
         (let ((finite (funcall 'neovm--s3-cons 100
                                (lambda ()
                                  (funcall 'neovm--s3-cons 200
                                           (lambda ()
                                             (funcall 'neovm--s3-cons 300
                                                      (lambda () nil))))))))
           (funcall 'neovm--s3-take 10
                    (funcall 'neovm--s3-zipwith #'+ finite nats)))))
    (fmakunbound 'neovm--s3-delay)
    (fmakunbound 'neovm--s3-force)
    (fmakunbound 'neovm--s3-cons)
    (fmakunbound 'neovm--s3-head)
    (fmakunbound 'neovm--s3-tail)
    (fmakunbound 'neovm--s3-empty-p)
    (fmakunbound 'neovm--s3-take)
    (fmakunbound 'neovm--s3-nats)
    (fmakunbound 'neovm--s3-map)
    (fmakunbound 'neovm--s3-zip)
    (fmakunbound 'neovm--s3-zipwith)
    (fmakunbound 'neovm--s3-fibs)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 . 1) (2 . 4) (3 . 9) (4 . 16) (5 . 25) (6 . 36)) (11 22 33 44 55 66) (2 6 12 20 30 42 56) (0 1 1 2 3 5 8 13 21 34 55 89 144 233 377) ((1 1 1) (2 4 8) (3 9 27) (4 16 64) (5 25 125)) (101 202 303))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sieve of Eratosthenes via lazy streams
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_stream_sieve_eratosthenes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--s4-delay (lambda (thunk) (cons nil thunk)))
  (fset 'neovm--s4-force
    (lambda (p) (if (car p) (cdr p) (let ((v (funcall (cdr p)))) (setcar p t) (setcdr p v) v))))
  (fset 'neovm--s4-cons
    (lambda (h tt) (cons h (funcall 'neovm--s4-delay tt))))
  (fset 'neovm--s4-head (lambda (s) (car s)))
  (fset 'neovm--s4-tail (lambda (s) (funcall 'neovm--s4-force (cdr s))))
  (fset 'neovm--s4-empty-p (lambda (s) (null s)))
  (fset 'neovm--s4-take
    (lambda (n s)
      (let ((r nil) (c s) (i 0))
        (while (and (< i n) (not (funcall 'neovm--s4-empty-p c)))
          (push (funcall 'neovm--s4-head c) r)
          (setq c (funcall 'neovm--s4-tail c) i (1+ i)))
        (nreverse r))))

  ;; Integers from n
  (fset 'neovm--s4-from
    (lambda (n) (funcall 'neovm--s4-cons n (lambda () (funcall 'neovm--s4-from (1+ n))))))

  ;; Filter
  (fset 'neovm--s4-filter
    (lambda (pred s)
      (if (funcall 'neovm--s4-empty-p s) nil
        (let ((h (funcall 'neovm--s4-head s)))
          (if (funcall pred h)
              (funcall 'neovm--s4-cons h
                       (lambda () (funcall 'neovm--s4-filter pred (funcall 'neovm--s4-tail s))))
            (funcall 'neovm--s4-filter pred (funcall 'neovm--s4-tail s)))))))

  ;; Sieve: take head as prime, remove its multiples from rest
  (fset 'neovm--s4-sieve
    (lambda (s)
      (if (funcall 'neovm--s4-empty-p s) nil
        (let ((p (funcall 'neovm--s4-head s)))
          (funcall 'neovm--s4-cons p
                   (lambda ()
                     (funcall 'neovm--s4-sieve
                              (funcall 'neovm--s4-filter
                                       (lambda (x) (/= (% x p) 0))
                                       (funcall 'neovm--s4-tail s)))))))))

  (fset 'neovm--s4-primes
    (lambda () (funcall 'neovm--s4-sieve (funcall 'neovm--s4-from 2))))

  ;; Nth element (0-indexed)
  (fset 'neovm--s4-nth
    (lambda (n s)
      (let ((cur s) (i 0))
        (while (< i n)
          (setq cur (funcall 'neovm--s4-tail cur) i (1+ i)))
        (funcall 'neovm--s4-head cur))))

  (unwind-protect
      (let ((primes (funcall 'neovm--s4-primes)))
        (list
         ;; First 25 primes
         (funcall 'neovm--s4-take 25 primes)
         ;; 10th prime (0-indexed) = 29
         (funcall 'neovm--s4-nth 9 (funcall 'neovm--s4-primes))
         ;; Twin primes among first 20 primes: (p, p+2) both prime
         (let ((ps (funcall 'neovm--s4-take 20 (funcall 'neovm--s4-primes))))
           (let ((twins nil)
                 (rest ps))
             (while (cdr rest)
               (when (= (- (cadr rest) (car rest)) 2)
                 (push (list (car rest) (cadr rest)) twins))
               (setq rest (cdr rest)))
             (nreverse twins)))
         ;; Sum of first 15 primes
         (let ((ps (funcall 'neovm--s4-take 15 (funcall 'neovm--s4-primes))))
           (apply #'+ ps))
         ;; Verify: all returned values are indeed prime (trial division)
         (let ((ps (funcall 'neovm--s4-take 20 (funcall 'neovm--s4-primes))))
           (let ((all-prime t))
             (dolist (p ps)
               (when (> p 1)
                 (let ((div 2) (is-prime t))
                   (while (and (<= (* div div) p) is-prime)
                     (when (= (% p div) 0) (setq is-prime nil))
                     (setq div (1+ div)))
                   (unless is-prime (setq all-prime nil)))))
             all-prime))))
    (fmakunbound 'neovm--s4-delay)
    (fmakunbound 'neovm--s4-force)
    (fmakunbound 'neovm--s4-cons)
    (fmakunbound 'neovm--s4-head)
    (fmakunbound 'neovm--s4-tail)
    (fmakunbound 'neovm--s4-empty-p)
    (fmakunbound 'neovm--s4-take)
    (fmakunbound 'neovm--s4-from)
    (fmakunbound 'neovm--s4-filter)
    (fmakunbound 'neovm--s4-sieve)
    (fmakunbound 'neovm--s4-primes)
    (fmakunbound 'neovm--s4-nth)))"#;
    let expect = expect_test::expect![[
        r#""OK ((2 3 5 7 11 13 17 19 23 29 31 37 41 43 47 53 59 61 67 71 73 79 83 89 97) 29 ((3 5) (5 7) (11 13) (17 19) (29 31) (41 43) (59 61)) 328 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Stream accumulation and reduction (foldl, scanl)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_stream_accumulate_reduce() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--s5-delay (lambda (thunk) (cons nil thunk)))
  (fset 'neovm--s5-force
    (lambda (p) (if (car p) (cdr p) (let ((v (funcall (cdr p)))) (setcar p t) (setcdr p v) v))))
  (fset 'neovm--s5-cons
    (lambda (h tt) (cons h (funcall 'neovm--s5-delay tt))))
  (fset 'neovm--s5-head (lambda (s) (car s)))
  (fset 'neovm--s5-tail (lambda (s) (funcall 'neovm--s5-force (cdr s))))
  (fset 'neovm--s5-empty-p (lambda (s) (null s)))
  (fset 'neovm--s5-take
    (lambda (n s)
      (let ((r nil) (c s) (i 0))
        (while (and (< i n) (not (funcall 'neovm--s5-empty-p c)))
          (push (funcall 'neovm--s5-head c) r)
          (setq c (funcall 'neovm--s5-tail c) i (1+ i)))
        (nreverse r))))
  (fset 'neovm--s5-nats
    (lambda (n) (funcall 'neovm--s5-cons n (lambda () (funcall 'neovm--s5-nats (1+ n))))))

  ;; Scanl: like foldl but produces a stream of partial results
  ;; scanl f z (x1:x2:...) = z : (f z x1) : (f (f z x1) x2) : ...
  (fset 'neovm--s5-scanl
    (lambda (fn acc s)
      (funcall 'neovm--s5-cons acc
               (lambda ()
                 (if (funcall 'neovm--s5-empty-p s)
                     nil
                   (funcall 'neovm--s5-scanl
                            fn
                            (funcall fn acc (funcall 'neovm--s5-head s))
                            (funcall 'neovm--s5-tail s)))))))

  ;; Foldl on first n elements (eagerly reduces)
  (fset 'neovm--s5-foldl
    (lambda (fn acc n s)
      (let ((cur s) (result acc) (i 0))
        (while (and (< i n) (not (funcall 'neovm--s5-empty-p cur)))
          (setq result (funcall fn result (funcall 'neovm--s5-head cur))
                cur (funcall 'neovm--s5-tail cur)
                i (1+ i)))
        result)))

  ;; TakeWhile: take elements while predicate holds
  (fset 'neovm--s5-takewhile
    (lambda (pred s)
      (if (or (funcall 'neovm--s5-empty-p s)
              (not (funcall pred (funcall 'neovm--s5-head s))))
          nil
        (funcall 'neovm--s5-cons
                 (funcall 'neovm--s5-head s)
                 (lambda ()
                   (funcall 'neovm--s5-takewhile pred
                            (funcall 'neovm--s5-tail s)))))))

  (unwind-protect
      (let ((nats (funcall 'neovm--s5-nats 1)))
        (list
         ;; Scanl with +: running sum stream [0, 1, 3, 6, 10, 15, ...]
         (funcall 'neovm--s5-take 8
                  (funcall 'neovm--s5-scanl #'+ 0 nats))
         ;; Scanl with *: running product [1, 1, 2, 6, 24, 120, ...] (factorials!)
         (funcall 'neovm--s5-take 8
                  (funcall 'neovm--s5-scanl #'* 1 nats))
         ;; Foldl: sum of first 10 naturals
         (funcall 'neovm--s5-foldl #'+ 0 10 nats)
         ;; Foldl: product of first 6 naturals (6! = 720)
         (funcall 'neovm--s5-foldl #'* 1 6 nats)
         ;; Foldl: max of first 10 values of (n mod 7)
         (funcall 'neovm--s5-foldl
                  (lambda (acc x) (max acc (% x 7)))
                  0 20 nats)
         ;; Scanl to compute running averages (as integer division)
         (let ((sums (funcall 'neovm--s5-scanl #'+ 0 nats)))
           (funcall 'neovm--s5-take 8
                    ;; skip the initial 0, take sum[i+1]/(i+1)
                    (funcall 'neovm--s5-scanl
                             (lambda (acc x) (1+ acc))
                             0
                             nats)))
         ;; TakeWhile: running sum < 100
         (let ((running-sums (funcall 'neovm--s5-scanl #'+ 0 nats)))
           (funcall 'neovm--s5-take 20
                    (funcall 'neovm--s5-takewhile
                             (lambda (x) (< x 100))
                             running-sums)))))
    (fmakunbound 'neovm--s5-delay)
    (fmakunbound 'neovm--s5-force)
    (fmakunbound 'neovm--s5-cons)
    (fmakunbound 'neovm--s5-head)
    (fmakunbound 'neovm--s5-tail)
    (fmakunbound 'neovm--s5-empty-p)
    (fmakunbound 'neovm--s5-take)
    (fmakunbound 'neovm--s5-nats)
    (fmakunbound 'neovm--s5-scanl)
    (fmakunbound 'neovm--s5-foldl)
    (fmakunbound 'neovm--s5-takewhile)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 1 3 6 10 15 21 28) (1 1 2 6 24 120 720 5040) 55 720 6 (0 1 2 3 4 5 6 7) (0 1 3 6 10 15 21 28 36 45 55 66 78 91))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Stream interleaving and merge
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_stream_interleave_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--s6-delay (lambda (thunk) (cons nil thunk)))
  (fset 'neovm--s6-force
    (lambda (p) (if (car p) (cdr p) (let ((v (funcall (cdr p)))) (setcar p t) (setcdr p v) v))))
  (fset 'neovm--s6-cons
    (lambda (h tt) (cons h (funcall 'neovm--s6-delay tt))))
  (fset 'neovm--s6-head (lambda (s) (car s)))
  (fset 'neovm--s6-tail (lambda (s) (funcall 'neovm--s6-force (cdr s))))
  (fset 'neovm--s6-empty-p (lambda (s) (null s)))
  (fset 'neovm--s6-take
    (lambda (n s)
      (let ((r nil) (c s) (i 0))
        (while (and (< i n) (not (funcall 'neovm--s6-empty-p c)))
          (push (funcall 'neovm--s6-head c) r)
          (setq c (funcall 'neovm--s6-tail c) i (1+ i)))
        (nreverse r))))

  ;; Interleave: alternate elements from two streams
  ;; interleave (a:as) (b:bs) = a : b : interleave as bs
  (fset 'neovm--s6-interleave
    (lambda (s1 s2)
      (cond
       ((funcall 'neovm--s6-empty-p s1) s2)
       ((funcall 'neovm--s6-empty-p s2) s1)
       (t (funcall 'neovm--s6-cons
                   (funcall 'neovm--s6-head s1)
                   (lambda ()
                     (funcall 'neovm--s6-cons
                              (funcall 'neovm--s6-head s2)
                              (lambda ()
                                (funcall 'neovm--s6-interleave
                                         (funcall 'neovm--s6-tail s1)
                                         (funcall 'neovm--s6-tail s2))))))))))

  ;; Sorted merge: merge two sorted streams, keeping duplicates
  (fset 'neovm--s6-merge
    (lambda (s1 s2)
      (cond
       ((funcall 'neovm--s6-empty-p s1) s2)
       ((funcall 'neovm--s6-empty-p s2) s1)
       (t (let ((h1 (funcall 'neovm--s6-head s1))
                (h2 (funcall 'neovm--s6-head s2)))
            (if (<= h1 h2)
                (funcall 'neovm--s6-cons h1
                         (lambda ()
                           (funcall 'neovm--s6-merge
                                    (funcall 'neovm--s6-tail s1) s2)))
              (funcall 'neovm--s6-cons h2
                       (lambda ()
                         (funcall 'neovm--s6-merge
                                  s1 (funcall 'neovm--s6-tail s2))))))))))

  ;; Unique: remove consecutive duplicates from sorted stream
  (fset 'neovm--s6-unique
    (lambda (s)
      (if (funcall 'neovm--s6-empty-p s) nil
        (let ((h (funcall 'neovm--s6-head s)))
          (funcall 'neovm--s6-cons h
                   (lambda ()
                     (let ((rest (funcall 'neovm--s6-tail s)))
                       (while (and (not (funcall 'neovm--s6-empty-p rest))
                                   (= (funcall 'neovm--s6-head rest) h))
                         (setq rest (funcall 'neovm--s6-tail rest)))
                       (funcall 'neovm--s6-unique rest))))))))

  ;; Multiples stream
  (fset 'neovm--s6-multiples
    (lambda (k n)
      (funcall 'neovm--s6-cons n
               (lambda () (funcall 'neovm--s6-multiples k (+ n k))))))

  ;; From list to finite stream
  (fset 'neovm--s6-from-list
    (lambda (lst)
      (if (null lst) nil
        (funcall 'neovm--s6-cons (car lst)
                 (lambda () (funcall 'neovm--s6-from-list (cdr lst)))))))

  (unwind-protect
      (list
       ;; Interleave evens and odds
       (funcall 'neovm--s6-take 12
                (funcall 'neovm--s6-interleave
                         (funcall 'neovm--s6-multiples 2 0)   ;; 0,2,4,6,...
                         (funcall 'neovm--s6-multiples 2 1))) ;; 1,3,5,7,...
       ;; Interleave powers of 2 with powers of 3
       (funcall 'neovm--s6-take 10
                (funcall 'neovm--s6-interleave
                         (funcall 'neovm--s6-from-list '(1 2 4 8 16 32 64))
                         (funcall 'neovm--s6-from-list '(1 3 9 27 81 243))))
       ;; Sorted merge of multiples of 2 and 3
       (funcall 'neovm--s6-take 15
                (funcall 'neovm--s6-merge
                         (funcall 'neovm--s6-multiples 2 2)
                         (funcall 'neovm--s6-multiples 3 3)))
       ;; Sorted merge with unique: Hamming-like (no dups)
       (funcall 'neovm--s6-take 15
                (funcall 'neovm--s6-unique
                         (funcall 'neovm--s6-merge
                                  (funcall 'neovm--s6-multiples 2 2)
                                  (funcall 'neovm--s6-multiples 3 3))))
       ;; Three-way merge: multiples of 2, 3, 5 (unique)
       (funcall 'neovm--s6-take 20
                (funcall 'neovm--s6-unique
                         (funcall 'neovm--s6-merge
                                  (funcall 'neovm--s6-multiples 2 2)
                                  (funcall 'neovm--s6-merge
                                           (funcall 'neovm--s6-multiples 3 3)
                                           (funcall 'neovm--s6-multiples 5 5)))))
       ;; Interleave finite with infinite: finite one eventually stops contributing
       (funcall 'neovm--s6-take 8
                (funcall 'neovm--s6-interleave
                         (funcall 'neovm--s6-from-list '(100 200 300))
                         (funcall 'neovm--s6-multiples 1 1))))
    (fmakunbound 'neovm--s6-delay)
    (fmakunbound 'neovm--s6-force)
    (fmakunbound 'neovm--s6-cons)
    (fmakunbound 'neovm--s6-head)
    (fmakunbound 'neovm--s6-tail)
    (fmakunbound 'neovm--s6-empty-p)
    (fmakunbound 'neovm--s6-take)
    (fmakunbound 'neovm--s6-interleave)
    (fmakunbound 'neovm--s6-merge)
    (fmakunbound 'neovm--s6-unique)
    (fmakunbound 'neovm--s6-multiples)
    (fmakunbound 'neovm--s6-from-list)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 1 2 3 4 5 6 7 8 9 10 11) (1 1 2 3 4 9 8 27 16 81) (2 3 4 6 6 8 9 10 12 12 14 15 16 18 18) (2 3 4 6 8 9 10 12 14 15 16 18 20 21 22) (2 3 4 5 6 8 9 10 12 14 15 16 18 20 21 22 24 25 26 27) (100 1 200 2 300 3 4 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
