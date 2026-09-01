//! Oracle parity tests for comprehensive macro system operations:
//! defmacro with various parameter patterns, macroexpand/macroexpand-all,
//! nested macros, macro-generating macros, backquote with , and ,@ in
//! macro bodies, gensym for hygiene, macrop predicate, complex DSL macros.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// defmacro with various parameter patterns (&rest, &optional, &body)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_macro_parameter_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test defmacro with &optional, &rest, mixed params, and destructuring.
    let form = r#"(progn
  ;; Simple no-args macro
  (defmacro neovm--mcp-const () '(+ 1 2))

  ;; Single required arg
  (defmacro neovm--mcp-double (x) `(+ ,x ,x))

  ;; Multiple required args
  (defmacro neovm--mcp-add3 (a b c) `(+ ,a (+ ,b ,c)))

  ;; &optional parameters
  (defmacro neovm--mcp-maybe-add (a &optional b)
    (if b `(+ ,a ,b) a))

  ;; &optional with default via or
  (defmacro neovm--mcp-repeat (val &optional n)
    (let ((count (or n 1)))
      `(make-list ,count ,val)))

  ;; &rest parameter
  (defmacro neovm--mcp-sum-all (&rest nums)
    `(+ ,@nums))

  ;; Mixed required and &rest
  (defmacro neovm--mcp-first-and-rest (first &rest others)
    `(cons ,first (list ,@others)))

  ;; &rest used as body (common pattern)
  (defmacro neovm--mcp-with-timer (label &rest body)
    `(let ((neovm--timer-label ,label))
       (progn ,@body)))

  (unwind-protect
      (list
       ;; No args
       (neovm--mcp-const)
       ;; Single arg
       (neovm--mcp-double 5)
       ;; Multiple args
       (neovm--mcp-add3 10 20 30)
       ;; Optional: provided
       (neovm--mcp-maybe-add 3 7)
       ;; Optional: omitted
       (neovm--mcp-maybe-add 42)
       ;; Optional with default
       (neovm--mcp-repeat 'x 3)
       (neovm--mcp-repeat 'x)
       ;; Rest: multiple args
       (neovm--mcp-sum-all 1 2 3 4 5)
       ;; Rest: single arg
       (neovm--mcp-sum-all 100)
       ;; Mixed required + rest
       (neovm--mcp-first-and-rest 'a 'b 'c 'd)
       (neovm--mcp-first-and-rest 'alone)
       ;; Body pattern
       (neovm--mcp-with-timer "test"
         (+ 1 2)
         (* 3 4)))
    (fmakunbound 'neovm--mcp-const)
    (fmakunbound 'neovm--mcp-double)
    (fmakunbound 'neovm--mcp-add3)
    (fmakunbound 'neovm--mcp-maybe-add)
    (fmakunbound 'neovm--mcp-repeat)
    (fmakunbound 'neovm--mcp-sum-all)
    (fmakunbound 'neovm--mcp-first-and-rest)
    (fmakunbound 'neovm--mcp-with-timer)))"#;
    let expect =
        expect_test::expect![[r#""OK (3 10 60 10 42 (x x x) (x) 15 100 (a b c d) (alone) 12)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// macroexpand and macroexpand-1
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_macroexpand_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test macroexpand (full), macroexpand-1 (single step),
    // and the difference between them with nested macros.
    let form = r#"(progn
  (defmacro neovm--mcp-inc (var) `(setq ,var (1+ ,var)))
  (defmacro neovm--mcp-inc2 (var) `(progn (neovm--mcp-inc ,var) (neovm--mcp-inc ,var)))
  (defmacro neovm--mcp-square (x) `(let ((neovm--tmp ,x)) (* neovm--tmp neovm--tmp)))

  (unwind-protect
      (list
       ;; macroexpand-1: one level only
       (macroexpand-1 '(neovm--mcp-inc counter))
       ;; macroexpand: fully expand (recursively for top-level)
       (macroexpand '(neovm--mcp-inc counter))
       ;; macroexpand-1 of nested macro: only outermost expanded
       (macroexpand-1 '(neovm--mcp-inc2 x))
       ;; macroexpand of nested: expands top form recursively
       ;; (but not sub-forms for macroexpand, only macroexpand-all does that)
       (macroexpand '(neovm--mcp-inc2 x))
       ;; Non-macro form: returned as-is
       (macroexpand '(+ 1 2))
       (macroexpand-1 '(if t 1 2))
       ;; Macro that expands to a let
       (macroexpand '(neovm--mcp-square 5))
       ;; Verify execution matches expansion
       (let ((counter 0))
         (neovm--mcp-inc counter)
         counter)
       (let ((counter 0))
         (neovm--mcp-inc2 counter)
         counter)
       (neovm--mcp-square 7))
    (fmakunbound 'neovm--mcp-inc)
    (fmakunbound 'neovm--mcp-inc2)
    (fmakunbound 'neovm--mcp-square)))"#;
    let expect = expect_test::expect![[
        r#""OK ((setq counter (1+ counter)) (setq counter (1+ counter)) (progn (neovm--mcp-inc x) (neovm--mcp-inc x)) (progn (neovm--mcp-inc x) (neovm--mcp-inc x)) (+ 1 2) (if t 1 2) (let ((neovm--tmp 5)) (* neovm--tmp neovm--tmp)) 1 2 49)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested macro definitions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nested_macro_definitions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Macros defined inside progn, let, and other macros.
    // Test that inner macros work correctly and can reference outer context.
    let form = r#"(progn
  ;; Macro that defines another macro and then uses it
  (defmacro neovm--mcp-define-doubler (name)
    `(defmacro ,name (x) (list '* 2 x)))

  ;; Macro that expands to a progn with defmacro + usage
  (defmacro neovm--mcp-with-alias (alias original &rest body)
    `(progn
       (defmacro ,alias (&rest args)
         (cons ',original args))
       ,@body))

  (unwind-protect
      (progn
        ;; Define and use a doubler
        (neovm--mcp-define-doubler neovm--mcp-dbl)
        (let ((r1 (neovm--mcp-dbl 21)))
          ;; Use with-alias to alias list as my-list
          (neovm--mcp-with-alias neovm--mcp-my-list list
            (let ((r2 (neovm--mcp-my-list 1 2 3))
                  (r3 (neovm--mcp-my-list 'a 'b)))
              ;; Nested definition: macro inside let
              (defmacro neovm--mcp-add-ten (x) `(+ ,x 10))
              (let ((r4 (neovm--mcp-add-ten 5)))
                (list r1 r2 r3 r4))))))
    (fmakunbound 'neovm--mcp-define-doubler)
    (fmakunbound 'neovm--mcp-dbl)
    (fmakunbound 'neovm--mcp-with-alias)
    (fmakunbound 'neovm--mcp-my-list)
    (fmakunbound 'neovm--mcp-add-ten)))"#;
    let expect = expect_test::expect![[r#""OK (42 (1 2 3) (a b) 15)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Macro generating macro definitions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_macro_generating_macros() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A meta-macro that generates families of related macros.
    let form = r#"(progn
  ;; Meta-macro: define-predicate-macro creates macros like (when-positive x body...)
  (defmacro neovm--mcp-define-guard (name predicate)
    `(defmacro ,name (val &rest body)
       (let ((tmp (make-symbol "v")))
         (list 'let (list (list tmp val))
               (list 'if (list ',predicate tmp)
                     (cons 'progn body)
                     nil)))))

  ;; Meta-macro: define-accessor generates getter macros for plist fields
  (defmacro neovm--mcp-define-accessor (name key)
    `(defmacro ,name (plist)
       (list 'plist-get plist '',key)))

  ;; Meta-macro: define-binary-op wraps an operation with a name
  (defmacro neovm--mcp-define-binop (name op)
    `(defmacro ,name (a b) (list ',op a b)))

  (unwind-protect
      (progn
        ;; Generate guard macros
        (neovm--mcp-define-guard neovm--mcp-when-positive (lambda (x) (> x 0)))
        (neovm--mcp-define-guard neovm--mcp-when-string stringp)
        (neovm--mcp-define-guard neovm--mcp-when-list listp)

        ;; Generate accessor macros
        (neovm--mcp-define-accessor neovm--mcp-get-name :name)
        (neovm--mcp-define-accessor neovm--mcp-get-age :age)

        ;; Generate binary op macros
        (neovm--mcp-define-binop neovm--mcp-add +)
        (neovm--mcp-define-binop neovm--mcp-mul *)

        (list
         ;; Guard macros
         (neovm--mcp-when-positive 5 (+ 10 20))
         (neovm--mcp-when-positive -3 (+ 10 20))
         (neovm--mcp-when-string "hello" (concat "got: " "hello"))
         (neovm--mcp-when-string 42 "should not reach")
         (neovm--mcp-when-list '(1 2) (length '(1 2)))
         ;; Accessor macros
         (let ((person '(:name "Alice" :age 30)))
           (list (neovm--mcp-get-name person)
                 (neovm--mcp-get-age person)))
         ;; Binary op macros
         (neovm--mcp-add 3 4)
         (neovm--mcp-mul 5 6)))
    (fmakunbound 'neovm--mcp-define-guard)
    (fmakunbound 'neovm--mcp-when-positive)
    (fmakunbound 'neovm--mcp-when-string)
    (fmakunbound 'neovm--mcp-when-list)
    (fmakunbound 'neovm--mcp-define-accessor)
    (fmakunbound 'neovm--mcp-get-name)
    (fmakunbound 'neovm--mcp-get-age)
    (fmakunbound 'neovm--mcp-define-binop)
    (fmakunbound 'neovm--mcp-add)
    (fmakunbound 'neovm--mcp-mul)))"#;
    let expect =
        expect_test::expect![[r#""OK (30 nil \"got: hello\" nil 2 (\"Alice\" 30) 7 30)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Backquote with , and ,@ in macro bodies
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_backquote_in_macros() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Comprehensive test of backquote features inside macro definitions:
    // unquote, splice, nested backquote, backquote in data structures.
    let form = r#"(progn
  ;; Basic unquote
  (defmacro neovm--mcp-wrap-if (cond then else)
    `(if ,cond ,then ,else))

  ;; Splice into function call
  (defmacro neovm--mcp-call-with (fn &rest args)
    `(funcall ,fn ,@args))

  ;; Splice into list literal
  (defmacro neovm--mcp-make-list-from (&rest elts)
    `(list ,@elts))

  ;; Unquote in nested list structure
  (defmacro neovm--mcp-defun-wrapper (name params body)
    `(fset ',name (lambda ,params ,body)))

  ;; Splice in the middle of a form
  (defmacro neovm--mcp-let-and-do (bindings &rest body)
    `(let ,bindings ,@body))

  ;; Backquote generating backquoted data (not code)
  (defmacro neovm--mcp-make-template (tag &rest items)
    `(list ',tag ,@(mapcar (lambda (x) `(list 'item ,x)) items)))

  ;; Conditional splice
  (defmacro neovm--mcp-optional-body (flag &rest body)
    (if flag
        `(progn ,@body)
      'nil))

  (unwind-protect
      (list
       ;; Basic unquote
       (neovm--mcp-wrap-if t 'yes 'no)
       (neovm--mcp-wrap-if nil 'yes 'no)
       ;; Splice into function call
       (neovm--mcp-call-with #'+ 1 2 3)
       (neovm--mcp-call-with #'list 'a 'b 'c)
       ;; Splice into list
       (neovm--mcp-make-list-from 1 2 3 4 5)
       (neovm--mcp-make-list-from)
       ;; Defun wrapper
       (progn
         (neovm--mcp-defun-wrapper neovm--mcp-test-fn (x y) (+ x y))
         (prog1 (funcall 'neovm--mcp-test-fn 3 4)
           (fmakunbound 'neovm--mcp-test-fn)))
       ;; Let and do
       (neovm--mcp-let-and-do ((a 10) (b 20))
         (+ a b)
         (* a b))
       ;; Template generation
       (neovm--mcp-make-template header 1 2 3)
       ;; Conditional splice
       (neovm--mcp-optional-body t (+ 1 2) (* 3 4))
       (neovm--mcp-optional-body nil (+ 1 2)))
    (fmakunbound 'neovm--mcp-wrap-if)
    (fmakunbound 'neovm--mcp-call-with)
    (fmakunbound 'neovm--mcp-make-list-from)
    (fmakunbound 'neovm--mcp-defun-wrapper)
    (fmakunbound 'neovm--mcp-let-and-do)
    (fmakunbound 'neovm--mcp-make-template)
    (fmakunbound 'neovm--mcp-optional-body)))"#;
    let expect = expect_test::expect![[
        r#""OK (yes no 6 (a b c) (1 2 3 4 5) nil 7 200 (header (item 1) (item 2) (item 3)) 12 nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Gensym patterns for macro hygiene
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_macro_gensym_hygiene() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use make-symbol (gensym) to avoid variable capture in macros.
    let form = r#"(progn
  ;; Unhygienic: would capture 'tmp' from user code
  ;; (defmacro bad-swap (a b) `(let ((tmp ,a)) (setq ,a ,b ,b tmp)))

  ;; Hygienic: uses gensym
  (defmacro neovm--mcp-swap (a b)
    (let ((tmp (make-symbol "tmp")))
      `(let ((,tmp ,a))
         (setq ,a ,b ,b ,tmp))))

  ;; Hygienic once: evaluates expr only once
  (defmacro neovm--mcp-once (var expr &rest body)
    (let ((tmp (make-symbol "once")))
      `(let ((,tmp ,expr))
         (let ((,var ,tmp))
           ,@body))))

  ;; Hygienic loop: accumulate results
  (defmacro neovm--mcp-collect-n (n expr)
    (let ((counter (make-symbol "i"))
          (result (make-symbol "res")))
      `(let ((,counter 0) (,result nil))
         (while (< ,counter ,n)
           (setq ,result (cons ,expr ,result))
           (setq ,counter (1+ ,counter)))
         (nreverse ,result))))

  ;; Test that gensyms don't capture user variables named 'tmp', 'i', 'res'
  (defmacro neovm--mcp-safe-max (a b)
    (let ((va (make-symbol "a"))
          (vb (make-symbol "b")))
      `(let ((,va ,a) (,vb ,b))
         (if (> ,va ,vb) ,va ,vb))))

  (unwind-protect
      (progn
        ;; Swap with user variable named 'tmp' (would break without gensym)
        (let ((x 10) (y 20) (tmp 999))
          (neovm--mcp-swap x y)
          (list x y tmp))

        ;; Once: side effect happens only once
        (let* ((counter 0)
               (val (neovm--mcp-once v (progn (setq counter (1+ counter)) 42)
                      (+ v v))))
          (list val counter))

        ;; Collect-n with user variable named 'i' and 'res'
        (let ((i 100) (res "user"))
          (let ((collected (neovm--mcp-collect-n 5 'item)))
            (list collected i res)))

        ;; Safe max with various inputs
        (list (neovm--mcp-safe-max 3 7)
              (neovm--mcp-safe-max 10 4)
              (neovm--mcp-safe-max 5 5)
              ;; With expression args (each evaluated once)
              (let ((c 0))
                (list (neovm--mcp-safe-max (progn (setq c (1+ c)) 3)
                                           (progn (setq c (1+ c)) 7))
                      c))))
    (fmakunbound 'neovm--mcp-swap)
    (fmakunbound 'neovm--mcp-once)
    (fmakunbound 'neovm--mcp-collect-n)
    (fmakunbound 'neovm--mcp-safe-max)))"#;
    let expect = expect_test::expect![[r#""OK (7 10 5 (7 2))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// macrop predicate
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_macrop_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test macrop for macros, non-macros, and edge cases.
    let form = r#"(progn
  (defmacro neovm--mcp-test-mac (x) x)
  (fset 'neovm--mcp-test-fn (lambda (x) x))
  (defvar neovm--mcp-test-var 42)

  (unwind-protect
      (list
       ;; User-defined macro
       (macrop 'neovm--mcp-test-mac)
       ;; Lambda function (not a macro)
       (macrop 'neovm--mcp-test-fn)
       ;; Built-in special forms
       (macrop 'if)
       (macrop 'let)
       (macrop 'progn)
       ;; Built-in macros
       (macrop 'when)
       (macrop 'unless)
       (macrop 'push)
       (macrop 'pop)
       ;; Built-in functions
       (macrop 'car)
       (macrop 'cdr)
       (macrop '+)
       ;; Unbound symbol
       (macrop 'neovm--mcp-nonexistent-sym)
       ;; Variable (not a function/macro)
       (macrop 'neovm--mcp-test-var)
       ;; nil and t
       (macrop nil)
       (macrop t))
    (fmakunbound 'neovm--mcp-test-mac)
    (fmakunbound 'neovm--mcp-test-fn)
    (makunbound 'neovm--mcp-test-var)))"#;
    let expect =
        expect_test::expect![[r#""OK (t nil nil nil nil t t t t nil nil nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex macros: building control flow, DSL-like macros
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_macro_control_flow_dsl() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a pipeline macro and a pattern-matching-like cond macro.
    let form = r#"(progn
  (require 'cl-lib)
  ;; Pipeline: (-> val (fn1 args) (fn2 args) ...) threads val through
  (defmacro neovm--mcp--> (initial &rest forms)
    (let ((result initial))
      (dolist (form forms)
        (if (listp form)
            (setq result `(,(car form) ,result ,@(cdr form)))
          (setq result `(,form ,result))))
      result))

  ;; Pattern match on type
  (defmacro neovm--mcp-typecase (expr &rest clauses)
    (let ((tmp (make-symbol "val")))
      `(let ((,tmp ,expr))
         (cond
          ,@(mapcar
             (lambda (clause)
               (let ((type (car clause))
                     (body (cdr clause)))
                 (cond
                  ((eq type 'integer) `((integerp ,tmp) ,@body))
                  ((eq type 'string)  `((stringp ,tmp) ,@body))
                  ((eq type 'symbol)  `((symbolp ,tmp) ,@body))
                  ((eq type 'list)    `((listp ,tmp) ,@body))
                  ((eq type 'null)    `((null ,tmp) ,@body))
                  ((eq type t)        `(t ,@body))
                  (t                  `(t ,@body)))))
             clauses)))))

  ;; Anaphoric if: binds 'it' to the test result
  (defmacro neovm--mcp-aif (test then &optional else)
    `(let ((it ,test))
       (if it ,then ,else)))

  ;; With-collector: provides a 'collect' function in body, returns collected list
  (defmacro neovm--mcp-with-collector (name &rest body)
    (let ((result-var (make-symbol "collected")))
      `(let ((,result-var nil))
         (cl-flet ((,name (item) (setq ,result-var (cons item ,result-var))))
           ,@body)
         (nreverse ,result-var))))

  (unwind-protect
      (list
       ;; Pipeline: thread through arithmetic
       (neovm--mcp--> 5 (+ 3) (* 2) (- 1))
       ;; Pipeline: thread through string ops
       (neovm--mcp--> "hello" upcase (concat " WORLD"))
       ;; Pipeline: thread through list ops
       (neovm--mcp--> '(3 1 4 1 5) (append '(9 2)) (sort #'<) (length))

       ;; Typecase
       (neovm--mcp-typecase 42
         (string (concat "str: " "?"))
         (integer (+ 42 1))
         (t 'unknown))
       (neovm--mcp-typecase "hello"
         (integer (* 2 2))
         (string (length "hello"))
         (t 'unknown))
       (neovm--mcp-typecase '(1 2 3)
         (integer 'int)
         (string 'str)
         (list (length '(1 2 3)))
         (t 'unknown))
       (neovm--mcp-typecase nil
         (null 'is-null)
         (list 'is-list)
         (t 'other))

       ;; Anaphoric if
       (neovm--mcp-aif (assoc 'b '((a . 1) (b . 2) (c . 3)))
                        (cdr it)
                        'not-found)
       (neovm--mcp-aif (assoc 'z '((a . 1) (b . 2)))
                        (cdr it)
                        'not-found)

       ;; With-collector
       (neovm--mcp-with-collector collect
         (dotimes (i 5)
           (collect (* i i)))))
    (fmakunbound 'neovm--mcp-->)
    (fmakunbound 'neovm--mcp-typecase)
    (fmakunbound 'neovm--mcp-aif)
    (fmakunbound 'neovm--mcp-with-collector)))"#;
    let expect = expect_test::expect![[
        r#""OK (15 \"HELLO WORLD\" 7 43 5 3 is-null 2 not-found (0 1 4 9 16))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: macro-based mini-language for data transformation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_macro_data_transform_dsl() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a mini DSL with macros for defining and composing data transforms.
    let form = r#"(progn
  ;; Define a named transform (stored as a lambda)
  (defmacro neovm--mcp-deftransform (name params &rest body)
    `(fset ',name (lambda ,params ,@body)))

  ;; Compose multiple transforms into one
  (defmacro neovm--mcp-compose-transforms (&rest transform-names)
    (let ((arg (make-symbol "val")))
      `(lambda (,arg)
         ,(let ((form arg))
            (dolist (tn transform-names)
              (setq form `(funcall ',tn ,form)))
            form))))

  ;; Apply a transform to each element of a list
  (defmacro neovm--mcp-map-transform (transform-name lst)
    `(mapcar (lambda (el) (funcall ',transform-name el)) ,lst))

  ;; Conditional transform: apply only if predicate holds
  (defmacro neovm--mcp-when-transform (pred transform-name val)
    (let ((tmp (make-symbol "v")))
      `(let ((,tmp ,val))
         (if (funcall ,pred ,tmp)
             (funcall ',transform-name ,tmp)
           ,tmp))))

  (unwind-protect
      (progn
        ;; Define transforms
        (neovm--mcp-deftransform neovm--mcp-t-double (x) (* x 2))
        (neovm--mcp-deftransform neovm--mcp-t-inc (x) (1+ x))
        (neovm--mcp-deftransform neovm--mcp-t-square (x) (* x x))
        (neovm--mcp-deftransform neovm--mcp-t-negate (x) (- x))

        (let ((compose-double-inc
               (neovm--mcp-compose-transforms neovm--mcp-t-double neovm--mcp-t-inc))
              (compose-all
               (neovm--mcp-compose-transforms neovm--mcp-t-double
                                               neovm--mcp-t-inc
                                               neovm--mcp-t-square)))
          (list
           ;; Single transforms
           (funcall 'neovm--mcp-t-double 5)
           (funcall 'neovm--mcp-t-inc 5)
           (funcall 'neovm--mcp-t-square 5)
           ;; Composed: double then inc: (5*2)+1 = 11
           (funcall compose-double-inc 5)
           ;; Composed all: ((5*2)+1)^2 = 121
           (funcall compose-all 5)
           ;; Map transform over list
           (neovm--mcp-map-transform neovm--mcp-t-square '(1 2 3 4 5))
           (neovm--mcp-map-transform neovm--mcp-t-double '(10 20 30))
           ;; Conditional transform: negate only negatives
           (neovm--mcp-when-transform
            (lambda (x) (< x 0))
            neovm--mcp-t-negate -5)
           (neovm--mcp-when-transform
            (lambda (x) (< x 0))
            neovm--mcp-t-negate 5))))
    (fmakunbound 'neovm--mcp-deftransform)
    (fmakunbound 'neovm--mcp-compose-transforms)
    (fmakunbound 'neovm--mcp-map-transform)
    (fmakunbound 'neovm--mcp-when-transform)
    (fmakunbound 'neovm--mcp-t-double)
    (fmakunbound 'neovm--mcp-t-inc)
    (fmakunbound 'neovm--mcp-t-square)
    (fmakunbound 'neovm--mcp-t-negate)))"#;
    let expect = expect_test::expect![[r#""OK (10 6 25 11 121 (1 4 9 16 25) (20 40 60) 5 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: recursive macro expansion with compile-time computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_macro_compile_time_computation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Macros that perform computation at expansion time to generate
    // optimized code (compile-time factorial, unrolled loops, lookup tables).
    let form = r#"(progn
  ;; Compile-time factorial: expands to a literal number
  (defmacro neovm--mcp-ct-factorial (n)
    (let ((result 1))
      (dotimes (i n)
        (setq result (* result (1+ i))))
      result))

  ;; Compile-time power-of-2 table: generates a vector literal
  (defmacro neovm--mcp-ct-pow2-table (size)
    (let ((entries nil))
      (dotimes (i size)
        (setq entries (cons (expt 2 i) entries)))
      `(quote ,(apply #'vector (nreverse entries)))))

  ;; Unroll a loop at compile time
  (defmacro neovm--mcp-unroll (var from to &rest body)
    (let ((forms nil))
      (let ((i from))
        (while (<= i to)
          (setq forms (cons `(let ((,var ,i)) ,@body) forms))
          (setq i (1+ i))))
      `(progn ,@(nreverse forms))))

  ;; Compile-time string repetition
  (defmacro neovm--mcp-ct-repeat-string (s n)
    (let ((result ""))
      (dotimes (_ n)
        (setq result (concat result s)))
      result))

  (unwind-protect
      (list
       ;; Compile-time factorial
       (neovm--mcp-ct-factorial 0)
       (neovm--mcp-ct-factorial 1)
       (neovm--mcp-ct-factorial 5)
       (neovm--mcp-ct-factorial 10)
       ;; Power-of-2 lookup table
       (neovm--mcp-ct-pow2-table 8)
       (aref (neovm--mcp-ct-pow2-table 16) 10)
       ;; Unrolled loop: sum of squares 0..4
       (let ((sum 0))
         (neovm--mcp-unroll i 0 4
           (setq sum (+ sum (* i i))))
         sum)
       ;; Unrolled loop: collect values
       (let ((result nil))
         (neovm--mcp-unroll i 1 5
           (setq result (cons (* i 10) result)))
         (nreverse result))
       ;; Compile-time string repetition
       (neovm--mcp-ct-repeat-string "ab" 4)
       (length (neovm--mcp-ct-repeat-string "x" 10)))
    (fmakunbound 'neovm--mcp-ct-factorial)
    (fmakunbound 'neovm--mcp-ct-pow2-table)
    (fmakunbound 'neovm--mcp-unroll)
    (fmakunbound 'neovm--mcp-ct-repeat-string)))"#;
    let expect = expect_test::expect![[
        r#""OK (1 1 120 3628800 [1 2 4 8 16 32 64 128] 1024 30 (10 20 30 40 50) \"abababab\" 10)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
