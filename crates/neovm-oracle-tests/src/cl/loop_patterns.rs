//! Oracle parity tests for `cl-loop` macro patterns:
//! `for ... in`, `for ... from ... to`, `collect`, `sum`, `count`,
//! `maximize`/`minimize`, `when`/`unless`, `do`, `finally`, `with`,
//! `append`, `nconc`, multiple `for` clauses, `for ... on`,
//! `for ... across`, `for ... being hash-keys/hash-values`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic for...in with collect, sum, count
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_basic_for_in_collect_sum_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; collect: square each element
    (cl-loop for x in '(1 2 3 4 5) collect (* x x))
    ;; sum: total of elements
    (cl-loop for x in '(10 20 30 40 50) sum x)
    ;; count: how many are positive
    (cl-loop for x in '(-3 -1 0 2 5 -4 7) count (> x 0))
    ;; collect with when filter
    (cl-loop for x in '(1 2 3 4 5 6 7 8 9 10)
             when (= (% x 2) 0)
             collect x)
    ;; sum only odd elements
    (cl-loop for x in '(1 2 3 4 5 6 7 8 9 10)
             when (= (% x 2) 1)
             sum x)
    ;; count with unless
    (cl-loop for x in '("apple" "banana" "" "cherry" "" "date")
             unless (string= x "")
             count t)))"#;
    let expect = expect_test::expect![r#""OK ((1 4 9 16 25) 150 3 (2 4 6 8 10) 25 4)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// for...from...to with by, maximize, minimize
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_for_from_to_maximize_minimize() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; Basic range collect
    (cl-loop for i from 1 to 10 collect i)
    ;; Range with step
    (cl-loop for i from 0 to 20 by 3 collect i)
    ;; Downward range
    (cl-loop for i from 10 downto 1 collect i)
    ;; Downward by step
    (cl-loop for i from 100 downto 0 by 25 collect i)
    ;; Maximize
    (cl-loop for x in '(3 1 4 1 5 9 2 6 5 3 5) maximize x)
    ;; Minimize
    (cl-loop for x in '(3 1 4 1 5 9 2 6 5 3 5) minimize x)
    ;; Maximize of computed value
    (cl-loop for x in '(-5 -2 3 -1 4 -3)
             maximize (abs x))
    ;; Sum of range with filter
    (cl-loop for i from 1 to 100
             when (= (% i 3) 0)
             sum i)))"#;
    let expect = expect_test::expect![
        r#""OK ((1 2 3 4 5 6 7 8 9 10) (0 3 6 9 12 15 18) (10 9 8 7 6 5 4 3 2 1) (100 75 50 25 0) 9 1 5 1683)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple for clauses, with, do, finally
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_multiple_for_with_do_finally() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; Two parallel for clauses (zip)
    (cl-loop for x in '(a b c d)
             for y in '(1 2 3 4)
             collect (list x y))
    ;; Parallel for clauses of different lengths (stops at shorter)
    (cl-loop for x in '(a b c d e f)
             for y in '(1 2 3)
             collect (cons x y))
    ;; for with index
    (cl-loop for x in '(alpha beta gamma delta)
             for i from 0
             collect (list i x))
    ;; with clause (local variable)
    (cl-loop with total = 0
             for x in '(1 2 3 4 5)
             do (setq total (+ total (* x x)))
             finally return total)
    ;; with and collect
    (cl-loop with factor = 10
             for x in '(1 2 3 4 5)
             collect (* x factor))
    ;; do with side-effect accumulation
    (cl-loop with result = nil
             for x in '(a b c d e)
             for i from 1
             do (when (= (% i 2) 1)
                  (push (cons x i) result))
             finally return (nreverse result))))"#;
    let expect = expect_test::expect![
        r#""OK (((a 1) (b 2) (c 3) (d 4)) ((a . 1) (b . 2) (c . 3)) ((0 alpha) (1 beta) (2 gamma) (3 delta)) 55 (10 20 30 40 50) ((a . 1) (c . 3) (e . 5)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// for...on (iterate over cdrs), append, nconc
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_for_on_append_nconc() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; for...on: iterate over successive cdrs
    (cl-loop for tail on '(1 2 3 4 5) collect tail)
    ;; for...on: collect car of each tail
    (cl-loop for tail on '(a b c d) collect (car tail))
    ;; for...on: pairwise comparison
    (cl-loop for tail on '(1 3 5 7 9)
             when (cdr tail)
             collect (list (car tail) (cadr tail) (- (cadr tail) (car tail))))
    ;; append: flatten lists
    (cl-loop for x in '((1 2) (3 4) (5 6) (7 8))
             append x)
    ;; append with filter
    (cl-loop for x in '((1 2 3) (4 5 6) (7 8 9))
             when (> (car x) 3)
             append x)
    ;; nconc: like append but destructive (on fresh lists it's the same)
    (cl-loop for x in '((a b) (c d) (e f))
             nconc (copy-sequence x))
    ;; Combine append with transformation
    (cl-loop for x in '(1 2 3 4)
             append (list x (* x 10) (* x 100)))))"#;
    let expect = expect_test::expect![
        r#""OK (((1 2 3 4 5) (2 3 4 5) (3 4 5) (4 5) (5)) (a b c d) ((1 3 2) (3 5 2) (5 7 2) (7 9 2)) (1 2 3 4 5 6 7 8) (4 5 6 7 8 9) (a b c d e f) (1 10 100 2 20 200 3 30 300 4 40 400))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// for...across (vectors/strings)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_for_across() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; Iterate over vector
    (cl-loop for x across [10 20 30 40 50] collect x)
    ;; Iterate over vector with index
    (cl-loop for x across [a b c d e]
             for i from 0
             collect (list i x))
    ;; Sum over vector
    (cl-loop for x across [1 2 3 4 5 6 7 8 9 10] sum x)
    ;; Iterate over string (chars)
    (cl-loop for ch across "hello"
             collect ch)
    ;; Count vowels in string
    (cl-loop for ch across "the quick brown fox jumps"
             count (memq ch '(?a ?e ?i ?o ?u)))
    ;; Collect uppercase chars from string
    (cl-loop for ch across "Hello World 123"
             when (and (>= ch ?A) (<= ch ?Z))
             collect ch)))"#;
    let expect = expect_test::expect![
        r#""OK ((10 20 30 40 50) ((0 a) (1 b) (2 c) (3 d) (4 e)) 55 (104 101 108 108 111) 6 (72 87))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// for...being hash-keys / hash-values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_hash_table_iteration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (let ((ht (make-hash-table :test 'equal)))
    (puthash "alice" 30 ht)
    (puthash "bob" 25 ht)
    (puthash "charlie" 35 ht)
    (puthash "diana" 28 ht)
    (list
      ;; Collect all keys (sorted for determinism)
      (sort (cl-loop for k being the hash-keys of ht collect k) #'string<)
      ;; Collect all values (sorted)
      (sort (cl-loop for v being the hash-values of ht collect v) #'<)
      ;; Collect key-value pairs
      (sort (cl-loop for k being the hash-keys of ht using (hash-values v)
                     collect (cons k v))
            (lambda (a b) (string< (car a) (car b))))
      ;; Sum all values
      (cl-loop for v being the hash-values of ht sum v)
      ;; Count entries matching a predicate
      (cl-loop for v being the hash-values of ht count (>= v 30))
      ;; Collect keys where value > 27
      (sort (cl-loop for k being the hash-keys of ht using (hash-values v)
                     when (> v 27)
                     collect k)
            #'string<))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"alice\" \"bob\" \"charlie\" \"diana\") (25 28 30 35) ((\"alice\" . 30) (\"bob\" . 25) (\"charlie\" . 35) (\"diana\" . 28)) 118 2 (\"alice\" \"charlie\" \"diana\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: combined patterns (real-world-like queries)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_combined_complex_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (let ((students '(("Alice" . ((math . 95) (sci . 88) (eng . 92)))
                    ("Bob" . ((math . 72) (sci . 85) (eng . 78)))
                    ("Charlie" . ((math . 88) (sci . 92) (eng . 90)))
                    ("Diana" . ((math . 65) (sci . 70) (eng . 85)))
                    ("Eve" . ((math . 98) (sci . 95) (eng . 97))))))
    (list
      ;; Compute average score per student
      (cl-loop for student in students
               collect
               (let* ((name (car student))
                      (scores (cdr student))
                      (total (cl-loop for pair in scores sum (cdr pair)))
                      (avg (/ total (length scores))))
                 (list name avg)))
      ;; Find students with all scores >= 80
      (cl-loop for student in students
               when (cl-loop for pair in (cdr student)
                             always (>= (cdr pair) 80))
               collect (car student))
      ;; Maximize: highest single score across all students
      (cl-loop for student in students
               maximize
               (cl-loop for pair in (cdr student) maximize (cdr pair)))
      ;; Collect all (name subject score) triples where score >= 90
      (cl-loop for student in students
               append
               (cl-loop for pair in (cdr student)
                        when (>= (cdr pair) 90)
                        collect (list (car student) (car pair) (cdr pair))))
      ;; Count total number of scores across all students
      (cl-loop for student in students
               sum (length (cdr student)))
      ;; Top scorer per subject
      (cl-loop for subj in '(math sci eng)
               collect
               (cons subj
                     (car (cl-loop for student in students
                                   with best-name = nil
                                   with best-score = -1
                                   do (let ((score (cdr (assq subj (cdr student)))))
                                        (when (> score best-score)
                                          (setq best-name (car student)
                                                best-score score)))
                                   finally return (list best-name best-score))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"Alice\" 91) (\"Bob\" 78) (\"Charlie\" 90) (\"Diana\" 73) (\"Eve\" 96)) (\"Alice\" \"Charlie\" \"Eve\") 98 ((\"Alice\" math 95) (\"Alice\" eng 92) (\"Charlie\" sci 92) (\"Charlie\" eng 90) (\"Eve\" math 98) (\"Eve\" sci 95) (\"Eve\" eng 97)) 15 ((math . \"Eve\") (sci . \"Eve\") (eng . \"Eve\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Edge cases and special patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; Empty list
    (cl-loop for x in nil collect x)
    ;; Empty vector
    (cl-loop for x across [] collect x)
    ;; Single element
    (cl-loop for x in '(42) collect (* x 2))
    ;; from > to with positive step: no iterations
    (cl-loop for i from 10 to 5 collect i)
    ;; Collect with multiple values accumulated
    (cl-loop for x in '(1 2 3 4 5 6)
             if (= (% x 2) 0)
               collect x into evens
             else
               collect x into odds
             end
             finally return (list evens odds))
    ;; Destructuring in for...in
    (cl-loop for (key . val) in '((a . 1) (b . 2) (c . 3))
             collect (list val key))
    ;; Repeat clause
    (cl-loop repeat 5 collect 'x)
    ;; Thereis: return first match
    (cl-loop for x in '(1 3 5 6 7 8)
             thereis (and (= (% x 2) 0) x))))"#;
    let expect = expect_test::expect![
        r#""OK (nil nil (84) nil ((2 4 6) (1 3 5)) ((1 a) (2 b) (3 c)) (x x x x x) 6)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}
