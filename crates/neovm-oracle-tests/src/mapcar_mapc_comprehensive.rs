//! Comprehensive oracle parity tests for mapping functions:
//! mapcar with lambdas/subrs/symbols, mapc return value and side effects,
//! mapcan (nconc variant), mapconcat with separator, cl-mapcar with multiple
//! lists, cl-mapc with multiple lists, cl-mapcan, mapping over empty lists,
//! mapping with index tracking, nested maps, map with error handling.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// mapcar with lambdas, subrs (built-in functions), and quoted symbols
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapcar_mapc_comp_mapcar_function_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; mapcar with a lambda
  (mapcar (lambda (x) (* x x x)) '(1 2 3 4 5))
  ;; mapcar with a subr (built-in function)
  (mapcar #'1+ '(10 20 30 40))
  ;; mapcar with a quoted symbol name
  (mapcar 'car '((a 1) (b 2) (c 3)))
  ;; mapcar with #'symbol-name on symbols
  (mapcar #'symbol-name '(foo bar baz))
  ;; mapcar with #'length on strings
  (mapcar #'length '("hello" "hi" "hey" "howdy"))
  ;; mapcar with #'not for boolean negation
  (mapcar #'not '(t nil t nil nil t))
  ;; mapcar with #'type-of
  (mapcar #'type-of '(1 "str" sym 3.14 nil t))
  ;; mapcar with multi-expression lambda (let + arithmetic)
  (mapcar (lambda (x)
            (let ((doubled (* x 2))
                  (tripled (* x 3)))
              (+ doubled tripled)))
          '(1 2 3 4))
  ;; mapcar with identity function
  (mapcar #'identity '(a b c d)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 8 27 64 125) (11 21 31 41) (a b c) (\"foo\" \"bar\" \"baz\") (5 2 3 5) (nil t nil t t nil) (integer string symbol float symbol symbol) (5 10 15 20) (a b c d))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapc: return value is the original list, side effects matter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapcar_mapc_comp_mapc_return_and_side_effects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; mapc returns the input list, not mapped results
  (let ((input '(1 2 3)))
    (eq input (mapc #'identity input)))
  ;; mapc side effects: accumulate sum
  (let ((sum 0))
    (mapc (lambda (x) (setq sum (+ sum x))) '(1 2 3 4 5))
    sum)
  ;; mapc side effects: build list via push pattern
  (let ((acc nil))
    (mapc (lambda (x) (setq acc (cons (* x 2) acc))) '(1 2 3))
    (nreverse acc))
  ;; mapc side effects: populate hash table
  (let ((ht (make-hash-table :test 'equal)))
    (mapc (lambda (pair)
            (puthash (car pair) (cdr pair) ht))
          '((name . "Alice") (age . 30) (city . "NYC")))
    (list (gethash "name" ht)
          (gethash "age" ht)
          (gethash "city" ht)))
  ;; mapc on empty list: no side effects
  (let ((count 0))
    (mapc (lambda (x) (setq count (1+ count))) nil)
    count)
  ;; mapc return value for empty list
  (mapc #'identity nil))"#;
    let expect = expect_test::expect![[r#""OK (t 15 (2 4 6) (nil nil nil) 0 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapcan: like mapcar but uses nconc to concatenate results
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapcar_mapc_comp_mapcan() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; mapcan basic: flatten by returning lists
  (mapcan (lambda (x) (list x (* x 10))) '(1 2 3))
  ;; mapcan as filter: return list or nil
  (mapcan (lambda (x) (if (> x 3) (list x) nil)) '(1 2 3 4 5 6))
  ;; mapcan with variable-length results
  (mapcan (lambda (n) (make-list n n)) '(1 2 3))
  ;; mapcan on empty list
  (mapcan (lambda (x) (list x)) nil)
  ;; mapcan where all results are nil (filter removes everything)
  (mapcan (lambda (x) (if (> x 100) (list x) nil)) '(1 2 3 4 5))
  ;; mapcan for generating pairs
  (mapcan (lambda (x) (list (cons x (* x x)))) '(1 2 3 4 5))
  ;; mapcan to interleave elements with separator
  (let ((result (mapcan (lambda (x) (list x 'sep)) '(a b c))))
    (nbutlast result)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 10 2 20 3 30) (4 5 6) (1 2 2 3 3 3) nil nil ((1 . 1) (2 . 4) (3 . 9) (4 . 16) (5 . 25)) (a sep b sep c))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapconcat with various separators and transform functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapcar_mapc_comp_mapconcat_separators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; mapconcat with comma-space separator
  (mapconcat #'number-to-string '(1 2 3 4 5) ", ")
  ;; mapconcat with empty separator (concatenation)
  (mapconcat #'symbol-name '(hello world) "")
  ;; mapconcat with newline separator
  (mapconcat #'identity '("line1" "line2" "line3") "\n")
  ;; mapconcat with custom transform lambda
  (mapconcat (lambda (x) (format "[%s]" x)) '(a b c) " -> ")
  ;; mapconcat on single-element list (no separator in output)
  (mapconcat #'number-to-string '(42) ", ")
  ;; mapconcat on empty list
  (mapconcat #'number-to-string nil ", ")
  ;; mapconcat with multi-char separator
  (mapconcat #'symbol-name '(foo bar baz) " | ")
  ;; mapconcat with #'identity on string list
  (mapconcat #'identity '("alpha" "beta" "gamma") "-")
  ;; mapconcat to build CSV row
  (mapconcat (lambda (x) (format "%S" x))
             '("name" 42 3.14) ","))"#;
    let expect = expect_test::expect![[
        r#""OK (\"1, 2, 3, 4, 5\" \"helloworld\" \"line1\\nline2\\nline3\" \"[a] -> [b] -> [c]\" \"42\" \"\" \"foo | bar | baz\" \"alpha-beta-gamma\" \"\\\"name\\\",42,3.14\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-mapcar with multiple input lists (parallel mapping)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapcar_mapc_comp_cl_mapcar_multi_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; cl-mapcar with two lists: zip with addition
    (cl-mapcar #'+ '(1 2 3) '(10 20 30))
    ;; cl-mapcar with two lists of different lengths (stops at shortest)
    (cl-mapcar #'+ '(1 2 3 4 5) '(10 20))
    ;; cl-mapcar with three lists
    (cl-mapcar #'list '(a b c) '(1 2 3) '(x y z))
    ;; cl-mapcar with three lists, different lengths
    (cl-mapcar #'list '(a b c d) '(1 2) '(x y z))
    ;; cl-mapcar with lambda combining two lists
    (cl-mapcar (lambda (name score) (format "%s: %d" name score))
               '("Alice" "Bob" "Carol")
               '(95 87 92))
    ;; cl-mapcar with cons to create alist
    (cl-mapcar #'cons '(a b c) '(1 2 3))
    ;; cl-mapcar with one empty list
    (cl-mapcar #'+ '(1 2 3) nil)
    ;; cl-mapcar with single list (like regular mapcar)
    (cl-mapcar #'1+ '(10 20 30))))"#;
    let expect = expect_test::expect![[
        r#""OK ((11 22 33) (11 22) ((a 1 x) (b 2 y) (c 3 z)) ((a 1 x) (b 2 y)) (\"Alice: 95\" \"Bob: 87\" \"Carol: 92\") ((a . 1) (b . 2) (c . 3)) nil (11 21 31))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-mapc with multiple lists (side effects, parallel)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapcar_mapc_comp_cl_mapc_multi_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; cl-mapc with two lists: accumulate products
    (let ((acc nil))
      (cl-mapc (lambda (a b) (setq acc (cons (* a b) acc)))
               '(1 2 3) '(10 20 30))
      (nreverse acc))
    ;; cl-mapc with three lists
    (let ((acc nil))
      (cl-mapc (lambda (a b c) (setq acc (cons (list a b c) acc)))
               '(x y z) '(1 2 3) '(p q r))
      (nreverse acc))
    ;; cl-mapc returns first list argument
    (let ((first-list '(1 2 3)))
      (eq first-list (cl-mapc #'identity first-list)))
    ;; cl-mapc with different-length lists: stops at shortest
    (let ((count 0))
      (cl-mapc (lambda (a b) (setq count (1+ count)))
               '(1 2 3 4 5) '(a b))
      count)))"#;
    let expect = expect_test::expect![[r#""OK ((10 40 90) ((x 1 p) (y 2 q) (z 3 r)) t 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-mapcan: like cl-mapcar but concatenates results with nconc
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapcar_mapc_comp_cl_mapcan() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; cl-mapcan basic: expand each element into a list
    (cl-mapcan (lambda (x) (list x (* x 10) (* x 100))) '(1 2 3))
    ;; cl-mapcan as filter
    (cl-mapcan (lambda (x) (if (cl-evenp x) (list x) nil))
               '(1 2 3 4 5 6 7 8))
    ;; cl-mapcan with two lists
    (cl-mapcan (lambda (a b) (list (cons a b)))
               '(x y z) '(1 2 3))
    ;; cl-mapcan on empty list
    (cl-mapcan (lambda (x) (list x)) nil)
    ;; cl-mapcan where all results are nil
    (cl-mapcan (lambda (x) nil) '(1 2 3))))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 10 100 2 20 200 3 30 300) (2 4 6 8) ((x . 1) (y . 2) (z . 3)) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Mapping over empty lists: edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapcar_mapc_comp_empty_list_mapping() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; mapcar on nil
  (mapcar #'1+ nil)
  ;; mapc on nil
  (mapc #'1+ nil)
  ;; mapcan on nil
  (mapcan (lambda (x) (list x)) nil)
  ;; mapconcat on nil
  (mapconcat #'identity nil ", ")
  ;; Verify return types for empty mapping
  (null (mapcar #'identity nil))
  (null (mapcan #'list nil))
  (equal "" (mapconcat #'identity nil ","))
  ;; Mapping with side effects on empty list: no side effects triggered
  (let ((counter 0))
    (mapcar (lambda (x) (setq counter (1+ counter))) nil)
    counter))"#;
    let expect = expect_test::expect![[r#""OK (nil nil nil \"\" t t t 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Mapping with index tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapcar_mapc_comp_index_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Track index via closure counter
  (let ((idx 0))
    (mapcar (lambda (x)
              (let ((result (cons idx x)))
                (setq idx (1+ idx))
                result))
            '(a b c d e)))
  ;; Number-sequence + cl-mapcar for explicit indices
  (progn
    (require 'cl-lib)
    (cl-mapcar (lambda (i x) (list i x))
               (number-sequence 0 4) '(a b c d e)))
  ;; Index-based conditional mapping
  (let ((idx 0))
    (mapcar (lambda (x)
              (let ((result (if (= (% idx 2) 0) (upcase x) (downcase x))))
                (setq idx (1+ idx))
                result))
            '("Hello" "World" "Foo" "Bar")))
  ;; Track both index and running total
  (let ((idx 0) (running-sum 0))
    (mapcar (lambda (x)
              (setq running-sum (+ running-sum x))
              (let ((result (list idx x running-sum)))
                (setq idx (1+ idx))
                result))
            '(10 20 30 40))))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 . a) (1 . b) (2 . c) (3 . d) (4 . e)) ((0 a) (1 b) (2 c) (3 d) (4 e)) (\"HELLO\" \"world\" \"FOO\" \"bar\") ((0 10 10) (1 20 30) (2 30 60) (3 40 100)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested maps: map within map
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapcar_mapc_comp_nested_maps() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; mapcar inside mapcar: matrix operations
  (mapcar (lambda (row) (mapcar #'1+ row))
          '((1 2 3) (4 5 6) (7 8 9)))
  ;; Nested mapcar to compute outer product
  (mapcar (lambda (x)
            (mapcar (lambda (y) (* x y))
                    '(1 2 3)))
          '(10 20 30))
  ;; Double nesting: 3D structure
  (mapcar (lambda (plane)
            (mapcar (lambda (row)
                      (mapcar #'1+ row))
                    plane))
          '(((1 2) (3 4)) ((5 6) (7 8))))
  ;; Flat-map pattern: mapcan with inner mapcar
  (mapcan (lambda (row)
            (mapcar (lambda (x) (* x 2)) row))
          '((1 2 3) (4 5 6)))
  ;; Nested map with outer variable capture
  (mapcar (lambda (multiplier)
            (mapcar (lambda (x) (* x multiplier))
                    '(1 2 3)))
          '(1 10 100)))"#;
    let expect = expect_test::expect![[
        r#""OK (((2 3 4) (5 6 7) (8 9 10)) ((10 20 30) (20 40 60) (30 60 90)) (((2 3) (4 5)) ((6 7) (8 9))) (2 4 6 8 10 12) ((1 2 3) (10 20 30) (100 200 300)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Map with error handling via condition-case
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapcar_mapc_comp_error_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; mapcar with condition-case: safe division
  (mapcar (lambda (x)
            (condition-case err
                (/ 100 x)
              (arith-error 'division-by-zero)))
          '(10 5 0 2 0 1))
  ;; mapcar with condition-case: safe car
  (mapcar (lambda (x)
            (condition-case nil
                (car x)
              (wrong-type-argument 'not-a-list)))
          '((a b) 42 (c d) "str" nil))
  ;; mapcar with condition-case: safe string-to-number with validation
  (mapcar (lambda (s)
            (condition-case nil
                (let ((n (string-to-number s)))
                  (if (and (= n 0) (not (string= s "0")))
                      'not-a-number
                    n))
              (wrong-type-argument 'type-error)))
          '("42" "hello" "0" "3.14" ""))
  ;; Accumulate errors separately from successes
  (let ((successes nil)
        (errors nil))
    (mapc (lambda (x)
            (condition-case err
                (progn
                  (setq successes (cons (/ 100 x) successes)))
              (arith-error
               (setq errors (cons (list x (cadr err)) errors)))))
          '(10 5 0 25 0 50))
    (list (nreverse successes) (nreverse errors))))"#;
    let expect = expect_test::expect![[
        r#""OK ((10 20 division-by-zero 50 division-by-zero 100) (a not-a-list c not-a-list nil) (42 not-a-number 0 3.14 not-a-number) ((10 20 4 2) ((0 nil) (0 nil))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
