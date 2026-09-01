//! Oracle parity tests for subr (built-in function) inspection operations:
//! `subrp`, `subr-name`, `subr-arity` (min/max args), `commandp`,
//! `functionp`, `byte-code-function-p`, `compiled-function-p`,
//! `special-form-p`, `closurep`, combined predicates on various callable types.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// subrp: comprehensive predicate on all kinds of objects
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_ops_subrp_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Built-in functions are subrs
  (subrp (symbol-function '+))
  (subrp (symbol-function 'car))
  (subrp (symbol-function 'cdr))
  (subrp (symbol-function 'cons))
  (subrp (symbol-function 'length))
  (subrp (symbol-function 'concat))
  (subrp (symbol-function 'mapcar))
  (subrp (symbol-function 'apply))
  ;; Special forms are also subrs
  (subrp (symbol-function 'if))
  (subrp (symbol-function 'progn))
  (subrp (symbol-function 'let))
  (subrp (symbol-function 'setq))
  (subrp (symbol-function 'quote))
  (subrp (symbol-function 'cond))
  ;; Non-subr objects
  (subrp (lambda (x) x))
  (subrp nil)
  (subrp t)
  (subrp 42)
  (subrp "hello")
  (subrp '(1 2 3))
  (subrp (make-hash-table))
  (subrp [1 2 3]))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t t t t t t t t t t nil nil nil nil nil nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// subr-name: extracting the name of built-in functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_ops_subr_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Names of various built-in functions
  (subr-name (symbol-function '+))
  (subr-name (symbol-function 'car))
  (subr-name (symbol-function 'cdr))
  (subr-name (symbol-function 'cons))
  (subr-name (symbol-function 'list))
  (subr-name (symbol-function 'length))
  (subr-name (symbol-function 'concat))
  (subr-name (symbol-function 'format))
  (subr-name (symbol-function 'eq))
  (subr-name (symbol-function 'equal))
  ;; Special forms have names too
  (subr-name (symbol-function 'if))
  (subr-name (symbol-function 'progn))
  (subr-name (symbol-function 'let))
  ;; Return type is string
  (stringp (subr-name (symbol-function '+)))
  ;; Error on non-subr
  (condition-case err
      (subr-name (lambda (x) x))
    (wrong-type-argument (list 'error (car err)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"+\" \"car\" \"cdr\" \"cons\" \"list\" \"length\" \"concat\" \"format\" \"eq\" \"equal\" \"if\" \"progn\" \"let\" t (error wrong-type-argument))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// subr-arity: min/max args for all parameter patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_ops_subr_arity_all_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; (1 . 1): exactly 1 arg
  (subr-arity (symbol-function 'car))
  (subr-arity (symbol-function 'cdr))
  (subr-arity (symbol-function 'not))
  ;; (2 . 2): exactly 2 args
  (subr-arity (symbol-function 'cons))
  (subr-arity (symbol-function 'eq))
  (subr-arity (symbol-function 'aref))
  ;; (0 . many): zero or more
  (subr-arity (symbol-function '+))
  (subr-arity (symbol-function 'list))
  (subr-arity (symbol-function 'concat))
  ;; (1 . many): one or more
  (subr-arity (symbol-function 'append))
  ;; (2 . many): two or more
  (subr-arity (symbol-function 'mapcar))
  ;; Optional args: min < max but max is finite
  (subr-arity (symbol-function 'substring))
  (subr-arity (symbol-function 'nth))
  ;; Verify the many symbol
  (let ((ar (subr-arity (symbol-function '+))))
    (list (car ar) (eq (cdr ar) 'many)))
  ;; Error on non-subr
  (condition-case err
      (subr-arity 42)
    (wrong-type-argument (list 'error (car err)))))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument subrp null)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// commandp: interactive command detection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_ops_commandp_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Lambda without interactive: NOT a command
  (commandp (lambda (x) x))
  ;; Lambda with interactive: IS a command
  (commandp (lambda () (interactive) 42))
  ;; Lambda with interactive and args
  (commandp (lambda (n) (interactive "p") (* n 2)))
  ;; Built-in subrs: some are commands, some are not
  ;; + is not a command
  (commandp '+)
  (commandp 'car)
  ;; nil, t, numbers, strings
  (commandp nil)
  (commandp t)
  (commandp 42)
  (commandp "hello")
  ;; Symbol that is fbound to a lambda with interactive
  (unwind-protect
      (progn
        (fset 'neovm--test-cmd (lambda () (interactive) "a command" t))
        (commandp 'neovm--test-cmd))
    (fmakunbound 'neovm--test-cmd))
  ;; commandp with FOR-CALL-INTERACTIVELY = t (2nd arg)
  (commandp (lambda () (interactive) 42) t)
  ;; Verify commandp implies functionp for lambdas
  (let ((f (lambda () (interactive) 42)))
    (list (commandp f) (functionp f))))"#;
    let expect = expect_test::expect![[r#""OK (nil t t nil nil nil nil nil t t t (t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// functionp: comprehensive type checks
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_ops_functionp_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Lambda is a function
  (functionp (lambda (x) x))
  (functionp (lambda () nil))
  (functionp (lambda (a b &optional c) (+ a b)))
  (functionp (lambda (&rest args) args))
  ;; Built-in subr function object is a function
  (functionp (symbol-function '+))
  (functionp (symbol-function 'car))
  (functionp (symbol-function 'mapcar))
  ;; Symbol naming a function: functionp returns t
  (functionp '+)
  (functionp 'car)
  ;; Special forms: NOT functions per functionp
  (functionp (symbol-function 'if))
  (functionp (symbol-function 'progn))
  (functionp (symbol-function 'let))
  (functionp (symbol-function 'quote))
  ;; Non-functions
  (functionp nil)
  (functionp t)
  (functionp 42)
  (functionp 3.14)
  (functionp "hello")
  (functionp '(1 2 3))
  (functionp [1 2 3])
  (functionp (make-hash-table))
  ;; Void symbol
  (functionp 'nonexistent-function-xyz-12345))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t t t t t nil nil nil nil nil nil nil nil nil nil nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// special-form-p: detect special forms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_ops_special_form_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Known special forms
  (special-form-p (symbol-function 'if))
  (special-form-p (symbol-function 'let))
  (special-form-p (symbol-function 'let*))
  (special-form-p (symbol-function 'progn))
  (special-form-p (symbol-function 'setq))
  (special-form-p (symbol-function 'quote))
  (special-form-p (symbol-function 'cond))
  (special-form-p (symbol-function 'while))
  (special-form-p (symbol-function 'or))
  (special-form-p (symbol-function 'and))
  (special-form-p (symbol-function 'unwind-protect))
  (special-form-p (symbol-function 'condition-case))
  (special-form-p (symbol-function 'catch))
  (special-form-p (symbol-function 'defconst))
  (special-form-p (symbol-function 'function))
  ;; NOT special forms: regular built-in functions
  (special-form-p (symbol-function '+))
  (special-form-p (symbol-function 'car))
  (special-form-p (symbol-function 'cons))
  (special-form-p (symbol-function 'list))
  ;; Non-subr objects
  (special-form-p (lambda (x) x))
  (special-form-p nil)
  (special-form-p 42))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t t t t t t t t t t t nil nil nil nil nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_subr_ops_function_classification_matches_gnu_subr_el() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU subr.el:special-form-p resolves symbol function aliases via
    // indirect-function.  macrop returns the macro cons or matching autoload
    // tail, not necessarily canonical t.  compiled-function-p accepts ordinary
    // subrs but rejects unevalled special forms.
    let form = r#"
(let ((results nil))
  (defalias 'neovm--sf-alias 'if)
  (autoload 'neovm--autoload-macro "no-such-file" nil nil 'macro)
  (autoload 'neovm--autoload-fn "no-such-file" nil nil nil)
  (unwind-protect
      (setq results
            (list
             (special-form-p 'neovm--sf-alias)
             (special-form-p 'car)
             (macrop 'neovm--autoload-macro)
             (macrop 'neovm--autoload-fn)
             (macrop (symbol-function 'neovm--autoload-macro))
             (compiled-function-p 'car)
             (compiled-function-p (symbol-function 'car))
             (compiled-function-p (symbol-function 'if))))
    (fmakunbound 'neovm--sf-alias)
    (fmakunbound 'neovm--autoload-macro)
    (fmakunbound 'neovm--autoload-fn))
  results)
"#;
    let expect = expect_test::expect![[r#""OK (t nil (macro t) nil (macro t) nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// closurep: detect closure objects
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_ops_closurep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Note: in lexical binding context, lambda creates closures
    let form = r#"(list
  ;; Lambda in default lexical binding creates a closure
  (closurep (lambda (x) x))
  (closurep (lambda () nil))
  (closurep (lambda (a b) (+ a b)))
  (closurep (lambda (&rest args) args))
  ;; Closure capturing a lexical variable
  (let ((x 10))
    (closurep (lambda () x)))
  ;; Built-in subrs are NOT closures
  (closurep (symbol-function '+))
  (closurep (symbol-function 'car))
  ;; Special forms are NOT closures
  (closurep (symbol-function 'if))
  ;; Non-function types
  (closurep nil)
  (closurep t)
  (closurep 42)
  (closurep "hello")
  (closurep '(1 2 3))
  ;; Nested closure
  (let ((outer 1))
    (let ((inner 2))
      (closurep (lambda () (+ outer inner))))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t nil nil nil nil nil nil nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combined predicate matrix: classify callables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_ops_predicate_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--classify-callable
    (lambda (obj)
      "Classify a callable object using all available predicates."
      (list
        (list 'functionp (functionp obj))
        (list 'subrp (subrp obj))
        (list 'special-form-p (special-form-p obj))
        (list 'closurep (closurep obj))
        (list 'commandp (commandp obj)))))

  (unwind-protect
      (list
        ;; Regular lambda / closure
        (funcall 'neovm--classify-callable (lambda (x) x))
        ;; Built-in function (subr)
        (funcall 'neovm--classify-callable (symbol-function '+))
        (funcall 'neovm--classify-callable (symbol-function 'car))
        ;; Special form
        (funcall 'neovm--classify-callable (symbol-function 'if))
        (funcall 'neovm--classify-callable (symbol-function 'let))
        ;; Interactive command
        (funcall 'neovm--classify-callable (lambda () (interactive) nil))
        ;; nil
        (funcall 'neovm--classify-callable nil)
        ;; number
        (funcall 'neovm--classify-callable 42))
    (fmakunbound 'neovm--classify-callable)))"#;
    let expect = expect_test::expect![[
        r#""OK (((functionp t) (subrp nil) (special-form-p nil) (closurep t) (commandp nil)) ((functionp t) (subrp t) (special-form-p nil) (closurep nil) (commandp nil)) ((functionp t) (subrp t) (special-form-p nil) (closurep nil) (commandp nil)) ((functionp nil) (subrp t) (special-form-p t) (closurep nil) (commandp nil)) ((functionp nil) (subrp t) (special-form-p t) (closurep nil) (commandp nil)) ((functionp t) (subrp nil) (special-form-p nil) (closurep t) (commandp t)) ((functionp nil) (subrp nil) (special-form-p nil) (closurep nil) (commandp nil)) ((functionp nil) (subrp nil) (special-form-p nil) (closurep nil) (commandp nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Introspection pipeline: enumerate and inspect built-ins
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_ops_introspection_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--subr-info
    (lambda (sym)
      "Collect detailed info about a symbol's function binding."
      (let* ((def (and (fboundp sym) (symbol-function sym)))
             (is-subr (and def (subrp def)))
             (name (and is-subr (subr-name def)))
             (arity (and is-subr (subr-arity def)))
             (is-special (and def (special-form-p def)))
             (is-func (functionp sym)))
        (list sym
              (list 'bound (fboundp sym))
              (list 'subr is-subr)
              (list 'name name)
              (list 'min-args (and arity (car arity)))
              (list 'max-args (and arity (cdr arity)))
              (list 'special is-special)
              (list 'functionp is-func)))))

  (unwind-protect
      (let ((syms '(+ - * / = < > <= >=
                    car cdr cons list length nth
                    concat substring format
                    if let progn setq quote cond while
                    not null eq equal
                    apply funcall mapcar))
            (results nil))
        (dolist (s syms)
          (setq results (cons (funcall 'neovm--subr-info s) results)))
        ;; Return sorted by symbol name for stable comparison
        (nreverse results))
    (fmakunbound 'neovm--subr-info)))"#;
    let expect = expect_test::expect![[
        r#""OK ((+ (bound t) (subr t) (name \"+\") (min-args 0) (max-args many) (special nil) (functionp t)) (- (bound t) (subr t) (name \"-\") (min-args 0) (max-args many) (special nil) (functionp t)) (* (bound t) (subr t) (name \"*\") (min-args 0) (max-args many) (special nil) (functionp t)) (/ (bound t) (subr t) (name \"/\") (min-args 1) (max-args many) (special nil) (functionp t)) (= (bound t) (subr t) (name \"=\") (min-args 1) (max-args many) (special nil) (functionp t)) (< (bound t) (subr t) (name \"<\") (min-args 1) (max-args many) (special nil) (functionp t)) (> (bound t) (subr t) (name \">\") (min-args 1) (max-args many) (special nil) (functionp t)) (<= (bound t) (subr t) (name \"<=\") (min-args 1) (max-args many) (special nil) (functionp t)) (>= (bound t) (subr t) (name \">=\") (min-args 1) (max-args many) (special nil) (functionp t)) (car (bound t) (subr t) (name \"car\") (min-args 1) (max-args 1) (special nil) (functionp t)) (cdr (bound t) (subr t) (name \"cdr\") (min-args 1) (max-args 1) (special nil) (functionp t)) (cons (bound t) (subr t) (name \"cons\") (min-args 2) (max-args 2) (special nil) (functionp t)) (list (bound t) (subr t) (name \"list\") (min-args 0) (max-args many) (special nil) (functionp t)) (length (bound t) (subr t) (name \"length\") (min-args 1) (max-args 1) (special nil) (functionp t)) (nth (bound t) (subr t) (name \"nth\") (min-args 2) (max-args 2) (special nil) (functionp t)) (concat (bound t) (subr t) (name \"concat\") (min-args 0) (max-args many) (special nil) (functionp t)) (substring (bound t) (subr t) (name \"substring\") (min-args 1) (max-args 3) (special nil) (functionp t)) (format (bound t) (subr t) (name \"format\") (min-args 1) (max-args many) (special nil) (functionp t)) (if (bound t) (subr t) (name \"if\") (min-args 2) (max-args unevalled) (special t) (functionp nil)) (let (bound t) (subr t) (name \"let\") (min-args 1) (max-args unevalled) (special t) (functionp nil)) (progn (bound t) (subr t) (name \"progn\") (min-args 0) (max-args unevalled) (special t) (functionp nil)) (setq (bound t) (subr t) (name \"setq\") (min-args 0) (max-args unevalled) (special t) (functionp nil)) (quote (bound t) (subr t) (name \"quote\") (min-args 1) (max-args unevalled) (special t) (functionp nil)) (cond (bound t) (subr t) (name \"cond\") (min-args 0) (max-args unevalled) (special t) (functionp nil)) (while (bound t) (subr t) (name \"while\") (min-args 1) (max-args unevalled) (special t) (functionp nil)) (not (bound t) (subr nil) (name nil) (min-args nil) (max-args nil) (special nil) (functionp t)) (null (bound t) (subr t) (name \"null\") (min-args 1) (max-args 1) (special nil) (functionp t)) (eq (bound t) (subr t) (name \"eq\") (min-args 2) (max-args 2) (special nil) (functionp t)) (equal (bound t) (subr t) (name \"equal\") (min-args 2) (max-args 2) (special nil) (functionp t)) (apply (bound t) (subr t) (name \"apply\") (min-args 1) (max-args many) (special nil) (functionp t)) (funcall (bound t) (subr t) (name \"funcall\") (min-args 1) (max-args many) (special nil) (functionp t)) (mapcar (bound t) (subr t) (name \"mapcar\") (min-args 2) (max-args 2) (special nil) (functionp t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Arity-based function dispatch and validation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_ops_arity_validation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--check-arity
    (lambda (fn nargs)
      "Check if FN can be called with NARGS arguments.
       Returns (ok min max) or (too-few min max) or (too-many min max)."
      (let* ((def (if (symbolp fn) (symbol-function fn) fn))
             (arity (and (subrp def) (subr-arity def))))
        (if (null arity)
            (list 'unknown nil nil)
          (let ((min-a (car arity))
                (max-a (cdr arity)))
            (cond
              ((< nargs min-a)
               (list 'too-few min-a max-a))
              ((and (not (eq max-a 'many)) (> nargs max-a))
               (list 'too-many min-a max-a))
              (t
               (list 'ok min-a max-a))))))))

  (unwind-protect
      (list
        ;; car: exactly 1 arg
        (funcall 'neovm--check-arity 'car 0)
        (funcall 'neovm--check-arity 'car 1)
        (funcall 'neovm--check-arity 'car 2)
        ;; cons: exactly 2 args
        (funcall 'neovm--check-arity 'cons 0)
        (funcall 'neovm--check-arity 'cons 1)
        (funcall 'neovm--check-arity 'cons 2)
        (funcall 'neovm--check-arity 'cons 3)
        ;; +: 0 or more
        (funcall 'neovm--check-arity '+ 0)
        (funcall 'neovm--check-arity '+ 1)
        (funcall 'neovm--check-arity '+ 100)
        ;; substring: (1 . 3) — 1 to 3 args
        (funcall 'neovm--check-arity 'substring 0)
        (funcall 'neovm--check-arity 'substring 1)
        (funcall 'neovm--check-arity 'substring 2)
        (funcall 'neovm--check-arity 'substring 3)
        ;; lambda: unknown arity
        (funcall 'neovm--check-arity (lambda (x) x) 1))
    (fmakunbound 'neovm--check-arity)))"#;
    let expect = expect_test::expect![[
        r#""OK ((too-few 1 1) (ok 1 1) (too-many 1 1) (too-few 2 2) (too-few 2 2) (ok 2 2) (too-many 2 2) (ok 0 many) (ok 0 many) (ok 0 many) (too-few 1 3) (ok 1 3) (ok 1 3) (ok 1 3) (unknown nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
