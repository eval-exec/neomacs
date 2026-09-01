//! Advanced oracle parity tests for interpreter/evaluator combinations:
//! register-based VM, bytecode compiler + VM for simple expressions,
//! type-checking interpreter with runtime errors, interpreter with
//! closures/environments, tail-call optimization detection, and
//! interpreter with continuations.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Register-based VM (vs stack-based)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interp_adv_register_vm() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A register-based VM with instructions that operate on numbered registers.
    // Instructions: (load-imm REG VAL), (add REG-DST REG-A REG-B),
    // (sub REG-DST REG-A REG-B), (mul REG-DST REG-A REG-B),
    // (mov REG-DST REG-SRC), (ret REG)
    let form = r#"(progn
  (fset 'neovm--test-regvm-run
    (lambda (instrs)
      (let ((regs (make-vector 16 0))
            (pc 0)
            (result nil)
            (max-steps 100))
        (while (and (< pc (length instrs)) (not result)
                    (> max-steps 0))
          (setq max-steps (1- max-steps))
          (let ((instr (nth pc instrs)))
            (let ((op (car instr)))
              (cond
               ((eq op 'load-imm)
                (aset regs (cadr instr) (caddr instr))
                (setq pc (1+ pc)))
               ((eq op 'add)
                (aset regs (cadr instr)
                      (+ (aref regs (caddr instr))
                         (aref regs (cadddr instr))))
                (setq pc (1+ pc)))
               ((eq op 'sub)
                (aset regs (cadr instr)
                      (- (aref regs (caddr instr))
                         (aref regs (cadddr instr))))
                (setq pc (1+ pc)))
               ((eq op 'mul)
                (aset regs (cadr instr)
                      (* (aref regs (caddr instr))
                         (aref regs (cadddr instr))))
                (setq pc (1+ pc)))
               ((eq op 'mov)
                (aset regs (cadr instr)
                      (aref regs (caddr instr)))
                (setq pc (1+ pc)))
               ((eq op 'ret)
                (setq result (aref regs (cadr instr))))
               (t (setq result (list 'error 'unknown-op op))
                  (setq pc (length instrs)))))))
        result)))

  (unwind-protect
      (list
       ;; Compute 3 + 4
       (funcall 'neovm--test-regvm-run
                '((load-imm 0 3)
                  (load-imm 1 4)
                  (add 2 0 1)
                  (ret 2)))
       ;; Compute (5 * 6) - 10
       (funcall 'neovm--test-regvm-run
                '((load-imm 0 5)
                  (load-imm 1 6)
                  (mul 2 0 1)
                  (load-imm 3 10)
                  (sub 4 2 3)
                  (ret 4)))
       ;; Compute (a + b) * (a - b) where a=10, b=3 => 7 * 13 = 91
       (funcall 'neovm--test-regvm-run
                '((load-imm 0 10)
                  (load-imm 1 3)
                  (add 2 0 1)
                  (sub 3 0 1)
                  (mul 4 2 3)
                  (ret 4)))
       ;; Chain of moves
       (funcall 'neovm--test-regvm-run
                '((load-imm 0 42)
                  (mov 1 0)
                  (mov 2 1)
                  (mov 3 2)
                  (ret 3)))
       ;; Compute sum 1+2+3+4+5 = 15 using register accumulation
       (funcall 'neovm--test-regvm-run
                '((load-imm 0 0)
                  (load-imm 1 1)
                  (add 0 0 1)
                  (load-imm 1 2)
                  (add 0 0 1)
                  (load-imm 1 3)
                  (add 0 0 1)
                  (load-imm 1 4)
                  (add 0 0 1)
                  (load-imm 1 5)
                  (add 0 0 1)
                  (ret 0))))
    (fmakunbound 'neovm--test-regvm-run)))"#;
    let expect = expect_test::expect![[r#""OK (7 20 91 42 15)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bytecode compiler + VM for simple expressions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interp_adv_bytecode_compiler() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compile S-expression arithmetic to bytecode (vector of opcodes),
    // then run on a VM. Bytecodes: 0=PUSH, 1=ADD, 2=SUB, 3=MUL, 4=DIV,
    // 5=NEG, 6=DUP
    let form = r#"(progn
  ;; Compiler: expr -> bytecode vector
  (fset 'neovm--test-bc-compile
    (lambda (expr)
      (let ((code nil))
        (fset 'neovm--test-bc-emit
          (lambda (e)
            (cond
             ((numberp e)
              (setq code (cons (list 0 e) code)))
             ((and (consp e) (memq (car e) '(+ - * /)))
              (let ((op (car e))
                    (args (cdr e)))
                ;; Compile first arg
                (funcall 'neovm--test-bc-emit (car args))
                ;; Compile remaining and apply binary op
                (dolist (arg (cdr args))
                  (funcall 'neovm--test-bc-emit arg)
                  (setq code (cons (list (cond ((eq op '+) 1)
                                               ((eq op '-) 2)
                                               ((eq op '*) 3)
                                               ((eq op '/) 4)))
                                   code)))))
             ((and (consp e) (eq (car e) 'neg))
              (funcall 'neovm--test-bc-emit (cadr e))
              (setq code (cons '(5) code)))
             ((and (consp e) (eq (car e) 'dup))
              (funcall 'neovm--test-bc-emit (cadr e))
              (setq code (cons '(6) code))))))
        (funcall 'neovm--test-bc-emit expr)
        (vconcat (nreverse code)))))

  ;; VM: execute bytecode
  (fset 'neovm--test-bc-run
    (lambda (bytecode)
      (let ((stack nil)
            (pc 0)
            (len (length bytecode)))
        (while (< pc len)
          (let* ((instr (aref bytecode pc))
                 (op (car instr)))
            (cond
             ((= op 0) ;; PUSH
              (setq stack (cons (cadr instr) stack)))
             ((= op 1) ;; ADD
              (let ((b (car stack)) (a (cadr stack)))
                (setq stack (cons (+ a b) (cddr stack)))))
             ((= op 2) ;; SUB
              (let ((b (car stack)) (a (cadr stack)))
                (setq stack (cons (- a b) (cddr stack)))))
             ((= op 3) ;; MUL
              (let ((b (car stack)) (a (cadr stack)))
                (setq stack (cons (* a b) (cddr stack)))))
             ((= op 4) ;; DIV
              (let ((b (car stack)) (a (cadr stack)))
                (setq stack (cons (/ a b) (cddr stack)))))
             ((= op 5) ;; NEG
              (setq stack (cons (- (car stack)) (cdr stack))))
             ((= op 6) ;; DUP
              (setq stack (cons (car stack) stack)))))
          (setq pc (1+ pc)))
        (car stack))))

  (unwind-protect
      (let ((exprs '((+ 1 2 3)
                     (* 2 3 4)
                     (- 100 30 20)
                     (+ (* 3 4) (* 5 6))
                     (neg 42)
                     (dup 7)
                     (+ (dup 5) 3))))
        (mapcar (lambda (e)
                  (let* ((bc (funcall 'neovm--test-bc-compile e))
                         (result (funcall 'neovm--test-bc-run bc)))
                    (list 'expr e
                          'bytecode-len (length bc)
                          'result result)))
                exprs))
    (fmakunbound 'neovm--test-bc-compile)
    (fmakunbound 'neovm--test-bc-emit)
    (fmakunbound 'neovm--test-bc-run)))"#;
    let expect = expect_test::expect![[
        r#""OK ((expr (+ 1 2 3) bytecode-len 5 result 6) (expr (* 2 3 4) bytecode-len 5 result 24) (expr (- 100 30 20) bytecode-len 5 result 50) (expr (+ (* 3 4) (* 5 6)) bytecode-len 7 result 42) (expr (neg 42) bytecode-len 2 result -42) (expr (dup 7) bytecode-len 2 result 7) (expr (+ (dup 5) 3) bytecode-len 4 result 8))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Type-checking interpreter (with runtime type errors)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interp_adv_type_checking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // An interpreter that carries type information and produces typed results
    // or type-error values. Types: int, str, bool, error.
    let form = r#"(progn
  (fset 'neovm--test-typed-eval
    (lambda (expr env)
      (cond
       ;; Literal integer
       ((integerp expr)
        (cons 'int expr))
       ;; Literal string
       ((stringp expr)
        (cons 'str expr))
       ;; Boolean literals
       ((eq expr 'true) (cons 'bool t))
       ((eq expr 'false) (cons 'bool nil))
       ;; Variable reference
       ((symbolp expr)
        (let ((binding (assq expr env)))
          (if binding (cdr binding)
            (cons 'error (format "unbound: %s" (symbol-name expr))))))
       ;; Operations
       ((consp expr)
        (let ((op (car expr)))
          (cond
           ;; Arithmetic: both args must be int
           ((memq op '(add sub mul))
            (let ((a (funcall 'neovm--test-typed-eval (cadr expr) env))
                  (b (funcall 'neovm--test-typed-eval (caddr expr) env)))
              (cond
               ((eq (car a) 'error) a)
               ((eq (car b) 'error) b)
               ((and (eq (car a) 'int) (eq (car b) 'int))
                (cons 'int (cond ((eq op 'add) (+ (cdr a) (cdr b)))
                                 ((eq op 'sub) (- (cdr a) (cdr b)))
                                 ((eq op 'mul) (* (cdr a) (cdr b))))))
               (t (cons 'error
                        (format "type error: %s requires int, got %s and %s"
                                op (car a) (car b)))))))
           ;; String concatenation: both must be str
           ((eq op 'cat)
            (let ((a (funcall 'neovm--test-typed-eval (cadr expr) env))
                  (b (funcall 'neovm--test-typed-eval (caddr expr) env)))
              (cond
               ((eq (car a) 'error) a)
               ((eq (car b) 'error) b)
               ((and (eq (car a) 'str) (eq (car b) 'str))
                (cons 'str (concat (cdr a) (cdr b))))
               (t (cons 'error
                        (format "type error: cat requires str, got %s and %s"
                                (car a) (car b)))))))
           ;; Comparison: both must be same type
           ((eq op 'eq?)
            (let ((a (funcall 'neovm--test-typed-eval (cadr expr) env))
                  (b (funcall 'neovm--test-typed-eval (caddr expr) env)))
              (cond
               ((eq (car a) 'error) a)
               ((eq (car b) 'error) b)
               ((eq (car a) (car b))
                (cons 'bool (equal (cdr a) (cdr b))))
               (t (cons 'error
                        (format "type error: eq? requires same types, got %s and %s"
                                (car a) (car b)))))))
           ;; If: condition must be bool
           ((eq op 'if-then-else)
            (let ((cond-val (funcall 'neovm--test-typed-eval
                                     (cadr expr) env)))
              (cond
               ((eq (car cond-val) 'error) cond-val)
               ((eq (car cond-val) 'bool)
                (if (cdr cond-val)
                    (funcall 'neovm--test-typed-eval (caddr expr) env)
                  (funcall 'neovm--test-typed-eval (cadddr expr) env)))
               (t (cons 'error
                        (format "type error: if requires bool, got %s"
                                (car cond-val)))))))
           ;; Let binding
           ((eq op 'let-bind)
            (let* ((var (cadr expr))
                   (val (funcall 'neovm--test-typed-eval (caddr expr) env)))
              (if (eq (car val) 'error) val
                (funcall 'neovm--test-typed-eval
                         (cadddr expr)
                         (cons (cons var val) env)))))
           (t (cons 'error (format "unknown op: %s" op))))))
       (t (cons 'error "invalid expression")))))

  (unwind-protect
      (list
       ;; Valid: (add 3 4) => (int . 7)
       (funcall 'neovm--test-typed-eval '(add 3 4) nil)
       ;; Valid: (cat "hello" " world") => (str . "hello world")
       (funcall 'neovm--test-typed-eval '(cat "hello" " world") nil)
       ;; Type error: (add 3 "x")
       (funcall 'neovm--test-typed-eval '(add 3 "oops") nil)
       ;; Type error: (cat 1 2)
       (funcall 'neovm--test-typed-eval '(cat 1 2) nil)
       ;; Valid conditional
       (funcall 'neovm--test-typed-eval
                '(if-then-else true 10 20) nil)
       ;; Type error in condition
       (funcall 'neovm--test-typed-eval
                '(if-then-else 42 10 20) nil)
       ;; Let binding with type propagation
       (funcall 'neovm--test-typed-eval
                '(let-bind x 10 (add x 5)) nil)
       ;; Nested let with string ops
       (funcall 'neovm--test-typed-eval
                '(let-bind name "Alice"
                  (let-bind greeting "Hello, "
                    (cat greeting name)))
                nil)
       ;; Error propagation through nested ops
       (funcall 'neovm--test-typed-eval
                '(add (mul 2 3) (add "bad" 1)) nil))
    (fmakunbound 'neovm--test-typed-eval)))"#;
    let expect = expect_test::expect![[
        r#""OK ((int . 7) (str . \"hello world\") (error . \"type error: add requires int, got int and str\") (error . \"type error: cat requires str, got int and int\") (int . 10) (error . \"type error: if requires bool, got int\") (int . 15) (str . \"Hello, Alice\") (error . \"type error: add requires int, got str and int\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interpreter with environments (closures)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interp_adv_closures_env() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // An interpreter where lambda captures its defining environment,
    // supporting closures, higher-order functions, and currying.
    let form = r#"(progn
  (fset 'neovm--test-closure-eval
    (lambda (expr env)
      (cond
       ((numberp expr) expr)
       ((stringp expr) expr)
       ((eq expr 'nil) nil)
       ((eq expr 't) t)
       ((symbolp expr)
        (let ((b (assq expr env)))
          (if b (cdr b)
            (signal 'error (list "unbound" (symbol-name expr))))))
       ((not (consp expr))
        (signal 'error (list "invalid" expr)))
       (t
        (let ((op (car expr)))
          (cond
           ;; (fn (params...) body)
           ((eq op 'fn)
            (list 'closure (cadr expr) (caddr expr) env))
           ;; (call func arg...)
           ((eq op 'call)
            (let ((func (funcall 'neovm--test-closure-eval
                                 (cadr expr) env))
                  (args (mapcar (lambda (a)
                                  (funcall 'neovm--test-closure-eval a env))
                                (cddr expr))))
              (if (and (consp func) (eq (car func) 'closure))
                  (let* ((params (cadr func))
                         (body (caddr func))
                         (closed-env (cadddr func))
                         (new-env closed-env))
                    ;; Bind params to args
                    (let ((ps params) (as args))
                      (while ps
                        (setq new-env (cons (cons (car ps) (car as))
                                            new-env))
                        (setq ps (cdr ps) as (cdr as))))
                    (funcall 'neovm--test-closure-eval body new-env))
                (signal 'error (list "not callable" func)))))
           ;; (let ((var val)...) body)
           ((eq op 'let)
            (let ((bindings (cadr expr))
                  (body (caddr expr))
                  (new-env env))
              (dolist (b bindings)
                (let ((val (funcall 'neovm--test-closure-eval
                                    (cadr b) env)))
                  (setq new-env (cons (cons (car b) val) new-env))))
              (funcall 'neovm--test-closure-eval body new-env)))
           ;; Arithmetic
           ((eq op '+)
            (+ (funcall 'neovm--test-closure-eval (cadr expr) env)
               (funcall 'neovm--test-closure-eval (caddr expr) env)))
           ((eq op '-)
            (- (funcall 'neovm--test-closure-eval (cadr expr) env)
               (funcall 'neovm--test-closure-eval (caddr expr) env)))
           ((eq op '*)
            (* (funcall 'neovm--test-closure-eval (cadr expr) env)
               (funcall 'neovm--test-closure-eval (caddr expr) env)))
           ;; List construction
           ((eq op 'pair)
            (cons (funcall 'neovm--test-closure-eval (cadr expr) env)
                  (funcall 'neovm--test-closure-eval (caddr expr) env)))
           ((eq op 'fst)
            (car (funcall 'neovm--test-closure-eval (cadr expr) env)))
           ((eq op 'snd)
            (cdr (funcall 'neovm--test-closure-eval (cadr expr) env)))
           (t (signal 'error (list "unknown" op)))))))))

  (unwind-protect
      (list
       ;; Simple closure: make-adder
       (funcall 'neovm--test-closure-eval
                '(let ((make-adder (fn (n) (fn (x) (+ x n)))))
                   (let ((add5 (call make-adder 5)))
                     (call add5 10)))
                nil)
       ;; Currying: add as curried function
       (funcall 'neovm--test-closure-eval
                '(let ((curry-add (fn (a) (fn (b) (+ a b)))))
                   (+ (call (call curry-add 3) 4)
                      (call (call curry-add 10) 20)))
                nil)
       ;; Closure captures enclosing scope
       (funcall 'neovm--test-closure-eval
                '(let ((x 100))
                   (let ((get-x (fn () x))
                         (y 200))
                     (+ (call get-x) y)))
                nil)
       ;; Higher-order: apply-twice
       (funcall 'neovm--test-closure-eval
                '(let ((apply-twice (fn (f) (fn (x) (call f (call f x)))))
                       (double (fn (n) (* n 2))))
                   (call (call apply-twice double) 3))
                nil)
       ;; Compose two functions
       (funcall 'neovm--test-closure-eval
                '(let ((compose (fn (f g) (fn (x) (call f (call g x)))))
                       (inc (fn (x) (+ x 1)))
                       (dbl (fn (x) (* x 2))))
                   (pair (call (call compose inc dbl) 5)
                         (call (call compose dbl inc) 5)))
                nil))
    (fmakunbound 'neovm--test-closure-eval)))"#;
    let expect = expect_test::expect![[r#""OK (15 37 300 12 (11 . 12))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Tail-call optimization detection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interp_adv_tail_call_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Analyze AST nodes to detect which recursive calls are in tail position.
    // This is a static analysis pass (not actual TCO execution).
    // Nodes: (lit N), (ref X), (if COND THEN ELSE), (call F ARGS...),
    // (let ((var val)...) body), (seq E1 E2)
    let form = r#"(progn
  ;; Returns list of (call-name . is-tail) for all calls in expr
  (fset 'neovm--test-tco-analyze
    (lambda (expr tail-pos)
      (cond
       ((or (numberp expr) (stringp expr) (not (consp expr)))
        nil)
       (t
        (let ((op (car expr)))
          (cond
           ;; Literal / ref: no calls
           ((memq op '(lit ref))
            nil)
           ;; If: condition is NOT tail, then/else ARE tail if whole if is
           ((eq op 'if)
            (append
             (funcall 'neovm--test-tco-analyze (cadr expr) nil)
             (funcall 'neovm--test-tco-analyze (caddr expr) tail-pos)
             (funcall 'neovm--test-tco-analyze (cadddr expr) tail-pos)))
           ;; Seq: first expr is NOT tail, second IS tail if seq is
           ((eq op 'seq)
            (append
             (funcall 'neovm--test-tco-analyze (cadr expr) nil)
             (funcall 'neovm--test-tco-analyze (caddr expr) tail-pos)))
           ;; Let: val exprs NOT tail, body IS tail if let is
           ((eq op 'let)
            (let ((binding-results nil))
              (dolist (b (cadr expr))
                (setq binding-results
                      (append binding-results
                              (funcall 'neovm--test-tco-analyze
                                       (cadr b) nil))))
              (append binding-results
                      (funcall 'neovm--test-tco-analyze
                               (caddr expr) tail-pos))))
           ;; Call: this is a call site! Record whether it's in tail position.
           ;; Also analyze arguments (not tail position).
           ((eq op 'call)
            (let ((fn-name (cadr expr))
                  (arg-results nil))
              (dolist (arg (cddr expr))
                (setq arg-results
                      (append arg-results
                              (funcall 'neovm--test-tco-analyze arg nil))))
              (cons (cons fn-name tail-pos) arg-results)))
           (t nil)))))))

  (unwind-protect
      (list
       ;; Simple tail call: (call fact (- n 1)) in tail position
       (funcall 'neovm--test-tco-analyze
                '(if (ref n)
                     (call fact (ref n))
                     (lit 1))
                t)
       ;; Non-tail: call is wrapped in another operation
       ;; (+ 1 (call fact ...)) -- the call is an argument, not tail
       (funcall 'neovm--test-tco-analyze
                '(call + (lit 1) (call fact (ref n)))
                t)
       ;; Mixed: one tail call, one non-tail
       (funcall 'neovm--test-tco-analyze
                '(if (ref cond)
                     (call foo (ref x))
                     (seq (call bar (ref y))
                          (call baz (ref z))))
                t)
       ;; Let body in tail position
       (funcall 'neovm--test-tco-analyze
                '(let ((x (call compute (ref a))))
                   (call process (ref x)))
                t)
       ;; Nested ifs: only leaves are tail
       (funcall 'neovm--test-tco-analyze
                '(if (ref a)
                     (if (ref b)
                         (call f1 (ref x))
                         (call f2 (ref y)))
                     (call f3 (ref z)))
                t))
    (fmakunbound 'neovm--test-tco-analyze)))"#;
    let expect = expect_test::expect![[
        r#""OK (((fact . t)) ((+ . t) (fact)) ((foo . t) (bar) (baz . t)) ((compute) (process . t)) ((f1 . t) (f2 . t) (f3 . t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interpreter with continuations (CPS transform)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interp_adv_continuations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // CPS-transform simple expressions and evaluate them.
    // CPS representation: every function takes an extra continuation argument.
    // We simulate this by passing lambda continuations.
    let form = r#"(progn
  ;; CPS evaluator: evaluates expr, passes result to continuation k
  (fset 'neovm--test-cps-eval
    (lambda (expr env k)
      (cond
       ((numberp expr) (funcall k expr))
       ((symbolp expr)
        (let ((b (assq expr env)))
          (funcall k (if b (cdr b) 0))))
       ((not (consp expr)) (funcall k nil))
       (t
        (let ((op (car expr)))
          (cond
           ;; (+ a b) in CPS: eval a, then eval b, then add
           ((eq op '+)
            (funcall 'neovm--test-cps-eval (cadr expr) env
                     (lambda (av)
                       (funcall 'neovm--test-cps-eval (caddr expr) env
                                (lambda (bv) (funcall k (+ av bv)))))))
           ((eq op '-)
            (funcall 'neovm--test-cps-eval (cadr expr) env
                     (lambda (av)
                       (funcall 'neovm--test-cps-eval (caddr expr) env
                                (lambda (bv) (funcall k (- av bv)))))))
           ((eq op '*)
            (funcall 'neovm--test-cps-eval (cadr expr) env
                     (lambda (av)
                       (funcall 'neovm--test-cps-eval (caddr expr) env
                                (lambda (bv) (funcall k (* av bv)))))))
           ;; (if-zero test then else): test, branch to then/else
           ((eq op 'if-zero)
            (funcall 'neovm--test-cps-eval (cadr expr) env
                     (lambda (v)
                       (if (= v 0)
                           (funcall 'neovm--test-cps-eval
                                    (caddr expr) env k)
                         (funcall 'neovm--test-cps-eval
                                  (cadddr expr) env k)))))
           ;; (let1 var val body)
           ((eq op 'let1)
            (funcall 'neovm--test-cps-eval (caddr expr) env
                     (lambda (v)
                       (funcall 'neovm--test-cps-eval
                                (cadddr expr)
                                (cons (cons (cadr expr) v) env)
                                k))))
           ;; (seq a b): evaluate a (discard), then evaluate b
           ((eq op 'seq)
            (funcall 'neovm--test-cps-eval (cadr expr) env
                     (lambda (_)
                       (funcall 'neovm--test-cps-eval
                                (caddr expr) env k))))
           ;; (abort val): short-circuit to final result (ignore k)
           ;; This simulates callcc-like early exit
           ((eq op 'abort)
            (funcall 'neovm--test-cps-eval (cadr expr) env
                     #'identity))
           (t (funcall k nil))))))))

  ;; Helper: run CPS eval with identity continuation
  (fset 'neovm--test-cps-run
    (lambda (expr env)
      (funcall 'neovm--test-cps-eval expr env #'identity)))

  (unwind-protect
      (list
       ;; Simple arithmetic in CPS
       (funcall 'neovm--test-cps-run '(+ 3 4) nil)
       ;; Nested: (2 + 3) * (4 - 1)
       (funcall 'neovm--test-cps-run
                '(* (+ 2 3) (- 4 1)) nil)
       ;; Conditional branching
       (funcall 'neovm--test-cps-run
                '(if-zero 0 42 99) nil)
       (funcall 'neovm--test-cps-run
                '(if-zero 1 42 99) nil)
       ;; Let binding in CPS
       (funcall 'neovm--test-cps-run
                '(let1 x 10 (+ x 5)) nil)
       ;; Nested let
       (funcall 'neovm--test-cps-run
                '(let1 a 3
                   (let1 b 4
                     (+ (* a a) (* b b))))
                nil)
       ;; Seq + let
       (funcall 'neovm--test-cps-run
                '(let1 x 1
                   (seq x (+ x 10)))
                nil)
       ;; Abort: early exit ignores outer continuation
       (funcall 'neovm--test-cps-run
                '(+ 100 (abort 42)) nil))
    (fmakunbound 'neovm--test-cps-eval)
    (fmakunbound 'neovm--test-cps-run)))"#;
    let expect = expect_test::expect![[r#""OK (7 15 42 99 15 25 11 42)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
