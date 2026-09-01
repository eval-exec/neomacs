//! Advanced oracle parity tests for `subr-arity` and `subrp`.
//!
//! Tests subr-arity on various built-in functions, verifies (MIN . MAX)
//! cons cell structure, tests subrp predicate on diverse types,
//! tests &rest (many) and &optional arity bounds, and builds
//! a function introspection system using subr-arity for dispatch.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// subr-arity on core built-in functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_arity_core_builtins() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test subr-arity on a wide range of built-in functions.
    // Each returns a (MIN . MAX) cons cell where MAX may be 'many.
    let form = r#"(list
  ;; 1-arg functions
  (subr-arity (symbol-function 'car))
  (subr-arity (symbol-function 'cdr))
  (subr-arity (symbol-function 'length))
  (subr-arity (symbol-function 'not))
  (subr-arity (symbol-function 'null))
  (subr-arity (symbol-function 'atom))
  (subr-arity (symbol-function 'symbolp))
  (subr-arity (symbol-function 'stringp))
  ;; 2-arg functions
  (subr-arity (symbol-function 'cons))
  (subr-arity (symbol-function 'eq))
  (subr-arity (symbol-function 'equal))
  (subr-arity (symbol-function 'setcar))
  (subr-arity (symbol-function 'setcdr))
  ;; 0-or-more args (&rest / many)
  (subr-arity (symbol-function '+))
  (subr-arity (symbol-function '*))
  (subr-arity (symbol-function 'list))
  (subr-arity (symbol-function 'concat))
  (subr-arity (symbol-function 'append))
  ;; Functions with &optional
  (subr-arity (symbol-function 'substring))
  (subr-arity (symbol-function 'nth))
  (subr-arity (symbol-function 'make-string)))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument subrp null)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// subr-arity returns (MIN . MAX) cons cell: structural tests
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_arity_cons_cell_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify the returned value is always a cons cell, that car is an
    // integer >= 0, and that cdr is either an integer >= car or 'many.
    let form = r#"(progn
  (fset 'neovm--sa-validate
    (lambda (fn-sym)
      "Validate subr-arity return structure for FN-SYM."
      (let* ((def (symbol-function fn-sym))
             (arity (subr-arity def))
             (min-a (car arity))
             (max-a (cdr arity)))
        (list
          fn-sym
          (consp arity)
          (integerp min-a)
          (>= min-a 0)
          (or (eq max-a 'many) (integerp max-a))
          (or (eq max-a 'many) (>= max-a min-a))
          min-a
          max-a))))

  (unwind-protect
      (list
        (funcall 'neovm--sa-validate 'car)
        (funcall 'neovm--sa-validate 'cons)
        (funcall 'neovm--sa-validate '+)
        (funcall 'neovm--sa-validate 'list)
        (funcall 'neovm--sa-validate 'mapcar)
        (funcall 'neovm--sa-validate 'substring)
        (funcall 'neovm--sa-validate 'format)
        (funcall 'neovm--sa-validate 'make-vector))
    (fmakunbound 'neovm--sa-validate)))"#;
    let expect = expect_test::expect![[
        r#""OK ((car t t t t t 1 1) (cons t t t t t 2 2) (+ t t t t t 0 many) (list t t t t t 0 many) (mapcar t t t t t 2 2) (substring t t t t t 1 3) (format t t t t t 1 many) (make-vector t t t t t 2 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// subrp predicate on various types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subrp_on_various_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // subrp should return t only for built-in subr objects, not
    // lambdas, closures, macros, symbols, strings, or numbers.
    let form = r#"(progn
  (fset 'neovm--sa-test-lambda (lambda (x) (* x x)))

  (unwind-protect
      (list
        ;; Built-in subrs (should be t)
        (subrp (symbol-function 'car))
        (subrp (symbol-function '+))
        (subrp (symbol-function 'cons))
        (subrp (symbol-function 'concat))
        (subrp (symbol-function 'mapcar))
        (subrp (symbol-function 'length))
        ;; NOT subrs (should be nil)
        (subrp (symbol-function 'neovm--sa-test-lambda))
        (subrp (lambda (a b) (+ a b)))
        (subrp nil)
        (subrp t)
        (subrp 42)
        (subrp 3.14)
        (subrp "hello")
        (subrp '(1 2 3))
        (subrp [1 2 3])
        (subrp (make-hash-table))
        (subrp 'car)  ;; symbol, not the function itself
        (subrp '+))   ;; symbol, not the function
    (fmakunbound 'neovm--sa-test-lambda)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t t nil nil nil nil nil nil nil nil nil nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// subr-arity on &rest functions (many MAX)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_arity_rest_many() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Functions with &rest parameters have 'many as their MAX.
    // Verify that calling them with variable arg counts works as
    // expected and that arity reflects the &rest nature.
    let form = r#"(progn
  (defvar neovm--sa-rest-fns
    '(+ - * list vector concat append format message))

  (fset 'neovm--sa-check-rest
    (lambda (fn-sym)
      (let* ((def (symbol-function fn-sym))
             (arity (when (subrp def) (subr-arity def))))
        (list fn-sym
              (when arity (car arity))
              (when arity (eq (cdr arity) 'many))))))

  (unwind-protect
      (let ((results nil))
        (dolist (f neovm--sa-rest-fns)
          (setq results (cons (funcall 'neovm--sa-check-rest f) results)))
        (list
          (nreverse results)
          ;; Verify we can call &rest functions with varying arg counts
          (+ )           ;; 0 args
          (+ 1)          ;; 1 arg
          (+ 1 2)        ;; 2 args
          (+ 1 2 3 4 5)  ;; 5 args
          (list)          ;; 0 args
          (list 'a)       ;; 1 arg
          (list 'a 'b 'c 'd 'e 'f 'g)  ;; 7 args
          (concat)        ;; 0 args
          (concat "a" "b" "c" "d")))    ;; 4 args
    (fmakunbound 'neovm--sa-check-rest)
    (makunbound 'neovm--sa-rest-fns)))"#;
    let expect = expect_test::expect![[
        r#""OK (((+ 0 t) (- 0 t) (* 0 t) (list 0 t) (vector 0 t) (concat 0 t) (append 0 t) (format 1 t) (message 1 t)) 0 1 3 15 nil (a) (a b c d e f g) \"\" \"abcd\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// subr-arity on functions with &optional parameters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_arity_optional() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Functions with &optional have MIN < MAX (both integers).
    // Verify the difference between MIN and MAX reflects the number
    // of optional parameters.
    let form = r#"(progn
  (fset 'neovm--sa-opt-info
    (lambda (fn-sym)
      (let* ((def (symbol-function fn-sym))
             (arity (when (subrp def) (subr-arity def)))
             (min-a (when arity (car arity)))
             (max-a (when arity (cdr arity)))
             (opt-count (when (and min-a (integerp max-a))
                          (- max-a min-a))))
        (list fn-sym min-a max-a opt-count
              ;; Has optional params?
              (and opt-count (> opt-count 0))))))

  (unwind-protect
      (list
        ;; substring: 1 required (string), up to 3 total
        (funcall 'neovm--sa-opt-info 'substring)
        ;; nth: exactly 2 required
        (funcall 'neovm--sa-opt-info 'nth)
        ;; make-string: 2 required, 1 optional
        (funcall 'neovm--sa-opt-info 'make-string)
        ;; assoc: 2 required, 1 optional (testfn)
        (funcall 'neovm--sa-opt-info 'assoc)
        ;; Verify calling with different numbers of args
        (substring "hello" 1)
        (substring "hello" 1 3)
        (make-string 5 ?x)
        (assoc 'a '((a . 1) (b . 2)))
        (assoc "x" '(("x" . 1) ("y" . 2)) 'string=))
    (fmakunbound 'neovm--sa-opt-info)))"#;
    let expect = expect_test::expect![[
        r#""OK ((substring 1 3 2 t) (nth 2 2 0 nil) (make-string 2 3 1 t) (assoc 2 3 1 t) \"ello\" \"el\" \"xxxxx\" (a . 1) (\"x\" . 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: function introspection system using subr-arity
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_arity_introspection_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a function introspection framework that categorizes functions
    // by their arity patterns and uses that information for safe calling.
    let form = r#"(progn
  (fset 'neovm--sa-classify
    (lambda (fn-sym)
      "Classify a function by its arity pattern."
      (let* ((def (symbol-function fn-sym))
             (is-subr (subrp def))
             (arity (when is-subr (subr-arity def)))
             (min-a (when arity (car arity)))
             (max-a (when arity (cdr arity))))
        (cond
          ((not is-subr) 'not-a-subr)
          ((eq max-a 'many) 'variadic)
          ((= min-a max-a) 'fixed)
          (t 'optional)))))

  (fset 'neovm--sa-safe-call
    (lambda (fn-sym args)
      "Safely call FN-SYM with ARGS, padding or truncating based on arity."
      (let* ((def (symbol-function fn-sym))
             (arity (when (subrp def) (subr-arity def)))
             (min-a (if arity (car arity) 0))
             (max-a (if arity (cdr arity) 'many)))
        (condition-case err
            (let ((adjusted-args
                   (cond
                     ;; Variadic: pass as-is
                     ((eq max-a 'many) args)
                     ;; Too few args: pad with nil
                     ((< (length args) min-a)
                      (let ((padded (copy-sequence args)))
                        (while (< (length padded) min-a)
                          (setq padded (append padded '(nil))))
                        padded))
                     ;; Too many args: truncate
                     ((> (length args) max-a)
                      (let ((result nil) (i 0))
                        (while (< i max-a)
                          (setq result (cons (nth i args) result)
                                i (1+ i)))
                        (nreverse result)))
                     ;; Just right
                     (t args))))
              (list 'ok (apply fn-sym adjusted-args)))
          (error (list 'error (car err)))))))

  (fset 'neovm--sa-registry
    (lambda (fn-list)
      "Build a registry of function metadata."
      (let ((registry nil))
        (dolist (fn-sym fn-list)
          (let* ((def (symbol-function fn-sym))
                 (is-subr (subrp def))
                 (arity (when is-subr (subr-arity def)))
                 (class (funcall 'neovm--sa-classify fn-sym)))
            (setq registry
                  (cons (list fn-sym class
                              (when arity (car arity))
                              (when arity (cdr arity)))
                        registry))))
        (nreverse registry))))

  (unwind-protect
      (let ((fns '(car cdr cons + * list length concat
                   substring nth mapcar append)))
        (list
          ;; Build registry
          (funcall 'neovm--sa-registry fns)
          ;; Group by classification
          (let ((fixed nil) (variadic nil) (optional nil))
            (dolist (fn-sym fns)
              (let ((class (funcall 'neovm--sa-classify fn-sym)))
                (cond
                  ((eq class 'fixed) (setq fixed (cons fn-sym fixed)))
                  ((eq class 'variadic) (setq variadic (cons fn-sym variadic)))
                  ((eq class 'optional) (setq optional (cons fn-sym optional))))))
            (list (nreverse fixed)
                  (nreverse variadic)
                  (nreverse optional)))
          ;; Safe calling tests
          (funcall 'neovm--sa-safe-call 'car '((1 2 3)))
          (funcall 'neovm--sa-safe-call 'cons '(a))       ;; too few -> pad
          (funcall 'neovm--sa-safe-call '+ '(1 2 3 4 5))  ;; variadic
          (funcall 'neovm--sa-safe-call '+ nil)            ;; variadic with 0 args
          (funcall 'neovm--sa-safe-call 'list '(a b c))    ;; variadic
          (funcall 'neovm--sa-safe-call 'length '("hello"))
          ;; Count of each class
          (let ((counts (make-hash-table)))
            (dolist (fn-sym fns)
              (let ((class (funcall 'neovm--sa-classify fn-sym)))
                (puthash class (1+ (gethash class counts 0)) counts)))
            (list (gethash 'fixed counts 0)
                  (gethash 'variadic counts 0)
                  (gethash 'optional counts 0)))))
    (fmakunbound 'neovm--sa-classify)
    (fmakunbound 'neovm--sa-safe-call)
    (fmakunbound 'neovm--sa-registry)))"#;
    let expect = expect_test::expect![[
        r#""OK (((car fixed 1 1) (cdr fixed 1 1) (cons fixed 2 2) (+ variadic 0 many) (* variadic 0 many) (list variadic 0 many) (length fixed 1 1) (concat variadic 0 many) (substring optional 1 3) (nth fixed 2 2) (mapcar fixed 2 2) (append variadic 0 many)) ((car cdr cons length nth mapcar) (+ * list concat append) (substring)) (ok 1) (ok (a)) (ok 15) (ok 0) (ok (a b c)) (ok 5) (6 5 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// subr-arity comparison across arithmetic and logic builtins
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_arity_comparison_arithmetic_logic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compare arity across families of related functions.
    // Arithmetic: all variadic. Comparison: all variadic.
    // Logic: some are 1-arg, some variadic.
    let form = r#"(progn
  (fset 'neovm--sa-arity-pair
    (lambda (fn-sym)
      (let ((arity (subr-arity (symbol-function fn-sym))))
        (cons fn-sym arity))))

  (unwind-protect
      (list
        ;; Arithmetic family
        (mapcar 'neovm--sa-arity-pair '(+ - * / mod % max min))
        ;; Comparison family
        (mapcar 'neovm--sa-arity-pair '(< > <= >= = /=))
        ;; Logic/predicate family
        (mapcar 'neovm--sa-arity-pair '(not null atom consp listp))
        ;; String functions
        (mapcar 'neovm--sa-arity-pair '(string= string< concat))
        ;; Are all arithmetic functions variadic?
        (let ((all-variadic t))
          (dolist (fn '(+ - * /))
            (let ((arity (subr-arity (symbol-function fn))))
              (unless (eq (cdr arity) 'many)
                (setq all-variadic nil))))
          all-variadic)
        ;; Are all 1-arg predicates fixed arity?
        (let ((all-fixed t))
          (dolist (fn '(not null atom consp listp))
            (let ((arity (subr-arity (symbol-function fn))))
              (unless (and (= (car arity) 1)
                           (integerp (cdr arity))
                           (= (cdr arity) 1))
                (setq all-fixed nil))))
          all-fixed))
    (fmakunbound 'neovm--sa-arity-pair)))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument subrp null)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
