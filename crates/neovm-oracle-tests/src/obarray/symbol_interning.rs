//! Advanced oracle parity tests for obarray and symbol interning.
//!
//! Tests intern / intern-soft differences, make-symbol vs intern,
//! symbol identity, symbol-name roundtrips, custom symbol tables,
//! and symbol-based enum patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// intern creates, intern-soft only looks up
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_intern_vs_intern_soft() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // intern creates the symbol if absent; intern-soft returns nil.
    // After intern, intern-soft should find it.
    // Use a unique prefix to avoid pollution from other tests.
    let form = r#"(let ((name1 "neovm--osi-test-alpha-7291")
                        (name2 "neovm--osi-test-beta-7291"))
                    (let ((soft-before-1 (intern-soft name1))
                          (soft-before-2 (intern-soft name2)))
                      ;; intern name1 only
                      (let ((sym1 (intern name1)))
                        (let ((soft-after-1 (intern-soft name1))
                              (soft-after-2 (intern-soft name2)))
                          (list
                            ;; before: neither interned
                            (null soft-before-1)
                            (null soft-before-2)
                            ;; sym1 is a symbol
                            (symbolp sym1)
                            ;; after intern: name1 found, name2 still nil
                            (eq sym1 soft-after-1)
                            (null soft-after-2)
                            ;; symbol-name roundtrip
                            (symbol-name sym1))))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t \"neovm--osi-test-alpha-7291\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// intern into default obarray — identity guarantee
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_intern_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Interning the same name twice returns the exact same object (eq).
    // Also test that the quoted symbol is eq to the interned one.
    let form = r#"(let ((s1 (intern "car"))
                        (s2 (intern "car"))
                        (s3 'car))
                    (list
                      (eq s1 s2)
                      (eq s1 s3)
                      (eq s2 s3)
                      ;; Multiple calls, still eq
                      (eq (intern "progn") (intern "progn"))
                      ;; New symbol: two calls still eq
                      (let ((a (intern "neovm--osi-identity-check-8342"))
                            (b (intern "neovm--osi-identity-check-8342")))
                        (eq a b))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_obarray_intern_reuses_name_string_object() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
      (let* ((obarray (make-vector 17 0))
             (name (copy-sequence "abc"))
             (sym (intern name obarray)))
        (aset name 0 ?X)
        (list (eq name (symbol-name sym))
              name
              (symbol-name sym)
              (eq sym (intern-soft name obarray))
              (intern-soft "abc" obarray)
              (intern-soft "Xbc" obarray)))"#;
    let expect = expect_test::expect![[r#""OK (t \"Xbc\" \"Xbc\" t nil Xbc)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(r#"(t "Xbc" "Xbc" t nil Xbc)"#, &o, &n);
}

// ---------------------------------------------------------------------------
// make-symbol (uninterned) vs intern (interned)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_make_symbol_vs_intern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // make-symbol creates uninterned symbols: same name, different identity.
    // intern creates interned symbols: same name, same identity.
    let form = r#"(let ((name "neovm--osi-msvi-test-4427"))
                    (let ((interned (intern name))
                          (uninterned-a (make-symbol name))
                          (uninterned-b (make-symbol name)))
                      (list
                        ;; All have the same name
                        (equal (symbol-name interned) name)
                        (equal (symbol-name uninterned-a) name)
                        (equal (symbol-name uninterned-b) name)
                        ;; Interned is eq to itself via intern
                        (eq interned (intern name))
                        ;; Uninterned are NOT eq to interned
                        (eq uninterned-a interned)
                        (eq uninterned-b interned)
                        ;; Two uninterned are NOT eq to each other
                        (eq uninterned-a uninterned-b)
                        ;; intern-soft does NOT find uninterned symbols
                        (eq (intern-soft name) interned)
                        ;; symbolp for all
                        (symbolp interned)
                        (symbolp uninterned-a)
                        (symbolp uninterned-b))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t nil nil nil t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// symbol-name roundtrip with special characters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_symbol_name_roundtrip_special() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test symbol-name on various interned symbols, including those with
    // special characters, numbers, hyphens, and underscores.
    let form = r#"(let ((names '("foo" "bar-baz" "a_b_c" "x1y2z3"
                                  "with-hyphen-and-number-42"
                                  "CamelCase" "ALLCAPS"
                                  "has.dot" "has/slash")))
                    (mapcar (lambda (name)
                              (let ((sym (intern name)))
                                (list
                                  ;; Roundtrip: symbol-name of interned = original
                                  (equal (symbol-name sym) name)
                                  ;; Re-interning symbol-name gives back same symbol
                                  (eq (intern (symbol-name sym)) sym))))
                            names))"#;
    let expect =
        expect_test::expect![[r#""OK ((t t) (t t) (t t) (t t) (t t) (t t) (t t) (t t) (t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// intern-soft returns nil for non-existent, t-adjacent tests
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_intern_soft_nil_for_absent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Systematically test intern-soft with definitely-absent names.
    // Also test that after intern, intern-soft finds it, and that
    // the returned symbol has the expected properties.
    let form = r#"(let ((absent-names '("neovm--osi-absent-a-9981"
                                         "neovm--osi-absent-b-9981"
                                         "neovm--osi-absent-c-9981")))
                    (let ((before (mapcar #'intern-soft absent-names)))
                      ;; Intern the first two
                      (intern (nth 0 absent-names))
                      (intern (nth 1 absent-names))
                      (let ((after (mapcar #'intern-soft absent-names)))
                        (list
                          ;; All nil before
                          (mapcar #'null before)
                          ;; First two non-nil after, third still nil
                          (not (null (nth 0 after)))
                          (not (null (nth 1 after)))
                          (null (nth 2 after))
                          ;; The found symbols are eq to intern result
                          (eq (nth 0 after) (intern (nth 0 absent-names)))
                          (eq (nth 1 after) (intern (nth 1 absent-names)))))))"#;
    let expect = expect_test::expect![[r#""OK ((t t t) t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Uninterned symbol value/function isolation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_uninterned_isolation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Setting value/function on uninterned symbol does NOT affect interned one.
    let form = r#"(let ((name "neovm--osi-isolation-5534"))
                    (let ((interned (intern name))
                          (uninterned (make-symbol name)))
                      ;; Set value on uninterned
                      (set uninterned 999)
                      ;; Set function on uninterned
                      (fset uninterned (lambda (x) (* x 2)))
                      ;; Set plist on uninterned
                      (put uninterned 'tag 'special)
                      (list
                        ;; Uninterned has value, function, plist
                        (symbol-value uninterned)
                        (funcall (symbol-function uninterned) 21)
                        (get uninterned 'tag)
                        ;; Interned is unaffected (not bound)
                        (boundp interned)
                        (fboundp interned)
                        (get interned 'tag))))"#;
    let expect = expect_test::expect![[r#""OK (999 42 special nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: symbol table implementation with hash table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_custom_symbol_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a custom "namespace" using a hash table mapping names
    // to uninterned symbols. This gives isolated symbol spaces.
    let form = r#"(let ((ns-table (make-hash-table :test 'equal)))
                    (let ((ns-intern
                           (lambda (name)
                             (or (gethash name ns-table)
                                 (let ((sym (make-symbol name)))
                                   (puthash name sym ns-table)
                                   sym))))
                          (ns-intern-soft
                           (lambda (name)
                             (gethash name ns-table nil)))
                          (ns-set
                           (lambda (name value)
                             (let ((sym (or (gethash name ns-table)
                                           (let ((s (make-symbol name)))
                                             (puthash name s ns-table)
                                             s))))
                               (set sym value)
                               sym)))
                          (ns-get
                           (lambda (name)
                             (let ((sym (gethash name ns-table)))
                               (if sym (symbol-value sym) nil)))))
                      ;; Populate namespace
                      (funcall ns-set "counter" 0)
                      (funcall ns-set "name" "alice")
                      (funcall ns-set "items" '(a b c))
                      ;; Retrieve
                      (let ((c (funcall ns-get "counter"))
                            (n (funcall ns-get "name"))
                            (items (funcall ns-get "items")))
                        ;; Verify identity: re-interning same name gives eq symbol
                        (let ((s1 (funcall ns-intern "counter"))
                              (s2 (funcall ns-intern "counter")))
                          ;; Modify through the symbol
                          (set s1 (1+ (symbol-value s1)))
                          (list
                            c n items
                            (eq s1 s2)
                            (funcall ns-get "counter")
                            ;; Non-existent returns nil
                            (funcall ns-intern-soft "missing")
                            ;; Count entries
                            (hash-table-count ns-table))))))"#;
    let expect = expect_test::expect![[r#""OK (0 \"alice\" (a b c) t 1 nil 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: symbol-based enum with validation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_symbol_enum_validation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build an enum system: define valid values, validate membership,
    // map enum values to descriptions, support next/prev traversal.
    let form = r#"(let ((enum-values nil)
                        (enum-descs (make-hash-table :test 'eq))
                        (enum-order (make-hash-table :test 'eq)))
                    (let ((define-enum
                           (lambda (values descriptions)
                             (setq enum-values values)
                             (let ((i 0))
                               (dolist (v values)
                                 (puthash v (nth i descriptions) enum-descs)
                                 (puthash v i enum-order)
                                 (setq i (1+ i))))))
                          (enum-valid-p
                           (lambda (val)
                             (not (null (memq val enum-values)))))
                          (enum-desc
                           (lambda (val)
                             (gethash val enum-descs "unknown")))
                          (enum-next
                           (lambda (val)
                             (let ((idx (gethash val enum-order)))
                               (if (and idx (< (1+ idx) (length enum-values)))
                                   (nth (1+ idx) enum-values)
                                 nil))))
                          (enum-prev
                           (lambda (val)
                             (let ((idx (gethash val enum-order)))
                               (if (and idx (> idx 0))
                                   (nth (1- idx) enum-values)
                                 nil))))
                          (enum-range
                           (lambda (from to)
                             (let ((start (gethash from enum-order))
                                   (end (gethash to enum-order)))
                               (when (and start end (<= start end))
                                 (let ((result nil) (i start))
                                   (while (<= i end)
                                     (setq result (cons (nth i enum-values) result))
                                     (setq i (1+ i)))
                                   (nreverse result)))))))
                      ;; Define traffic light enum
                      (funcall define-enum
                               '(red yellow green)
                               '("Stop" "Caution" "Go"))
                      (list
                        ;; Validation
                        (funcall enum-valid-p 'red)
                        (funcall enum-valid-p 'green)
                        (funcall enum-valid-p 'blue)
                        ;; Descriptions
                        (funcall enum-desc 'red)
                        (funcall enum-desc 'yellow)
                        (funcall enum-desc 'green)
                        (funcall enum-desc 'blue)
                        ;; Navigation
                        (funcall enum-next 'red)
                        (funcall enum-next 'yellow)
                        (funcall enum-next 'green)
                        (funcall enum-prev 'green)
                        (funcall enum-prev 'red)
                        ;; Range
                        (funcall enum-range 'red 'green)
                        (funcall enum-range 'yellow 'green))))"#;
    let expect = expect_test::expect![[
        r#""OK (t t nil \"Stop\" \"Caution\" \"Go\" \"unknown\" yellow green nil yellow nil (red yellow green) (yellow green))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
