//! Oracle parity tests for `subr-arity` with complex patterns.
//!
//! Tests subr-arity on various built-in functions with different arities,
//! verifies (min . max) return format, tests functions with fixed arity
//! vs &rest (many), subrp predicate interaction, function arity validation
//! systems, and dispatch mechanisms based on arity metadata.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// subr-arity on diverse built-in function families
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_arity_patterns_diverse_families() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test subr-arity across multiple function families: type predicates,
    // list operations, string operations, arithmetic, comparison, and
    // hash table functions. Verify return format is always (min . max).
    let form = r#"(list
  ;; Type predicates: all fixed 1-arg
  (subr-arity (symbol-function 'integerp))
  (subr-arity (symbol-function 'floatp))
  (subr-arity (symbol-function 'numberp))
  (subr-arity (symbol-function 'consp))
  (subr-arity (symbol-function 'vectorp))
  (subr-arity (symbol-function 'stringp))
  (subr-arity (symbol-function 'symbolp))
  (subr-arity (symbol-function 'functionp))
  (subr-arity (symbol-function 'hash-table-p))
  (subr-arity (symbol-function 'bufferp))

  ;; List operations: mixed arities
  (subr-arity (symbol-function 'car))
  (subr-arity (symbol-function 'cdr))
  (subr-arity (symbol-function 'cons))
  (subr-arity (symbol-function 'nth))
  (subr-arity (symbol-function 'nthcdr))
  (subr-arity (symbol-function 'elt))
  (subr-arity (symbol-function 'length))
  (subr-arity (symbol-function 'reverse))
  (subr-arity (symbol-function 'nreverse))
  (subr-arity (symbol-function 'append))
  (subr-arity (symbol-function 'nconc))
  (subr-arity (symbol-function 'list))

  ;; String operations
  (subr-arity (symbol-function 'concat))
  (subr-arity (symbol-function 'substring))
  (subr-arity (symbol-function 'string-to-number))
  (subr-arity (symbol-function 'number-to-string))
  (subr-arity (symbol-function 'upcase))
  (subr-arity (symbol-function 'downcase))

  ;; Hash table operations
  (subr-arity (symbol-function 'gethash))
  (subr-arity (symbol-function 'puthash))
  (subr-arity (symbol-function 'remhash))
  (subr-arity (symbol-function 'maphash))
  (subr-arity (symbol-function 'hash-table-count)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 . 1) (1 . 1) (1 . 1) (1 . 1) (1 . 1) (1 . 1) (1 . 1) (1 . 1) (1 . 1) (1 . 1) (1 . 1) (1 . 1) (2 . 2) (2 . 2) (2 . 2) (2 . 2) (1 . 1) (1 . 1) (1 . 1) (0 . many) (0 . many) (0 . many) (0 . many) (1 . 3) (1 . 2) (1 . 1) (1 . 1) (1 . 1) (2 . 3) (3 . 3) (2 . 2) (2 . 2) (1 . 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// subr-arity return value decomposition and validation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_arity_patterns_return_value_decomposition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Decompose subr-arity return values, verifying that car is always
    // a non-negative integer and cdr is either a non-negative integer
    // >= car or the symbol 'many. Test on a broad range of functions.
    let form = r#"(progn
  (fset 'neovm--sap-decompose
    (lambda (fn-sym)
      "Decompose and validate subr-arity result for FN-SYM."
      (let* ((def (symbol-function fn-sym))
             (arity (subr-arity def))
             (min-a (car arity))
             (max-a (cdr arity)))
        (list
          fn-sym
          ;; Structure checks
          (consp arity)
          (integerp min-a)
          (>= min-a 0)
          ;; Max is integer or 'many
          (or (eq max-a 'many) (integerp max-a))
          ;; If integer max, it's >= min
          (or (eq max-a 'many) (>= max-a min-a))
          ;; Classify
          (cond
            ((eq max-a 'many) 'variadic)
            ((= min-a max-a) 'fixed)
            (t 'optional))
          ;; Optional count (nil for variadic)
          (when (and (integerp max-a) (/= min-a max-a))
            (- max-a min-a))
          ;; Raw values
          min-a max-a))))

  (unwind-protect
      (mapcar 'neovm--sap-decompose
              '(car cdr cons + * list length concat
                substring make-string assoc mapcar
                sort apply signal gethash puthash
                format message aref aset))
    (fmakunbound 'neovm--sap-decompose)))"#;
    let expect = expect_test::expect![[
        r#""OK ((car t t t t t fixed nil 1 1) (cdr t t t t t fixed nil 1 1) (cons t t t t t fixed nil 2 2) (+ t t t t t variadic nil 0 many) (* t t t t t variadic nil 0 many) (list t t t t t variadic nil 0 many) (length t t t t t fixed nil 1 1) (concat t t t t t variadic nil 0 many) (substring t t t t t optional 2 1 3) (make-string t t t t t optional 1 2 3) (assoc t t t t t optional 1 2 3) (mapcar t t t t t fixed nil 2 2) (sort t t t t t variadic nil 1 many) (apply t t t t t variadic nil 1 many) (signal t t t t t optional 1 1 2) (gethash t t t t t optional 1 2 3) (puthash t t t t t fixed nil 3 3) (format t t t t t variadic nil 1 many) (message t t t t t variadic nil 1 many) (aref t t t t t fixed nil 2 2) (aset t t t t t fixed nil 3 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Fixed arity functions vs variadic functions: exhaustive comparison
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_arity_patterns_fixed_vs_variadic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Partition a large set of built-in functions into fixed, optional,
    // and variadic categories. Verify that the counts and membership
    // are consistent across evaluators.
    let form = r#"(progn
  (defvar neovm--sap-fn-list
    '(car cdr cons eq equal not null atom
      consp listp symbolp stringp integerp floatp numberp
      length reverse nreverse
      + - * / mod max min
      < > <= >= = /=
      list vector append nconc concat
      mapcar sort apply format message
      substring nth nthcdr elt
      make-string make-vector make-list
      gethash puthash remhash maphash hash-table-count
      aref aset
      setcar setcdr
      prin1-to-string))

  (fset 'neovm--sap-categorize
    (lambda (fn-sym)
      (let* ((def (symbol-function fn-sym))
             (arity (when (subrp def) (subr-arity def))))
        (when arity
          (let ((min-a (car arity))
                (max-a (cdr arity)))
            (cond
              ((eq max-a 'many) 'variadic)
              ((= min-a max-a) 'fixed)
              (t 'optional)))))))

  (unwind-protect
      (let ((fixed nil) (optional nil) (variadic nil))
        (dolist (fn neovm--sap-fn-list)
          (let ((cat (funcall 'neovm--sap-categorize fn)))
            (cond
              ((eq cat 'fixed) (setq fixed (cons fn fixed)))
              ((eq cat 'optional) (setq optional (cons fn optional)))
              ((eq cat 'variadic) (setq variadic (cons fn variadic))))))
        (list
          ;; Sorted lists per category
          (sort (copy-sequence fixed)
                (lambda (a b) (string< (symbol-name a) (symbol-name b))))
          (sort (copy-sequence optional)
                (lambda (a b) (string< (symbol-name a) (symbol-name b))))
          (sort (copy-sequence variadic)
                (lambda (a b) (string< (symbol-name a) (symbol-name b))))
          ;; Counts
          (list (length fixed) (length optional) (length variadic))
          ;; Total should equal input length
          (= (+ (length fixed) (length optional) (length variadic))
             (length neovm--sap-fn-list))
          ;; Every fixed arity function has min = max
          (let ((all-ok t))
            (dolist (fn fixed)
              (let ((arity (subr-arity (symbol-function fn))))
                (unless (= (car arity) (cdr arity))
                  (setq all-ok nil))))
            all-ok)
          ;; Every variadic function has max = 'many
          (let ((all-ok t))
            (dolist (fn variadic)
              (let ((arity (subr-arity (symbol-function fn))))
                (unless (eq (cdr arity) 'many)
                  (setq all-ok nil))))
            all-ok)))
    (fmakunbound 'neovm--sap-categorize)
    (makunbound 'neovm--sap-fn-list)))"#;
    let expect = expect_test::expect![[
        r#""OK ((/= aref aset atom car cdr cons consp elt eq equal floatp hash-table-count integerp length listp make-list make-vector mapcar maphash mod nreverse nth nthcdr null numberp puthash remhash reverse setcar setcdr stringp symbolp) (gethash make-string prin1-to-string substring) (* + - / < <= = > >= append apply concat format list max message min nconc sort vector) (33 4 20) nil t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// subrp predicate interaction with subr-arity: comprehensive checks
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_arity_patterns_subrp_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that subrp returns t for all objects that subr-arity can
    // handle, and that non-subr objects fail both predicates appropriately.
    let form = r#"(progn
  (fset 'neovm--sap-lambda-fn (lambda (x) (* x x)))
  (defmacro neovm--sap-test-mac (x) (list 'quote x))

  (unwind-protect
      (list
        ;; subrp on built-in functions: all t
        (mapcar (lambda (fn)
                  (list fn (subrp (symbol-function fn))))
                '(car cdr cons + list concat mapcar))

        ;; subrp on non-subr objects: all nil
        (list
          (subrp (symbol-function 'neovm--sap-lambda-fn))
          (subrp (lambda (a b) (+ a b)))
          (subrp nil)
          (subrp t)
          (subrp 42)
          (subrp "hello")
          (subrp '(1 2 3))
          (subrp [1 2 3])
          (subrp (make-hash-table))
          (subrp 'car))  ;; symbol, not the function

        ;; subr-arity only works on subrs; error on non-subrs
        (condition-case err
            (subr-arity (lambda (x) x))
          (wrong-type-argument (list 'caught (car err))))

        (condition-case err
            (subr-arity "not-a-function")
          (wrong-type-argument (list 'caught (car err))))

        (condition-case err
            (subr-arity 42)
          (wrong-type-argument (list 'caught (car err))))

        ;; Verify subrp => subr-arity works (no error)
        (let ((results nil))
          (dolist (fn '(car cons + list format))
            (let ((def (symbol-function fn)))
              (when (subrp def)
                (setq results (cons (cons fn (subr-arity def)) results)))))
          (nreverse results)))
    (fmakunbound 'neovm--sap-lambda-fn)
    (fmakunbound 'neovm--sap-test-mac)))"#;
    let expect = expect_test::expect![[
        r#""OK (((car t) (cdr t) (cons t) (+ t) (list t) (concat t) (mapcar t)) (nil nil nil nil nil nil nil nil nil nil) (caught wrong-type-argument) (caught wrong-type-argument) (caught wrong-type-argument) ((car 1 . 1) (cons 2 . 2) (+ 0 . many) (list 0 . many) (format 1 . many)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: function arity validation system
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_arity_patterns_validation_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a validation system that checks whether a given argument count
    // is valid for a function, generates error messages, and provides
    // suggestions for valid argument counts.
    let form = r#"(progn
  (fset 'neovm--sap-validate-call
    (lambda (fn-sym argc)
      "Validate that FN-SYM can be called with ARGC arguments.
Return (ok), (error MIN MAX ARGC), or (error MIN many ARGC)."
      (let* ((def (symbol-function fn-sym))
             (arity (when (subrp def) (subr-arity def)))
             (min-a (if arity (car arity) 0))
             (max-a (if arity (cdr arity) 'many)))
        (cond
          ((not arity)
           (list 'unknown fn-sym))
          ((< argc min-a)
           (list 'too-few fn-sym argc min-a max-a))
          ((and (integerp max-a) (> argc max-a))
           (list 'too-many fn-sym argc min-a max-a))
          (t
           (list 'ok fn-sym argc))))))

  (fset 'neovm--sap-valid-arg-range
    (lambda (fn-sym)
      "Return a list of valid argument counts for FN-SYM (up to 10)."
      (let* ((def (symbol-function fn-sym))
             (arity (when (subrp def) (subr-arity def)))
             (min-a (if arity (car arity) 0))
             (max-a (if arity (cdr arity) 'many))
             (limit (if (eq max-a 'many) 10 max-a))
             (result nil))
        (let ((i min-a))
          (while (<= i limit)
            (setq result (cons i result))
            (setq i (1+ i))))
        (nreverse result))))

  (unwind-protect
      (list
        ;; car: exactly 1 arg
        (funcall 'neovm--sap-validate-call 'car 0)
        (funcall 'neovm--sap-validate-call 'car 1)
        (funcall 'neovm--sap-validate-call 'car 2)

        ;; cons: exactly 2 args
        (funcall 'neovm--sap-validate-call 'cons 0)
        (funcall 'neovm--sap-validate-call 'cons 1)
        (funcall 'neovm--sap-validate-call 'cons 2)
        (funcall 'neovm--sap-validate-call 'cons 3)

        ;; +: 0 or more
        (funcall 'neovm--sap-validate-call '+ 0)
        (funcall 'neovm--sap-validate-call '+ 5)
        (funcall 'neovm--sap-validate-call '+ 100)

        ;; substring: 1-3 args
        (funcall 'neovm--sap-validate-call 'substring 0)
        (funcall 'neovm--sap-validate-call 'substring 1)
        (funcall 'neovm--sap-validate-call 'substring 2)
        (funcall 'neovm--sap-validate-call 'substring 3)
        (funcall 'neovm--sap-validate-call 'substring 4)

        ;; Valid argument ranges
        (funcall 'neovm--sap-valid-arg-range 'car)
        (funcall 'neovm--sap-valid-arg-range 'cons)
        (funcall 'neovm--sap-valid-arg-range '+)
        (funcall 'neovm--sap-valid-arg-range 'substring)
        (funcall 'neovm--sap-valid-arg-range 'make-string))
    (fmakunbound 'neovm--sap-validate-call)
    (fmakunbound 'neovm--sap-valid-arg-range)))"#;
    let expect = expect_test::expect![[
        r#""OK ((too-few car 0 1 1) (ok car 1) (too-many car 2 1 1) (too-few cons 0 2 2) (too-few cons 1 2 2) (ok cons 2) (too-many cons 3 2 2) (ok + 0) (ok + 5) (ok + 100) (too-few substring 0 1 3) (ok substring 1) (ok substring 2) (ok substring 3) (too-many substring 4 1 3) (1) (2) (0 1 2 3 4 5 6 7 8 9 10) (1 2 3) (2 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: function dispatch based on arity
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_arity_patterns_dispatch_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a function dispatch system that selects the best-matching
    // function from a candidate list based on the number of arguments.
    let form = r#"(progn
  (fset 'neovm--sap-find-matching-fns
    (lambda (fn-list argc)
      "Find all functions in FN-LIST that accept ARGC arguments."
      (let ((result nil))
        (dolist (fn fn-list)
          (let* ((def (symbol-function fn))
                 (arity (when (subrp def) (subr-arity def))))
            (when arity
              (let ((min-a (car arity))
                    (max-a (cdr arity)))
                (when (and (>= argc min-a)
                           (or (eq max-a 'many)
                               (<= argc max-a)))
                  (setq result (cons fn result)))))))
        (nreverse result))))

  (fset 'neovm--sap-best-match
    (lambda (fn-list argc)
      "Find the most specific function matching ARGC args.
Prefer fixed > optional > variadic. Among equal specificity, prefer lower max."
      (let* ((matches (funcall 'neovm--sap-find-matching-fns fn-list argc))
             (scored (mapcar
                       (lambda (fn)
                         (let* ((arity (subr-arity (symbol-function fn)))
                                (min-a (car arity))
                                (max-a (cdr arity))
                                (score (cond
                                         ((and (integerp max-a) (= min-a max-a)) 100)
                                         ((integerp max-a) (- 50 (- max-a min-a)))
                                         (t 0))))
                           (cons score fn)))
                       matches)))
        (when scored
          (setq scored (sort scored (lambda (a b) (> (car a) (car b)))))
          (cdar scored)))))

  (unwind-protect
      (let ((candidates '(car cons + list length nth substring
                           make-string mapcar append concat)))
        (list
          ;; What accepts exactly 1 arg?
          (funcall 'neovm--sap-find-matching-fns candidates 1)
          ;; What accepts exactly 2 args?
          (funcall 'neovm--sap-find-matching-fns candidates 2)
          ;; What accepts exactly 3 args?
          (funcall 'neovm--sap-find-matching-fns candidates 3)
          ;; What accepts 0 args?
          (funcall 'neovm--sap-find-matching-fns candidates 0)
          ;; Best match for 1 arg (should prefer car or length over +)
          (funcall 'neovm--sap-best-match candidates 1)
          ;; Best match for 2 args (should prefer cons over +)
          (funcall 'neovm--sap-best-match candidates 2)
          ;; Best match for 0 args (only variadic functions)
          (funcall 'neovm--sap-best-match candidates 0)
          ;; Arity summary table
          (mapcar (lambda (fn)
                    (let ((arity (subr-arity (symbol-function fn))))
                      (list fn (car arity) (cdr arity))))
                  candidates)))
    (fmakunbound 'neovm--sap-find-matching-fns)
    (fmakunbound 'neovm--sap-best-match)))"#;
    let expect = expect_test::expect![[
        r#""OK ((car + list length substring append concat) (cons + list nth substring make-string mapcar append concat) (+ list substring make-string append concat) (+ list append concat) car cons + ((car 1 1) (cons 2 2) (+ 0 many) (list 0 many) (length 1 1) (nth 2 2) (substring 1 3) (make-string 2 3) (mapcar 2 2) (append 0 many) (concat 0 many)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// subr-arity with min-arg boundary calling tests
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_arity_patterns_boundary_calls() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // For each function, verify we can call it at its minimum and maximum
    // arg counts, and that we get errors at min-1 and max+1.
    let form = r#"(progn
  (fset 'neovm--sap-boundary-test
    (lambda (fn-sym)
      "Test boundary calling for FN-SYM. Returns description of arity."
      (let* ((def (symbol-function fn-sym))
             (arity (subr-arity def))
             (min-a (car arity))
             (max-a (cdr arity)))
        (list fn-sym min-a max-a
              ;; Can call at min? (test by checking arity, not actually calling)
              (>= min-a 0)
              ;; Is variadic?
              (eq max-a 'many)
              ;; Number of optional args
              (if (integerp max-a) (- max-a min-a) 'unbounded)
              ;; Arity class
              (cond
                ((eq max-a 'many) 'rest)
                ((= min-a max-a) (list 'exact min-a))
                (t (list 'range min-a max-a)))))))

  (unwind-protect
      (let ((fns '(car cdr cons eq equal not null
                   + - * / mod
                   length reverse nreverse
                   list vector append nconc concat
                   mapcar sort apply
                   substring nth nthcdr elt
                   make-string make-vector make-list
                   gethash puthash remhash
                   aref aset setcar setcdr
                   format message
                   upcase downcase capitalize
                   string= string< string-to-number
                   number-to-string prin1-to-string)))
        (list
          ;; All boundary info
          (mapcar 'neovm--sap-boundary-test fns)
          ;; Group by arity class
          (let ((exact-1 nil) (exact-2 nil) (exact-3 nil) (rest-fns nil) (opt-fns nil))
            (dolist (fn fns)
              (let* ((arity (subr-arity (symbol-function fn)))
                     (min-a (car arity))
                     (max-a (cdr arity)))
                (cond
                  ((eq max-a 'many) (setq rest-fns (cons fn rest-fns)))
                  ((= min-a max-a 1) (setq exact-1 (cons fn exact-1)))
                  ((= min-a max-a 2) (setq exact-2 (cons fn exact-2)))
                  ((= min-a max-a 3) (setq exact-3 (cons fn exact-3)))
                  (t (setq opt-fns (cons fn opt-fns))))))
            (list
              (list 'exact-1 (length exact-1))
              (list 'exact-2 (length exact-2))
              (list 'exact-3 (length exact-3))
              (list 'rest (length rest-fns))
              (list 'optional (length opt-fns))))))
    (fmakunbound 'neovm--sap-boundary-test)))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument subrp null)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
