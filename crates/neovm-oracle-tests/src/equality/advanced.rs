//! Advanced oracle parity tests for equality and comparison primitives.
//!
//! Tests `eq`, `eql`, `equal`, `string-equal`, `/=`, and complex
//! structural equality patterns across all value types.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// eq on all value types (identity comparison)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eq_all_value_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // eq on symbols, integers, strings, cons, vectors, nil, t, characters
    let form = r#"(let ((sym 'foo)
                        (n 42)
                        (s "hello")
                        (c (cons 1 2))
                        (v (vector 1 2 3))
                        (ch ?A))
                    (list
                     ;; Symbols: same symbol is eq
                     (eq sym sym)
                     (eq 'foo 'foo)
                     (eq sym 'foo)
                     ;; Integers: fixnums are eq by value
                     (eq n 42)
                     (eq 0 0)
                     (eq -1 -1)
                     ;; Strings: distinct allocations are NOT eq
                     (eq "hello" "hello")
                     (eq s s)
                     ;; Cons: same object is eq, distinct are not
                     (eq c c)
                     (eq (cons 1 2) (cons 1 2))
                     ;; Vectors: same object is eq, distinct are not
                     (eq v v)
                     (eq (vector 1 2 3) (vector 1 2 3))
                     ;; Nil and t
                     (eq nil nil)
                     (eq t t)
                     (eq nil t)
                     ;; Characters: same char is eq (fixnum)
                     (eq ch ?A)
                     (eq ?Z ?Z)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t nil t t nil t nil t t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// eql vs eq: numbers use value equality, not identity
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eql_vs_eq_number_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // eql compares numbers by value and type; eq uses identity
    let form = r#"(let ((i1 42)
                        (i2 42)
                        (f1 3.14)
                        (f2 3.14))
                    (list
                     ;; integers: eq and eql both true for same value
                     (eq i1 i2)
                     (eql i1 i2)
                     ;; floats: eq is identity (may differ), eql is value
                     (eql 1.5 1.5)
                     (eql 0.0 -0.0)
                     ;; integer vs float: eql is type-sensitive
                     (eql 1 1.0)
                     (eql 0 0.0)
                     ;; eql on non-numbers falls back to eq
                     (eql "abc" "abc")
                     (eql 'x 'x)
                     (eql nil nil)
                     ;; large integers: eql uses value
                     (eql 536870911 536870911)
                     (eql -536870912 -536870912)
                     ;; computed floats with same value are eql
                     (eql (+ 1.0 2.0) (- 5.0 2.0))
                     ;; but NOT eq
                     (eq (+ 1.0 2.0) (- 5.0 2.0))))"#;
    let expect = expect_test::expect![[r#""OK (t t t nil nil nil nil t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// equal on deeply nested mixed structures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_equal_deeply_nested_mixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // equal compares structurally: lists of vectors of strings
    let form = r#"(list
                   ;; Equal nested structures
                   (equal '(1 (2 (3 (4 (5))))) '(1 (2 (3 (4 (5))))))
                   ;; Vectors inside lists
                   (equal (list [1 2 3] [4 5 6]) (list [1 2 3] [4 5 6]))
                   ;; Strings inside vectors inside lists
                   (equal (list ["a" "b"] '("c" "d"))
                          (list ["a" "b"] '("c" "d")))
                   ;; Mixed numeric types in structures
                   (equal '(1 2.0 3) '(1 2.0 3))
                   ;; Deep nesting: 6 levels
                   (equal '((((((deep))))))
                          '((((((deep)))))))
                   ;; Vectors of lists of vectors
                   (equal (vector (list (vector 1 2) (vector 3 4)))
                          (vector (list (vector 1 2) (vector 3 4))))
                   ;; Difference at deep level
                   (equal '(1 (2 (3 (4 (5))))) '(1 (2 (3 (4 (6))))))
                   ;; Dotted pairs nested
                   (equal '((a . 1) (b . (c . 2)))
                          '((a . 1) (b . (c . 2)))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// equal on hash tables: NOT equal even with same content
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_equal_hash_tables_not_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // In Emacs, equal on hash tables compares identity (like eq),
    // so two hash tables with identical content are NOT equal
    let form = r#"(let ((h1 (make-hash-table :test 'equal))
                        (h2 (make-hash-table :test 'equal)))
                    (puthash 'a 1 h1)
                    (puthash 'b 2 h1)
                    (puthash 'a 1 h2)
                    (puthash 'b 2 h2)
                    (list
                     ;; Same hash table is equal to itself
                     (equal h1 h1)
                     ;; Different hash tables with same content are NOT equal
                     (equal h1 h2)
                     ;; eq and equal behave the same for hash tables
                     (eq h1 h1)
                     (eq h1 h2)
                     ;; But hash table values can be retrieved and compared
                     (equal (gethash 'a h1) (gethash 'a h2))
                     (equal (gethash 'b h1) (gethash 'b h2))))"#;
    let expect = expect_test::expect![[r#""OK (t nil t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-equal: case sensitivity and basic behavior
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_equal_case_and_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // string-equal (string=) ignores text properties but is case-sensitive
    let form = r#"(list
                   ;; Basic equality
                   (string-equal "hello" "hello")
                   ;; Case sensitive
                   (string-equal "Hello" "hello")
                   (string-equal "ABC" "abc")
                   ;; Empty strings
                   (string-equal "" "")
                   (string-equal "" "a")
                   ;; Symbols are accepted (converted to name)
                   (string-equal 'hello "hello")
                   (string-equal "hello" 'hello)
                   (string-equal 'foo 'foo)
                   (string-equal 'foo 'bar)
                   ;; Strings with special characters
                   (string-equal "a\nb" "a\nb")
                   (string-equal "a\tb" "a\tb")
                   (string-equal "a\nb" "a\tb")
                   ;; Unicode
                   (string-equal "\u00e9" "\u00e9"))"#;
    let expect = expect_test::expect![[r#""OK (t nil nil t nil t t t nil t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// /= (not-equal) on numeric comparisons
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_not_equal_numeric() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // /= returns t if args are numerically not equal
    let form = r#"(list
                   ;; Basic integer comparisons
                   (/= 1 2)
                   (/= 1 1)
                   (/= 0 0)
                   (/= -1 1)
                   ;; Float comparisons
                   (/= 1.0 2.0)
                   (/= 1.0 1.0)
                   ;; Mixed int/float: /= compares numerically
                   (/= 1 1.0)
                   (/= 2 2.0)
                   (/= 1 2.0)
                   ;; Edge cases
                   (/= 0 0.0)
                   (/= 0.0 -0.0)
                   ;; Large numbers
                   (/= 536870911 536870911)
                   (/= 536870911 536870912)
                   ;; Computed values
                   (/= (+ 2 3) (* 1 5))
                   (/= (+ 2 3) (* 1 6)))"#;
    let expect =
        expect_test::expect![[r#""OK (t nil nil t t nil nil nil t nil nil nil t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: implementing structural equality for custom tagged types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_equality_custom_structural() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a custom deep-equal that handles vectors, lists, strings,
    // numbers recursively, and test it on complex nested structures
    let form = r#"(let ((my-deep-equal nil))
                    (setq my-deep-equal
                          (lambda (a b)
                            (cond
                             ;; Both nil
                             ((and (null a) (null b)) t)
                             ;; Both symbols
                             ((and (symbolp a) (symbolp b)) (eq a b))
                             ;; Both numbers
                             ((and (numberp a) (numberp b)) (= a b))
                             ;; Both strings
                             ((and (stringp a) (stringp b)) (string-equal a b))
                             ;; Both cons
                             ((and (consp a) (consp b))
                              (and (funcall my-deep-equal (car a) (car b))
                                   (funcall my-deep-equal (cdr a) (cdr b))))
                             ;; Both vectors
                             ((and (vectorp a) (vectorp b))
                              (and (= (length a) (length b))
                                   (let ((i 0) (same t))
                                     (while (and same (< i (length a)))
                                       (unless (funcall my-deep-equal
                                                        (aref a i) (aref b i))
                                         (setq same nil))
                                       (setq i (1+ i)))
                                     same)))
                             ;; Otherwise not equal
                             (t nil))))
                    ;; Tagged record: (type . fields-vector)
                    (let ((rec1 (cons 'point (vector 1 2 3)))
                          (rec2 (cons 'point (vector 1 2 3)))
                          (rec3 (cons 'point (vector 1 2 4)))
                          (rec4 (cons 'color (vector 1 2 3)))
                          (nested1 (list (cons 'pair (vector "a" '(1 2 3)))))
                          (nested2 (list (cons 'pair (vector "a" '(1 2 3))))))
                      (list
                       ;; Same structure
                       (funcall my-deep-equal rec1 rec2)
                       ;; Different field value
                       (funcall my-deep-equal rec1 rec3)
                       ;; Different tag
                       (funcall my-deep-equal rec1 rec4)
                       ;; Deeply nested records
                       (funcall my-deep-equal nested1 nested2)
                       ;; Compare with built-in equal for reference
                       (equal rec1 rec2)
                       (equal nested1 nested2))))"#;
    let expect = expect_test::expect![[r#""OK (t nil nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: comparison-based binary search tree with equal keys
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_equality_bst_with_equal_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // BST that uses numeric comparison; tests equality in search/insert
    let form = r#"(let ((bst-insert nil)
                        (bst-search nil)
                        (bst-to-sorted-list nil))
                    ;; Node: (key value left right)
                    (setq bst-insert
                          (lambda (tree key val)
                            (if (null tree)
                                (list key val nil nil)
                              (let ((k (car tree))
                                    (v (cadr tree))
                                    (left (caddr tree))
                                    (right (cadddr tree)))
                                (cond
                                 ((= key k) (list k val left right))
                                 ((< key k)
                                  (list k v (funcall bst-insert left key val) right))
                                 (t
                                  (list k v left (funcall bst-insert right key val))))))))
                    (setq bst-search
                          (lambda (tree key)
                            (if (null tree)
                                nil
                              (let ((k (car tree))
                                    (v (cadr tree))
                                    (left (caddr tree))
                                    (right (cadddr tree)))
                                (cond
                                 ((= key k) v)
                                 ((< key k) (funcall bst-search left key))
                                 (t (funcall bst-search right key)))))))
                    (setq bst-to-sorted-list
                          (lambda (tree)
                            (if (null tree)
                                nil
                              (append
                               (funcall bst-to-sorted-list (caddr tree))
                               (list (cons (car tree) (cadr tree)))
                               (funcall bst-to-sorted-list (cadddr tree))))))
                    ;; Build a BST
                    (let ((tree nil))
                      (setq tree (funcall bst-insert tree 5 "five"))
                      (setq tree (funcall bst-insert tree 3 "three"))
                      (setq tree (funcall bst-insert tree 7 "seven"))
                      (setq tree (funcall bst-insert tree 1 "one"))
                      (setq tree (funcall bst-insert tree 4 "four"))
                      (setq tree (funcall bst-insert tree 6 "six"))
                      (setq tree (funcall bst-insert tree 9 "nine"))
                      ;; Update existing key
                      (setq tree (funcall bst-insert tree 5 "FIVE"))
                      (list
                       ;; Search results
                       (funcall bst-search tree 5)
                       (funcall bst-search tree 1)
                       (funcall bst-search tree 9)
                       (funcall bst-search tree 42)
                       ;; In-order traversal
                       (funcall bst-to-sorted-list tree))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"FIVE\" \"one\" \"nine\" nil ((1 . \"one\") (3 . \"three\") (4 . \"four\") (5 . \"FIVE\") (6 . \"six\") (7 . \"seven\") (9 . \"nine\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
