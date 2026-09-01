//! Complex oracle tests for functional programming patterns in Elisp.
//!
//! Tests higher-order functions, composition, transducers, and
//! functional data-processing pipelines.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Higher-order function combinators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_functional_compose_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a compose function and chain transformations
    let form = "(let ((compose
                       (lambda (f g)
                         (lambda (x) (funcall f (funcall g x)))))
                      (add1 (lambda (x) (+ x 1)))
                      (double (lambda (x) (* x 2)))
                      (square (lambda (x) (* x x))))
                  (let ((add1-then-double
                         (funcall compose double add1))
                        (double-then-square
                         (funcall compose square double))
                        (triple-compose
                         (funcall compose
                                  (funcall compose square double)
                                  add1)))
                    (list (funcall add1-then-double 3)
                          (funcall double-then-square 3)
                          (funcall triple-compose 3))))";
    let expect = expect_test::expect![[r#""OK (8 36 64)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_functional_partial_application() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Manual partial application (currying)
    let form = "(let ((partial
                       (lambda (f &rest initial-args)
                         (lambda (&rest more-args)
                           (apply f (append initial-args more-args))))))
                  (let ((add (lambda (a b) (+ a b)))
                        (mul (lambda (a b c) (* a b c))))
                    (let ((add5 (funcall partial add 5))
                          (mul-by-2-3 (funcall partial mul 2 3)))
                      (list (funcall add5 10)
                            (funcall add5 0)
                            (funcall mul-by-2-3 7)))))";
    let expect = expect_test::expect![[r#""OK (15 5 42)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_functional_flip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Flip argument order
    let form = "(let ((flip (lambda (f)
                      (lambda (a b) (funcall f b a)))))
                  (let ((sub (lambda (a b) (- a b)))
                        (div (lambda (a b) (/ a b))))
                    (let ((rsub (funcall flip sub))
                          (rdiv (funcall flip div)))
                      (list (funcall rsub 3 10)
                            (funcall rdiv 2 10)))))";
    let expect = expect_test::expect![[r#""OK (7 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Fold/reduce patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_functional_foldl() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Left fold implementation
    let form = "(let ((foldl (lambda (f init lst)
                      (let ((acc init))
                        (dolist (x lst)
                          (setq acc (funcall f acc x)))
                        acc))))
                  (list
                    ;; Sum
                    (funcall foldl (lambda (a b) (+ a b)) 0 '(1 2 3 4 5))
                    ;; Product
                    (funcall foldl (lambda (a b) (* a b)) 1 '(1 2 3 4 5))
                    ;; Build reversed list
                    (funcall foldl (lambda (acc x) (cons x acc))
                             nil '(a b c d))
                    ;; Max
                    (funcall foldl (lambda (a b) (if (> a b) a b))
                             0 '(3 1 4 1 5 9 2 6))))";
    let expect = expect_test::expect![[r#""OK (15 120 (d c b a) 9)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_functional_foldr() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Right fold using recursion
    let form = "(progn
  (fset 'neovm--test-foldr
    (lambda (f init lst)
      (if (null lst)
          init
        (funcall f (car lst)
                 (funcall 'neovm--test-foldr f init (cdr lst))))))
  (unwind-protect
      (list
        ;; Build list (identity transform)
        (funcall 'neovm--test-foldr
                 (lambda (x acc) (cons x acc))
                 nil '(1 2 3 4 5))
        ;; Right-associative subtraction
        (funcall 'neovm--test-foldr
                 (lambda (x acc) (- x acc))
                 0 '(1 2 3)))
    (fmakunbound 'neovm--test-foldr)))";
    let expect = expect_test::expect![[r#""OK ((1 2 3 4 5) 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Filter / partition / group-by
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_functional_filter_partition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((filter-fn
                       (lambda (pred lst)
                         (let ((result nil))
                           (dolist (x lst)
                             (when (funcall pred x)
                               (setq result (cons x result))))
                           (nreverse result))))
                      (partition
                       (lambda (pred lst)
                         (let ((yes nil) (no nil))
                           (dolist (x lst)
                             (if (funcall pred x)
                                 (setq yes (cons x yes))
                               (setq no (cons x no))))
                           (list (nreverse yes) (nreverse no))))))
                  (let ((nums '(1 2 3 4 5 6 7 8 9 10)))
                    (list
                      (funcall filter-fn #'evenp nums)
                      (funcall partition #'evenp nums)
                      (funcall filter-fn
                               (lambda (x) (> x 5)) nums))))";
    let expect =
        expect_test::expect![[r#""OK ((2 4 6 8 10) ((2 4 6 8 10) (1 3 5 7 9)) (6 7 8 9 10))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_functional_group_by() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Group elements by a key function using hash table
    let form = "(let ((group-by
                       (lambda (key-fn lst)
                         (let ((table (make-hash-table :test 'equal)))
                           (dolist (x lst)
                             (let ((k (funcall key-fn x)))
                               (puthash k
                                        (cons x (gethash k table nil))
                                        table)))
                           ;; Convert to sorted alist
                           (let ((result nil))
                             (maphash (lambda (k v)
                                        (setq result
                                              (cons (cons k (nreverse v))
                                                    result)))
                                      table)
                             (sort result
                                   (lambda (a b)
                                     (< (car a) (car b)))))))))
                  (funcall group-by
                           (lambda (x) (% x 3))
                           '(1 2 3 4 5 6 7 8 9)))";
    let expect = expect_test::expect![[r#""OK ((0 3 6 9) (1 1 4 7) (2 2 5 8))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Transducer-like pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_functional_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Process data through a pipeline of transformations
    let form = r#"(let ((data '((name . "Alice") (score . 85)
                                 (name . "Bob") (score . 92)
                                 (name . "Carol") (score . 78)
                                 (name . "Dave") (score . 95)
                                 (name . "Eve") (score . 88))))
                    ;; Extract (name . score) pairs
                    (let ((pairs nil)
                          (remaining data))
                      (while remaining
                        (let ((name-entry (car remaining))
                              (score-entry (cadr remaining)))
                          (when (and (eq (car name-entry) 'name)
                                     (eq (car score-entry) 'score))
                            (setq pairs
                                  (cons (cons (cdr name-entry)
                                              (cdr score-entry))
                                        pairs))))
                        (setq remaining (cddr remaining)))
                      ;; Filter scores >= 85, sort by score desc
                      (let ((filtered
                             (let ((result nil))
                               (dolist (p (nreverse pairs))
                                 (when (>= (cdr p) 85)
                                   (setq result (cons p result))))
                               (nreverse result))))
                        (sort filtered
                              (lambda (a b) (> (cdr a) (cdr b)))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Dave\" . 95) (\"Bob\" . 92) (\"Eve\" . 88) (\"Alice\" . 85))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Memoization with hash-table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_functional_memoize() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Generic memoize wrapper
    let form = "(let ((call-count 0))
                  (let ((memoize
                         (lambda (f)
                           (let ((cache (make-hash-table :test 'equal)))
                             (lambda (&rest args)
                               (let ((cached (gethash args cache 'miss)))
                                 (if (eq cached 'miss)
                                     (let ((result (apply f args)))
                                       (puthash args result cache)
                                       result)
                                   cached)))))))
                    (let ((expensive
                           (funcall memoize
                                    (lambda (n)
                                      (setq call-count (1+ call-count))
                                      (* n n n)))))
                      ;; Call with same args multiple times
                      (let ((r1 (funcall expensive 5))
                            (r2 (funcall expensive 5))
                            (r3 (funcall expensive 3))
                            (r4 (funcall expensive 3))
                            (r5 (funcall expensive 5)))
                        (list r1 r2 r3 r4 r5 call-count)))))";
    let expect = expect_test::expect![[r#""OK (125 125 27 27 125 2)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(125 125 27 27 125 2)", &o, &n);
}

// ---------------------------------------------------------------------------
// Iterative map/filter/reduce with accumulators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_functional_mapcat() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // mapcat (flatmap): map then concatenate results
    let form = "(let ((mapcat (lambda (f lst)
                      (let ((result nil))
                        (dolist (x lst)
                          (let ((mapped (funcall f x)))
                            (dolist (item mapped)
                              (setq result (cons item result)))))
                        (nreverse result)))))
                  (list
                    ;; Expand each number to range
                    (funcall mapcat
                             (lambda (n)
                               (let ((r nil))
                                 (dotimes (i n)
                                   (setq r (cons (1+ i) r)))
                                 (nreverse r)))
                             '(1 2 3))
                    ;; Split words
                    (funcall mapcat
                             (lambda (s) (split-string s \" \"))
                             '(\"hello world\" \"foo bar baz\"))))";
    let expect = expect_test::expect![[
        r#""OK ((1 1 2 1 2 3) (\"hello\" \"world\" \"foo\" \"bar\" \"baz\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_functional_scan() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // scan (prefix sums / running accumulation)
    let form = "(let ((scan (lambda (f init lst)
                      (let ((acc init)
                            (result (list init)))
                        (dolist (x lst)
                          (setq acc (funcall f acc x))
                          (setq result (cons acc result)))
                        (nreverse result)))))
                  (list
                    ;; Running sum
                    (funcall scan #'+ 0 '(1 2 3 4 5))
                    ;; Running product
                    (funcall scan #'* 1 '(1 2 3 4 5))
                    ;; Running max
                    (funcall scan #'max 0 '(3 1 4 1 5 9 2 6))))";
    let expect =
        expect_test::expect![[r#""OK ((0 1 3 6 10 15) (1 1 2 6 24 120) (0 3 3 4 4 5 9 9 9))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: mini data-processing language
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_functional_data_processing_dsl() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a simple query DSL over lists
    let form = "(let ((where-fn
                       (lambda (pred data)
                         (let ((r nil))
                           (dolist (row data)
                             (when (funcall pred row)
                               (setq r (cons row r))))
                           (nreverse r))))
                      (select-fn
                       (lambda (keys data)
                         (mapcar
                          (lambda (row)
                            (let ((r nil))
                              (dolist (k keys)
                                (let ((pair (assq k row)))
                                  (when pair
                                    (setq r (cons pair r)))))
                              (nreverse r)))
                          data)))
                      (order-by-fn
                       (lambda (key dir data)
                         (sort (copy-sequence data)
                               (if (eq dir 'asc)
                                   (lambda (a b)
                                     (< (cdr (assq key a))
                                        (cdr (assq key b))))
                                 (lambda (a b)
                                   (> (cdr (assq key a))
                                      (cdr (assq key b)))))))))
                  (let ((dataset '(((name . alice) (age . 30) (score . 85))
                                   ((name . bob) (age . 25) (score . 92))
                                   ((name . carol) (age . 35) (score . 78))
                                   ((name . dave) (age . 28) (score . 95)))))
                    ;; Pipeline: filter age>27, sort by score desc,
                    ;; select name+score
                    (funcall select-fn '(name score)
                             (funcall order-by-fn 'score 'desc
                                      (funcall where-fn
                                               (lambda (r)
                                                 (> (cdr (assq 'age r)) 27))
                                               dataset)))))";
    let expect = expect_test::expect![[
        r#""OK (((name . dave) (score . 95)) ((name . alice) (score . 85)) ((name . carol) (score . 78)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
