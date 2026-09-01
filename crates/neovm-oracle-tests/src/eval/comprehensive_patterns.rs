//! Comprehensive oracle parity tests for `eval`:
//! eval with LEXICAL parameter (nil, t, alist), self-evaluating forms,
//! symbol evaluation, function calls, special forms, quoted forms,
//! lexical environment alists, nested eval, eval with constructed forms.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// eval with LEXICAL parameter: nil, t, alist
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval_comp_lexical_parameter_modes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; eval with LEXICAL=nil (dynamic binding): lambda cannot capture let-bound vars
  (condition-case err
      (let ((f (eval '(let ((x 10)) (lambda () x)) nil)))
        (funcall f))
    (void-variable 'dynamic-failed))
  ;; eval with LEXICAL=t (lexical binding): lambda captures let-bound vars
  (let ((f (eval '(let ((x 10)) (lambda () x)) t)))
    (funcall f))
  ;; eval with LEXICAL as alist: provides bindings directly
  (eval 'x '((x . 42) (y . 99)))
  (eval '(+ x y) '((x . 10) (y . 20)))
  ;; Alist with nil value
  (eval 'x '((x . nil)))
  ;; Alist with implicit nil (bare symbol)
  (eval 'x '((x)))
  ;; Alist shadows outer dynamic binding
  (let ((x 100))
    (eval 'x '((x . 200))))
  ;; Alist: first binding wins with duplicates
  (eval 'x '((x . first) (x . second)))
  ;; Alist enables closure capture
  (let ((f (eval '(lambda () (+ x y)) '((x . 3) (y . 7)))))
    (funcall f))
  ;; Alist with let inside eval that shadows alist binding
  (eval '(let ((x 999)) x) '((x . 1))))"#;
    let expect =
        expect_test::expect![[r#""OK (dynamic-failed 10 42 30 nil nil 200 first 10 999)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Self-evaluating forms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval_comp_self_evaluating_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Integers
  (eval 0)
  (eval 42)
  (eval -1)
  (eval most-positive-fixnum)
  (eval most-negative-fixnum)
  ;; Floats
  (eval 0.0)
  (eval 3.14)
  (eval -2.718)
  (eval 1.5e10)
  ;; Strings
  (eval "")
  (eval "hello world")
  (eval "line\nbreak")
  ;; Vectors (self-evaluating, elements also evaluated)
  (eval [1 2 3])
  (eval [])
  (eval ["a" "b" "c"])
  ;; Keywords are self-evaluating
  (eval :foo)
  (eval :bar-baz)
  ;; t and nil
  (eval t)
  (eval nil)
  ;; Characters
  (eval ?A)
  (eval ?\n)
  ;; Nested self-evaluating: vector containing only self-eval forms
  (eval [1 "two" :three]))"#;
    let expect = expect_test::expect![[
        r#""OK (0 42 -1 0 0 0.0 3.14 -2.718 15000000000.0 \"\" \"hello world\" \"line\\nbreak\" [1 2 3] [] [\"a\" \"b\" \"c\"] :foo :bar-baz t nil 65 10 [1 \"two\" :three])""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Evaluating symbols: bound and unbound
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval_comp_symbol_evaluation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Bound symbol via let
  (let ((x 42)) (eval 'x))
  ;; Multiple bound symbols
  (let ((a 1) (b 2) (c 3)) (eval '(list a b c)))
  ;; Unbound symbol signals void-variable
  (condition-case err
      (eval 'neovm--unbound-test-symbol-xyz)
    (void-variable (list 'caught (cadr err))))
  ;; defvar-bound symbol
  (progn
    (defvar neovm--eval-test-dv-77 77)
    (unwind-protect
        (eval 'neovm--eval-test-dv-77)
      (makunbound 'neovm--eval-test-dv-77)))
  ;; setq then eval
  (let ((z nil))
    (setq z 123)
    (eval 'z))
  ;; Symbol in lexical env alist
  (eval 'my-var '((my-var . "lexical-value")))
  ;; Constant symbols: t, nil, keywords
  (list (eval 't) (eval 'nil) (eval ':test-kw)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable x)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Evaluating function calls
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval_comp_function_calls() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Built-in functions
  (eval '(+ 1 2 3 4 5))
  (eval '(* 2 3 4))
  (eval '(concat "hello" " " "world"))
  (eval '(length '(a b c d e)))
  (eval '(car '(x y z)))
  (eval '(cdr '(x y z)))
  (eval '(cons 'head '(tail)))
  (eval '(list 1 2 3 4 5))
  (eval '(append '(1 2) '(3 4) '(5)))
  ;; Nested function calls
  (eval '(+ (* 2 3) (* 4 5)))
  (eval '(car (cdr (cdr '(a b c d e)))))
  (eval '(length (mapcar '1+ '(1 2 3 4 5))))
  ;; Lambda call
  (eval '((lambda (x y) (+ x y)) 10 20))
  ;; funcall inside eval
  (eval '(funcall (lambda (n) (* n n)) 7))
  ;; apply inside eval
  (eval '(apply '+ '(1 2 3 4 5))))"#;
    let expect = expect_test::expect![[
        r#""OK (15 24 \"hello world\" 5 x (y z) (head tail) (1 2 3 4 5) (1 2 3 4 5) 26 c 5 30 49 15)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_eval_dotted_function_form_checks_raw_argument_list_first() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (condition-case err
     (eval '(list 1 . tail))
   (error (list (car err) (cdr err))))
 (condition-case err
     (eval '(quote value . tail))
   (error (list (car err) (cdr err))))
 (condition-case err
     (eval '((lambda (x) x) 1 . tail))
   (error (list (car err) (cdr err)))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument (listp tail)) (wrong-type-argument (listp tail)) (wrong-type-argument (listp tail)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Evaluating special forms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval_comp_special_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; if
  (eval '(if t 'yes 'no))
  (eval '(if nil 'yes 'no))
  (eval '(if (> 3 2) (+ 10 20) (+ 30 40)))
  ;; let
  (eval '(let ((x 10) (y 20)) (+ x y)))
  (eval '(let* ((x 5) (y (* x 2))) (+ x y)))
  ;; progn
  (eval '(progn 1 2 3 4 5))
  (eval '(progn (+ 1 2) (+ 3 4)))
  ;; cond
  (eval '(cond ((= 1 2) 'a) ((= 2 2) 'b) (t 'c)))
  (eval '(cond (nil 'never) (nil 'also-never) (t 'default)))
  ;; when / unless
  (eval '(when t 'yes))
  (eval '(when nil 'yes))
  (eval '(unless nil 'ran))
  (eval '(unless t 'ran))
  ;; or / and
  (eval '(or nil nil 42 99))
  (eval '(and 1 2 3 4 5))
  (eval '(and 1 nil 3))
  ;; setq inside let
  (eval '(let ((x 0)) (setq x 42) x))
  ;; condition-case
  (eval '(condition-case err
             (/ 1 0)
           (arith-error 'caught-division)))
  ;; unwind-protect
  (eval '(let ((result nil))
           (unwind-protect
               (progn (setq result 'body) result)
             (setq result 'cleanup))
           result)))"#;
    let expect = expect_test::expect![[
        r#""OK (yes no 30 30 15 5 7 b default yes nil ran nil 42 5 nil 42 caught-division cleanup)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Evaluating quoted forms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval_comp_quoted_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; quote returns unevaluated
  (eval '(quote hello))
  (eval '(quote (1 2 3)))
  (eval '(quote (+ 1 2)))
  ;; Nested quote
  (eval '(quote (quote x)))
  ;; quote in various contexts
  (eval '(list (quote a) (quote b) (quote c)))
  (eval '(car (quote (x y z))))
  ;; Double eval with quote
  (eval (eval '(quote (quote (+ 1 2)))))
  ;; function quote
  (let ((f (eval '(function (lambda (x) (* x x))))))
    (funcall f 6))
  ;; Backquote inside eval
  (let ((val 42))
    (eval (list 'list (list 'quote 'result) val)))
  ;; Quoted special form name is just a symbol
  (eval '(quote if))
  (eval '(quote let))
  (eval '(quote progn)))"#;
    let expect = expect_test::expect![[
        r#""OK (hello (1 2 3) (+ 1 2) 'x (a b c) x (+ 1 2) 36 (result 42) if let progn)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested eval calls
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval_comp_nested_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Double eval: outer evaluates result of inner
  (eval (eval '(quote (+ 1 2))))
  ;; Triple eval
  (eval (eval (eval '(quote (quote (+ 10 20))))))
  ;; eval inside eval with different lexical modes
  (let ((f (eval '(eval '(let ((x 7)) (lambda () x)) t) nil)))
    (funcall f))
  ;; Nested eval with alist environments
  (eval '(eval 'x '((x . 99))) '((x . 1)))
  ;; eval of eval form
  (eval '(eval '(+ 3 4)))
  ;; eval producing a form that gets evaled again
  (eval (eval '(list '+ 10 20)))
  ;; Nested eval with side effects
  (let ((counter 0))
    (eval '(setq counter (1+ counter)))
    (eval '(setq counter (1+ counter)))
    (eval '(setq counter (1+ counter)))
    counter)
  ;; eval with constructed nested form
  (eval (list 'eval (list 'quote (list '+ 100 200)))))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable counter)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// eval with constructed forms (code generation)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval_comp_constructed_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Build and eval arithmetic expression
  (let ((op '+) (args '(1 2 3 4 5)))
    (eval (cons op args)))
  ;; Build a let form dynamically
  (let ((bindings '((a 10) (b 20) (c 30)))
        (body '(+ a b c)))
    (eval (list 'let bindings body)))
  ;; Build a cond form
  (let ((clauses (list (list '(= 1 2) ''branch-a)
                       (list '(= 2 2) ''branch-b)
                       (list t ''default))))
    (eval (cons 'cond clauses)))
  ;; Build a lambda and call it
  (let ((params '(x y))
        (body '(* x y)))
    (funcall (eval (list 'lambda params body)) 6 7))
  ;; Code generation: build a chain of operations
  (let ((ops '(1+ 1+ 1+ 1+ 1+))
        (start 0))
    (let ((form start))
      (dolist (op ops)
        (setq form (list op form)))
      (eval form)))
  ;; Generate a progn with multiple setq
  (let ((assignments '((x . 10) (y . 20) (z . 30))))
    (eval
     (let ((body nil))
       (dolist (pair (reverse assignments))
         (setq body (cons (list 'setq (car pair) (cdr pair)) body)))
       ;; Final form: (let ((x nil) (y nil) (z nil)) (setq x 10) (setq y 20) (setq z 30) (list x y z))
       (list 'let
             (mapcar (lambda (p) (list (car p) nil)) assignments)
             (cons 'progn (append body (list '(list x y z))))))))
  ;; Build if-else chain from a decision table
  (let ((table '((1 . "one") (2 . "two") (3 . "three")))
        (input 2))
    (let ((form ''unknown))
      (dolist (entry (reverse table))
        (setq form (list 'if (list '= 'n (car entry))
                         (cdr entry) form)))
      (eval (list 'let (list (list 'n input)) form))))
  ;; eval of a dynamically constructed recursive-like form using cl-labels
  ;; (simpler: just chain lets)
  (eval '(let* ((a 1) (b (+ a 1)) (c (+ b 1)) (d (+ c 1)) (e (+ d 1)))
           (list a b c d e))))"#;
    let expect =
        expect_test::expect![[r#""OK (15 60 branch-b 42 5 (10 20 30) \"two\" (1 2 3 4 5))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// eval error handling and edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval_comp_errors_and_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Wrong number of arguments to eval
  (condition-case err (eval) (wrong-number-of-arguments 'caught-no-args))
  ;; eval of invalid form
  (condition-case err (eval '(1 2 3)) (invalid-function 'caught-invalid))
  ;; eval of unbound variable
  (condition-case err
      (eval 'neovm--never-bound-xyz-abc)
    (void-variable (list 'void (cadr err))))
  ;; eval with wrong-type lexenv
  (condition-case err
      (eval 'x '(x . 1))
    (wrong-type-argument 'caught-bad-lexenv))
  ;; eval of car on non-list
  (condition-case err (eval '(car 42)) (wrong-type-argument 'caught-car))
  ;; eval preserves error from nested eval
  (condition-case err
      (eval '(eval '(/ 1 0)))
    (arith-error 'caught-nested-div-zero))
  ;; eval of empty progn
  (eval '(progn))
  ;; eval of single-element progn
  (eval '(progn 42))
  ;; eval nil
  (eval nil)
  ;; eval t
  (eval t))"#;
    let expect = expect_test::expect![[
        r#""OK (caught-no-args caught-invalid (void neovm--never-bound-xyz-abc) caught-bad-lexenv caught-car caught-nested-div-zero nil 42 nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
