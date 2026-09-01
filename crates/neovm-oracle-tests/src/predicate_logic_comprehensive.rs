//! Comprehensive oracle parity tests for all predicate functions.
//! Tests every predicate against every major value type: nil, t, integers,
//! floats, strings, cons cells, vectors, symbols, keywords, hash-tables,
//! char-tables, bool-vectors, functions (lambdas, subrs), buffers, markers.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// null, atom, listp, consp, nlistp — systematic cross-type testing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_predicate_null_atom_listp_consp_nlistp_cross_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((values (list nil t 0 1 -1 42 3.14 -2.7 0.0
                             "" "hello" 'foo :bar
                             '(1 2 3) '(a . b) (cons nil nil)
                             [1 2 3] (vector) (make-vector 5 0)
                             (make-hash-table) (lambda (x) x)
                             (make-bool-vector 8 nil) (make-char-table 'syntax-table)))
              (preds (list #'null #'atom #'listp #'consp #'nlistp))
              (pred-names '(null atom listp consp nlistp)))
  (let ((results nil))
    (dolist (pred-name pred-names)
      (let ((pred (nth (- (length pred-names)
                          (length (memq pred-name pred-names)))
                       preds))
            (row nil))
        (dolist (val values)
          (setq row (cons (if (funcall pred val) t nil) row)))
        (setq results (cons (cons pred-name (nreverse row)) results))))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((null t nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil) (atom t t t t t t t t t t t t t nil nil nil t t t t t t t) (listp t nil nil nil nil nil nil nil nil nil nil nil nil t t t nil nil nil nil nil nil nil) (consp nil nil nil nil nil nil nil nil nil nil nil nil nil t t t nil nil nil nil nil nil nil) (nlistp nil t t t t t t t t t t t t nil nil nil t t t t t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// numberp, integerp, floatp, natnump, zerop — numeric predicates
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_predicate_numeric_predicates_cross_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((values (list nil t 0 1 -1 42 -100 most-positive-fixnum
                             most-negative-fixnum
                             0.0 1.0 -1.0 3.14 1e10 1e-10 -0.0
                             0.0e+NaN 1.0e+INF -1.0e+INF
                             "" "123" '(1) [1] 'foo :bar)))
  (list
    (mapcar #'numberp values)
    (mapcar #'integerp values)
    (mapcar #'floatp values)
    (mapcar #'natnump values)
    (mapcar #'zerop (list 0 0.0 -0 -0.0 1 -1 0.1))))"#;
    let expect = expect_test::expect![[
        r#""OK ((nil nil t t t t t t t t t t t t t t t t t nil nil nil nil nil nil) (nil nil t t t t t t t nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil) (nil nil nil nil nil nil nil nil nil t t t t t t t t t t nil nil nil nil nil nil) (nil nil t t nil t nil t nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil) (t t t t nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// stringp, vectorp, symbolp, keywordp — type predicates on strings/vectors/symbols
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_predicate_stringp_vectorp_symbolp_keywordp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((values (list nil t 0 1.0 "" "abc" "nil" "t"
                             'nil 't 'foo 'bar
                             :foo :bar :nil
                             [] [1 2] (vector 'a 'b)
                             '(1) (cons 1 2) (make-hash-table))))
  (list
    (mapcar #'stringp values)
    (mapcar #'vectorp values)
    (mapcar #'symbolp values)
    (mapcar #'keywordp values)))"#;
    let expect = expect_test::expect![[
        r#""OK ((nil nil nil nil t t t t nil nil nil nil nil nil nil nil nil nil nil nil nil) (nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil t t t nil nil nil) (t t nil nil nil nil nil nil t t t t t t t nil nil nil nil nil nil) (nil nil nil nil nil nil nil nil nil nil nil nil t t t nil nil nil nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// functionp, subrp — function type predicates
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_predicate_functionp_subrp_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(list
  ;; functionp tests
  (functionp nil)
  (functionp t)
  (functionp 42)
  (functionp "hello")
  (functionp #'car)
  (functionp #'cons)
  (functionp #'+)
  (functionp (lambda (x) x))
  (functionp (lambda (x y) (+ x y)))
  (functionp '(lambda (x) x))  ;; quoted lambda is NOT a function
  (functionp 'car)  ;; symbol is NOT a function
  ;; subrp tests
  (subrp (symbol-function 'car))
  (subrp (symbol-function 'cons))
  (subrp (symbol-function '+))
  (subrp (lambda (x) x))
  (subrp nil)
  (subrp 42)
  (subrp "hello"))"#;
    let expect =
        expect_test::expect![[r#""OK (nil nil nil nil t t t t t t t t t t nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// hash-table-p, char-table-p, bool-vector-p
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_predicate_hashtable_chartable_boolvector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((ht (make-hash-table :test 'equal))
              (ct (make-char-table 'syntax-table))
              (bv (make-bool-vector 16 t))
              (others (list nil t 0 1.0 "" "abc" '(1 2) [1 2] 'foo :bar
                           (lambda (x) x))))
  (list
    ;; hash-table-p
    (hash-table-p ht)
    (mapcar #'hash-table-p others)
    ;; char-table-p
    (char-table-p ct)
    (char-table-p ht)
    (mapcar #'char-table-p others)
    ;; bool-vector-p
    (bool-vector-p bv)
    (bool-vector-p ht)
    (bool-vector-p ct)
    (mapcar #'bool-vector-p others)
    ;; cross checks: none of these should satisfy the wrong predicate
    (hash-table-p ct)
    (hash-table-p bv)
    (char-table-p ht)
    (char-table-p bv)
    (bool-vector-p ht)
    (bool-vector-p ct)))"#;
    let expect = expect_test::expect![[
        r#""OK (t (nil nil nil nil nil nil nil nil nil nil nil) t nil (nil nil nil nil nil nil nil nil nil nil nil) t nil nil (nil nil nil nil nil nil nil nil nil nil nil) nil nil nil nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// sequencep, arrayp — container predicates
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_predicate_sequencep_arrayp_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((values (list nil t 0 1 -1 3.14
                             "" "hello"
                             '() '(1 2 3) '(a . b)
                             [] [1 2 3]
                             (make-vector 0 nil) (make-vector 5 42)
                             (make-bool-vector 8 nil)
                             (make-char-table 'syntax-table)
                             (make-hash-table)
                             'foo :bar
                             (lambda (x) x))))
  (list
    (mapcar #'sequencep values)
    (mapcar #'arrayp values)
    ;; sequencep includes: lists, strings, vectors, bool-vectors, char-tables
    ;; arrayp includes: strings, vectors, bool-vectors, char-tables (NOT lists)
    ;; Verify specific distinctions
    (sequencep '(1 2 3))   ;; t (list is sequence)
    (arrayp '(1 2 3))      ;; nil (list is NOT array)
    (sequencep "abc")       ;; t
    (arrayp "abc")          ;; t
    (sequencep [1 2])       ;; t
    (arrayp [1 2])))"#;
    let expect = expect_test::expect![[
        r#""OK ((t nil nil nil nil nil t t t t t t t t t t t nil nil nil nil) (nil nil nil nil nil nil t t nil nil nil t t t t t t nil nil nil nil) t nil t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// characterp — character predicate with edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_predicate_characterp_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(list
  ;; Characters are just integers in valid range
  (characterp ?a)
  (characterp ?Z)
  (characterp ?0)
  (characterp ?\n)
  (characterp ?\t)
  (characterp ?\ )
  (characterp 0)         ;; 0 is a valid char (null character)
  (characterp 65)        ;; ?A
  (characterp 127)       ;; DEL
  (characterp 128)       ;; still valid
  (characterp #x10FFFF)  ;; max Unicode
  ;; Invalid: negative, too large, non-integer
  (characterp -1)
  (characterp nil)
  (characterp t)
  (characterp 3.14)
  (characterp "a")
  (characterp 'a)
  (characterp '(65))
  (characterp [65]))"#;
    let expect =
        expect_test::expect![[r#""OK (t t t t t t t t t t t nil nil nil nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// booleanp — only nil and t
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_predicate_booleanp_exhaustive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(list
  (booleanp nil)
  (booleanp t)
  (booleanp 0)
  (booleanp 1)
  (booleanp "")
  (booleanp "nil")
  (booleanp "t")
  (booleanp 'nil)  ;; nil is the symbol nil, so booleanp -> t
  (booleanp 't)    ;; t is the symbol t, so booleanp -> t
  (booleanp 'foo)
  (booleanp :nil)
  (booleanp :t)
  (booleanp '())   ;; '() is nil
  (booleanp '(t))
  (booleanp [])
  (booleanp (lambda () t))
  (booleanp (make-hash-table)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t nil nil nil nil nil t t nil nil nil t nil nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Predicate composition: and/or/not with predicates
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_predicate_composition_logic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((check-type
         (lambda (val)
           "Return a list of all matching type predicates for VAL."
           (let ((result nil))
             (when (null val) (setq result (cons 'null result)))
             (when (atom val) (setq result (cons 'atom result)))
             (when (listp val) (setq result (cons 'listp result)))
             (when (consp val) (setq result (cons 'consp result)))
             (when (nlistp val) (setq result (cons 'nlistp result)))
             (when (numberp val) (setq result (cons 'numberp result)))
             (when (integerp val) (setq result (cons 'integerp result)))
             (when (floatp val) (setq result (cons 'floatp result)))
             (when (stringp val) (setq result (cons 'stringp result)))
             (when (vectorp val) (setq result (cons 'vectorp result)))
             (when (symbolp val) (setq result (cons 'symbolp result)))
             (when (keywordp val) (setq result (cons 'keywordp result)))
             (when (functionp val) (setq result (cons 'functionp result)))
             (when (sequencep val) (setq result (cons 'sequencep result)))
             (when (arrayp val) (setq result (cons 'arrayp result)))
             (when (booleanp val) (setq result (cons 'booleanp result)))
             (nreverse result)))))
  (list
    (funcall check-type nil)
    (funcall check-type t)
    (funcall check-type 42)
    (funcall check-type 3.14)
    (funcall check-type "hello")
    (funcall check-type '(1 2 3))
    (funcall check-type (cons 1 2))
    (funcall check-type [1 2 3])
    (funcall check-type 'foo)
    (funcall check-type :bar)
    (funcall check-type (lambda (x) x))
    (funcall check-type #'+)))"#;
    let expect = expect_test::expect![[
        r#""OK ((null atom listp symbolp sequencep booleanp) (atom nlistp symbolp booleanp) (atom nlistp numberp integerp) (atom nlistp numberp floatp) (atom nlistp stringp sequencep arrayp) (listp consp sequencep) (listp consp sequencep) (atom nlistp vectorp sequencep arrayp) (atom nlistp symbolp) (atom nlistp symbolp keywordp) (atom nlistp functionp) (atom nlistp symbolp functionp))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Predicate invariants: mutual exclusivity and exhaustiveness
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_predicate_invariants_mutual_exclusivity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((values (list nil t 0 -1 42 0.0 3.14 -1.5
                             "" "abc"
                             '(1 2) '(a . b)
                             [1 2] (make-vector 3 0)
                             'foo :bar
                             (lambda (x) x)
                             (make-hash-table)
                             (make-bool-vector 4 t))))
  ;; Invariant 1: consp and atom are mutually exclusive and exhaustive
  (let ((consp-atom-ok t)
        ;; Invariant 2: listp = (or null consp)
        (listp-ok t)
        ;; Invariant 3: nlistp = (not listp)
        (nlistp-ok t)
        ;; Invariant 4: numberp = (or integerp floatp)
        (numberp-ok t)
        ;; Invariant 5: for integers, natnump = (>= val 0)
        (natnump-ok t))
    (dolist (val values)
      ;; consp XOR atom must always be true
      (unless (and (or (consp val) (atom val))
                   (not (and (consp val) (atom val))))
        (setq consp-atom-ok nil))
      ;; listp = null or consp
      (unless (eq (listp val) (or (null val) (consp val)))
        (setq listp-ok nil))
      ;; nlistp = not listp
      (unless (eq (if (nlistp val) t nil) (if (listp val) nil t))
        (setq nlistp-ok nil))
      ;; numberp = integerp or floatp
      (unless (eq (if (numberp val) t nil)
                  (if (or (integerp val) (floatp val)) t nil))
        (setq numberp-ok nil))
      ;; natnump for integers
      (when (integerp val)
        (unless (eq (if (natnump val) t nil)
                    (if (>= val 0) t nil))
          (setq natnump-ok nil))))
    (list consp-atom-ok listp-ok nlistp-ok numberp-ok natnump-ok)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Predicate edge cases: special values (most-positive-fixnum, empty collections)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_predicate_edge_cases_special_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(list
  ;; most-positive-fixnum / most-negative-fixnum
  (integerp most-positive-fixnum)
  (integerp most-negative-fixnum)
  (natnump most-positive-fixnum)
  (natnump most-negative-fixnum)
  (numberp most-positive-fixnum)
  (zerop most-positive-fixnum)
  ;; Float special values
  (floatp 1.0e+INF)
  (floatp -1.0e+INF)
  (floatp 0.0e+NaN)
  (numberp 1.0e+INF)
  (numberp 0.0e+NaN)
  (natnump 1.0e+INF)  ;; not an integer
  ;; Empty containers
  (null '())
  (listp '())
  (consp '())
  (sequencep '())
  (arrayp '())
  (stringp "")
  (sequencep "")
  (arrayp "")
  (vectorp [])
  (sequencep [])
  (arrayp [])
  ;; Boolean edge: 'nil vs nil
  (eq 'nil nil)
  (booleanp 'nil)
  (null 'nil))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t nil t nil t t t t t nil t t nil t nil t t t t t t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested predicate application: predicates on results of predicates
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_predicate_nested_application() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(list
  ;; Predicates return t or nil, which are both booleans and symbols
  (booleanp (integerp 42))
  (booleanp (stringp "hello"))
  (booleanp (null nil))
  (symbolp (consp '(1)))
  (symbolp (atom 42))
  ;; null of predicate results
  (null (integerp 42))      ;; integerp returns t, null of t is nil
  (null (integerp "hello")) ;; integerp returns nil, null of nil is t
  (null (null nil))          ;; null nil = t, null t = nil
  (null (null t))            ;; null t = nil, null nil = t
  ;; Chain: type of predicate result
  (integerp (if (numberp 42) 1 0))  ;; t -> 1, integerp 1 -> t
  (stringp (if (stringp "a") "yes" "no"))
  ;; mapcar with predicates
  (mapcar #'integerp '(1 2.0 "a" nil t 42))
  (mapcar #'stringp '("a" 1 nil "b" t ""))
  (mapcar #'symbolp '(foo bar 1 nil t :kw "str"))
  (mapcar #'consp '(nil (1) (a . b) t 42 "x" [1])))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t nil t nil t t t (t nil nil nil nil t) (t nil nil t nil t) (t t nil t t t nil) (nil t t nil nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
