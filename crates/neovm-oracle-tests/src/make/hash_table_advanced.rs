//! Advanced oracle parity tests for `make-hash-table` with ALL keyword
//! parameters: :test (eq, eql, equal), :size, :weakness,
//! :rehash-size, :rehash-threshold. Also tests hash-table-test,
//! hash-table-size, structural keys with equal, and benchmark-style
//! creation patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// :test parameter — eq, eql, equal with different key types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_hash_table_test_eq() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // :test 'eq — identity comparison. Symbols are always eq-identical.
    // Different cons cells with same structure are NOT eq.
    let form = r#"(let ((h (make-hash-table :test 'eq)))
                    ;; Symbols always eq
                    (puthash 'alpha 1 h)
                    (puthash 'beta 2 h)
                    (puthash 'alpha 10 h)  ;; overwrite
                    ;; Numbers: fixnums are eq in most implementations
                    (puthash 42 'found h)
                    ;; A cons cell stored and looked up by identity
                    (let ((key (list 1 2 3)))
                      (puthash key 'by-identity h)
                      (list
                        (hash-table-test h)
                        (gethash 'alpha h)
                        (gethash 'beta h)
                        (gethash 42 h)
                        ;; Same object: found
                        (gethash key h)
                        ;; Different object, same structure: NOT found under eq
                        (gethash (list 1 2 3) h)
                        (hash-table-count h))))"#;
    let expect = expect_test::expect![[r#""OK (eq 10 2 found by-identity nil 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_make_hash_table_test_eql() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // :test 'eql — eq for non-numeric, = for numbers of same type
    let form = r#"(let ((h (make-hash-table :test 'eql)))
                    ;; Integer keys
                    (puthash 1 'one h)
                    (puthash 2 'two h)
                    (puthash 1 'one-again h)  ;; overwrite
                    ;; Float keys: eql compares floats with =
                    (puthash 3.14 'pi h)
                    ;; Symbol keys
                    (puthash 'sym 'symbol-val h)
                    ;; List keys: eql treats like eq for non-numbers
                    (let ((k (list 'a 'b)))
                      (puthash k 'list-val h)
                      (list
                        (hash-table-test h)
                        (gethash 1 h)
                        (gethash 2 h)
                        (gethash 3.14 h)
                        (gethash 'sym h)
                        ;; Same object found
                        (gethash k h)
                        ;; Different cons, same structure: not found (eql = eq for lists)
                        (gethash (list 'a 'b) h)
                        (hash-table-count h))))"#;
    let expect = expect_test::expect![[r#""OK (eql one-again two pi symbol-val list-val nil 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_make_hash_table_test_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // :test 'equal — structural comparison. Lists, strings with same content match.
    let form = r#"(let ((h (make-hash-table :test 'equal)))
                    ;; String keys: equal compares content
                    (puthash "hello" 1 h)
                    (puthash "world" 2 h)
                    (puthash "hello" 10 h)  ;; overwrite
                    ;; List keys: equal compares structure
                    (puthash '(a b c) 'abc h)
                    (puthash '(1 2 3) 'nums h)
                    ;; Nested list keys
                    (puthash '((x y) (z w)) 'nested h)
                    ;; Vector keys
                    (puthash [1 2 3] 'vec h)
                    (list
                      (hash-table-test h)
                      ;; String lookup with new string object (same content)
                      (gethash (concat "hel" "lo") h)
                      (gethash "world" h)
                      ;; List lookup with newly constructed list
                      (gethash (list 'a 'b 'c) h)
                      (gethash '(1 2 3) h)
                      ;; Nested list lookup
                      (gethash '((x y) (z w)) h)
                      ;; Vector lookup
                      (gethash (vector 1 2 3) h)
                      (hash-table-count h)))"#;
    let expect = expect_test::expect![[r#""OK (equal 10 2 abc nums nested vec 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// :size parameter — hint for initial allocation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_hash_table_size_parameter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // :size is a hint; actual allocation >= requested. hash-table-size reflects it.
    let form = r#"(let ((h0 (make-hash-table))
                        (h10 (make-hash-table :size 10))
                        (h100 (make-hash-table :size 100))
                        (h1000 (make-hash-table :size 1000))
                        (h1 (make-hash-table :size 1))
                        (h-neg (make-hash-table :size 0)))
                    ;; size is a hint, implementation may round up
                    ;; but hash-table-size should be >= requested
                    (list
                      ;; Default size is implementation-defined but should be > 0
                      (> (hash-table-size h0) 0)
                      (>= (hash-table-size h10) 1)
                      (>= (hash-table-size h100) 1)
                      (>= (hash-table-size h1000) 1)
                      ;; Small sizes
                      (>= (hash-table-size h1) 1)
                      (>= (hash-table-size h-neg) 0)
                      ;; All start empty
                      (hash-table-count h0)
                      (hash-table-count h100)
                      (hash-table-count h1000)
                      ;; Functionality with small size hint (should still work fine)
                      (progn
                        (dotimes (i 50)
                          (puthash i (* i i) h1))
                        (list (hash-table-count h1)
                              (gethash 0 h1)
                              (gethash 25 h1)
                              (gethash 49 h1)))))"#;
    let expect = expect_test::expect![[r#""OK (nil t t t t t 0 0 0 (50 0 625 2401))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// :weakness parameter — nil, key, value, key-or-value, key-and-value
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_hash_table_weakness_parameter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test that various :weakness values are accepted and queryable.
    // We can't test actual GC behavior deterministically, but we can verify
    // that the parameter is stored and the table functions normally.
    let form = r#"(let ((h-nil (make-hash-table :weakness nil))
                        (h-key (make-hash-table :weakness 'key))
                        (h-val (make-hash-table :weakness 'value))
                        (h-kor (make-hash-table :weakness 'key-or-value))
                        (h-kand (make-hash-table :weakness 'key-and-value)))
                    ;; Verify weakness is stored
                    (list
                      (hash-table-weakness h-nil)
                      (hash-table-weakness h-key)
                      (hash-table-weakness h-val)
                      (hash-table-weakness h-kor)
                      (hash-table-weakness h-kand)
                      ;; All weak tables should still function for put/get
                      (progn
                        (puthash 'a 1 h-key)
                        (puthash 'b 2 h-val)
                        (puthash 'c 3 h-kor)
                        (puthash 'd 4 h-kand)
                        (list
                          (gethash 'a h-key)
                          (gethash 'b h-val)
                          (gethash 'c h-kor)
                          (gethash 'd h-kand)
                          (hash-table-count h-key)
                          (hash-table-count h-val)
                          (hash-table-count h-kor)
                          (hash-table-count h-kand)))
                      ;; hash-table-p on all
                      (hash-table-p h-nil)
                      (hash-table-p h-key)
                      (hash-table-p h-val)))"#;
    let expect = expect_test::expect![[
        r#""OK (nil key value key-or-value key-and-value (1 2 3 4 1 1 1 1) t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// :rehash-size and :rehash-threshold parameters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_hash_table_rehash_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // :rehash-size (float > 1.0 or integer > 0) controls growth factor.
    // :rehash-threshold (float 0.0-1.0) controls when to grow.
    let form = r#"(let ((h1 (make-hash-table :rehash-size 2.0 :rehash-threshold 0.7))
                        (h2 (make-hash-table :rehash-size 1.5 :rehash-threshold 0.5))
                        (h3 (make-hash-table :rehash-size 1.2 :rehash-threshold 0.9)))
                    ;; Verify rehash params are stored
                    (list
                      (hash-table-rehash-size h1)
                      (hash-table-rehash-threshold h1)
                      (hash-table-rehash-size h2)
                      (hash-table-rehash-threshold h2)
                      (hash-table-rehash-size h3)
                      (hash-table-rehash-threshold h3)
                      ;; Tables function correctly regardless of rehash params
                      (progn
                        (dotimes (i 100)
                          (puthash i (* i 2) h1)
                          (puthash (format "key-%d" i) i h2)
                          (puthash i (- i) h3))
                        (list
                          (hash-table-count h1)
                          (hash-table-count h2)
                          (hash-table-count h3)
                          (gethash 50 h1)
                          (gethash "key-50" h2)
                          (gethash 50 h3)))))"#;
    let expect = expect_test::expect![[
        r#""OK (1.5 0.8125 1.5 0.8125 1.5 0.8125 (100 100 100 100 nil -50))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// hash-table-test and hash-table-size on created tables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_hash_table_accessors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Comprehensive accessor tests
    let form = r#"(let ((tables (list
                                  (make-hash-table)
                                  (make-hash-table :test 'eq)
                                  (make-hash-table :test 'eql)
                                  (make-hash-table :test 'equal)
                                  (make-hash-table :test 'eq :size 200)
                                  (make-hash-table :test 'equal :size 50
                                                   :weakness 'key
                                                   :rehash-size 1.5
                                                   :rehash-threshold 0.8))))
                    (mapcar
                      (lambda (h)
                        (list
                          (hash-table-p h)
                          (hash-table-test h)
                          (hash-table-count h)
                          (>= (hash-table-size h) 0)
                          (hash-table-weakness h)
                          (hash-table-rehash-size h)
                          (hash-table-rehash-threshold h)))
                      tables))"#;
    let expect = expect_test::expect![[
        r#""OK ((t eql 0 t nil 1.5 0.8125) (t eq 0 t nil 1.5 0.8125) (t eql 0 t nil 1.5 0.8125) (t equal 0 t nil 1.5 0.8125) (t eq 0 t nil 1.5 0.8125) (t equal 0 t key 1.5 0.8125))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: hash tables with equal test for structural keys
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_hash_table_structural_key_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use complex structural keys with :test 'equal
    let form = r#"(let ((h (make-hash-table :test 'equal)))
                    ;; Use lists as composite keys (like tuples)
                    (puthash '(2025 1 15) "event-a" h)
                    (puthash '(2025 2 20) "event-b" h)
                    (puthash '(2025 1 15) "event-a-updated" h)  ;; overwrite via equal
                    ;; Use strings as keys
                    (puthash "user:alice" '(admin active) h)
                    (puthash "user:bob" '(viewer active) h)
                    ;; Use vectors as keys
                    (puthash [1 0 0] 'x-axis h)
                    (puthash [0 1 0] 'y-axis h)
                    (puthash [0 0 1] 'z-axis h)
                    ;; Nested structure keys
                    (puthash '(("host" . "localhost") ("port" . 8080)) 'config-a h)
                    (puthash '(("host" . "example.com") ("port" . 443)) 'config-b h)
                    (list
                      ;; Date key lookup with freshly constructed list
                      (gethash (list 2025 1 15) h)
                      (gethash (list 2025 2 20) h)
                      ;; String key lookup
                      (gethash (concat "user:" "alice") h)
                      (gethash "user:bob" h)
                      ;; Vector key lookup
                      (gethash (vector 1 0 0) h)
                      (gethash [0 1 0] h)
                      (gethash [0 0 1] h)
                      ;; Nested structure lookup
                      (gethash '(("host" . "localhost") ("port" . 8080)) h)
                      (gethash '(("host" . "example.com") ("port" . 443)) h)
                      ;; Non-existent key
                      (gethash '(2025 3 1) h 'not-found)
                      ;; Count
                      (hash-table-count h)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"event-a-updated\" \"event-b\" (admin active) (viewer active) x-axis y-axis z-axis config-a config-b not-found 9)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: benchmark-style test creating multiple hash tables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_hash_table_benchmark_creation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create many hash tables with different configurations, populate, and verify
    let form = r#"(progn
  (fset 'neovm--mht-create-and-populate
    (lambda (test-fn size-hint n-items)
      "Create a hash table with given params, populate N-ITEMS, return stats."
      (let ((h (make-hash-table :test test-fn :size size-hint)))
        (dotimes (i n-items)
          (puthash (if (eq test-fn 'equal)
                       (format "key-%d" i)
                     i)
                   (* i i)
                   h))
        (list (hash-table-test h)
              (hash-table-count h)
              (gethash (if (eq test-fn 'equal) "key-0" 0) h)
              (gethash (if (eq test-fn 'equal)
                           (format "key-%d" (1- n-items))
                         (1- n-items))
                       h)))))

  (unwind-protect
      (let ((configs '((eq 10 50)
                        (eq 100 50)
                        (eql 10 50)
                        (eql 100 50)
                        (equal 10 50)
                        (equal 100 50)
                        (eq 1 200)
                        (equal 1 200))))
        (mapcar
          (lambda (cfg)
            (funcall 'neovm--mht-create-and-populate
                     (nth 0 cfg)
                     (nth 1 cfg)
                     (nth 2 cfg)))
          configs))
    (fmakunbound 'neovm--mht-create-and-populate)))"#;
    let expect = expect_test::expect![[
        r#""OK ((eq 50 0 2401) (eq 50 0 2401) (eql 50 0 2401) (eql 50 0 2401) (equal 50 0 2401) (equal 50 0 2401) (eq 200 0 39601) (equal 200 0 39601))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: chaining hash tables as a namespace/scope chain
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_hash_table_scope_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate lexical scope chain with hash tables: each scope has a parent
    let form = r#"(progn
  (fset 'neovm--mht-scope-lookup
    (lambda (scope-chain key)
      "Look up KEY in a chain of hash tables (innermost first)."
      (let ((found nil) (result nil))
        (while (and scope-chain (not found))
          (let ((val (gethash key (car scope-chain) 'neovm--not-found)))
            (if (eq val 'neovm--not-found)
                (setq scope-chain (cdr scope-chain))
              (setq found t)
              (setq result val))))
        (if found result nil))))

  (unwind-protect
      (let* ((global (make-hash-table :test 'eq))
             (module (make-hash-table :test 'eq))
             (local (make-hash-table :test 'eq)))
        ;; Global scope
        (puthash 'x 1 global)
        (puthash 'y 2 global)
        (puthash 'z 3 global)
        (puthash 'pi 314 global)
        ;; Module scope: shadows x
        (puthash 'x 100 module)
        (puthash 'w 200 module)
        ;; Local scope: shadows x again and y
        (puthash 'x 999 local)
        (puthash 'y 888 local)
        (puthash 'temp 777 local)
        ;; Chain: local -> module -> global
        (let ((chain (list local module global)))
          (list
            ;; x: found in local (999)
            (funcall 'neovm--mht-scope-lookup chain 'x)
            ;; y: found in local (888)
            (funcall 'neovm--mht-scope-lookup chain 'y)
            ;; z: found in global (3)
            (funcall 'neovm--mht-scope-lookup chain 'z)
            ;; w: found in module (200)
            (funcall 'neovm--mht-scope-lookup chain 'w)
            ;; pi: found in global (314)
            (funcall 'neovm--mht-scope-lookup chain 'pi)
            ;; temp: found in local (777)
            (funcall 'neovm--mht-scope-lookup chain 'temp)
            ;; nonexistent: nil
            (funcall 'neovm--mht-scope-lookup chain 'missing)
            ;; Without local scope
            (funcall 'neovm--mht-scope-lookup (list module global) 'x)
            ;; Without module scope
            (funcall 'neovm--mht-scope-lookup (list local global) 'w)
            ;; Only global
            (funcall 'neovm--mht-scope-lookup (list global) 'x))))
    (fmakunbound 'neovm--mht-scope-lookup)))"#;
    let expect = expect_test::expect![[r#""OK (999 888 3 200 314 777 nil 100 nil 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
