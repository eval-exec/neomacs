//! Oracle parity tests for advanced `cl-loop` patterns:
//! `named` loops with `cl-return-from`, `thereis`/`never`/`always` termination,
//! `initially`/`finally` blocks, `concat`/`vconcat` accumulation,
//! `for ... being hash-keys/hash-values`, `if`/`else` conditional accumulation,
//! `nconc` accumulation, and deeply nested destructuring.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// thereis / never / always termination clauses
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_adv_thereis_never_always() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r##"(progn
  (require 'cl-lib)
  (list
    ;; thereis: returns the first truthy predicate value
    (cl-loop for x in '(1 3 5 7 8 9 11)
             thereis (and (= (% x 2) 0) x))
    ;; thereis on all odd list: returns nil
    (cl-loop for x in '(1 3 5 7 9 11)
             thereis (and (= (% x 2) 0) x))
    ;; never: true when predicate never holds
    (cl-loop for x in '(2 4 6 8 10)
             never (< x 0))
    ;; never: fails when predicate holds
    (cl-loop for x in '(2 4 -1 8 10)
             never (< x 0))
    ;; always: true when predicate always holds
    (cl-loop for x in '(2 4 6 8 10)
             always (> x 0))
    ;; always: fails when predicate fails
    (cl-loop for x in '(2 4 0 8 10)
             always (> x 0))
    ;; thereis with computed value (first square > 50)
    (cl-loop for x in '(1 2 3 4 5 6 7 8 9 10)
             thereis (let ((sq (* x x)))
                       (and (> sq 50) sq)))
    ;; never on empty list: vacuously true
    (cl-loop for x in nil
             never (= x 42))))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// named loops with cl-return-from
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_adv_named_return_from() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; Named outer loop: return from it when inner loop finds something
    (cl-loop named outer
             for xs in '((1 2 3) (4 5 6) (7 8 9) (10 11 12))
             do (cl-loop for x in xs
                         when (> x 7)
                         do (cl-return-from outer (list :found x :in xs))))
    ;; Named loop returning a collected partial result
    (cl-loop named finder
             for x in '(10 20 30 40 50 60 70 80 90 100)
             if (> x 55)
             do (cl-return-from finder (list :first-over-55 x))
             end)
    ;; Named loop with no early return (runs to completion, returns nil)
    (cl-loop named no-match
             for x in '(1 2 3 4 5)
             when (> x 100)
             do (cl-return-from no-match x))
    ;; Nested named loops: outer collects results from inner
    (cl-loop named outer
             for row in '((1 2 3) (4 5 6) (7 8 9))
             collect (cl-loop for x in row sum (* x x)))))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// concat and vconcat accumulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_adv_concat_vconcat() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; concat: build string from parts
    (cl-loop for w in '("hello" " " "world" "!" " " "foo")
             concat w)
    ;; concat with transformation
    (cl-loop for c across "abcdef"
             concat (format "[%c]" c))
    ;; concat with when filter
    (cl-loop for w in '("keep" "drop" "keep" "drop" "keep")
             for i from 0
             when (= (% i 2) 0)
             concat (format "%s " w))
    ;; vconcat: build vector from sub-vectors
    (cl-loop for v in '([1 2] [3 4] [5 6])
             vconcat v)
    ;; vconcat with single-element vectors
    (cl-loop for x in '(10 20 30 40 50)
             vconcat (vector (* x x)))
    ;; concat from numbers formatted as strings
    (cl-loop for i from 1 to 8
             concat (format "%d" (* i i))
             concat (if (< i 8) "," ""))
    ;; vconcat filtering
    (cl-loop for x in '(1 2 3 4 5 6 7 8 9 10)
             when (= (% x 3) 0)
             vconcat (vector x))))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// for ... being hash-keys / hash-values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_adv_hash_iteration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (let ((ht (make-hash-table :test 'equal)))
    (puthash "alpha" 1 ht)
    (puthash "beta" 2 ht)
    (puthash "gamma" 3 ht)
    (puthash "delta" 4 ht)
    (puthash "epsilon" 5 ht)
    (let* (;; Collect all keys (sort for determinism)
           (keys (sort (cl-loop for k being the hash-keys of ht collect k)
                       #'string<))
           ;; Collect all values (sort for determinism)
           (vals (sort (cl-loop for v being the hash-values of ht collect v)
                       #'<))
           ;; Collect key-value pairs using hash-keys with using
           (pairs (sort (cl-loop for k being the hash-keys of ht using (hash-values v)
                                 collect (cons k v))
                        (lambda (a b) (string< (car a) (car b)))))
           ;; Sum of all values via hash-values
           (total (cl-loop for v being the hash-values of ht sum v))
           ;; Count keys matching predicate
           (long-keys (cl-loop for k being the hash-keys of ht
                               when (> (length k) 4)
                               count t))
           ;; Collect transformed values
           (doubled (sort (cl-loop for v being the hash-values of ht
                                   collect (* v 2))
                          #'<)))
      (list :keys keys
            :vals vals
            :pairs pairs
            :total total
            :long-keys long-keys
            :doubled doubled))))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// if / else conditional accumulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_adv_if_else_accumulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; if/else with collect into different lists
    (cl-loop for x in '(1 -2 3 -4 5 -6 7 -8 9 -10)
             if (> x 0) collect x into positives
             else collect x into negatives
             finally return (list :pos positives :neg negatives))
    ;; if/else with sum vs count
    (cl-loop for x in '(5 12 3 18 7 25 9 30 2 15)
             if (> x 10) sum x into big-sum
             else count t into small-count
             finally return (list :big-sum big-sum :small-count small-count))
    ;; Nested if inside when
    (cl-loop for x in '(1 2 3 4 5 6 7 8 9 10 11 12)
             when (> x 3)
             if (= (% x 2) 0) collect x into evens
             else collect x into odds
             end
             finally return (list :evens evens :odds odds))
    ;; Multiple accumulation in single if branch
    (cl-loop for s in '("apple" "a" "banana" "be" "cherry" "c" "date")
             if (> (length s) 2)
               collect s into long-words and
               sum (length s) into total-len
             else
               count t into short-count
             end
             finally return (list :long long-words
                                  :total-len total-len
                                  :short short-count))))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// initially / finally blocks
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_adv_initially_finally() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; finally return with accumulated values
    (cl-loop for x in '(3 1 4 1 5 9 2 6 5 3 5)
             maximize x into mx
             minimize x into mn
             sum x into total
             count t into cnt
             finally return (list :max mx :min mn :sum total :count cnt
                                  :mean (/ (float total) cnt)))
    ;; finally return after collect into named variable
    (cl-loop for i from 1 to 20
             when (= (% i 3) 0) collect i into threes
             when (= (% i 5) 0) collect i into fives
             when (and (= (% i 3) 0) (= (% i 5) 0)) collect i into fifteens
             finally return (list :threes threes :fives fives :fizzbuzz fifteens))
    ;; initially sets up state (with variable)
    (cl-loop with seen = (make-hash-table :test 'equal)
             for x in '(1 2 3 2 4 3 5 1 6 5 7)
             unless (gethash x seen)
               collect x
               and do (puthash x t seen))
    ;; Complex finally with computed summary
    (cl-loop for word in '("the" "quick" "brown" "fox" "jumps" "over" "the" "lazy" "dog")
             sum (length word) into total-chars
             maximize (length word) into longest
             minimize (length word) into shortest
             count t into word-count
             finally return (list :total-chars total-chars
                                  :avg-len (/ (float total-chars) word-count)
                                  :longest longest
                                  :shortest shortest
                                  :word-count word-count))))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// append / nconc accumulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_adv_append_nconc() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; append: flatten one level
    (cl-loop for xs in '((1 2 3) (4 5) (6 7 8 9) (10))
             append xs)
    ;; append with transformation
    (cl-loop for x in '(a b c d)
             append (list x x))
    ;; append with when filter
    (cl-loop for xs in '((1 2) (3) nil (4 5 6) nil (7))
             when xs
             append xs)
    ;; nconc: destructive flatten (on fresh lists)
    (cl-loop for x in '(1 2 3 4 5)
             nconc (list x (* x 10)))
    ;; nconc with conditional
    (cl-loop for x in '(1 2 3 4 5 6 7 8)
             if (= (% x 2) 0)
               nconc (list x (* x x))
             else
               nconc (list (- x))
             end)
    ;; append vs collect equivalence check
    (let ((via-append (cl-loop for x in '(10 20 30)
                               append (list (1- x) x (1+ x))))
          (via-nconc (cl-loop for x in '(10 20 30)
                              nconc (list (1- x) x (1+ x)))))
      (list :same (equal via-append via-nconc)
            :result via-append))
    ;; append building association list
    (cl-loop for k in '(a b c d e)
             for v from 1
             append (list (cons k v)))))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// for ... on (iterating over tails/sublists)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_adv_for_on_tails() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; Basic for...on: collect successive tails
    (cl-loop for tail on '(a b c d e)
             collect (length tail))
    ;; for...on with by: skip by cddr
    (cl-loop for tail on '(1 2 3 4 5 6 7 8) by #'cddr
             collect (car tail))
    ;; All pairs from a list using for...on
    (cl-loop for tail on '(a b c d)
             append (cl-loop for x in (cdr tail)
                             collect (list (car tail) x)))
    ;; Sliding window of size 3 via for...on
    (cl-loop for tail on '(10 20 30 40 50 60 70)
             while (>= (length tail) 3)
             collect (list (nth 0 tail) (nth 1 tail) (nth 2 tail)))
    ;; Check if list is sorted using for...on
    (cl-loop for tail on '(1 3 5 7 9 11)
             always (or (null (cdr tail))
                        (<= (car tail) (cadr tail))))
    ;; Count adjacent duplicates
    (cl-loop for tail on '(1 1 2 3 3 3 4 5 5)
             when (and (cdr tail) (= (car tail) (cadr tail)))
             count t)))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// for ... across (iterating over vectors/strings)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_adv_for_across() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; across a vector: collect squares
    (cl-loop for x across [2 4 6 8 10]
             collect (* x x))
    ;; across a string: collect character codes
    (cl-loop for c across "Hello"
             collect c)
    ;; across with index via parallel for
    (cl-loop for c across "abcdef"
             for i from 0
             collect (cons i c))
    ;; across a vector with filtering
    (cl-loop for x across [1 -2 3 -4 5 -6 7 -8 9 -10]
             when (> x 0) sum x)
    ;; across a string counting vowels
    (cl-loop for c across "The quick brown fox jumps over the lazy dog"
             when (member (downcase c) '(?a ?e ?i ?o ?u))
             count t)
    ;; across nested: matrix row sums from vector of vectors
    (cl-loop for row across [[1 2 3] [4 5 6] [7 8 9]]
             collect (cl-loop for x across row sum x))
    ;; across with maximize/minimize
    (cl-loop for x across [42 17 89 3 56 91 24 68]
             maximize x into mx
             minimize x into mn
             finally return (list :max mx :min mn :range (- mx mn)))))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// Complex multi-clause loops with parallel iteration and multiple accumulators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_adv_multi_clause_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; Three parallel iterators with multiple accumulators
    (cl-loop for x in '(1 2 3 4 5)
             for y in '(10 20 30 40 50)
             for z in '(100 200 300 400 500)
             sum x into sx
             sum y into sy
             sum z into sz
             collect (+ x y z) into sums
             finally return (list :sx sx :sy sy :sz sz :sums sums))
    ;; Running statistics over pairs
    (cl-loop for (label . score) in '(("math" . 92) ("phys" . 88)
                                      ("chem" . 95) ("bio" . 79)
                                      ("eng" . 85))
             maximize score into best
             minimize score into worst
             sum score into total
             count t into n
             collect label into subjects
             finally return (list :subjects subjects
                                  :best best :worst worst
                                  :avg (/ (float total) n)
                                  :range (- best worst)))
    ;; Compute dot product and magnitudes simultaneously
    (cl-loop for a in '(1 2 3 4 5)
             for b in '(5 4 3 2 1)
             sum (* a b) into dot
             sum (* a a) into mag-a-sq
             sum (* b b) into mag-b-sq
             finally return (list :dot dot
                                  :mag-a-sq mag-a-sq
                                  :mag-b-sq mag-b-sq))
    ;; Repeat with = then for state machine simulation
    (cl-loop for input in '(0 1 1 0 1 0 0 1)
             for state = 'idle then
               (cond ((and (eq state 'idle) (= input 1)) 'active)
                     ((and (eq state 'active) (= input 0)) 'idle)
                     (t state))
             collect (list input state))))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// Deeply nested destructuring with (x . y) patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_loop_adv_deep_destructuring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; Dotted pair destructuring with sum
    (cl-loop for (x . y) in '((1 . 10) (2 . 20) (3 . 30) (4 . 40))
             sum (* x y))
    ;; Three-element destructuring collecting transformed results
    (cl-loop for (op a b) in '((+ 1 2) (- 10 3) (* 4 5) (+ 100 200))
             collect (cond ((eq op '+) (+ a b))
                           ((eq op '-) (- a b))
                           ((eq op '*) (* a b))
                           (t 0)))
    ;; Destructuring with nested cons
    (cl-loop for (a . (b . c)) in '((1 . (2 . 3)) (4 . (5 . 6)) (7 . (8 . 9)))
             collect (list a b c (+ a b c)))
    ;; Partial destructuring (ignore some elements)
    (cl-loop for (first _ third) in '((a b c) (d e f) (g h i))
             collect (list first third))
    ;; Destructuring in parallel with counter
    (cl-loop for (name . score) in '(("Alice" . 95) ("Bob" . 87)
                                     ("Carol" . 92) ("Dave" . 78))
             for rank from 1
             when (>= score 90)
             collect (format "#%d %s (%d)" rank name score))))"##;
    let expect = expect_test::expect![[
        r##""OK (300 (3 7 20 300) ((1 2 3 6) (4 5 6 15) (7 8 9 24)) ((a c) (d f) (g i)) (\"#1 Alice (95)\" \"#3 Carol (92)\"))""##
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
