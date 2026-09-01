//! Oracle parity tests for all conditional forms together.
//!
//! Covers: `if` with/without else, `cond` multi-clause, `when`/`unless` as
//! special cases, `and`/`or` as conditional control flow, `cl-case`/`cl-ecase`/
//! `cl-typecase`, short-circuit evaluation, nested conditionals, conditionals
//! with side effects, type-dispatch patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// if with/without else — comprehensive edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_if_cond_if_with_without_else() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Basic if true
  (if t 'yes 'no)
  ;; Basic if false
  (if nil 'yes 'no)
  ;; if without else returns nil on false
  (if nil 'yes)
  ;; if with multiple body forms in then (only last counted via progn)
  (if t (progn 1 2 3) 'nope)
  ;; if with multiple else forms
  (if nil 'nope 'a 'b 'c)
  ;; Non-nil non-t value is truthy
  (if 42 'truthy 'falsy)
  (if "" 'truthy 'falsy)
  (if '(1) 'truthy 'falsy)
  (if 0 'truthy 'falsy)
  ;; Nested if
  (if (if t nil t) 'outer-true 'outer-false)
  ;; if condition with side effects
  (let ((x 0))
    (if (progn (setq x 10) (> x 5))
        (+ x 100)
      (+ x 200))))"#;
    let expect = expect_test::expect![[
        r#""OK (yes no nil 3 c truthy truthy truthy truthy outer-false 110)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cond multi-clause with complex conditions and bodies
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_if_cond_multi_clause() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Classic type-dispatch
  (let ((dispatch (lambda (x)
          (cond
           ((numberp x) (list 'number (* x 2)))
           ((stringp x) (list 'string (length x)))
           ((symbolp x) (list 'symbol (symbol-name x)))
           ((listp x) (list 'list (length x)))
           (t (list 'unknown x))))))
    (list
     (funcall dispatch 21)
     (funcall dispatch "hello")
     (funcall dispatch 'foo)
     (funcall dispatch '(a b c))))
  ;; cond with no-body clauses (returns test value)
  (cond (nil) (42) (99))
  ;; cond with side effects only in matching clause
  (let ((trace nil))
    (cond
     (nil (push 'first trace))
     (nil (push 'second trace))
     (t (push 'third trace)))
    trace)
  ;; cond with all nil predicates
  (cond (nil 1) (nil 2) (nil 3))
  ;; cond returning last body form
  (cond (t (+ 1 2) (+ 3 4) (+ 5 6))))"#;
    let expect = expect_test::expect![[
        r#""OK (((number 42) (string 5) (symbol \"foo\") (list 3)) 42 (third) nil 11)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// when/unless as special cases of if
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_if_cond_when_unless_special_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; when true: evaluates body, returns last form
  (when t 1 2 3)
  ;; when false: returns nil
  (when nil 1 2 3)
  ;; unless false: evaluates body
  (unless nil 'a 'b 'c)
  ;; unless true: returns nil
  (unless t 'a 'b 'c)
  ;; when with side effects
  (let ((x 0))
    (when (> 5 3) (setq x 10) (setq x (+ x 5)))
    x)
  ;; unless with side effects (should NOT fire)
  (let ((x 100))
    (unless (> 5 3) (setq x 0))
    x)
  ;; when/unless equivalence: (when P B) == (if P (progn B))
  (equal (when t 'yes) (if t (progn 'yes)))
  (equal (when nil 'yes) (if nil (progn 'yes)))
  ;; unless equivalence: (unless P B) == (if (not P) (progn B))
  (equal (unless nil 'yes) (if (not nil) (progn 'yes)))
  (equal (unless t 'yes) (if (not t) (progn 'yes))))"#;
    let expect = expect_test::expect![[r#""OK (3 nil c nil 15 100 t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// and/or as conditional control flow (short-circuit)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_if_cond_and_or_short_circuit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((trace nil))
  (list
   ;; and returns last value if all truthy
   (and 1 2 3)
   ;; and returns nil at first falsy
   (and 1 nil 3)
   ;; and with zero args
   (and)
   ;; or returns first truthy value
   (or nil nil 42 99)
   ;; or returns nil when all falsy
   (or nil nil nil)
   ;; or with zero args
   (or)
   ;; Short-circuit: and stops at nil, does not evaluate later forms
   (progn
     (setq trace nil)
     (and nil (push 'should-not-appear trace))
     trace)
   ;; Short-circuit: or stops at first truthy
   (progn
     (setq trace nil)
     (or 'found (push 'should-not-appear trace))
     trace)
   ;; and/or as conditionals: (and P Q) ~ (if P Q nil)
   (and (> 5 3) 'big)
   ;; (or P Q) ~ (if P P Q) with P evaluated once
   (or nil 'fallback)
   ;; Nested and/or
   (and (or nil t) (or nil nil 'deep))))"#;
    let expect = expect_test::expect![[r#""OK (3 nil t 42 nil nil nil nil big fallback deep)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-case / cl-ecase / cl-typecase (require cl-lib)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_if_cond_cl_case_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
   ;; cl-case basic
   (cl-case 'banana
     (apple 'fruit-a)
     (banana 'fruit-b)
     (cherry 'fruit-c)
     (t 'unknown))
   ;; cl-case with grouped keys
   (cl-case 3
     ((1 2) 'small)
     ((3 4) 'medium)
     ((5 6) 'large)
     (t 'other))
   ;; cl-case no match falls through to t
   (cl-case 99
     (1 'one)
     (2 'two)
     (t 'default))
   ;; cl-case no match, no default => nil
   (cl-case 99
     (1 'one)
     (2 'two))
   ;; cl-typecase
   (cl-typecase 42
     (string 'its-a-string)
     (integer 'its-an-integer)
     (symbol 'its-a-symbol)
     (t 'other))
   (cl-typecase "hello"
     (string 'its-a-string)
     (integer 'its-an-integer)
     (t 'other))
   (cl-typecase '(1 2 3)
     (string 'its-a-string)
     (cons 'its-a-list)
     (t 'other))))"#;
    let expect = expect_test::expect![[
        r#""OK (fruit-b medium default nil its-an-integer its-a-string its-a-list)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Short-circuit evaluation with side effects tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_if_cond_short_circuit_side_effects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((log nil))
  (list
   ;; and: each form evaluated until nil
   (and (progn (push 'a log) t)
        (progn (push 'b log) t)
        (progn (push 'c log) nil)
        (progn (push 'd log) t))
   (nreverse log)
   ;; reset
   (progn (setq log nil) nil)
   ;; or: each form evaluated until truthy
   (or (progn (push 'e log) nil)
       (progn (push 'f log) nil)
       (progn (push 'g log) 'found)
       (progn (push 'h log) t))
   (nreverse log)
   ;; reset
   (progn (setq log nil) nil)
   ;; Nested: (and (or ...) (and ...))
   (and (or (progn (push 'i log) nil)
            (progn (push 'j log) 'ok))
        (and (progn (push 'k log) t)
             (progn (push 'l log) 'done)))
   (nreverse log)))"#;
    let expect =
        expect_test::expect![[r#""OK (nil (a b c) nil found (e f g) nil done (i j k l))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Deeply nested conditionals
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_if_cond_deeply_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((classify
         (lambda (x)
           (if (numberp x)
               (if (integerp x)
                   (if (> x 0)
                       (if (> x 100) 'large-positive 'small-positive)
                     (if (< x 0)
                         (if (< x -100) 'large-negative 'small-negative)
                       'zero))
                 (if (> x 0) 'positive-float 'non-positive-float))
             (if (stringp x)
                 (if (string= x "") 'empty-string 'non-empty-string)
               (if (null x) 'nil-value 'other))))))
  (list
   (funcall classify 200)
   (funcall classify 42)
   (funcall classify 0)
   (funcall classify -5)
   (funcall classify -999)
   (funcall classify 3.14)
   (funcall classify -2.5)
   (funcall classify "hello")
   (funcall classify "")
   (funcall classify nil)
   (funcall classify '(a b))))"#;
    let expect = expect_test::expect![[
        r#""OK (large-positive small-positive zero small-negative large-negative positive-float non-positive-float non-empty-string empty-string nil-value other)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Conditionals with side effects and unwind-protect
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_if_cond_with_side_effects_cleanup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--test-cond-counter 0)
  (fset 'neovm--test-cond-process
    (lambda (items)
      (let ((results nil))
        (dolist (item items)
          (setq neovm--test-cond-counter (1+ neovm--test-cond-counter))
          (push
           (cond
            ((and (numberp item) (> item 50))
             (cons 'big item))
            ((and (numberp item) (> item 10))
             (cons 'medium item))
            ((numberp item)
             (cons 'small item))
            ((stringp item)
             (cons 'text (length item)))
            ((null item)
             (cons 'empty 0))
            (t
             (cons 'other neovm--test-cond-counter)))
           results))
        (nreverse results))))
  (unwind-protect
      (let ((data '(5 25 75 "hi" nil 'sym 100 0 "longstring" 15)))
        (list
         (funcall 'neovm--test-cond-process data)
         neovm--test-cond-counter))
    (fmakunbound 'neovm--test-cond-process)
    (makunbound 'neovm--test-cond-counter)))"#;
    let expect = expect_test::expect![[
        r#""OK (((small . 5) (medium . 25) (big . 75) (text . 2) (empty . 0) (other . 6) (big . 100) (small . 0) (text . 10) (medium . 15)) 10)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Type-dispatch pattern: cond as pattern match over types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_if_cond_type_dispatch_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-serialize
    (lambda (obj)
      (cond
       ((null obj) "null")
       ((eq obj t) "true")
       ((integerp obj) (format "int:%d" obj))
       ((floatp obj) (format "float:%s" obj))
       ((stringp obj) (format "str:\"%s\"" obj))
       ((symbolp obj) (format "sym:%s" (symbol-name obj)))
       ((consp obj)
        (format "(%s . %s)"
                (funcall 'neovm--test-serialize (car obj))
                (funcall 'neovm--test-serialize (cdr obj))))
       ((vectorp obj)
        (concat "["
                (mapconcat (lambda (x) (funcall 'neovm--test-serialize x))
                           (append obj nil) ",")
                "]"))
       (t "?"))))
  (unwind-protect
      (list
       (funcall 'neovm--test-serialize nil)
       (funcall 'neovm--test-serialize t)
       (funcall 'neovm--test-serialize 42)
       (funcall 'neovm--test-serialize 3.14)
       (funcall 'neovm--test-serialize "hello")
       (funcall 'neovm--test-serialize 'world)
       (funcall 'neovm--test-serialize '(1 . 2))
       (funcall 'neovm--test-serialize [10 20 30])
       (funcall 'neovm--test-serialize '(1 (2 3) . 4)))
    (fmakunbound 'neovm--test-serialize)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"null\" \"true\" \"int:42\" \"float:3.14\" \"str:\\\"hello\\\"\" \"sym:world\" \"(int:1 . int:2)\" \"[int:10,int:20,int:30]\" \"(int:1 . ((int:2 . (int:3 . null)) . int:4))\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Conditional interplay: mixing if/cond/when/unless/and/or together
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_if_cond_mixed_conditionals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((evaluate
         (lambda (score)
           (let ((grade
                  (cond
                   ((>= score 90) 'A)
                   ((>= score 80) 'B)
                   ((>= score 70) 'C)
                   ((>= score 60) 'D)
                   (t 'F))))
             (list
              :score score
              :grade grade
              :passing (if (and grade (not (eq grade 'F))) t nil)
              :honors (when (and (>= score 90) (eq grade 'A)) 'with-honors)
              :warning (unless (>= score 60) 'academic-warning)
              :description (or (and (eq grade 'A) "Excellent")
                               (and (eq grade 'B) "Good")
                               (and (eq grade 'C) "Satisfactory")
                               "Needs improvement"))))))
  (list
   (funcall evaluate 95)
   (funcall evaluate 85)
   (funcall evaluate 72)
   (funcall evaluate 65)
   (funcall evaluate 45)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:score 95 :grade A :passing t :honors with-honors :warning nil :description \"Excellent\") (:score 85 :grade B :passing t :honors nil :warning nil :description \"Good\") (:score 72 :grade C :passing t :honors nil :warning nil :description \"Satisfactory\") (:score 65 :grade D :passing t :honors nil :warning nil :description \"Needs improvement\") (:score 45 :grade F :passing nil :honors nil :warning academic-warning :description \"Needs improvement\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
