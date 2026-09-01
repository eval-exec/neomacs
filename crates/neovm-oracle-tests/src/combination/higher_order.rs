//! Complex oracle parity tests for higher-order function combinations:
//! function composition chains, currying and partial application,
//! fold/reduce with complex accumulators, trampolining for mutual
//! recursion, Church encoding, and fixed-point combinators.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Function composition chains (compose, pipe, partial)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compose_pipe_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build compose (right-to-left) and pipe (left-to-right) combinators
    // and chain multiple transformations
    let form = "(let ((compose2
                       (lambda (f g)
                         (lambda (&rest args) (funcall f (apply g args)))))
                      (pipe
                       (lambda (&rest fns)
                         (let ((result (car fns))
                               (rest (cdr fns)))
                           (dolist (f rest)
                             (let ((prev result)
                                   (cur f))
                               (setq result
                                     (lambda (&rest args)
                                       (funcall cur (apply prev args))))))
                           result))))
                  (let ((add1 (lambda (x) (+ x 1)))
                        (double (lambda (x) (* x 2)))
                        (negate (lambda (x) (- x)))
                        (square (lambda (x) (* x x))))
                    (list
                     ;; compose: negate(double(add1(3))) = -(2*(3+1)) = -8
                     (funcall (funcall compose2 negate
                                       (funcall compose2 double add1)) 3)
                     ;; pipe: 3 -> add1 -> double -> square = ((3+1)*2)^2 = 64
                     (funcall (funcall pipe add1 double square) 3)
                     ;; pipe with negate: 5 -> double -> negate -> add1 = -(5*2)+1 = -9
                     (funcall (funcall pipe double negate add1) 5))))";
    let expect = expect_test::expect![[r#""OK (-8 64 -9)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Currying and partial application patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_currying_partial_application() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Manual currying: convert multi-arg function to chain of single-arg
    // Also partial application with arbitrary arg count
    let form = "(let ((curry2
                       (lambda (f)
                         (lambda (a) (lambda (b) (funcall f a b)))))
                      (curry3
                       (lambda (f)
                         (lambda (a) (lambda (b) (lambda (c) (funcall f a b c))))))
                      (partial
                       (lambda (f &rest initial)
                         (lambda (&rest remaining)
                           (apply f (append initial remaining))))))
                  (let ((add (lambda (a b) (+ a b)))
                        (mul3 (lambda (a b c) (* a b c)))
                        (sub (lambda (a b) (- a b))))
                    (list
                     ;; curry2: add -> (lambda (a) (lambda (b) (+ a b)))
                     (funcall (funcall (funcall curry2 add) 10) 5)   ;; 15
                     ;; Reuse partially applied curried function
                     (let ((add10 (funcall (funcall curry2 add) 10)))
                       (list (funcall add10 1)    ;; 11
                             (funcall add10 20)   ;; 30
                             (funcall add10 -5))) ;; 5
                     ;; curry3: mul3
                     (funcall (funcall (funcall (funcall curry3 mul3) 2) 3) 4) ;; 24
                     ;; partial: fix first args
                     (funcall (funcall partial mul3 2 3) 7)  ;; 42
                     (funcall (funcall partial sub 100) 30)  ;; 70
                     ;; partial with no initial args (identity wrapper)
                     (funcall (funcall partial add) 3 4))))";
    let expect = expect_test::expect![[r#""OK (15 (11 30 5) 24 42 70 7)""#]];
    // 7
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Fold/reduce with complex accumulators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fold_complex_accumulators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use fold with accumulators that are structured data (alists, plists, lists)
    let form = r#"(let ((foldl (lambda (f init lst)
                        (let ((acc init))
                          (dolist (x lst)
                            (setq acc (funcall f acc x)))
                          acc))))
                    (list
                     ;; Accumulate into an alist: count occurrences
                     (funcall foldl
                              (lambda (counts item)
                                (let ((entry (assq item counts)))
                                  (if entry
                                      (progn (setcdr entry (1+ (cdr entry)))
                                             counts)
                                    (cons (cons item 1) counts))))
                              nil
                              '(a b a c b a d b a))
                     ;; Accumulate running stats: (count sum min max)
                     (funcall foldl
                              (lambda (stats n)
                                (list (1+ (nth 0 stats))
                                      (+ (nth 1 stats) n)
                                      (min (nth 2 stats) n)
                                      (max (nth 3 stats) n)))
                              (list 0 0 999999 -999999)
                              '(5 3 8 1 9 2 7))
                     ;; Build nested structure: group consecutive equal elements
                     (funcall foldl
                              (lambda (acc x)
                                (if (and acc (equal (caar acc) x))
                                    (cons (cons x (1+ (cdar acc)))
                                          (cdr acc))
                                  (cons (cons x 1) acc)))
                              nil
                              '(a a a b b c a a))))"#;
    let expect = expect_test::expect![[
        r#""OK (((d . 1) (c . 1) (b . 3) (a . 4)) (7 35 1 9) ((a . 2) (c . 1) (b . 2) (a . 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Trampolining for mutual recursion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_trampoline_mutual_recursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a trampoline to avoid stack overflow in mutual recursion.
    // is-even? and is-odd? call each other via thunks.
    let form = "(let ((trampoline
                       (lambda (thunk)
                         (let ((result (funcall thunk)))
                           (while (functionp result)
                             (setq result (funcall result)))
                           result))))
                  (let (is-even-thunk is-odd-thunk)
                    (setq is-even-thunk
                          (lambda (n)
                            (if (= n 0) t
                              (lambda () (funcall is-odd-thunk (1- n))))))
                    (setq is-odd-thunk
                          (lambda (n)
                            (if (= n 0) nil
                              (lambda () (funcall is-even-thunk (1- n))))))
                    (list
                     (funcall trampoline (lambda () (funcall is-even-thunk 0)))   ;; t
                     (funcall trampoline (lambda () (funcall is-even-thunk 1)))   ;; nil
                     (funcall trampoline (lambda () (funcall is-even-thunk 10)))  ;; t
                     (funcall trampoline (lambda () (funcall is-odd-thunk 0)))    ;; nil
                     (funcall trampoline (lambda () (funcall is-odd-thunk 7)))    ;; t
                     (funcall trampoline (lambda () (funcall is-odd-thunk 8)))))) ;; nil
";
    let expect = expect_test::expect![[r#""OK (t nil t nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Church encoding of booleans and numbers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_church_encoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Church booleans and Church numerals in Elisp lambdas
    let form = "(let (;; Church booleans
                      (ch-true  (lambda (a b) a))
                      (ch-false (lambda (a b) b))
                      (ch-and   (lambda (p q)
                                  (funcall p q p)))
                      (ch-or    (lambda (p q)
                                  (funcall p p q)))
                      (ch-not   (lambda (p)
                                  (lambda (a b) (funcall p b a))))
                      ;; Church numerals
                      (ch-zero  (lambda (f x) x))
                      (ch-one   (lambda (f x) (funcall f x)))
                      (ch-two   (lambda (f x) (funcall f (funcall f x))))
                      (ch-succ  (lambda (n)
                                  (lambda (f x) (funcall f (funcall n f x)))))
                      (ch-add   (lambda (m n)
                                  (lambda (f x) (funcall m f (funcall n f x)))))
                      (ch-mul   (lambda (m n)
                                  (lambda (f x) (funcall m (lambda (y) (funcall n f y)) x))))
                      ;; Convert Church numeral to integer
                      (ch-to-int (lambda (n)
                                   (funcall n (lambda (x) (1+ x)) 0))))
                  (let ((three (funcall ch-succ ch-two))
                        (four (funcall ch-add ch-two ch-two))
                        (six (funcall ch-mul ch-two three)))
                    (list
                     ;; Boolean tests
                     (funcall ch-true 'yes 'no)                           ;; yes
                     (funcall ch-false 'yes 'no)                          ;; no
                     (funcall (funcall ch-and ch-true ch-true) 'y 'n)     ;; y
                     (funcall (funcall ch-and ch-true ch-false) 'y 'n)    ;; n
                     (funcall (funcall ch-or ch-false ch-true) 'y 'n)     ;; y
                     (funcall (funcall ch-not ch-true) 'y 'n)             ;; n
                     ;; Numeral tests
                     (funcall ch-to-int ch-zero)   ;; 0
                     (funcall ch-to-int ch-one)    ;; 1
                     (funcall ch-to-int ch-two)    ;; 2
                     (funcall ch-to-int three)     ;; 3
                     (funcall ch-to-int four)      ;; 4
                     (funcall ch-to-int six))))";
    let expect = expect_test::expect![[r#""ERR (void-variable three)""#]];
    // 6
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Fixed-point combinator (Y combinator in strict Elisp)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_y_combinator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Z combinator (applicative-order Y combinator for strict languages).
    // Z = lambda f. (lambda x. f (lambda v. x x v)) (lambda x. f (lambda v. x x v))
    let form = "(let ((Z (lambda (f)
                    (funcall
                     (lambda (x)
                       (funcall f (lambda (v) (funcall (funcall x x) v))))
                     (lambda (x)
                       (funcall f (lambda (v) (funcall (funcall x x) v))))))))
                  (let (;; Factorial via Z combinator
                        (fact
                         (funcall Z
                                  (lambda (self)
                                    (lambda (n)
                                      (if (<= n 1) 1
                                        (* n (funcall self (1- n))))))))
                        ;; Fibonacci via Z combinator
                        (fib
                         (funcall Z
                                  (lambda (self)
                                    (lambda (n)
                                      (cond ((<= n 0) 0)
                                            ((= n 1) 1)
                                            (t (+ (funcall self (- n 1))
                                                  (funcall self (- n 2))))))))))
                    (list
                     (funcall fact 0)    ;; 1
                     (funcall fact 1)    ;; 1
                     (funcall fact 5)    ;; 120
                     (funcall fact 7)    ;; 5040
                     (funcall fib 0)     ;; 0
                     (funcall fib 1)     ;; 1
                     (funcall fib 6)     ;; 8
                     (funcall fib 10)))) ;; 55
";
    let expect = expect_test::expect![[r#""OK (1 1 120 5040 0 1 8 55)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: higher-order function toolkit
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_higher_order_toolkit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a small functional toolkit (juxt, complement, constantly, iterate)
    // and combine them
    let form = "(let ((juxt
                       ;; Apply multiple functions to same args, collect results
                       (lambda (&rest fns)
                         (lambda (&rest args)
                           (mapcar (lambda (f) (apply f args)) fns))))
                      (complement
                       ;; Negate a predicate
                       (lambda (pred)
                         (lambda (&rest args)
                           (not (apply pred args)))))
                      (constantly
                       ;; Always return the same value
                       (lambda (val)
                         (lambda (&rest _args) val)))
                      (iterate
                       ;; Apply f to x n times
                       (lambda (f x n)
                         (let ((result x))
                           (dotimes (_ n)
                             (setq result (funcall f result)))
                           result))))
                  (list
                   ;; juxt: apply multiple fns to same input
                   (funcall (funcall juxt #'1+ #'1- (lambda (x) (* x x))) 5)
                   ;; complement: negate evenp
                   (let ((oddp (funcall complement #'evenp)))
                     (list (funcall oddp 3) (funcall oddp 4)))
                   ;; constantly: always returns 42
                   (mapcar (funcall constantly 42) '(a b c d))
                   ;; iterate: apply 1+ to 0, 10 times
                   (funcall iterate #'1+ 0 10)
                   ;; iterate: double 1, 8 times = 256
                   (funcall iterate (lambda (x) (* 2 x)) 1 8)
                   ;; combine: juxt with iterate
                   (funcall (funcall juxt
                                     (lambda (x) (funcall iterate #'1+ x 3))
                                     (lambda (x) (funcall iterate (lambda (y) (* 2 y)) x 3)))
                            5)))";
    let expect = expect_test::expect![[r#""OK ((6 4 25) (t nil) (42 42 42 42) 10 256 (8 40))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: function algebra (monoid of endomorphisms)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_endomorphism_monoid() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Functions from a type to itself form a monoid under composition.
    // Build an endo wrapper with compose and identity, then fold a list
    // of transformations.
    let form = "(let ((endo-id (lambda (x) x))
                      (endo-compose
                       (lambda (f g)
                         (lambda (x) (funcall f (funcall g x)))))
                      (endo-fold
                       ;; Fold a list of endomorphisms into one
                       (lambda (fns)
                         (let ((result (lambda (x) x)))
                           (dolist (f fns)
                             (let ((prev result)
                                   (cur f))
                               (setq result
                                     (lambda (x) (funcall cur (funcall prev x))))))
                           result))))
                  (let ((transforms
                         (list (lambda (x) (+ x 1))
                               (lambda (x) (* x 2))
                               (lambda (x) (- x 3))
                               (lambda (x) (* x x)))))
                    (let ((combined (funcall endo-fold transforms)))
                      (list
                       ;; Apply combined: ((((5+1)*2)-3)^2 = (12-3)^2 = 81
                       (funcall combined 5)
                       ;; Identity composed with anything is identity
                       (funcall (funcall endo-compose endo-id (car transforms)) 10)
                       ;; Fold empty list gives identity
                       (funcall (funcall endo-fold nil) 42)
                       ;; Fold single function
                       (funcall (funcall endo-fold (list (lambda (x) (* x 3)))) 7)
                       ;; Different input through same pipeline
                       (funcall combined 0)       ;; ((((0+1)*2)-3)^2 = (-1)^2 = 1
                       (funcall combined 10)))))";
    let expect = expect_test::expect![[r#""OK (81 11 42 21 1 361)""#]];
    // ((((10+1)*2)-3)^2 = (22-3)^2 = 361
    crate::common::assert_oracle_parity_expect(form, expect);
}
