//! Comprehensive oracle parity tests for `cl-loop`:
//! `for ... in`, `for ... on`, `for ... across`, `for ... from ... to ... by`,
//! `for ... = ... then`, `collect`, `append`, `nconc`, `sum`, `count`,
//! `maximize`, `minimize`, `while`, `until`, `when`, `unless`, `with`,
//! `finally`, multiple `for` clauses (parallel iteration), destructuring.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// for ... = ... then (general iteration variable)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_for_eq_then() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; Simple for = then: powers of 2
    (cl-loop for x = 1 then (* x 2)
             repeat 10
             collect x)
    ;; Two variables with = then: Collatz-like sequence from 7
    (cl-loop for x = 7 then (if (= (% x 2) 0) (/ x 2) (1+ (* 3 x)))
             for i from 0 to 15
             collect x)
    ;; Accumulate running sum with = then
    (cl-loop for x in '(10 20 30 40 50)
             for running = 0 then (+ running x)
             collect (+ running x))
    ;; Fibonacci via two = then vars
    (cl-loop for a = 0 then b
             for b = 1 then (+ a b)
             repeat 12
             collect a)
    ;; Geometric series: 1, 1/2, 1/4, ...  accumulated
    (cl-loop for term = 1000 then (/ term 2)
             for i from 0 to 9
             sum term)
    ;; Iterate a function: repeatedly apply (lambda (x) (+ x (% x 7)))
    (cl-loop for x = 1 then (+ x (% x 7))
             repeat 8
             collect x)))"#;
    let expect = expect_test::expect![
        r#""OK ((1 2 4 8 16 32 64 128 256 512) (7 22 11 34 17 52 26 13 40 20 10 5 16 8 4 2) (10 40 80 130 190) (0 1 2 4 8 16 32 64 128 256 512 1024) 1994 (1 2 4 8 9 11 15 16))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Destructuring in for-in with complex patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_destructuring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; Destructure dotted pairs
    (cl-loop for (key . val) in '((a . 1) (b . 2) (c . 3) (d . 4))
             collect (list val key))
    ;; Destructure full lists
    (cl-loop for (name age city) in '(("Alice" 30 "NYC")
                                       ("Bob" 25 "LA")
                                       ("Carol" 35 "SF"))
             collect (format "%s (%d) from %s" name age city))
    ;; Nested destructuring
    (cl-loop for (x (y z)) in '((1 (2 3)) (4 (5 6)) (7 (8 9)))
             collect (+ x y z))
    ;; Destructure with nil padding (shorter sublists)
    (cl-loop for (a b c) in '((1 2 3) (4 5) (6))
             collect (list a b c))
    ;; Destructure in for-on (tails)
    (cl-loop for (first second . rest) on '(1 2 3 4 5 6 7)
             when second
             collect (list first second (length rest)))
    ;; Mix destructuring with regular for
    (cl-loop for (key . val) in '((x . 10) (y . 20) (z . 30))
             for i from 1
             collect (list i key (* val i)))))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 a) (2 b) (3 c) (4 d)) (\"Alice (30) from NYC\" \"Bob (25) from LA\" \"Carol (35) from SF\") (6 15 24) ((1 2 3) (4 5 nil) (6 nil nil)) ((1 2 5) (2 3 4) (3 4 3) (4 5 2) (5 6 1) (6 7 0)) ((1 x 10) (2 y 40) (3 z 90)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// while / until clauses with various accumulators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_while_until() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; while: collect numbers from list until hitting zero
    (cl-loop for x in '(3 7 2 0 5 8 1)
             while (> x 0)
             collect x)
    ;; until: collect until hitting a negative
    (cl-loop for x in '(5 3 8 2 -1 4 7)
             until (< x 0)
             collect x)
    ;; while with sum
    (cl-loop for x in '(1 2 3 4 5 6 7 8 9 10)
             for running-sum = x then (+ running-sum x)
             while (<= running-sum 20)
             collect x)
    ;; until with count
    (cl-loop for x in '(1 1 1 1 2 1 1 1)
             until (= x 2)
             count t)
    ;; Combine while with when
    (cl-loop for x from 1
             while (<= (* x x) 100)
             when (= (% x 2) 0)
             collect (* x x))
    ;; until in numeric range
    (cl-loop for i from 0
             for fib = 1 then (+ fib prev-fib)
             for prev-fib = 0 then (- fib prev-fib)
             until (> fib 100)
             collect fib)
    ;; while with maximize
    (cl-loop for x in '(5 12 3 18 7 2 25 1)
             for i from 0
             while (< i 6)
             maximize x)))"#;
    let expect = expect_test::expect![
        r#""OK ((3 7 2) (5 3 8 2) (1 2 3 4 5) 4 (4 16 36 64 100) (1 1 2 3 5 8 13 21 34 55 89) 18)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple accumulators: collect into, sum into, count into
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_multiple_accumulators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; Partition into evens and odds using collect into
    (cl-loop for x in '(1 2 3 4 5 6 7 8 9 10)
             if (= (% x 2) 0)
               collect x into evens
             else
               collect x into odds
             end
             finally return (list evens odds))
    ;; Simultaneously sum, count, and collect
    (cl-loop for x in '(3 -1 4 -1 5 -9 2 -6 5 3)
             if (> x 0)
               sum x into pos-sum and count t into pos-count
             else
               count t into neg-count
             end
             finally return (list pos-sum pos-count neg-count))
    ;; Partition strings by length
    (cl-loop for s in '("a" "bb" "ccc" "dd" "e" "fff" "gg" "h")
             if (<= (length s) 1)
               collect s into short
             else if (<= (length s) 2)
               collect s into medium
             else
               collect s into long
             end
             finally return (list short medium long))
    ;; Collect and maximize/minimize simultaneously
    (cl-loop for x in '(4 2 7 1 8 3 6 5)
             collect (* x x) into squares
             maximize x into biggest
             minimize x into smallest
             finally return (list squares biggest smallest))
    ;; Append into with filter
    (cl-loop for pair in '((a 1 2) (b 3) (c 4 5 6) (d) (e 7 8))
             when (> (length pair) 1)
               append (cdr pair) into values
             end
             finally return values)))"#;
    let expect = expect_test::expect![[
        r#""OK (((2 4 6 8 10) (1 3 5 7 9)) (22 6 4) ((\"a\" \"e\" \"h\") (\"bb\" \"dd\" \"gg\") (\"ccc\" \"fff\")) ((16 4 49 1 64 9 36 25) 8 1) (1 2 3 4 5 6 7 8))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Parallel iteration with mismatched lengths and complex for combos
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_parallel_iteration_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; Three parallel lists of different lengths (stops at shortest)
    (cl-loop for a in '(1 2 3 4 5)
             for b in '(10 20 30)
             for c in '(100 200 300 400)
             collect (+ a b c))
    ;; Parallel: list + vector + range
    (cl-loop for x in '(a b c d)
             for y across [10 20 30 40]
             for i from 0
             collect (list i x y))
    ;; Parallel: range + = then
    (cl-loop for i from 1 to 8
             for sq = 1 then (* (1+ sq-root) (1+ sq-root))
             for sq-root = 0 then (1+ sq-root)
             collect (list i sq))
    ;; Parallel with on and in
    (cl-loop for x in '(a b c d e)
             for rest on '(1 2 3 4 5)
             collect (list x (car rest) (length rest)))
    ;; Compute dot product
    (cl-loop for a in '(1 2 3 4 5)
             for b in '(5 4 3 2 1)
             sum (* a b))
    ;; Running max with parallel index
    (cl-loop for x in '(3 1 4 1 5 9 2 6 5 3 5)
             for i from 0
             for running-max = x then (max running-max x)
             when (= x running-max)
             collect (list i x))))"#;
    let expect = expect_test::expect![
        r#""OK ((111 222 333) ((0 a 10) (1 b 20) (2 c 30) (3 d 40)) ((1 1) (2 1) (3 4) (4 9) (5 16) (6 25) (7 36) (8 49)) ((a 1 5) (b 2 4) (c 3 3) (d 4 2) (e 5 1)) 35 ((0 3) (2 4) (4 5) (5 9)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// finally clause variations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_finally_clause() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; finally return with computed value
    (cl-loop for x in '(1 2 3 4 5)
             for product = 1 then (* product x)
             finally return product)
    ;; finally return combining multiple with-vars
    (cl-loop with min-val = most-positive-fixnum
             with max-val = most-negative-fixnum
             with total = 0
             for x in '(15 3 27 8 42 1 19)
             do (setq min-val (min min-val x)
                      max-val (max max-val x)
                      total (+ total x))
             finally return (list min-val max-val total
                                  (/ total 7)))
    ;; finally return with conditional
    (cl-loop for x in '(2 4 6 8 10 12)
             for i from 0
             when (> x 9)
             return (list 'found-gt-9 i x)
             finally return (list 'none-found))
    ;; Build an alist in finally
    (cl-loop for k in '(name age city email)
             for v in '("Alice" 30 "NYC" "alice@example.com")
             collect (cons k v) into pairs
             finally return (list pairs (length pairs)
                                  (assq 'city pairs)))))"#;
    let expect = expect_test::expect![[
        r#""OK (120 (1 42 115 16) (found-gt-9 4 10) (((name . \"Alice\") (age . 30) (city . \"NYC\") (email . \"alice@example.com\")) 4 (city . \"NYC\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// nconc accumulator and append with transformations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_nconc_append_advanced() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; nconc with copy to avoid mutation issues
    (cl-loop for x in '((1 2 3) (4 5) (6 7 8 9) (10))
             nconc (copy-sequence x))
    ;; append: interleave element with separator
    (cl-loop for x in '("hello" "beautiful" "world")
             for i from 0
             if (> i 0)
               append (list " " x)
             else
               append (list x)
             end)
    ;; nconc with generated sublists
    (cl-loop for i from 1 to 5
             nconc (cl-loop for j from 1 to i collect (list i j)))
    ;; append filtered sublists
    (cl-loop for sublist in '((1 -2 3) (-4 5 -6) (7 -8 9))
             append (cl-loop for x in sublist when (> x 0) collect x))
    ;; Flatten deeply nested via append
    (cl-loop for outer in '(((a b) (c d)) ((e f) (g h)) ((i j)))
             append (cl-loop for inner in outer append inner))
    ;; nconc with reverse of each sublist
    (cl-loop for sub in '((1 2 3) (4 5 6) (7 8 9))
             nconc (reverse (copy-sequence sub)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7 8 9 10) (\"hello\" \" \" \"beautiful\" \" \" \"world\") ((1 1) (2 1) (2 2) (3 1) (3 2) (3 3) (4 1) (4 2) (4 3) (4 4) (5 1) (5 2) (5 3) (5 4) (5 5)) (1 3 5 7 9) (a b c d e f g h i j) (3 2 1 6 5 4 9 8 7))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Real-world-like: data processing pipeline with cl-loop
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_data_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (let ((transactions '((1 "Alice" "buy"  100)
                         (2 "Bob"   "sell" 200)
                         (3 "Alice" "buy"  150)
                         (4 "Carol" "buy"  300)
                         (5 "Bob"   "buy"  50)
                         (6 "Alice" "sell" 80)
                         (7 "Carol" "sell" 120)
                         (8 "Bob"   "buy"  175)
                         (9 "Alice" "buy"  90)
                         (10 "Carol" "buy" 250))))
    (list
      ;; Total buy amount
      (cl-loop for (id name type amount) in transactions
               when (string= type "buy")
               sum amount)
      ;; Total sell amount
      (cl-loop for (id name type amount) in transactions
               when (string= type "sell")
               sum amount)
      ;; Number of transactions per person (sorted by name)
      (let ((counts (cl-loop for (id name type amount) in transactions
                             with ht = (make-hash-table :test 'equal)
                             do (puthash name (1+ (gethash name ht 0)) ht)
                             finally return ht)))
        (sort (cl-loop for k being the hash-keys of counts
                       using (hash-values v)
                       collect (cons k v))
              (lambda (a b) (string< (car a) (car b)))))
      ;; Largest single transaction
      (cl-loop for (id name type amount) in transactions
               maximize amount)
      ;; Collect names of people who both bought and sold
      (let ((buyers (cl-loop for (id name type amount) in transactions
                             when (string= type "buy")
                             collect name))
            (sellers (cl-loop for (id name type amount) in transactions
                              when (string= type "sell")
                              collect name)))
        (sort (cl-remove-duplicates
               (cl-loop for s in sellers
                        when (member s buyers)
                        collect s)
               :test 'string=)
              'string<))
      ;; Running balance per person (Alice only)
      (cl-loop for (id name type amount) in transactions
               with balance = 0
               when (string= name "Alice")
               do (setq balance (if (string= type "buy")
                                    (- balance amount)
                                  (+ balance amount)))
               and collect (list id balance)))))"#;
    let expect = expect_test::expect![[
        r#""OK (1115 400 ((\"Alice\" . 4) (\"Bob\" . 3) (\"Carol\" . 3)) 300 (\"Alice\" \"Bob\" \"Carol\") ((1 -100) (3 -250) (6 -170) (9 -260)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested cl-loop and advanced control flow
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_nested_and_control() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; Nested loops: multiplication table
    (cl-loop for i from 1 to 4
             collect
             (cl-loop for j from 1 to 4
                      collect (* i j)))
    ;; thereis: find first pair that sums to 10
    (cl-loop for x in '(1 3 5 7 9)
             thereis (cl-loop for y in '(1 3 5 7 9)
                              thereis (and (= (+ x y) 10)
                                          (list x y))))
    ;; always: check all elements are positive
    (list
      (cl-loop for x in '(1 2 3 4 5) always (> x 0))
      (cl-loop for x in '(1 2 -3 4 5) always (> x 0)))
    ;; never: check no element is zero
    (list
      (cl-loop for x in '(1 2 3 4 5) never (= x 0))
      (cl-loop for x in '(1 2 0 4 5) never (= x 0)))
    ;; return from middle of loop
    (cl-loop for x in '(10 20 30 40 50)
             for i from 0
             when (= x 30)
             return (list 'found-at i))
    ;; Matrix transpose via nested loops
    (let ((matrix '((1 2 3) (4 5 6) (7 8 9))))
      (cl-loop for col from 0 to 2
               collect
               (cl-loop for row in matrix
                        collect (nth col row))))
    ;; Collect with do side-effects and complex when/unless
    (cl-loop for x from 1 to 20
             when (= (% x 3) 0)
               collect (list x 'fizz) into result
             else when (= (% x 5) 0)
               collect (list x 'buzz) into result
             end
             finally return result)))"#;
    let expect = expect_test::expect![
        r#""OK (((1 2 3 4) (2 4 6 8) (3 6 9 12) (4 8 12 16)) (1 9) (t nil) (t nil) (found-at 2) ((1 4 7) (2 5 8) (3 6 9)) ((3 fizz) (5 buzz) (6 fizz) (9 fizz) (10 buzz) (12 fizz) (15 fizz) (18 fizz) (20 buzz)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}
