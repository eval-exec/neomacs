//! Comprehensive oracle parity tests for while/loop patterns:
//! while with complex and/or conditions, setq accumulation,
//! nested while loops, dotimes with result and index,
//! dolist with result, catch/throw early exit, loop building
//! complex data structures, and mutation tracking.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// while with complex conditions (and/or combinations)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_while_complex_conditions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // and condition: both conditions must hold
    let form = r#"(let ((i 0) (j 100) (result nil))
                    (while (and (< i 10) (> j 90))
                      (setq result (cons (cons i j) result))
                      (setq i (1+ i))
                      (setq j (- j 2)))
                    (nreverse result))"#;
    let expect = expect_test::expect![[r#""OK ((0 . 100) (1 . 98) (2 . 96) (3 . 94) (4 . 92))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // or condition: either condition continues the loop
    let form2 = r#"(let ((x 0) (count 0) (phases nil))
                      (while (or (< x 5) (= (% x 3) 0))
                        (setq phases (cons (list x (< x 5) (= (% x 3) 0)) phases))
                        (setq x (1+ x))
                        (setq count (1+ count))
                        (when (> count 20) (setq x 100)))
                      (list count (nreverse phases)))"#;
    let expect =
        expect_test::expect![[r#""OK (5 ((0 t t) (1 t nil) (2 t nil) (3 t t) (4 t nil)))""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);

    // Nested and/or
    let form3 = r#"(let ((a 0) (b 20) (c 10) (steps nil))
                      (while (and (or (< a 8) (> b 15))
                                  (> c 0))
                        (setq steps (cons (list a b c) steps))
                        (setq a (1+ a))
                        (setq b (1- b))
                        (setq c (1- c)))
                      (list (length steps) (car (nreverse steps)) (car steps)))"#;
    let expect = expect_test::expect![[r#""OK (8 (0 20 10) (7 13 3))""#]];
    crate::common::assert_oracle_parity_expect(form3, expect);

    // not in condition
    let form4 = r#"(let ((lst '(1 2 3 nil 4 5)) (acc nil))
                      (while (and lst (not (null (car lst))))
                        (setq acc (cons (* (car lst) (car lst)) acc))
                        (setq lst (cdr lst)))
                      (nreverse acc))"#;
    let expect = expect_test::expect![[r#""OK (1 4 9)""#]];
    crate::common::assert_oracle_parity_expect(form4, expect);
}

// ---------------------------------------------------------------------------
// while with setq accumulation patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_while_setq_accumulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Running sum, product, and count simultaneously
    let form = r#"(let ((data '(3 1 4 1 5 9 2 6 5 3 5))
                        (rest nil)
                        (sum 0) (product 1) (count 0)
                        (min-val nil) (max-val nil)
                        (even-count 0) (odd-count 0))
                    (setq rest data)
                    (while rest
                      (let ((x (car rest)))
                        (setq sum (+ sum x))
                        (setq product (* product x))
                        (setq count (1+ count))
                        (when (or (null min-val) (< x min-val)) (setq min-val x))
                        (when (or (null max-val) (> x max-val)) (setq max-val x))
                        (if (= (% x 2) 0)
                            (setq even-count (1+ even-count))
                          (setq odd-count (1+ odd-count))))
                      (setq rest (cdr rest)))
                    (list sum product count min-val max-val even-count odd-count))"#;
    let expect = expect_test::expect![[r#""OK (44 486000 11 1 9 3 8)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Accumulate into multiple buckets
    let form2 = r#"(let ((nums '(15 22 3 47 8 31 12 45 6 29))
                         (rest nil)
                         (small nil) (medium nil) (large nil))
                      (setq rest nums)
                      (while rest
                        (let ((n (car rest)))
                          (cond
                            ((< n 10) (setq small (cons n small)))
                            ((< n 30) (setq medium (cons n medium)))
                            (t (setq large (cons n large)))))
                        (setq rest (cdr rest)))
                      (list (sort (nreverse small) #'<)
                            (sort (nreverse medium) #'<)
                            (sort (nreverse large) #'<)))"#;
    let expect = expect_test::expect![[r#""OK ((3 6 8) (12 15 22 29) (31 45 47))""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);

    // Fibonacci with while + multiple setq
    let form3 = r#"(let ((a 0) (b 1) (n 15) (fibs nil) (i 0))
                      (while (< i n)
                        (setq fibs (cons a fibs))
                        (let ((tmp (+ a b)))
                          (setq a b)
                          (setq b tmp))
                        (setq i (1+ i)))
                      (nreverse fibs))"#;
    let expect = expect_test::expect![[r#""OK (0 1 1 2 3 5 8 13 21 34 55 89 144 233 377)""#]];
    crate::common::assert_oracle_parity_expect(form3, expect);
}

// ---------------------------------------------------------------------------
// Nested while loops
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_while_nested_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Matrix transpose via nested while
    let form = r#"(let ((matrix '((1 2 3) (4 5 6) (7 8 9)))
                        (rows 3) (cols 3)
                        (result nil)
                        (j 0))
                    (while (< j cols)
                      (let ((col nil) (i 0))
                        (while (< i rows)
                          (setq col (cons (nth j (nth i matrix)) col))
                          (setq i (1+ i)))
                        (setq result (cons (nreverse col) result)))
                      (setq j (1+ j)))
                    (nreverse result))"#;
    let expect = expect_test::expect![[r#""OK ((1 4 7) (2 5 8) (3 6 9))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Generate multiplication table with inner accumulation
    let form2 = r#"(let ((result nil) (i 1))
                      (while (<= i 5)
                        (let ((row nil) (j 1))
                          (while (<= j 5)
                            (setq row (cons (* i j) row))
                            (setq j (1+ j)))
                          (setq result (cons (nreverse row) result)))
                        (setq i (1+ i)))
                      (nreverse result))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5) (2 4 6 8 10) (3 6 9 12 15) (4 8 12 16 20) (5 10 15 20 25))""#
    ]];
    crate::common::assert_oracle_parity_expect(form2, expect);

    // Triple nested: find Pythagorean triples
    let form3 = r#"(let ((triples nil) (limit 20) (a 1))
                      (while (< a limit)
                        (let ((b a))
                          (while (< b limit)
                            (let ((c b))
                              (while (<= c limit)
                                (when (= (+ (* a a) (* b b)) (* c c))
                                  (setq triples (cons (list a b c) triples)))
                                (setq c (1+ c))))
                            (setq b (1+ b))))
                        (setq a (1+ a)))
                      (nreverse triples))"#;
    let expect = expect_test::expect![[
        r#""OK ((3 4 5) (5 12 13) (6 8 10) (8 15 17) (9 12 15) (12 16 20))""#
    ]];
    crate::common::assert_oracle_parity_expect(form3, expect);
}

// ---------------------------------------------------------------------------
// dotimes with result form and index variable usage
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dotimes_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 45""#]];
    // Basic dotimes with result
    crate::common::assert_oracle_parity_expect(
        r#"(let ((sum 0)) (dotimes (i 10 sum) (setq sum (+ sum i))))"#,
        expect,
    );

    // dotimes building a list, result is the list
    let form = r#"(let ((result nil))
                    (dotimes (i 8 (nreverse result))
                      (setq result (cons (* i i) result))))"#;
    let expect = expect_test::expect![[r#""OK (0 1 4 9 16 25 36 49)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // dotimes with conditional accumulation
    let form2 = r#"(let ((evens nil) (odds nil))
                      (dotimes (i 12)
                        (if (= (% i 2) 0)
                            (setq evens (cons i evens))
                          (setq odds (cons i odds))))
                      (list (nreverse evens) (nreverse odds)))"#;
    let expect = expect_test::expect![[r#""OK ((0 2 4 6 8 10) (1 3 5 7 9 11))""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);

    // dotimes with index used in complex expressions
    let form3 = r#"(let ((table nil))
                      (dotimes (i 6 (nreverse table))
                        (setq table
                              (cons (list i
                                         (* i i)
                                         (* i i i)
                                         (if (= (% i 2) 0) 'even 'odd)
                                         (> i 3))
                                    table))))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 0 0 even nil) (1 1 1 odd nil) (2 4 8 even nil) (3 9 27 odd nil) (4 16 64 even t) (5 25 125 odd t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form3, expect);

    // Nested dotimes
    let form4 = r#"(let ((pairs nil))
                      (dotimes (i 4)
                        (dotimes (j 4)
                          (when (< i j)
                            (setq pairs (cons (list i j (+ i j)) pairs)))))
                      (nreverse pairs))"#;
    let expect =
        expect_test::expect![[r#""OK ((0 1 1) (0 2 2) (0 3 3) (1 2 3) (1 3 4) (2 3 5))""#]];
    crate::common::assert_oracle_parity_expect(form4, expect);

    let expect = expect_test::expect![[r#""OK 5""#]];
    // dotimes result form references loop variable (always COUNT after loop)
    crate::common::assert_oracle_parity_expect(r#"(dotimes (i 5 i))"#, expect);
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(r#"(dotimes (i 0 i))"#, expect);
    let expect = expect_test::expect![[r#""OK 42""#]];
    crate::common::assert_oracle_parity_expect(r#"(dotimes (i 0 42))"#, expect);
}

// ---------------------------------------------------------------------------
// dolist with result form
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dolist_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Basic dolist with result
    let form = r#"(let ((sum 0))
                    (dolist (x '(1 2 3 4 5) sum)
                      (setq sum (+ sum x))))"#;
    let expect = expect_test::expect![[r#""OK 15""#]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // dolist building reversed copy
    let form2 = r#"(let ((result nil))
                      (dolist (x '(a b c d e) (nreverse result))
                        (setq result (cons x result))))"#;
    let expect = expect_test::expect![[r#""OK (a b c d e)""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);

    // dolist with conditional, result is a filtered list
    let form3 = r#"(let ((positives nil))
                      (dolist (x '(-3 1 -2 4 0 -5 7 2) positives)
                        (when (> x 0)
                          (setq positives (cons x positives)))))"#;
    let expect = expect_test::expect![[r#""OK (2 7 4 1)""#]];
    crate::common::assert_oracle_parity_expect(form3, expect);

    // dolist transforming elements
    let form4 = r#"(let ((mapped nil))
                      (dolist (pair '((a . 1) (b . 2) (c . 3)) (nreverse mapped))
                        (setq mapped (cons (cons (cdr pair) (car pair)) mapped))))"#;
    let expect = expect_test::expect![[r#""OK ((1 . a) (2 . b) (3 . c))""#]];
    crate::common::assert_oracle_parity_expect(form4, expect);

    // dolist with nested dolist
    let form5 = r#"(let ((result nil))
                      (dolist (lst '((1 2) (3 4) (5 6)))
                        (dolist (x lst)
                          (setq result (cons (* x 10) result))))
                      (nreverse result))"#;
    let expect = expect_test::expect![[r#""OK (10 20 30 40 50 60)""#]];
    crate::common::assert_oracle_parity_expect(form5, expect);

    let expect = expect_test::expect![[r#""OK 42""#]];
    // dolist over empty list
    crate::common::assert_oracle_parity_expect(r#"(let ((x 42)) (dolist (e nil x)))"#, expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(r#"(dolist (e nil))"#, expect);

    let expect = expect_test::expect![[r#""OK nil""#]];
    // dolist result form is nil by default
    crate::common::assert_oracle_parity_expect(r#"(dolist (x '(1 2 3)))"#, expect);
}

// ---------------------------------------------------------------------------
// Loop with catch/throw for early exit
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_loop_catch_throw_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Early exit from while using catch/throw
    let form = r#"(catch 'found
                    (let ((i 0))
                      (while (< i 100)
                        (when (= (* i i) 2025)
                          (throw 'found (list 'found-at i)))
                        (setq i (1+ i)))
                      'not-found))"#;
    let expect = expect_test::expect![[r#""OK (found-at 45)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Early exit from nested loops
    let form2 = r#"(catch 'done
                      (let ((i 0))
                        (while (< i 10)
                          (let ((j 0))
                            (while (< j 10)
                              (when (= (+ (* i 10) j) 37)
                                (throw 'done (list i j)))
                              (setq j (1+ j))))
                          (setq i (1+ i)))))"#;
    let expect = expect_test::expect![[r#""OK (3 7)""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);

    // catch/throw with dolist
    let form3 = r#"(catch 'found
                      (dolist (x '(10 20 30 40 50 60 70))
                        (when (> x 45)
                          (throw 'found (list 'first-above-45 x)))))"#;
    let expect = expect_test::expect![[r#""OK (first-above-45 50)""#]];
    crate::common::assert_oracle_parity_expect(form3, expect);

    // Nested catch for multiple exit points
    let form4 = r#"(catch 'outer
                      (let ((sum 0))
                        (dotimes (i 20)
                          (catch 'skip
                            (when (= (% i 3) 0)
                              (throw 'skip nil))
                            (setq sum (+ sum i))
                            (when (> sum 30)
                              (throw 'outer (list 'overflow sum i)))))
                        (list 'completed sum)))"#;
    let expect = expect_test::expect![[r#""OK (overflow 37 10)""#]];
    crate::common::assert_oracle_parity_expect(form4, expect);

    // throw value propagation
    let form5 = r#"(let ((result (catch 'tag
                                    (let ((acc nil))
                                      (dolist (x '(1 2 3 4 5))
                                        (setq acc (cons (* x x) acc))
                                        (when (= x 3)
                                          (throw 'tag (nreverse acc))))
                                      (nreverse acc)))))
                      (list 'result result))"#;
    let expect = expect_test::expect![[r#""OK (result (1 4 9))""#]];
    crate::common::assert_oracle_parity_expect(form5, expect);
}

// ---------------------------------------------------------------------------
// Loop building complex data structures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_loop_build_complex_structures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build association list from parallel lists
    let form = r#"(let ((keys '(name age city score))
                        (vals '("Alice" 30 "NYC" 95))
                        (alist nil))
                    (while (and keys vals)
                      (setq alist (cons (cons (car keys) (car vals)) alist))
                      (setq keys (cdr keys))
                      (setq vals (cdr vals)))
                    (list (nreverse alist)
                          (cdr (assq 'name (nreverse alist)))
                          (cdr (assq 'age (nreverse alist)))))"#;
    let expect = expect_test::expect![[
        r#""OK (((name . \"Alice\") (age . 30) (city . \"NYC\") (score . 95)) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Build a tree-like structure (nested alists)
    let form2 = r#"(let ((tree nil) (i 0))
                      (while (< i 3)
                        (let ((children nil) (j 0))
                          (while (< j 3)
                            (setq children (cons (cons (+ (* i 10) j)
                                                       (format "node-%d-%d" i j))
                                                 children))
                            (setq j (1+ j)))
                          (setq tree (cons (cons (format "parent-%d" i)
                                                 (nreverse children))
                                           tree)))
                        (setq i (1+ i)))
                      (nreverse tree))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"parent-0\" (0 . \"node-0-0\") (1 . \"node-0-1\") (2 . \"node-0-2\")) (\"parent-1\" (10 . \"node-1-0\") (11 . \"node-1-1\") (12 . \"node-1-2\")) (\"parent-2\" (20 . \"node-2-0\") (21 . \"node-2-1\") (22 . \"node-2-2\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form2, expect);

    // Build vector from loop results
    let form3 = r#"(let ((v (make-vector 10 0))
                         (i 0))
                      (while (< i 10)
                        (aset v i (if (= (% i 2) 0) (* i i) (- i)))
                        (setq i (1+ i)))
                      (list v (aref v 4) (aref v 7)
                            (let ((sum 0) (k 0))
                              (while (< k 10)
                                (setq sum (+ sum (aref v k)))
                                (setq k (1+ k)))
                              sum)))"#;
    let expect = expect_test::expect![[r#""OK ([0 -1 4 -3 16 -5 36 -7 64 -9] 16 -7 95)""#]];
    crate::common::assert_oracle_parity_expect(form3, expect);

    // Build hash table from loop
    let form4 = r#"(let ((ht (make-hash-table :test 'equal))
                         (words '("the" "cat" "sat" "on" "the" "mat" "the" "cat")))
                      (dolist (w words)
                        (puthash w (1+ (gethash w ht 0)) ht))
                      (let ((entries nil))
                        (maphash (lambda (k v) (setq entries (cons (cons k v) entries))) ht)
                        (sort entries (lambda (a b) (string< (car a) (car b))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"cat\" . 2) (\"mat\" . 1) (\"on\" . 1) (\"sat\" . 1) (\"the\" . 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form4, expect);
}

// ---------------------------------------------------------------------------
// Loop with mutation tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_loop_mutation_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Track all state changes in a state machine
    let form = r#"(let ((state 'idle)
                        (events '(start process pause resume process finish reset start))
                        (log nil))
                    (dolist (event events)
                      (let ((old-state state))
                        (setq state
                              (cond
                                ((and (eq state 'idle) (eq event 'start)) 'running)
                                ((and (eq state 'running) (eq event 'process)) 'running)
                                ((and (eq state 'running) (eq event 'pause)) 'paused)
                                ((and (eq state 'paused) (eq event 'resume)) 'running)
                                ((and (eq state 'running) (eq event 'finish)) 'done)
                                ((eq event 'reset) 'idle)
                                (t state)))
                        (setq log (cons (list event old-state '-> state) log))))
                    (list (nreverse log) state))"#;
    let expect = expect_test::expect![[
        r#""OK (((start idle -> running) (process running -> running) (pause running -> paused) (resume paused -> running) (process running -> running) (finish running -> done) (reset done -> idle) (start idle -> running)) running)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Track mutations on a list (cons cell sharing)
    let form2 = r#"(let* ((original '(1 2 3 4 5))
                          (copy (copy-sequence original))
                          (reversed (reverse original))
                          (sorted-copy (sort (copy-sequence original) #'>)))
                     (list original copy reversed sorted-copy
                           (equal original copy)
                           (equal original '(1 2 3 4 5))))"#;
    let expect =
        expect_test::expect![[r#""OK ((1 2 3 4 5) (1 2 3 4 5) (5 4 3 2 1) (5 4 3 2 1) t t)""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);

    // Accumulator pattern with history
    let form3 = r#"(let ((value 0) (history nil) (ops '((add 5) (mul 3) (sub 7) (add 2) (mul 2))))
                      (dolist (op ops)
                        (let ((old value))
                          (setq value
                                (cond
                                  ((eq (car op) 'add) (+ value (cadr op)))
                                  ((eq (car op) 'sub) (- value (cadr op)))
                                  ((eq (car op) 'mul) (* value (cadr op)))
                                  (t value)))
                          (setq history (cons (list (car op) (cadr op) old '-> value) history))))
                      (list value (nreverse history)))"#;
    let expect = expect_test::expect![[
        r#""OK (20 ((add 5 0 -> 5) (mul 3 5 -> 15) (sub 7 15 -> 8) (add 2 8 -> 10) (mul 2 10 -> 20)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form3, expect);

    // Bubble sort with swap counting
    let form4 = r#"(let ((v (vector 5 3 8 1 9 2 7 4 6))
                         (n 9) (swaps 0) (passes 0))
                      (let ((changed t))
                        (while changed
                          (setq changed nil)
                          (setq passes (1+ passes))
                          (let ((i 0))
                            (while (< i (1- n))
                              (when (> (aref v i) (aref v (1+ i)))
                                (let ((tmp (aref v i)))
                                  (aset v i (aref v (1+ i)))
                                  (aset v (1+ i) tmp))
                                (setq changed t)
                                (setq swaps (1+ swaps)))
                              (setq i (1+ i))))))
                      (list v swaps passes))"#;
    let expect = expect_test::expect![[r#""OK ([1 2 3 4 5 6 7 8 9] 17 5)""#]];
    crate::common::assert_oracle_parity_expect(form4, expect);
}
