//! Advanced oracle parity tests for type predicate combinations.
//!
//! Systematic testing of all type predicates on every value type,
//! predicate algebra (numberp = integerp OR floatp, etc.),
//! atom vs consp relationships, and complex type-dispatch patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Systematic type predicates on every value type
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_predicates_systematic_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test every primary predicate against every primary type.
    // Returns a matrix (list of lists) where each row is one value
    // tested against all predicates.
    let form = r#"
(let ((values (list 42 3.14 "hello" 'foo nil t :kw '(1 2) [1 2]
                    (make-hash-table) (lambda (x) x)))
      (preds (list #'integerp #'floatp #'numberp #'stringp
                   #'symbolp #'consp #'listp #'vectorp
                   #'arrayp #'sequencep #'atom #'null
                   #'booleanp #'keywordp #'functionp)))
  (mapcar (lambda (val)
            (mapcar (lambda (pred) (if (funcall pred val) t nil))
                    preds))
          values))
"#;
    let expect = expect_test::expect![[
        r#""OK ((t nil t nil nil nil nil nil nil nil t nil nil nil nil) (nil t t nil nil nil nil nil nil nil t nil nil nil nil) (nil nil nil t nil nil nil nil t t t nil nil nil nil) (nil nil nil nil t nil nil nil nil nil t nil nil nil nil) (nil nil nil nil t nil t nil nil t t t t nil nil) (nil nil nil nil t nil nil nil nil nil t nil t nil nil) (nil nil nil nil t nil nil nil nil nil t nil nil t nil) (nil nil nil nil nil t t nil nil t nil nil nil nil nil) (nil nil nil nil nil nil nil t t t t nil nil nil nil) (nil nil nil nil nil nil nil nil nil nil t nil nil nil nil) (nil nil nil nil nil nil nil nil nil nil t nil nil nil t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// type-of for all value types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_of_exhaustive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // type-of should return canonical type symbols for each value kind
    let form = r#"
(let ((values (list 0 -1 most-positive-fixnum most-negative-fixnum
                    0.0 1.0e10 -2.5
                    "" "abc"
                    'foo 'bar nil t :keyword
                    '(a b c) (cons 1 2)
                    [1 2 3] []
                    (make-hash-table))))
  (mapcar #'type-of values))
"#;
    let expect = expect_test::expect![[
        r#""OK (integer integer integer integer float float float string string symbol symbol symbol symbol symbol cons cons vector vector hash-table)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Predicate algebra: numberp = integerp OR floatp, etc.
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_predicate_algebra_identities() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify algebraic identities between predicates hold for every value.
    // numberp  == (or integerp floatp)
    // sequencep == (or listp vectorp stringp)
    // atom      == (not consp)
    // listp     == (or null consp)
    // nlistp    == (not listp)
    let form = r#"
(let ((values (list 42 3.14 "hello" 'foo nil t :kw '(1 . 2)
                    '(a b) [1 2] (make-hash-table)
                    (lambda () nil) ?a)))
  (let ((results nil))
    (dolist (v values)
      (let ((id1 (eq (numberp v)
                     (or (integerp v) (floatp v))))
            (id2 (eq (sequencep v)
                     (or (listp v) (vectorp v) (stringp v))))
            (id3 (eq (atom v)
                     (not (consp v))))
            (id4 (eq (listp v)
                     (or (null v) (consp v))))
            (id5 (eq (nlistp v)
                     (not (listp v)))))
        (setq results (cons (list id1 id2 id3 id4 id5) results))))
    (nreverse results)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((t t t t t) (t t t t t) (t t t t t) (t t t t t) (t t t t t) (t t t t t) (t t t t t) (t t t t t) (t t t t t) (t t t t t) (t t t t t) (t t t t t) (t t t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// null vs not vs consp on empty/non-empty lists and other types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_null_not_consp_relationships() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Explore the three-way relationship between null, not, and consp
    // on various list-like and non-list-like values.
    let form = r#"
(let ((values (list nil '() t 0 "" '(1) '(1 . 2) '(nil)
                    (list nil nil) [nil] 'symbol)))
  (mapcar (lambda (v)
            (list (null v) (not v) (consp v)
                  ;; derived checks
                  (and (null v) (listp v))
                  (and (not (null v)) (listp v))
                  (and (atom v) (not (null v)))))
          values))
"#;
    let expect = expect_test::expect![[
        r#""OK ((t t nil t nil nil) (t t nil t nil nil) (nil nil nil nil nil t) (nil nil nil nil nil t) (nil nil nil nil nil t) (nil nil t nil t nil) (nil nil t nil t nil) (nil nil t nil t nil) (nil nil t nil t nil) (nil nil nil nil nil t) (nil nil nil nil nil t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: type-safe generic function dispatch
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_safe_generic_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a multi-method-style dispatch table using type predicates.
    // Each "method" is chosen based on the type of the argument.
    // The dispatch table maps predicate -> handler function.
    let form = r#"
(progn
  (fset 'neovm--td-size
    (lambda (val)
      (let ((dispatch
             (list (cons #'null (lambda (_) 0))
                   (cons #'stringp (lambda (v) (length v)))
                   (cons #'vectorp (lambda (v) (length v)))
                   (cons #'consp (lambda (v) (length v)))
                   (cons #'hash-table-p (lambda (v) (hash-table-count v)))
                   (cons #'integerp (lambda (v) (abs v)))
                   (cons #'floatp (lambda (v) (truncate (abs v))))
                   (cons #'symbolp (lambda (v) (length (symbol-name v)))))))
        (let ((result nil))
          (dolist (entry dispatch)
            (when (and (not result) (funcall (car entry) val))
              (setq result (funcall (cdr entry) val))))
          (or result -1)))))
  (unwind-protect
      (list (funcall 'neovm--td-size nil)
            (funcall 'neovm--td-size "hello world")
            (funcall 'neovm--td-size [a b c d e])
            (funcall 'neovm--td-size '(1 2 3))
            (funcall 'neovm--td-size (let ((h (make-hash-table)))
                                       (puthash 'a 1 h)
                                       (puthash 'b 2 h) h))
            (funcall 'neovm--td-size -42)
            (funcall 'neovm--td-size 3.7)
            (funcall 'neovm--td-size 'hello))
    (fmakunbound 'neovm--td-size)))
"#;
    let expect = expect_test::expect![[r#""OK (0 11 5 3 2 42 3 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: value serializer using exhaustive type dispatch
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_value_serializer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a JSON-like serializer that handles every Elisp type differently.
    // This tests that type-of and predicates agree and produce consistent output.
    let form = r#"
(progn
  (fset 'neovm--ts-serialize
    (lambda (val)
      (cond
        ((null val) "null")
        ((eq val t) "true")
        ((integerp val) (concat "int(" (number-to-string val) ")"))
        ((floatp val) (concat "float(" (number-to-string val) ")"))
        ((stringp val) (concat "str(\"" val "\")"))
        ((keywordp val) (concat "kw(" (symbol-name val) ")"))
        ((symbolp val) (concat "sym(" (symbol-name val) ")"))
        ((vectorp val)
         (concat "vec["
                 (mapconcat (lambda (el) (funcall 'neovm--ts-serialize el))
                            (append val nil) ",")
                 "]"))
        ((consp val)
         (if (listp (cdr val))
             (concat "list("
                     (mapconcat (lambda (el) (funcall 'neovm--ts-serialize el))
                                val ",")
                     ")")
           (concat "cons("
                   (funcall 'neovm--ts-serialize (car val))
                   "."
                   (funcall 'neovm--ts-serialize (cdr val))
                   ")")))
        ((hash-table-p val) (concat "hash(" (number-to-string (hash-table-count val)) ")"))
        (t "unknown"))))
  (unwind-protect
      (list (funcall 'neovm--ts-serialize nil)
            (funcall 'neovm--ts-serialize t)
            (funcall 'neovm--ts-serialize 42)
            (funcall 'neovm--ts-serialize 2.718)
            (funcall 'neovm--ts-serialize "hello")
            (funcall 'neovm--ts-serialize :test)
            (funcall 'neovm--ts-serialize 'foo)
            (funcall 'neovm--ts-serialize [1 "two" nil])
            (funcall 'neovm--ts-serialize '(a (b c) d))
            (funcall 'neovm--ts-serialize (cons 1 2))
            (funcall 'neovm--ts-serialize (make-hash-table)))
    (fmakunbound 'neovm--ts-serialize)))
"#;
    let expect = expect_test::expect![[
        r#""OK (\"null\" \"true\" \"int(42)\" \"float(2.718)\" \"str(\\\"hello\\\")\" \"kw(:test)\" \"sym(foo)\" \"vec[int(1),str(\\\"two\\\"),null]\" \"list(sym(a),list(sym(b),sym(c)),sym(d))\" \"cons(int(1).int(2))\" \"hash(0)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: type coercion graph
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_coercion_graph() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a coercion graph: for each pair of types, determine what
    // conversions are possible and verify them. Tests the interplay
    // of number-to-string, string-to-number, symbol-name, intern,
    // append (vector->list), vconcat (list->vector), etc.
    let form = r#"
(let ((int-val 42)
      (float-val 3.14)
      (str-val "hello")
      (sym-val 'world)
      (list-val '(1 2 3))
      (vec-val [4 5 6]))
  (list
    ;; int -> float
    (floatp (float int-val))
    ;; float -> int
    (integerp (truncate float-val))
    ;; int -> string
    (stringp (number-to-string int-val))
    ;; string -> number (valid numeric string)
    (numberp (string-to-number "42"))
    ;; string -> number (non-numeric returns 0)
    (= 0 (string-to-number "abc"))
    ;; symbol -> string
    (stringp (symbol-name sym-val))
    ;; string -> symbol
    (symbolp (intern str-val))
    ;; list -> vector
    (vectorp (vconcat list-val))
    ;; vector -> list
    (listp (append vec-val nil))
    ;; int -> string -> int roundtrip
    (= int-val (string-to-number (number-to-string int-val)))
    ;; symbol -> string -> symbol roundtrip
    (eq sym-val (intern (symbol-name sym-val)))
    ;; list -> vector -> list roundtrip
    (equal list-val (append (vconcat list-val) nil))
    ;; vector -> list -> vector roundtrip
    (equal vec-val (vconcat (append vec-val nil)))
    ;; char -> string -> char roundtrip
    (= ?A (aref (char-to-string ?A) 0))
    ;; number-to-string preserves type info
    (string-match-p "\\." (number-to-string float-val))
    (not (string-match-p "\\." (number-to-string int-val)))))
"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t t t 1 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: type-aware equality comparator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_aware_equality() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a custom equality function that behaves differently based
    // on the types of its two arguments: structural for collections,
    // numeric for numbers (cross int/float), string-equal for strings.
    let form = r#"
(progn
  (fset 'neovm--te-smart-equal
    (lambda (a b)
      (cond
        ;; Both numbers: numeric equality (cross int/float)
        ((and (numberp a) (numberp b)) (= a b))
        ;; Both strings: string-equal (case-sensitive)
        ((and (stringp a) (stringp b)) (string-equal a b))
        ;; Both symbols: eq
        ((and (symbolp a) (symbolp b)) (eq a b))
        ;; Both lists: recursive structural
        ((and (consp a) (consp b))
         (and (funcall 'neovm--te-smart-equal (car a) (car b))
              (funcall 'neovm--te-smart-equal (cdr a) (cdr b))))
        ;; Both vectors: element-wise
        ((and (vectorp a) (vectorp b))
         (and (= (length a) (length b))
              (let ((i 0) (eq-so-far t))
                (while (and eq-so-far (< i (length a)))
                  (setq eq-so-far
                        (funcall 'neovm--te-smart-equal
                                 (aref a i) (aref b i)))
                  (setq i (1+ i)))
                eq-so-far)))
        ;; nil is equal to nil
        ((and (null a) (null b)) t)
        ;; Different types: not equal
        (t nil))))
  (unwind-protect
      (list
        ;; cross-type numeric
        (funcall 'neovm--te-smart-equal 42 42.0)
        (funcall 'neovm--te-smart-equal 0 0.0)
        ;; strings
        (funcall 'neovm--te-smart-equal "abc" "abc")
        (funcall 'neovm--te-smart-equal "abc" "ABC")
        ;; symbols
        (funcall 'neovm--te-smart-equal 'foo 'foo)
        (funcall 'neovm--te-smart-equal 'foo 'bar)
        ;; nested lists
        (funcall 'neovm--te-smart-equal '(1 (2 3)) '(1 (2 3)))
        (funcall 'neovm--te-smart-equal '(1 (2 3)) '(1 (2 4)))
        ;; vectors
        (funcall 'neovm--te-smart-equal [1 "a" foo] [1 "a" foo])
        (funcall 'neovm--te-smart-equal [1 2] [1 3])
        ;; cross-type rejection
        (funcall 'neovm--te-smart-equal 42 "42")
        (funcall 'neovm--te-smart-equal nil '())
        ;; nested mixed
        (funcall 'neovm--te-smart-equal '(1 [2 3.0] "x") '(1.0 [2.0 3] "x")))
    (fmakunbound 'neovm--te-smart-equal)))
"#;
    let expect = expect_test::expect![[r#""OK (t t t nil t nil t nil t nil nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
