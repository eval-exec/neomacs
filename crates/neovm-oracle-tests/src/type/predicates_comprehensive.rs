//! Oracle parity tests for comprehensive type predicates:
//! `type-of` for all types, numeric predicates (`integerp`, `floatp`,
//! `numberp`, `natnump`, `fixnump`, `bignump`), collection predicates
//! (`stringp`, `symbolp`, `consp`, `listp`, `nlistp`, `atom`, `vectorp`,
//! `sequencep`, `arrayp`), function predicates (`functionp`, `subrp`,
//! `commandp`), boolean/keyword predicates (`null`, `booleanp`, `keywordp`),
//! special predicates (`characterp`, `char-table-p`, `hash-table-p`,
//! `markerp`, `bufferp`), and complex type checking on mixed data.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// type-of for all types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_of_all_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test type-of on every fundamental Elisp type.
    let form = r####"(list
  (type-of 0)
  (type-of 42)
  (type-of -1)
  (type-of most-positive-fixnum)
  (type-of most-negative-fixnum)
  (type-of 3.14)
  (type-of -0.0)
  (type-of 1.0e+INF)
  (type-of "hello")
  (type-of "")
  (type-of 'foo)
  (type-of nil)
  (type-of t)
  (type-of :keyword)
  (type-of '(1 2 3))
  (type-of '(a . b))
  (type-of [1 2 3])
  (type-of [])
  (type-of (lambda (x) x))
  (type-of (make-hash-table))
  (type-of (make-bool-vector 8 nil))
  (type-of #'car)
  (type-of ?a)
  (type-of (make-symbol "uninterned")))
"####;
    let expect = expect_test::expect![[
        r#""OK (integer integer integer integer integer float float float string string symbol symbol symbol symbol cons cons vector vector interpreted-function hash-table bool-vector symbol integer symbol)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Numeric predicates: integerp, floatp, numberp, natnump, fixnump, bignump
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numeric_predicates_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test all numeric predicates across boundary values, floats, non-numbers.
    let form = r####"(let ((values (list 0 1 -1 42 -42
                           most-positive-fixnum
                           most-negative-fixnum
                           (1+ most-positive-fixnum)
                           (1- most-negative-fixnum)
                           0.0 3.14 -2.7 1.0e10
                           1.0e+INF -1.0e+INF
                           0.0e+NaN
                           nil t 'foo "42" '(1) [1])))
  (mapcar (lambda (v)
            (list (integerp v) (floatp v) (numberp v)
                  (natnump v) (fixnump v)
                  ;; bignump may not exist in all versions, wrap safely
                  (condition-case nil (bignump v) (void-function 'unknown))))
          values))
"####;
    let expect = expect_test::expect![[
        r#""OK ((t nil t t t nil) (t nil t t t nil) (t nil t nil t nil) (t nil t t t nil) (t nil t nil t nil) (t nil t t t nil) (t nil t nil t nil) (t nil t t nil t) (t nil t nil nil t) (nil t t nil nil nil) (nil t t nil nil nil) (nil t t nil nil nil) (nil t t nil nil nil) (nil t t nil nil nil) (nil t t nil nil nil) (nil t t nil nil nil) (nil nil nil nil nil nil) (nil nil nil nil nil nil) (nil nil nil nil nil nil) (nil nil nil nil nil nil) (nil nil nil nil nil nil) (nil nil nil nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_numeric_predicates_edge_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Edge case: natnump on zero and negative, fixnump boundaries,
    // numberp on chars (chars are integers in Elisp).
    let form = r####"(list
  ;; natnump: non-negative integers
  (natnump 0)
  (natnump 1)
  (natnump -1)
  (natnump most-positive-fixnum)
  (natnump 0.0)  ;; float is NOT natnump
  ;; Characters are integers
  (integerp ?a)
  (natnump ?a)
  (numberp ?a)
  (fixnump ?a)
  ;; Arithmetic results
  (integerp (+ 1 2))
  (floatp (+ 1 2.0))
  (floatp (/ 1.0 3))
  (integerp (/ 10 3))  ;; integer division
  ;; numberp includes both
  (numberp (+ 1 2))
  (numberp 3.14)
  (numberp nil))
"####;
    let expect = expect_test::expect![[r#""OK (t t nil t nil t t t t t t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// String, symbol, cons, list predicates
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_collection_predicates_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test stringp, symbolp, consp, listp, nlistp, atom across all types.
    let form = r####"(let ((values (list nil t 'foo :kw
                           0 3.14
                           "hello" ""
                           '(1 2 3) '(a . b) '()
                           [1 2] []
                           (lambda () nil)
                           (make-hash-table)
                           (make-symbol "unint"))))
  (mapcar (lambda (v)
            (list (stringp v) (symbolp v) (consp v)
                  (listp v) (nlistp v) (atom v)))
          values))
"####;
    let expect = expect_test::expect![[
        r#""OK ((nil t nil t nil t) (nil t nil nil t t) (nil t nil nil t t) (nil t nil nil t t) (nil nil nil nil t t) (nil nil nil nil t t) (t nil nil nil t t) (t nil nil nil t t) (nil nil t t nil nil) (nil nil t t nil nil) (nil t nil t nil t) (nil nil nil nil t t) (nil nil nil nil t t) (nil nil nil nil t t) (nil nil nil nil t t) (nil t nil nil t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_list_predicate_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // listp: nil is a list, consp: nil is NOT a cons, nlistp: nil is not nlist
    let form = r####"(list
  ;; nil is special: list but not cons
  (listp nil) (consp nil) (nlistp nil) (atom nil)
  ;; dotted pair is a cons and a list
  (listp '(a . b)) (consp '(a . b)) (nlistp '(a . b)) (atom '(a . b))
  ;; proper list
  (listp '(1 2 3)) (consp '(1 2 3))
  ;; nested
  (listp '((a) (b) (c)))
  (consp (car '((a) (b))))
  ;; single element
  (listp '(x)) (consp '(x))
  ;; improper list
  (listp '(1 2 . 3))
  (consp '(1 2 . 3))
  ;; constructed
  (let ((c (cons 'a 'b)))
    (list (consp c) (listp c) (atom c)))
  ;; nlistp on non-lists
  (nlistp 42) (nlistp "hi") (nlistp [1 2]) (nlistp 'sym))
"####;
    let expect = expect_test::expect![[
        r#""OK (t nil nil t t t nil nil t t t t t t t t (t t nil) t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// vectorp, sequencep, arrayp
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sequence_array_predicates_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // sequencep = list OR vector OR string OR bool-vector
    // arrayp = vector OR string OR bool-vector OR char-table
    // vectorp = only vectors
    let form = r####"(let ((values (list nil '(1 2 3) '(a . b)
                           [1 2 3] [] "hello" ""
                           (make-bool-vector 4 t)
                           42 3.14 'sym :kw
                           (make-hash-table)
                           (lambda () nil))))
  (mapcar (lambda (v)
            (list (sequencep v) (arrayp v) (vectorp v)))
          values))
"####;
    let expect = expect_test::expect![[
        r#""OK ((t nil nil) (t nil nil) (t nil nil) (t t t) (t t t) (t t nil) (t t nil) (t t nil) (nil nil nil) (nil nil nil) (nil nil nil) (nil nil nil) (nil nil nil) (nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// functionp, subrp, commandp
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_function_predicates_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // functionp: lambdas, closures, subrps, byte-compiled
    // subrp: only C-level built-in functions
    // commandp: interactive commands
    let form = r####"(list
  ;; Lambdas
  (functionp (lambda (x) x))
  (functionp (lambda (&rest args) args))
  (subrp (lambda (x) x))
  ;; Built-ins
  (functionp #'car)
  (functionp #'+)
  (subrp #'car)
  (subrp #'+)
  (subrp #'cons)
  ;; Closures
  (let ((x 10))
    (let ((f (lambda () x)))
      (list (functionp f) (subrp f))))
  ;; Symbols that have function bindings
  (functionp 'car)  ;; symbol, not function itself
  ;; Non-functions
  (functionp nil)
  (functionp 42)
  (functionp "hello")
  (functionp '(1 2))
  (functionp [1 2])
  ;; commandp on interactive vs non-interactive
  (commandp #'car)
  (commandp 'save-buffer)
  ;; Macros are not functionp
  (functionp (symbol-function 'when)))
"####;
    let expect = expect_test::expect![[
        r#""OK (t t nil t t nil nil nil (t nil) t nil nil nil nil nil nil t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// null, booleanp, keywordp
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_boolean_keyword_predicates_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // null = same as (eq x nil), booleanp = (or (eq x nil) (eq x t)),
    // keywordp: symbols starting with :
    let form = r####"(let ((values (list nil t 0 1 -1 "" "nil" 'nil 'foo
                           :foo :bar : '() '(nil) [nil]
                           (intern ":manual-kw"))))
  (list
    ;; null
    (mapcar #'null values)
    ;; booleanp
    (mapcar #'booleanp values)
    ;; keywordp
    (mapcar #'keywordp values)
    ;; Special: results of comparison are booleans
    (booleanp (= 1 1))
    (booleanp (string= "a" "b"))
    ;; null and not are equivalent
    (list (null nil) (not nil)
          (null t) (not t)
          (null 0) (not 0)
          (null '()) (not '()))))
"####;
    let expect = expect_test::expect![[
        r#""OK ((t nil nil nil nil nil nil t nil nil nil nil t nil nil nil) (t t nil nil nil nil nil t nil nil nil nil t nil nil nil) (nil nil nil nil nil nil nil nil nil t t t nil nil nil t) t t (t t nil nil nil nil t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// characterp, char-table-p, hash-table-p
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_special_type_predicates_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // characterp: valid character (integer in valid range)
    // char-table-p: char-tables
    // hash-table-p: hash-tables
    let form = r####"(list
  ;; characterp: integers 0 to #x3FFFFF
  (characterp 0)
  (characterp 65)
  (characterp ?a)
  (characterp ?Z)
  (characterp ?\n)
  (characterp #x10ffff)
  (characterp #x110000)
  (characterp -1)
  (characterp nil)
  (characterp "a")
  (characterp 'a)
  ;; Large value beyond character range
  (characterp #x3fffff)
  (characterp #x400000)
  ;; char-table-p
  (char-table-p (make-char-table 'test))
  (char-table-p [1 2 3])
  (char-table-p nil)
  (char-table-p '(a . b))
  ;; hash-table-p
  (hash-table-p (make-hash-table))
  (hash-table-p (make-hash-table :test 'equal :size 100))
  (hash-table-p nil)
  (hash-table-p '((a . 1)))
  (hash-table-p [1 2 3]))
"####;
    let expect = expect_test::expect![[
        r#""OK (t t t t t t t nil nil nil nil t nil t nil nil nil t t nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: type-safe heterogeneous container
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_safe_container_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a type-safe container that stores values with type tags and
    // provides type-checked access, using all predicates for validation.
    let form = r####"(progn
  (fset 'neovm--tpc-tag
    (lambda (val)
      (cond
        ((null val)      'null)
        ((booleanp val)  'boolean)
        ((integerp val)  'integer)
        ((floatp val)    'float)
        ((stringp val)   'string)
        ((keywordp val)  'keyword)
        ((symbolp val)   'symbol)
        ((functionp val) 'function)
        ((vectorp val)   'vector)
        ((consp val)     'cons)
        ((hash-table-p val) 'hash-table)
        (t               'unknown))))

  (fset 'neovm--tpc-validate
    (lambda (val expected-type)
      (let ((actual (funcall 'neovm--tpc-tag val)))
        (if (eq actual expected-type)
            (cons t val)
          (cons nil (list 'type-error actual expected-type))))))

  (fset 'neovm--tpc-coerce
    (lambda (val target-type)
      (let ((source (funcall 'neovm--tpc-tag val)))
        (cond
          ;; same type: identity
          ((eq source target-type) val)
          ;; to string
          ((eq target-type 'string)
           (cond ((integerp val) (number-to-string val))
                 ((floatp val) (number-to-string val))
                 ((symbolp val) (symbol-name val))
                 (t (prin1-to-string val))))
          ;; to integer
          ((eq target-type 'integer)
           (cond ((floatp val) (truncate val))
                 ((stringp val) (string-to-number val))
                 (t nil)))
          ;; to float
          ((eq target-type 'float)
           (cond ((integerp val) (float val))
                 ((stringp val) (float (string-to-number val)))
                 (t nil)))
          ;; to boolean
          ((eq target-type 'boolean)
           (not (null val)))
          (t nil)))))

  (unwind-protect
      (let ((data (list 42 3.14 "hello" nil t :config 'sym
                        (lambda (x) x) [1 2 3] '(a b c)
                        (make-hash-table))))
        (list
          ;; Tag all values
          (mapcar (lambda (v) (funcall 'neovm--tpc-tag v)) data)
          ;; Validate correct types
          (funcall 'neovm--tpc-validate 42 'integer)
          (funcall 'neovm--tpc-validate 42 'string)
          (funcall 'neovm--tpc-validate "hi" 'string)
          (funcall 'neovm--tpc-validate nil 'null)
          ;; Coerce chain: integer -> float -> string -> integer
          (let* ((v1 42)
                 (v2 (funcall 'neovm--tpc-coerce v1 'float))
                 (v3 (funcall 'neovm--tpc-coerce v2 'string))
                 (v4 (funcall 'neovm--tpc-coerce v3 'integer)))
            (list v1 v2 v3 v4
                  (funcall 'neovm--tpc-tag v1)
                  (funcall 'neovm--tpc-tag v2)
                  (funcall 'neovm--tpc-tag v3)
                  (funcall 'neovm--tpc-tag v4)))
          ;; Coerce various to boolean
          (mapcar (lambda (v) (funcall 'neovm--tpc-coerce v 'boolean))
                  (list nil 0 "" t 42 "hi" '(1)))
          ;; Coerce various to string
          (mapcar (lambda (v) (funcall 'neovm--tpc-coerce v 'string))
                  (list 42 3.14 'hello :kw nil))))
    (fmakunbound 'neovm--tpc-tag)
    (fmakunbound 'neovm--tpc-validate)
    (fmakunbound 'neovm--tpc-coerce)))
"####;
    let expect = expect_test::expect![[
        r#""OK ((integer float string null boolean keyword symbol function vector cons hash-table) (t . 42) (nil type-error integer string) (t . \"hi\") (t) (42 42.0 \"42.0\" 42.0 integer float string float) (nil t t t t t t) (\"42\" \"3.14\" \"hello\" \":kw\" \"nil\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: type-based pattern matching dispatch
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_pattern_matching() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a multi-method dispatch system that selects implementation
    // based on argument types.
    let form = r####"(progn
  ;; Dispatch table: ((type-pattern . function) ...)
  ;; type-pattern: list of type symbols, one per arg
  (fset 'neovm--tpm-typeof
    (lambda (v)
      (cond ((integerp v) 'integer)
            ((floatp v) 'float)
            ((stringp v) 'string)
            ((listp v) 'list)
            ((vectorp v) 'vector)
            (t 'other))))

  (fset 'neovm--tpm-match-types
    (lambda (args pattern)
      (if (not (= (length args) (length pattern)))
          nil
        (let ((ok t) (a args) (p pattern))
          (while (and ok a)
            (unless (eq (funcall 'neovm--tpm-typeof (car a)) (car p))
              (setq ok nil))
            (setq a (cdr a) p (cdr p)))
          ok))))

  (fset 'neovm--tpm-dispatch
    (lambda (table args)
      (let ((found nil) (remaining table))
        (while (and (not found) remaining)
          (let ((entry (car remaining)))
            (when (funcall 'neovm--tpm-match-types args (car entry))
              (setq found (cdr entry))))
          (setq remaining (cdr remaining)))
        (if found
            (apply found args)
          (list 'no-match (mapcar (lambda (a) (funcall 'neovm--tpm-typeof a)) args))))))

  (unwind-protect
      (let ((add-table
             (list
               (cons '(integer integer) (lambda (a b) (+ a b)))
               (cons '(float float) (lambda (a b) (+ a b)))
               (cons '(integer float) (lambda (a b) (+ (float a) b)))
               (cons '(string string) (lambda (a b) (concat a b)))
               (cons '(list list) (lambda (a b) (append a b)))
               (cons '(vector vector)
                     (lambda (a b) (vconcat a b))))))
        (list
          (funcall 'neovm--tpm-dispatch add-table (list 1 2))
          (funcall 'neovm--tpm-dispatch add-table (list 1.5 2.5))
          (funcall 'neovm--tpm-dispatch add-table (list 1 2.0))
          (funcall 'neovm--tpm-dispatch add-table (list "hello" " world"))
          (funcall 'neovm--tpm-dispatch add-table (list '(1 2) '(3 4)))
          (funcall 'neovm--tpm-dispatch add-table (list [1 2] [3 4]))
          ;; No match
          (funcall 'neovm--tpm-dispatch add-table (list 1 "two"))
          (funcall 'neovm--tpm-dispatch add-table (list nil nil))))
    (fmakunbound 'neovm--tpm-typeof)
    (fmakunbound 'neovm--tpm-match-types)
    (fmakunbound 'neovm--tpm-dispatch)))
"####;
    let expect = expect_test::expect![[
        r#""OK (3 4.0 3.0 \"hello world\" (1 2 3 4) [1 2 3 4] (no-match (integer string)) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
