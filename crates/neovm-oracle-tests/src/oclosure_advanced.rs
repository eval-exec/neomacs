//! Advanced oracle parity tests for closure and function object semantics:
//! &rest capturing, &optional defaults, closures as first-class values,
//! closure equality, interactive spec, lambda docstrings, closure-based
//! module systems, and method resolution chains.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Closure capturing &rest arguments
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_oclosure_adv_capture_rest_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A closure factory captures &rest args from the outer scope and
    // later combines them with its own &rest args.
    let form = r#"(let ((make-prefixed-logger
                     (lambda (&rest prefix-parts)
                       (let ((prefix (mapconcat
                                       (lambda (p)
                                         (cond ((stringp p) p)
                                               ((symbolp p) (symbol-name p))
                                               ((numberp p) (number-to-string p))
                                               (t "?")))
                                       prefix-parts ":")))
                         (lambda (&rest msg-parts)
                           (concat "[" prefix "] "
                                   (mapconcat
                                     (lambda (m)
                                       (if (stringp m) m
                                         (prin1-to-string m)))
                                     msg-parts " ")))))))
      (let ((app-logger (funcall make-prefixed-logger 'app "v2" 1))
            (db-logger  (funcall make-prefixed-logger "db" 'postgres)))
        (list
          (funcall app-logger "started" "successfully")
          (funcall app-logger "error" 404)
          (funcall db-logger "connected" "to" "host")
          ;; Empty rest args
          (funcall app-logger)
          ;; Single arg
          (funcall db-logger "ping"))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"[app:v2:1] started successfully\" \"[app:v2:1] error 404\" \"[db:postgres] connected to host\" \"[app:v2:1] \" \"[db:postgres] ping\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closure with &optional parameters and defaults
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_oclosure_adv_optional_params_defaults() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Closures with &optional parameters where default logic lives
    // inside the body (Elisp &optional defaults to nil).
    let form = r#"(let ((make-range
                     (lambda (&optional start end step)
                       (let ((s (or start 0))
                             (e (or end 10))
                             (st (or step 1)))
                         (let ((result nil)
                               (i s))
                           (while (< i e)
                             (setq result (cons i result))
                             (setq i (+ i st)))
                           (nreverse result))))))
      (list
        (funcall make-range)
        (funcall make-range 5)
        (funcall make-range 1 6)
        (funcall make-range 0 20 5)
        (funcall make-range 10 10)
        (funcall make-range 0 3 1)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 1 2 3 4 5 6 7 8 9) (5 6 7 8 9) (1 2 3 4 5) (0 5 10 15) nil (0 1 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closures as first-class values (stored in lists, hash tables)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_oclosure_adv_first_class_storage() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Store closures in a list and a hash table, retrieve and call them.
    let form = r#"(let ((ops (list
                      (cons 'double (lambda (x) (* x 2)))
                      (cons 'square (lambda (x) (* x x)))
                      (cons 'negate (lambda (x) (- 0 x)))
                      (cons 'inc    (lambda (x) (+ x 1))))))
      (let ((tbl (make-hash-table :test 'eq)))
        ;; Copy to hash table
        (dolist (pair ops)
          (puthash (car pair) (cdr pair) tbl))
        ;; Apply from list
        (let ((from-list
               (mapcar (lambda (pair)
                         (funcall (cdr pair) 7))
                       ops))
              ;; Apply from hash table
              (from-table
               (mapcar (lambda (name)
                         (funcall (gethash name tbl) 7))
                       '(double square negate inc))))
          ;; Pipeline: chain ops from list
          (let ((pipeline-result
                 (let ((val 3))
                   (dolist (pair ops)
                     (setq val (funcall (cdr pair) val)))
                   val)))
            (list from-list from-table pipeline-result)))))"#;
    let expect = expect_test::expect![[r#""OK ((14 49 -7 8) (14 49 -7 8) -35)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closure equality: closures are NOT eq even if identical
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_oclosure_adv_closure_equality() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Two closures created by the same code are NOT eq. Even re-calling
    // the same factory yields distinct closure objects.
    let form = r#"(let ((make-adder (lambda (n) (lambda (x) (+ x n)))))
      (let ((a1 (funcall make-adder 5))
            (a2 (funcall make-adder 5))
            (a3 (funcall make-adder 10)))
        ;; Same factory, same arg -> still not eq
        (let ((same-factory-eq (eq a1 a2))
              (diff-arg-eq (eq a1 a3))
              ;; Self eq
              (self-eq (eq a1 a1))
              ;; funcall results are equal though
              (results-equal (= (funcall a1 100) (funcall a2 100)))
              (results-diff  (not (= (funcall a1 100) (funcall a3 100))))
              ;; functionp on all
              (all-funcp (list (functionp a1) (functionp a2) (functionp a3))))
          (list same-factory-eq diff-arg-eq self-eq
                results-equal results-diff all-funcp))))"#;
    let expect = expect_test::expect![[r#""OK (nil nil t t t (t t t))""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(nil nil t t t (t t t))", &o, &n);
}

// ---------------------------------------------------------------------------
// Interactive spec in closures (commandp tests)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_oclosure_adv_interactive_spec() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Lambdas with (interactive) are commands; without are not.
    // commandp reflects this.
    let form = r#"(let ((plain-fn (lambda (x) (+ x 1)))
                    (interactive-fn (lambda (x)
                                     (interactive "nNumber: ")
                                     (* x x)))
                    (interactive-no-args (lambda ()
                                          (interactive)
                                          42)))
      (list
        (commandp plain-fn)
        (commandp interactive-fn)
        (commandp interactive-no-args)
        ;; functionp is true for all
        (functionp plain-fn)
        (functionp interactive-fn)
        ;; Can still funcall interactive fns directly
        (funcall interactive-fn 6)
        (funcall interactive-no-args)
        ;; Built-in commands
        (commandp 'self-insert-command)
        ;; Non-functions
        (commandp 42)
        (commandp "string")))"#;
    let expect = expect_test::expect![[r#""OK (nil t t t t 36 42 t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Lambda with documentation strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_oclosure_adv_lambda_docstring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Lambdas can have docstrings; they don't affect evaluation but
    // can be observed via documentation function (if available) or
    // by inspecting the lambda structure.
    let form = r#"(let ((documented
                     (lambda (x y)
                       "Add X and Y together and return the sum."
                       (+ x y)))
                    (undocumented
                     (lambda (x y) (+ x y)))
                    (docstring-only
                     (lambda ()
                       "This function has only a docstring."
                       nil))
                    (multi-body-doc
                     (lambda (a b)
                       "Compute a complex result from A and B."
                       (let ((sum (+ a b))
                             (prod (* a b)))
                         (list sum prod (- sum prod))))))
      (list
        ;; All are callable
        (funcall documented 3 4)
        (funcall undocumented 3 4)
        (funcall docstring-only)
        (funcall multi-body-doc 5 3)
        ;; All are functions
        (functionp documented)
        (functionp undocumented)
        (functionp docstring-only)
        (functionp multi-body-doc)))"#;
    let expect = expect_test::expect![[r#""OK (7 7 nil (8 15 -7) t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: closure-based module system (private/public exports)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_oclosure_adv_module_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a module: private state and helper functions are captured
    // in closure scope; only the "public API" is returned as an alist.
    let form = r#"(let ((make-math-module
                     (lambda ()
                       ;; Private state
                       (let ((call-count 0)
                             (cache (make-hash-table :test 'equal)))
                         ;; Private helper
                         (let ((track-call
                                (lambda (name)
                                  (setq call-count (1+ call-count))))
                               ;; Private memoization wrapper
                               (memoize
                                (lambda (key compute-fn)
                                  (let ((cached (gethash key cache)))
                                    (or cached
                                        (let ((val (funcall compute-fn)))
                                          (puthash key val cache)
                                          val))))))
                           ;; Public API as alist
                           (list
                            (cons 'factorial
                                  (lambda (n)
                                    (funcall track-call 'factorial)
                                    (funcall memoize (format "fact-%d" n)
                                             (lambda ()
                                               (let ((result 1) (i 1))
                                                 (while (<= i n)
                                                   (setq result (* result i))
                                                   (setq i (1+ i)))
                                                 result)))))
                            (cons 'fibonacci
                                  (lambda (n)
                                    (funcall track-call 'fibonacci)
                                    (funcall memoize (format "fib-%d" n)
                                             (lambda ()
                                               (if (< n 2) n
                                                 (let ((a 0) (b 1) (i 2) (tmp 0))
                                                   (while (<= i n)
                                                     (setq tmp (+ a b))
                                                     (setq a b)
                                                     (setq b tmp)
                                                     (setq i (1+ i)))
                                                   b))))))
                            (cons 'call-count
                                  (lambda () call-count))
                            (cons 'cache-size
                                  (lambda ()
                                    (hash-table-count cache)))))))))
      ;; Instantiate two independent modules
      (let* ((m1 (funcall make-math-module))
             (m2 (funcall make-math-module))
             (m1-fact (cdr (assq 'factorial m1)))
             (m1-fib  (cdr (assq 'fibonacci m1)))
             (m1-cc   (cdr (assq 'call-count m1)))
             (m1-cs   (cdr (assq 'cache-size m1)))
             (m2-fact (cdr (assq 'factorial m2)))
             (m2-cc   (cdr (assq 'call-count m2))))
        ;; Use module 1
        (let ((f5 (funcall m1-fact 5))
              (f10 (funcall m1-fact 10))
              (fib8 (funcall m1-fib 8))
              ;; Call again (should use cache)
              (f5-again (funcall m1-fact 5)))
          ;; Use module 2 (independent)
          (let ((m2-f7 (funcall m2-fact 7)))
            (list f5 f10 fib8 f5-again
                  (funcall m1-cc)   ;; 4 calls in m1
                  (funcall m1-cs)   ;; 3 unique cache entries
                  m2-f7
                  (funcall m2-cc)   ;; 1 call in m2
                  ;; Independence check
                  (= (funcall m1-cc) (funcall m2-cc)))))))"#;
    let expect = expect_test::expect![[r#""OK (120 3628800 21 120 4 3 5040 1 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: method resolution order using closure chains
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_oclosure_adv_method_resolution_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate C3 linearization-like method resolution using a chain
    // of closure "classes". Each "class" is a closure that takes a
    // method name and either handles it or delegates to its parent.
    let form = r#"(let ((make-class nil))
      (setq make-class
            (lambda (name methods &optional parent)
              (lambda (msg &rest args)
                (let ((handler (cdr (assq msg methods))))
                  (if handler
                      (apply handler args)
                    (if parent
                        (apply parent msg args)
                      (list 'no-method name msg)))))))
      ;; Base class: shape
      (let ((shape-class
             (funcall make-class 'shape
                      (list
                       (cons 'type (lambda () 'shape))
                       (cons 'describe (lambda (name) (format "shape:%s" name)))
                       (cons 'area (lambda () 0))))))
        ;; Derived: rectangle (overrides area, describe)
        (let ((rect-class
               (funcall make-class 'rectangle
                        (list
                         (cons 'type (lambda () 'rectangle))
                         (cons 'describe (lambda (name) (format "rect:%s" name)))
                         (cons 'area (lambda (w h) (* w h)))
                         (cons 'perimeter (lambda (w h) (* 2 (+ w h)))))
                        shape-class)))
          ;; Derived from rectangle: square (overrides area, perimeter)
          (let ((square-class
                 (funcall make-class 'square
                          (list
                           (cons 'type (lambda () 'square))
                           (cons 'area (lambda (s _ignored) (* s s)))
                           (cons 'perimeter (lambda (s _ignored) (* 4 s))))
                          rect-class)))
            (list
              ;; Shape
              (funcall shape-class 'type)
              (funcall shape-class 'describe "base")
              (funcall shape-class 'area)
              ;; Rectangle
              (funcall rect-class 'type)
              (funcall rect-class 'describe "box")
              (funcall rect-class 'area 3 4)
              (funcall rect-class 'perimeter 3 4)
              ;; Square inherits describe from rectangle
              (funcall square-class 'type)
              (funcall square-class 'describe "unit")
              (funcall square-class 'area 5 5)
              (funcall square-class 'perimeter 5 5)
              ;; Method not found in any class
              (funcall shape-class 'color))))))"#;
    let expect = expect_test::expect![[
        r#""OK (shape \"shape:base\" 0 rectangle \"rect:box\" 12 14 square \"rect:unit\" 25 20 (no-method shape color))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
