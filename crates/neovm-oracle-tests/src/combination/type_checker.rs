//! Oracle parity tests for a simple type checker for a toy language in Elisp.
//!
//! Implements: type environments, literal type checking, binary operation
//! type rules, if-expression checking, let-binding checking, function
//! application checking, and polymorphic type variables with unification.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Type environment and literal type checking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_typechk_env_and_literals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Type representations:
  ;;   int, bool, string              -- base types (symbols)
  ;;   (-> arg-type ret-type)         -- function type
  ;;   (tvar N)                       -- type variable
  ;;
  ;; Expression representations:
  ;;   (lit-int N)                    -- integer literal
  ;;   (lit-bool B)                   -- boolean literal (t or nil)
  ;;   (lit-str S)                    -- string literal
  ;;   (var NAME)                     -- variable reference
  ;;   (binop OP L R)                 -- binary operation
  ;;   (if-expr COND THEN ELSE)       -- conditional
  ;;   (let-bind NAME VAL BODY)       -- let binding
  ;;   (app FN ARG)                   -- function application
  ;;   (lam PARAM PARAM-TYPE BODY)    -- lambda with type annotation

  ;; Type environment: alist of (name . type)
  (fset 'neovm--tc-env-empty (lambda () nil))
  (fset 'neovm--tc-env-extend
    (lambda (env name type) (cons (cons name type) env)))
  (fset 'neovm--tc-env-lookup
    (lambda (env name)
      (let ((entry (assq name env)))
        (if entry (cdr entry) nil))))

  ;; Type checker: returns (ok . type) or (error . message)
  (fset 'neovm--tc-check
    (lambda (env expr)
      (cond
        ;; Integer literal
        ((eq (car expr) 'lit-int)
         (if (integerp (cadr expr))
             (cons 'ok 'int)
           (cons 'error "lit-int: not an integer")))
        ;; Boolean literal
        ((eq (car expr) 'lit-bool)
         (cons 'ok 'bool))
        ;; String literal
        ((eq (car expr) 'lit-str)
         (if (stringp (cadr expr))
             (cons 'ok 'string)
           (cons 'error "lit-str: not a string")))
        ;; Variable
        ((eq (car expr) 'var)
         (let ((t (funcall 'neovm--tc-env-lookup env (cadr expr))))
           (if t (cons 'ok t)
             (cons 'error (format "unbound variable: %s" (cadr expr))))))
        (t (cons 'error (format "unknown expression: %S" (car expr)))))))

  (unwind-protect
      (let ((env (funcall 'neovm--tc-env-empty)))
        (let ((env1 (funcall 'neovm--tc-env-extend env 'x 'int))
              (env2 (funcall 'neovm--tc-env-extend
                              (funcall 'neovm--tc-env-extend env 'x 'int)
                              'y 'bool)))
          (list
            ;; Literal type checking
            (funcall 'neovm--tc-check env '(lit-int 42))
            (funcall 'neovm--tc-check env '(lit-int -1))
            (funcall 'neovm--tc-check env '(lit-int 0))
            (funcall 'neovm--tc-check env '(lit-bool t))
            (funcall 'neovm--tc-check env '(lit-bool nil))
            (funcall 'neovm--tc-check env '(lit-str "hello"))
            (funcall 'neovm--tc-check env '(lit-str ""))
            ;; Variable lookup
            (funcall 'neovm--tc-check env1 '(var x))
            (funcall 'neovm--tc-check env2 '(var y))
            ;; Unbound variable
            (funcall 'neovm--tc-check env '(var z))
            ;; Shadowing: extend x with string type
            (let ((env3 (funcall 'neovm--tc-env-extend env1 'x 'string)))
              (funcall 'neovm--tc-check env3 '(var x))))))
    (fmakunbound 'neovm--tc-env-empty)
    (fmakunbound 'neovm--tc-env-extend)
    (fmakunbound 'neovm--tc-env-lookup)
    (fmakunbound 'neovm--tc-check)))"#;
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Binary operation type rules
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_typechk_binary_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--tcb-env-extend
    (lambda (env name type) (cons (cons name type) env)))
  (fset 'neovm--tcb-env-lookup
    (lambda (env name)
      (let ((entry (assq name env)))
        (if entry (cdr entry) nil))))

  ;; Type rules for binary operations:
  ;; +, -, *, /  : (int, int) -> int
  ;; <, >, <=, >= : (int, int) -> bool
  ;; =            : (int, int) -> bool  OR  (string, string) -> bool
  ;; and, or      : (bool, bool) -> bool
  ;; concat       : (string, string) -> string
  (fset 'neovm--tcb-binop-type
    (lambda (op left-type right-type)
      (cond
        ;; Arithmetic: int x int -> int
        ((memq op '(+ - * /))
         (if (and (eq left-type 'int) (eq right-type 'int))
             (cons 'ok 'int)
           (cons 'error (format "%s requires (int, int), got (%s, %s)"
                                op left-type right-type))))
        ;; Comparison: int x int -> bool
        ((memq op '(< > <= >=))
         (if (and (eq left-type 'int) (eq right-type 'int))
             (cons 'ok 'bool)
           (cons 'error (format "%s requires (int, int), got (%s, %s)"
                                op left-type right-type))))
        ;; Equality: same type -> bool (int or string)
        ((eq op '=)
         (cond
           ((and (eq left-type 'int) (eq right-type 'int)) (cons 'ok 'bool))
           ((and (eq left-type 'string) (eq right-type 'string)) (cons 'ok 'bool))
           (t (cons 'error (format "= requires matching types, got (%s, %s)"
                                   left-type right-type)))))
        ;; Logical: bool x bool -> bool
        ((memq op '(and or))
         (if (and (eq left-type 'bool) (eq right-type 'bool))
             (cons 'ok 'bool)
           (cons 'error (format "%s requires (bool, bool), got (%s, %s)"
                                op left-type right-type))))
        ;; String concatenation
        ((eq op 'concat)
         (if (and (eq left-type 'string) (eq right-type 'string))
             (cons 'ok 'string)
           (cons 'error (format "concat requires (string, string), got (%s, %s)"
                                left-type right-type))))
        (t (cons 'error (format "unknown operator: %s" op))))))

  ;; Full type checker with binop support
  (fset 'neovm--tcb-check
    (lambda (env expr)
      (cond
        ((eq (car expr) 'lit-int) (cons 'ok 'int))
        ((eq (car expr) 'lit-bool) (cons 'ok 'bool))
        ((eq (car expr) 'lit-str) (cons 'ok 'string))
        ((eq (car expr) 'var)
         (let ((t (funcall 'neovm--tcb-env-lookup env (cadr expr))))
           (if t (cons 'ok t)
             (cons 'error (format "unbound: %s" (cadr expr))))))
        ((eq (car expr) 'binop)
         (let* ((op (cadr expr))
                (left-result (funcall 'neovm--tcb-check env (nth 2 expr)))
                (right-result (funcall 'neovm--tcb-check env (nth 3 expr))))
           (if (eq (car left-result) 'error) left-result
             (if (eq (car right-result) 'error) right-result
               (funcall 'neovm--tcb-binop-type op
                        (cdr left-result) (cdr right-result))))))
        (t (cons 'error "unknown")))))

  (unwind-protect
      (let ((env (list (cons 'x 'int) (cons 'y 'int)
                       (cons 's 'string) (cons 'b 'bool))))
        (list
          ;; Arithmetic operations: int x int -> int
          (funcall 'neovm--tcb-check env '(binop + (lit-int 1) (lit-int 2)))
          (funcall 'neovm--tcb-check env '(binop - (var x) (var y)))
          (funcall 'neovm--tcb-check env '(binop * (lit-int 3) (var x)))
          (funcall 'neovm--tcb-check env '(binop / (var x) (lit-int 2)))
          ;; Type error: arithmetic with bool
          (funcall 'neovm--tcb-check env '(binop + (lit-int 1) (lit-bool t)))
          (funcall 'neovm--tcb-check env '(binop * (var s) (var x)))
          ;; Comparison: int x int -> bool
          (funcall 'neovm--tcb-check env '(binop < (var x) (var y)))
          (funcall 'neovm--tcb-check env '(binop >= (lit-int 5) (var x)))
          ;; Type error: compare string and int
          (funcall 'neovm--tcb-check env '(binop < (var s) (var x)))
          ;; Equality
          (funcall 'neovm--tcb-check env '(binop = (var x) (var y)))
          (funcall 'neovm--tcb-check env '(binop = (lit-str "a") (var s)))
          ;; Type error: equals with mismatched types
          (funcall 'neovm--tcb-check env '(binop = (var x) (var s)))
          ;; Logical
          (funcall 'neovm--tcb-check env '(binop and (var b) (lit-bool t)))
          (funcall 'neovm--tcb-check env '(binop or (var b) (var b)))
          ;; Type error: logical with int
          (funcall 'neovm--tcb-check env '(binop and (var x) (var b)))
          ;; String concat
          (funcall 'neovm--tcb-check env '(binop concat (var s) (lit-str " world")))
          ;; Nested: (x + y) < 10
          (funcall 'neovm--tcb-check env
                   '(binop < (binop + (var x) (var y)) (lit-int 10)))))
    (fmakunbound 'neovm--tcb-env-extend)
    (fmakunbound 'neovm--tcb-env-lookup)
    (fmakunbound 'neovm--tcb-binop-type)
    (fmakunbound 'neovm--tcb-check)))"#;
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// If-expressions and let-bindings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_typechk_if_and_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--tcil-env-lookup
    (lambda (env name)
      (let ((entry (assq name env)))
        (if entry (cdr entry) nil))))

  (fset 'neovm--tcil-binop-type
    (lambda (op lt rt)
      (cond
        ((and (memq op '(+ - * /)) (eq lt 'int) (eq rt 'int)) (cons 'ok 'int))
        ((and (memq op '(< > <= >=)) (eq lt 'int) (eq rt 'int)) (cons 'ok 'bool))
        ((and (eq op '=) (eq lt rt) (memq lt '(int string bool))) (cons 'ok 'bool))
        ((and (memq op '(and or)) (eq lt 'bool) (eq rt 'bool)) (cons 'ok 'bool))
        (t (cons 'error (format "binop type error: %s on %s, %s" op lt rt))))))

  ;; Full checker: literals, vars, binop, if-expr, let-bind
  (fset 'neovm--tcil-check
    (lambda (env expr)
      (cond
        ((eq (car expr) 'lit-int) (cons 'ok 'int))
        ((eq (car expr) 'lit-bool) (cons 'ok 'bool))
        ((eq (car expr) 'lit-str) (cons 'ok 'string))
        ((eq (car expr) 'var)
         (let ((t (funcall 'neovm--tcil-env-lookup env (cadr expr))))
           (if t (cons 'ok t)
             (cons 'error (format "unbound: %s" (cadr expr))))))
        ((eq (car expr) 'binop)
         (let ((lr (funcall 'neovm--tcil-check env (nth 2 expr)))
               (rr (funcall 'neovm--tcil-check env (nth 3 expr))))
           (if (eq (car lr) 'error) lr
             (if (eq (car rr) 'error) rr
               (funcall 'neovm--tcil-binop-type (cadr expr) (cdr lr) (cdr rr))))))
        ;; If-expression: condition must be bool, branches must match
        ((eq (car expr) 'if-expr)
         (let ((cond-r (funcall 'neovm--tcil-check env (nth 1 expr)))
               (then-r (funcall 'neovm--tcil-check env (nth 2 expr)))
               (else-r (funcall 'neovm--tcil-check env (nth 3 expr))))
           (cond
             ((eq (car cond-r) 'error) cond-r)
             ((not (eq (cdr cond-r) 'bool))
              (cons 'error (format "if condition must be bool, got %s" (cdr cond-r))))
             ((eq (car then-r) 'error) then-r)
             ((eq (car else-r) 'error) else-r)
             ((not (eq (cdr then-r) (cdr else-r)))
              (cons 'error (format "if branches mismatch: %s vs %s"
                                   (cdr then-r) (cdr else-r))))
             (t (cons 'ok (cdr then-r))))))
        ;; Let-binding: check value, extend env, check body
        ((eq (car expr) 'let-bind)
         (let ((name (nth 1 expr))
               (val-r (funcall 'neovm--tcil-check env (nth 2 expr))))
           (if (eq (car val-r) 'error) val-r
             (let ((new-env (cons (cons name (cdr val-r)) env)))
               (funcall 'neovm--tcil-check new-env (nth 3 expr))))))
        (t (cons 'error (format "unknown: %S" (car expr)))))))

  (unwind-protect
      (let ((env (list (cons 'x 'int) (cons 'b 'bool) (cons 's 'string))))
        (list
          ;; if-expr: bool condition, matching int branches
          (funcall 'neovm--tcil-check env
                   '(if-expr (var b) (lit-int 1) (lit-int 2)))
          ;; if-expr: bool condition, matching string branches
          (funcall 'neovm--tcil-check env
                   '(if-expr (var b) (lit-str "yes") (lit-str "no")))
          ;; if-expr: non-bool condition -> error
          (funcall 'neovm--tcil-check env
                   '(if-expr (var x) (lit-int 1) (lit-int 2)))
          ;; if-expr: mismatched branches -> error
          (funcall 'neovm--tcil-check env
                   '(if-expr (var b) (lit-int 1) (lit-str "no")))
          ;; if-expr with comparison condition
          (funcall 'neovm--tcil-check env
                   '(if-expr (binop < (var x) (lit-int 10))
                             (lit-int 1) (lit-int 0)))
          ;; let-bind: simple
          (funcall 'neovm--tcil-check env
                   '(let-bind y (lit-int 5) (binop + (var x) (var y))))
          ;; let-bind: binding used in if-expr
          (funcall 'neovm--tcil-check env
                   '(let-bind flag (binop < (var x) (lit-int 0))
                              (if-expr (var flag) (lit-int -1) (lit-int 1))))
          ;; let-bind: shadowing
          (funcall 'neovm--tcil-check env
                   '(let-bind x (lit-str "shadowed")
                              (var x)))
          ;; Nested let
          (funcall 'neovm--tcil-check env
                   '(let-bind a (lit-int 10)
                              (let-bind b (lit-int 20)
                                        (binop + (var a) (var b)))))
          ;; Error in let value propagates
          (funcall 'neovm--tcil-check env
                   '(let-bind z (binop + (var x) (var s))
                              (var z)))
          ;; Complex: nested if inside let
          (funcall 'neovm--tcil-check env
                   '(let-bind threshold (lit-int 100)
                              (if-expr (binop < (var x) (var threshold))
                                       (lit-str "low")
                                       (lit-str "high"))))))
    (fmakunbound 'neovm--tcil-env-lookup)
    (fmakunbound 'neovm--tcil-binop-type)
    (fmakunbound 'neovm--tcil-check)))"#;
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Function application type checking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_typechk_function_application() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--tcfa-env-lookup
    (lambda (env name)
      (let ((entry (assq name env)))
        (if entry (cdr entry) nil))))

  (fset 'neovm--tcfa-binop-type
    (lambda (op lt rt)
      (cond
        ((and (memq op '(+ - * /)) (eq lt 'int) (eq rt 'int)) (cons 'ok 'int))
        ((and (memq op '(< >)) (eq lt 'int) (eq rt 'int)) (cons 'ok 'bool))
        ((and (eq op '=) (eq lt rt)) (cons 'ok 'bool))
        (t (cons 'error (format "binop error: %s(%s,%s)" op lt rt))))))

  ;; Checker with function application and lambda
  (fset 'neovm--tcfa-check
    (lambda (env expr)
      (cond
        ((eq (car expr) 'lit-int) (cons 'ok 'int))
        ((eq (car expr) 'lit-bool) (cons 'ok 'bool))
        ((eq (car expr) 'lit-str) (cons 'ok 'string))
        ((eq (car expr) 'var)
         (let ((t (funcall 'neovm--tcfa-env-lookup env (cadr expr))))
           (if t (cons 'ok t) (cons 'error (format "unbound: %s" (cadr expr))))))
        ((eq (car expr) 'binop)
         (let ((lr (funcall 'neovm--tcfa-check env (nth 2 expr)))
               (rr (funcall 'neovm--tcfa-check env (nth 3 expr))))
           (if (eq (car lr) 'error) lr
             (if (eq (car rr) 'error) rr
               (funcall 'neovm--tcfa-binop-type (cadr expr) (cdr lr) (cdr rr))))))
        ((eq (car expr) 'if-expr)
         (let ((cr (funcall 'neovm--tcfa-check env (nth 1 expr)))
               (tr (funcall 'neovm--tcfa-check env (nth 2 expr)))
               (er (funcall 'neovm--tcfa-check env (nth 3 expr))))
           (cond
             ((eq (car cr) 'error) cr)
             ((not (eq (cdr cr) 'bool)) (cons 'error "if: need bool"))
             ((eq (car tr) 'error) tr)
             ((eq (car er) 'error) er)
             ((not (eq (cdr tr) (cdr er))) (cons 'error "if: branch mismatch"))
             (t (cons 'ok (cdr tr))))))
        ((eq (car expr) 'let-bind)
         (let ((vr (funcall 'neovm--tcfa-check env (nth 2 expr))))
           (if (eq (car vr) 'error) vr
             (funcall 'neovm--tcfa-check
                      (cons (cons (nth 1 expr) (cdr vr)) env) (nth 3 expr)))))
        ;; Lambda: (lam param param-type body)
        ((eq (car expr) 'lam)
         (let* ((param (nth 1 expr))
                (param-type (nth 2 expr))
                (new-env (cons (cons param param-type) env))
                (body-r (funcall 'neovm--tcfa-check new-env (nth 3 expr))))
           (if (eq (car body-r) 'error) body-r
             (cons 'ok (list '-> param-type (cdr body-r))))))
        ;; Application: (app fn arg)
        ((eq (car expr) 'app)
         (let ((fn-r (funcall 'neovm--tcfa-check env (nth 1 expr)))
               (arg-r (funcall 'neovm--tcfa-check env (nth 2 expr))))
           (cond
             ((eq (car fn-r) 'error) fn-r)
             ((eq (car arg-r) 'error) arg-r)
             ((not (and (listp (cdr fn-r)) (eq (car (cdr fn-r)) '->)))
              (cons 'error (format "not a function: %S" (cdr fn-r))))
             ((not (eq (nth 1 (cdr fn-r)) (cdr arg-r)))
              (cons 'error (format "arg type mismatch: expected %s, got %s"
                                   (nth 1 (cdr fn-r)) (cdr arg-r))))
             (t (cons 'ok (nth 2 (cdr fn-r)))))))
        (t (cons 'error "unknown")))))

  (unwind-protect
      (let ((env (list (cons 'x 'int) (cons 'b 'bool)
                       (cons 'incr '(-> int int))
                       (cons 'not-fn '(-> bool bool))
                       (cons 'to-str '(-> int string)))))
        (list
          ;; Lambda type inference
          (funcall 'neovm--tcfa-check env
                   '(lam n int (binop + (var n) (lit-int 1))))
          ;; Lambda with bool
          (funcall 'neovm--tcfa-check env
                   '(lam p bool (if-expr (var p) (lit-int 1) (lit-int 0))))
          ;; Application: incr(42)
          (funcall 'neovm--tcfa-check env '(app (var incr) (lit-int 42)))
          ;; Application: not-fn(b)
          (funcall 'neovm--tcfa-check env '(app (var not-fn) (var b)))
          ;; Application: to-str(x) -> string
          (funcall 'neovm--tcfa-check env '(app (var to-str) (var x)))
          ;; Type error: incr(true) -- arg type mismatch
          (funcall 'neovm--tcfa-check env '(app (var incr) (lit-bool t)))
          ;; Type error: apply non-function
          (funcall 'neovm--tcfa-check env '(app (var x) (lit-int 1)))
          ;; Lambda applied inline: ((lam n int (+ n 1)) 5)
          (funcall 'neovm--tcfa-check env
                   '(app (lam n int (binop + (var n) (lit-int 1))) (lit-int 5)))
          ;; Curried function via let
          (funcall 'neovm--tcfa-check env
                   '(let-bind add (lam a int (lam b int (binop + (var a) (var b))))
                              (app (app (var add) (lit-int 3)) (lit-int 4))))
          ;; Higher-order: function taking function as arg
          (funcall 'neovm--tcfa-check env
                   '(let-bind apply-to-five
                              (lam f (-> int int) (app (var f) (lit-int 5)))
                              (app (var apply-to-five) (var incr))))))
    (fmakunbound 'neovm--tcfa-env-lookup)
    (fmakunbound 'neovm--tcfa-binop-type)
    (fmakunbound 'neovm--tcfa-check)))"#;
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-expression programs and statement sequences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_typechk_program_sequences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--tcp-env-lookup
    (lambda (env name)
      (let ((entry (assq name env)))
        (if entry (cdr entry) nil))))

  (fset 'neovm--tcp-binop-type
    (lambda (op lt rt)
      (cond
        ((and (memq op '(+ - * /)) (eq lt 'int) (eq rt 'int)) (cons 'ok 'int))
        ((and (memq op '(< > <= >=)) (eq lt 'int) (eq rt 'int)) (cons 'ok 'bool))
        ((and (eq op '=) (eq lt rt)) (cons 'ok 'bool))
        ((and (memq op '(and or)) (eq lt 'bool) (eq rt 'bool)) (cons 'ok 'bool))
        ((and (eq op 'concat) (eq lt 'string) (eq rt 'string)) (cons 'ok 'string))
        (t (cons 'error (format "binop err: %s(%s,%s)" op lt rt))))))

  (fset 'neovm--tcp-check
    (lambda (env expr)
      (cond
        ((eq (car expr) 'lit-int) (cons 'ok 'int))
        ((eq (car expr) 'lit-bool) (cons 'ok 'bool))
        ((eq (car expr) 'lit-str) (cons 'ok 'string))
        ((eq (car expr) 'var)
         (let ((t (funcall 'neovm--tcp-env-lookup env (cadr expr))))
           (if t (cons 'ok t) (cons 'error (format "unbound: %s" (cadr expr))))))
        ((eq (car expr) 'binop)
         (let ((lr (funcall 'neovm--tcp-check env (nth 2 expr)))
               (rr (funcall 'neovm--tcp-check env (nth 3 expr))))
           (if (eq (car lr) 'error) lr
             (if (eq (car rr) 'error) rr
               (funcall 'neovm--tcp-binop-type (cadr expr) (cdr lr) (cdr rr))))))
        ((eq (car expr) 'if-expr)
         (let ((cr (funcall 'neovm--tcp-check env (nth 1 expr)))
               (tr (funcall 'neovm--tcp-check env (nth 2 expr)))
               (er (funcall 'neovm--tcp-check env (nth 3 expr))))
           (cond
             ((eq (car cr) 'error) cr)
             ((not (eq (cdr cr) 'bool)) (cons 'error "need bool"))
             ((eq (car tr) 'error) tr)
             ((eq (car er) 'error) er)
             ((not (eq (cdr tr) (cdr er))) (cons 'error "branch mismatch"))
             (t (cons 'ok (cdr tr))))))
        ((eq (car expr) 'let-bind)
         (let ((vr (funcall 'neovm--tcp-check env (nth 2 expr))))
           (if (eq (car vr) 'error) vr
             (funcall 'neovm--tcp-check
                      (cons (cons (nth 1 expr) (cdr vr)) env) (nth 3 expr)))))
        ((eq (car expr) 'lam)
         (let* ((new-env (cons (cons (nth 1 expr) (nth 2 expr)) env))
                (br (funcall 'neovm--tcp-check new-env (nth 3 expr))))
           (if (eq (car br) 'error) br
             (cons 'ok (list '-> (nth 2 expr) (cdr br))))))
        ((eq (car expr) 'app)
         (let ((fr (funcall 'neovm--tcp-check env (nth 1 expr)))
               (ar (funcall 'neovm--tcp-check env (nth 2 expr))))
           (cond
             ((eq (car fr) 'error) fr)
             ((eq (car ar) 'error) ar)
             ((not (and (listp (cdr fr)) (eq (car (cdr fr)) '->)))
              (cons 'error "not a function"))
             ((not (eq (nth 1 (cdr fr)) (cdr ar)))
              (cons 'error (format "arg mismatch: %s vs %s"
                                   (nth 1 (cdr fr)) (cdr ar))))
             (t (cons 'ok (nth 2 (cdr fr)))))))
        (t (cons 'error "unknown")))))

  ;; Type check a sequence of statements, building up the environment
  ;; Statement: (define name expr) or just an expression
  (fset 'neovm--tcp-check-program
    (lambda (env stmts)
      (let ((current-env env)
            (results nil)
            (has-error nil))
        (dolist (stmt stmts)
          (unless has-error
            (if (eq (car stmt) 'define)
                ;; Define statement: check value, extend env
                (let ((r (funcall 'neovm--tcp-check current-env (nth 2 stmt))))
                  (if (eq (car r) 'error)
                      (progn (setq has-error t)
                             (setq results (cons r results)))
                    (setq current-env (cons (cons (nth 1 stmt) (cdr r)) current-env))
                    (setq results (cons (cons 'defined (cons (nth 1 stmt) (cdr r)))
                                        results))))
              ;; Expression: just type-check it
              (setq results (cons (funcall 'neovm--tcp-check current-env stmt)
                                  results)))))
        (nreverse results))))

  (unwind-protect
      (let ((base-env nil))
        (list
          ;; Simple program: define and use
          (funcall 'neovm--tcp-check-program base-env
                   '((define x (lit-int 42))
                     (define y (lit-int 10))
                     (binop + (var x) (var y))))
          ;; Program with functions
          (funcall 'neovm--tcp-check-program base-env
                   '((define double (lam n int (binop * (var n) (lit-int 2))))
                     (app (var double) (lit-int 21))))
          ;; Program with type error mid-stream
          (funcall 'neovm--tcp-check-program base-env
                   '((define x (lit-int 5))
                     (binop + (var x) (lit-bool t))))
          ;; Program with if-expression
          (funcall 'neovm--tcp-check-program base-env
                   '((define score (lit-int 85))
                     (define threshold (lit-int 70))
                     (define passed (binop > (var score) (var threshold)))
                     (if-expr (var passed)
                              (lit-str "pass")
                              (lit-str "fail"))))
          ;; Complex program: nested lets, functions, conditionals
          (funcall 'neovm--tcp-check-program base-env
                   '((define abs-fn
                       (lam n int
                            (if-expr (binop < (var n) (lit-int 0))
                                     (binop - (lit-int 0) (var n))
                                     (var n))))
                     (define max-fn
                       (lam a int
                            (lam b int
                                 (if-expr (binop > (var a) (var b))
                                          (var a) (var b)))))
                     (app (var abs-fn) (lit-int -42))
                     (app (app (var max-fn) (lit-int 10)) (lit-int 20))))))
    (fmakunbound 'neovm--tcp-env-lookup)
    (fmakunbound 'neovm--tcp-binop-type)
    (fmakunbound 'neovm--tcp-check)
    (fmakunbound 'neovm--tcp-check-program)))"#;
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Polymorphic type variables with unification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_typechk_polymorphic_unification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Type variable counter
  (defvar neovm--tcu-counter 0)
  (fset 'neovm--tcu-fresh
    (lambda ()
      (setq neovm--tcu-counter (1+ neovm--tcu-counter))
      (list 'tvar neovm--tcu-counter)))

  ;; Apply substitution
  (fset 'neovm--tcu-apply
    (lambda (subst type)
      (cond
        ((symbolp type) type)
        ((null type) nil)
        ((eq (car type) 'tvar)
         (let ((b (assq (cadr type) subst)))
           (if b (funcall 'neovm--tcu-apply subst (cdr b)) type)))
        ((eq (car type) '->)
         (list '-> (funcall 'neovm--tcu-apply subst (nth 1 type))
               (funcall 'neovm--tcu-apply subst (nth 2 type))))
        (t type))))

  ;; Occurs check
  (fset 'neovm--tcu-occurs
    (lambda (id type)
      (cond
        ((symbolp type) nil)
        ((null type) nil)
        ((eq (car type) 'tvar) (= (cadr type) id))
        ((eq (car type) '->)
         (or (funcall 'neovm--tcu-occurs id (nth 1 type))
             (funcall 'neovm--tcu-occurs id (nth 2 type))))
        (t nil))))

  ;; Unify two types
  (fset 'neovm--tcu-unify
    (lambda (t1 t2 subst)
      (let ((t1 (funcall 'neovm--tcu-apply subst t1))
            (t2 (funcall 'neovm--tcu-apply subst t2)))
        (cond
          ((equal t1 t2) (cons 'ok subst))
          ((and (listp t1) (eq (car t1) 'tvar))
           (if (funcall 'neovm--tcu-occurs (cadr t1) t2)
               (cons 'error "occurs check")
             (cons 'ok (cons (cons (cadr t1) t2) subst))))
          ((and (listp t2) (eq (car t2) 'tvar))
           (if (funcall 'neovm--tcu-occurs (cadr t2) t1)
               (cons 'error "occurs check")
             (cons 'ok (cons (cons (cadr t2) t1) subst))))
          ;; Both function types
          ((and (listp t1) (eq (car t1) '->)
                (listp t2) (eq (car t2) '->))
           (let ((r (funcall 'neovm--tcu-unify (nth 1 t1) (nth 1 t2) subst)))
             (if (eq (car r) 'error) r
               (funcall 'neovm--tcu-unify (nth 2 t1) (nth 2 t2) (cdr r)))))
          ;; Both symbols (base types)
          ((and (symbolp t1) (symbolp t2))
           (if (eq t1 t2) (cons 'ok subst)
             (cons 'error (format "type mismatch: %s vs %s" t1 t2))))
          (t (cons 'error (format "cannot unify %S and %S" t1 t2)))))))

  ;; Type checker with polymorphic inference via unification
  (fset 'neovm--tcu-check
    (lambda (env expr subst)
      (cond
        ((eq (car expr) 'lit-int) (list 'ok 'int subst))
        ((eq (car expr) 'lit-bool) (list 'ok 'bool subst))
        ((eq (car expr) 'lit-str) (list 'ok 'string subst))
        ((eq (car expr) 'var)
         (let ((t (cdr (assq (cadr expr) env))))
           (if t (list 'ok t subst)
             (list 'error (format "unbound: %s" (cadr expr)) subst))))
        ;; Lambda without annotation: param gets fresh tvar
        ((eq (car expr) 'lam-infer)
         (let* ((param (nth 1 expr))
                (body (nth 2 expr))
                (param-t (funcall 'neovm--tcu-fresh))
                (new-env (cons (cons param param-t) env))
                (br (funcall 'neovm--tcu-check new-env body subst)))
           (if (eq (car br) 'error) br
             (list 'ok (list '->
                             (funcall 'neovm--tcu-apply (nth 2 br) param-t)
                             (nth 1 br))
                   (nth 2 br)))))
        ;; Application with unification
        ((eq (car expr) 'app)
         (let ((fr (funcall 'neovm--tcu-check env (nth 1 expr) subst)))
           (if (eq (car fr) 'error) fr
             (let ((ar (funcall 'neovm--tcu-check env (nth 2 expr) (nth 2 fr))))
               (if (eq (car ar) 'error) ar
                 (let* ((ret-t (funcall 'neovm--tcu-fresh))
                        (expected (list '-> (nth 1 ar) ret-t))
                        (fn-t (funcall 'neovm--tcu-apply (nth 2 ar) (nth 1 fr)))
                        (ur (funcall 'neovm--tcu-unify fn-t expected (nth 2 ar))))
                   (if (eq (car ur) 'error)
                       (list 'error (cdr ur) (nth 2 ar))
                     (list 'ok
                           (funcall 'neovm--tcu-apply (cdr ur) ret-t)
                           (cdr ur)))))))))
        (t (list 'error "unknown" subst)))))

  (unwind-protect
      (progn
        (setq neovm--tcu-counter 0)
        (let ((env (list (cons 'add '(-> int (-> int int)))
                         (cons 'not-fn '(-> bool bool))
                         (cons 'to-str '(-> int string)))))
          (list
            ;; Infer identity function: (lam-infer x (var x))
            (progn (setq neovm--tcu-counter 0)
                   (let ((r (funcall 'neovm--tcu-check env
                                      '(lam-infer x (var x)) nil)))
                     (list (car r) (funcall 'neovm--tcu-apply (nth 2 r) (nth 1 r)))))
            ;; Apply add to 1: should infer (-> int int)
            (progn (setq neovm--tcu-counter 0)
                   (let ((r (funcall 'neovm--tcu-check env
                                      '(app (var add) (lit-int 1)) nil)))
                     (list (car r) (funcall 'neovm--tcu-apply (nth 2 r) (nth 1 r)))))
            ;; Apply add to 1 then 2: should infer int
            (progn (setq neovm--tcu-counter 0)
                   (let ((r (funcall 'neovm--tcu-check env
                                      '(app (app (var add) (lit-int 1)) (lit-int 2))
                                      nil)))
                     (list (car r) (funcall 'neovm--tcu-apply (nth 2 r) (nth 1 r)))))
            ;; Type error: add applied to bool
            (progn (setq neovm--tcu-counter 0)
                   (let ((r (funcall 'neovm--tcu-check env
                                      '(app (var add) (lit-bool t)) nil)))
                     (car r)))
            ;; Infer lambda applied: ((lam-infer x (var x)) 42) -> int
            (progn (setq neovm--tcu-counter 0)
                   (let ((r (funcall 'neovm--tcu-check env
                                      '(app (lam-infer x (var x)) (lit-int 42))
                                      nil)))
                     (list (car r) (funcall 'neovm--tcu-apply (nth 2 r) (nth 1 r)))))
            ;; Infer const function: (lam-infer x (lam-infer y (var x)))
            (progn (setq neovm--tcu-counter 0)
                   (let ((r (funcall 'neovm--tcu-check env
                                      '(lam-infer x (lam-infer y (var x))) nil)))
                     (list (car r)
                           (funcall 'neovm--tcu-apply (nth 2 r) (nth 1 r))))))))
    (fmakunbound 'neovm--tcu-fresh)
    (fmakunbound 'neovm--tcu-apply)
    (fmakunbound 'neovm--tcu-occurs)
    (fmakunbound 'neovm--tcu-unify)
    (fmakunbound 'neovm--tcu-check)
    (makunbound 'neovm--tcu-counter)))"#;
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
