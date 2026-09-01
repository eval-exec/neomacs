//! Advanced oracle parity tests for `if`, `when`, `unless`, and `cond`:
//! complex predicate combinations, progn bodies, return value semantics,
//! nested decision trees, cond fallthrough, side effects in branches,
//! and pattern-matching simulation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// if with complex test expressions (and/or combinations)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_if_complex_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Nested and/or/not combinations as if test
    let form = r#"(let ((results nil))
      (dolist (triple '((1 2 3) (0 2 3) (1 0 3) (1 2 0)
                        (-1 -2 -3) (10 20 30) (0 0 0)))
        (let ((a (car triple)) (b (cadr triple)) (c (caddr triple)))
          (setq results
                (cons
                 (list
                  ;; and of comparisons
                  (if (and (> a 0) (> b 0) (> c 0)) 'all-pos 'not-all-pos)
                  ;; or with nested and
                  (if (or (and (> a 5) (> b 5))
                          (and (< a 0) (< b 0)))
                      'extreme 'moderate)
                  ;; not with and/or
                  (if (not (or (= a 0) (= b 0) (= c 0)))
                      'no-zeros 'has-zero)
                  ;; deeply nested
                  (if (and (or (> a 0) (> b 0))
                           (not (and (= a 0) (= b 0)))
                           (or (> c 0) (and (< a 0) (< b 0))))
                      'complex-true 'complex-false))
                 results))))
      (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((all-pos moderate no-zeros complex-true) (not-all-pos moderate has-zero complex-true) (not-all-pos moderate has-zero complex-true) (not-all-pos moderate has-zero complex-false) (not-all-pos extreme no-zeros complex-false) (all-pos extreme no-zeros complex-true) (not-all-pos moderate has-zero complex-false))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// if with multi-form then body via progn, return values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_if_progn_bodies_and_returns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // progn in then and else branches; verify return value is last form
    let form = r#"(let ((log nil))
      (let ((r1 (if t
                    (progn
                      (setq log (cons 'then-1 log))
                      (setq log (cons 'then-2 log))
                      (setq log (cons 'then-3 log))
                      'then-result)
                  (progn
                    (setq log (cons 'else log))
                    'else-result)))
            (r2 (if nil
                    (progn
                      (setq log (cons 'bad log))
                      'should-not-happen)
                  (progn
                    (setq log (cons 'else-a log))
                    (setq log (cons 'else-b log))
                    (* 6 7))))
            ;; if without else returns nil
            (r3 (if nil 'unreachable))
            ;; if with else that has multiple forms (implicit progn)
            (r4 (if nil 'nope 'first-else 'second-else 'third-else)))
        (list r1 r2 r3 r4 (nreverse log))))"#;
    let expect = expect_test::expect![[
        r#""OK (then-result 42 nil third-else (then-1 then-2 then-3 else-a else-b))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// when/unless return values (nil for false branch)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_when_unless_return_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // when returns nil when condition is false, last body form when true
    // unless returns nil when condition is true, last body form when false
    let form = r#"(let ((side nil))
      (list
       ;; when true: returns last body form
       (when t (setq side (cons 'w1 side)) (setq side (cons 'w2 side)) 'when-true-val)
       ;; when false: returns nil, no side effects
       (when nil (setq side (cons 'bad side)) 'when-false-val)
       ;; unless false: returns last body form
       (unless nil (setq side (cons 'u1 side)) 'unless-false-val)
       ;; unless true: returns nil, no side effects
       (unless t (setq side (cons 'bad2 side)) 'unless-true-val)
       ;; Nested when inside when
       (when (> 5 3)
         (when (< 1 2)
           (setq side (cons 'nested side))
           'deep-when-val))
       ;; unless with complex predicate
       (unless (and nil t)
         (setq side (cons 'unless-complex side))
         'unless-complex-val)
       ;; when with 0 (0 is truthy in Elisp!)
       (when 0 'zero-is-truthy)
       ;; when with empty string (also truthy)
       (when "" 'empty-string-truthy)
       ;; Side effect log
       (nreverse side)))"#;
    let expect = expect_test::expect![[
        r#""OK (when-true-val nil unless-false-val nil deep-when-val unless-complex-val zero-is-truthy empty-string-truthy (w1 w2 u1 nested unless-complex))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested if/cond for complex multi-level dispatch
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_if_cond_nested_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multi-level dispatch: classify geometric shapes by type and size
    let form = r#"(let ((classify
                   (lambda (shape width height)
                     (cond
                      ((eq shape 'circle)
                       (let ((radius width))
                         (if (> radius 100) 'large-circle
                           (if (> radius 50) 'medium-circle
                             (if (> radius 10) 'small-circle
                               'tiny-circle)))))
                      ((eq shape 'rect)
                       (cond
                        ((= width height)
                         (if (> width 100) 'large-square 'small-square))
                        ((> width height)
                         (if (> (/ width height) 3) 'wide-rect 'rect))
                        (t
                         (if (> (/ height width) 3) 'tall-rect 'rect))))
                      ((eq shape 'triangle)
                       (if (= width height)
                           (if (> width 50) 'large-equilateral 'small-equilateral)
                         'scalene))
                      (t 'unknown)))))
      (list
       (funcall classify 'circle 200 0)
       (funcall classify 'circle 75 0)
       (funcall classify 'circle 30 0)
       (funcall classify 'circle 5 0)
       (funcall classify 'rect 100 100)
       (funcall classify 'rect 20 20)
       (funcall classify 'rect 400 10)
       (funcall classify 'rect 50 30)
       (funcall classify 'rect 10 400)
       (funcall classify 'triangle 60 60)
       (funcall classify 'triangle 30 30)
       (funcall classify 'triangle 40 20)
       (funcall classify 'polygon 10 10)))"#;
    let expect = expect_test::expect![[
        r#""OK (large-circle medium-circle small-circle tiny-circle small-square small-square wide-rect rect tall-rect large-equilateral small-equilateral scalene unknown)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cond with t as catch-all and body-less clause fallthrough
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cond_fallthrough_and_catchall() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Body-less cond clauses return the test value itself
    // Multiple fallthrough cases testing various truthy values
    let form = r#"(list
      ;; Body-less: returns the truthy test value itself
      (cond (42))
      (cond (nil) ("hello"))
      (cond (nil) (nil) ((+ 10 20)))
      ;; t catch-all with no prior match
      (cond ((= 1 2) 'wrong) ((= 3 4) 'also-wrong) (t 'catch-all))
      ;; First truthy clause wins, rest skipped
      (let ((counter 0))
        (cond
         (nil (setq counter (1+ counter)))
         (t (setq counter (+ counter 10)))
         (t (setq counter (+ counter 100))))  ;; never reached
        counter)
      ;; cond with only t clause
      (cond (t 'only-option))
      ;; cond with all nil tests returns nil
      (cond (nil 'a) (nil 'b) (nil 'c))
      ;; cond with test being a list (always truthy)
      (cond ('(1 2 3)))
      ;; Complex: cond computing value through test expression
      (let ((x 42))
        (cond
         ((and (> x 0) (< x 10)) 'small)
         ((and (>= x 10) (< x 50))
          (let ((decade (/ x 10)))
            (list 'medium decade)))
         ((>= x 50) 'large))))"#;
    let expect = expect_test::expect![[
        r#""OK (42 \"hello\" 30 catch-all 10 only-option nil (1 2 3) (medium 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// if with side effects in both branches
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_if_side_effects_both_branches() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Side effects must only execute in the taken branch
    let form = r#"(let ((trace nil))
      ;; Run a series of ifs, accumulating which branches executed
      (let ((vals
             (list
              (if (> 10 5)
                  (progn (setq trace (cons 'a-then trace)) 'took-then)
                (progn (setq trace (cons 'a-else trace)) 'took-else))
              (if (< 10 5)
                  (progn (setq trace (cons 'b-then trace)) 'took-then)
                (progn (setq trace (cons 'b-else trace)) 'took-else))
              ;; Nested with side effects at each level
              (if t
                  (if nil
                      (progn (setq trace (cons 'c-inner-then trace)) 1)
                    (progn (setq trace (cons 'c-inner-else trace)) 2))
                (progn (setq trace (cons 'c-outer-else trace)) 3))
              ;; when/unless side effects
              (progn
                (when (> 3 2) (setq trace (cons 'when-fire trace)))
                (when (< 3 2) (setq trace (cons 'when-skip trace)))
                (unless (> 3 2) (setq trace (cons 'unless-skip trace)))
                (unless (< 3 2) (setq trace (cons 'unless-fire trace)))
                'done)
              ;; Mutating a shared counter from different branches
              (let ((n 0))
                (dotimes (i 6)
                  (if (= (% i 2) 0)
                      (setq n (+ n i))
                    (setq n (- n i))))
                n))))
        (list vals (nreverse trace))))"#;
    let expect = expect_test::expect![[
        r#""OK ((took-then took-else 2 done -3) (a-then b-else c-inner-else when-fire unless-fire))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: decision tree classifier
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_if_decision_tree() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build and traverse a decision tree represented as nested lists
    // Tree node: (feature threshold left-child right-child)
    // Leaf node: (leaf label)
    let form = r#"(progn
  (fset 'neovm--test-dt-classify
    (lambda (tree features)
      (if (eq (car tree) 'leaf)
          (cadr tree)
        (let ((feature-name (nth 0 tree))
              (threshold (nth 1 tree))
              (left (nth 2 tree))
              (right (nth 3 tree)))
          (let ((val (cdr (assq feature-name features))))
            (if (<= val threshold)
                (funcall 'neovm--test-dt-classify left features)
              (funcall 'neovm--test-dt-classify right features)))))))

  (unwind-protect
      (let ((tree
             ;; Decision tree for classifying animals
             ;; has-fur? -> has-claws? -> big? -> ...
             '(has-fur 0
               ;; no fur path
               (has-feathers 0
                 (leaf fish)
                 (can-fly 0
                   (leaf penguin)
                   (leaf bird)))
               ;; has fur path
               (has-claws 0
                 ;; no claws
                 (size 50
                   (leaf rabbit)
                   (leaf horse))
                 ;; has claws
                 (size 100
                   (leaf cat)
                   (leaf bear))))))
        (list
         ;; fish: no fur, no feathers
         (funcall 'neovm--test-dt-classify tree
                  '((has-fur . 0) (has-feathers . 0) (can-fly . 0) (has-claws . 0) (size . 10)))
         ;; bird: no fur, feathers, can fly
         (funcall 'neovm--test-dt-classify tree
                  '((has-fur . 0) (has-feathers . 1) (can-fly . 1) (has-claws . 0) (size . 5)))
         ;; penguin: no fur, feathers, can't fly
         (funcall 'neovm--test-dt-classify tree
                  '((has-fur . 0) (has-feathers . 1) (can-fly . 0) (has-claws . 0) (size . 20)))
         ;; rabbit: fur, no claws, small
         (funcall 'neovm--test-dt-classify tree
                  '((has-fur . 1) (has-feathers . 0) (can-fly . 0) (has-claws . 0) (size . 10)))
         ;; horse: fur, no claws, big
         (funcall 'neovm--test-dt-classify tree
                  '((has-fur . 1) (has-feathers . 0) (can-fly . 0) (has-claws . 0) (size . 200)))
         ;; cat: fur, claws, small
         (funcall 'neovm--test-dt-classify tree
                  '((has-fur . 1) (has-feathers . 0) (can-fly . 0) (has-claws . 1) (size . 20)))
         ;; bear: fur, claws, big
         (funcall 'neovm--test-dt-classify tree
                  '((has-fur . 1) (has-feathers . 0) (can-fly . 0) (has-claws . 1) (size . 200)))))
    (fmakunbound 'neovm--test-dt-classify)))"#;
    let expect = expect_test::expect![[r#""OK (fish bird penguin rabbit horse cat bear)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: pattern matching simulation with cond
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cond_pattern_matching_simulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate algebraic data type matching using cond:
    // Expressions: (num N), (var X), (add E1 E2), (mul E1 E2), (neg E)
    // Operations: simplify, evaluate, pretty-print
    let form = r#"(progn
  (fset 'neovm--test-expr-simplify
    (lambda (expr)
      (cond
       ;; Atoms pass through
       ((eq (car expr) 'num) expr)
       ((eq (car expr) 'var) expr)
       ;; Negation: --x = x, -0 = 0
       ((eq (car expr) 'neg)
        (let ((inner (funcall 'neovm--test-expr-simplify (cadr expr))))
          (cond
           ((and (eq (car inner) 'num) (= (cadr inner) 0)) '(num 0))
           ((eq (car inner) 'neg) (cadr inner))
           (t (list 'neg inner)))))
       ;; Addition rules
       ((eq (car expr) 'add)
        (let ((left (funcall 'neovm--test-expr-simplify (cadr expr)))
              (right (funcall 'neovm--test-expr-simplify (caddr expr))))
          (cond
           ;; x + 0 = x
           ((and (eq (car right) 'num) (= (cadr right) 0)) left)
           ;; 0 + x = x
           ((and (eq (car left) 'num) (= (cadr left) 0)) right)
           ;; const + const
           ((and (eq (car left) 'num) (eq (car right) 'num))
            (list 'num (+ (cadr left) (cadr right))))
           ;; x + x = 2*x
           ((equal left right) (list 'mul '(num 2) left))
           (t (list 'add left right)))))
       ;; Multiplication rules
       ((eq (car expr) 'mul)
        (let ((left (funcall 'neovm--test-expr-simplify (cadr expr)))
              (right (funcall 'neovm--test-expr-simplify (caddr expr))))
          (cond
           ;; x * 0 = 0
           ((and (eq (car right) 'num) (= (cadr right) 0)) '(num 0))
           ((and (eq (car left) 'num) (= (cadr left) 0)) '(num 0))
           ;; x * 1 = x
           ((and (eq (car right) 'num) (= (cadr right) 1)) left)
           ((and (eq (car left) 'num) (= (cadr left) 1)) right)
           ;; const * const
           ((and (eq (car left) 'num) (eq (car right) 'num))
            (list 'num (* (cadr left) (cadr right))))
           (t (list 'mul left right)))))
       (t expr))))

  (fset 'neovm--test-expr-pretty
    (lambda (expr)
      (cond
       ((eq (car expr) 'num) (number-to-string (cadr expr)))
       ((eq (car expr) 'var) (symbol-name (cadr expr)))
       ((eq (car expr) 'neg)
        (concat "-" (funcall 'neovm--test-expr-pretty (cadr expr))))
       ((eq (car expr) 'add)
        (concat "(" (funcall 'neovm--test-expr-pretty (cadr expr))
                " + " (funcall 'neovm--test-expr-pretty (caddr expr)) ")"))
       ((eq (car expr) 'mul)
        (concat "(" (funcall 'neovm--test-expr-pretty (cadr expr))
                " * " (funcall 'neovm--test-expr-pretty (caddr expr)) ")"))
       (t "?"))))

  (unwind-protect
      (let ((cases
             (list
              ;; 0 + x -> x
              '(add (num 0) (var x))
              ;; x + 0 -> x
              '(add (var x) (num 0))
              ;; 3 + 4 -> 7
              '(add (num 3) (num 4))
              ;; x + x -> 2*x
              '(add (var x) (var x))
              ;; x * 0 -> 0
              '(mul (var x) (num 0))
              ;; x * 1 -> x
              '(mul (var x) (num 1))
              ;; --x -> x
              '(neg (neg (var y)))
              ;; -0 -> 0
              '(neg (num 0))
              ;; (2+3) * (1+0) -> 5
              '(mul (add (num 2) (num 3)) (add (num 1) (num 0)))
              ;; nested: (x+0) + (0+y) -> x + y
              '(add (add (var x) (num 0)) (add (num 0) (var y))))))
        (mapcar (lambda (expr)
                  (let ((simplified (funcall 'neovm--test-expr-simplify expr)))
                    (list (funcall 'neovm--test-expr-pretty expr)
                          (funcall 'neovm--test-expr-pretty simplified))))
                cases))
    (fmakunbound 'neovm--test-expr-simplify)
    (fmakunbound 'neovm--test-expr-pretty)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"(0 + x)\" \"x\") (\"(x + 0)\" \"x\") (\"(3 + 4)\" \"7\") (\"(x + x)\" \"(2 * x)\") (\"(x * 0)\" \"0\") (\"(x * 1)\" \"x\") (\"--y\" \"y\") (\"-0\" \"0\") (\"((2 + 3) * (1 + 0))\" \"5\") (\"((x + 0) + (0 + y))\" \"(x + y)\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
