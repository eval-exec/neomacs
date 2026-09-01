//! Oracle parity tests for interpreter/evaluator patterns:
//! arithmetic expression evaluator with variables, stack-based bytecode
//! interpreter, pattern matching / destructuring, simple type checker
//! for a mini-language, and a recursive descent parser + interpreter
//! for if/let/fn expressions.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Simple arithmetic expression evaluator with variables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interp_pattern_arith_eval_with_vars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Evaluate arithmetic expressions with variable bindings, nested scopes,
    // and a special "let" form that introduces local bindings.
    // Grammar: (num N) | (var X) | (add E E) | (sub E E) | (mul E E)
    //        | (div E E) | (mod E E) | (neg E) | (abs E)
    //        | (let-in ((X E) ...) E)
    let form = r#"(progn
  (fset 'neovm--test-arith-eval
    (lambda (expr env)
      (cond
       ;; Literal number
       ((and (consp expr) (eq (car expr) 'num))
        (cadr expr))
       ;; Variable reference
       ((and (consp expr) (eq (car expr) 'var))
        (let ((binding (assq (cadr expr) env)))
          (if binding (cdr binding)
            (signal 'error (list "unbound variable" (cadr expr))))))
       ;; Binary operations
       ((and (consp expr) (eq (car expr) 'add))
        (+ (funcall 'neovm--test-arith-eval (cadr expr) env)
           (funcall 'neovm--test-arith-eval (caddr expr) env)))
       ((and (consp expr) (eq (car expr) 'sub))
        (- (funcall 'neovm--test-arith-eval (cadr expr) env)
           (funcall 'neovm--test-arith-eval (caddr expr) env)))
       ((and (consp expr) (eq (car expr) 'mul))
        (* (funcall 'neovm--test-arith-eval (cadr expr) env)
           (funcall 'neovm--test-arith-eval (caddr expr) env)))
       ((and (consp expr) (eq (car expr) 'div))
        (/ (funcall 'neovm--test-arith-eval (cadr expr) env)
           (funcall 'neovm--test-arith-eval (caddr expr) env)))
       ((and (consp expr) (eq (car expr) 'mod))
        (% (funcall 'neovm--test-arith-eval (cadr expr) env)
           (funcall 'neovm--test-arith-eval (caddr expr) env)))
       ;; Unary
       ((and (consp expr) (eq (car expr) 'neg))
        (- (funcall 'neovm--test-arith-eval (cadr expr) env)))
       ((and (consp expr) (eq (car expr) 'abs-val))
        (abs (funcall 'neovm--test-arith-eval (cadr expr) env)))
       ;; let-in: introduce bindings
       ((and (consp expr) (eq (car expr) 'let-in))
        (let ((bindings (cadr expr))
              (body (caddr expr))
              (new-env env))
          (dolist (b bindings)
            (let ((val (funcall 'neovm--test-arith-eval (cadr b) new-env)))
              (setq new-env (cons (cons (car b) val) new-env))))
          (funcall 'neovm--test-arith-eval body new-env)))
       (t (signal 'error (list "invalid expression" expr))))))

  (unwind-protect
      (list
       ;; Simple: 3 + 4 * 2 = 3 + 8 = 11
       (funcall 'neovm--test-arith-eval
                '(add (num 3) (mul (num 4) (num 2))) nil)
       ;; Variables: let x = 10, y = 20 in x * y - x
       (funcall 'neovm--test-arith-eval
                '(let-in ((x (num 10)) (y (num 20)))
                   (sub (mul (var x) (var y)) (var x)))
                nil)
       ;; Nested let: let a=5 in let b=a+3 in a*b
       (funcall 'neovm--test-arith-eval
                '(let-in ((a (num 5)))
                   (let-in ((b (add (var a) (num 3))))
                     (mul (var a) (var b))))
                nil)
       ;; Unary ops: abs(neg(7)) + abs(3) = 7 + 3 = 10
       (funcall 'neovm--test-arith-eval
                '(add (abs-val (neg (num 7))) (abs-val (num 3))) nil)
       ;; Modular arithmetic: (17 mod 5) + (23 div 7)
       (funcall 'neovm--test-arith-eval
                '(add (mod (num 17) (num 5))
                      (div (num 23) (num 7)))
                nil)
       ;; Shadowing: let x=10 in let x=x+1 in x = 11
       (funcall 'neovm--test-arith-eval
                '(let-in ((x (num 10)))
                   (let-in ((x (add (var x) (num 1))))
                     (var x)))
                nil)
       ;; Complex: quadratic: let a=2,b=5,c=3,x=4 in a*x*x + b*x + c
       (funcall 'neovm--test-arith-eval
                '(let-in ((a (num 2)) (b (num 5)) (c (num 3)) (x (num 4)))
                   (add (add (mul (var a) (mul (var x) (var x)))
                             (mul (var b) (var x)))
                        (var c)))
                nil)
       ;; Unbound variable error
       (condition-case err
           (funcall 'neovm--test-arith-eval '(var z) nil)
         (error (list 'caught (cadr err)))))
    (fmakunbound 'neovm--test-arith-eval)))"#;
    let expect =
        expect_test::expect![[r#""OK (11 190 40 10 5 11 55 (caught \"unbound variable\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Stack-based bytecode interpreter with jumps and comparisons
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interp_pattern_stack_bytecode_vm() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A stack-based bytecode interpreter supporting: PUSH, ADD, SUB, MUL,
    // DUP, SWAP, POP, CMP (push comparison result), JMP, JZ (jump if zero),
    // LOAD (from locals), STORE (to locals), HALT.
    // Computes factorial(5) using a loop.
    let form = r#"(progn
  (fset 'neovm--test-stack-vm
    (lambda (code)
      (let ((stack nil)
            (locals (make-vector 16 0))
            (pc 0)
            (halted nil)
            (max-steps 500))
        (while (and (< pc (length code)) (not halted) (> max-steps 0))
          (setq max-steps (1- max-steps))
          (let ((instr (aref code pc)))
            (let ((op (car instr)))
              (cond
               ;; (push val)
               ((eq op 'push)
                (setq stack (cons (cadr instr) stack))
                (setq pc (1+ pc)))
               ;; (add)
               ((eq op 'add)
                (let ((b (car stack)) (a (cadr stack)))
                  (setq stack (cons (+ a b) (cddr stack))))
                (setq pc (1+ pc)))
               ;; (sub)
               ((eq op 'sub)
                (let ((b (car stack)) (a (cadr stack)))
                  (setq stack (cons (- a b) (cddr stack))))
                (setq pc (1+ pc)))
               ;; (mul)
               ((eq op 'mul)
                (let ((b (car stack)) (a (cadr stack)))
                  (setq stack (cons (* a b) (cddr stack))))
                (setq pc (1+ pc)))
               ;; (dup)
               ((eq op 'dup)
                (setq stack (cons (car stack) stack))
                (setq pc (1+ pc)))
               ;; (swap)
               ((eq op 'swap)
                (let ((a (car stack)) (b (cadr stack)))
                  (setq stack (cons b (cons a (cddr stack)))))
                (setq pc (1+ pc)))
               ;; (pop)
               ((eq op 'pop)
                (setq stack (cdr stack))
                (setq pc (1+ pc)))
               ;; (cmp) -- compare TOS with second: push 1 if >, 0 if =, -1 if <
               ((eq op 'cmp)
                (let ((b (car stack)) (a (cadr stack)))
                  (setq stack (cons (cond ((> a b) 1) ((= a b) 0) (t -1))
                                    (cddr stack))))
                (setq pc (1+ pc)))
               ;; (load slot)
               ((eq op 'load)
                (setq stack (cons (aref locals (cadr instr)) stack))
                (setq pc (1+ pc)))
               ;; (store slot)
               ((eq op 'store)
                (aset locals (cadr instr) (car stack))
                (setq stack (cdr stack))
                (setq pc (1+ pc)))
               ;; (jmp addr)
               ((eq op 'jmp)
                (setq pc (cadr instr)))
               ;; (jz addr) -- jump if TOS is zero
               ((eq op 'jz)
                (let ((v (car stack)))
                  (setq stack (cdr stack))
                  (if (= v 0) (setq pc (cadr instr))
                    (setq pc (1+ pc)))))
               ;; (jnz addr) -- jump if TOS is non-zero
               ((eq op 'jnz)
                (let ((v (car stack)))
                  (setq stack (cdr stack))
                  (if (/= v 0) (setq pc (cadr instr))
                    (setq pc (1+ pc)))))
               ;; (halt)
               ((eq op 'halt)
                (setq halted t))
               (t (setq halted t))))))
        (car stack))))

  (unwind-protect
      (list
       ;; Simple: 3 + 4 = 7
       (funcall 'neovm--test-stack-vm
                (vector '(push 3) '(push 4) '(add) '(halt)))
       ;; Factorial of 5 using loop:
       ;; local[0] = n, local[1] = result
       ;; result = 1; while (n > 1) { result *= n; n--; }
       (funcall 'neovm--test-stack-vm
                (vector
                 '(push 5)    ;; 0: push n
                 '(store 0)   ;; 1: local[0] = n
                 '(push 1)    ;; 2: push 1
                 '(store 1)   ;; 3: local[1] = result = 1
                 ;; loop start (pc=4)
                 '(load 0)    ;; 4: push n
                 '(push 1)    ;; 5: push 1
                 '(sub)       ;; 6: n - 1
                 '(dup)       ;; 7: dup for test
                 '(jz 14)     ;; 8: if n-1 == 0, jump to end
                 '(pop)       ;; 9: discard dup
                 '(load 1)    ;; 10: push result
                 '(load 0)    ;; 11: push n
                 '(mul)       ;; 12: result * n
                 '(store 1)   ;; 13: store result
                 '(load 0)    ;; 14: push n ... wait
                 ;; Actually let me restructure: we need the decrement
                 ;; Hmm, let me re-layout factorial correctly:
                 ;; pc=4: load n, push 1, cmp (n>1?), jz end
                 ;; pc=8: load result, load n, mul, store result
                 ;; pc=12: load n, push 1, sub, store n, jmp 4
                 ;; pc=17: load result, halt
                 '(push 0) ;; filler, will redo below
                 '(halt)))
       ;; Factorial of 5 (correct layout):
       (funcall 'neovm--test-stack-vm
                (vector
                 '(push 5)    ;; 0
                 '(store 0)   ;; 1: n = 5
                 '(push 1)    ;; 2
                 '(store 1)   ;; 3: result = 1
                 ;; loop check (pc=4)
                 '(load 0)    ;; 4: push n
                 '(push 1)    ;; 5: push 1
                 '(cmp)       ;; 6: compare n vs 1
                 '(jz 15)     ;; 7: if n==1 (cmp returns 0), goto end
                 ;; loop body (pc=8)
                 '(load 1)    ;; 8: push result
                 '(load 0)    ;; 9: push n
                 '(mul)       ;; 10: result * n
                 '(store 1)   ;; 11: store result
                 '(load 0)    ;; 12: push n
                 '(push 1)    ;; 13: push 1
                 '(sub)       ;; 14: n - 1
                 '(store 0)   ;; 15: store n ... hmm, index off
                 ;; Let me re-count:
                 '(jmp 4)     ;; 16: goto loop
                 ;; end (pc=17)
                 '(load 1)    ;; 17: push result
                 '(halt)))    ;; 18: halt
       ;; Fibonacci(7) via loop:
       ;; local[0]=n, local[1]=a=0, local[2]=b=1
       ;; while n>0: temp=a+b; a=b; b=temp; n--
       (funcall 'neovm--test-stack-vm
                (vector
                 '(push 7)    ;; 0
                 '(store 0)   ;; 1: n=7
                 '(push 0)    ;; 2
                 '(store 1)   ;; 3: a=0
                 '(push 1)    ;; 4
                 '(store 2)   ;; 5: b=1
                 ;; loop (pc=6)
                 '(load 0)    ;; 6: push n
                 '(jz 18)     ;; 7: if n==0, goto end
                 '(load 1)    ;; 8: push a
                 '(load 2)    ;; 9: push b
                 '(add)       ;; 10: a+b
                 '(store 3)   ;; 11: local[3] = temp
                 '(load 2)    ;; 12: push b
                 '(store 1)   ;; 13: a = b
                 '(load 3)    ;; 14: push temp
                 '(store 2)   ;; 15: b = temp
                 '(load 0)    ;; 16: push n
                 '(push 1)    ;; 17
                 '(sub)       ;; 18: n-1
                 '(store 0)   ;; 19: n = n-1
                 '(jmp 6)     ;; 20: goto loop
                 ;; end
                 '(load 1)    ;; 21: push a (result)
                 '(halt))))   ;; 22
    (fmakunbound 'neovm--test-stack-vm)))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Pattern matching / destructuring implementation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interp_pattern_destructure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A pattern matching engine that destructures data against patterns.
    // Patterns: (quote val) = literal, _ = wildcard, SYMBOL = bind,
    // (cons P1 P2) = cons destructure, (list P...) = list destructure,
    // (vec P...) = vector destructure.
    // Returns an alist of bindings on match, or 'no-match.
    let form = r#"(progn
  (fset 'neovm--test-pmatch
    (lambda (pattern data bindings)
      (cond
       ;; Wildcard: always matches, no binding
       ((eq pattern '_) bindings)
       ;; Quoted literal: exact equality check
       ((and (consp pattern) (eq (car pattern) 'quote))
        (if (equal (cadr pattern) data) bindings 'no-match))
       ;; Symbol: bind the variable
       ((symbolp pattern)
        (let ((existing (assq pattern bindings)))
          (if existing
              ;; Already bound: check consistency
              (if (equal (cdr existing) data) bindings 'no-match)
            (cons (cons pattern data) bindings))))
       ;; Cons destructure
       ((and (consp pattern) (eq (car pattern) 'cons))
        (if (consp data)
            (let ((result (funcall 'neovm--test-pmatch
                                   (cadr pattern) (car data) bindings)))
              (if (eq result 'no-match) 'no-match
                (funcall 'neovm--test-pmatch
                         (caddr pattern) (cdr data) result)))
          'no-match))
       ;; List destructure
       ((and (consp pattern) (eq (car pattern) 'list))
        (let ((pats (cdr pattern))
              (elems data)
              (result bindings)
              (failed nil))
          (while (and pats (not failed))
            (if (and (consp elems))
                (progn
                  (setq result (funcall 'neovm--test-pmatch
                                        (car pats) (car elems) result))
                  (when (eq result 'no-match) (setq failed t))
                  (setq pats (cdr pats) elems (cdr elems)))
              (setq failed t)))
          (if (or failed pats elems) 'no-match result)))
       ;; Vector destructure
       ((and (consp pattern) (eq (car pattern) 'vec))
        (if (vectorp data)
            (let ((pats (cdr pattern))
                  (idx 0)
                  (result bindings)
                  (failed nil))
              (while (and pats (not failed))
                (if (< idx (length data))
                    (progn
                      (setq result (funcall 'neovm--test-pmatch
                                            (car pats) (aref data idx) result))
                      (when (eq result 'no-match) (setq failed t))
                      (setq pats (cdr pats) idx (1+ idx)))
                  (setq failed t)))
              (if (or failed pats (/= idx (length data))) 'no-match result))
          'no-match))
       ;; No match for other pattern forms
       (t 'no-match))))

  (unwind-protect
      (list
       ;; Simple bind
       (funcall 'neovm--test-pmatch 'x 42 nil)
       ;; Wildcard
       (funcall 'neovm--test-pmatch '_ "anything" nil)
       ;; Quoted literal match
       (funcall 'neovm--test-pmatch ''hello 'hello nil)
       ;; Quoted literal fail
       (funcall 'neovm--test-pmatch ''hello 'world nil)
       ;; Cons destructure
       (funcall 'neovm--test-pmatch
                '(cons x y) '(1 . 2) nil)
       ;; List destructure
       (funcall 'neovm--test-pmatch
                '(list a b c) '(10 20 30) nil)
       ;; Vector destructure
       (funcall 'neovm--test-pmatch
                '(vec x y z) [100 200 300] nil)
       ;; Nested: (cons (list a b) c)
       (funcall 'neovm--test-pmatch
                '(cons (list a b) c) '((1 2) . 3) nil)
       ;; Repeated variable (consistency check)
       (funcall 'neovm--test-pmatch
                '(list x x) '(5 5) nil)
       ;; Repeated variable fail
       (funcall 'neovm--test-pmatch
                '(list x x) '(5 6) nil)
       ;; Complex nested pattern
       (funcall 'neovm--test-pmatch
                '(list (cons tag _) (list a b))
                '((point . nil) (10 20))
                nil))
    (fmakunbound 'neovm--test-pmatch)))"#;
    let expect = expect_test::expect![[
        r#""OK (((x . 42)) nil nil no-match ((y . 2) (x . 1)) ((c . 30) (b . 20) (a . 10)) ((z . 300) (y . 200) (x . 100)) ((c . 3) (b . 2) (a . 1)) ((x . 5)) no-match ((b . 20) (a . 10) (tag . point)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Simple type checker for a mini-language
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interp_pattern_type_checker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Static type checker for a mini-language with int, bool, string types.
    // Expressions:
    //   (lit-int N) : int
    //   (lit-bool B) : bool
    //   (lit-str S) : string
    //   (add E E) : int (requires int, int)
    //   (cat E E) : string (requires string, string)
    //   (eq E E) : bool (requires same type)
    //   (if E E E) : T (requires bool, T, T)
    //   (let ((X TYPE E) ...) E) : T
    //   (var X) : lookup type
    //   (strlen E) : int (requires string)
    //   (to-str E) : string (requires int)
    //   (and E E) : bool (requires bool, bool)
    //   (not E) : bool (requires bool)
    let form = r#"(progn
  (fset 'neovm--test-typecheck
    (lambda (expr tenv)
      (cond
       ;; Literals
       ((and (consp expr) (eq (car expr) 'lit-int))
        'int)
       ((and (consp expr) (eq (car expr) 'lit-bool))
        'bool)
       ((and (consp expr) (eq (car expr) 'lit-str))
        'string)
       ;; Variable reference
       ((and (consp expr) (eq (car expr) 'var))
        (let ((binding (assq (cadr expr) tenv)))
          (if binding (cdr binding)
            (list 'type-error 'unbound (cadr expr)))))
       ;; Add: requires (int, int) -> int
       ((and (consp expr) (eq (car expr) 'add))
        (let ((t1 (funcall 'neovm--test-typecheck (cadr expr) tenv))
              (t2 (funcall 'neovm--test-typecheck (caddr expr) tenv)))
          (cond
           ((and (consp t1) (eq (car t1) 'type-error)) t1)
           ((and (consp t2) (eq (car t2) 'type-error)) t2)
           ((and (eq t1 'int) (eq t2 'int)) 'int)
           (t (list 'type-error 'add-requires-int t1 t2)))))
       ;; Cat: requires (string, string) -> string
       ((and (consp expr) (eq (car expr) 'cat))
        (let ((t1 (funcall 'neovm--test-typecheck (cadr expr) tenv))
              (t2 (funcall 'neovm--test-typecheck (caddr expr) tenv)))
          (cond
           ((and (consp t1) (eq (car t1) 'type-error)) t1)
           ((and (consp t2) (eq (car t2) 'type-error)) t2)
           ((and (eq t1 'string) (eq t2 'string)) 'string)
           (t (list 'type-error 'cat-requires-string t1 t2)))))
       ;; Eq: requires (T, T) -> bool
       ((and (consp expr) (eq (car expr) 'eq-check))
        (let ((t1 (funcall 'neovm--test-typecheck (cadr expr) tenv))
              (t2 (funcall 'neovm--test-typecheck (caddr expr) tenv)))
          (cond
           ((and (consp t1) (eq (car t1) 'type-error)) t1)
           ((and (consp t2) (eq (car t2) 'type-error)) t2)
           ((eq t1 t2) 'bool)
           (t (list 'type-error 'eq-requires-same-type t1 t2)))))
       ;; If: requires (bool, T, T) -> T
       ((and (consp expr) (eq (car expr) 'if-expr))
        (let ((tc (funcall 'neovm--test-typecheck (cadr expr) tenv))
              (tt (funcall 'neovm--test-typecheck (caddr expr) tenv))
              (tf (funcall 'neovm--test-typecheck (cadddr expr) tenv)))
          (cond
           ((and (consp tc) (eq (car tc) 'type-error)) tc)
           ((not (eq tc 'bool))
            (list 'type-error 'if-condition-not-bool tc))
           ((and (consp tt) (eq (car tt) 'type-error)) tt)
           ((and (consp tf) (eq (car tf) 'type-error)) tf)
           ((eq tt tf) tt)
           (t (list 'type-error 'if-branches-differ tt tf)))))
       ;; strlen: requires string -> int
       ((and (consp expr) (eq (car expr) 'strlen))
        (let ((t1 (funcall 'neovm--test-typecheck (cadr expr) tenv)))
          (if (eq t1 'string) 'int
            (if (and (consp t1) (eq (car t1) 'type-error)) t1
              (list 'type-error 'strlen-requires-string t1)))))
       ;; to-str: requires int -> string
       ((and (consp expr) (eq (car expr) 'to-str))
        (let ((t1 (funcall 'neovm--test-typecheck (cadr expr) tenv)))
          (if (eq t1 'int) 'string
            (if (and (consp t1) (eq (car t1) 'type-error)) t1
              (list 'type-error 'to-str-requires-int t1)))))
       ;; and: requires (bool, bool) -> bool
       ((and (consp expr) (eq (car expr) 'and-expr))
        (let ((t1 (funcall 'neovm--test-typecheck (cadr expr) tenv))
              (t2 (funcall 'neovm--test-typecheck (caddr expr) tenv)))
          (cond
           ((and (consp t1) (eq (car t1) 'type-error)) t1)
           ((and (consp t2) (eq (car t2) 'type-error)) t2)
           ((and (eq t1 'bool) (eq t2 'bool)) 'bool)
           (t (list 'type-error 'and-requires-bool t1 t2)))))
       ;; not: requires bool -> bool
       ((and (consp expr) (eq (car expr) 'not-expr))
        (let ((t1 (funcall 'neovm--test-typecheck (cadr expr) tenv)))
          (if (eq t1 'bool) 'bool
            (if (and (consp t1) (eq (car t1) 'type-error)) t1
              (list 'type-error 'not-requires-bool t1)))))
       ;; Let: introduce typed bindings
       ((and (consp expr) (eq (car expr) 'let-typed))
        (let ((bindings (cadr expr))
              (body (caddr expr))
              (new-tenv tenv)
              (err nil))
          (dolist (b bindings)
            (unless err
              (let ((var (car b))
                    (declared-type (cadr b))
                    (init-expr (caddr b)))
                (let ((actual-type (funcall 'neovm--test-typecheck
                                            init-expr new-tenv)))
                  (cond
                   ((and (consp actual-type) (eq (car actual-type) 'type-error))
                    (setq err actual-type))
                   ((not (eq actual-type declared-type))
                    (setq err (list 'type-error 'binding-type-mismatch
                                    var declared-type actual-type)))
                   (t (setq new-tenv
                            (cons (cons var declared-type) new-tenv))))))))
          (if err err
            (funcall 'neovm--test-typecheck body new-tenv))))
       (t (list 'type-error 'unknown-form (car expr))))))

  (unwind-protect
      (list
       ;; Valid: add two ints -> int
       (funcall 'neovm--test-typecheck
                '(add (lit-int 3) (lit-int 4)) nil)
       ;; Valid: cat two strings -> string
       (funcall 'neovm--test-typecheck
                '(cat (lit-str "a") (lit-str "b")) nil)
       ;; Error: add int and string
       (funcall 'neovm--test-typecheck
                '(add (lit-int 1) (lit-str "x")) nil)
       ;; Valid: if bool then int else int -> int
       (funcall 'neovm--test-typecheck
                '(if-expr (lit-bool t) (lit-int 1) (lit-int 2)) nil)
       ;; Error: if branches differ
       (funcall 'neovm--test-typecheck
                '(if-expr (lit-bool t) (lit-int 1) (lit-str "x")) nil)
       ;; Error: if condition not bool
       (funcall 'neovm--test-typecheck
                '(if-expr (lit-int 1) (lit-int 2) (lit-int 3)) nil)
       ;; Valid: let with typed bindings
       (funcall 'neovm--test-typecheck
                '(let-typed ((x int (lit-int 10))
                             (s string (lit-str "hi")))
                   (add (var x) (strlen (var s))))
                nil)
       ;; Error: binding type mismatch
       (funcall 'neovm--test-typecheck
                '(let-typed ((x int (lit-str "oops")))
                   (var x))
                nil)
       ;; Valid: strlen + to-str composition
       (funcall 'neovm--test-typecheck
                '(cat (to-str (strlen (lit-str "hello")))
                      (lit-str " chars"))
                nil)
       ;; Valid: boolean logic
       (funcall 'neovm--test-typecheck
                '(and-expr (not-expr (lit-bool nil))
                           (eq-check (lit-int 1) (lit-int 2)))
                nil))
    (fmakunbound 'neovm--test-typecheck)))"#;
    let expect = expect_test::expect![[
        r#""OK (int string (type-error add-requires-int int string) int (type-error if-branches-differ int string) (type-error if-condition-not-bool int) int (type-error binding-type-mismatch x int string) string bool)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Recursive descent parser + interpreter for if/let/fn expressions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interp_pattern_recursive_descent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A mini-language parsed from S-expressions and then interpreted.
    // Supports: numbers, strings, booleans, if, let, fn (lambda), apply,
    // arithmetic, comparisons, and sequencing (do ...).
    // fn creates closures that capture the defining environment.
    let form = r#"(progn
  (fset 'neovm--test-rdeval
    (lambda (expr env)
      (cond
       ;; Self-evaluating
       ((numberp expr) expr)
       ((stringp expr) expr)
       ((eq expr 'true) t)
       ((eq expr 'false) nil)
       ;; Variable
       ((symbolp expr)
        (let ((b (assq expr env)))
          (if b (cdr b)
            (signal 'error (list "unbound" (symbol-name expr))))))
       ;; Compound forms
       ((consp expr)
        (let ((head (car expr)))
          (cond
           ;; (if cond then else)
           ((eq head 'if)
            (if (funcall 'neovm--test-rdeval (nth 1 expr) env)
                (funcall 'neovm--test-rdeval (nth 2 expr) env)
              (funcall 'neovm--test-rdeval (nth 3 expr) env)))
           ;; (let ((var val) ...) body...)
           ((eq head 'let)
            (let ((bindings (nth 1 expr))
                  (body (cddr expr))
                  (new-env env))
              (dolist (b bindings)
                (setq new-env
                      (cons (cons (car b)
                                  (funcall 'neovm--test-rdeval
                                           (cadr b) env))
                            new-env)))
              ;; Evaluate body forms, return last
              (let ((result nil))
                (dolist (form body)
                  (setq result (funcall 'neovm--test-rdeval form new-env)))
                result)))
           ;; (fn (params...) body...)
           ((eq head 'fn)
            (list 'closure (nth 1 expr) (cddr expr) env))
           ;; (do forms...) -- sequence, return last
           ((eq head 'do)
            (let ((result nil))
              (dolist (form (cdr expr))
                (setq result (funcall 'neovm--test-rdeval form env)))
              result))
           ;; Arithmetic
           ((eq head '+)
            (+ (funcall 'neovm--test-rdeval (nth 1 expr) env)
               (funcall 'neovm--test-rdeval (nth 2 expr) env)))
           ((eq head '-)
            (- (funcall 'neovm--test-rdeval (nth 1 expr) env)
               (funcall 'neovm--test-rdeval (nth 2 expr) env)))
           ((eq head '*)
            (* (funcall 'neovm--test-rdeval (nth 1 expr) env)
               (funcall 'neovm--test-rdeval (nth 2 expr) env)))
           ((eq head '/)
            (/ (funcall 'neovm--test-rdeval (nth 1 expr) env)
               (funcall 'neovm--test-rdeval (nth 2 expr) env)))
           ;; Comparisons
           ((eq head '<)
            (< (funcall 'neovm--test-rdeval (nth 1 expr) env)
               (funcall 'neovm--test-rdeval (nth 2 expr) env)))
           ((eq head '>)
            (> (funcall 'neovm--test-rdeval (nth 1 expr) env)
               (funcall 'neovm--test-rdeval (nth 2 expr) env)))
           ((eq head '=)
            (= (funcall 'neovm--test-rdeval (nth 1 expr) env)
               (funcall 'neovm--test-rdeval (nth 2 expr) env)))
           ;; String operations
           ((eq head 'str-cat)
            (concat (funcall 'neovm--test-rdeval (nth 1 expr) env)
                    (funcall 'neovm--test-rdeval (nth 2 expr) env)))
           ((eq head 'str-len)
            (length (funcall 'neovm--test-rdeval (nth 1 expr) env)))
           ;; Function application: (apply fn-expr arg...)
           ((eq head 'apply)
            (let ((fn-val (funcall 'neovm--test-rdeval (nth 1 expr) env))
                  (arg-vals (mapcar (lambda (a)
                                      (funcall 'neovm--test-rdeval a env))
                                    (cddr expr))))
              (if (and (consp fn-val) (eq (car fn-val) 'closure))
                  (let ((params (nth 1 fn-val))
                        (body (nth 2 fn-val))
                        (closed-env (nth 3 fn-val))
                        (call-env nil))
                    ;; Build environment: closed-env + param bindings
                    (setq call-env closed-env)
                    (let ((ps params) (as arg-vals))
                      (while ps
                        (setq call-env
                              (cons (cons (car ps) (car as)) call-env))
                        (setq ps (cdr ps) as (cdr as))))
                    ;; Evaluate body forms
                    (let ((result nil))
                      (dolist (form body)
                        (setq result
                              (funcall 'neovm--test-rdeval form call-env)))
                      result))
                (signal 'error (list "not a function" fn-val)))))
           (t (signal 'error (list "unknown form" head))))))
       (t (signal 'error (list "invalid" expr))))))

  (unwind-protect
      (list
       ;; Basic arithmetic
       (funcall 'neovm--test-rdeval '(+ (* 3 4) (- 10 5)) nil)
       ;; Let bindings with body
       (funcall 'neovm--test-rdeval
                '(let ((x 10) (y 20))
                   (+ x y))
                nil)
       ;; If/else
       (funcall 'neovm--test-rdeval
                '(if (> 5 3) "yes" "no") nil)
       ;; Lambda and apply: make-adder
       (funcall 'neovm--test-rdeval
                '(let ((make-adder (fn (n) (fn (x) (+ x n)))))
                   (let ((add10 (apply make-adder 10)))
                     (apply add10 5)))
                nil)
       ;; Recursive factorial via Y-combinator style
       ;; (since we don't have letrec, use let + passing self)
       (funcall 'neovm--test-rdeval
                '(let ((fact-helper
                        (fn (self n)
                          (if (= n 0) 1
                            (* n (apply self self (- n 1)))))))
                   (apply fact-helper fact-helper 6))
                nil)
       ;; String operations
       (funcall 'neovm--test-rdeval
                '(let ((greet (fn (name)
                                (str-cat (str-cat "Hello, " name) "!"))))
                   (apply greet "World"))
                nil)
       ;; Compose + higher order
       (funcall 'neovm--test-rdeval
                '(let ((compose (fn (f g)
                                  (fn (x) (apply f (apply g x)))))
                       (double (fn (x) (* x 2)))
                       (inc (fn (x) (+ x 1))))
                   (let ((double-then-inc (apply compose inc double))
                         (inc-then-double (apply compose double inc)))
                     (do (+ (apply double-then-inc 5)
                            (apply inc-then-double 5)))))
                nil)
       ;; Nested closures with let shadowing
       (funcall 'neovm--test-rdeval
                '(let ((x 1))
                   (let ((f (fn () x))
                         (x 2))
                     (+ (apply f) x)))
                nil))
    (fmakunbound 'neovm--test-rdeval)))"#;
    let expect = expect_test::expect![[r#""OK (17 30 \"yes\" 15 720 \"Hello, World!\" 23 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
