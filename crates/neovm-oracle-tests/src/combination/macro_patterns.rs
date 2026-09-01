//! Complex oracle tests for macro-heavy patterns: defmacro with &rest body,
//! macro generating multiple defuns, macroexpand inspection, nested macro
//! expansion, gensym-like naming, iteration construct macros, and
//! backquote splicing.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// defmacro with &rest body wrapping (timing/logging wrapper)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_macro_rest_body_wrapping() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Macro that wraps a body with pre/post hooks and captures the result
    let form = r#"(progn
                    (defvar neovm--test-trace-log nil)
                    (defmacro neovm--test-with-trace (label &rest body)
                      `(let ((neovm--trace-start-marker (format ">>> %s" ,label)))
                         (setq neovm--test-trace-log
                               (cons neovm--trace-start-marker neovm--test-trace-log))
                         (let ((neovm--trace-result (progn ,@body)))
                           (setq neovm--test-trace-log
                                 (cons (format "<<< %s => %S" ,label neovm--trace-result)
                                       neovm--test-trace-log))
                           neovm--trace-result)))
                    (unwind-protect
                        (progn
                          (setq neovm--test-trace-log nil)
                          (let ((r1 (neovm--test-with-trace "outer"
                                      (let ((x 10))
                                        (neovm--test-with-trace "inner"
                                          (setq x (* x x)))
                                        (+ x 5)))))
                            (list r1 (nreverse neovm--test-trace-log))))
                      (fmakunbound 'neovm--test-with-trace)
                      (makunbound 'neovm--test-trace-log)))"#;
    let expect = expect_test::expect![[
        r#""OK (105 (\">>> outer\" \">>> inner\" \"<<< inner => 100\" \"<<< outer => 105\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Macro generating multiple defuns (accessor generator)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_macro_generating_multiple_defuns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Macro that generates getter, setter, and predicate for a property-list based object
    let form = r#"(progn
                    (defmacro neovm--test-defaccessors (prefix &rest fields)
                      (let ((forms nil))
                        (dolist (field fields)
                          (let ((getter (intern (format "%s-%s" prefix field)))
                                (setter (intern (format "%s-set-%s" prefix field)))
                                (pred (intern (format "%s-%s-p" prefix field))))
                            (setq forms
                                  (append forms
                                          (list
                                           `(fset ',getter
                                                  (lambda (obj) (plist-get obj ',field)))
                                           `(fset ',setter
                                                  (lambda (obj val)
                                                    (plist-put obj ',field val)))
                                           `(fset ',pred
                                                  (lambda (obj)
                                                    (not (null (plist-member obj ',field))))))))))
                        `(progn ,@forms ',prefix)))
                    (unwind-protect
                        (progn
                          (neovm--test-defaccessors neovm--person name age email)
                          (let* ((p (list 'name "Alice" 'age 30))
                                 (p2 (funcall 'neovm--person-set-email p "alice@example.com")))
                            (list
                             (funcall 'neovm--person-name p)
                             (funcall 'neovm--person-age p)
                             (funcall 'neovm--person-email-p p)
                             (funcall 'neovm--person-email-p p2)
                             (funcall 'neovm--person-email p2))))
                      (fmakunbound 'neovm--test-defaccessors)
                      (dolist (sym '(neovm--person-name neovm--person-set-name neovm--person-name-p
                                     neovm--person-age neovm--person-set-age neovm--person-age-p
                                     neovm--person-email neovm--person-set-email neovm--person-email-p))
                        (fmakunbound sym))))"#;
    let expect = expect_test::expect![[r#""OK (\"Alice\" 30 t t \"alice@example.com\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// macroexpand / macroexpand-all inspection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_macro_macroexpand_inspection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify macroexpand returns the correct expansion for known macros
    let form = r#"(progn
                    (defmacro neovm--test-swap (a b)
                      `(let ((neovm--tmp ,a))
                         (setq ,a ,b)
                         (setq ,b neovm--tmp)))
                    (unwind-protect
                        (let ((expansion (macroexpand '(neovm--test-swap x y))))
                          ;; Verify the expansion shape
                          (list
                           (car expansion)        ; should be 'let
                           (length (cadr expansion)) ; 1 binding
                           (caar (cadr expansion))   ; bound var name
                           ;; Actually run the macro to verify behavior
                           (let ((x 10) (y 20))
                             (neovm--test-swap x y)
                             (list x y))))
                      (fmakunbound 'neovm--test-swap)))"#;
    let expect = expect_test::expect![[r#""OK (let 1 neovm--tmp (20 10))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested macro expansion (macro calling macro)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_macro_nested_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // One macro uses another in its expansion
    let form = r#"(progn
                    (defmacro neovm--test-bind-default (var default &rest body)
                      `(let ((,var (or ,var ,default)))
                         ,@body))
                    (defmacro neovm--test-with-defaults (bindings &rest body)
                      (if (null bindings)
                          `(progn ,@body)
                        (let ((binding (car bindings)))
                          `(neovm--test-bind-default ,(car binding) ,(cadr binding)
                             (neovm--test-with-defaults ,(cdr bindings) ,@body)))))
                    (unwind-protect
                        (list
                         ;; All defaults used (all vars are nil)
                         (let ((a nil) (b nil) (c nil))
                           (neovm--test-with-defaults ((a 10) (b 20) (c 30))
                             (+ a b c)))
                         ;; Some overridden
                         (let ((a 5) (b nil) (c 100))
                           (neovm--test-with-defaults ((a 10) (b 20) (c 30))
                             (+ a b c)))
                         ;; All overridden
                         (let ((a 1) (b 2) (c 3))
                           (neovm--test-with-defaults ((a 10) (b 20) (c 30))
                             (list a b c))))
                      (fmakunbound 'neovm--test-bind-default)
                      (fmakunbound 'neovm--test-with-defaults)))"#;
    let expect = expect_test::expect![[r#""OK (60 125 (1 2 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Macro with gensym-like unique naming via make-symbol
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_macro_gensym_make_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Macro that uses make-symbol to avoid variable capture
    let form = r#"(progn
                    (defmacro neovm--test-swap-safe (a b)
                      (let ((tmp (make-symbol "tmp")))
                        `(let ((,tmp ,a))
                           (setq ,a ,b
                                 ,b ,tmp))))
                    ;; Test that the macro works even when 'tmp' exists in scope
                    (unwind-protect
                        (let ((x 100)
                              (y 200)
                              (tmp 999))
                          (neovm--test-swap-safe x y)
                          ;; tmp should be unaffected by the macro
                          (list x y tmp))
                      (fmakunbound 'neovm--test-swap-safe)))"#;
    let expect = expect_test::expect![[r#""OK (200 100 999)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Macro defining iteration constructs (do-alist, do-hash)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_macro_iteration_constructs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Define custom iteration macros and use them in complex scenarios
    let form = r#"(progn
                    ;; do-alist: iterate over association list binding key and value
                    (defmacro neovm--test-do-alist (spec &rest body)
                      (let ((key-var (nth 0 spec))
                            (val-var (nth 1 spec))
                            (alist-expr (nth 2 spec))
                            (result-form (nth 3 spec))
                            (pair-sym (make-symbol "pair")))
                        `(let ((,pair-sym nil))
                           (dolist (,pair-sym ,alist-expr ,result-form)
                             (let ((,key-var (car ,pair-sym))
                                   (,val-var (cdr ,pair-sym)))
                               ,@body)))))
                    ;; do-hash: iterate over hash table binding key and value
                    (defmacro neovm--test-do-hash (spec &rest body)
                      (let ((key-var (nth 0 spec))
                            (val-var (nth 1 spec))
                            (hash-expr (nth 2 spec)))
                        `(progn
                           (maphash (lambda (,key-var ,val-var) ,@body)
                                    ,hash-expr)
                           nil)))
                    (unwind-protect
                        (let ((inventory '((apples . 5) (bananas . 12) (cherries . 3)
                                           (dates . 8) (elderberries . 1)))
                              (expensive nil)
                              (total 0))
                          ;; Use do-alist to process the inventory
                          (neovm--test-do-alist (fruit count inventory)
                            (setq total (+ total count))
                            (when (> count 4)
                              (setq expensive (cons fruit expensive))))
                          ;; Convert to hash and iterate with do-hash
                          (let ((h (make-hash-table :test 'eq))
                                (doubled nil))
                            (dolist (pair inventory)
                              (puthash (car pair) (cdr pair) h))
                            (neovm--test-do-hash (k v h)
                              (setq doubled (cons (cons k (* v 2)) doubled)))
                            (list total
                                  (sort expensive
                                        (lambda (a b) (string< (symbol-name a) (symbol-name b))))
                                  (sort doubled
                                        (lambda (a b) (string< (symbol-name (car a))
                                                               (symbol-name (car b))))))))
                      (fmakunbound 'neovm--test-do-alist)
                      (fmakunbound 'neovm--test-do-hash)))"#;
    let expect = expect_test::expect![[
        r#""OK (29 (apples bananas dates) ((apples . 10) (bananas . 24) (cherries . 6) (dates . 16) (elderberries . 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Backquote splicing in macro bodies
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_macro_backquote_splicing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Macro using ,@ splicing to compose code from lists
    let form = r#"(progn
                    ;; Macro that creates a function composing multiple transformations
                    (defmacro neovm--test-defpipeline (name &rest transforms)
                      (let ((arg-sym (make-symbol "x")))
                        `(fset ',name
                               (lambda (,arg-sym)
                                 ,@(let ((forms nil)
                                         (prev arg-sym))
                                     (dolist (tr (reverse transforms))
                                       (setq forms
                                             (cons `(setq ,arg-sym (funcall ,tr ,arg-sym))
                                                   forms)))
                                     forms)
                                 ,arg-sym))))
                    (unwind-protect
                        (progn
                          (neovm--test-defpipeline neovm--test-process
                            (lambda (x) (* x 2))
                            (lambda (x) (+ x 10))
                            (lambda (x) (* x x)))
                          ;; (((5 * 2) + 10) ^ 2) = ((10 + 10) ^ 2) = 400
                          (list
                           (funcall 'neovm--test-process 5)
                           (funcall 'neovm--test-process 0)
                           (funcall 'neovm--test-process 3)))
                      (fmakunbound 'neovm--test-defpipeline)
                      (fmakunbound 'neovm--test-process)))"#;
    let expect = expect_test::expect![[r#""OK (400 100 256)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Macro: conditional compilation pattern
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_macro_conditional_compilation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Macro that conditionally includes debug instrumentation
    let form = r#"(progn
                    (defvar neovm--test-debug-mode nil)
                    (defmacro neovm--test-defn (name args &rest body)
                      (if neovm--test-debug-mode
                          `(fset ',name
                                 (lambda ,args
                                   (let ((neovm--entry-args (list ,@args)))
                                     (condition-case err
                                         (let ((neovm--result (progn ,@body)))
                                           (list 'ok neovm--result))
                                       (error (list 'error (car err) neovm--entry-args))))))
                        `(fset ',name (lambda ,args ,@body))))
                    (unwind-protect
                        (let ((results nil))
                          ;; Non-debug mode
                          (setq neovm--test-debug-mode nil)
                          (neovm--test-defn neovm--test-add (a b) (+ a b))
                          (setq results (cons (funcall 'neovm--test-add 3 4) results))
                          ;; Debug mode
                          (setq neovm--test-debug-mode t)
                          (neovm--test-defn neovm--test-div (a b) (/ a b))
                          (setq results (cons (funcall 'neovm--test-div 10 2) results))
                          (setq results (cons (funcall 'neovm--test-div 10 0) results))
                          (nreverse results))
                      (fmakunbound 'neovm--test-defn)
                      (fmakunbound 'neovm--test-add)
                      (fmakunbound 'neovm--test-div)
                      (makunbound 'neovm--test-debug-mode)))"#;
    let expect = expect_test::expect![[r#""OK (7 (ok 5) (error arith-error (10 0)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
