//! Oracle parity tests for lexical vs dynamic binding comprehensive coverage:
//! closure capture differences, let/let* scoping, `defvar` making dynamic,
//! `special-variable-p`, funcall with captured closures, dynamic wind via
//! `let` vs `unwind-protect`, `symbol-value` on lexical vs dynamic vars,
//! `set` on lexical vs dynamic, nested `let` shadowing, lambda in let
//! capturing lexical scope, `mapc` with closure accumulator.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Closure capture differences: lexical vs dynamic
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexdyn_closure_capture_lexical() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Under lexical binding, a closure captures the binding at definition
    // time. A later rebinding of the same name in a different scope does
    // not affect the closure.
    let form = r#"(let ((x 10)
      (y 20))
  (let ((get-x (lambda () x))
        (get-y (lambda () y))
        (get-sum (lambda () (+ x y))))
    ;; Rebind x and y in a new scope
    (let ((x 999)
          (y 888))
      ;; Closures still see the original lexical bindings
      (list (funcall get-x)
            (funcall get-y)
            (funcall get-sum)
            ;; But direct references see the new scope
            x y))))"#;
    let expect = expect_test::expect![[r#""OK (10 20 30 999 888)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(10 20 30 999 888)", &o, &n);
}

#[test]
fn oracle_prop_lexdyn_closure_capture_dynamic_via_defvar() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A defvar'd variable is dynamically scoped. A lambda that references it
    // sees the CURRENT dynamic binding at call time, not definition time.
    let form = r#"(progn
  (defvar neovm--lvd-dynx 100)
  (unwind-protect
      (let ((reader (lambda () neovm--lvd-dynx)))
        (let ((r1 (funcall reader)))
          ;; Rebind dynamically
          (let ((neovm--lvd-dynx 200))
            (let ((r2 (funcall reader)))
              (let ((neovm--lvd-dynx 300))
                (let ((r3 (funcall reader)))
                  ;; After let exits, dynamic binding restored
                  (list r1 r2 r3 (funcall reader))))))))
    (makunbound 'neovm--lvd-dynx)))"#;
    let expect = expect_test::expect![[r#""OK (100 200 300 300)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// let/let* scoping: parallel vs sequential
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexdyn_let_parallel_swap() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Classic parallel let swap: (let ((a b) (b a)) ...) swaps values.
    let form = r#"(let ((a 1) (b 2) (c 3))
  ;; Parallel let: each binding sees the OUTER scope
  (let ((a b) (b c) (c a))
    ;; a=2, b=3, c=1 (cyclic rotation)
    (list a b c)))"#;
    let expect = expect_test::expect![[r#""OK (2 3 1)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(2 3 1)", &o, &n);
}

#[test]
fn oracle_prop_lexdyn_let_star_sequential_dependency() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // let* bindings see previous bindings in the same form.
    let form = r#"(let* ((a 5)
        (b (* a 2))
        (c (+ a b))
        (d (- c a))
        (e (list a b c d)))
  ;; a=5, b=10, c=15, d=10, e=(5 10 15 10)
  (list a b c d e))"#;
    let expect = expect_test::expect![[r#""OK (5 10 15 10 (5 10 15 10))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_lexdyn_let_star_closure_captures_intermediate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Closures defined in let* capture intermediate values.
    let form = r#"(let* ((x 10)
        (f1 (lambda () x))
        (x (* x 2))
        (f2 (lambda () x))
        (x (* x 3))
        (f3 (lambda () x)))
  ;; f1 captures x=10, f2 captures x=20, f3 captures x=60
  ;; Wait -- in let*, each new x shadows the previous one.
  ;; f1 captures the FIRST x=10 binding
  ;; After (x (* x 2)), x is a NEW binding = 20, f2 captures this
  ;; After (x (* x 3)), x is a NEW binding = 60, f3 captures this
  (list (funcall f1) (funcall f2) (funcall f3) x))"#;
    let expect = expect_test::expect![[r#""OK (10 20 60 60)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// defvar making a variable dynamic
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexdyn_defvar_makes_dynamic_in_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Once a variable is declared special via defvar, all let-bindings of
    // that variable become dynamic, even in lexical-binding mode.
    let form = r#"(progn
  (defvar neovm--lvd-special 'default)
  (fset 'neovm--lvd-read-special (lambda () neovm--lvd-special))
  (unwind-protect
      (progn
        ;; let-bind the special variable
        (let ((neovm--lvd-special 'bound1))
          (let ((r1 (funcall 'neovm--lvd-read-special)))
            ;; Nest deeper
            (let ((neovm--lvd-special 'bound2))
              (let ((r2 (funcall 'neovm--lvd-read-special)))
                ;; setq modifies the current dynamic binding
                (setq neovm--lvd-special 'modified)
                (let ((r3 (funcall 'neovm--lvd-read-special)))
                  ;; After inner let, reverts to bound1
                  (list r1 r2 r3))))))
        ;; After all lets, reverts to default
        (funcall 'neovm--lvd-read-special))
    (fmakunbound 'neovm--lvd-read-special)
    (makunbound 'neovm--lvd-special)))"#;
    let expect = expect_test::expect![[r#""OK default""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// special-variable-p
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexdyn_special_variable_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // special-variable-p returns t for defvar'd vars, nil for regular vars.
    let form = r#"(progn
  (defvar neovm--lvd-svp-test 'val)
  (unwind-protect
      (list
        (special-variable-p 'neovm--lvd-svp-test)           ;; t
        (special-variable-p 'this-is-not-special-surely-xyz) ;; nil
        ;; Built-in specials
        (special-variable-p 'most-positive-fixnum)           ;; result depends
        ;; After defvar, always special
        (progn
          (defvar neovm--lvd-svp-test2 nil)
          (special-variable-p 'neovm--lvd-svp-test2)))
    (makunbound 'neovm--lvd-svp-test)
    (makunbound 'neovm--lvd-svp-test2)))"#;
    let expect = expect_test::expect![[r#""OK (t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// funcall with captured closures in different contexts
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexdyn_funcall_closure_in_different_scope() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Pass closures through function calls and verify they retain their
    // captured environment regardless of the calling context.
    let form = r#"(let ((make-adder (lambda (n) (lambda (x) (+ x n)))))
  (let ((add5 (funcall make-adder 5))
        (add10 (funcall make-adder 10)))
    ;; Apply closures from a different scope
    (let ((n 999))
      ;; n=999 does NOT affect the closures
      (let ((results (mapcar (lambda (fn) (funcall fn 100))
                             (list add5 add10))))
        ;; Also apply them in a deeply nested context
        (let ((n -1))
          (list results
                (funcall add5 0)
                (funcall add10 0)
                (funcall add5 (funcall add10 1))))))))"#;
    let expect = expect_test::expect![[r#""OK ((105 110) 5 10 16)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_lexdyn_closure_passed_to_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A closure used as a comparison function for sort, capturing a
    // lexical variable that determines sort direction.
    let form = r#"(let ((data '(3 1 4 1 5 9 2 6 5 3 5)))
  (let ((ascending
         (let ((direction 'asc))
           (lambda (a b) (if (eq direction 'asc) (< a b) (> a b)))))
        (descending
         (let ((direction 'desc))
           (lambda (a b) (if (eq direction 'desc) (> a b) (< a b))))))
    (list (sort (copy-sequence data) ascending)
          (sort (copy-sequence data) descending))))"#;
    let expect =
        expect_test::expect![[r#""OK ((1 1 2 3 3 4 5 5 5 6 9) (9 6 5 5 5 4 3 3 2 1 1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dynamic wind via let vs unwind-protect
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexdyn_dynamic_wind_let_unwind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compare dynamic variable restoration via let (automatic) vs
    // unwind-protect (manual).
    let form = r#"(progn
  (defvar neovm--lvd-wind-var 'initial)
  (unwind-protect
      (let ((log nil))
        ;; Automatic via let
        (let ((neovm--lvd-wind-var 'let-bound))
          (push (list 'inside-let neovm--lvd-wind-var) log))
        (push (list 'after-let neovm--lvd-wind-var) log)
        ;; Manual via unwind-protect + set
        (let ((saved neovm--lvd-wind-var))
          (unwind-protect
              (progn
                (set 'neovm--lvd-wind-var 'manually-set)
                (push (list 'inside-unwind neovm--lvd-wind-var) log)
                ;; Simulate error
                (condition-case _
                    (progn
                      (set 'neovm--lvd-wind-var 'about-to-error)
                      (error "test error"))
                  (error
                   (push (list 'in-handler neovm--lvd-wind-var) log))))
            (set 'neovm--lvd-wind-var saved)))
        (push (list 'after-unwind neovm--lvd-wind-var) log)
        (nreverse log))
    (makunbound 'neovm--lvd-wind-var)))"#;
    let expect = expect_test::expect![[
        r#""OK ((inside-let let-bound) (after-let initial) (inside-unwind manually-set) (in-handler about-to-error) (after-unwind initial))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// symbol-value on lexical vs dynamic vars
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexdyn_symbol_value_lexical_vs_dynamic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // symbol-value reads the dynamic binding of a symbol.
    // For a purely lexical variable, symbol-value does NOT see it.
    let form = r#"(progn
  (defvar neovm--lvd-sv-dyn 'dyn-val)
  (unwind-protect
      (let ((lex-var 'lex-val))
        (list
          ;; symbol-value sees dynamic variable
          (symbol-value 'neovm--lvd-sv-dyn)
          ;; symbol-value does NOT see lexical variable
          ;; (it would error or see global if any)
          (condition-case err
              (symbol-value 'lex-var)
            (void-variable 'void))
          ;; let-bind the dynamic var and check
          (let ((neovm--lvd-sv-dyn 'rebound))
            (symbol-value 'neovm--lvd-sv-dyn))
          ;; After let, restored
          (symbol-value 'neovm--lvd-sv-dyn)))
    (makunbound 'neovm--lvd-sv-dyn)))"#;
    let expect = expect_test::expect![[r#""OK (dyn-val void rebound dyn-val)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// set on lexical vs dynamic vars
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexdyn_set_on_dynamic_vs_lexical() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // `set` always operates on the symbol's dynamic binding.
    // It does NOT affect lexical bindings.
    let form = r#"(progn
  (defvar neovm--lvd-set-dyn 'original)
  (unwind-protect
      (let ((lex-x 10))
        ;; set on dynamic: works
        (set 'neovm--lvd-set-dyn 'modified)
        (let ((r1 neovm--lvd-set-dyn))
          ;; set on dynamic within let binding
          (let ((neovm--lvd-set-dyn 'let-bound))
            (set 'neovm--lvd-set-dyn 'set-in-let)
            (let ((r2 neovm--lvd-set-dyn))
              ;; After let, restored to 'modified (the set before let)
              (list r1 r2 neovm--lvd-set-dyn
                    ;; Lexical var unchanged by any set call
                    lex-x)))))
    (makunbound 'neovm--lvd-set-dyn)))"#;
    let expect = expect_test::expect![[r#""OK (modified set-in-let set-in-let 10)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested let shadowing with closures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexdyn_nested_let_shadowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple nested let bindings of the same variable name.
    // Each closure captures its specific level's binding.
    let form = r#"(let ((x 'level0))
  (let ((f0 (lambda () x)))
    (let ((x 'level1))
      (let ((f1 (lambda () x)))
        (let ((x 'level2))
          (let ((f2 (lambda () x)))
            (let ((x 'level3))
              (let ((f3 (lambda () x)))
                ;; Each closure sees its own level
                (list (funcall f0)
                      (funcall f1)
                      (funcall f2)
                      (funcall f3)
                      ;; Direct reference sees innermost
                      x)))))))))"#;
    let expect = expect_test::expect![[r#""OK (level0 level1 level2 level3 level3)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(level0 level1 level2 level3 level3)", &o, &n);
}

#[test]
fn oracle_prop_lexdyn_shadowing_with_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Mutation (setq) of a shadowed lexical variable only affects the
    // specific binding, not any outer one.
    let form = r#"(let ((x 'outer))
  (let ((save-outer (lambda () x)))
    (let ((x 'inner))
      ;; Mutate the inner x
      (setq x 'inner-mutated)
      (list (funcall save-outer)  ;; still 'outer
            x))))"#;
    let expect = expect_test::expect![[r#""OK (outer inner-mutated)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(outer inner-mutated)", &o, &n);
}

// ---------------------------------------------------------------------------
// Lambda in let capturing lexical scope
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexdyn_lambda_factory_with_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A factory function that returns multiple closures sharing mutable state.
    // Tests that setq inside one closure is visible to the other.
    let form = r#"(let ((make-pair
           (lambda (init)
             (let ((state init))
               (cons (lambda (new-val) (setq state new-val) state)
                     (lambda () state))))))
  (let* ((pair1 (funcall make-pair 'a))
         (pair2 (funcall make-pair 'x))
         (set1 (car pair1)) (get1 (cdr pair1))
         (set2 (car pair2)) (get2 (cdr pair2)))
    (list
      (funcall get1)           ;; a
      (funcall get2)           ;; x
      (funcall set1 'b)        ;; b
      (funcall get1)           ;; b (mutated)
      (funcall get2)           ;; x (independent)
      (funcall set2 'y)        ;; y
      (funcall set1 'c)        ;; c
      (funcall get1)           ;; c
      (funcall get2))))"#;
    let expect = expect_test::expect![[r#""OK (a x b b x y c c y)""#]];
    // y
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapc with closure accumulator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexdyn_mapc_closure_accumulator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use mapc with a closure that accumulates results into a lexical variable.
    let form = r#"(let ((sum 0)
      (product 1)
      (items nil))
  (mapc (lambda (n)
          (setq sum (+ sum n))
          (setq product (* product n))
          (setq items (cons (* n n) items)))
        '(1 2 3 4 5))
  (list sum product (nreverse items)))"#;
    let expect = expect_test::expect![[r#""OK (15 120 (1 4 9 16 25))""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(15 120 (1 4 9 16 25))", &o, &n);
}

#[test]
fn oracle_prop_lexdyn_mapc_multiple_closures_shared_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple closures sharing state, used with mapc and mapcar.
    let form = r#"(let ((even-count 0)
      (odd-count 0)
      (even-sum 0)
      (odd-sum 0))
  (mapc (lambda (n)
          (if (= (% n 2) 0)
              (progn (setq even-count (1+ even-count))
                     (setq even-sum (+ even-sum n)))
            (setq odd-count (1+ odd-count))
            (setq odd-sum (+ odd-sum n))))
        '(1 2 3 4 5 6 7 8 9 10))
  (list even-count odd-count even-sum odd-sum
        (+ even-sum odd-sum)))"#;
    let expect = expect_test::expect![[r#""OK (5 5 30 25 55)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(5 5 30 25 55)", &o, &n);
}

// ---------------------------------------------------------------------------
// Mixed: dynamic + lexical in same form, complex interactions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexdyn_mixed_complex_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A function that uses both lexical closure and dynamic variable.
    // The closure captures a lexical var; the function also reads a dynamic var.
    let form = r#"(progn
  (defvar neovm--lvd-mix-flag t)
  (unwind-protect
      (let ((base 100))
        (let ((compute
               (lambda (x)
                 ;; base is lexical (captured), neovm--lvd-mix-flag is dynamic
                 (if neovm--lvd-mix-flag
                     (+ base x)
                   (- base x)))))
          (let ((r1 (funcall compute 5)))          ;; flag=t => 100+5=105
            (let ((neovm--lvd-mix-flag nil))
              (let ((r2 (funcall compute 5)))      ;; flag=nil => 100-5=95
                ;; Mutate the lexical base
                (setq base 200)
                (let ((r3 (funcall compute 10)))   ;; flag=nil => 200-10=190
                  (let ((neovm--lvd-mix-flag t))
                    (let ((r4 (funcall compute 10))) ;; flag=t => 200+10=210
                      (list r1 r2 r3 r4)))))))))
    (makunbound 'neovm--lvd-mix-flag)))"#;
    let expect = expect_test::expect![[r#""OK (105 95 190 210)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_lexdyn_dynamic_controls_lexical_closure_behavior() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A set of closures whose behavior is controlled by a dynamic variable.
    // The closures themselves are lexical, but they read a dynamic "config".
    let form = r#"(progn
  (defvar neovm--lvd-mode 'normal)
  (unwind-protect
      (let ((transform
             (lambda (s)
               (cond
                ((eq neovm--lvd-mode 'upper) (upcase s))
                ((eq neovm--lvd-mode 'lower) (downcase s))
                (t s)))))
        (let ((r1 (funcall transform "Hello")))  ;; normal -> "Hello"
          (let ((neovm--lvd-mode 'upper))
            (let ((r2 (funcall transform "Hello")))  ;; upper -> "HELLO"
              (let ((neovm--lvd-mode 'lower))
                (let ((r3 (funcall transform "Hello")))  ;; lower -> "hello"
                  (list r1 r2 r3
                        ;; After lets, back to normal
                        (funcall transform "World"))))))))
    (makunbound 'neovm--lvd-mode)))"#;
    let expect = expect_test::expect![[r#""OK (\"Hello\" \"HELLO\" \"hello\" \"world\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closures created in dolist with lexical binding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexdyn_dolist_closures_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Each iteration of dolist creates a closure. Under lexical binding,
    // each should capture the loop variable's value at that iteration.
    // Uses `push` and `dolist` which are macros from subr.el, so we need
    // the bootstrap evaluator that has those macros loaded.
    let form = r#"(let ((fns nil))
  (dolist (item '(a b c d e))
    (let ((captured item))
      (push (lambda () captured) fns)))
  (mapcar 'funcall (nreverse fns)))"#;
    let expect = expect_test::expect![[r#""OK (a b c d e)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
