//! Oracle parity tests for `let`/`let*` with lexical vs dynamic binding
//! interactions: closures capturing let-bound variables, defvar + let
//! dynamic bindings, sequential let* references, nested shadowing,
//! closure factories, configuration patterns, and condition-case scope.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Lexical closures capturing let-bound variables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_lexical_closure_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A closure captures a lexically-scoped let variable and retains
    // its value even after the let form has exited.
    let form = r#"(progn
  (let ((results nil))
    ;; Create several closures that each capture a different value of x
    (let ((closures
           (let ((acc nil))
             (dolist (n '(1 2 3 4 5))
               (let ((x (* n 10)))
                 (setq acc (cons (lambda () x) acc))))
             (nreverse acc))))
      ;; Call each closure — should get 10 20 30 40 50
      (dolist (fn closures)
        (setq results (cons (funcall fn) results)))
      (nreverse results))))"#;
    let expect = expect_test::expect![[r#""OK (10 20 30 40 50)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Lexical closure mutation of captured variable
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_lexical_closure_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Two closures share a lexical variable; one mutates it, the other reads it.
    let form = r#"(let ((state 0))
  (let ((inc (lambda (n) (setq state (+ state n)) state))
        (get (lambda () state)))
    (list (funcall get)
          (funcall inc 5)
          (funcall get)
          (funcall inc 3)
          (funcall inc 2)
          (funcall get))))"#;
    let expect = expect_test::expect![[r#""OK (0 5 5 8 10 10)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(0 5 5 8 10 10)", &o, &n);
}

// ---------------------------------------------------------------------------
// Dynamic binding with defvar + let
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_dynamic_defvar_deep_call_stack() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // defvar creates a special variable; let rebinds it dynamically,
    // visible through multiple levels of function calls.
    let form = r#"(progn
  (defvar neovm--test-ldp-config 'default)

  (fset 'neovm--test-ldp-level3
    (lambda () (symbol-name neovm--test-ldp-config)))

  (fset 'neovm--test-ldp-level2
    (lambda () (concat "L2:" (funcall 'neovm--test-ldp-level3))))

  (fset 'neovm--test-ldp-level1
    (lambda () (concat "L1:" (funcall 'neovm--test-ldp-level2))))

  (unwind-protect
      (list
       ;; Without rebinding
       (funcall 'neovm--test-ldp-level1)
       ;; With dynamic rebinding — visible through entire call chain
       (let ((neovm--test-ldp-config 'custom))
         (funcall 'neovm--test-ldp-level1))
       ;; After let exits — restored to default
       (funcall 'neovm--test-ldp-level1))
    (fmakunbound 'neovm--test-ldp-level1)
    (fmakunbound 'neovm--test-ldp-level2)
    (fmakunbound 'neovm--test-ldp-level3)
    (makunbound 'neovm--test-ldp-config)))"#;
    let expect =
        expect_test::expect![[r#""OK (\"L1:L2:default\" \"L1:L2:custom\" \"L1:L2:default\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// let* sequential bindings referencing earlier bindings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_star_sequential_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // let* allows each binding to reference all preceding bindings,
    // including complex computed chains.
    let form = r#"(let* ((a 2)
       (b (* a a))           ; b = 4
       (c (+ a b))           ; c = 6
       (d (* b c))           ; d = 24
       (e (- d a))           ; e = 22
       (f (list a b c d e))  ; f = (2 4 6 24 22)
       (g (apply #'+ f))     ; g = 58
       (h (mapcar (lambda (x) (* x x)) f))  ; h = (4 16 36 576 484)
       (i (apply #'+ h)))    ; i = 1116
  (list f g h i))"#;
    let expect = expect_test::expect![[r#""OK ((2 4 6 24 22) 58 (4 16 36 576 484) 1116)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested let with shadowing — lexical vs dynamic
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_nested_shadowing_lexical_dynamic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Mix lexical (regular) and dynamic (defvar) variables with nested
    // shadowing to verify that each follows its own scoping rules.
    let form = r#"(progn
  (defvar neovm--test-ldp-dyn 'outer-dyn)

  ;; A function that reads both a lexical and dynamic variable
  (fset 'neovm--test-ldp-reader
    (lambda (lex-val)
      (list lex-val neovm--test-ldp-dyn)))

  (unwind-protect
      (let ((lex-var 'outer-lex))
        (list
         ;; Baseline
         (funcall 'neovm--test-ldp-reader lex-var)
         ;; Shadow lexical only
         (let ((lex-var 'inner-lex))
           (funcall 'neovm--test-ldp-reader lex-var))
         ;; Shadow dynamic only
         (let ((neovm--test-ldp-dyn 'inner-dyn))
           (funcall 'neovm--test-ldp-reader lex-var))
         ;; Shadow both
         (let ((lex-var 'inner-lex2)
               (neovm--test-ldp-dyn 'inner-dyn2))
           (funcall 'neovm--test-ldp-reader lex-var))
         ;; After all lets — should be restored
         (funcall 'neovm--test-ldp-reader lex-var)))
    (fmakunbound 'neovm--test-ldp-reader)
    (makunbound 'neovm--test-ldp-dyn)))"#;
    let expect = expect_test::expect![[
        r#""OK ((outer-lex outer-dyn) (inner-lex outer-dyn) (outer-lex inner-dyn) (inner-lex2 inner-dyn2) (outer-lex outer-dyn))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closure factory with let — generate parameterized closures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_closure_factory() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A factory function returns a closure that has captured its parameter.
    // Multiple calls to the factory produce independent closures.
    let form = r#"(let ((make-scaler
         (lambda (factor)
           (lambda (x) (* x factor)))))
  (let ((double (funcall make-scaler 2))
        (triple (funcall make-scaler 3))
        (negate (funcall make-scaler -1)))
    ;; Each closure is independent
    (let ((inputs '(1 2 3 4 5)))
      (list
       (mapcar double inputs)
       (mapcar triple inputs)
       (mapcar negate inputs)
       ;; Composition: triple then double = *6
       (mapcar (lambda (x) (funcall double (funcall triple x))) inputs)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((2 4 6 8 10) (3 6 9 12 15) (-1 -2 -3 -4 -5) (6 12 18 24 30))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dynamic binding for configuration/options pattern
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_dynamic_configuration_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Common Elisp pattern: defvar for config, let-bind around operations
    // to temporarily change behavior, then restore automatically.
    let form = r#"(progn
  (defvar neovm--test-ldp-indent 2)
  (defvar neovm--test-ldp-prefix "")
  (defvar neovm--test-ldp-sep " ")

  (fset 'neovm--test-ldp-format-item
    (lambda (item depth)
      (concat neovm--test-ldp-prefix
              (make-string (* depth neovm--test-ldp-indent) ?\s)
              (if (symbolp item) (symbol-name item) item)
              neovm--test-ldp-sep)))

  (fset 'neovm--test-ldp-format-tree
    (lambda (tree depth)
      (if (consp tree)
          (let ((result ""))
            (dolist (child tree)
              (setq result
                    (concat result
                            (funcall 'neovm--test-ldp-format-tree child depth))))
            result)
        (funcall 'neovm--test-ldp-format-item
                 (if (symbolp tree) (symbol-name tree) tree)
                 depth))))

  (unwind-protect
      (let ((tree '("a" "b" "c")))
        (list
         ;; Default config
         (funcall 'neovm--test-ldp-format-tree tree 0)
         ;; Override indent + prefix
         (let ((neovm--test-ldp-indent 4)
               (neovm--test-ldp-prefix "> "))
           (funcall 'neovm--test-ldp-format-tree tree 1))
         ;; Override separator
         (let ((neovm--test-ldp-sep ", "))
           (funcall 'neovm--test-ldp-format-tree tree 0))
         ;; After let — all restored
         (funcall 'neovm--test-ldp-format-tree tree 0)))
    (fmakunbound 'neovm--test-ldp-format-item)
    (fmakunbound 'neovm--test-ldp-format-tree)
    (makunbound 'neovm--test-ldp-indent)
    (makunbound 'neovm--test-ldp-prefix)
    (makunbound 'neovm--test-ldp-sep)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"a b c \" \">     a >     b >     c \" \"a, b, c, \" \"a b c \")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// let with condition-case — variable scope in handlers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_condition_case_scope() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that let-bound variables (both lexical and dynamic) are
    // accessible in condition-case handlers, and that dynamic bindings
    // are properly restored after errors.
    let form = r#"(progn
  (defvar neovm--test-ldp-flag 'initial)

  (unwind-protect
      (let ((results nil))
        ;; Test 1: Lexical variable visible in handler
        (setq results
              (cons
               (let ((x 42))
                 (condition-case err
                     (progn (error "boom")
                            'unreachable)
                   (error (list 'caught x (cadr err)))))
               results))

        ;; Test 2: Dynamic variable restored after error in let body
        (condition-case nil
            (let ((neovm--test-ldp-flag 'inside-let))
              (error "force unwind"))
          (error nil))
        (setq results (cons neovm--test-ldp-flag results))

        ;; Test 3: Nested let + condition-case
        (setq results
              (cons
               (let ((a 1))
                 (let ((b 2))
                   (condition-case nil
                       (let ((c 3))
                         (condition-case nil
                             (let ((d 4))
                               (error "deep"))
                           (error (list a b c 'inner-handler))))
                     (error (list a b 'outer-handler)))))
               results))

        ;; Test 4: let* in handler body
        (setq results
              (cons
               (condition-case err
                   (/ 1 0)
                 (arith-error
                  (let* ((msg (error-message-string err))
                         (len (length msg))
                         (has-div (> len 0)))
                    (list 'arith has-div))))
               results))

        (nreverse results))
    (makunbound 'neovm--test-ldp-flag)))"#;
    let expect = expect_test::expect![[
        r#""OK ((caught 42 \"boom\") initial (1 2 3 inner-handler) (arith t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// let-over-lambda with let* and deep nesting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_star_closure_accumulator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // let* creates chained bindings; closures capture intermediate results
    let form = r#"(let* ((history nil)
       (log (lambda (msg)
              (setq history (cons msg history))))
       (make-acc (lambda (init)
                   (let ((val init))
                     (lambda (op &optional n)
                       (cond
                        ((eq op 'add) (setq val (+ val (or n 0)))
                         (funcall log (format "add %d -> %d" (or n 0) val))
                         val)
                        ((eq op 'get) val)
                        ((eq op 'reset) (setq val init)
                         (funcall log "reset")
                         val)
                        (t (error "unknown op"))))))))
  (let ((acc (funcall make-acc 100)))
    (list
     (funcall acc 'get)
     (funcall acc 'add 5)
     (funcall acc 'add 15)
     (funcall acc 'get)
     (funcall acc 'reset)
     (funcall acc 'get)
     (length (nreverse history)))))"#;
    let expect = expect_test::expect![[r#""OK (100 105 120 120 100 100 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dynamic binding with unwind-protect inside let
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_dynamic_unwind_protect_nesting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that dynamic bindings are restored through nested
    // unwind-protect forms even with multiple error paths.
    let form = r#"(progn
  (defvar neovm--test-ldp-trace nil)

  (fset 'neovm--test-ldp-log
    (lambda (msg)
      (setq neovm--test-ldp-trace
            (cons msg neovm--test-ldp-trace))))

  (unwind-protect
      (progn
        (let ((neovm--test-ldp-trace nil))
          (unwind-protect
              (let ((neovm--test-ldp-trace nil))
                (funcall 'neovm--test-ldp-log "inner")
                (unwind-protect
                    (let ((neovm--test-ldp-trace nil))
                      (funcall 'neovm--test-ldp-log "innermost")
                      ;; Capture innermost trace
                      (setq neovm--test-ldp-innermost-snap
                            (copy-sequence neovm--test-ldp-trace)))
                  ;; Cleanup: back to "inner" level
                  (funcall 'neovm--test-ldp-log "cleanup-innermost"))
                ;; Capture inner trace
                (setq neovm--test-ldp-inner-snap
                      (copy-sequence neovm--test-ldp-trace)))
            ;; Cleanup: back to outer level
            (funcall 'neovm--test-ldp-log "cleanup-inner"))
          ;; Capture outer trace
          (list neovm--test-ldp-innermost-snap
                neovm--test-ldp-inner-snap
                (copy-sequence neovm--test-ldp-trace))))
    (fmakunbound 'neovm--test-ldp-log)
    (makunbound 'neovm--test-ldp-trace)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"innermost\") (\"cleanup-innermost\" \"inner\") (\"cleanup-inner\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
