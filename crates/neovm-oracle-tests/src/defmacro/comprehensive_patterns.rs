//! Comprehensive oracle parity tests for `defmacro` patterns:
//! &rest, &body, &optional, backquote expansion, nested macros,
//! macros expanding to other macro calls, recursive macros with termination,
//! macros with gensym for hygiene, macroexpand vs macroexpand-all,
//! macros generating defun, destructuring, declare forms, compiler macros.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Macros with &rest and &body (treated identically, but test both)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defmacro_comp_rest_vs_body() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // &rest and &body are identical in Elisp defmacro; test both forms
    // with multiple body expressions and verify evaluation order.
    let form = r#"(progn
  (defmacro neovm--dcp-with-rest (tag &rest body)
    `(let ((neovm--dcp-log nil))
       (setq neovm--dcp-log (cons ,tag neovm--dcp-log))
       ,@body
       (nreverse neovm--dcp-log)))
  (defmacro neovm--dcp-with-body (tag &body body)
    `(let ((neovm--dcp-log nil))
       (setq neovm--dcp-log (cons ,tag neovm--dcp-log))
       ,@body
       (nreverse neovm--dcp-log)))
  (unwind-protect
      (list
        ;; Single body expression
        (neovm--dcp-with-rest :start
          (setq neovm--dcp-log (cons :step1 neovm--dcp-log)))
        ;; Multiple body expressions
        (neovm--dcp-with-body :begin
          (setq neovm--dcp-log (cons :a neovm--dcp-log))
          (setq neovm--dcp-log (cons :b neovm--dcp-log))
          (setq neovm--dcp-log (cons :c neovm--dcp-log)))
        ;; Empty body
        (neovm--dcp-with-rest :empty)
        ;; Nested rest usage
        (neovm--dcp-with-rest :outer
          (let ((inner-result
                 (neovm--dcp-with-body :inner
                   (setq neovm--dcp-log (cons :deep neovm--dcp-log)))))
            (setq neovm--dcp-log (cons inner-result neovm--dcp-log))))
    (fmakunbound 'neovm--dcp-with-rest)
    (fmakunbound 'neovm--dcp-with-body)))"#;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Macros with &optional and default values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defmacro_comp_optional_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Macro with multiple &optional params, some supplied, some defaulted.
    let form = r#"(progn
  (defmacro neovm--dcp-make-alist (key val &optional default-key default-val)
    `(list (cons ,(or key ''unnamed) ,(or val 0))
           (cons ,(or default-key ''default) ,(or default-val ''none))))
  (defmacro neovm--dcp-wrap-call (fn &optional (arg1 0) (arg2 1))
    `(funcall ,fn ,arg1 ,arg2))
  (unwind-protect
      (list
        ;; All params supplied
        (neovm--dcp-make-alist 'name "hello" 'alt "world")
        ;; Only required params
        (neovm--dcp-make-alist 'key 42)
        ;; wrap-call with defaults
        (neovm--dcp-wrap-call #'+ )
        ;; wrap-call with one override
        (neovm--dcp-wrap-call #'+ 10)
        ;; wrap-call with both overrides
        (neovm--dcp-wrap-call #'* 6 7))
    (fmakunbound 'neovm--dcp-make-alist)
    (fmakunbound 'neovm--dcp-wrap-call)))"#;
    let expect = expect_test::expect![[
        r#""ERR (invalid-function (closure (t) (fn &optional (arg1 0) (arg2 1)) `(funcall ,fn ,arg1 ,arg2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Backquote expansion: nested backquotes and splicing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defmacro_comp_backquote_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Complex backquote patterns: nested backquotes, ,@ splicing,
    // conditional inclusion, and computed unquotes.
    let form = r#"(progn
  ;; Macro that builds a cond form from pairs
  (defmacro neovm--dcp-build-cond (&rest clauses)
    `(cond ,@(mapcar (lambda (clause)
                       `(,(car clause) ,(cadr clause)))
                     clauses)))
  ;; Macro that splices expressions around a core
  (defmacro neovm--dcp-sandwich (before-exprs core after-exprs)
    `(progn ,@before-exprs ,core ,@after-exprs))
  ;; Macro with computed structure
  (defmacro neovm--dcp-make-let (bindings &rest body)
    `(let ,(mapcar (lambda (b)
                     (if (consp b) b (list b nil)))
                   bindings)
       ,@body))
  (unwind-protect
      (list
        ;; Build-cond
        (let ((x 2))
          (neovm--dcp-build-cond
            ((= x 1) "one")
            ((= x 2) "two")
            ((= x 3) "three")
            (t "other")))
        ;; Sandwich
        (let ((log nil))
          (neovm--dcp-sandwich
            ((setq log (cons :before log)))
            (setq log (cons :core log))
            ((setq log (cons :after log))))
          (nreverse log))
        ;; make-let with mixed bindings
        (neovm--dcp-make-let (a (b 10) c (d "hello"))
          (list a b c d)))
    (fmakunbound 'neovm--dcp-build-cond)
    (fmakunbound 'neovm--dcp-sandwich)
    (fmakunbound 'neovm--dcp-make-let)))"#;
    let expect =
        expect_test::expect![[r#""OK (\"two\" (:before :core :after) (nil 10 nil \"hello\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested macro definitions: macro that defines another macro
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defmacro_comp_nested_definitions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A macro-defining macro: creates a family of accessor macros.
    let form = r#"(progn
  ;; Macro that defines a getter macro for a plist key
  (defmacro neovm--dcp-def-getter (name key)
    `(defmacro ,name (plist)
       `(plist-get ,plist ',',key)))
  ;; Use it to define getters
  (neovm--dcp-def-getter neovm--dcp-get-name name)
  (neovm--dcp-def-getter neovm--dcp-get-age age)
  (neovm--dcp-def-getter neovm--dcp-get-role role)
  (unwind-protect
      (let ((person '(name "Alice" age 30 role :admin)))
        (list
          (neovm--dcp-get-name person)
          (neovm--dcp-get-age person)
          (neovm--dcp-get-role person)
          ;; Verify macroexpand of the generated macro
          (macroexpand '(neovm--dcp-get-name some-plist))))
    (fmakunbound 'neovm--dcp-def-getter)
    (fmakunbound 'neovm--dcp-get-name)
    (fmakunbound 'neovm--dcp-get-age)
    (fmakunbound 'neovm--dcp-get-role)))"#;
    let expect =
        expect_test::expect![[r#""OK (\"Alice\" 30 :admin (plist-get some-plist 'name))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Macros expanding to other macro calls (chain expansion)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defmacro_comp_chain_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Macro A expands to a call to macro B, which expands to a call to
    // macro C. Test that the full expansion chain works.
    let form = r#"(progn
  (defmacro neovm--dcp-c (x)
    `(* ,x ,x))
  (defmacro neovm--dcp-b (x)
    `(+ (neovm--dcp-c ,x) 1))
  (defmacro neovm--dcp-a (x)
    `(list (neovm--dcp-b ,x) (neovm--dcp-b (+ ,x 1))))
  (unwind-protect
      (list
        (neovm--dcp-a 3)     ; (list (+ 9 1) (+ 16 1)) = (10 17)
        (neovm--dcp-a 0)     ; (list (+ 0 1) (+ 1 1)) = (1 2)
        (neovm--dcp-a -2)    ; (list (+ 4 1) (+ 1 1)) = (5 2)
        ;; macroexpand-1 on A only expands A, not B or C
        (car (macroexpand-1 '(neovm--dcp-a 5))))
    (fmakunbound 'neovm--dcp-a)
    (fmakunbound 'neovm--dcp-b)
    (fmakunbound 'neovm--dcp-c)))"#;
    let expect = expect_test::expect![[r#""OK ((10 17) (1 2) (5 2) list)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Recursive macros with termination via compile-time recursion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defmacro_comp_recursive_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Recursive macro that builds a nested let chain from a list of pairs.
    // Terminates when the list is empty.
    let form = r#"(progn
  (defmacro neovm--dcp-let-chain (bindings &rest body)
    (if (null bindings)
        `(progn ,@body)
      (let ((first (car bindings))
            (rest (cdr bindings)))
        `(let ((,(car first) ,(cadr first)))
           (neovm--dcp-let-chain ,rest ,@body)))))
  (unwind-protect
      (list
        ;; Single binding
        (neovm--dcp-let-chain ((x 10)) x)
        ;; Multiple bindings
        (neovm--dcp-let-chain ((a 1) (b 2) (c 3))
          (+ a b c))
        ;; Empty bindings
        (neovm--dcp-let-chain () 42)
        ;; Bindings that reference earlier ones
        (neovm--dcp-let-chain ((x 5) (y (* x 2)) (z (+ x y)))
          (list x y z)))
    (fmakunbound 'neovm--dcp-let-chain)))"#;
    let expect = expect_test::expect![[r#""OK (10 6 42 (5 10 15))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Macros with gensym/make-symbol for hygiene
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defmacro_comp_gensym_hygiene() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Macros that use make-symbol to prevent variable capture.
    // Test that expansion doesn't interfere with user variables.
    let form = r#"(progn
  (require 'cl-lib)
  ;; once-only: evaluate expression exactly once
  (defmacro neovm--dcp-once-only (var expr &rest body)
    (let ((temp (make-symbol "once")))
      `(let ((,temp ,expr))
         (cl-symbol-macrolet ((,var ,temp))
           ,@body))))
  ;; safe-incf: no double evaluation
  (defmacro neovm--dcp-safe-incf (place &optional delta)
    (let ((tmp (make-symbol "v"))
          (d (make-symbol "d")))
      `(let* ((,tmp ,place)
              (,d ,(or delta 1)))
         (setq ,place (+ ,tmp ,d))
         ,place)))
  (unwind-protect
      (list
        ;; once-only ensures single evaluation
        (let ((counter 0))
          (neovm--dcp-once-only val (progn (setq counter (1+ counter)) 42)
            (list val val val counter)))
        ;; safe-incf
        (let ((x 10))
          (list (neovm--dcp-safe-incf x)
                (neovm--dcp-safe-incf x 5)
                x))
        ;; Verify no variable capture: user var named "once" shouldn't clash
        (let ((once 999))
          (neovm--dcp-once-only val (+ 1 2)
            (list val once))))
    (fmakunbound 'neovm--dcp-once-only)
    (fmakunbound 'neovm--dcp-safe-incf)))"#;
    let expect = expect_test::expect![[r#""OK ((42 42 42 1) (11 16 16) (3 999))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// macroexpand vs macroexpand-1: partial vs full expansion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defmacro_comp_expand_vs_expand1() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test the difference between macroexpand-1 and macroexpand.
    let form = r#"(progn
  (defmacro neovm--dcp-outer (x)
    `(neovm--dcp-inner (* ,x 2)))
  (defmacro neovm--dcp-inner (x)
    `(+ ,x 100))
  (unwind-protect
      (let ((expr '(neovm--dcp-outer 5)))
        (list
          ;; macroexpand-1: only one step
          (macroexpand-1 expr)
          ;; macroexpand: full expansion (iterates until no more macros at top)
          (macroexpand expr)
          ;; Verify evaluation gives same result as full expansion
          (eval (macroexpand expr))
          ;; Non-macro form: macroexpand returns it unchanged
          (macroexpand '(+ 1 2))
          ;; Already expanded: macroexpand-1 returns unchanged
          (equal (macroexpand-1 '(+ 1 2)) '(+ 1 2))))
    (fmakunbound 'neovm--dcp-outer)
    (fmakunbound 'neovm--dcp-inner)))"#;
    let expect = expect_test::expect![[
        r#""OK ((neovm--dcp-inner (* 5 2)) (+ (* 5 2) 100) 110 (+ 1 2) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Macros that generate defun definitions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defmacro_comp_generate_defun() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A macro that generates multiple defun forms from a spec.
    let form = r#"(progn
  ;; Define an arithmetic operator function
  (defmacro neovm--dcp-def-arith (name op identity)
    `(defun ,name (&rest args)
       (let ((result ,identity))
         (dolist (a args)
           (setq result (,op result a)))
         result)))
  (neovm--dcp-def-arith neovm--dcp-sum + 0)
  (neovm--dcp-def-arith neovm--dcp-prod * 1)
  (neovm--dcp-def-arith neovm--dcp-max-val max most-negative-fixnum)
  (unwind-protect
      (list
        (neovm--dcp-sum 1 2 3 4 5)
        (neovm--dcp-prod 2 3 4)
        (neovm--dcp-max-val 3 1 4 1 5 9 2 6)
        ;; No args: identity
        (neovm--dcp-sum)
        (neovm--dcp-prod)
        ;; Single arg
        (neovm--dcp-sum 42)
        ;; Verify they are actual functions
        (functionp #'neovm--dcp-sum)
        (functionp #'neovm--dcp-prod))
    (fmakunbound 'neovm--dcp-def-arith)
    (fmakunbound 'neovm--dcp-sum)
    (fmakunbound 'neovm--dcp-prod)
    (fmakunbound 'neovm--dcp-max-val)))"#;
    let expect = expect_test::expect![[r#""OK (15 24 9 0 1 42 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Macros with destructuring in parameter list
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defmacro_comp_destructuring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Elisp defmacro supports destructuring in its arglist.
    let form = r#"(progn
  ;; Destructuring: (x y) extracts from a list argument form
  (defmacro neovm--dcp-with-pair ((a b) &rest body)
    `(let ((,a (car '(,a ,b)))  ; just use the names directly
           (,b (cadr '(,a ,b))))
       ;; Actually bind to runtime values
       ,@body))

  ;; Simpler: macro that takes two explicit args and builds a pair
  (defmacro neovm--dcp-bind-pair (var pair-expr &rest body)
    (let ((tmp (make-symbol "pair")))
      `(let* ((,tmp ,pair-expr)
              (,(car var) (car ,tmp))
              (,(cadr var) (cdr ,tmp)))
         ,@body)))

  (unwind-protect
      (list
        ;; bind-pair: destructure a cons cell
        (neovm--dcp-bind-pair (x y) '(10 . 20)
          (+ x y))
        ;; bind-pair: with complex expression
        (neovm--dcp-bind-pair (head tail) (cons "hello" "world")
          (concat head " " tail))
        ;; Nested bind-pair
        (neovm--dcp-bind-pair (a b) '(1 . 2)
          (neovm--dcp-bind-pair (c d) '(3 . 4)
            (list a b c d (+ a b c d)))))
    (fmakunbound 'neovm--dcp-with-pair)
    (fmakunbound 'neovm--dcp-bind-pair)))"#;
    let expect = expect_test::expect![[r#""OK (30 \"hello world\" (1 2 3 4 10))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Macro that implements a mini pattern-matching DSL
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defmacro_comp_pattern_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Pattern matching: match a value against literal patterns.
    let form = r#"(progn
  (defmacro neovm--dcp-match (expr &rest clauses)
    (let ((val (make-symbol "val")))
      `(let ((,val ,expr))
         (cond
           ,@(mapcar
              (lambda (clause)
                (let ((pat (car clause))
                      (body (cdr clause)))
                  (cond
                   ((eq pat '_)
                    `(t ,@body))
                   ((and (consp pat) (eq (car pat) 'quote))
                    `((equal ,val ,pat) ,@body))
                   ((symbolp pat)
                    `(t (let ((,pat ,val)) ,@body)))
                   (t
                    `((equal ,val ',pat) ,@body)))))
              clauses)))))
  (unwind-protect
      (list
        ;; Match literal symbols
        (neovm--dcp-match 'foo
          ('foo "found foo")
          ('bar "found bar")
          (_ "unknown"))
        ;; Match numbers
        (neovm--dcp-match 42
          (0 "zero")
          (1 "one")
          (42 "the answer")
          (_ "other"))
        ;; Bind to variable (wildcard with name)
        (neovm--dcp-match '(1 2 3)
          ('nil "empty")
          (xs (length xs)))
        ;; Default case with _
        (neovm--dcp-match "hello"
          ('nil "nil")
          (_ "catchall")))
    (fmakunbound 'neovm--dcp-match)))"#;
    let expect = expect_test::expect![[r#""OK (\"found foo\" \"the answer\" 3 \"catchall\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Anaphoric macros (aif, awhen, awhile)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defmacro_comp_anaphoric() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Anaphoric macros bind the test result to `it`.
    let form = r#"(progn
  (defmacro neovm--dcp-aif (test then &optional else)
    `(let ((it ,test))
       (if it ,then ,else)))
  (defmacro neovm--dcp-awhen (test &rest body)
    `(let ((it ,test))
       (when it ,@body)))
  (defmacro neovm--dcp-aand (&rest forms)
    (cond
     ((null forms) t)
     ((null (cdr forms)) (car forms))
     (t `(let ((it ,(car forms)))
           (when it (neovm--dcp-aand ,@(cdr forms)))))))
  (unwind-protect
      (list
        ;; aif: truthy
        (neovm--dcp-aif (assoc 'b '((a 1) (b 2) (c 3)))
          (cadr it)
          :not-found)
        ;; aif: falsy
        (neovm--dcp-aif (assoc 'z '((a 1) (b 2)))
          (cadr it)
          :not-found)
        ;; awhen: truthy
        (neovm--dcp-awhen (member 3 '(1 2 3 4 5))
          (length it))
        ;; awhen: falsy returns nil
        (neovm--dcp-awhen (member 9 '(1 2 3))
          (length it))
        ;; aand: all truthy
        (neovm--dcp-aand 1 2 3)
        ;; aand: short-circuit on nil
        (neovm--dcp-aand 1 nil 3)
        ;; aand: empty
        (neovm--dcp-aand))
    (fmakunbound 'neovm--dcp-aif)
    (fmakunbound 'neovm--dcp-awhen)
    (fmakunbound 'neovm--dcp-aand)))"#;
    let expect = expect_test::expect![[r#""OK (2 :not-found 3 nil 3 nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Macro-generating looping constructs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defmacro_comp_loop_constructs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Macros for for-range, while-let, do-while.
    let form = r#"(progn
  ;; for-range: iterate var from start below end
  (defmacro neovm--dcp-for-range (var start end &rest body)
    (let ((limit (make-symbol "limit")))
      `(let ((,var ,start)
             (,limit ,end))
         (while (< ,var ,limit)
           ,@body
           (setq ,var (1+ ,var))))))
  ;; do-while: execute body then check condition
  (defmacro neovm--dcp-do-while (test &rest body)
    (let ((continue (make-symbol "cont")))
      `(let ((,continue t))
         (while ,continue
           ,@body
           (setq ,continue ,test)))))
  (unwind-protect
      (list
        ;; for-range collecting
        (let ((result nil))
          (neovm--dcp-for-range i 0 5
            (setq result (cons (* i i) result)))
          (nreverse result))
        ;; for-range with empty range
        (let ((count 0))
          (neovm--dcp-for-range i 5 5
            (setq count (1+ count)))
          count)
        ;; do-while: at least one iteration
        (let ((x 0) (log nil))
          (neovm--dcp-do-while (< x 3)
            (setq log (cons x log))
            (setq x (1+ x)))
          (nreverse log))
        ;; do-while: condition false from start, still executes once
        (let ((executed nil))
          (neovm--dcp-do-while nil
            (setq executed t))
          executed))
    (fmakunbound 'neovm--dcp-for-range)
    (fmakunbound 'neovm--dcp-do-while)))"#;
    let expect = expect_test::expect![[r#""OK ((0 1 4 9 16) 0 (0 1 2) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// declare forms in macros (doc strings, indent, debug)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defmacro_comp_declare_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test that declare forms don't affect macro behavior,
    // and that doc strings are preserved.
    let form = r#"(progn
  (defmacro neovm--dcp-with-doc (x)
    "Double the value X. This is a documented macro."
    (declare (indent 1) (debug t))
    `(+ ,x ,x))
  (defmacro neovm--dcp-with-debug (&rest body)
    "Execute BODY forms in sequence."
    (declare (indent 0) (debug (&rest form)))
    `(progn ,@body))
  (unwind-protect
      (list
        ;; Macro works correctly despite declare
        (neovm--dcp-with-doc 21)
        ;; Body macro works
        (neovm--dcp-with-debug (+ 1 2) (* 3 4))
        ;; Documentation is accessible
        (stringp (documentation 'neovm--dcp-with-doc))
        ;; macroexpand ignores declare
        (macroexpand '(neovm--dcp-with-doc 5)))
    (fmakunbound 'neovm--dcp-with-doc)
    (fmakunbound 'neovm--dcp-with-debug)))"#;
    let expect = expect_test::expect![[r#""OK (42 12 t (+ 5 5))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Macro-based struct/record definition
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defmacro_comp_struct_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A macro that defines a constructor and accessors for a simple record
    // implemented as a plist.
    let form = r#"(progn
  (defmacro neovm--dcp-defrecord (name &rest fields)
    (let ((constructor (intern (format "neovm--dcp-make-%s" name)))
          (predicate (intern (format "neovm--dcp-%s-p" name))))
      `(progn
         ;; Constructor
         (defun ,constructor (&rest args)
           (let ((record (list :type ',name))
                 (field-names ',fields)
                 (vals args))
             (while (and field-names vals)
               (setq record (plist-put record (car field-names) (car vals)))
               (setq field-names (cdr field-names))
               (setq vals (cdr vals)))
             record))
         ;; Predicate
         (defun ,predicate (obj)
           (and (listp obj) (eq (plist-get obj :type) ',name)))
         ;; Return name for verification
         ',name)))
  (neovm--dcp-defrecord point :x :y)
  (neovm--dcp-defrecord person :name :age :email)
  (unwind-protect
      (let ((p1 (neovm--dcp-make-point 3 4))
            (p2 (neovm--dcp-make-person "Alice" 30 "alice@example.com")))
        (list
          ;; Access fields
          (plist-get p1 :x)
          (plist-get p1 :y)
          (plist-get p2 :name)
          (plist-get p2 :age)
          ;; Predicates
          (neovm--dcp-point-p p1)
          (neovm--dcp-person-p p2)
          (neovm--dcp-point-p p2)
          (neovm--dcp-person-p p1)
          ;; Type tag
          (plist-get p1 :type)
          (plist-get p2 :type)))
    (fmakunbound 'neovm--dcp-defrecord)
    (fmakunbound 'neovm--dcp-make-point)
    (fmakunbound 'neovm--dcp-point-p)
    (fmakunbound 'neovm--dcp-make-person)
    (fmakunbound 'neovm--dcp-person-p)))"#;
    let expect = expect_test::expect![[r#""OK (3 4 \"Alice\" 30 t t nil nil point person)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// defmacro defined via (load ...) of a lexical-binding file
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defmacro_comp_load_file_with_declare() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Write a temp file, load it, and check that defmacro defined the macro
    let form = r#"(let ((tmpfile (make-temp-file "neovm-test-" nil ".el")))
  (unwind-protect
      (progn
        (with-temp-file tmpfile
          (insert ";;; -*- lexical-binding: t; -*-\n")
          (insert "(defvar test-disabled-list nil)\n")
          (insert "(defmacro test-file-macro! (name &rest body)\n")
          (insert "  \"Test macro.\"\n")
          (insert "  (declare (indent 1))\n")
          (insert "  (unless (memq name test-disabled-list)\n")
          (insert "    `(progn ,@body)))\n"))
        (load tmpfile nil 'nomessage)
        (list (fboundp 'test-file-macro!)
              (macroexpand '(test-file-macro! foo (+ 1 2)))))
    (delete-file tmpfile)
    (fmakunbound 'test-file-macro!)))"#;
    let expect = expect_test::expect![[r#""OK (t (progn (+ 1 2)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
