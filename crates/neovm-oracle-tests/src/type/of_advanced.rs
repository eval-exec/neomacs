//! Advanced oracle parity tests for `type-of`.
//!
//! Tests type-of for all basic types, nil/t/keywords, subrs, lambdas/closures,
//! char-tables, bool-vectors, and a type-based dispatch system.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// type-of for all basic types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_of_all_basic_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-type-catalog
    (lambda ()
      (let* ((ht (make-hash-table :test 'equal))
             (_ (puthash "key" "val" ht))
             (ct (make-char-table 'neovm--test-tcat nil)))
        (list
          ;; Numeric types
          (type-of 0)
          (type-of 42)
          (type-of -999)
          (type-of most-positive-fixnum)
          (type-of most-negative-fixnum)
          (type-of 3.14)
          (type-of -0.0)
          (type-of 1.0e10)
          ;; String types
          (type-of "hello")
          (type-of "")
          (type-of (make-string 5 ?x))
          ;; Symbol
          (type-of 'foo)
          (type-of 'some-long-symbol-name)
          ;; Cons and list
          (type-of '(a . b))
          (type-of '(1 2 3))
          (type-of (cons nil nil))
          ;; Vector
          (type-of [1 2 3])
          (type-of (make-vector 10 0))
          ;; Hash table
          (type-of ht)
          ;; Char table
          (type-of ct)))))
  (unwind-protect
      (funcall 'neovm--test-type-catalog)
    (fmakunbound 'neovm--test-type-catalog)))"#;
    let expect = expect_test::expect![[
        r#""OK (integer integer integer integer integer float float float string string string symbol symbol cons cons cons vector vector hash-table char-table)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// type-of for nil, t, and keywords
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_of_nil_t_keywords() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-special-symbols
    (lambda ()
      (let* ((type-nil (type-of nil))
             (type-t (type-of t))
             (type-kw-test (type-of :test))
             (type-kw-size (type-of :size))
             ;; Verify nil is also symbol
             (nil-is-symbol (symbolp nil))
             (t-is-symbol (symbolp t))
             ;; Keywords are symbols too
             (kw-is-symbol (symbolp :test))
             ;; But keywords have special keywordp predicate
             (kw-is-keyword (keywordp :test))
             (nil-is-keyword (keywordp nil))
             (t-is-keyword (keywordp t))
             ;; type-of compared with eq
             (nil-type-eq-symbol (eq type-nil 'symbol))
             (t-type-eq-symbol (eq type-t 'symbol)))
        (list type-nil type-t type-kw-test type-kw-size
              nil-is-symbol t-is-symbol kw-is-symbol
              kw-is-keyword nil-is-keyword t-is-keyword
              nil-type-eq-symbol t-type-eq-symbol))))
  (unwind-protect
      (funcall 'neovm--test-special-symbols)
    (fmakunbound 'neovm--test-special-symbols)))"#;
    let expect =
        expect_test::expect![[r#""OK (symbol symbol symbol symbol t t t t nil nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// type-of for subrs (built-in functions)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_of_subrs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-subr-types
    (lambda ()
      (list
        ;; Built-in functions
        (type-of (symbol-function '+))
        (type-of (symbol-function 'car))
        (type-of (symbol-function 'cons))
        (type-of (symbol-function 'length))
        (type-of (symbol-function 'type-of))
        ;; subrp predicate
        (subrp (symbol-function '+))
        (subrp (symbol-function 'car))
        ;; type-of for special forms
        (type-of (symbol-function 'if))
        (type-of (symbol-function 'progn))
        (type-of (symbol-function 'let))
        (type-of (symbol-function 'setq))
        ;; Not a subr
        (subrp 42)
        (subrp "hello")
        (subrp nil))))
  (unwind-protect
      (funcall 'neovm--test-subr-types)
    (fmakunbound 'neovm--test-subr-types)))"#;
    let expect = expect_test::expect![[
        r#""OK (subr subr subr subr subr t t subr subr subr subr nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// type-of for lambdas and closures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_of_lambdas_closures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-lambda-types
    (lambda ()
      (let* (;; Plain lambda (no lexical env)
             (plain-lam (lambda (x) (+ x 1)))
             (type-plain (type-of plain-lam))
             ;; Closure via lexical-let pattern
             (maker (lambda (n) (lambda (x) (+ x n))))
             (closure-5 (funcall maker 5))
             (type-closure (type-of closure-5))
             ;; Verify they are functions
             (plain-is-fn (functionp plain-lam))
             (closure-is-fn (functionp closure-5))
             ;; Calling them works
             (plain-result (funcall plain-lam 10))
             (closure-result (funcall closure-5 10))
             ;; Nested closure
             (adder-maker
              (lambda (a)
                (lambda (b)
                  (lambda (c) (+ a b c)))))
             (nested (funcall (funcall adder-maker 1) 2))
             (nested-result (funcall nested 3))
             (type-nested (type-of nested)))
        (list type-plain type-closure
              plain-is-fn closure-is-fn
              plain-result closure-result
              nested-result type-nested))))
  (unwind-protect
      (funcall 'neovm--test-lambda-types)
    (fmakunbound 'neovm--test-lambda-types)))"#;
    let expect = expect_test::expect![[
        r#""OK (interpreted-function interpreted-function t t 11 15 6 interpreted-function)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// type-of for char-tables and bool-vectors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_of_char_tables_bool_vectors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-special-container-types
    (lambda ()
      (let* ((ct1 (make-char-table 'neovm--test-sct nil))
             (ct2 (make-char-table 'neovm--test-sct2 'default-val))
             (bv1 (make-bool-vector 8 nil))
             (bv2 (make-bool-vector 16 t))
             (bv-empty (make-bool-vector 0 nil)))
        ;; Set some char-table entries
        (set-char-table-range ct1 ?A 'letter-a)
        (set-char-table-range ct2 '(65 . 90) 'uppercase)
        (list
          ;; Type identification
          (type-of ct1)
          (type-of ct2)
          (type-of bv1)
          (type-of bv2)
          (type-of bv-empty)
          ;; Predicates
          (char-table-p ct1)
          (char-table-p ct2)
          (bool-vector-p bv1)
          (bool-vector-p bv2)
          ;; Cross-type negative checks
          (char-table-p bv1)
          (bool-vector-p ct1)
          (char-table-p [1 2 3])
          (bool-vector-p [1 2 3])
          ;; Lengths
          (length bv1)
          (length bv2)
          (length bv-empty)))))
  (unwind-protect
      (funcall 'neovm--test-special-container-types)
    (fmakunbound 'neovm--test-special-container-types)))"#;
    let expect = expect_test::expect![[
        r#""OK (char-table char-table bool-vector bool-vector bool-vector t t t t nil nil nil nil 8 16 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// type-of consistency across equal values created differently
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_of_construction_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Verify that type-of is consistent regardless of how a value is constructed
  (fset 'neovm--test-type-consistency
    (lambda ()
      (let* (;; Integers created differently
             (i1 42)
             (i2 (+ 40 2))
             (i3 (string-to-number "42"))
             ;; Strings created differently
             (s1 "hello")
             (s2 (concat "hel" "lo"))
             (s3 (make-string 5 ?h))
             ;; Lists created differently
             (l1 '(1 2 3))
             (l2 (list 1 2 3))
             (l3 (cons 1 (cons 2 (cons 3 nil))))
             ;; Vectors created differently
             (v1 [1 2 3])
             (v2 (vector 1 2 3))
             (v3 (make-vector 3 0)))
        (list
          ;; All integers should have same type
          (eq (type-of i1) (type-of i2))
          (eq (type-of i2) (type-of i3))
          ;; All strings should have same type
          (eq (type-of s1) (type-of s2))
          (eq (type-of s2) (type-of s3))
          ;; All lists (cons cells) should have same type
          (eq (type-of l1) (type-of l2))
          (eq (type-of l2) (type-of l3))
          ;; All vectors should have same type
          (eq (type-of v1) (type-of v2))
          (eq (type-of v2) (type-of v3))
          ;; Cross-type differences
          (eq (type-of i1) (type-of s1))
          (eq (type-of s1) (type-of l1))
          (eq (type-of l1) (type-of v1))
          ;; The actual type names
          (type-of i1) (type-of s1) (type-of l1) (type-of v1)))))
  (unwind-protect
      (funcall 'neovm--test-type-consistency)
    (fmakunbound 'neovm--test-type-consistency)))"#;
    let expect =
        expect_test::expect![[r#""OK (t t t t t t t t nil nil nil integer string cons vector)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: type-based dispatch system using type-of
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_of_dispatch_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Build a dynamic dispatch table that maps type names to handler functions
  (defvar neovm--test-dispatch-table nil)
  (setq neovm--test-dispatch-table (make-hash-table :test 'eq))

  ;; Register handlers for each type
  (fset 'neovm--test-register-handler
    (lambda (type-name handler)
      (puthash type-name handler neovm--test-dispatch-table)))

  ;; Dispatch a value to its handler
  (fset 'neovm--test-dispatch
    (lambda (value)
      (let* ((tp (type-of value))
             (handler (gethash tp neovm--test-dispatch-table)))
        (if handler
            (funcall handler value)
          (list 'unknown tp value)))))

  ;; Serialize any value to a tagged representation
  (fset 'neovm--test-serialize
    (lambda (value)
      (funcall 'neovm--test-dispatch value)))

  ;; Deserialize back from tagged representation
  (fset 'neovm--test-deserialize
    (lambda (tagged)
      (let ((tag (car tagged))
            (payload (cdr tagged)))
        (cond
          ((eq tag 'int) (car payload))
          ((eq tag 'flt) (car payload))
          ((eq tag 'str) (car payload))
          ((eq tag 'sym) (car payload))
          ((eq tag 'lst)
           (mapcar (lambda (item)
                     (funcall 'neovm--test-deserialize item))
                   (car payload)))
          ((eq tag 'vec)
           (apply #'vector
                  (mapcar (lambda (item)
                            (funcall 'neovm--test-deserialize item))
                          (car payload))))
          (t (list 'error 'unknown-tag tag))))))

  (unwind-protect
      (progn
        ;; Register type handlers
        (funcall 'neovm--test-register-handler 'integer
                 (lambda (v) (list 'int v)))
        (funcall 'neovm--test-register-handler 'float
                 (lambda (v) (list 'flt v)))
        (funcall 'neovm--test-register-handler 'string
                 (lambda (v) (list 'str v)))
        (funcall 'neovm--test-register-handler 'symbol
                 (lambda (v) (list 'sym v)))
        (funcall 'neovm--test-register-handler 'cons
                 (lambda (v)
                   (list 'lst (mapcar 'neovm--test-serialize v))))
        (funcall 'neovm--test-register-handler 'vector
                 (lambda (v)
                   (list 'vec (mapcar 'neovm--test-serialize
                                      (append v nil)))))

        ;; Test serialization
        (let* ((data (list 42 3.14 "hello" 'world
                           (list 1 2 3) [4 5 6]))
               (serialized (mapcar 'neovm--test-serialize data))
               ;; Roundtrip test: deserialize each serialized form
               (deserialized (mapcar 'neovm--test-deserialize serialized))
               ;; Verify parity
               (roundtrip-ok (equal data deserialized))
               ;; Dispatch for nested structures
               (nested-data '(1 (2 (3 4)) 5))
               (nested-ser (funcall 'neovm--test-serialize nested-data))
               (nested-des (funcall 'neovm--test-deserialize nested-ser))
               (nested-ok (equal nested-data nested-des))
               ;; Count dispatches by type
               (type-counts (make-hash-table :test 'eq)))
          (dolist (item data)
            (let ((tp (type-of item)))
              (puthash tp (1+ (or (gethash tp type-counts) 0)) type-counts)))
          (list serialized
                roundtrip-ok
                nested-ok
                (gethash 'integer type-counts)
                (gethash 'float type-counts)
                (gethash 'string type-counts)
                (gethash 'symbol type-counts)
                (gethash 'cons type-counts)
                (gethash 'vector type-counts))))
    (fmakunbound 'neovm--test-register-handler)
    (fmakunbound 'neovm--test-dispatch)
    (fmakunbound 'neovm--test-serialize)
    (fmakunbound 'neovm--test-deserialize)
    (makunbound 'neovm--test-dispatch-table)))"#;
    let expect = expect_test::expect![[
        r#""OK (((int 42) (flt 3.14) (str \"hello\") (sym world) (lst ((int 1) (int 2) (int 3))) (vec ((int 4) (int 5) (int 6)))) t t 1 1 1 1 1 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// type-of mapped over heterogeneous collections
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_of_heterogeneous_mapping() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Build a heterogeneous collection and classify each element
  (fset 'neovm--test-classify-collection
    (lambda (items)
      (let ((type-groups (make-hash-table :test 'eq)))
        ;; Group items by type
        (dolist (item items)
          (let* ((tp (type-of item))
                 (existing (gethash tp type-groups)))
            (puthash tp (cons item existing) type-groups)))
        ;; Build sorted summary
        (let ((summary nil))
          (maphash (lambda (tp items)
                     (setq summary
                           (cons (list tp (length items) (nreverse items))
                                 summary)))
                   type-groups)
          ;; Sort by type name for deterministic output
          (sort summary (lambda (a b) (string< (symbol-name (car a))
                                                (symbol-name (car b)))))))))
  (unwind-protect
      (let ((collection (list 1 2 3 "a" "b" 'x 'y 'z
                              3.14 2.71 '(p q) '(r s t)
                              [1 2] [3])))
        (let* ((types-list (mapcar #'type-of collection))
               (unique-types (let ((seen nil))
                               (dolist (tp types-list)
                                 (unless (memq tp seen)
                                   (setq seen (cons tp seen))))
                               (sort seen (lambda (a b)
                                            (string< (symbol-name a)
                                                     (symbol-name b))))))
               (grouped (funcall 'neovm--test-classify-collection collection))
               (total-items (apply #'+ (mapcar (lambda (g) (nth 1 g)) grouped))))
          (list types-list unique-types grouped total-items)))
    (fmakunbound 'neovm--test-classify-collection)))"#;
    let expect = expect_test::expect![[
        r#""OK ((integer integer integer string string symbol symbol symbol float float cons cons vector vector) (cons float integer string symbol vector) ((cons 2 ((p q) (r s t))) (float 2 (3.14 2.71)) (integer 3 (1 2 3)) (string 2 (\"a\" \"b\")) (symbol 3 (x y z)) (vector 2 ([1 2] [3]))) 14)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
