//! Oracle parity tests for comprehensive symbol operations:
//! `intern` vs `intern-soft`, `make-symbol` (uninterned), `symbol-name`,
//! `symbol-value`, `symbol-function`, `symbol-plist`, `boundp`, `fboundp`,
//! `makunbound`, `fmakunbound`, `setplist`, `indirect-function` chain
//! following, symbol equality (eq for interned, identity for uninterned),
//! and obarray operations (custom obarray).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// intern vs intern-soft: interaction and edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_intern_vs_intern_soft_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test interplay between intern and intern-soft: intern-soft returns nil
    // for unknown symbols, intern creates and makes them findable, multiple
    // intern calls return the same (eq) object, and special symbols like
    // nil/t/keywords behave consistently.
    let form = r#"(progn
  ;; Use a unique prefix to avoid pollution
  (let* ((prefix "neovm--scp-ivis-")
         (name1 (concat prefix "alpha"))
         (name2 (concat prefix "beta"))
         (name3 (concat prefix "gamma")))
    (unwind-protect
        (list
          ;; intern-soft returns nil for never-interned names
          (intern-soft name1)
          (intern-soft name2)
          ;; intern creates the symbol
          (let ((s1 (intern name1)))
            (list
              (symbolp s1)
              (symbol-name s1)
              ;; intern-soft now finds it
              (eq (intern-soft name1) s1)
              ;; second intern returns same object
              (eq (intern name1) s1)))
          ;; intern two different names, they are not eq
          (let ((s1 (intern name1))
                (s2 (intern name2)))
            (eq s1 s2))
          ;; intern built-in symbols
          (list (eq (intern "nil") nil)
                (eq (intern "t") t)
                (eq (intern "car") 'car)
                (eq (intern "+") '+))
          ;; intern-soft with symbol argument instead of string
          (eq (intern-soft 'car) 'car)
          ;; intern empty string
          (let ((empty-sym (intern "")))
            (list (symbolp empty-sym)
                  (string= (symbol-name empty-sym) "")))
          ;; intern with special characters
          (let ((special (intern "foo bar")))
            (symbol-name special)))
      ;; cleanup: we can't really unintern from the default obarray easily,
      ;; but the symbols are harmless with unique prefix
      nil)))"#;
    let expect = expect_test::expect![[
        r#""OK (nil nil (t \"neovm--scp-ivis-alpha\" t t) nil (t t t t) t (t t) \"foo bar\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// make-symbol (uninterned): identity, value, function, plist
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_make_symbol_full_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Comprehensive test of uninterned symbol lifecycle: creation, naming,
    // non-eq to interned counterpart, value binding, function binding,
    // plist, and multiple independent uninterned symbols with the same name.
    let form = r####"(let* ((s1 (make-symbol "lifecycle-test"))
           (s2 (make-symbol "lifecycle-test"))
           (s3 (make-symbol "lifecycle-test")))
  ;; All are symbols with the same name
  (list
    (symbolp s1) (symbolp s2) (symbolp s3)
    (symbol-name s1) (symbol-name s2)
    ;; None are eq to each other
    (eq s1 s2) (eq s2 s3) (eq s1 s3)
    ;; None are eq to the interned symbol
    (eq s1 (intern "lifecycle-test"))
    ;; Initially unbound
    (boundp s1) (fboundp s1)
    ;; Set value on s1 only
    (progn (set s1 42) (symbol-value s1))
    ;; s2 is still unbound
    (boundp s2)
    ;; Set function on s2 only
    (progn (fset s2 (lambda (x) (* x x))) (fboundp s2))
    (funcall (symbol-function s2) 7)
    ;; s1 has no function, s2 has no value (as global)
    (fboundp s1)
    ;; Set plist on s3
    (progn
      (setplist s3 '(color red size 10 active t))
      (list (get s3 'color)
            (get s3 'size)
            (get s3 'active)
            (symbol-plist s3)))
    ;; s1 and s2 plists are empty
    (symbol-plist s1)
    (symbol-plist s2)))
"####;
    let expect = expect_test::expect![[
        r#""OK (t t t \"lifecycle-test\" \"lifecycle-test\" nil nil nil nil nil nil 42 nil t 49 nil (red 10 t (color red size 10 active t)) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// symbol-name: edge cases and special symbols
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_name_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test symbol-name on a variety of symbol types: regular, keyword,
    // nil, t, symbols with unusual names, and uninterned symbols.
    let form = r####"(list
  (symbol-name 'regular-sym)
  (symbol-name :keyword-sym)
  (symbol-name nil)
  (symbol-name t)
  (symbol-name '+)
  (symbol-name '1+)
  (symbol-name 'and)
  (symbol-name 'or)
  ;; Symbol with numeric-looking name
  (symbol-name (intern "123"))
  ;; Empty symbol name
  (symbol-name (intern ""))
  ;; Uninterned symbol preserves name
  (symbol-name (make-symbol "uninterned-name"))
  ;; Keywords start with :
  (string-match-p "^:" (symbol-name :test))
  ;; symbol-name returns string type
  (stringp (symbol-name 'anything)))
"####;
    let expect = expect_test::expect![[
        r#""OK (\"regular-sym\" \":keyword-sym\" \"nil\" \"t\" \"+\" \"1+\" \"and\" \"or\" \"123\" \"\" \"uninterned-name\" 0 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// boundp / fboundp / makunbound / fmakunbound lifecycle
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_bound_unbound_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full lifecycle: defvar -> check boundp -> makunbound -> check again,
    // fset -> check fboundp -> fmakunbound -> check again, with interleaving.
    let form = r#"(progn
  (defvar neovm--scp-bul-var nil)
  (unwind-protect
      (progn
        ;; Phase 1: value binding
        (set 'neovm--scp-bul-var 'initial)
        (let ((r1 (list (boundp 'neovm--scp-bul-var)
                        (symbol-value 'neovm--scp-bul-var))))
          ;; Phase 2: function binding on same symbol
          (fset 'neovm--scp-bul-var (lambda (x) (+ x 1)))
          (let ((r2 (list (fboundp 'neovm--scp-bul-var)
                          (funcall 'neovm--scp-bul-var 10))))
            ;; Phase 3: makunbound doesn't affect function
            (makunbound 'neovm--scp-bul-var)
            (let ((r3 (list (boundp 'neovm--scp-bul-var)
                            (fboundp 'neovm--scp-bul-var)
                            (funcall 'neovm--scp-bul-var 20))))
              ;; Phase 4: fmakunbound doesn't affect value
              (set 'neovm--scp-bul-var 'rebound)
              (fmakunbound 'neovm--scp-bul-var)
              (let ((r4 (list (boundp 'neovm--scp-bul-var)
                              (symbol-value 'neovm--scp-bul-var)
                              (fboundp 'neovm--scp-bul-var))))
                ;; Phase 5: both unbound
                (makunbound 'neovm--scp-bul-var)
                (let ((r5 (list (boundp 'neovm--scp-bul-var)
                                (fboundp 'neovm--scp-bul-var))))
                  (list r1 r2 r3 r4 r5)))))))
    (ignore-errors (makunbound 'neovm--scp-bul-var))
    (ignore-errors (fmakunbound 'neovm--scp-bul-var))))"#;
    let expect =
        expect_test::expect![[r#""OK ((t initial) (t 11) (nil t 21) (t rebound nil) (nil nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// setplist: comprehensive manipulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_setplist_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test setplist: setting, replacing, clearing, and interaction with put/get.
    let form = r#"(progn
  (setplist 'neovm--scp-sp-sym nil)
  (unwind-protect
      (progn
        ;; Set initial plist
        (setplist 'neovm--scp-sp-sym '(a 1 b 2 c 3))
        (let ((r1 (list (get 'neovm--scp-sp-sym 'a)
                        (get 'neovm--scp-sp-sym 'b)
                        (get 'neovm--scp-sp-sym 'c)
                        (symbol-plist 'neovm--scp-sp-sym))))
          ;; Replace entire plist
          (setplist 'neovm--scp-sp-sym '(x 10 y 20))
          (let ((r2 (list (get 'neovm--scp-sp-sym 'a)
                          (get 'neovm--scp-sp-sym 'x)
                          (get 'neovm--scp-sp-sym 'y)
                          (symbol-plist 'neovm--scp-sp-sym))))
            ;; Use put to modify after setplist
            (put 'neovm--scp-sp-sym 'z 30)
            (put 'neovm--scp-sp-sym 'x 99)
            (let ((r3 (list (get 'neovm--scp-sp-sym 'x)
                            (get 'neovm--scp-sp-sym 'z))))
              ;; Clear plist
              (setplist 'neovm--scp-sp-sym nil)
              (let ((r4 (list (symbol-plist 'neovm--scp-sp-sym)
                              (get 'neovm--scp-sp-sym 'x))))
                (list r1 r2 r3 r4))))))
    (setplist 'neovm--scp-sp-sym nil)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 (a 1 b 2 c 3)) (nil 10 20 (x 99 y 20 z 30)) (99 30) (nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// indirect-function chain following
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_indirect_function_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test indirect-function: follows chains of symbol-function to the
    // ultimate function object. Test single hop, double hop, and direct
    // function (no indirection).
    let form = r#"(progn
  (fset 'neovm--scp-if-a (lambda (x) (* x 2)))
  (fset 'neovm--scp-if-b 'neovm--scp-if-a)
  (fset 'neovm--scp-if-c 'neovm--scp-if-b)
  (unwind-protect
      (list
        ;; Direct function: indirect-function returns it
        (functionp (indirect-function 'neovm--scp-if-a))
        ;; One hop: b -> a's function
        (functionp (indirect-function 'neovm--scp-if-b))
        ;; Two hops: c -> b -> a's function
        (functionp (indirect-function 'neovm--scp-if-c))
        ;; All resolve to the same function
        (eq (indirect-function 'neovm--scp-if-a)
            (indirect-function 'neovm--scp-if-b))
        (eq (indirect-function 'neovm--scp-if-b)
            (indirect-function 'neovm--scp-if-c))
        ;; Calling through chain works
        (funcall (indirect-function 'neovm--scp-if-c) 21)
        ;; indirect-function on a lambda directly
        (functionp (indirect-function (lambda (x) x)))
        ;; indirect-function on built-in
        (functionp (indirect-function 'car))
        ;; defalias creates indirection too
        (progn
          (defalias 'neovm--scp-if-d 'neovm--scp-if-c)
          (funcall (indirect-function 'neovm--scp-if-d) 5)))
    (fmakunbound 'neovm--scp-if-a)
    (fmakunbound 'neovm--scp-if-b)
    (fmakunbound 'neovm--scp-if-c)
    (fmakunbound 'neovm--scp-if-d)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t 42 t t 10)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Symbol equality: eq for interned, identity for uninterned
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_equality_interned_vs_uninterned() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Interned symbols with the same name are eq; uninterned symbols with
    // the same name are NOT eq. equal compares by identity for symbols too.
    let form = r####"(let ((interned-a (intern "neovm--scp-eq-test"))
         (interned-b (intern "neovm--scp-eq-test"))
         (uninterned-a (make-symbol "neovm--scp-eq-test"))
         (uninterned-b (make-symbol "neovm--scp-eq-test")))
  (list
    ;; Interned: same name => eq
    (eq interned-a interned-b)
    ;; Uninterned: same name => NOT eq
    (eq uninterned-a uninterned-b)
    ;; Interned vs uninterned: NOT eq
    (eq interned-a uninterned-a)
    ;; equal on symbols behaves like eq (no deep comparison)
    (equal interned-a interned-b)
    (equal uninterned-a uninterned-b)
    (equal interned-a uninterned-a)
    ;; But symbol-name returns equal strings
    (equal (symbol-name interned-a) (symbol-name uninterned-a))
    (string= (symbol-name uninterned-a) (symbol-name uninterned-b))
    ;; Comparison via string works
    (string= (symbol-name interned-a) "neovm--scp-eq-test")
    ;; Keywords are always interned and eq
    (eq :foo :foo)
    (eq (intern ":foo") :foo)))
"####;
    let expect = expect_test::expect![[r#""OK (t nil nil t nil nil t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Custom obarray operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_custom_obarray() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create a custom obarray, intern symbols into it, verify they are
    // separate from the global obarray.
    let form = r#"(let ((my-obarray (obarray-make 17)))
  (list
    ;; intern into custom obarray
    (let ((s1 (intern "alpha" my-obarray))
          (s2 (intern "beta" my-obarray)))
      (list
        (symbolp s1)
        (symbolp s2)
        (symbol-name s1)
        (symbol-name s2)
        ;; intern again returns same symbol
        (eq s1 (intern "alpha" my-obarray))
        ;; intern-soft finds it in custom obarray
        (eq s1 (intern-soft "alpha" my-obarray))
        ;; NOT found in default obarray (unless already interned globally)
        ;; We use a unique name to ensure this
        (let ((unique (intern "neovm--scp-cob-unique-xyz" my-obarray)))
          (list
            (symbolp unique)
            ;; intern-soft in default obarray should NOT find it
            ;; (unless something else interned it)
            (null (intern-soft "neovm--scp-cob-unique-xyz"))
            ;; But finds it in custom
            (eq unique (intern-soft "neovm--scp-cob-unique-xyz" my-obarray))))))
    ;; Two custom obarrays are independent
    (let ((ob2 (obarray-make 7)))
      (let ((s-in-1 (intern "shared-name" my-obarray))
            (s-in-2 (intern "shared-name" ob2)))
        (list
          ;; Same name, different obarrays => different symbols
          (eq s-in-1 s-in-2)
          (equal (symbol-name s-in-1) (symbol-name s-in-2)))))
    ;; Size doesn't affect behavior (only performance)
    (let ((small-ob (obarray-make 1))
          (large-ob (obarray-make 1024)))
      (let ((s-small (intern "test-sym" small-ob))
            (s-large (intern "test-sym" large-ob)))
        (list (symbolp s-small) (symbolp s-large)
              (eq s-small s-large))))))
"#;
    let expect =
        expect_test::expect![[r#""OK ((t t \"alpha\" \"beta\" t t (t t t)) (nil t) (t t nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: symbol-based memoization table using plists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_memo_table_via_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use an uninterned symbol's plist as a memoization table for a
    // recursive fibonacci, storing results keyed by argument.
    let form = r####"(progn
  (fset 'neovm--scp-memo-fib
    (lambda (memo-sym n)
      (or (get memo-sym n)
          (let ((result
                 (cond
                   ((= n 0) 0)
                   ((= n 1) 1)
                   (t (+ (funcall 'neovm--scp-memo-fib memo-sym (- n 1))
                         (funcall 'neovm--scp-memo-fib memo-sym (- n 2)))))))
            (put memo-sym n result)
            result))))

  (unwind-protect
      (let ((memo (make-symbol "fib-cache")))
        (list
          ;; Compute fib values
          (funcall 'neovm--scp-memo-fib memo 0)
          (funcall 'neovm--scp-memo-fib memo 1)
          (funcall 'neovm--scp-memo-fib memo 5)
          (funcall 'neovm--scp-memo-fib memo 10)
          (funcall 'neovm--scp-memo-fib memo 15)
          (funcall 'neovm--scp-memo-fib memo 20)
          ;; Verify cached values are accessible
          (get memo 10)
          (get memo 15)
          ;; Number of cached entries (plist length / 2)
          (/ (length (symbol-plist memo)) 2)))
    (fmakunbound 'neovm--scp-memo-fib)))
"####;
    let expect = expect_test::expect![[r#""OK (0 1 5 55 610 6765 55 610 21)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: symbol registry with inheritance via indirect-function chains
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_registry_with_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a simple type hierarchy using symbol plists for metadata and
    // indirect-function for method resolution.
    let form = r#"(progn
  ;; Define "base class" methods
  (fset 'neovm--scp-rgi-base-describe
    (lambda (obj) (format "Object(%s)" (plist-get obj :name))))
  (fset 'neovm--scp-rgi-base-type
    (lambda (obj) 'base))

  ;; Define "child class" methods - override describe, inherit type
  (fset 'neovm--scp-rgi-child-describe
    (lambda (obj) (format "Child(%s, age=%d)"
                          (plist-get obj :name)
                          (plist-get obj :age))))
  ;; child-type is an alias (indirect) to base-type
  (fset 'neovm--scp-rgi-child-type 'neovm--scp-rgi-base-type)

  ;; Method dispatch table via symbol plists
  (setplist 'neovm--scp-rgi-base-class nil)
  (put 'neovm--scp-rgi-base-class 'describe 'neovm--scp-rgi-base-describe)
  (put 'neovm--scp-rgi-base-class 'type-of 'neovm--scp-rgi-base-type)

  (setplist 'neovm--scp-rgi-child-class nil)
  (put 'neovm--scp-rgi-child-class 'parent 'neovm--scp-rgi-base-class)
  (put 'neovm--scp-rgi-child-class 'describe 'neovm--scp-rgi-child-describe)
  (put 'neovm--scp-rgi-child-class 'type-of 'neovm--scp-rgi-child-type)

  ;; Method lookup with inheritance
  (fset 'neovm--scp-rgi-dispatch
    (lambda (class method)
      (or (get class method)
          (let ((parent (get class 'parent)))
            (if parent
                (funcall 'neovm--scp-rgi-dispatch parent method)
              nil)))))

  (unwind-protect
      (let ((base-obj '(:name "base-1"))
            (child-obj '(:name "child-1" :age 25)))
        (list
          ;; Direct method call on base
          (funcall (funcall 'neovm--scp-rgi-dispatch
                            'neovm--scp-rgi-base-class 'describe)
                   base-obj)
          ;; Overridden method on child
          (funcall (funcall 'neovm--scp-rgi-dispatch
                            'neovm--scp-rgi-child-class 'describe)
                   child-obj)
          ;; Inherited method (type-of) through indirect-function
          (let ((type-method (funcall 'neovm--scp-rgi-dispatch
                                      'neovm--scp-rgi-child-class 'type-of)))
            (funcall (indirect-function type-method) child-obj))
          ;; Non-existent method returns nil
          (funcall 'neovm--scp-rgi-dispatch
                   'neovm--scp-rgi-child-class 'nonexistent)))
    (fmakunbound 'neovm--scp-rgi-base-describe)
    (fmakunbound 'neovm--scp-rgi-base-type)
    (fmakunbound 'neovm--scp-rgi-child-describe)
    (fmakunbound 'neovm--scp-rgi-child-type)
    (fmakunbound 'neovm--scp-rgi-dispatch)
    (setplist 'neovm--scp-rgi-base-class nil)
    (setplist 'neovm--scp-rgi-child-class nil)))"#;
    let expect =
        expect_test::expect![[r#""OK (\"Object(base-1)\" \"Child(child-1, age=25)\" base nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
