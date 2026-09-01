//! Oracle parity tests for memoization framework patterns in Elisp:
//! generic memoize wrapper, cache statistics (hits/misses), cache invalidation,
//! LRU eviction with bounded cache, multi-arg memoization, recursive function
//! memoization (fibonacci, Catalan numbers, partition function).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Generic memoize wrapper with hit/miss statistics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_memo_generic_wrapper_with_stats() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Build a memoization wrapper that tracks hits and misses.
  ;; Returns a closure triple: (call stats reset)
  (fset 'neovm--test-make-memoized
    (lambda (fn)
      "Wrap FN (1-arg) with memoization. Returns (call-fn stats-fn reset-fn)."
      (let ((cache (make-hash-table :test 'equal))
            (hits 0)
            (misses 0))
        (list
         ;; call
         (lambda (arg)
           (let ((cached (gethash arg cache 'neovm--miss)))
             (if (eq cached 'neovm--miss)
                 (progn
                   (setq misses (1+ misses))
                   (let ((val (funcall fn arg)))
                     (puthash arg val cache)
                     val))
               (setq hits (1+ hits))
               cached)))
         ;; stats
         (lambda ()
           (list :hits hits :misses misses
                 :total (+ hits misses)
                 :cache-size (hash-table-count cache)
                 :hit-rate (if (= 0 (+ hits misses)) 0
                             (/ (* hits 100) (+ hits misses)))))
         ;; reset
         (lambda ()
           (clrhash cache)
           (setq hits 0 misses 0)
           t)))))

  (unwind-protect
      (let* ((square-calls 0)
             (expensive-square
              (lambda (n)
                (setq square-calls (1+ square-calls))
                (* n n)))
             (memo (funcall 'neovm--test-make-memoized expensive-square))
             (call-fn (nth 0 memo))
             (stats-fn (nth 1 memo))
             (reset-fn (nth 2 memo)))
        ;; First round: all misses
        (let ((r1 (mapcar call-fn '(1 2 3 4 5))))
          (let ((s1 (funcall stats-fn))
                (calls-after-first square-calls))
            ;; Second round: all hits (same args)
            (let ((r2 (mapcar call-fn '(1 2 3 4 5))))
              (let ((s2 (funcall stats-fn))
                    (calls-after-second square-calls))
                ;; Mix of hits and misses
                (let ((r3 (mapcar call-fn '(3 6 1 7 5))))
                  (let ((s3 (funcall stats-fn)))
                    ;; Reset
                    (funcall reset-fn)
                    (let ((s4 (funcall stats-fn)))
                      ;; After reset, same args are misses again
                      (funcall call-fn 1)
                      (let ((s5 (funcall stats-fn)))
                        (list :r1 r1 :r2 r2 :r3 r3
                              :s1 s1 :s2 s2 :s3 s3
                              :s4-after-reset s4
                              :s5-after-reset-call s5
                              :actual-calls-first calls-after-first
                              :actual-calls-second calls-after-second))))))))))
    (fmakunbound 'neovm--test-make-memoized)))"#;
    let expect = expect_test::expect![[
        r#""OK (:r1 (1 4 9 16 25) :r2 (1 4 9 16 25) :r3 (9 36 1 49 25) :s1 (:hits 0 :misses 5 :total 5 :cache-size 5 :hit-rate 0) :s2 (:hits 5 :misses 5 :total 10 :cache-size 5 :hit-rate 50) :s3 (:hits 8 :misses 7 :total 15 :cache-size 7 :hit-rate 53) :s4-after-reset (:hits 0 :misses 0 :total 0 :cache-size 0 :hit-rate 0) :s5-after-reset-call (:hits 0 :misses 1 :total 1 :cache-size 1 :hit-rate 0) :actual-calls-first 5 :actual-calls-second 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-argument memoization with key construction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_memo_multi_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Memoize a 2-arg function by using (arg1 . arg2) as cache key
  (fset 'neovm--test-make-memo2
    (lambda (fn)
      (let ((cache (make-hash-table :test 'equal))
            (hits 0) (misses 0))
        (list
         (lambda (a b)
           (let* ((key (cons a b))
                  (cached (gethash key cache 'neovm--miss2)))
             (if (eq cached 'neovm--miss2)
                 (progn
                   (setq misses (1+ misses))
                   (let ((val (funcall fn a b)))
                     (puthash key val cache)
                     val))
               (setq hits (1+ hits))
               cached)))
         (lambda () (list :hits hits :misses misses
                          :size (hash-table-count cache)))))))

  (unwind-protect
      (let* ((add-fn (lambda (x y) (+ x y)))
             (memo (funcall 'neovm--test-make-memo2 add-fn))
             (call-fn (car memo))
             (stats-fn (cadr memo)))
        ;; Compute some values
        (let ((r1 (funcall call-fn 1 2))
              (r2 (funcall call-fn 3 4))
              (r3 (funcall call-fn 1 2))   ;; hit
              (r4 (funcall call-fn 2 1))   ;; miss: (2 . 1) != (1 . 2)
              (r5 (funcall call-fn 3 4)))  ;; hit
          ;; Now use with string concatenation
          (let* ((concat-fn (lambda (a b) (concat a "-" b)))
                 (memo2 (funcall 'neovm--test-make-memo2 concat-fn))
                 (call2 (car memo2))
                 (stats2 (cadr memo2)))
            (let ((c1 (funcall call2 "foo" "bar"))
                  (c2 (funcall call2 "baz" "qux"))
                  (c3 (funcall call2 "foo" "bar"))   ;; hit
                  (c4 (funcall call2 "bar" "foo")))  ;; miss
              (list :nums (list r1 r2 r3 r4 r5)
                    :num-stats (funcall stats-fn)
                    :strs (list c1 c2 c3 c4)
                    :str-stats (funcall stats2))))))
    (fmakunbound 'neovm--test-make-memo2)))"#;
    let expect = expect_test::expect![[
        r#""OK (:nums (3 7 3 3 7) :num-stats (:hits 2 :misses 3 :size 3) :strs (\"foo-bar\" \"baz-qux\" \"foo-bar\" \"bar-foo\") :str-stats (:hits 1 :misses 3 :size 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Recursive memoization: Fibonacci with explicit cache threading
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_memo_recursive_fibonacci() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Recursive Fibonacci with shared memoization table
  (defvar neovm--test-fib-cache (make-hash-table :test 'eql))
  (defvar neovm--test-fib-calls 0)

  (fset 'neovm--test-memo-fib
    (lambda (n)
      (setq neovm--test-fib-calls (1+ neovm--test-fib-calls))
      (or (gethash n neovm--test-fib-cache)
          (let ((val (cond
                       ((= n 0) 0)
                       ((= n 1) 1)
                       (t (+ (funcall 'neovm--test-memo-fib (- n 1))
                             (funcall 'neovm--test-memo-fib (- n 2)))))))
            (puthash n val neovm--test-fib-cache)
            val))))

  (unwind-protect
      (progn
        ;; Compute fib(30) — should be efficient with memoization
        (let ((fib30 (funcall 'neovm--test-memo-fib 30))
              (calls-after-30 neovm--test-fib-calls))
          ;; Compute fib(25) — should be all cache hits
          (setq neovm--test-fib-calls 0)
          (let ((fib25 (funcall 'neovm--test-memo-fib 25))
                (calls-for-25 neovm--test-fib-calls))
            ;; Compute fib(35) — only needs 31-35, rest cached
            (setq neovm--test-fib-calls 0)
            (let ((fib35 (funcall 'neovm--test-memo-fib 35))
                  (calls-for-35 neovm--test-fib-calls))
              (let ((cache-size (hash-table-count neovm--test-fib-cache))
                    ;; Verify known values
                    (fibs (mapcar (lambda (n) (funcall 'neovm--test-memo-fib n))
                                  '(0 1 2 3 4 5 6 7 8 9 10 15 20))))
                (list :fib30 fib30
                      :fib25 fib25
                      :fib35 fib35
                      :calls-for-30 calls-after-30
                      :calls-for-25-cached calls-for-25
                      :calls-for-35 calls-for-35
                      :cache-size cache-size
                      :known-fibs fibs))))))
    (fmakunbound 'neovm--test-memo-fib)
    (makunbound 'neovm--test-fib-cache)
    (makunbound 'neovm--test-fib-calls)))"#;
    let expect = expect_test::expect![[
        r#""OK (:fib30 832040 :fib25 75025 :fib35 9227465 :calls-for-30 59 :calls-for-25-cached 1 :calls-for-35 11 :cache-size 36 :known-fibs (0 1 1 2 3 5 8 13 21 34 55 610 6765))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Memoized Catalan numbers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_memo_catalan_numbers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Catalan(n) = sum_{i=0}^{n-1} Catalan(i) * Catalan(n-1-i)
  ;; C(0) = 1
  ;; First few: 1, 1, 2, 5, 14, 42, 132, 429, 1430, 4862
  (defvar neovm--test-catalan-cache (make-hash-table :test 'eql))

  (fset 'neovm--test-catalan
    (lambda (n)
      (or (gethash n neovm--test-catalan-cache)
          (let ((val (if (= n 0) 1
                       (let ((sum 0) (i 0))
                         (while (< i n)
                           (setq sum (+ sum (* (funcall 'neovm--test-catalan i)
                                               (funcall 'neovm--test-catalan (- n 1 i)))))
                           (setq i (1+ i)))
                         sum))))
            (puthash n val neovm--test-catalan-cache)
            val))))

  (unwind-protect
      (let ((catalans (mapcar (lambda (n) (funcall 'neovm--test-catalan n))
                              '(0 1 2 3 4 5 6 7 8 9 10)))
            (cache-size (hash-table-count neovm--test-catalan-cache)))
        ;; Verify recurrence: C(n) = (2*(2n-1))/(n+1) * C(n-1)
        ;; We verify by checking: C(n) * (n+1) = (4n-2) * C(n-1)
        (let ((recurrence-ok t))
          (let ((i 1))
            (while (<= i 10)
              (let ((cn (funcall 'neovm--test-catalan i))
                    (cn1 (funcall 'neovm--test-catalan (- i 1))))
                (unless (= (* cn (+ i 1)) (* (- (* 4 i) 2) cn1))
                  (setq recurrence-ok nil)))
              (setq i (1+ i))))
          (list :catalans catalans
                :cache-size cache-size
                :recurrence-valid recurrence-ok)))
    (fmakunbound 'neovm--test-catalan)
    (makunbound 'neovm--test-catalan-cache)))"#;
    let expect = expect_test::expect![[
        r#""OK (:catalans (1 1 2 5 14 42 132 429 1430 4862 16796) :cache-size 11 :recurrence-valid t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Memoized integer partition function
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_memo_partition_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // p(n, k) = number of ways to partition n using parts <= k
    // p(n) = p(n, n)
    // p(0, k) = 1
    // p(n, 0) = 0 for n > 0
    // p(n, k) = p(n, k-1) + p(n-k, k) if k <= n
    // p(n, k) = p(n, n) if k > n
    let form = r#"(progn
  (defvar neovm--test-part-cache (make-hash-table :test 'equal))

  (fset 'neovm--test-partition
    (lambda (n k)
      (let ((key (cons n k)))
        (or (gethash key neovm--test-part-cache)
            (let ((val (cond
                         ((= n 0) 1)
                         ((= k 0) 0)
                         ((> k n) (funcall 'neovm--test-partition n n))
                         (t (+ (funcall 'neovm--test-partition n (- k 1))
                               (funcall 'neovm--test-partition (- n k) k))))))
              (puthash key val neovm--test-part-cache)
              val)))))

  (fset 'neovm--test-partitions
    (lambda (n) (funcall 'neovm--test-partition n n)))

  (unwind-protect
      (let ((p-values (mapcar (lambda (n) (funcall 'neovm--test-partitions n))
                              '(0 1 2 3 4 5 6 7 8 9 10 15 20))))
        ;; Known values: p(0)=1, p(1)=1, p(2)=2, p(3)=3, p(4)=5,
        ;; p(5)=7, p(10)=42, p(15)=176, p(20)=627
        (let ((cache-size (hash-table-count neovm--test-part-cache)))
          ;; Verify some restricted partition counts
          ;; p(10, 3) = partitions of 10 using parts 1,2,3
          (let ((p10-3 (funcall 'neovm--test-partition 10 3))
                ;; p(10, 1) = 1 (only 1+1+...+1)
                (p10-1 (funcall 'neovm--test-partition 10 1))
                ;; p(10, 2) = 6 (number of ways to split into 1s and 2s)
                (p10-2 (funcall 'neovm--test-partition 10 2)))
            (list :partitions p-values
                  :cache-size cache-size
                  :p10-restricted-to-3 p10-3
                  :p10-restricted-to-1 p10-1
                  :p10-restricted-to-2 p10-2))))
    (fmakunbound 'neovm--test-partition)
    (fmakunbound 'neovm--test-partitions)
    (makunbound 'neovm--test-part-cache)))"#;
    let expect = expect_test::expect![[
        r#""OK (:partitions (1 1 2 3 5 7 11 15 22 30 42 176 627) :cache-size 206 :p10-restricted-to-3 14 :p10-restricted-to-1 1 :p10-restricted-to-2 6)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bounded cache with LRU eviction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_memo_bounded_lru_cache() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Bounded memoization with LRU eviction policy.
  ;; Cache is an alist ordered by recency (most recent first).
  (fset 'neovm--test-make-bounded-memo
    (lambda (fn capacity)
      (let ((cache nil)
            (hits 0) (misses 0) (evictions 0))
        (list
         ;; call
         (lambda (arg)
           (let ((entry nil) (rest nil))
             ;; Search cache
             (dolist (e cache)
               (if (equal (car e) arg)
                   (setq entry e)
                 (setq rest (cons e rest))))
             (if entry
                 ;; Hit: move to front
                 (progn
                   (setq hits (1+ hits))
                   (setq cache (cons entry (nreverse rest)))
                   (cdr entry))
               ;; Miss: compute, add to front, maybe evict
               (setq misses (1+ misses))
               (let ((val (funcall fn arg)))
                 (setq cache (cons (cons arg val) (nreverse rest)))
                 ;; Evict if over capacity
                 (when (> (length cache) capacity)
                   (setq cache (butlast cache (- (length cache) capacity)))
                   (setq evictions (1+ evictions)))
                 val))))
         ;; stats
         (lambda ()
           (list :hits hits :misses misses :evictions evictions
                 :size (length cache)
                 :keys (mapcar #'car cache)))))))

  (unwind-protect
      (let* ((double-fn (lambda (x) (* x 2)))
             (memo (funcall 'neovm--test-make-bounded-memo double-fn 3))
             (call-fn (car memo))
             (stats-fn (cadr memo)))
        ;; Fill cache: 1, 2, 3
        (let ((r1 (funcall call-fn 1))
              (r2 (funcall call-fn 2))
              (r3 (funcall call-fn 3)))
          (let ((s1 (funcall stats-fn)))
            ;; Access 1 (hit, moves to front)
            (funcall call-fn 1)
            ;; Add 4 → evicts LRU (2)
            (let ((r4 (funcall call-fn 4)))
              (let ((s2 (funcall stats-fn)))
                ;; Access 2 → miss (was evicted)
                (let ((r5 (funcall call-fn 2)))
                  ;; This evicts LRU (3)
                  (let ((s3 (funcall stats-fn)))
                    ;; Access 3 → miss (was evicted)
                    (funcall call-fn 3)
                    ;; Access sequence to verify order
                    (funcall call-fn 1)
                    (funcall call-fn 4)
                    (let ((s4 (funcall stats-fn)))
                      (list :r1 r1 :r2 r2 :r3 r3 :r4 r4 :r5 r5
                            :s1 s1 :s2 s2 :s3 s3 :s4 s4)))))))))
    (fmakunbound 'neovm--test-make-bounded-memo)))"#;
    let expect = expect_test::expect![[
        r#""OK (:r1 2 :r2 4 :r3 6 :r4 8 :r5 4 :s1 (:hits 0 :misses 3 :evictions 0 :size 3 :keys (3 2 1)) :s2 (:hits 1 :misses 4 :evictions 1 :size 3 :keys (4 1 3)) :s3 (:hits 1 :misses 5 :evictions 2 :size 3 :keys (2 4 1)) :s4 (:hits 1 :misses 8 :evictions 5 :size 3 :keys (4 1 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cache invalidation with dependencies
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_memo_cache_invalidation_dependencies() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A memoization system where invalidating one key also invalidates
    // all keys that depend on it (dependency tracking).
    let form = r#"(progn
  (fset 'neovm--test-make-dep-cache
    (lambda ()
      "Create cache with dependency tracking."
      (let ((values (make-hash-table :test 'equal))
            (deps (make-hash-table :test 'equal)))  ;; key -> list of dependents
        (list
         ;; put(key, val, depends-on-keys)
         (lambda (key val depend-keys)
           (puthash key val values)
           ;; Register this key as dependent on each of depend-keys
           (dolist (dk depend-keys)
             (let ((existing (gethash dk deps)))
               (unless (member key existing)
                 (puthash dk (cons key existing) deps)))))
         ;; get(key)
         (lambda (key)
           (gethash key values))
         ;; invalidate(key) — also invalidates all dependents recursively
         (lambda (key)
           (let ((to-invalidate (list key))
                 (invalidated nil))
             (while to-invalidate
               (let ((k (car to-invalidate)))
                 (setq to-invalidate (cdr to-invalidate))
                 (unless (member k invalidated)
                   (setq invalidated (cons k invalidated))
                   (remhash k values)
                   ;; Add dependents to queue
                   (dolist (dep (gethash k deps))
                     (setq to-invalidate (cons dep to-invalidate))))))
             (nreverse invalidated)))
         ;; keys
         (lambda ()
           (let ((ks nil))
             (maphash (lambda (k _v) (setq ks (cons k ks))) values)
             (sort ks #'string<)))))))

  (unwind-protect
      (let* ((cache (funcall 'neovm--test-make-dep-cache))
             (put-fn (nth 0 cache))
             (get-fn (nth 1 cache))
             (invalidate-fn (nth 2 cache))
             (keys-fn (nth 3 cache)))
        ;; Build dependency chain: config -> derived-a -> final-x
        ;;                         config -> derived-b -> final-y
        (funcall put-fn "config" "base-value" nil)
        (funcall put-fn "derived-a" "from-config-a" '("config"))
        (funcall put-fn "derived-b" "from-config-b" '("config"))
        (funcall put-fn "final-x" "from-derived-a" '("derived-a"))
        (funcall put-fn "final-y" "from-derived-b" '("derived-b"))
        (funcall put-fn "independent" "no-deps" nil)
        (let ((keys-before (funcall keys-fn))
              (val-before (funcall get-fn "final-x")))
          ;; Invalidate "config" -> cascades to derived-a, derived-b, final-x, final-y
          (let ((invalidated (funcall invalidate-fn "config")))
            (let ((keys-after (funcall keys-fn))
                  (config-after (funcall get-fn "config"))
                  (indep-after (funcall get-fn "independent")))
              (list :keys-before keys-before
                    :val-before val-before
                    :invalidated (sort invalidated #'string<)
                    :keys-after keys-after
                    :config-nil config-after
                    :independent-survives indep-after)))))
    (fmakunbound 'neovm--test-make-dep-cache)))"#;
    let expect = expect_test::expect![[
        r#""OK (:keys-before (\"config\" \"derived-a\" \"derived-b\" \"final-x\" \"final-y\" \"independent\") :val-before \"from-derived-a\" :invalidated (\"config\" \"derived-a\" \"derived-b\" \"final-x\" \"final-y\") :keys-after (\"independent\") :config-nil nil :independent-survives \"no-deps\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
