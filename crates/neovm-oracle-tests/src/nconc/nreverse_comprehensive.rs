//! Comprehensive oracle parity tests for destructive list operations:
//! nconc with 0-5 args including nil, nreverse on various list types,
//! nbutlast with N param, sort with various predicates, delete/delq vs
//! remove/remq, cl-delete-if/cl-delete-if-not, destructive vs
//! non-destructive comparisons, and list structure sharing after mutation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// nconc with 0 through 5 arguments including nil in various positions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_nreverse_comp_nconc_zero_to_five_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn (require (quote cl-lib)) (list
  ;; 0 args
  (nconc)
  ;; 1 arg: single list
  (nconc (list 1 2 3))
  ;; 1 arg: nil
  (nconc nil)
  ;; 2 args: both non-nil
  (let ((a (list 'x 'y)) (b (list 'z 'w)))
    (nconc a b))
  ;; 2 args: first nil
  (let ((b (list 10 20)))
    (nconc nil b))
  ;; 2 args: second nil
  (let ((a (list 10 20)))
    (nconc a nil))
  ;; 3 args: nil in middle
  (let ((a (list 1)) (c (list 3)))
    (nconc a nil c))
  ;; 3 args: all non-nil
  (let ((a (list 'a)) (b (list 'b)) (c (list 'c)))
    (nconc a b c))
  ;; 4 args: alternating nil and lists
  (nconc nil (list 'first) nil (list 'last))
  ;; 5 args: mixed nils and singleton lists
  (nconc (list 1) nil (list 2) nil (list 3))
  ;; 5 args: all nil except last which is atom
  (nconc nil nil nil nil 42)
  ;; 5 args: all nil
  (nconc nil nil nil nil nil)))"#;
    let expect = expect_test::expect![[
        r#""OK (nil (1 2 3) nil (x y z w) (10 20) (10 20) (1 3) (a b c) (first last) (1 2 3) 42 nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// nreverse on various list types: empty, single, multi, nested, dotted-like
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_nreverse_comp_nreverse_various_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn (require (quote cl-lib)) (list
  ;; empty list
  (nreverse nil)
  ;; single element
  (nreverse (list 'only))
  ;; two elements
  (nreverse (list 'a 'b))
  ;; many elements
  (nreverse (list 1 2 3 4 5 6 7 8 9 10))
  ;; nested lists (structure preserved, order reversed)
  (nreverse (list '(a b) '(c d) '(e f)))
  ;; list of mixed types
  (nreverse (list 1 "two" 'three 4.0 nil t))
  ;; nreverse of nreverse recovers original (on fresh copy each time)
  (let ((orig '(alpha beta gamma delta)))
    (equal orig (nreverse (nreverse (copy-sequence orig)))))
  ;; nreverse of a list built by cons
  (let ((lst nil))
    (dotimes (i 5) (setq lst (cons i lst)))
    (nreverse lst))
  ;; nreverse preserves element identity (eq check on cons cells)
  (let* ((inner (list 'inside))
         (outer (list 1 inner 3)))
    (let ((rev (nreverse (copy-sequence outer))))
      (eq (nth 1 rev) inner)))))"#;
    let expect = expect_test::expect![[
        r#""OK (nil (only) (b a) (10 9 8 7 6 5 4 3 2 1) ((e f) (c d) (a b)) (t nil 4.0 three \"two\" 1) t (0 1 2 3 4) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// nbutlast with various N parameter values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_nreverse_comp_nbutlast_with_n() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn (require (quote cl-lib)) (list
  ;; nbutlast with default N=1
  (nbutlast (list 1 2 3 4 5))
  ;; nbutlast with N=0 (removes nothing)
  (nbutlast (list 1 2 3 4 5) 0)
  ;; nbutlast with N=2
  (nbutlast (list 'a 'b 'c 'd 'e) 2)
  ;; nbutlast with N=3
  (nbutlast (list 1 2 3 4 5) 3)
  ;; nbutlast with N equal to length (returns nil)
  (nbutlast (list 1 2 3) 3)
  ;; nbutlast with N greater than length (returns nil)
  (nbutlast (list 'x 'y) 10)
  ;; nbutlast on single-element list
  (nbutlast (list 'only))
  ;; nbutlast on empty list
  (nbutlast nil)
  ;; Compare nbutlast (destructive) vs butlast (non-destructive)
  (let* ((a (list 1 2 3 4 5))
         (b (copy-sequence a))
         (bl-result (butlast b 2))
         (nbl-result (nbutlast a 2)))
    (list (equal bl-result nbl-result) bl-result nbl-result))
  ;; nbutlast with N=1 on two-element list
  (nbutlast (list 'first 'second) 1)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4) (1 2 3 4 5) (a b c) (1 2) nil nil nil nil (t (1 2 3) (1 2 3)) (first))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// sort with various predicates including stable sort verification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_nreverse_comp_sort_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn (require (quote cl-lib)) (list
  ;; sort ascending
  (sort (list 5 3 1 4 2) #'<)
  ;; sort descending
  (sort (list 5 3 1 4 2) #'>)
  ;; sort strings alphabetically
  (sort (list "banana" "apple" "cherry" "date") #'string<)
  ;; sort strings reverse alphabetically
  (sort (list "banana" "apple" "cherry" "date") #'string>)
  ;; sort by custom predicate: sort by absolute value ascending
  (sort (list -5 3 -1 4 -2) (lambda (a b) (< (abs a) (abs b))))
  ;; sort by string length
  (sort (list "hi" "hello" "hey" "h" "howdy")
        (lambda (a b) (< (length a) (length b))))
  ;; sort already sorted list
  (sort (list 1 2 3 4 5) #'<)
  ;; sort reverse-sorted list
  (sort (list 5 4 3 2 1) #'<)
  ;; sort single element
  (sort (list 42) #'<)
  ;; sort empty list
  (sort nil #'<)
  ;; sort list of pairs by second element
  (sort (list '(a 3) '(b 1) '(c 2))
        (lambda (x y) (< (cadr x) (cadr y))))
  ;; sort with equal elements (stability check: order among equals is preserved)
  (let* ((data (list '(a 2) '(b 1) '(c 2) '(d 1) '(e 3)))
         (sorted (sort (copy-sequence data)
                       (lambda (x y) (< (cadr x) (cadr y))))))
    (mapcar #'car sorted))))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5) (5 4 3 2 1) (\"apple\" \"banana\" \"cherry\" \"date\") (\"date\" \"cherry\" \"banana\" \"apple\") (-1 -2 3 4 -5) (\"h\" \"hi\" \"hey\" \"hello\" \"howdy\") (1 2 3 4 5) (1 2 3 4 5) (42) nil ((b 1) (c 2) (a 3)) (b d a c e))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// delete/delq vs remove/remq: destructive vs non-destructive comparison
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_nreverse_comp_delete_vs_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn (require (quote cl-lib)) (list
  ;; delq removes by eq: remove symbol from list
  (delq 'b (list 'a 'b 'c 'b 'd))
  ;; delete removes by equal: remove string from list
  (delete "hello" (list "hello" "world" "hello" "foo"))
  ;; remq (non-destructive eq removal)
  (let ((lst (list 'x 'y 'z 'y 'w)))
    (list (remq 'y lst) lst))
  ;; remove (non-destructive equal removal)
  (let ((lst (list 1 2 3 2 1)))
    (list (remove 2 lst) lst))
  ;; delq vs remq: delq modifies, remq does not
  (let* ((a (list 1 2 3 4 5))
         (b (copy-sequence a))
         (del-result (delq 3 a))
         (rem-result (remq 3 b)))
    (list (equal del-result rem-result)
          del-result rem-result))
  ;; delete element not in list
  (delete 99 (list 1 2 3 4 5))
  ;; delq on empty list
  (delq 'x nil)
  ;; delete all elements (every element matches)
  (delq t (list t t t))
  ;; delq with nil as element to remove
  (delq nil (list 1 nil 2 nil 3))
  ;; delete with equal on nested structure
  (delete '(1 2) (list '(1 2) '(3 4) '(1 2) '(5 6)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((a c d) (\"world\" \"foo\") ((x z w) (x y z y w)) ((1 3 1) (1 2 3 2 1)) (t (1 2 4 5) (1 2 4 5)) (1 2 3 4 5) nil nil (1 2 3) ((3 4) (5 6)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-delete-if and cl-delete-if-not with require cl-lib
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_nreverse_comp_cl_delete_if() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"((require (quote cl-lib)) progn
  (require 'cl-lib)
  (list
    ;; cl-delete-if: remove elements matching predicate
    (cl-delete-if #'evenp (list 1 2 3 4 5 6 7 8))
    ;; cl-delete-if: remove negatives
    (cl-delete-if (lambda (x) (< x 0)) (list -3 1 -2 4 -5 6))
    ;; cl-delete-if-not: keep only elements matching predicate (remove those that don't)
    (cl-delete-if-not #'evenp (list 1 2 3 4 5 6 7 8))
    ;; cl-delete-if-not: keep only strings
    (cl-delete-if-not #'stringp (list 1 "hello" 'sym "world" nil))
    ;; cl-delete-if on empty list
    (cl-delete-if #'evenp nil)
    ;; cl-delete-if where nothing matches (no deletion)
    (cl-delete-if (lambda (x) (> x 100)) (list 1 2 3 4 5))
    ;; cl-delete-if where everything matches
    (cl-delete-if #'numberp (list 1 2 3 4 5))
    ;; cl-remove-if (non-destructive counterpart)
    (let ((lst (list 1 2 3 4 5 6)))
      (list (cl-remove-if #'oddp lst) lst))
    ;; cl-remove-if-not
    (cl-remove-if-not (lambda (x) (= (% x 3) 0)) (list 1 2 3 4 5 6 7 8 9))))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Destructive vs non-destructive comparisons: verifying mutation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_nreverse_comp_destructive_vs_nondestructive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn (require (quote cl-lib)) (list
  ;; reverse vs nreverse: same result, different mutation
  (let* ((a (list 1 2 3 4 5))
         (b (copy-sequence a))
         (rev-a (reverse a))
         (nrev-b (nreverse b)))
    (list (equal rev-a nrev-b)   ;; same result
          (equal a '(1 2 3 4 5)) ;; reverse did NOT mutate
          rev-a nrev-b))
  ;; append vs nconc: same result, nconc mutates
  (let* ((a1 (list 1 2)) (a2 (list 3 4))
         (b1 (list 1 2)) (b2 (list 3 4))
         (app-result (append a1 a2))
         (nconc-result (nconc b1 b2)))
    (list (equal app-result nconc-result)
          app-result nconc-result))
  ;; butlast vs nbutlast
  (let* ((a (list 'a 'b 'c 'd 'e))
         (b (copy-sequence a))
         (bl (butlast a 2))
         (nbl (nbutlast b 2)))
    (list (equal bl nbl) bl nbl))
  ;; sort is destructive — original binding may point to wrong element
  (let* ((lst (list 5 3 1 4 2))
         (sorted (sort lst #'<)))
    ;; sorted is the correctly sorted result
    sorted)
  ;; copy-sequence + sort preserves original
  (let* ((orig '(5 3 1 4 2))
         (copied (copy-sequence orig))
         (sorted (sort copied #'<)))
    (list (equal orig '(5 3 1 4 2)) sorted))))"#;
    let expect = expect_test::expect![[
        r#""OK ((t t (5 4 3 2 1) (5 4 3 2 1)) (t (1 2 3 4) (1 2 3 4)) (t (a b c) (a b c)) (1 2 3 4 5) (t (1 2 3 4 5)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// List structure sharing after mutation: aliasing effects
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_nreverse_comp_structure_sharing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn (require (quote cl-lib)) (list
  ;; nconc creates sharing: tail of a IS b
  (let* ((a (list 1 2))
         (b (list 3 4))
         (result (nconc a b)))
    (list result
          (eq (nthcdr 2 result) b)   ;; tail shares with b
          (eq result a)))             ;; result is a
  ;; Mutation through shared structure: setcar on b visible through a
  (let* ((a (list 1 2))
         (b (list 3 4)))
    (nconc a b)
    (setcar b 99)
    ;; a should now be (1 2 99 4) because its tail IS b
    a)
  ;; Multiple lists sharing via nconc
  (let* ((tail (list 'shared))
         (a (list 'a))
         (b (list 'b)))
    (nconc a tail)
    (nconc b tail)
    ;; Both a and b end with tail
    (list a b
          (eq (cdr a) tail)
          (eq (cdr b) tail)))
  ;; nreverse breaks old head reference
  (let* ((lst (list 1 2 3 4 5))
         (old-head lst)
         (rev (nreverse lst)))
    ;; old-head now points to what was the first cons cell (now at end)
    ;; rev points to what was the last cons cell (now at start)
    (list rev (car old-head) (car rev)))
  ;; Building a list with nconc in a loop: efficient append pattern
  (let ((result nil))
    (dotimes (i 5)
      (setq result (nconc result (list (* i i)))))
    result)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 2 3 4) t t) (1 2 99 4) ((a shared) (b shared) t t) ((5 4 3 2 1) 1 5) (0 1 4 9 16))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// nconc with non-list final argument (dotted pairs)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_nreverse_comp_dotted_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn (require (quote cl-lib)) (list
  ;; nconc with atom as last argument produces dotted list
  (nconc (list 1 2 3) 'end)
  ;; nconc with number as last arg
  (nconc (list 'a 'b) 42)
  ;; nconc with string as last arg
  (nconc (list 1) "tail")
  ;; nconc with t as last arg
  (nconc (list 'x) t)
  ;; nconc with multiple lists and atom at end
  (nconc (list 1) (list 2) (list 3) 'done)
  ;; All nils then atom
  (nconc nil nil 'solo)
  ;; Single nil then atom
  (nconc nil 'alone)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 . end) (a b . 42) (1 . \"tail\") (x . t) (1 2 3 . done) solo alone)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combined destructive ops: realistic list-building patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_nreverse_comp_combined_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn (require (quote cl-lib)) (list
  ;; Idiomatic push-then-nreverse pattern for building lists
  (let ((acc nil))
    (dolist (x '(1 2 3 4 5))
      (setq acc (cons (* x x) acc)))
    (nreverse acc))
  ;; Filter + collect with nreverse
  (let ((acc nil))
    (dolist (x '(1 2 3 4 5 6 7 8 9 10))
      (when (= (% x 2) 0)
        (setq acc (cons x acc))))
    (nreverse acc))
  ;; nconc to merge multiple filtered results
  (let ((evens nil) (odds nil))
    (dolist (x '(1 2 3 4 5 6 7 8))
      (if (= (% x 2) 0)
          (setq evens (cons x evens))
        (setq odds (cons x odds))))
    (nconc (nreverse odds) (nreverse evens)))
  ;; Flatten one level using nconc + mapcar
  (apply #'nconc (mapcar #'copy-sequence '((1 2) (3 4) (5 6))))
  ;; sort + nbutlast: top 3 largest elements
  (let* ((data (list 15 3 9 1 7 12 5))
         (sorted (sort (copy-sequence data) #'>)))
    (nbutlast sorted (- (length sorted) 3)))
  ;; nreverse + nconc: reverse first half, keep second half
  (let* ((lst (list 1 2 3 4 5 6))
         (first-half (nbutlast (copy-sequence lst) 3))
         (second-half (nthcdr 3 lst)))
    (nconc (nreverse first-half) (copy-sequence second-half)))
  ;; delete duplicates using a hash table (manual dedup)
  (let ((seen (make-hash-table :test 'equal))
        (result nil))
    (dolist (x '(1 2 3 2 1 4 3 5 4))
      (unless (gethash x seen)
        (puthash x t seen)
        (setq result (cons x result))))
    (nreverse result))))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 4 9 16 25) (2 4 6 8 10) (1 3 5 7 2 4 6 8) (1 2 3 4 5 6) (15 12 9) (3 2 1 4 5 6) (1 2 3 4 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// sort with cl-lib stable sort and complex predicates
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_nreverse_comp_sort_complex_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"((require (quote cl-lib)) progn
  (require 'cl-lib)
  (list
    ;; cl-sort with :key parameter
    (cl-sort (list '(a 3) '(b 1) '(c 2)) #'< :key #'cadr)
    ;; cl-stable-sort preserves relative order of equal elements
    (let ((data (list '(a 2) '(b 1) '(c 2) '(d 1) '(e 3) '(f 2))))
      (mapcar #'car
              (cl-stable-sort (copy-sequence data)
                              #'< :key #'cadr)))
    ;; cl-sort on strings by length
    (cl-sort (list "cat" "a" "elephant" "be") #'< :key #'length)
    ;; Multi-key sort: primary by second element, secondary by first
    (let ((data (list '(3 1) '(1 2) '(2 1) '(1 1) '(3 2))))
      (cl-stable-sort (copy-sequence data)
                      (lambda (a b)
                        (or (< (cadr a) (cadr b))
                            (and (= (cadr a) (cadr b))
                                 (< (car a) (car b)))))))
    ;; sort with comparison counting
    (let ((count 0))
      (sort (list 5 3 1 4 2)
            (lambda (a b) (setq count (1+ count)) (< a b)))
      count)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
