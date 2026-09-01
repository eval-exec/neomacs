//! Oracle parity tests for caching strategy implementations:
//! LRU cache, LFU cache, TTL-based cache, write-through vs
//! write-back simulation, multi-level cache, and cache statistics.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// LRU cache with fixed capacity
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cache_lru_fixed_capacity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // LRU cache: on access, move to front. On eviction, drop tail.
    // Cache is an alist (key . value), ordered by recency.
    let form = r#"(let ((make-lru
           (lambda (capacity)
             (list nil capacity 0 0)))  ;; (entries capacity hits misses)
          (lru-get nil)
          (lru-put nil)
          (lru-entries (lambda (c) (car c)))
          (lru-stats (lambda (c) (list :hits (nth 2 c) :misses (nth 3 c)
                                       :size (length (car c))
                                       :capacity (cadr c)))))
      ;; lru-get: if found, move to front, increment hits. Else miss.
      (setq lru-get
            (lambda (cache key)
              (let ((entries (car cache))
                    (cap (cadr cache))
                    (hits (nth 2 cache))
                    (misses (nth 3 cache))
                    (found nil)
                    (rest nil))
                (dolist (e entries)
                  (if (equal (car e) key)
                      (setq found e)
                    (setq rest (cons e rest))))
                (if found
                    (list (cdr found)
                          (list (cons found (nreverse rest))
                                cap (1+ hits) misses))
                  (list nil (list entries cap hits (1+ misses)))))))
      ;; lru-put: add to front, evict tail if over capacity
      (setq lru-put
            (lambda (cache key value)
              (let ((entries (car cache))
                    (cap (cadr cache))
                    (hits (nth 2 cache))
                    (misses (nth 3 cache))
                    (new-entries nil))
                ;; Remove existing entry with same key
                (dolist (e entries)
                  (unless (equal (car e) key)
                    (setq new-entries (cons e new-entries))))
                (setq new-entries (cons (cons key value) (nreverse new-entries)))
                ;; Evict if over capacity
                (when (> (length new-entries) cap)
                  (setq new-entries (butlast new-entries
                                             (- (length new-entries) cap))))
                (list new-entries cap hits misses))))
      ;; Test
      (let ((c (funcall make-lru 3)))
        ;; Fill cache
        (setq c (funcall lru-put c "a" 1))
        (setq c (funcall lru-put c "b" 2))
        (setq c (funcall lru-put c "c" 3))
        (let ((s1 (mapcar #'car (funcall lru-entries c))))  ;; c b a
          ;; Access "a" → moves to front
          (let ((r1 (funcall lru-get c "a")))
            (setq c (cadr r1))
            (let ((s2 (mapcar #'car (funcall lru-entries c)))) ;; a c b
              ;; Add "d" → evicts "b" (LRU)
              (setq c (funcall lru-put c "d" 4))
              (let ((s3 (mapcar #'car (funcall lru-entries c)))) ;; d a c
                ;; Miss on "b"
                (let ((r2 (funcall lru-get c "b")))
                  (setq c (cadr r2))
                  ;; Hit on "d"
                  (let ((r3 (funcall lru-get c "d")))
                    (setq c (cadr r3))
                    (list s1 s2 s3
                          (car r1)   ;; value of "a" = 1
                          (car r2)   ;; nil (miss)
                          (car r3)   ;; value of "d" = 4
                          (funcall lru-stats c))))))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"c\" \"b\" \"a\") (\"a\" \"c\" \"b\") (\"d\" \"a\" \"c\") 1 nil 4 (:hits 2 :misses 1 :size 3 :capacity 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// LFU cache: least frequently used eviction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cache_lfu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // LFU cache: each entry has a frequency counter.
    // On eviction, remove the entry with lowest frequency.
    let form = r#"(let ((make-lfu
           (lambda (capacity)
             (list nil capacity)))  ;; (entries capacity)
          ;; entries: alist of (key value freq)
          (lfu-get nil)
          (lfu-put nil)
          (lfu-entries (lambda (c) (car c))))
      ;; lfu-get: increment freq if found
      (setq lfu-get
            (lambda (cache key)
              (let ((entries (car cache))
                    (cap (cadr cache))
                    (found nil)
                    (rest nil))
                (dolist (e entries)
                  (if (equal (car e) key)
                      (setq found (list (car e) (cadr e) (1+ (nth 2 e))))
                    (setq rest (cons e rest))))
                (if found
                    (list (cadr found)
                          (list (cons found (nreverse rest)) cap))
                  (list nil cache)))))
      ;; lfu-put: add with freq=1. If at capacity, evict min-freq entry.
      (setq lfu-put
            (lambda (cache key value)
              (let ((entries (car cache))
                    (cap (cadr cache))
                    (existing nil)
                    (rest nil))
                ;; Remove existing entry with same key
                (dolist (e entries)
                  (if (equal (car e) key)
                      (setq existing e)
                    (setq rest (cons e rest))))
                (let ((new-entries (nreverse rest))
                      (new-freq (if existing (1+ (nth 2 existing)) 1)))
                  ;; If at capacity (and not updating), evict lowest freq
                  (when (and (not existing) (>= (length new-entries) cap))
                    (let ((min-freq most-positive-fixnum)
                          (min-entry nil))
                      ;; Find min freq
                      (dolist (e new-entries)
                        (when (< (nth 2 e) min-freq)
                          (setq min-freq (nth 2 e))
                          (setq min-entry e)))
                      ;; Remove it
                      (let ((trimmed nil))
                        (let ((removed nil))
                          (dolist (e new-entries)
                            (if (and (not removed) (eq e min-entry))
                                (setq removed t)
                              (setq trimmed (cons e trimmed)))))
                        (setq new-entries (nreverse trimmed)))))
                  (list (cons (list key value new-freq) new-entries) cap)))))
      ;; Test
      (let ((c (funcall make-lfu 3)))
        (setq c (funcall lfu-put c "a" 10))
        (setq c (funcall lfu-put c "b" 20))
        (setq c (funcall lfu-put c "c" 30))
        ;; Access "a" twice, "b" once → a:freq=3, b:freq=2, c:freq=1
        (let ((r (funcall lfu-get c "a"))) (setq c (cadr r)))
        (let ((r (funcall lfu-get c "a"))) (setq c (cadr r)))
        (let ((r (funcall lfu-get c "b"))) (setq c (cadr r)))
        ;; Snapshot freqs
        (let ((freqs-before (mapcar (lambda (e) (list (car e) (nth 2 e)))
                                    (funcall lfu-entries c))))
          ;; Add "d" → should evict "c" (lowest freq=1)
          (setq c (funcall lfu-put c "d" 40))
          (let ((keys-after (mapcar #'car (funcall lfu-entries c))))
            ;; Verify "c" is gone
            (let ((r (funcall lfu-get c "c")))
              (list freqs-before
                    keys-after
                    (car r)   ;; nil: "c" was evicted
                    ;; "a" is still there
                    (car (funcall lfu-get c "a"))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"b\" 2) (\"a\" 3) (\"c\" 1)) (\"d\" \"b\" \"a\") nil 10)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// TTL-based cache (expiry by "timestamp")
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cache_ttl() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulated clock. Each entry has an expiry time.
    // get checks expiry and returns nil + removes entry if expired.
    let form = r#"(let ((make-ttl-cache
           (lambda () (list nil 0)))  ;; (entries current-time)
          (ttl-set-time
           (lambda (cache time) (list (car cache) time)))
          (ttl-put nil)
          (ttl-get nil)
          (ttl-cleanup nil)
          (ttl-keys (lambda (c) (mapcar #'car (car c)))))
      ;; ttl-put: store (key value expiry-time)
      (setq ttl-put
            (lambda (cache key value ttl)
              (let ((now (cadr cache))
                    (entries (car cache))
                    (filtered nil))
                ;; Remove existing entry with same key
                (dolist (e entries)
                  (unless (equal (car e) key)
                    (setq filtered (cons e filtered))))
                (list (cons (list key value (+ now ttl)) (nreverse filtered))
                      now))))
      ;; ttl-get: return value if not expired, else nil and remove
      (setq ttl-get
            (lambda (cache key)
              (let ((now (cadr cache))
                    (entries (car cache))
                    (found nil)
                    (rest nil))
                (dolist (e entries)
                  (if (equal (car e) key)
                      (if (> (nth 2 e) now)
                          (setq found e)
                        nil)  ;; expired, don't keep it
                    (setq rest (cons e rest))))
                (if found
                    (list (cadr found)
                          (list (cons found (nreverse rest)) now))
                  (list nil (list (nreverse rest) now))))))
      ;; ttl-cleanup: remove all expired entries
      (setq ttl-cleanup
            (lambda (cache)
              (let ((now (cadr cache))
                    (alive nil))
                (dolist (e (car cache))
                  (when (> (nth 2 e) now)
                    (setq alive (cons e alive))))
                (list (nreverse alive) now))))
      ;; Test
      (let ((c (funcall make-ttl-cache)))
        ;; At time 0: add entries with different TTLs
        (setq c (funcall ttl-put c "short" "val-short" 5))
        (setq c (funcall ttl-put c "medium" "val-medium" 10))
        (setq c (funcall ttl-put c "long" "val-long" 20))
        (let ((keys-t0 (funcall ttl-keys c)))
          ;; Advance to time 3: all still valid
          (setq c (funcall ttl-set-time c 3))
          (let ((r1 (funcall ttl-get c "short")))
            (setq c (cadr r1))
            ;; Advance to time 7: "short" expired
            (setq c (funcall ttl-set-time c 7))
            (let ((r2 (funcall ttl-get c "short")))
              (setq c (cadr r2))
              (let ((r3 (funcall ttl-get c "medium")))
                (setq c (cadr r3))
                ;; Advance to time 15: "medium" also expired
                (setq c (funcall ttl-set-time c 15))
                (setq c (funcall ttl-cleanup c))
                (let ((keys-t15 (funcall ttl-keys c)))
                  (list keys-t0
                        (car r1)    ;; "val-short" at t=3
                        (car r2)    ;; nil at t=7 (expired)
                        (car r3)    ;; "val-medium" at t=7
                        keys-t15    ;; only "long" remains
                        ))))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"long\" \"medium\" \"short\") \"val-short\" nil \"val-medium\" (\"long\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Write-through vs write-back simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cache_write_through_vs_write_back() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Write-through: every write goes to both cache and "store" immediately.
    // Write-back: writes go to cache only; flushed to store on demand.
    let form = r#"(let ((make-store (lambda () nil))  ;; alist
          (store-get (lambda (store key) (cdr (assoc key store))))
          (store-put (lambda (store key val)
                       (let ((filtered nil))
                         (dolist (e store)
                           (unless (equal (car e) key)
                             (setq filtered (cons e filtered))))
                         (cons (cons key val) (nreverse filtered)))))
          (make-wt-cache nil)
          (wt-get nil) (wt-put nil)
          (make-wb-cache nil)
          (wb-get nil) (wb-put nil) (wb-flush nil))
      ;; Write-through cache
      (setq make-wt-cache (lambda (store) (list nil store nil)))
      (setq wt-get
            (lambda (cache key store-get-fn)
              (let ((cached (assoc key (car cache))))
                (if cached
                    (list (cdr cached) cache 'hit)
                  (let ((val (funcall store-get-fn (cadr cache) key)))
                    (list val
                          (list (cons (cons key val) (car cache))
                                (cadr cache) (nth 2 cache))
                          'miss))))))
      (setq wt-put
            (lambda (cache key val store-put-fn)
              ;; Write to BOTH cache and store
              (let ((new-cache-entries
                     (let ((f nil))
                       (dolist (e (car cache))
                         (unless (equal (car e) key)
                           (setq f (cons e f))))
                       (cons (cons key val) (nreverse f))))
                    (new-store (funcall store-put-fn (cadr cache) key val)))
                (list new-cache-entries new-store
                      (cons (list 'wt-write key) (nth 2 cache))))))
      ;; Write-back cache
      (setq make-wb-cache (lambda (store) (list nil store nil nil)))  ;; +dirty-keys
      (setq wb-get
            (lambda (cache key store-get-fn)
              (let ((cached (assoc key (car cache))))
                (if cached
                    (list (cdr cached) cache 'hit)
                  (let ((val (funcall store-get-fn (cadr cache) key)))
                    (list val
                          (list (cons (cons key val) (car cache))
                                (cadr cache) (nth 2 cache) (nth 3 cache))
                          'miss))))))
      (setq wb-put
            (lambda (cache key val)
              ;; Write to cache only, mark as dirty
              (let ((new-entries
                     (let ((f nil))
                       (dolist (e (car cache))
                         (unless (equal (car e) key)
                           (setq f (cons e f))))
                       (cons (cons key val) (nreverse f))))
                    (dirty (if (member key (nth 3 cache))
                               (nth 3 cache)
                             (cons key (nth 3 cache)))))
                (list new-entries (cadr cache)
                      (cons (list 'wb-write key) (nth 2 cache))
                      dirty))))
      (setq wb-flush
            (lambda (cache store-put-fn)
              ;; Flush all dirty entries to store
              (let ((store (cadr cache))
                    (flushed 0))
                (dolist (key (nth 3 cache))
                  (let ((val (cdr (assoc key (car cache)))))
                    (setq store (funcall store-put-fn store key val))
                    (setq flushed (1+ flushed))))
                (list (car cache) store
                      (cons (list 'flush flushed) (nth 2 cache))
                      nil))))  ;; dirty cleared
      ;; Test both strategies
      (let ((store (funcall make-store)))
        (setq store (funcall store-put store "x" 1))
        (setq store (funcall store-put store "y" 2))
        ;; Write-through
        (let ((wt (funcall make-wt-cache store)))
          (setq wt (funcall wt-put wt "x" 10 store-put))
          (setq wt (funcall wt-put wt "z" 30 store-put))
          (let ((wt-store-x (funcall store-get (cadr wt) "x"))
                (wt-store-z (funcall store-get (cadr wt) "z")))
            ;; Write-back
            (let ((wb (funcall make-wb-cache store)))
              (setq wb (funcall wb-put wb "x" 100))
              (setq wb (funcall wb-put wb "w" 400))
              (let ((wb-store-x-before (funcall store-get (cadr wb) "x"))
                    (wb-dirty (nth 3 wb)))
                (setq wb (funcall wb-flush wb store-put))
                (let ((wb-store-x-after (funcall store-get (cadr wb) "x"))
                      (wb-dirty-after (nth 3 wb)))
                  (list
                   ;; Write-through: store is immediately updated
                   wt-store-x   ;; 10
                   wt-store-z   ;; 30
                   ;; Write-back: store NOT updated until flush
                   wb-store-x-before  ;; 1 (old value)
                   wb-dirty           ;; ("w" "x")
                   ;; After flush
                   wb-store-x-after   ;; 100
                   wb-dirty-after     ;; nil
                   ))))))))"#;
    let expect = expect_test::expect![[r#""OK (10 30 1 (\"w\" \"x\") 100 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-level cache (L1 fast/small, L2 slow/large)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cache_multi_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // L1: capacity 2, L2: capacity 4, backing store.
    // Read: try L1, then L2 (promote to L1), then store (populate both).
    // L1 evicts to L2 on overflow.
    let form = r#"(let ((make-level
           (lambda (capacity) (list nil capacity 0 0)))  ;; (entries cap hits misses)
          (level-get nil)
          (level-put nil)
          (level-evict-lru nil)
          (level-entries (lambda (l) (car l)))
          (level-stats (lambda (l) (list :hits (nth 2 l) :misses (nth 3 l)))))
      ;; Simple alist-based level operations
      (setq level-get
            (lambda (level key)
              (let ((entry (assoc key (car level))))
                (if entry
                    ;; Move to front (most recent)
                    (let ((rest nil))
                      (dolist (e (car level))
                        (unless (equal (car e) key)
                          (setq rest (cons e rest))))
                      (list (cdr entry)
                            (list (cons entry (nreverse rest))
                                  (cadr level)
                                  (1+ (nth 2 level))
                                  (nth 3 level))))
                  (list nil (list (car level) (cadr level)
                                 (nth 2 level) (1+ (nth 3 level))))))))
      (setq level-put
            (lambda (level key value)
              (let ((filtered nil)
                    (evicted nil))
                (dolist (e (car level))
                  (unless (equal (car e) key)
                    (setq filtered (cons e filtered))))
                (let ((new-entries (cons (cons key value) (nreverse filtered))))
                  (when (> (length new-entries) (cadr level))
                    ;; Evict the last (LRU) entry
                    (setq evicted (car (last new-entries)))
                    (setq new-entries (butlast new-entries)))
                  (list (list new-entries (cadr level)
                              (nth 2 level) (nth 3 level))
                        evicted)))))
      ;; Multi-level get
      (let ((ml-get nil)
            (store '(("a" . 1) ("b" . 2) ("c" . 3)
                     ("d" . 4) ("e" . 5) ("f" . 6))))
        (setq ml-get
              (lambda (l1 l2 key)
                ;; Try L1
                (let ((r1 (funcall level-get l1 key)))
                  (if (car r1)
                      (list (car r1) (cadr r1) l2 'L1-hit)
                    (setq l1 (cadr r1))
                    ;; Try L2
                    (let ((r2 (funcall level-get l2 key)))
                      (if (car r2)
                          ;; Promote to L1
                          (let ((promoted (funcall level-put l1 key (car r2))))
                            (let ((new-l1 (car promoted))
                                  (evicted (cadr promoted)))
                              ;; If L1 evicted something, push to L2
                              (let ((new-l2 (cadr r2)))
                                (when evicted
                                  (let ((ev-result (funcall level-put new-l2
                                                            (car evicted)
                                                            (cdr evicted))))
                                    (setq new-l2 (car ev-result))))
                                (list (car r2) new-l1 new-l2 'L2-hit))))
                        (setq l2 (cadr r2))
                        ;; Fetch from store
                        (let ((val (cdr (assoc key store))))
                          (if val
                              (let ((p1 (funcall level-put l1 key val)))
                                (let ((new-l1 (car p1))
                                      (evicted (cadr p1)))
                                  (let ((new-l2 l2))
                                    (when evicted
                                      (let ((p2 (funcall level-put new-l2
                                                          (car evicted)
                                                          (cdr evicted))))
                                        (setq new-l2 (car p2))))
                                    (list val new-l1 new-l2 'store-hit))))
                            (list nil l1 l2 'miss)))))))))
        ;; Test
        (let ((l1 (funcall make-level 2))
              (l2 (funcall make-level 4))
              (access-log nil))
          ;; Access sequence
          (dolist (key '("a" "b" "c" "a" "d" "b" "e"))
            (let ((r (funcall ml-get l1 l2 key)))
              (setq l1 (cadr r))
              (setq l2 (nth 2 r))
              (setq access-log
                    (cons (list key (nth 3 r)) access-log))))
          (list (nreverse access-log)
                (mapcar #'car (funcall level-entries l1))
                (mapcar #'car (funcall level-entries l2))
                (funcall level-stats l1)
                (funcall level-stats l2)))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"a\" store-hit) (\"b\" store-hit) (\"c\" store-hit) (\"a\" L2-hit) (\"d\" store-hit) (\"b\" L2-hit) (\"e\" store-hit)) (\"e\" \"b\") (\"d\" \"a\" \"b\" \"c\") (:hits 0 :misses 7) (:hits 2 :misses 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cache statistics: hit rate, miss rate, eviction tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cache_statistics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A cache that tracks detailed statistics: hits, misses, evictions,
    // hit rate calculation, per-key access counts.
    let form = r#"(let ((make-stats-cache
           (lambda (capacity)
             (list nil capacity 0 0 0 nil)))
          ;; (entries capacity hits misses evictions per-key-counts)
          (sc-get nil)
          (sc-put nil)
          (sc-hit-rate
           (lambda (cache)
             (let ((total (+ (nth 2 cache) (nth 3 cache))))
               (if (= total 0) 0
                 ;; Return as percentage * 100 (integer)
                 (/ (* (nth 2 cache) 10000) total)))))
          (sc-top-keys
           (lambda (cache n)
             "Return top N most-accessed keys."
             (let ((sorted (sort (copy-sequence (nth 5 cache))
                                 (lambda (a b) (> (cdr a) (cdr b))))))
               (let ((result nil) (count 0))
                 (while (and sorted (< count n))
                   (setq result (cons (car sorted) result))
                   (setq sorted (cdr sorted))
                   (setq count (1+ count)))
                 (nreverse result))))))
      ;; sc-get: lookup + stats tracking
      (setq sc-get
            (lambda (cache key)
              (let ((entries (car cache))
                    (cap (cadr cache))
                    (hits (nth 2 cache))
                    (misses (nth 3 cache))
                    (evictions (nth 4 cache))
                    (counts (copy-sequence (nth 5 cache)))
                    (found nil)
                    (rest nil))
                ;; Update per-key access count
                (let ((kc (assoc key counts)))
                  (if kc (setcdr kc (1+ (cdr kc)))
                    (setq counts (cons (cons key 1) counts))))
                ;; Lookup
                (dolist (e entries)
                  (if (equal (car e) key)
                      (setq found e)
                    (setq rest (cons e rest))))
                (if found
                    (list (cdr found)
                          (list (cons found (nreverse rest))
                                cap (1+ hits) misses evictions counts))
                  (list nil
                        (list entries cap hits (1+ misses) evictions counts))))))
      ;; sc-put: insert + eviction tracking
      (setq sc-put
            (lambda (cache key val)
              (let ((entries (car cache))
                    (cap (cadr cache))
                    (hits (nth 2 cache))
                    (misses (nth 3 cache))
                    (evictions (nth 4 cache))
                    (counts (nth 5 cache))
                    (filtered nil))
                (dolist (e entries)
                  (unless (equal (car e) key)
                    (setq filtered (cons e filtered))))
                (let ((new-entries (cons (cons key val) (nreverse filtered)))
                      (new-evictions evictions))
                  (when (> (length new-entries) cap)
                    (setq new-entries (butlast new-entries
                                               (- (length new-entries) cap)))
                    (setq new-evictions (1+ evictions)))
                  (list new-entries cap hits misses new-evictions counts)))))
      ;; Run workload
      (let ((c (funcall make-stats-cache 3)))
        ;; Populate
        (setq c (funcall sc-put c "x" 10))
        (setq c (funcall sc-put c "y" 20))
        (setq c (funcall sc-put c "z" 30))
        ;; Access pattern: x heavily, y moderately, z once, w (miss)
        (dolist (key '("x" "x" "x" "y" "y" "z" "w" "x" "w"))
          (let ((r (funcall sc-get c key)))
            (setq c (cadr r))))
        ;; Cause eviction
        (setq c (funcall sc-put c "a" 40))
        ;; More accesses
        (let ((r (funcall sc-get c "a")))
          (setq c (cadr r)))
        (let ((r (funcall sc-get c "z")))  ;; might be evicted
          (setq c (cadr r)))
        (list
         :hits (nth 2 c)
         :misses (nth 3 c)
         :evictions (nth 4 c)
         :hit-rate (funcall sc-hit-rate c)
         :top-3 (funcall sc-top-keys c 3)
         :total-accesses (+ (nth 2 c) (nth 3 c)))))"#;
    let expect = expect_test::expect![[
        r#""OK (:hits 9 :misses 2 :evictions 1 :hit-rate 8181 :top-3 ((\"x\" . 4) (\"w\" . 2) (\"z\" . 2)) :total-accesses 11)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
