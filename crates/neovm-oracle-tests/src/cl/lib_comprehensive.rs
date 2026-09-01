//! Oracle parity tests for cl-lib sequence/set functions comprehensively.
//!
//! Covers: cl-reduce, cl-every, cl-some, cl-notany, cl-notevery,
//! cl-count, cl-count-if, cl-find, cl-find-if, cl-position, cl-position-if,
//! cl-search, cl-mismatch, cl-subseq, cl-substitute, cl-substitute-if,
//! cl-remove-duplicates, cl-set-difference, cl-intersection, cl-union, cl-adjoin.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// cl-reduce with :initial-value, :from-end, :key, :start/:end
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_lib_comp_reduce_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; Basic sum reduction
    (cl-reduce #'+ '(1 2 3 4 5))
    ;; With :initial-value
    (cl-reduce #'+ '(1 2 3 4 5) :initial-value 100)
    ;; With :from-end t (right fold)
    (cl-reduce #'cons '(a b c d) :from-end t :initial-value nil)
    ;; Left fold with cons (builds reversed)
    (cl-reduce (lambda (acc x) (cons x acc)) '(a b c d) :initial-value nil)
    ;; With :key
    (cl-reduce #'+ '((1 . a) (2 . b) (3 . c)) :key #'car)
    ;; With :start and :end (subsequence reduce)
    (cl-reduce #'* '(1 2 3 4 5 6) :start 1 :end 4)
    ;; Reduce on string (characters)
    (cl-reduce (lambda (a b) (max a b)) "zacbxm")
    ;; Empty list with initial value
    (cl-reduce #'+ '() :initial-value 42)
    ;; Single-element list, no initial value
    (cl-reduce #'+ '(99))))"#;
    let expect = expect_test::expect![r#""OK (15 115 (a b c d) (d c b a) 6 24 122 42 99)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-every, cl-some, cl-notany, cl-notevery
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_lib_comp_quantifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; cl-every: all positive?
    (cl-every #'cl-plusp '(1 2 3 4 5))
    (cl-every #'cl-plusp '(1 2 -3 4 5))
    ;; cl-every with two sequences (stops at shortest)
    (cl-every #'< '(1 2 3) '(4 5 6))
    (cl-every #'< '(1 2 3) '(4 5 2))
    ;; cl-some: any negative?
    (cl-some #'cl-minusp '(1 2 3))
    (cl-some #'cl-minusp '(1 -2 3))
    ;; cl-some returns the first truthy value
    (cl-some (lambda (x) (and (> x 3) (* x 10))) '(1 2 3 4 5))
    ;; cl-notany: none are strings?
    (cl-notany #'stringp '(1 2 3))
    (cl-notany #'stringp '(1 "two" 3))
    ;; cl-notevery: not all are even?
    (cl-notevery #'cl-evenp '(2 4 6 8))
    (cl-notevery #'cl-evenp '(2 4 5 8))
    ;; Edge: empty list
    (cl-every #'cl-plusp '())
    (cl-some #'cl-plusp '())
    (cl-notany #'cl-plusp '())
    (cl-notevery #'cl-plusp '())))"#;
    let expect = expect_test::expect![r#""OK (t nil t nil nil t 40 t nil nil t t nil t nil)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-count and cl-count-if with all keyword args
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_lib_comp_count_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; Basic cl-count
    (cl-count 3 '(1 3 2 3 4 3 5))
    ;; cl-count with :test
    (cl-count "abc" '("abc" "def" "abc" "ghi") :test #'string=)
    ;; cl-count with :key
    (cl-count 2 '((1 . a) (2 . b) (2 . c) (3 . d)) :key #'car)
    ;; cl-count with :start and :end
    (cl-count 3 '(3 1 3 2 3 4 3 5) :start 2 :end 6)
    ;; cl-count-if
    (cl-count-if #'cl-evenp '(1 2 3 4 5 6 7 8))
    ;; cl-count-if with :key
    (cl-count-if #'cl-plusp '((-1 . a) (2 . b) (-3 . c) (4 . d)) :key #'car)
    ;; cl-count-if with :start/:end
    (cl-count-if #'cl-oddp '(1 2 3 4 5 6 7 8 9 10) :start 3 :end 8)
    ;; cl-count on vector
    (cl-count 'x [a x b x c x])
    ;; cl-count on string
    (cl-count ?a "banana")))"#;
    let expect = expect_test::expect![r#""OK (3 2 2 2 4 2 2 3 3)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-find and cl-find-if with :key, :start, :end, :from-end, :test
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_lib_comp_find_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; Basic find
    (cl-find 3 '(1 2 3 4 5))
    ;; Find not present
    (cl-find 99 '(1 2 3 4 5))
    ;; Find with :test
    (cl-find "b" '("a" "b" "c") :test #'string=)
    ;; Find with :key (find pair by car)
    (cl-find 2 '((1 . alpha) (2 . beta) (3 . gamma)) :key #'car)
    ;; Find with :from-end t (last match)
    (cl-find 'x '((x . 1) (y . 2) (x . 3)) :key #'car :from-end t)
    ;; Find with :start/:end
    (cl-find 5 '(5 1 2 5 3 5 4) :start 2 :end 5)
    ;; cl-find-if: first even
    (cl-find-if #'cl-evenp '(1 3 5 4 6))
    ;; cl-find-if with :key
    (cl-find-if (lambda (x) (> x 10))
                '((5 . a) (15 . b) (3 . c) (20 . d))
                :key #'car)
    ;; cl-find-if with :from-end t
    (cl-find-if #'cl-oddp '(2 3 4 5 6) :from-end t)
    ;; cl-find on vector
    (cl-find 'c [a b c d e])))"#;
    let expect =
        expect_test::expect![[r#""OK (3 nil \"b\" (2 . beta) (x . 3) 5 4 (15 . b) 5 c)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-position and cl-position-if
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_lib_comp_position_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; Basic position
    (cl-position 3 '(1 2 3 4 5))
    ;; Not found
    (cl-position 99 '(1 2 3))
    ;; With :test
    (cl-position "hello" '("world" "hello" "foo") :test #'string=)
    ;; With :from-end t (last occurrence index)
    (cl-position 3 '(3 1 3 2 3) :from-end t)
    ;; With :start
    (cl-position 3 '(3 1 3 2 3) :start 1)
    ;; With :key
    (cl-position 'b '((a . 1) (b . 2) (c . 3)) :key #'car)
    ;; cl-position-if
    (cl-position-if #'cl-evenp '(1 3 5 4 6))
    ;; cl-position-if with :key and :start
    (cl-position-if #'cl-minusp '((1 . a) (-2 . b) (3 . c) (-4 . d))
                    :key #'car :start 2)
    ;; Position in vector
    (cl-position 'c [a b c d e])
    ;; Position in string
    (cl-position ?n "banana")))"#;
    let expect = expect_test::expect![r#""OK (2 nil 1 4 2 1 3 3 2 2)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-search and cl-mismatch
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_lib_comp_search_mismatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; cl-search: find subsequence
    (cl-search '(3 4) '(1 2 3 4 5))
    ;; Not found
    (cl-search '(3 5) '(1 2 3 4 5))
    ;; cl-search with :from-end
    (cl-search '(2 3) '(1 2 3 4 2 3 5) :from-end t)
    ;; cl-search with :test
    (cl-search '("b" "c") '("a" "b" "c" "d") :test #'string=)
    ;; cl-mismatch: first differing index
    (cl-mismatch '(1 2 3 4 5) '(1 2 9 4 5))
    ;; No mismatch
    (cl-mismatch '(1 2 3) '(1 2 3))
    ;; Mismatch with different lengths (shorter ends first)
    (cl-mismatch '(1 2) '(1 2 3))
    ;; Mismatch with :from-end
    (cl-mismatch '(1 2 3 4) '(1 9 3 4) :from-end t)
    ;; Mismatch with :start1/:end1/:start2/:end2
    (cl-mismatch '(a b c d e) '(x b c y z) :start1 1 :end1 3 :start2 1 :end2 3)
    ;; cl-search with :key
    (cl-search '(2 3) '((1 . a) (2 . b) (3 . c) (4 . d)) :key #'car)))"#;
    let expect = expect_test::expect![r#""ERR (wrong-type-argument listp 2)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-subseq
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_lib_comp_subseq_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; Basic subseq on list
    (cl-subseq '(a b c d e) 1 3)
    ;; From start
    (cl-subseq '(a b c d e) 0 2)
    ;; To end (no end arg)
    (cl-subseq '(a b c d e) 2)
    ;; Full copy
    (cl-subseq '(1 2 3) 0)
    ;; Subseq on vector
    (cl-subseq [10 20 30 40 50] 1 4)
    ;; Subseq on string
    (cl-subseq "hello world" 6)
    (cl-subseq "hello world" 0 5)
    ;; Empty subseq
    (cl-subseq '(a b c) 2 2)
    ;; Single element
    (cl-subseq '(a b c d) 1 2)
    ;; setf cl-subseq to modify a sequence in-place
    (let ((v (vector 1 2 3 4 5)))
      (setf (cl-subseq v 1 3) '(20 30))
      (append v nil))))"#;
    let expect = expect_test::expect![[
        r#""OK ((b c) (a b) (c d e) (1 2 3) [20 30 40] \"world\" \"hello\" nil (b) (1 20 30 4 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-substitute and cl-substitute-if
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_lib_comp_substitute_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; Basic substitute
    (cl-substitute 'x 'b '(a b c b d))
    ;; Substitute with :count
    (cl-substitute 'x 'b '(a b c b d b) :count 2)
    ;; Substitute with :from-end and :count
    (cl-substitute 'x 'b '(a b c b d b) :from-end t :count 1)
    ;; Substitute with :start/:end
    (cl-substitute 99 3 '(3 1 3 2 3 4 3) :start 2 :end 5)
    ;; Substitute with :key
    (cl-substitute 'new 2 '((1 . a) (2 . b) (3 . c) (2 . d)) :key #'car)
    ;; Substitute with :test
    (cl-substitute 'FOUND "target" '("a" "target" "b" "target") :test #'string=)
    ;; cl-substitute-if
    (cl-substitute-if 0 #'cl-minusp '(1 -2 3 -4 5))
    ;; cl-substitute-if with :key
    (cl-substitute-if '(0 . zero) #'cl-minusp
                      '((1 . a) (-2 . b) (3 . c) (-4 . d))
                      :key #'car)
    ;; Substitute on vector
    (cl-substitute 'X 'b [a b c b d])
    ;; Original is unchanged (non-destructive)
    (let ((orig '(1 2 3 2 1)))
      (cl-substitute 99 2 orig)
      orig)))"#;
    let expect = expect_test::expect![[
        r#""OK ((a x c x d) (a x c x d b) (a b c b d x) (3 1 99 2 99 4 3) ((1 . a) new (3 . c) new) (\"a\" FOUND \"b\" FOUND) (1 0 3 0 5) ((1 . a) (0 . zero) (3 . c) (0 . zero)) [a X c X d] (1 2 3 2 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-remove-duplicates with :test, :key, :from-end
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_lib_comp_remove_duplicates_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; Basic remove-duplicates
    (cl-remove-duplicates '(1 2 3 2 1 4 3 5))
    ;; With :from-end nil (keep last occurrence)
    (cl-remove-duplicates '(a b a c b d) :from-end nil)
    ;; With :from-end t (keep first occurrence)
    (cl-remove-duplicates '(a b a c b d) :from-end t)
    ;; With :test
    (cl-remove-duplicates '("a" "A" "b" "B" "a") :test #'string=)
    ;; With :key
    (cl-remove-duplicates '((1 . a) (2 . b) (1 . c) (3 . d)) :key #'car)
    ;; With :start/:end
    (cl-remove-duplicates '(1 2 3 2 3 4 5) :start 1 :end 5)
    ;; On vector
    (cl-remove-duplicates [1 2 3 2 1])
    ;; On string
    (cl-remove-duplicates "abracadabra")
    ;; Empty input
    (cl-remove-duplicates '())
    ;; All same
    (cl-remove-duplicates '(x x x x))))"#;
    let expect = expect_test::expect![[
        r#""OK ((2 1 4 3 5) (a c b d) (a b c d) (\"A\" \"b\" \"B\" \"a\") ((2 . b) (1 . c) (3 . d)) (1 2 3 4 5) [3 2 1] \"cdbra\" nil (x))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-set-difference, cl-intersection, cl-union
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_lib_comp_set_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Set operations may return elements in any order, so we sort results
    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; cl-set-difference
    (sort (cl-set-difference '(1 2 3 4 5) '(2 4 6)) #'<)
    ;; cl-set-difference with :test
    (sort (cl-set-difference '("a" "b" "c" "d") '("b" "d")
                             :test #'string=)
          #'string<)
    ;; cl-set-difference with :key
    (sort (mapcar #'car
            (cl-set-difference '((1 . a) (2 . b) (3 . c))
                               '((2 . x) (3 . y))
                               :key #'car))
          #'<)
    ;; cl-intersection
    (sort (cl-intersection '(1 2 3 4 5) '(2 4 6 8)) #'<)
    ;; cl-intersection with :test
    (sort (cl-intersection '("apple" "banana" "cherry")
                           '("banana" "date" "cherry")
                           :test #'string=)
          #'string<)
    ;; cl-union
    (sort (cl-union '(1 2 3) '(3 4 5)) #'<)
    ;; cl-union with :test
    (sort (cl-union '("a" "b") '("b" "c") :test #'string=)
          #'string<)
    ;; Edge cases
    (sort (cl-set-difference '(1 2 3) '()) #'<)
    (cl-intersection '() '(1 2 3))
    (sort (cl-union '() '(1 2 3)) #'<)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 3 5) (\"a\" \"c\") (1) (2 4) (\"banana\" \"cherry\") (1 2 3 4 5) (\"a\" \"b\" \"c\") (1 2 3) nil (1 2 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-adjoin
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_lib_comp_adjoin_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; Add non-member (new element prepended)
    (cl-adjoin 4 '(1 2 3))
    ;; Add existing member (no change)
    (cl-adjoin 2 '(1 2 3))
    ;; With :test
    (cl-adjoin "b" '("a" "b" "c") :test #'string=)
    (cl-adjoin "d" '("a" "b" "c") :test #'string=)
    ;; With :key
    (cl-adjoin 2 '((1 . a) (2 . b) (3 . c)) :key #'car)
    (cl-adjoin 4 '((1 . a) (2 . b) (3 . c)) :key #'car)
    ;; Empty list
    (cl-adjoin 'x '())
    ;; Multiple adjoins building a set
    (let ((s '()))
      (setq s (cl-adjoin 'a s))
      (setq s (cl-adjoin 'b s))
      (setq s (cl-adjoin 'a s))
      (setq s (cl-adjoin 'c s))
      (setq s (cl-adjoin 'b s))
      (length s))))"#;
    let expect = expect_test::expect![r#""ERR (wrong-type-argument listp 2)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-reduce with complex accumulation: building a frequency table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_lib_comp_reduce_frequency_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  ;; Use cl-reduce to build a frequency alist from a list of symbols
  (let* ((data '(a b a c b a d c a b))
         (freq (cl-reduce
                 (lambda (acc x)
                   (let ((entry (assq x acc)))
                     (if entry
                         (progn (setcdr entry (1+ (cdr entry))) acc)
                       (cons (cons x 1) acc))))
                 data
                 :initial-value nil)))
    ;; Sort by key for deterministic output
    (sort freq (lambda (a b) (string< (symbol-name (car a))
                                       (symbol-name (car b)))))))"#;
    let expect = expect_test::expect![r#""OK ((a . 4) (b . 3) (c . 2) (d . 1))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}
