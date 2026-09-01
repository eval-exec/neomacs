//! Oracle parity tests for LRU (Least Recently Used) cache implementation.
//!
//! Implements an LRU cache in Elisp using a hash table for O(1) lookup
//! and an ordered list for recency tracking. Tests fixed capacity eviction,
//! cache hit/miss ratio tracking, TTL-based expiry, and two-level caching.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Basic LRU cache: fixed capacity, get/put, eviction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lru_cache_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; LRU cache: (capacity hash-table order-list)
  ;; order-list: most-recently-used at front, least at back
  (fset 'neovm--lru-create
    (lambda (capacity)
      (list capacity (make-hash-table :test 'equal) nil)))

  (fset 'neovm--lru-get
    (lambda (cache key)
      "Get value for KEY, moving it to most-recently-used. Returns nil if not found."
      (let ((ht (nth 1 cache))
            (val (gethash key (nth 1 cache))))
        (when val
          ;; Move key to front of order list
          (setcar (cddr cache) (cons key (delete key (nth 2 cache)))))
        val)))

  (fset 'neovm--lru-put
    (lambda (cache key value)
      "Put KEY/VALUE into cache. Evict LRU if at capacity."
      (let ((cap (nth 0 cache))
            (ht (nth 1 cache))
            (order (nth 2 cache)))
        ;; If key already exists, update in place
        (if (gethash key ht)
            (progn
              (puthash key value ht)
              (setcar (cddr cache) (cons key (delete key order))))
          ;; New key: check capacity
          (when (>= (hash-table-count ht) cap)
            ;; Evict least-recently-used (last in list)
            (let ((lru-key (car (last order))))
              (remhash lru-key ht)
              (setcar (cddr cache) (butlast order))))
          (puthash key value ht)
          (setcar (cddr cache) (cons key (nth 2 cache)))))))

  (fset 'neovm--lru-keys
    (lambda (cache)
      "Return keys in MRU-to-LRU order."
      (nth 2 cache)))

  (fset 'neovm--lru-size
    (lambda (cache)
      (hash-table-count (nth 1 cache))))

  (unwind-protect
      (let ((c (funcall 'neovm--lru-create 3)))
        ;; Put 3 items
        (funcall 'neovm--lru-put c "a" 1)
        (funcall 'neovm--lru-put c "b" 2)
        (funcall 'neovm--lru-put c "c" 3)
        (let ((after-3 (list (funcall 'neovm--lru-keys c)
                             (funcall 'neovm--lru-size c))))
          ;; Access "a" -> moves to front
          (funcall 'neovm--lru-get c "a")
          (let ((after-get-a (funcall 'neovm--lru-keys c)))
            ;; Put 4th item -> evicts LRU ("b")
            (funcall 'neovm--lru-put c "d" 4)
            (let ((after-evict (list (funcall 'neovm--lru-keys c)
                                     (funcall 'neovm--lru-size c)
                                     (funcall 'neovm--lru-get c "b")   ;; nil (evicted)
                                     (funcall 'neovm--lru-get c "a")   ;; 1
                                     (funcall 'neovm--lru-get c "d")))) ;; 4
              (list after-3 after-get-a after-evict)))))
    (fmakunbound 'neovm--lru-create)
    (fmakunbound 'neovm--lru-get)
    (fmakunbound 'neovm--lru-put)
    (fmakunbound 'neovm--lru-keys)
    (fmakunbound 'neovm--lru-size)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"c\" \"b\") 3) (\"a\" \"c\" \"b\") ((\"d\" \"c\") 3 nil 1 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// LRU cache: update existing key
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lru_cache_update_existing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--lru2-create
    (lambda (cap) (list cap (make-hash-table :test 'equal) nil)))

  (fset 'neovm--lru2-get
    (lambda (c key)
      (let ((val (gethash key (nth 1 c))))
        (when val
          (setcar (cddr c) (cons key (delete key (nth 2 c)))))
        val)))

  (fset 'neovm--lru2-put
    (lambda (c key value)
      (let ((ht (nth 1 c)))
        (if (gethash key ht)
            (progn
              (puthash key value ht)
              (setcar (cddr c) (cons key (delete key (nth 2 c)))))
          (when (>= (hash-table-count ht) (nth 0 c))
            (let ((lru (car (last (nth 2 c)))))
              (remhash lru ht)
              (setcar (cddr c) (butlast (nth 2 c)))))
          (puthash key value ht)
          (setcar (cddr c) (cons key (nth 2 c)))))))

  (unwind-protect
      (let ((c (funcall 'neovm--lru2-create 3)))
        (funcall 'neovm--lru2-put c "x" 10)
        (funcall 'neovm--lru2-put c "y" 20)
        (funcall 'neovm--lru2-put c "z" 30)
        ;; Update "x" with new value -> moves to front, no eviction
        (funcall 'neovm--lru2-put c "x" 100)
        (list
          (funcall 'neovm--lru2-get c "x")     ;; 100 (updated)
          (funcall 'neovm--lru2-get c "y")     ;; 20
          (funcall 'neovm--lru2-get c "z")     ;; 30
          (nth 2 c)                            ;; order after accesses
          (hash-table-count (nth 1 c))         ;; still 3
          ;; Now add new key, "y" is LRU (accessed earliest)
          (progn
            (funcall 'neovm--lru2-put c "w" 40)
            (list (funcall 'neovm--lru2-get c "y")   ;; nil (evicted)
                  (funcall 'neovm--lru2-get c "w")   ;; 40
                  (hash-table-count (nth 1 c))))))    ;; 3
    (fmakunbound 'neovm--lru2-create)
    (fmakunbound 'neovm--lru2-get)
    (fmakunbound 'neovm--lru2-put)))"#;
    let expect = expect_test::expect![[r#""OK (100 20 30 (\"z\" \"y\" \"x\") 3 (20 40 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// LRU cache: eviction order correctness
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lru_cache_eviction_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--lru3-create
    (lambda (cap) (list cap (make-hash-table :test 'equal) nil)))

  (fset 'neovm--lru3-get
    (lambda (c key)
      (let ((val (gethash key (nth 1 c))))
        (when val
          (setcar (cddr c) (cons key (delete key (nth 2 c)))))
        val)))

  (fset 'neovm--lru3-put
    (lambda (c key value)
      (let ((ht (nth 1 c)))
        (if (gethash key ht)
            (progn
              (puthash key value ht)
              (setcar (cddr c) (cons key (delete key (nth 2 c)))))
          (when (>= (hash-table-count ht) (nth 0 c))
            (let ((lru (car (last (nth 2 c)))))
              (remhash lru ht)
              (setcar (cddr c) (butlast (nth 2 c)))))
          (puthash key value ht)
          (setcar (cddr c) (cons key (nth 2 c)))))))

  (unwind-protect
      (let ((c (funcall 'neovm--lru3-create 4))
            (eviction-log nil))
        ;; Fill cache: a b c d
        (funcall 'neovm--lru3-put c "a" 1)
        (funcall 'neovm--lru3-put c "b" 2)
        (funcall 'neovm--lru3-put c "c" 3)
        (funcall 'neovm--lru3-put c "d" 4)
        ;; Access in order: b, d (making a, c the LRU candidates)
        (funcall 'neovm--lru3-get c "b")
        (funcall 'neovm--lru3-get c "d")
        ;; Order now: d b c a (MRU to LRU)
        (let ((order-before (copy-sequence (nth 2 c))))
          ;; Add "e" -> evicts "a" (LRU)
          (funcall 'neovm--lru3-put c "e" 5)
          (setq eviction-log (cons (list "evict-a" (funcall 'neovm--lru3-get c "a")) eviction-log))
          ;; Add "f" -> evicts "c" (now LRU)
          (funcall 'neovm--lru3-put c "f" 6)
          (setq eviction-log (cons (list "evict-c" (funcall 'neovm--lru3-get c "c")) eviction-log))
          (list
            order-before
            (nreverse eviction-log)
            (nth 2 c)
            ;; Remaining values
            (funcall 'neovm--lru3-get c "b")
            (funcall 'neovm--lru3-get c "d")
            (funcall 'neovm--lru3-get c "e")
            (funcall 'neovm--lru3-get c "f"))))
    (fmakunbound 'neovm--lru3-create)
    (fmakunbound 'neovm--lru3-get)
    (fmakunbound 'neovm--lru3-put)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"d\" \"b\" \"c\" \"a\") ((\"evict-a\" nil) (\"evict-c\" nil)) (\"f\") 2 4 5 6)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: cache hit/miss ratio tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lru_cache_hit_miss_ratio() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Cache with hit/miss counters: (capacity ht order hits misses)
  (fset 'neovm--lrus-create
    (lambda (cap) (list cap (make-hash-table :test 'equal) nil 0 0)))

  (fset 'neovm--lrus-get
    (lambda (c key)
      (let ((val (gethash key (nth 1 c))))
        (if val
            (progn
              (setcar (cddr c) (cons key (delete key (nth 2 c))))
              (setcar (nthcdr 3 c) (1+ (nth 3 c)))  ;; hits++
              val)
          (setcar (nthcdr 4 c) (1+ (nth 4 c)))  ;; misses++
          nil))))

  (fset 'neovm--lrus-put
    (lambda (c key value)
      (let ((ht (nth 1 c)))
        (if (gethash key ht)
            (progn
              (puthash key value ht)
              (setcar (cddr c) (cons key (delete key (nth 2 c)))))
          (when (>= (hash-table-count ht) (nth 0 c))
            (let ((lru (car (last (nth 2 c)))))
              (remhash lru ht)
              (setcar (cddr c) (butlast (nth 2 c)))))
          (puthash key value ht)
          (setcar (cddr c) (cons key (nth 2 c)))))))

  (fset 'neovm--lrus-stats
    (lambda (c)
      (let ((hits (nth 3 c))
            (misses (nth 4 c)))
        (list hits misses (+ hits misses)
              (if (> (+ hits misses) 0)
                  (/ (* 100 hits) (+ hits misses))
                0)))))

  (unwind-protect
      (let ((c (funcall 'neovm--lrus-create 3)))
        ;; Fill cache
        (funcall 'neovm--lrus-put c "a" 1)
        (funcall 'neovm--lrus-put c "b" 2)
        (funcall 'neovm--lrus-put c "c" 3)
        ;; Access pattern: hit hit miss hit miss hit
        (funcall 'neovm--lrus-get c "a")  ;; hit
        (funcall 'neovm--lrus-get c "b")  ;; hit
        (funcall 'neovm--lrus-get c "z")  ;; miss
        (funcall 'neovm--lrus-get c "c")  ;; hit
        (funcall 'neovm--lrus-get c "q")  ;; miss
        (funcall 'neovm--lrus-get c "a")  ;; hit
        (let ((stats-1 (funcall 'neovm--lrus-stats c)))
          ;; Evict and test more
          (funcall 'neovm--lrus-put c "d" 4)  ;; evicts "b"
          (funcall 'neovm--lrus-get c "b")    ;; miss (evicted)
          (funcall 'neovm--lrus-get c "d")    ;; hit
          (let ((stats-2 (funcall 'neovm--lrus-stats c)))
            (list stats-1 stats-2))))
    (fmakunbound 'neovm--lrus-create)
    (fmakunbound 'neovm--lrus-get)
    (fmakunbound 'neovm--lrus-put)
    (fmakunbound 'neovm--lrus-stats)))"#;
    let expect = expect_test::expect![[r#""OK ((4 2 6 66) (5 3 8 62))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: LRU cache with time-to-live (TTL)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lru_cache_ttl() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulated TTL: each entry stores (value . timestamp).
    // A "current-time" counter is manually advanced.
    let form = r#"(progn
  ;; TTL cache: (capacity ht order ttl current-time)
  (fset 'neovm--lrut-create
    (lambda (cap ttl)
      (list cap (make-hash-table :test 'equal) nil ttl 0)))

  (fset 'neovm--lrut-advance-time
    (lambda (c delta)
      (setcar (nthcdr 4 c) (+ (nth 4 c) delta))))

  (fset 'neovm--lrut-put
    (lambda (c key value)
      (let ((ht (nth 1 c))
            (now (nth 4 c)))
        (if (gethash key ht)
            (progn
              (puthash key (cons value now) ht)
              (setcar (cddr c) (cons key (delete key (nth 2 c)))))
          (when (>= (hash-table-count ht) (nth 0 c))
            (let ((lru (car (last (nth 2 c)))))
              (remhash lru ht)
              (setcar (cddr c) (butlast (nth 2 c)))))
          (puthash key (cons value now) ht)
          (setcar (cddr c) (cons key (nth 2 c)))))))

  (fset 'neovm--lrut-get
    (lambda (c key)
      "Get value for KEY. Returns nil if not found or expired."
      (let* ((ht (nth 1 c))
             (entry (gethash key ht))
             (ttl (nth 3 c))
             (now (nth 4 c)))
        (if entry
            (if (> (- now (cdr entry)) ttl)
                ;; Expired: remove it
                (progn
                  (remhash key ht)
                  (setcar (cddr c) (delete key (nth 2 c)))
                  nil)
              ;; Valid: move to front
              (setcar (cddr c) (cons key (delete key (nth 2 c))))
              (car entry))
          nil))))

  (unwind-protect
      (let ((c (funcall 'neovm--lrut-create 4 10)))
        ;; Insert at time=0
        (funcall 'neovm--lrut-put c "a" 100)
        (funcall 'neovm--lrut-put c "b" 200)
        ;; Advance time by 5
        (funcall 'neovm--lrut-advance-time c 5)
        (funcall 'neovm--lrut-put c "c" 300)
        ;; Get "a" at time=5 (age=5, ttl=10 -> valid)
        (let ((get-a-5 (funcall 'neovm--lrut-get c "a")))
          ;; Advance time to 12
          (funcall 'neovm--lrut-advance-time c 7)
          ;; Get "a" at time=12 (age=12, ttl=10 -> expired)
          (let ((get-a-12 (funcall 'neovm--lrut-get c "a")))
            ;; Get "b" at time=12 (age=12, ttl=10 -> expired)
            (let ((get-b-12 (funcall 'neovm--lrut-get c "b")))
              ;; Get "c" at time=12 (age=7, ttl=10 -> valid)
              (let ((get-c-12 (funcall 'neovm--lrut-get c "c")))
                (list
                  get-a-5     ;; 100 (valid)
                  get-a-12    ;; nil (expired)
                  get-b-12    ;; nil (expired)
                  get-c-12    ;; 300 (valid)
                  (hash-table-count (nth 1 c))  ;; 1 (only "c" remains)
                  (nth 2 c)))))))               ;; order list
    (fmakunbound 'neovm--lrut-create)
    (fmakunbound 'neovm--lrut-advance-time)
    (fmakunbound 'neovm--lrut-put)
    (fmakunbound 'neovm--lrut-get)))"#;
    let expect = expect_test::expect![[r#""OK (100 nil nil 300 1 (\"c\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: two-level cache (L1 small/fast + L2 larger/slow)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lru_cache_two_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // L1 is a small cache (cap=2). On miss, check L2 (cap=4).
    // On L2 hit, promote to L1. On L2 miss, fetch and insert into both.
    let form = r#"(progn
  (fset 'neovm--lru2l-create-level
    (lambda (cap) (list cap (make-hash-table :test 'equal) nil)))

  (fset 'neovm--lru2l-level-get
    (lambda (c key)
      (let ((val (gethash key (nth 1 c))))
        (when val
          (setcar (cddr c) (cons key (delete key (nth 2 c)))))
        val)))

  (fset 'neovm--lru2l-level-put
    (lambda (c key value)
      (let ((ht (nth 1 c)))
        (if (gethash key ht)
            (progn
              (puthash key value ht)
              (setcar (cddr c) (cons key (delete key (nth 2 c)))))
          (when (>= (hash-table-count ht) (nth 0 c))
            (let ((lru (car (last (nth 2 c)))))
              (remhash lru ht)
              (setcar (cddr c) (butlast (nth 2 c)))))
          (puthash key value ht)
          (setcar (cddr c) (cons key (nth 2 c)))))))

  ;; Two-level cache: (l1 l2 fetch-count)
  (fset 'neovm--lru2l-create
    (lambda (l1-cap l2-cap)
      (list (funcall 'neovm--lru2l-create-level l1-cap)
            (funcall 'neovm--lru2l-create-level l2-cap)
            0)))

  (fset 'neovm--lru2l-get
    (lambda (cache key fetch-fn)
      "Get KEY. Check L1, then L2, then call FETCH-FN."
      (let ((l1 (nth 0 cache))
            (l2 (nth 1 cache)))
        ;; Try L1
        (let ((v1 (funcall 'neovm--lru2l-level-get l1 key)))
          (if v1
              (cons 'l1-hit v1)
            ;; Try L2
            (let ((v2 (funcall 'neovm--lru2l-level-get l2 key)))
              (if v2
                  (progn
                    ;; Promote to L1
                    (funcall 'neovm--lru2l-level-put l1 key v2)
                    (cons 'l2-hit v2))
                ;; Fetch from source
                (let ((fetched (funcall fetch-fn key)))
                  (setcar (cddr cache) (1+ (nth 2 cache)))
                  (funcall 'neovm--lru2l-level-put l1 key fetched)
                  (funcall 'neovm--lru2l-level-put l2 key fetched)
                  (cons 'fetched fetched)))))))))

  (unwind-protect
      (let ((cache (funcall 'neovm--lru2l-create 2 4))
            (results nil))
        ;; Fetch function: doubles the key number
        (let ((fetch-fn (lambda (k) (* (string-to-number k) 2))))
          ;; First access: all fetches
          (setq results (cons (funcall 'neovm--lru2l-get cache "1" fetch-fn) results))
          (setq results (cons (funcall 'neovm--lru2l-get cache "2" fetch-fn) results))
          (setq results (cons (funcall 'neovm--lru2l-get cache "3" fetch-fn) results))
          ;; "1" evicted from L1 (cap=2), still in L2 (cap=4)
          (setq results (cons (funcall 'neovm--lru2l-get cache "1" fetch-fn) results))  ;; L2 hit
          ;; "2" still in L1
          (setq results (cons (funcall 'neovm--lru2l-get cache "2" fetch-fn) results))  ;; L1 hit
          ;; New keys to fill L2
          (setq results (cons (funcall 'neovm--lru2l-get cache "4" fetch-fn) results))
          (setq results (cons (funcall 'neovm--lru2l-get cache "5" fetch-fn) results))
          (setq results (cons (funcall 'neovm--lru2l-get cache "6" fetch-fn) results))
          ;; "3" was evicted from both L1 and L2
          (setq results (cons (funcall 'neovm--lru2l-get cache "3" fetch-fn) results))  ;; fetched
          (list
            (nreverse results)
            (nth 2 cache))))  ;; total fetch count
    (fmakunbound 'neovm--lru2l-create-level)
    (fmakunbound 'neovm--lru2l-level-get)
    (fmakunbound 'neovm--lru2l-level-put)
    (fmakunbound 'neovm--lru2l-create)
    (fmakunbound 'neovm--lru2l-get)))"#;
    let expect = expect_test::expect![[
        r#""OK (((fetched . 2) (fetched . 4) (fetched . 6) (l2-hit . 2) (l2-hit . 4) (fetched . 8) (fetched . 10) (fetched . 12) (fetched . 6)) 7)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// LRU cache: capacity-1 edge case
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lru_cache_capacity_one() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--lru1-create
    (lambda (cap) (list cap (make-hash-table :test 'equal) nil)))

  (fset 'neovm--lru1-get
    (lambda (c key)
      (let ((val (gethash key (nth 1 c))))
        (when val
          (setcar (cddr c) (cons key (delete key (nth 2 c)))))
        val)))

  (fset 'neovm--lru1-put
    (lambda (c key value)
      (let ((ht (nth 1 c)))
        (if (gethash key ht)
            (progn
              (puthash key value ht)
              (setcar (cddr c) (cons key (delete key (nth 2 c)))))
          (when (>= (hash-table-count ht) (nth 0 c))
            (let ((lru (car (last (nth 2 c)))))
              (remhash lru ht)
              (setcar (cddr c) (butlast (nth 2 c)))))
          (puthash key value ht)
          (setcar (cddr c) (cons key (nth 2 c)))))))

  (unwind-protect
      (let ((c (funcall 'neovm--lru1-create 1)))
        (funcall 'neovm--lru1-put c "a" 1)
        (let ((r1 (funcall 'neovm--lru1-get c "a")))   ;; 1
          (funcall 'neovm--lru1-put c "b" 2)            ;; evicts "a"
          (let ((r2 (funcall 'neovm--lru1-get c "a"))   ;; nil
                (r3 (funcall 'neovm--lru1-get c "b")))  ;; 2
            ;; Update existing
            (funcall 'neovm--lru1-put c "b" 20)
            (let ((r4 (funcall 'neovm--lru1-get c "b")))  ;; 20
              (funcall 'neovm--lru1-put c "c" 3)           ;; evicts "b"
              (list r1 r2 r3 r4
                    (funcall 'neovm--lru1-get c "b")       ;; nil
                    (funcall 'neovm--lru1-get c "c")       ;; 3
                    (hash-table-count (nth 1 c))           ;; 1
                    (nth 2 c))))))                         ;; ("c")
    (fmakunbound 'neovm--lru1-create)
    (fmakunbound 'neovm--lru1-get)
    (fmakunbound 'neovm--lru1-put)))"#;
    let expect = expect_test::expect![[r#""OK (1 nil 2 20 nil 3 1 (\"c\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
