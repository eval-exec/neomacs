//! Oracle parity tests for comprehensive `dolist` and `dotimes` patterns:
//! result forms, early return via throw/catch, nested iteration, accumulation,
//! various list types, index usage, variable scoping, side effects,
//! and interaction with cl-return.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// dolist result form captures accumulated state and loop var (nil after loop)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dolist_dotimes_comp_dolist_result_form_captures_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // After dolist completes, the loop variable is nil.
    // The result form sees accumulated values and nil loop var.
    let form = r#"(let ((sum 0)
                        (product 1)
                        (items-seen nil))
                    (dolist (x '(2 3 5 7 11)
                            (list :sum sum :product product
                                  :last-item x :items (nreverse items-seen)))
                      (setq sum (+ sum x))
                      (setq product (* product x))
                      (setq items-seen (cons x items-seen))))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable x)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// dotimes result form referencing the index variable (equals count after loop)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dolist_dotimes_comp_dotimes_result_form_index_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // After dotimes finishes, the index var equals the count.
    // Build a list of index^3, return via result form with final index.
    let form = r#"(let ((cubes nil))
                    (dotimes (k 8 (list :cubes (nreverse cubes)
                                        :final-k k
                                        :length (length cubes)))
                      (setq cubes (cons (* k k k) cubes))))"#;
    let expect =
        expect_test::expect![[r#""OK (:cubes (0 1 8 27 64 125 216 343) :final-k 8 :length 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Early return from dolist via catch/throw
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dolist_dotimes_comp_dolist_early_throw() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Search for the first element satisfying a predicate, return it
    // along with how many elements were inspected before finding it.
    let form = r#"(let ((inspected 0))
                    (catch 'found
                      (dolist (x '(3 7 12 18 25 31 40 55))
                        (setq inspected (1+ inspected))
                        (when (and (> x 20) (= (% x 5) 0))
                          (throw 'found (list :value x :inspected inspected))))
                      (list :value nil :inspected inspected)))"#;
    let expect = expect_test::expect![[r#""OK (:value 25 :inspected 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Early return from dotimes via catch/throw
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dolist_dotimes_comp_dotimes_early_throw() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Find the first index where cumulative sum exceeds a threshold.
    let form = r#"(let ((cumsum 0))
                    (catch 'threshold
                      (dotimes (i 100)
                        (setq cumsum (+ cumsum i))
                        (when (>= cumsum 50)
                          (throw 'threshold (list :index i :cumsum cumsum))))
                      (list :index -1 :cumsum cumsum)))"#;
    let expect = expect_test::expect![[r#""OK (:index 10 :cumsum 55)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested dolist and dotimes: build multiplication table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dolist_dotimes_comp_nested_mixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Outer dolist iterates a list of labels, inner dotimes generates columns.
    // Produces an association of label -> row-vector.
    let form = r#"(let ((table nil)
                        (labels '(alpha beta gamma)))
                    (let ((row-idx 1))
                      (dolist (label labels)
                        (let ((row nil))
                          (dotimes (col 5)
                            (setq row (cons (* row-idx (1+ col)) row)))
                          (setq table (cons (cons label (nreverse row)) table)))
                        (setq row-idx (1+ row-idx))))
                    (nreverse table))"#;
    let expect =
        expect_test::expect![[r#""OK ((alpha 1 2 3 4 5) (beta 2 4 6 8 10) (gamma 3 6 9 12 15))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Accumulation patterns: running min, max, mean via dolist
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dolist_dotimes_comp_accumulation_stats() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute running statistics over a list of numbers.
    let form = r#"(let ((nums '(42 17 93 8 55 71 3 66 29 84))
                        (running-min most-positive-fixnum)
                        (running-max most-negative-fixnum)
                        (running-sum 0)
                        (count 0)
                        (trace nil))
                    (dolist (n nums (list :final-min running-min
                                         :final-max running-max
                                         :final-sum running-sum
                                         :count count
                                         :trace (nreverse trace)))
                      (setq count (1+ count))
                      (setq running-sum (+ running-sum n))
                      (when (< n running-min) (setq running-min n))
                      (when (> n running-max) (setq running-max n))
                      (setq trace (cons (list n running-min running-max running-sum) trace))))"#;
    let expect = expect_test::expect![[
        r#""OK (:final-min 3 :final-max 93 :final-sum 468 :count 10 :trace ((42 42 42 42) (17 17 42 59) (93 17 93 152) (8 8 93 160) (55 8 93 215) (71 8 93 286) (3 3 93 289) (66 3 93 355) (29 3 93 384) (84 3 93 468)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// dolist over various list types: nested lists, mixed types, single element
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dolist_dotimes_comp_dolist_various_list_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
                    ;; Nested lists
                    (dolist (x '((1 2) (3 4) (5 6)))
                      (setq results (cons (apply '+ x) results)))
                    ;; Mixed types
                    (dolist (x '(42 "hello" nil t (a b) [1 2 3]))
                      (setq results (cons (type-of x) results)))
                    ;; Single element
                    (dolist (x '(sole-item))
                      (setq results (cons x results)))
                    ;; Symbols
                    (dolist (x '(foo bar baz quux))
                      (setq results (cons (length (symbol-name x)) results)))
                    (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK (3 7 11 integer string symbol symbol cons vector sole-item 3 3 3 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// dotimes index usage patterns: triangular numbers, powers, conditionals on index
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dolist_dotimes_comp_dotimes_index_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use index variable in multiple ways: as exponent, for conditional logic,
    // to compute triangular numbers, and to index into a vector.
    let form = r#"(let ((powers-of-2 nil)
                        (triangular nil)
                        (even-indices nil)
                        (vec [a b c d e f g h i j])
                        (vec-results nil)
                        (tri-sum 0))
                    (dotimes (i 10)
                      ;; Powers of 2
                      (let ((p 1))
                        (dotimes (_ i) (setq p (* p 2)))
                        (setq powers-of-2 (cons p powers-of-2)))
                      ;; Triangular numbers
                      (setq tri-sum (+ tri-sum i))
                      (setq triangular (cons tri-sum triangular))
                      ;; Even indices only
                      (when (= (% i 2) 0)
                        (setq even-indices (cons i even-indices)))
                      ;; Index into vector
                      (setq vec-results (cons (aref vec i) vec-results)))
                    (list :powers (nreverse powers-of-2)
                          :triangular (nreverse triangular)
                          :even-indices (nreverse even-indices)
                          :vec-results (nreverse vec-results)))"#;
    let expect = expect_test::expect![[
        r#""OK (:powers (1 2 4 8 16 32 64 128 256 512) :triangular (0 1 3 6 10 15 21 28 36 45) :even-indices (0 2 4 6 8) :vec-results (a b c d e f g h i j))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Variable scoping: inner let shadows dolist loop var, outer still accessible
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dolist_dotimes_comp_variable_scoping() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The loop variable can be shadowed by a let inside the body.
    // After the let, the loop variable should still hold its original value.
    let form = r#"(let ((log nil))
                    (dolist (x '(10 20 30))
                      (let ((outer-x x))
                        (let ((x (* x 100)))
                          ;; x is now shadowed
                          (setq log (cons (list :shadowed x :outer outer-x) log)))
                        ;; x restored after inner let
                        (setq log (cons (list :restored x) log))))
                    (nreverse log))"#;
    let expect = expect_test::expect![[
        r#""OK ((:shadowed 1000 :outer 10) (:restored 10) (:shadowed 2000 :outer 20) (:restored 20) (:shadowed 3000 :outer 30) (:restored 30))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Side effects in dolist body: puthash, setcar, vector modification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dolist_dotimes_comp_side_effects_in_body() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Mutate a hash table, modify cons cells, and write to a vector during iteration.
    let form = r#"(let ((ht (make-hash-table :test 'equal))
                        (pairs (list (cons 'a 1) (cons 'b 2) (cons 'c 3)))
                        (vec (make-vector 5 0)))
                    ;; dolist modifying hash table
                    (dolist (p pairs)
                      (puthash (symbol-name (car p)) (* (cdr p) 10) ht))
                    ;; dotimes modifying vector
                    (dotimes (i 5)
                      (aset vec i (+ (* i i) 1)))
                    ;; dolist modifying cons cells via setcdr
                    (dolist (p pairs)
                      (setcdr p (+ (cdr p) (gethash (symbol-name (car p)) ht))))
                    (list :ht-a (gethash "a" ht)
                          :ht-b (gethash "b" ht)
                          :ht-c (gethash "c" ht)
                          :pairs pairs
                          :vec vec))"#;
    let expect = expect_test::expect![[
        r#""OK (:ht-a 10 :ht-b 20 :ht-c 30 :pairs ((a . 11) (b . 22) (c . 33)) :vec [1 2 5 10 17])""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// dolist and dotimes together: sieve of Eratosthenes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dolist_dotimes_comp_sieve_of_eratosthenes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use dotimes to initialize, dolist to eliminate multiples, dotimes to collect primes.
    let form = r#"(let* ((limit 50)
                         (sieve (make-vector (1+ limit) t)))
                    ;; 0 and 1 are not prime
                    (aset sieve 0 nil)
                    (aset sieve 1 nil)
                    ;; Mark composites
                    (dotimes (i (1+ limit))
                      (when (and (>= i 2) (aref sieve i))
                        (let ((j (* i i)))
                          (while (<= j limit)
                            (aset sieve j nil)
                            (setq j (+ j i))))))
                    ;; Collect primes
                    (let ((primes nil))
                      (dotimes (i (1+ limit))
                        (when (aref sieve i)
                          (setq primes (cons i primes))))
                      (nreverse primes)))"#;
    let expect = expect_test::expect![[r#""OK (2 3 5 7 11 13 17 19 23 29 31 37 41 43 47)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested dotimes with catch: find first pair summing to target
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dolist_dotimes_comp_nested_dotimes_catch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Two-sum problem: given a vector, find the first pair of distinct indices
    // whose values sum to the target. Use nested dotimes with early exit.
    let form = r#"(let ((nums [11 7 2 15 3 8 6 1 13 4])
                        (target 14))
                    (catch 'pair-found
                      (dotimes (i (length nums))
                        (dotimes (j (length nums))
                          (when (and (< i j)
                                     (= (+ (aref nums i) (aref nums j)) target))
                            (throw 'pair-found
                                   (list :indices (list i j)
                                         :values (list (aref nums i) (aref nums j))
                                         :sum target)))))
                      nil))"#;
    let expect = expect_test::expect![[r#""OK (:indices (0 4) :values (11 3) :sum 14)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
