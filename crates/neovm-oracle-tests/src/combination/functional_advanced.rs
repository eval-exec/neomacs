//! Advanced functional programming oracle tests: Church numerals,
//! Y-combinator, fold-based derivations, trampolined recursion,
//! continuation-passing style, and monadic patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Higher-order composition: compose, pipe, partial
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_funcadv_compose_pipe_partial() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((compose (lambda (f g) (lambda (x) (funcall f (funcall g x)))))
                        (pipe    (lambda (f g) (lambda (x) (funcall g (funcall f x)))))
                        (partial (lambda (f &rest bound)
                                   (lambda (&rest rest)
                                     (apply f (append bound rest))))))
                    (let* ((inc    (lambda (x) (+ x 1)))
                           (dbl    (lambda (x) (* x 2)))
                           (square (lambda (x) (* x x)))
                           ;; compose: right-to-left  =>  square(dbl(inc(x)))
                           (f1 (funcall compose square (funcall compose dbl inc)))
                           ;; pipe: left-to-right    =>  square(dbl(inc(x)))
                           (f2 (funcall pipe inc (funcall pipe dbl square)))
                           ;; partial application of a 3-arg function
                           (add3 (lambda (a b c) (+ a b c)))
                           (add10and (funcall partial add3 4 6)))
                      (list (funcall f1 3)   ;; (3+1)*2 = 8, 8^2 = 64
                            (funcall f2 3)   ;; same
                            (funcall add10and 5)   ;; 4+6+5 = 15
                            (funcall add10and 100))))"#;
    let expect = expect_test::expect![[r#""OK (64 64 15 110)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Church numerals
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_funcadv_church_numerals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Church encoding: a number n is (lambda (f) (lambda (x) (f (f ... (f x)))))
    let form = r#"(progn
  (fset 'neovm--church-zero  (lambda (f) (lambda (x) x)))
  (fset 'neovm--church-succ  (lambda (n)
    (lambda (f) (lambda (x) (funcall f (funcall (funcall n f) x))))))
  (fset 'neovm--church-add   (lambda (m n)
    (lambda (f) (lambda (x) (funcall (funcall m f) (funcall (funcall n f) x))))))
  (fset 'neovm--church-mul   (lambda (m n)
    (lambda (f) (funcall m (funcall n f)))))
  (fset 'neovm--church-to-int (lambda (n)
    (funcall (funcall n (lambda (x) (+ x 1))) 0)))
  (unwind-protect
      (let* ((zero  (funcall 'neovm--church-zero))
             (one   (funcall 'neovm--church-succ zero))
             (two   (funcall 'neovm--church-succ one))
             (three (funcall 'neovm--church-succ two))
             (five  (funcall 'neovm--church-add two three))
             (six   (funcall 'neovm--church-mul two three)))
        (list (funcall 'neovm--church-to-int zero)
              (funcall 'neovm--church-to-int one)
              (funcall 'neovm--church-to-int two)
              (funcall 'neovm--church-to-int three)
              (funcall 'neovm--church-to-int five)
              (funcall 'neovm--church-to-int six)))
    (fmakunbound 'neovm--church-zero)
    (fmakunbound 'neovm--church-succ)
    (fmakunbound 'neovm--church-add)
    (fmakunbound 'neovm--church-mul)
    (fmakunbound 'neovm--church-to-int)))"#;
    let expect = expect_test::expect![[
        r#""ERR (wrong-number-of-arguments (closure (t) (f) (lambda (x) x)) 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Y-combinator for anonymous recursion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_funcadv_y_combinator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Z-combinator (applicative-order Y) for strict languages
    let form = r#"(let ((Z (lambda (f)
                   (funcall
                    (lambda (x) (funcall f (lambda (&rest args) (apply (funcall x x) args))))
                    (lambda (x) (funcall f (lambda (&rest args) (apply (funcall x x) args))))))))
                    ;; Factorial via Z
                    (let ((fact (funcall Z
                                  (lambda (self)
                                    (lambda (n)
                                      (if (<= n 1) 1 (* n (funcall self (- n 1)))))))))
                      ;; Fibonacci via Z
                      (let ((fib (funcall Z
                                   (lambda (self)
                                     (lambda (n)
                                       (cond ((<= n 0) 0)
                                             ((= n 1) 1)
                                             (t (+ (funcall self (- n 1))
                                                   (funcall self (- n 2))))))))))
                        (list (funcall fact 0)
                              (funcall fact 1)
                              (funcall fact 6)
                              (funcall fact 10)
                              (funcall fib 0)
                              (funcall fib 1)
                              (funcall fib 10)))))"#;
    let expect = expect_test::expect![[r#""OK (1 1 720 3628800 0 1 55)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Fold (reduce) implementing map, filter, reverse, flatten
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_funcadv_fold_derived_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((foldl (lambda (f init lst)
                        (let ((acc init))
                          (dolist (x lst) (setq acc (funcall f acc x)))
                          acc))))
                    ;; Derive map from foldl
                    (let ((my-map (lambda (f lst)
                            (nreverse
                              (funcall foldl
                                       (lambda (acc x) (cons (funcall f x) acc))
                                       nil lst))))
                          ;; Derive filter from foldl
                          (my-filter (lambda (pred lst)
                            (nreverse
                              (funcall foldl
                                       (lambda (acc x)
                                         (if (funcall pred x) (cons x acc) acc))
                                       nil lst))))
                          ;; Derive reverse from foldl
                          (my-reverse (lambda (lst)
                            (funcall foldl (lambda (acc x) (cons x acc)) nil lst)))
                          ;; Derive flatten from foldl
                          (my-flatten nil))
                      ;; flatten needs recursion, define via fset
                      (setq my-flatten
                            (lambda (lst)
                              (funcall foldl
                                       (lambda (acc x)
                                         (if (listp x)
                                             (append acc (funcall my-flatten x))
                                           (append acc (list x))))
                                       nil lst)))
                      (list
                        (funcall my-map (lambda (x) (* x x)) '(1 2 3 4 5))
                        (funcall my-filter #'evenp '(1 2 3 4 5 6 7 8))
                        (funcall my-reverse '(a b c d e))
                        (funcall my-flatten '(1 (2 3) (4 (5 6)) 7)))))"#;
    let expect =
        expect_test::expect![[r#""OK ((1 4 9 16 25) (2 4 6 8) (e d c b a) (1 2 3 4 5 6 7))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Trampolined recursion for stack safety
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_funcadv_trampoline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A trampoline repeatedly calls thunks until a non-function result.
    // We use this pattern to compute sum(1..N) without deep recursion.
    let form = r#"(progn
  (fset 'neovm--trampoline
    (lambda (thunk)
      (let ((result (funcall thunk)))
        (while (functionp result)
          (setq result (funcall result)))
        result)))
  (fset 'neovm--tsum-helper
    (lambda (n acc)
      (if (<= n 0)
          acc
        (lambda () (funcall 'neovm--tsum-helper (1- n) (+ acc n))))))
  (fset 'neovm--tsum
    (lambda (n)
      (funcall 'neovm--trampoline
               (lambda () (funcall 'neovm--tsum-helper n 0)))))
  (unwind-protect
      (list (funcall 'neovm--tsum 0)
            (funcall 'neovm--tsum 1)
            (funcall 'neovm--tsum 10)
            (funcall 'neovm--tsum 100)
            (funcall 'neovm--tsum 500))
    (fmakunbound 'neovm--trampoline)
    (fmakunbound 'neovm--tsum-helper)
    (fmakunbound 'neovm--tsum)))"#;
    let expect = expect_test::expect![[r#""OK (0 1 55 5050 125250)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Continuation-passing style (CPS) tree algorithm
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_funcadv_cps_tree_sum() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // CPS transform of a tree-sum: each recursive call receives a continuation
    let form = r#"(progn
  (fset 'neovm--tree-sum-cps
    (lambda (tree k)
      (cond
       ((null tree) (funcall k 0))
       ((atom tree) (funcall k tree))
       (t (funcall 'neovm--tree-sum-cps (car tree)
                   (lambda (left-sum)
                     (funcall 'neovm--tree-sum-cps (cdr tree)
                              (lambda (right-sum)
                                (funcall k (+ left-sum right-sum))))))))))
  (fset 'neovm--tree-sum
    (lambda (tree)
      (funcall 'neovm--tree-sum-cps tree #'identity)))
  (unwind-protect
      (list
        (funcall 'neovm--tree-sum nil)
        (funcall 'neovm--tree-sum 42)
        (funcall 'neovm--tree-sum '(1 2 3))
        (funcall 'neovm--tree-sum '(1 (2 (3 4) 5) (6 7)))
        (funcall 'neovm--tree-sum '((10 20) (30 (40 50)))))
    (fmakunbound 'neovm--tree-sum-cps)
    (fmakunbound 'neovm--tree-sum)))"#;
    let expect = expect_test::expect![[r#""OK (0 42 6 28 150)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Monadic bind/return for optional (Maybe) values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_funcadv_maybe_monad() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Maybe monad: nil = Nothing, anything else = Just value
    // bind: if value is nil, short-circuit; else apply f
    let form = r#"(let ((maybe-return #'identity)
                        (maybe-bind
                          (lambda (val f)
                            (if (null val) nil (funcall f val)))))
                    ;; safe-div: returns nil on division by zero
                    (let ((safe-div (lambda (a b)
                            (if (= b 0) nil (/ a b)))))
                      ;; Chain: 100 / 5 / 2 / 1
                      (let ((chain1
                              (funcall maybe-bind (funcall safe-div 100 5)
                                (lambda (r1)
                                  (funcall maybe-bind (funcall safe-div r1 2)
                                    (lambda (r2)
                                      (funcall safe-div r2 1))))))
                            ;; Chain that hits zero: 100 / 5 / 0 / ...
                            (chain2
                              (funcall maybe-bind (funcall safe-div 100 5)
                                (lambda (r1)
                                  (funcall maybe-bind (funcall safe-div r1 0)
                                    (lambda (r2)
                                      (funcall safe-div r2 1))))))
                            ;; Lookup chain: alist -> nested alist
                            (data '((user . ((name . "Alice") (age . 30)
                                             (address . ((city . "Boston")
                                                         (zip . "02101")))))))
                            (safe-assq (lambda (key alist)
                                          (let ((pair (assq key alist)))
                                            (if pair (cdr pair) nil)))))
                        (let ((city
                                (funcall maybe-bind (funcall safe-assq 'user data)
                                  (lambda (u)
                                    (funcall maybe-bind (funcall safe-assq 'address u)
                                      (lambda (addr)
                                        (funcall safe-assq 'city addr))))))
                              (missing
                                (funcall maybe-bind (funcall safe-assq 'user data)
                                  (lambda (u)
                                    (funcall maybe-bind (funcall safe-assq 'phone u)
                                      (lambda (ph)
                                        (funcall safe-assq 'area-code ph)))))))
                          (list chain1 chain2 city missing)))))"#;
    let expect = expect_test::expect![[r#""OK (10 nil \"Boston\" nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Stateful iterator protocol via closures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_funcadv_closure_iterator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build an iterator that yields elements of a list, supports
    // map-iterator and filter-iterator combinators
    let form = r#"(let ((make-iter
                          (lambda (lst)
                            (let ((remaining lst))
                              (lambda ()
                                (if (null remaining)
                                    'done
                                  (let ((val (car remaining)))
                                    (setq remaining (cdr remaining))
                                    val))))))
                        (iter-collect
                          (lambda (iter)
                            (let ((result nil) (val nil))
                              (while (not (eq (setq val (funcall iter)) 'done))
                                (setq result (cons val result)))
                              (nreverse result))))
                        (iter-map
                          (lambda (f iter)
                            (lambda ()
                              (let ((val (funcall iter)))
                                (if (eq val 'done) 'done (funcall f val))))))
                        (iter-filter
                          (lambda (pred iter)
                            (lambda ()
                              (let ((val 'skip))
                                (while (and (not (eq val 'done))
                                            (eq val 'skip))
                                  (setq val (funcall iter))
                                  (unless (or (eq val 'done) (funcall pred val))
                                    (setq val 'skip)))
                                val)))))
                    ;; Pipeline: filter evens, then square them
                    (let* ((base (funcall make-iter '(1 2 3 4 5 6 7 8 9 10)))
                           (evens (funcall iter-filter #'evenp base))
                           (squared (funcall iter-map (lambda (x) (* x x)) evens)))
                      (funcall iter-collect squared)))"#;
    let expect = expect_test::expect![[r#""OK (4 16 36 64 100)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
