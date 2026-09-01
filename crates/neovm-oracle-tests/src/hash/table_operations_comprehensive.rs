//! Oracle parity tests for comprehensive hash table operations:
//! `make-hash-table` with all keyword args, `gethash` with default,
//! `puthash`, `remhash`, `clrhash`, `maphash`, `hash-table-count`,
//! `hash-table-test`, `hash-table-size`, `hash-table-rehash-size`,
//! `hash-table-rehash-threshold`, `copy-hash-table`,
//! `hash-table-keys`/`hash-table-values`, and `hash-table-p`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// make-hash-table: all keyword arguments
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_ops_make_all_kwargs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test every keyword arg of make-hash-table and the accessor roundtrip
    let form = r#"(let ((h1 (make-hash-table :test 'eq :size 64
                                              :rehash-size 2.0
                                              :rehash-threshold 0.8))
                        (h2 (make-hash-table :test 'equal :size 16
                                              :rehash-size 1.5
                                              :rehash-threshold 0.9))
                        (h3 (make-hash-table :test 'eql))
                        (h4 (make-hash-table)))
                    (list
                      ;; test accessors
                      (hash-table-test h1) (hash-table-test h2)
                      (hash-table-test h3) (hash-table-test h4)
                      ;; size >= requested
                      (>= (hash-table-size h1) 64)
                      (>= (hash-table-size h2) 16)
                      ;; rehash-size and threshold preserved
                      (hash-table-rehash-size h1)
                      (hash-table-rehash-size h2)
                      (hash-table-rehash-threshold h1)
                      (hash-table-rehash-threshold h2)
                      ;; all are hash tables
                      (hash-table-p h1) (hash-table-p h2)
                      (hash-table-p h3) (hash-table-p h4)
                      ;; non-hash-table checks
                      (hash-table-p nil) (hash-table-p '(a b))
                      (hash-table-p [1 2 3]) (hash-table-p "string")))"#;
    let expect = expect_test::expect![[
        r#""OK (eq equal eql eql t t 1.5 1.5 0.8125 0.8125 t t t t nil nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_hash_table_ops_make_keyword_validation_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:Fmake_hash_table parses keyword/value pairs from the end
    // while assigning to one slot per supported keyword.  That makes duplicate
    // keyword arguments first-one-wins from Lisp's left-to-right call shape.
    // Obsolete rehash keywords are accepted and ignored, while invalid
    // keywords, tests, weakness values, sizes, and odd argument counts signal
    // their exact GNU error data.
    let form = r#"
(list
 (condition-case err
     (make-hash-table :test)
   (error (cons (car err) (cdr err))))
 (condition-case err
     (make-hash-table :bogus 1)
   (error (cons (car err) (cdr err))))
 (condition-case err
     (make-hash-table :test 'missing-test)
   (error (cons (car err) (cdr err))))
 (condition-case err
     (make-hash-table :weakness 'missing-weakness)
   (error (cons (car err) (cdr err))))
 (condition-case err
     (make-hash-table :size -1)
   (error (cons (car err) (cdr err))))
 (condition-case err
     (make-hash-table :size 1.0)
   (error (cons (car err) (cdr err))))
 (hash-table-test
  (make-hash-table :test 'eq :test 'equal))
 (hash-table-weakness
  (make-hash-table :weakness 'key :weakness t))
 (hash-table-test
  (make-hash-table :test 'equal
                   :rehash-size 'ignored
                   :rehash-threshold 'ignored
                   :purecopy 'ignored))
 (hash-table-rehash-size
  (make-hash-table :rehash-size 'ignored))
 (hash-table-rehash-threshold
  (make-hash-table :rehash-threshold 'ignored)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((error \"Odd number of arguments\") (error \"Invalid keyword argument\" :bogus) (error \"Invalid hash table test\" missing-test) (error \"Invalid hash table weakness\" missing-weakness) (error \"Invalid hash table size\" -1) (error \"Invalid hash table size\" 1.0) eq key equal 1.5 0.8125)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// gethash: default value behavior
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_ops_gethash_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // gethash returns default when key is absent; nil when no default given
    let form = r#"(let ((h (make-hash-table :test 'eq)))
                    (puthash 'present 'here h)
                    (puthash 'nil-val nil h)
                    (list
                      ;; Key exists: return value
                      (gethash 'present h)
                      ;; Key missing, no default: nil
                      (gethash 'missing h)
                      ;; Key missing, with default
                      (gethash 'missing h 'fallback)
                      (gethash 'missing h 42)
                      (gethash 'missing h '(default list))
                      ;; Key exists with nil value: returns nil, NOT default
                      (gethash 'nil-val h 'should-not-see)
                      ;; Default is not stored
                      (gethash 'missing h 'temp)
                      (gethash 'missing h)))"#;
    let expect =
        expect_test::expect![[r#""OK (here nil fallback 42 (default list) nil temp nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// puthash: overwrite semantics and return value
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_ops_puthash_overwrite() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // puthash returns the value; overwrites preserve count
    let form = r#"(let ((h (make-hash-table :test 'equal)))
                    (let ((r1 (puthash "k1" 100 h))
                          (r2 (puthash "k2" 200 h))
                          (r3 (puthash "k3" 300 h)))
                      (let ((count-before (hash-table-count h)))
                        ;; Overwrite k2 multiple times
                        (puthash "k2" 'overwrite-1 h)
                        (puthash "k2" 'overwrite-2 h)
                        (puthash "k2" 'overwrite-3 h)
                        (let ((count-after (hash-table-count h)))
                          (list r1 r2 r3
                                count-before count-after
                                ;; count unchanged after overwrites
                                (= count-before count-after)
                                (gethash "k1" h)
                                (gethash "k2" h)
                                (gethash "k3" h))))))"#;
    let expect = expect_test::expect![[r#""OK (100 200 300 3 3 t 100 overwrite-3 300)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// remhash: removal and re-insertion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_ops_remhash_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Remove, verify absence, re-insert, verify presence
    let form = r#"(let ((h (make-hash-table :test 'eq)))
                    (dotimes (i 8) (puthash i (* i 10) h))
                    (let ((c0 (hash-table-count h)))
                      ;; Remove even keys
                      (remhash 0 h) (remhash 2 h) (remhash 4 h) (remhash 6 h)
                      (let ((c1 (hash-table-count h)))
                        ;; Remove non-existent key (no-op)
                        (remhash 99 h)
                        (let ((c2 (hash-table-count h)))
                          ;; Re-insert some removed keys with new values
                          (puthash 0 'zero h)
                          (puthash 6 'six h)
                          (let ((c3 (hash-table-count h)))
                            (list c0 c1 c2 c3
                                  (gethash 0 h)
                                  (gethash 1 h)
                                  (gethash 2 h 'gone)
                                  (gethash 6 h)
                                  (gethash 7 h)))))))"#;
    let expect = expect_test::expect![[r#""OK (8 4 4 6 zero 10 gone six 70)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// clrhash: clear and reuse
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_ops_clrhash_reuse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // clrhash empties the table but preserves metadata
    let form = r#"(let ((h (make-hash-table :test 'equal :size 32)))
                    (dotimes (i 20) (puthash (number-to-string i) i h))
                    (let ((pre-count (hash-table-count h))
                          (pre-test (hash-table-test h)))
                      (clrhash h)
                      (let ((post-count (hash-table-count h))
                            (post-test (hash-table-test h)))
                        ;; Reuse after clear
                        (puthash "new-1" 'alpha h)
                        (puthash "new-2" 'beta h)
                        (list pre-count post-count
                              (eq pre-test post-test)
                              (hash-table-count h)
                              (gethash "0" h 'not-found)
                              (gethash "new-1" h)
                              (gethash "new-2" h)))))"#;
    let expect = expect_test::expect![[r#""OK (20 0 t 2 not-found alpha beta)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// maphash: iteration order independence and accumulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_ops_maphash_accumulate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // maphash collects all k/v pairs; order is unspecified so we sort
    let form = r#"(let ((h (make-hash-table :test 'eq)))
                    (puthash 'alpha 1 h)
                    (puthash 'beta 2 h)
                    (puthash 'gamma 3 h)
                    (puthash 'delta 4 h)
                    (puthash 'epsilon 5 h)
                    (let ((keys nil) (vals nil) (sum 0))
                      (maphash (lambda (k v)
                                 (push k keys)
                                 (push v vals)
                                 (setq sum (+ sum v)))
                               h)
                      (list (sort keys (lambda (a b) (string< (symbol-name a)
                                                               (symbol-name b))))
                            (sort vals '<)
                            sum
                            (= sum 15))))"#;
    let expect =
        expect_test::expect![[r#""OK ((alpha beta delta epsilon gamma) (1 2 3 4 5) 15 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// hash-table-count: tracks insertions and removals precisely
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_ops_count_precise_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Exhaustive count tracking through various operations
    let form = r#"(let ((h (make-hash-table :test 'eql))
                        (counts nil))
                    ;; Empty
                    (push (hash-table-count h) counts)
                    ;; Insert 5
                    (dotimes (i 5) (puthash i i h))
                    (push (hash-table-count h) counts)
                    ;; Overwrite all 5 (count unchanged)
                    (dotimes (i 5) (puthash i (* i 100) h))
                    (push (hash-table-count h) counts)
                    ;; Remove 3
                    (remhash 0 h) (remhash 2 h) (remhash 4 h)
                    (push (hash-table-count h) counts)
                    ;; Remove non-existent (no change)
                    (remhash 999 h) (remhash -1 h)
                    (push (hash-table-count h) counts)
                    ;; Insert 2 new + 1 overwrite
                    (puthash 10 'ten h) (puthash 20 'twenty h) (puthash 1 'one h)
                    (push (hash-table-count h) counts)
                    ;; Clear
                    (clrhash h)
                    (push (hash-table-count h) counts)
                    ;; Rebuild
                    (puthash 'only 'one h)
                    (push (hash-table-count h) counts)
                    (nreverse counts))"#;
    let expect = expect_test::expect![[r#""OK (0 5 5 2 2 4 0 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// copy-hash-table: deep independence and metadata preservation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_ops_copy_independence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Copies share no state; metadata (test, rehash-*) is preserved
    let form = r#"(let ((orig (make-hash-table :test 'equal :size 32
                                                :rehash-size 2.0 :rehash-threshold 0.8)))
                    (puthash "a" 1 orig) (puthash "b" 2 orig) (puthash "c" 3 orig)
                    (let ((copy (copy-hash-table orig)))
                      ;; Mutate each independently
                      (puthash "d" 4 orig) (remhash "a" orig)
                      (puthash "e" 5 copy) (puthash "a" 999 copy)
                      (list
                        ;; orig state
                        (gethash "a" orig 'gone) (gethash "d" orig)
                        (hash-table-count orig)
                        ;; copy state
                        (gethash "a" copy) (gethash "e" copy)
                        (gethash "d" copy 'not-here)
                        (hash-table-count copy)
                        ;; metadata preserved
                        (eq (hash-table-test orig) (hash-table-test copy))
                        (= (hash-table-rehash-size orig) (hash-table-rehash-size copy))
                        (= (hash-table-rehash-threshold orig)
                           (hash-table-rehash-threshold copy)))))"#;
    let expect = expect_test::expect![[r#""OK (gone 4 3 999 5 not-here 4 t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// hash-table-keys / hash-table-values (from subr-x or built-in in 29+)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_ops_keys_and_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // hash-table-keys and hash-table-values should return all entries
    let form = r#"(progn (require 'subr-x) (let ((h (make-hash-table :test 'eq)))
                    (puthash 'x 10 h) (puthash 'y 20 h)
                    (puthash 'z 30 h) (puthash 'w 40 h)
                    (let ((ks (sort (hash-table-keys h)
                                    (lambda (a b) (string< (symbol-name a)
                                                            (symbol-name b)))))
                          (vs (sort (hash-table-values h) '<)))
                      ;; After removal
                      (remhash 'y h)
                      (let ((ks2 (sort (hash-table-keys h)
                                       (lambda (a b) (string< (symbol-name a)
                                                               (symbol-name b)))))
                            (vs2 (sort (hash-table-values h) '<)))
                        (list ks vs
                              ks2 vs2
                              (length ks) (length ks2))))))"#;
    let expect = expect_test::expect![[r#""OK ((w x y z) (10 20 30 40) (w x z) (10 30 40) 4 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// hash-table-test: eq vs eql vs equal behavior
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_ops_test_eq_eql_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Demonstrate how test affects key lookup for different types
    let form = r#"(let ((h-eq (make-hash-table :test 'eq))
                        (h-eql (make-hash-table :test 'eql))
                        (h-equal (make-hash-table :test 'equal)))
                    ;; Integer keys: eql and equal match by value, eq may or may not
                    (puthash 1 'one h-eq) (puthash 1 'one h-eql) (puthash 1 'one h-equal)
                    ;; String keys: only equal matches by content
                    (let ((k1 (copy-sequence "hello"))
                          (k2 (copy-sequence "hello")))
                      (puthash k1 'found h-eq)
                      (puthash k1 'found h-eql)
                      (puthash k1 'found h-equal)
                      (list
                        ;; Integer lookup (same fixnum)
                        (gethash 1 h-eq) (gethash 1 h-eql) (gethash 1 h-equal)
                        ;; String lookup with same object
                        (gethash k1 h-eq) (gethash k1 h-eql) (gethash k1 h-equal)
                        ;; String lookup with different object, same content
                        ;; eq/eql will fail, equal will succeed
                        (gethash k2 h-equal)
                        ;; Symbol keys: always work for all tests (interned => eq)
                        (progn (puthash 'sym 'ok h-eq) (gethash 'sym h-eq))
                        (progn (puthash 'sym 'ok h-eql) (gethash 'sym h-eql))
                        (progn (puthash 'sym 'ok h-equal) (gethash 'sym h-equal))
                        ;; List keys: only equal works
                        (progn (puthash '(1 2) 'list-val h-equal)
                               (gethash '(1 2) h-equal)))))"#;
    let expect =
        expect_test::expect![[r#""OK (one one one found found found found ok ok ok list-val)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: frequency counter using hash tables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_ops_frequency_counter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a word frequency table, then find top-N by count
    let form = r#"(let ((freq (make-hash-table :test 'equal))
                        (words '("the" "quick" "brown" "fox" "the" "quick"
                                 "the" "lazy" "dog" "the" "brown" "fox"
                                 "quick" "the" "fox" "dog" "lazy" "the")))
                    ;; Count frequencies
                    (dolist (w words)
                      (puthash w (1+ (gethash w freq 0)) freq))
                    ;; Collect as sorted pairs
                    (let ((pairs nil))
                      (maphash (lambda (k v) (push (cons k v) pairs)) freq)
                      ;; Sort by frequency descending, then alphabetically
                      (let ((sorted (sort pairs
                                          (lambda (a b)
                                            (or (> (cdr a) (cdr b))
                                                (and (= (cdr a) (cdr b))
                                                     (string< (car a) (car b))))))))
                        (list
                          sorted
                          (hash-table-count freq)
                          ;; Most frequent word
                          (caar sorted)
                          (cdar sorted)
                          ;; Total word count
                          (let ((total 0))
                            (maphash (lambda (_k v) (setq total (+ total v))) freq)
                            total)
                          (= (let ((s 0))
                               (maphash (lambda (_k v) (setq s (+ s v))) freq) s)
                             (length words))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"the\" . 6) (\"fox\" . 3) (\"quick\" . 3) (\"brown\" . 2) (\"dog\" . 2) (\"lazy\" . 2)) 6 \"the\" 6 18 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: two-table join (relational-style)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_ops_relational_join() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a relational inner join: employees x departments
    let form = r#"(let ((employees (make-hash-table :test 'eq))
                        (departments (make-hash-table :test 'eq)))
                    ;; employees: id -> (name . dept-id)
                    (puthash 1 '("Alice" . eng) employees)
                    (puthash 2 '("Bob" . sales) employees)
                    (puthash 3 '("Carol" . eng) employees)
                    (puthash 4 '("Dave" . hr) employees)
                    (puthash 5 '("Eve" . sales) employees)
                    ;; departments: dept-id -> (dept-name . budget)
                    (puthash 'eng '("Engineering" . 500000) departments)
                    (puthash 'sales '("Sales" . 300000) departments)
                    (puthash 'hr '("Human Resources" . 200000) departments)
                    ;; Inner join: employee x department on dept-id
                    (let ((joined nil))
                      (maphash
                        (lambda (emp-id emp-data)
                          (let* ((emp-name (car emp-data))
                                 (dept-id (cdr emp-data))
                                 (dept-data (gethash dept-id departments)))
                            (when dept-data
                              (push (list emp-id emp-name
                                         (car dept-data) (cdr dept-data))
                                    joined))))
                        employees)
                      ;; Sort by employee id
                      (let ((result (sort joined (lambda (a b) (< (car a) (car b))))))
                        (list
                          result
                          (length result)
                          ;; Group by department: count employees per dept
                          (let ((dept-count (make-hash-table :test 'eq)))
                            (dolist (row result)
                              (let ((dept-name (nth 2 row)))
                                (puthash dept-name (1+ (gethash dept-name dept-count 0))
                                         dept-count)))
                            (let ((dc-pairs nil))
                              (maphash (lambda (k v) (push (cons k v) dc-pairs)) dept-count)
                              (sort dc-pairs
                                    (lambda (a b) (string< (car a) (car b))))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 \"Alice\" \"Engineering\" 500000) (2 \"Bob\" \"Sales\" 300000) (3 \"Carol\" \"Engineering\" 500000) (4 \"Dave\" \"Human Resources\" 200000) (5 \"Eve\" \"Sales\" 300000)) 5 ((\"Engineering\" . 2) (\"Human Resources\" . 1) (\"Sales\" . 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
