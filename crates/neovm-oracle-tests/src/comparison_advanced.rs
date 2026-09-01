//! Oracle parity tests for advanced comparison and equality:
//! `eq` vs `eql` vs `equal` across all types, deeply nested `equal`,
//! `string-equal` vs `equal`, `=` vs `eq` for numbers,
//! `compare-strings` with all 6+ parameters, IGNORE-CASE,
//! custom deep-equal implementation, and sort stability via equal.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// eq vs eql vs equal on all types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eq_eql_equal_all_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Systematic comparison of eq/eql/equal across integers, floats,
    // strings, symbols, cons cells, vectors
    let form = r#"(let ((sym 'foo)
                        (int1 42)
                        (flt 3.14)
                        (str "hello")
                        (lst '(1 2 3))
                        (vec [1 2 3]))
  (list
   ;; Symbols: eq = eql = equal for same symbol
   (list (eq sym sym) (eql sym sym) (equal sym sym)
         (eq 'foo 'foo) (eq 'foo 'bar))
   ;; Integers: eq works for fixnums, eql and equal also work
   (list (eq int1 42) (eql int1 42) (equal int1 42)
         (eq 0 0) (eql 0 0))
   ;; Floats: eq only if same object, eql compares value, equal compares value
   (list (let ((x 3.14)) (eq x x))
         (eql 3.14 3.14) (equal 3.14 3.14)
         (eql 0.0 -0.0) (equal 0.0 -0.0))
   ;; Strings: eq only if same object, equal compares contents
   (list (eq "hello" "hello") (equal "hello" "hello")
         (let ((s "hello")) (eq s s))
         (equal "" ""))
   ;; Cons: eq only if same object, equal recurses
   (list (eq '(1 2) '(1 2)) (equal '(1 2) '(1 2))
         (let ((c '(1 2))) (eq c c))
         (equal '(a . b) '(a . b)))
   ;; Vectors: eq only if same object, equal compares elements
   (list (eq [1 2] [1 2]) (equal [1 2] [1 2])
         (let ((v [1 2])) (eq v v))
         (equal [] []))))"#;
    let expect = expect_test::expect![[
        r#""OK ((t t t t nil) (t t t t t) (t t t nil nil) (nil t t t) (nil t t t) (nil t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// equal on deeply nested structures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_equal_deeply_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build deeply nested structures and compare them with equal
    let form = r#"(let* ((deep1 '(a (b (c (d (e (f (g))))))))
                         (deep2 '(a (b (c (d (e (f (g))))))))
                         (deep3 '(a (b (c (d (e (f (h))))))))
                         ;; Mixed nesting: lists within vectors within lists
                         (mixed1 (list 1 [2 (3 4) [5 6]] '(7 . 8)))
                         (mixed2 (list 1 [2 (3 4) [5 6]] '(7 . 8)))
                         (mixed3 (list 1 [2 (3 4) [5 7]] '(7 . 8)))
                         ;; Alist structures
                         (alist1 '((a . 1) (b . (2 3)) (c . [4 5])))
                         (alist2 '((a . 1) (b . (2 3)) (c . [4 5])))
                         (alist3 '((a . 1) (b . (2 3)) (c . [4 6]))))
  (list
   (equal deep1 deep2)
   (equal deep1 deep3)
   (equal mixed1 mixed2)
   (equal mixed1 mixed3)
   (equal alist1 alist2)
   (equal alist1 alist3)
   ;; nil comparisons
   (equal nil nil)
   (equal '() '())
   (equal nil '())
   ;; Edge: empty nested
   (equal '(()) '(()))
   (equal '(nil) '(nil))
   (equal '((nil)) '((nil)))))"#;
    let expect = expect_test::expect![[r#""OK (t nil t nil t nil t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-equal vs equal on strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_equal_vs_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // string-equal compares only the text, ignoring text properties.
    // equal on unpropertized strings behaves same as string-equal.
    // Also test string-equal with symbols (symbol-name is used).
    let form = r#"(list
  ;; Basic string-equal
  (string-equal "abc" "abc")
  (string-equal "abc" "ABC")
  (string-equal "" "")
  (string-equal "a" "b")
  ;; string-equal with symbols (uses symbol-name)
  (string-equal 'foo "foo")
  (string-equal "bar" 'bar)
  (string-equal 'baz 'baz)
  ;; equal on strings (same as string-equal for unpropertized)
  (equal "abc" "abc")
  (equal "" "")
  ;; Multibyte strings
  (string-equal "hello" "hello")
  (equal "hello" "hello")
  ;; Different lengths
  (string-equal "ab" "abc")
  (string-equal "abc" "ab")
  ;; Symbols that look like numbers
  (string-equal '42 "42"))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 42)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// = vs eq for numbers (fixnum identity, float comparison)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numeric_eq_vs_equal_sign() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // = compares numeric value (works across int/float).
    // eq compares identity (works for fixnums, not for floats).
    // eql compares type+value (int != float even if same numeric value).
    let form = r#"(list
  ;; Small fixnums: eq works because they're cached
  (eq 1 1) (= 1 1) (eql 1 1)
  ;; Large fixnums: eq still works in Emacs
  (eq 100000 100000) (= 100000 100000) (eql 100000 100000)
  ;; Float vs int: = says yes, eq says no, eql says no
  (= 1 1.0) (eq 1 1.0) (eql 1 1.0)
  ;; Float vs float: = compares value, eq compares identity
  (= 3.14 3.14) (eql 3.14 3.14)
  ;; Zero special cases
  (= 0 0.0) (eql 0 0.0) (= 0 -0) (= 0.0 -0.0)
  ;; Negative numbers
  (eq -5 -5) (= -5 -5) (eql -5 -5)
  ;; Arithmetic results
  (= (+ 1 2) 3) (= (* 2 3) 6)
  (eql (+ 1 2) 3) (eql (+ 1.0 2.0) 3.0))"#;
    let expect =
        expect_test::expect![[r#""OK (t t t t t t t nil nil t t t nil t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// compare-strings with all 6 parameters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compare_strings_all_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Exercise all 6 positional params: STR1 START1 END1 STR2 START2 END2
    let form = r#"(list
  ;; Full range comparison
  (compare-strings "abcdef" 0 6 "abcdef" 0 6)
  ;; Subranges that match
  (compare-strings "xxabcyy" 2 5 "zzabcww" 2 5)
  ;; Subranges that differ
  (compare-strings "xxabcyy" 2 5 "zzabdww" 2 5)
  ;; Start1/End1 differ from Start2/End2 but content matches
  (compare-strings "---hello---" 3 8 "hello" 0 5)
  ;; nil means beginning/end
  (compare-strings "hello" nil nil "hello" nil nil)
  (compare-strings "hello" 0 nil "hello" nil 5)
  ;; Prefix comparison (str1 shorter)
  (compare-strings "ab" nil nil "abcd" nil nil)
  ;; Prefix comparison (str2 shorter)
  (compare-strings "abcd" nil nil "ab" nil nil)
  ;; Single character ranges
  (compare-strings "abc" 1 2 "xbx" 1 2)
  ;; Empty ranges
  (compare-strings "abc" 0 0 "xyz" 0 0)
  ;; Overlapping range results
  (compare-strings "abcdef" 0 3 "abxdef" 0 3))"#;
    let expect = expect_test::expect![[r#""OK (t t -3 t t t -3 3 t t -3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// compare-strings with IGNORE-CASE
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compare_strings_ignore_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // 7th parameter controls case sensitivity
    let form = r#"(list
  ;; Case-sensitive: differ
  (compare-strings "Hello" nil nil "hello" nil nil)
  ;; Case-insensitive: match
  (compare-strings "Hello" nil nil "hello" nil nil t)
  ;; Mixed case subranges
  (compare-strings "xxABCyy" 2 5 "zzabcww" 2 5 t)
  (compare-strings "xxABCyy" 2 5 "zzabcww" 2 5 nil)
  ;; Case-insensitive ordering
  (compare-strings "AAA" nil nil "aab" nil nil t)
  (compare-strings "ZZZ" nil nil "aaa" nil nil t)
  ;; Ignore-case with equal strings
  (compare-strings "HELLO WORLD" nil nil "hello world" nil nil t)
  ;; Partial match with case folding
  (compare-strings "FoObAr" 0 3 "fOo" nil nil t))"#;
    let expect = expect_test::expect![[r#""OK (-1 t t -1 -3 1 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: implementing deep-equal with custom comparison
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_custom_deep_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a custom deep-equal that handles lists, vectors, and atoms,
    // then verify it agrees with built-in equal on various inputs
    let form = r#"(progn
  (fset 'neovm--deep-eq
    (lambda (a b)
      (cond
        ;; Both nil
        ((and (null a) (null b)) t)
        ;; One nil
        ((or (null a) (null b)) nil)
        ;; Both cons
        ((and (consp a) (consp b))
         (and (funcall 'neovm--deep-eq (car a) (car b))
              (funcall 'neovm--deep-eq (cdr a) (cdr b))))
        ;; Both vectors
        ((and (vectorp a) (vectorp b))
         (and (= (length a) (length b))
              (let ((i 0) (len (length a)) (result t))
                (while (and result (< i len))
                  (unless (funcall 'neovm--deep-eq (aref a i) (aref b i))
                    (setq result nil))
                  (setq i (1+ i)))
                result)))
        ;; Both strings
        ((and (stringp a) (stringp b))
         (string-equal a b))
        ;; Both numbers
        ((and (numberp a) (numberp b))
         (= a b))
        ;; Symbols or other atoms
        (t (eq a b)))))
  (unwind-protect
      (let ((test-cases
             (list
              (cons '(1 2 (3 4) [5 6]) '(1 2 (3 4) [5 6]))
              (cons '(1 2 (3 4) [5 6]) '(1 2 (3 4) [5 7]))
              (cons nil nil)
              (cons '(a . b) '(a . b))
              (cons [1 [2 [3]]] [1 [2 [3]]])
              (cons "hello" "hello")
              (cons 42 42)
              (cons '((a 1) (b 2)) '((a 1) (b 2)))
              (cons '((a 1) (b 2)) '((a 1) (b 3))))))
        ;; For each test case, verify our deep-eq agrees with equal
        (mapcar (lambda (pair)
                  (let ((a (car pair)) (b (cdr pair)))
                    (list (equal a b)
                          (funcall 'neovm--deep-eq a b)
                          (eq (if (equal a b) t nil)
                              (if (funcall 'neovm--deep-eq a b) t nil)))))
                test-cases))
    (fmakunbound 'neovm--deep-eq)))"#;
    let expect = expect_test::expect![[
        r#""OK ((t t t) (nil nil t) (t t t) (t t t) (t t t) (t t t) (t t t) (t t t) (nil nil t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: sort stability test via equal comparison
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_stability_via_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Sort a list of pairs by first element; pairs with equal first elements
    // should preserve relative order (stability test). Emacs sort is stable.
    // Verify by comparing sorted result with manually expected output.
    let form = r#"(let* ((data (list '(1 . a) '(3 . b) '(1 . c) '(2 . d)
                                '(3 . e) '(2 . f) '(1 . g)))
                         ;; Sort by car using <
                         (sorted (sort (copy-sequence data)
                                       (lambda (x y) (< (car x) (car y)))))
                         ;; Extract just the cdrs grouped by car
                         (group-1 (mapcar 'cdr
                                    (seq-filter (lambda (x) (= (car x) 1))
                                                sorted)))
                         (group-2 (mapcar 'cdr
                                    (seq-filter (lambda (x) (= (car x) 2))
                                                sorted)))
                         (group-3 (mapcar 'cdr
                                    (seq-filter (lambda (x) (= (car x) 3))
                                                sorted))))
  (list sorted
        ;; Within each group, original order is preserved (stable sort)
        group-1
        group-2
        group-3
        ;; Overall length preserved
        (= (length sorted) (length data))
        ;; Each element appears exactly once
        (equal (sort (mapcar 'cdr (copy-sequence sorted))
                     (lambda (a b) (string< (symbol-name a) (symbol-name b))))
               '(a b c d e f g))))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 . a) (1 . c) (1 . g) (2 . d) (2 . f) (3 . b) (3 . e)) (a c g) (d f) (b e) t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
