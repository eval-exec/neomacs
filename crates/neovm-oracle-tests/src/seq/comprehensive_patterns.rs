//! Oracle parity tests for seq.el functions with comprehensive patterns:
//! seq-map, seq-filter, seq-reduce on lists/vectors/strings,
//! seq-find, seq-some, seq-every-p, seq-count, seq-length, seq-elt,
//! seq-uniq with custom test, seq-remove, seq-sort, data pipelines,
//! and set operations using seq functions.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// seq-map, seq-filter, seq-reduce on lists, vectors, strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_map_filter_reduce_all_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test seq-map, seq-filter, seq-reduce across all three sequence types
    let form = r#"(progn (require (quote cl-lib)) (list
      ;; seq-map on list
      (seq-map #'1+ '(1 2 3 4 5))
      ;; seq-map on vector
      (seq-map #'1+ [10 20 30])
      ;; seq-map on string (char codes)
      (seq-map #'upcase "hello")
      ;; seq-filter on list
      (seq-filter #'cl-evenp '(1 2 3 4 5 6 7 8))
      ;; seq-filter on vector
      (seq-filter (lambda (x) (> x 20)) [10 25 30 5 40])
      ;; seq-filter on string
      (seq-filter (lambda (c) (>= c ?a)) "Hello World")
      ;; seq-reduce on list
      (seq-reduce #'+ '(1 2 3 4 5) 0)
      ;; seq-reduce on vector
      (seq-reduce #'* [2 3 4] 1)
      ;; seq-reduce on string (sum of char codes)
      (seq-reduce (lambda (acc c) (+ acc c)) "abc" 0)
      ;; seq-reduce with complex accumulator (build alist of char frequencies)
      (let ((result (seq-reduce
                      (lambda (acc c)
                        (let ((entry (assq c acc)))
                          (if entry
                              (progn (setcdr entry (1+ (cdr entry))) acc)
                            (cons (cons c 1) acc))))
                      "abracadabra" nil)))
        (sort result (lambda (a b) (< (car a) (car b)))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((2 3 4 5 6) (11 21 31) (72 69 76 76 79) (2 4 6 8) (25 30 40) (101 108 108 111 111 114 108 100) 15 24 294 ((97 . 5) (98 . 2) (99 . 1) (100 . 1) (114 . 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-find, seq-some, seq-every-p
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_find_some_every() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn (require (quote cl-lib)) (list
      ;; seq-find: first even number
      (seq-find #'cl-evenp '(1 3 5 4 7 8))
      ;; seq-find: not found returns nil
      (seq-find #'cl-evenp '(1 3 5 7))
      ;; seq-find: with default value
      (seq-find #'cl-evenp '(1 3 5 7) 'not-found)
      ;; seq-find on vector
      (seq-find (lambda (x) (> x 100)) [10 50 200 300])
      ;; seq-find on string
      (seq-find (lambda (c) (= c ?l)) "hello")
      ;; seq-some: returns first truthy predicate result
      (seq-some (lambda (x) (and (> x 5) (* x 10))) '(1 2 3 6 7))
      ;; seq-some: none match
      (seq-some (lambda (x) (and (> x 100) x)) '(1 2 3))
      ;; seq-some on vector
      (seq-some #'cl-evenp [1 3 5 7 8])
      ;; seq-some on empty sequence
      (seq-some #'identity nil)
      ;; seq-every-p: all match
      (seq-every-p #'cl-evenp '(2 4 6 8))
      ;; seq-every-p: not all match
      (seq-every-p #'cl-evenp '(2 4 5 8))
      ;; seq-every-p on vector
      (seq-every-p (lambda (x) (> x 0)) [1 2 3 4])
      ;; seq-every-p on empty (vacuously true)
      (seq-every-p #'cl-evenp nil)
      ;; seq-every-p on string
      (seq-every-p (lambda (c) (and (>= c ?a) (<= c ?z))) "hello")
      (seq-every-p (lambda (c) (and (>= c ?a) (<= c ?z))) "Hello")))"#;
    let expect =
        expect_test::expect![[r#""OK (4 nil not-found 200 108 60 nil t nil t nil t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-count, seq-length, seq-elt
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_count_length_elt() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn (require (quote cl-lib)) (list
      ;; seq-count
      (seq-count #'cl-evenp '(1 2 3 4 5 6))
      (seq-count (lambda (x) (> x 3)) '(1 2 3 4 5 6))
      (seq-count #'cl-evenp [1 2 3 4 5 6])
      (seq-count (lambda (c) (= c ?l)) "hello world")
      (seq-count #'identity nil)
      ;; seq-length
      (seq-length '(a b c d))
      (seq-length [1 2 3])
      (seq-length "hello")
      (seq-length nil)
      ;; seq-elt
      (seq-elt '(a b c d e) 0)
      (seq-elt '(a b c d e) 2)
      (seq-elt '(a b c d e) 4)
      (seq-elt [10 20 30 40] 1)
      (seq-elt [10 20 30 40] 3)
      (seq-elt "abcde" 0)
      (seq-elt "abcde" 4)))"#;
    let expect = expect_test::expect![[r#""OK (3 3 3 3 0 4 3 5 0 a c e 20 40 97 101)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-uniq with custom test function, seq-remove
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_uniq_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn (require (quote cl-lib)) (list
      ;; seq-uniq: basic dedup with default eq
      (seq-uniq '(1 2 3 2 1 4 3 5))
      ;; seq-uniq on symbols
      (seq-uniq '(a b a c b d))
      ;; seq-uniq with custom test: equal for string comparison
      (seq-uniq '("hello" "world" "hello" "foo" "world") #'equal)
      ;; seq-uniq with custom test: case-insensitive strings
      (seq-uniq '("Hello" "hello" "HELLO" "world" "World")
                (lambda (a b) (string= (downcase a) (downcase b))))
      ;; seq-uniq on vector
      (seq-uniq [1 1 2 3 3 4])
      ;; seq-uniq on empty
      (seq-uniq nil)
      ;; seq-remove: complement of seq-filter
      (seq-remove #'cl-evenp '(1 2 3 4 5 6))
      ;; seq-remove on vector
      (seq-remove (lambda (x) (> x 3)) [1 2 3 4 5 6])
      ;; seq-remove: remove nothing
      (seq-remove #'cl-evenp '(1 3 5 7))
      ;; seq-remove: remove everything
      (seq-remove #'numberp '(1 2 3))
      ;; seq-remove on string
      (seq-remove (lambda (c) (= c ?l)) "hello world")))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5) (a b c d) (\"hello\" \"world\" \"foo\") (\"Hello\" \"world\") (1 2 3 4) nil (1 3 5) (1 2 3) (1 3 5 7) nil (104 101 111 32 119 111 114 100))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-sort with various comparators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_sort_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn (require (quote cl-lib)) (list
      ;; seq-sort on list (ascending)
      (seq-sort #'< '(5 3 8 1 4 2 7 6))
      ;; seq-sort on list (descending)
      (seq-sort #'> '(5 3 8 1 4 2 7 6))
      ;; seq-sort on vector
      (seq-sort #'< [50 30 80 10 40])
      ;; seq-sort strings alphabetically
      (seq-sort #'string< '("banana" "apple" "cherry" "date"))
      ;; seq-sort by string length
      (seq-sort (lambda (a b) (< (length a) (length b)))
                '("hi" "hello" "hey" "a" "world"))
      ;; seq-sort empty
      (seq-sort #'< nil)
      ;; seq-sort single element
      (seq-sort #'< '(42))
      ;; seq-sort already sorted
      (seq-sort #'< '(1 2 3 4 5))
      ;; seq-sort reversed
      (seq-sort #'< '(5 4 3 2 1))))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7 8) (8 7 6 5 4 3 2 1) [10 30 40 50 80] (\"apple\" \"banana\" \"cherry\" \"date\") (\"a\" \"hi\" \"hey\" \"hello\" \"world\") nil (42) (1 2 3 4 5) (1 2 3 4 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: data pipeline using seq functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_data_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a data analysis pipeline: filter, transform, aggregate
    let form = r#"((require (quote cl-lib)) let ((records '((alice 28 85000)
                                   (bob 35 92000)
                                   (carol 42 78000)
                                   (dave 23 65000)
                                   (eve 31 110000)
                                   (frank 55 95000)
                                   (grace 29 72000)
                                   (hank 38 88000))))
      (let ((name (lambda (r) (nth 0 r)))
            (age (lambda (r) (nth 1 r)))
            (salary (lambda (r) (nth 2 r))))
        (list
          ;; Pipeline 1: names of people over 30, sorted alphabetically
          (seq-sort #'string<
                    (seq-map (lambda (r) (symbol-name (funcall name r)))
                             (seq-filter (lambda (r) (> (funcall age r) 30))
                                         records)))
          ;; Pipeline 2: average salary of people under 35
          (let* ((young (seq-filter (lambda (r) (< (funcall age r) 35)) records))
                 (salaries (seq-map salary young))
                 (total (seq-reduce #'+ salaries 0))
                 (count (seq-length young)))
            (/ total count))
          ;; Pipeline 3: count of high earners (>= 90000)
          (seq-count (lambda (r) (>= (funcall salary r) 90000)) records)
          ;; Pipeline 4: find first person with salary > 100000
          (let ((found (seq-find (lambda (r) (> (funcall salary r) 100000)) records)))
            (if found (funcall name found) 'none))
          ;; Pipeline 5: are all salaries positive?
          (seq-every-p (lambda (r) (> (funcall salary r) 0)) records)
          ;; Pipeline 6: unique ages mod 10 (decade of life)
          (seq-sort #'<
                    (seq-uniq (seq-map (lambda (r) (* 10 (/ (funcall age r) 10)))
                                       records)))
          ;; Pipeline 7: total salary
          (seq-reduce #'+ (seq-map salary records) 0))))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: set operations using seq functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_set_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement set operations (union, intersection, difference,
    // symmetric difference) using seq functions
    let form = r#"((require (quote cl-lib)) progn
  (fset 'neovm--seq-union
    (lambda (a b)
      (seq-uniq (append a b))))

  (fset 'neovm--seq-intersection
    (lambda (a b)
      (seq-filter (lambda (x) (seq-some (lambda (y) (equal x y)) b)) a)))

  (fset 'neovm--seq-difference
    (lambda (a b)
      (seq-remove (lambda (x) (seq-some (lambda (y) (equal x y)) b)) a)))

  (fset 'neovm--seq-symmetric-difference
    (lambda (a b)
      (append (funcall 'neovm--seq-difference a b)
              (funcall 'neovm--seq-difference b a))))

  (fset 'neovm--seq-subset-p
    (lambda (a b)
      (seq-every-p (lambda (x) (seq-some (lambda (y) (equal x y)) b)) a)))

  (fset 'neovm--seq-powerset
    (lambda (s)
      (if (null s) (list nil)
        (let ((rest-ps (funcall 'neovm--seq-powerset (cdr s))))
          (append rest-ps
                  (seq-map (lambda (subset) (cons (car s) subset))
                           rest-ps))))))

  (unwind-protect
      (let ((a '(1 2 3 4 5))
            (b '(3 4 5 6 7))
            (c '(1 2 3)))
        (list
          ;; Union
          (seq-sort #'< (funcall 'neovm--seq-union a b))
          ;; Intersection
          (seq-sort #'< (funcall 'neovm--seq-intersection a b))
          ;; A - B
          (seq-sort #'< (funcall 'neovm--seq-difference a b))
          ;; B - A
          (seq-sort #'< (funcall 'neovm--seq-difference b a))
          ;; Symmetric difference
          (seq-sort #'< (funcall 'neovm--seq-symmetric-difference a b))
          ;; Subset tests
          (funcall 'neovm--seq-subset-p c a)     ;; {1,2,3} subset of {1..5}
          (funcall 'neovm--seq-subset-p a c)     ;; {1..5} not subset of {1,2,3}
          (funcall 'neovm--seq-subset-p nil a)   ;; empty subset of anything
          ;; Powerset of small set
          (let ((ps (funcall 'neovm--seq-powerset '(1 2 3))))
            (list (length ps)
                  (seq-sort (lambda (a b)
                              (< (seq-reduce #'+ a 0) (seq-reduce #'+ b 0)))
                            ps)))))
    (fmakunbound 'neovm--seq-union)
    (fmakunbound 'neovm--seq-intersection)
    (fmakunbound 'neovm--seq-difference)
    (fmakunbound 'neovm--seq-symmetric-difference)
    (fmakunbound 'neovm--seq-subset-p)
    (fmakunbound 'neovm--seq-powerset)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
