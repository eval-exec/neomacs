//! Comprehensive oracle parity tests for hash table operations.
//!
//! Tests make-hash-table with ALL keyword args (:test, :size, :rehash-size,
//! :rehash-threshold, :weakness), different test functions (eq, eql, equal),
//! puthash/gethash with DEFAULT arg, remhash, clrhash, maphash with complex
//! lambdas, hash-table-count/size/test, copy-hash-table independence,
//! nested hash tables, and mixed key types.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// make-hash-table with all keyword arguments
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_all_keyword_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((h1 (make-hash-table :test 'eq :size 50
                                              :rehash-size 2.0 :rehash-threshold 0.8))
                        (h2 (make-hash-table :test 'equal :size 200
                                              :rehash-size 1.5 :rehash-threshold 0.9))
                        (h3 (make-hash-table :test 'eql :size 10
                                              :rehash-size 1.3 :rehash-threshold 0.65))
                        (h4 (make-hash-table :weakness 'key))
                        (h5 (make-hash-table :weakness 'value))
                        (h6 (make-hash-table :weakness 'key-or-value))
                        (h7 (make-hash-table :weakness 'key-and-value))
                        (h8 (make-hash-table)))
                    (list
                      ;; test functions
                      (hash-table-test h1)
                      (hash-table-test h2)
                      (hash-table-test h3)
                      (hash-table-test h8)
                      ;; size hints (at least as large as requested)
                      (>= (hash-table-size h1) 50)
                      (>= (hash-table-size h2) 200)
                      ;; rehash-size
                      (hash-table-rehash-size h1)
                      (hash-table-rehash-size h2)
                      (hash-table-rehash-size h3)
                      ;; rehash-threshold
                      (hash-table-rehash-threshold h1)
                      (hash-table-rehash-threshold h2)
                      (hash-table-rehash-threshold h3)
                      ;; weakness
                      (hash-table-weakness h4)
                      (hash-table-weakness h5)
                      (hash-table-weakness h6)
                      (hash-table-weakness h7)
                      (hash-table-weakness h8)
                      ;; predicates
                      (hash-table-p h1)
                      (hash-table-p 42)
                      (hash-table-p "not a table")
                      (hash-table-p nil)))"#;
    let expect = expect_test::expect![[
        r#""OK (eq equal eql eql t t 1.5 1.5 1.5 0.8125 0.8125 0.8125 key value key-or-value key-and-value nil t nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// eq vs eql vs equal test function semantics with various key types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_test_function_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((h-eq (make-hash-table :test 'eq))
                        (h-eql (make-hash-table :test 'eql))
                        (h-equal (make-hash-table :test 'equal)))
                    ;; Integer keys: eq may or may not work for large ints,
                    ;; eql and equal should always work
                    (puthash 42 'found-eq h-eq)
                    (puthash 42 'found-eql h-eql)
                    (puthash 42 'found-equal h-equal)
                    ;; Float keys
                    (puthash 3.14 'pi-eql h-eql)
                    (puthash 3.14 'pi-equal h-equal)
                    ;; String keys: eq treats separate string objects as different
                    (let ((s1 (copy-sequence "hello"))
                          (s2 (copy-sequence "hello")))
                      (puthash s1 'str-eq h-eq)
                      (puthash s1 'str-eql h-eql)
                      (puthash s1 'str-equal h-equal)
                      ;; Lookup with different string object, same content
                      (let ((eq-s2-result (gethash s2 h-eq 'not-found))
                            (eql-s2-result (gethash s2 h-eql 'not-found))
                            (equal-s2-result (gethash s2 h-equal)))
                        ;; List keys
                        (let ((l1 (list 'a 'b 'c))
                              (l2 (list 'a 'b 'c)))
                          (puthash l1 'list-eq h-eq)
                          (puthash l1 'list-equal h-equal)
                          (list
                            ;; Integer lookups
                            (gethash 42 h-eq)
                            (gethash 42 h-eql)
                            (gethash 42 h-equal)
                            ;; Float lookups
                            (gethash 3.14 h-eql)
                            (gethash 3.14 h-equal)
                            ;; String with different object
                            eq-s2-result
                            eql-s2-result
                            equal-s2-result
                            ;; List with different cons cells
                            (gethash l2 h-eq)
                            (gethash l2 h-equal)
                            ;; Same cons cells
                            (gethash l1 h-eq)
                            (gethash l1 h-equal))))))"#;
    let expect = expect_test::expect![[
        r#""OK (found-eq found-eql found-equal pi-eql pi-equal not-found not-found str-equal nil list-equal list-eq list-equal)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// gethash DEFAULT argument and remhash edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_gethash_default_and_remhash() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((h (make-hash-table :test 'equal)))
                    ;; Put some entries including nil as value
                    (puthash 'a 'val-a h)
                    (puthash 'b nil h)
                    (puthash 'c 0 h)
                    (puthash nil 'nil-key h)
                    (puthash t 'true-key h)
                    (let ((results nil))
                      ;; gethash with default
                      (setq results (cons (gethash 'a h 'default) results))
                      (setq results (cons (gethash 'b h 'default) results))  ;; nil value, not default
                      (setq results (cons (gethash 'c h 'default) results))  ;; 0 value
                      (setq results (cons (gethash 'missing h 'default) results))  ;; returns default
                      (setq results (cons (gethash 'missing h) results))  ;; returns nil (no default)
                      (setq results (cons (gethash nil h 'default) results))  ;; nil as key
                      (setq results (cons (gethash t h 'default) results))  ;; t as key
                      ;; remhash returns nil regardless
                      (setq results (cons (remhash 'a h) results))
                      (setq results (cons (remhash 'nonexistent h) results))
                      ;; Verify removal
                      (setq results (cons (gethash 'a h 'gone) results))
                      ;; Double remove is safe
                      (setq results (cons (remhash 'a h) results))
                      (setq results (cons (hash-table-count h) results))
                      (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK (val-a nil 0 default nil nil-key true-key nil nil gone nil 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// clrhash and reuse cycle with count/size tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_clrhash_and_reuse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((h (make-hash-table :test 'equal :size 20)))
                    (let ((snapshots nil))
                      ;; Phase 1: fill it up
                      (dotimes (i 30)
                        (puthash (format "key-%d" i) (* i 10) h))
                      (setq snapshots (cons (list 'phase1
                                                   (hash-table-count h)
                                                   (>= (hash-table-size h) 20)
                                                   (gethash "key-0" h)
                                                   (gethash "key-29" h))
                                             snapshots))
                      ;; Phase 2: clear and verify empty
                      (clrhash h)
                      (setq snapshots (cons (list 'phase2
                                                   (hash-table-count h)
                                                   (gethash "key-0" h 'gone)
                                                   (gethash "key-29" h 'gone)
                                                   (hash-table-test h))
                                             snapshots))
                      ;; Phase 3: reuse with different data
                      (puthash 'alpha 1 h)
                      (puthash 'beta 2 h)
                      (puthash 'gamma 3 h)
                      (setq snapshots (cons (list 'phase3
                                                   (hash-table-count h)
                                                   (gethash 'alpha h)
                                                   (gethash 'beta h)
                                                   (gethash 'gamma h))
                                             snapshots))
                      ;; Phase 4: clear again and immediately check
                      (clrhash h)
                      (clrhash h)  ;; double clear is safe
                      (setq snapshots (cons (list 'phase4
                                                   (hash-table-count h)
                                                   (hash-table-p h))
                                             snapshots))
                      (nreverse snapshots)))"#;
    let expect = expect_test::expect![[
        r#""OK ((phase1 30 t 0 290) (phase2 0 gone gone equal) (phase3 3 1 2 3) (phase4 0 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// maphash with complex accumulation and side-effect patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_maphash_complex_lambdas() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((h (make-hash-table :test 'equal)))
                    ;; Populate with numeric data
                    (puthash "alice" 85 h)
                    (puthash "bob" 92 h)
                    (puthash "carol" 78 h)
                    (puthash "dave" 95 h)
                    (puthash "eve" 88 h)
                    ;; Compute multiple aggregates in a single maphash pass
                    (let ((sum 0)
                          (count 0)
                          (max-name "")
                          (max-val -1)
                          (min-name "")
                          (min-val 1000)
                          (passing nil))
                      (maphash (lambda (name score)
                                 (setq sum (+ sum score))
                                 (setq count (1+ count))
                                 (when (> score max-val)
                                   (setq max-val score)
                                   (setq max-name name))
                                 (when (< score min-val)
                                   (setq min-val score)
                                   (setq min-name name))
                                 (when (>= score 80)
                                   (setq passing (cons name passing))))
                               h)
                      ;; Build a grade table using maphash
                      (let ((grades (make-hash-table :test 'equal)))
                        (maphash (lambda (name score)
                                   (puthash name
                                            (cond ((>= score 90) "A")
                                                  ((>= score 80) "B")
                                                  ((>= score 70) "C")
                                                  (t "F"))
                                            grades))
                                 h)
                        (let ((grade-list nil))
                          (maphash (lambda (k v)
                                     (setq grade-list (cons (cons k v) grade-list)))
                                   grades)
                          (list
                            sum count
                            (cons max-name max-val)
                            (cons min-name min-val)
                            (sort passing #'string<)
                            (sort grade-list
                                  (lambda (a b) (string< (car a) (car b)))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (438 5 (\"dave\" . 95) (\"carol\" . 78) (\"alice\" \"bob\" \"dave\" \"eve\") ((\"alice\" . \"B\") (\"bob\" . \"A\") (\"carol\" . \"C\") (\"dave\" . \"A\") (\"eve\" . \"B\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// copy-hash-table: verify deep independence and test preservation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_copy_independence_thorough() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((orig (make-hash-table :test 'equal :size 30)))
                    ;; Populate with various types
                    (puthash 1 "one" orig)
                    (puthash 2 "two" orig)
                    (puthash 3 "three" orig)
                    (puthash "str-key" '(a b c) orig)
                    (puthash '(list key) 'list-val orig)
                    (puthash nil 'nil-val orig)
                    (let ((copy (copy-hash-table orig)))
                      ;; Verify initial equality
                      (let ((init-match
                              (and (= (hash-table-count orig) (hash-table-count copy))
                                   (eq (hash-table-test orig) (hash-table-test copy))
                                   (equal (gethash 1 orig) (gethash 1 copy))
                                   (equal (gethash "str-key" orig) (gethash "str-key" copy))
                                   (equal (gethash nil orig) (gethash nil copy)))))
                        ;; Mutate original: add, remove, overwrite
                        (puthash 4 "four" orig)
                        (remhash 1 orig)
                        (puthash 2 "TWO-MODIFIED" orig)
                        ;; Mutate copy differently
                        (puthash 5 "five" copy)
                        (remhash 3 copy)
                        (puthash "str-key" '(x y z) copy)
                        ;; Verify full independence
                        (list
                          init-match
                          ;; Original state
                          (hash-table-count orig)
                          (gethash 1 orig 'gone)
                          (gethash 2 orig)
                          (gethash 4 orig)
                          (gethash 5 orig 'not-here)
                          ;; Copy state
                          (hash-table-count copy)
                          (gethash 1 copy)
                          (gethash 2 copy)
                          (gethash 3 copy 'gone)
                          (gethash 5 copy)
                          (gethash "str-key" copy)
                          (gethash 4 copy 'not-here)))))"#;
    let expect = expect_test::expect![[
        r#""OK (t 6 gone \"TWO-MODIFIED\" \"four\" not-here 6 \"one\" \"two\" gone \"five\" (x y z) not-here)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested hash tables: hash tables as values inside hash tables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_nested_tables() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((outer (make-hash-table :test 'eq)))
                    ;; Create inner tables
                    (let ((inner1 (make-hash-table :test 'equal))
                          (inner2 (make-hash-table :test 'equal))
                          (inner3 (make-hash-table :test 'equal)))
                      ;; Populate inner tables
                      (puthash "name" "Alice" inner1)
                      (puthash "age" 30 inner1)
                      (puthash "hobbies" '("reading" "coding") inner1)
                      (puthash "name" "Bob" inner2)
                      (puthash "age" 25 inner2)
                      (puthash "hobbies" '("gaming" "cooking") inner2)
                      (puthash "name" "Carol" inner3)
                      (puthash "age" 35 inner3)
                      (puthash "hobbies" '("hiking") inner3)
                      ;; Store inner tables in outer
                      (puthash 'user1 inner1 outer)
                      (puthash 'user2 inner2 outer)
                      (puthash 'user3 inner3 outer)
                      ;; Deeply nested: table inside a table inside a table
                      (let ((deep (make-hash-table :test 'equal)))
                        (puthash "level" 3 deep)
                        (let ((mid (make-hash-table :test 'equal)))
                          (puthash "nested" deep mid)
                          (puthash "level" 2 mid)
                          (puthash 'deep-nest mid outer)))
                      ;; Queries through nesting
                      (list
                        (hash-table-count outer)
                        ;; Access inner table fields
                        (gethash "name" (gethash 'user1 outer))
                        (gethash "age" (gethash 'user2 outer))
                        (gethash "hobbies" (gethash 'user3 outer))
                        ;; Deep nesting
                        (gethash "level" (gethash "nested" (gethash 'deep-nest outer)))
                        (gethash "level" (gethash 'deep-nest outer))
                        ;; Modify through nesting
                        (progn
                          (puthash "age" 31 (gethash 'user1 outer))
                          (gethash "age" (gethash 'user1 outer)))
                        ;; Verify inner1 was modified too (same object)
                        (gethash "age" inner1)
                        ;; hash-table-p on inner values
                        (hash-table-p (gethash 'user1 outer))
                        (hash-table-p (gethash "hobbies" inner1)))))"#;
    let expect = expect_test::expect![[r#""OK (4 \"Alice\" 25 (\"hiking\") 3 2 31 31 t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Mixed key types: symbols, strings, integers, floats, nil, t, lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_mixed_key_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((h (make-hash-table :test 'equal)))
                    ;; Insert with diverse key types
                    (puthash 'sym-key 'symbol-value h)
                    (puthash "string-key" 'string-value h)
                    (puthash 0 'zero h)
                    (puthash 1 'one h)
                    (puthash -1 'neg-one h)
                    (puthash 999999 'big-int h)
                    (puthash 1.5 'one-point-five h)
                    (puthash 0.0 'zero-float h)
                    (puthash nil 'nil-value h)
                    (puthash t 'true-value h)
                    (puthash '(a b c) 'list-value h)
                    (puthash '(1 2 3) 'num-list-value h)
                    (puthash [1 2 3] 'vector-value h)
                    (puthash "" 'empty-string h)
                    (puthash '() 'empty-list h)  ;; same as nil
                    ;; Verify all retrievals
                    (let ((results nil))
                      (setq results (cons (gethash 'sym-key h) results))
                      (setq results (cons (gethash "string-key" h) results))
                      (setq results (cons (gethash 0 h) results))
                      (setq results (cons (gethash 1 h) results))
                      (setq results (cons (gethash -1 h) results))
                      (setq results (cons (gethash 999999 h) results))
                      (setq results (cons (gethash 1.5 h) results))
                      (setq results (cons (gethash 0.0 h) results))
                      (setq results (cons (gethash nil h) results))
                      (setq results (cons (gethash t h) results))
                      (setq results (cons (gethash '(a b c) h) results))
                      (setq results (cons (gethash '(1 2 3) h) results))
                      (setq results (cons (gethash [1 2 3] h) results))
                      (setq results (cons (gethash "" h) results))
                      ;; nil as key overwrites empty-list (both are nil)
                      (setq results (cons (hash-table-count h) results))
                      ;; Overwrite with same key different value
                      (puthash 'sym-key 'overwritten h)
                      (setq results (cons (gethash 'sym-key h) results))
                      ;; Count shouldn't change on overwrite
                      (setq results (cons (hash-table-count h) results))
                      (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK (symbol-value string-value zero one neg-one big-int one-point-five zero-float empty-list true-value list-value num-list-value vector-value empty-string 14 overwritten 14)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// hash-table-count/size/test with dynamic changes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_metadata_dynamic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((h (make-hash-table :test 'equal :size 5)))
                    (let ((trace nil))
                      ;; Track count as we add
                      (dotimes (i 20)
                        (puthash i (* i i) h)
                        (when (memq i '(0 4 9 14 19))
                          (setq trace (cons (list 'add i
                                                   (hash-table-count h)
                                                   (>= (hash-table-size h) (hash-table-count h)))
                                             trace))))
                      ;; Track count as we remove
                      (dotimes (i 20)
                        (when (= 0 (% i 3))
                          (remhash i h))
                        (when (memq i '(0 5 11 17))
                          (setq trace (cons (list 'rem i (hash-table-count h)) trace))))
                      ;; Overwrite doesn't change count
                      (let ((count-before (hash-table-count h)))
                        (maphash (lambda (k v) (puthash k (1+ v) h)) h)
                        (setq trace (cons (list 'overwrite-no-count-change
                                                 (= count-before (hash-table-count h)))
                                           trace)))
                      ;; test is preserved throughout
                      (setq trace (cons (list 'test-preserved (hash-table-test h)) trace))
                      (nreverse trace)))"#;
    let expect = expect_test::expect![[
        r#""OK ((add 0 1 t) (add 4 5 t) (add 9 10 t) (add 14 15 t) (add 19 20 t) (rem 0 19) (rem 5 18) (rem 11 16) (rem 17 14) (overwrite-no-count-change t) (test-preserved equal))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Frequency counter and histogram via hash tables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_frequency_counter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((freq (make-hash-table :test 'equal))
                        (words '("the" "quick" "brown" "fox" "the" "quick"
                                 "the" "lazy" "dog" "the" "brown" "fox"
                                 "quick" "brown" "fox" "the")))
                    ;; Count frequencies
                    (dolist (w words)
                      (puthash w (1+ (gethash w freq 0)) freq))
                    ;; Build sorted frequency list
                    (let ((freq-list nil))
                      (maphash (lambda (k v)
                                 (setq freq-list (cons (cons k v) freq-list)))
                               freq)
                      ;; Sort by frequency descending, then by word ascending
                      (setq freq-list
                            (sort freq-list
                                  (lambda (a b)
                                    (if (= (cdr a) (cdr b))
                                        (string< (car a) (car b))
                                      (> (cdr a) (cdr b))))))
                      ;; Find words appearing more than twice
                      (let ((common nil))
                        (maphash (lambda (k v)
                                   (when (> v 2)
                                     (setq common (cons k common))))
                                 freq)
                        ;; Group by frequency: build freq->words mapping
                        (let ((by-freq (make-hash-table :test 'eql)))
                          (maphash (lambda (word count)
                                     (puthash count
                                              (cons word (gethash count by-freq nil))
                                              by-freq))
                                   freq)
                          (let ((groups nil))
                            (maphash (lambda (count words)
                                       (setq groups
                                             (cons (cons count (sort words #'string<))
                                                   groups)))
                                     by-freq)
                            (list
                              freq-list
                              (sort common #'string<)
                              (hash-table-count freq)
                              (sort groups (lambda (a b) (> (car a) (car b))))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"the\" . 5) (\"brown\" . 3) (\"fox\" . 3) (\"quick\" . 3) (\"dog\" . 1) (\"lazy\" . 1)) (\"brown\" \"fox\" \"quick\" \"the\") 6 ((5 \"the\") (3 \"brown\" \"fox\" \"quick\") (1 \"dog\" \"lazy\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
