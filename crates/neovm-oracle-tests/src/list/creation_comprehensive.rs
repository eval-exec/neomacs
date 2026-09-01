//! Oracle parity tests for comprehensive list creation patterns.
//!
//! Tests `list`, `cons`, `make-list`, `number-sequence`, `nthcdr`/`nth`,
//! `last`, `butlast`/`nbutlast`, `take`/`ntake`, `proper-list-p`, `list*`
//! (via apply), `append`, circular list detection via `safe-length`,
//! dotted pair operations, `cl-list*`, and `cl-pairlis`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Test 1: list, cons, and list* via apply — various arities and nesting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_list_creation_list_cons_and_list_star() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Empty list
  (list)
  ;; Single element
  (list 42)
  ;; Multiple elements of mixed types
  (list 1 "hello" nil t 3.14 ?a)
  ;; Nested lists
  (list (list 1 2) (list 3 (list 4 5)) nil)
  ;; cons building a dotted pair
  (cons 'a 'b)
  ;; cons building a proper list element
  (cons 'head '(tail1 tail2))
  ;; Nested cons
  (cons (cons 1 2) (cons 3 4))
  ;; list* via apply: (apply #'list* '(1 2 3 (4 5))) => (1 2 3 4 5)
  (apply #'list 1 2 '(3 4 5))
  ;; Deep nesting
  (list (list (list (list 'deep)))))"#;
    let expect = expect_test::expect![[
        r#""OK (nil (42) (1 \"hello\" nil t 3.14 97) ((1 2) (3 (4 5)) nil) (a . b) (head tail1 tail2) ((1 . 2) 3 . 4) (1 2 3 4 5) ((((deep)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 2: make-list with various sizes and init values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_list_creation_make_list_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Zero-length list
  (make-list 0 'x)
  ;; Single element
  (make-list 1 'solo)
  ;; Multiple identical elements
  (make-list 5 0)
  ;; With nil init
  (make-list 3 nil)
  ;; With string init
  (make-list 4 "abc")
  ;; With cons cell init
  (make-list 3 '(a . b))
  ;; Length verification
  (length (make-list 7 t))
  ;; Equality: all elements are eq
  (let ((lst (make-list 4 'same)))
    (and (eq (nth 0 lst) (nth 1 lst))
         (eq (nth 2 lst) (nth 3 lst)))))"#;
    let expect = expect_test::expect![[
        r#""OK (nil (solo) (0 0 0 0 0) (nil nil nil) (\"abc\" \"abc\" \"abc\" \"abc\") ((a . b) (a . b) (a . b)) 7 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 3: number-sequence with all parameter combinations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_list_creation_number_sequence_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Basic ascending
  (number-sequence 1 5)
  ;; Ascending with step
  (number-sequence 0 20 5)
  ;; Descending with negative step
  (number-sequence 10 1 -1)
  ;; Descending with step -3
  (number-sequence 15 0 -3)
  ;; Single element (from = to)
  (number-sequence 7 7)
  ;; Step 2 skipping odds
  (number-sequence 2 10 2)
  ;; Large step that overshoots: only start element
  (number-sequence 1 3 10)
  ;; Negative range
  (number-sequence -5 -1)
  ;; Zero crossing
  (number-sequence -3 3)
  ;; Float sequence
  (number-sequence 0 10 3))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5) (0 5 10 15 20) (10 9 8 7 6 5 4 3 2 1) (15 12 9 6 3 0) (7) (2 4 6 8 10) (1) (-5 -4 -3 -2 -1) (-3 -2 -1 0 1 2 3) (0 3 6 9))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 4: nthcdr and nth — boundary cases and deep access
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_list_creation_nthcdr_nth_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((lst '(a b c d e f g h)))
  (list
    ;; nth at each position
    (nth 0 lst)
    (nth 3 lst)
    (nth 7 lst)
    ;; nth beyond end
    (nth 10 lst)
    (nth 100 lst)
    ;; nthcdr at various positions
    (nthcdr 0 lst)
    (nthcdr 1 lst)
    (nthcdr 4 lst)
    (nthcdr 8 lst)
    ;; nthcdr beyond end
    (nthcdr 20 lst)
    ;; nthcdr 0 returns the original
    (eq (nthcdr 0 lst) lst)
    ;; nth on nil
    (nth 0 nil)
    ;; nthcdr on nil
    (nthcdr 5 nil)))"#;
    let expect = expect_test::expect![[
        r#""OK (a d h nil nil (a b c d e f g h) (b c d e f g h) (e f g h) nil nil t nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 5: last, butlast, nbutlast with optional N parameter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_list_creation_last_butlast_nbutlast() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((lst '(1 2 3 4 5 6 7)))
  (list
    ;; last: returns last cons cell
    (last lst)
    ;; last with n: returns last n cons cells
    (last lst 1)
    (last lst 3)
    (last lst 7)
    ;; last with n > length
    (last lst 20)
    ;; last with n=0
    (last lst 0)
    ;; butlast: all but last element (copy)
    (butlast lst)
    ;; butlast with n
    (butlast lst 2)
    (butlast lst 5)
    ;; butlast with n >= length returns nil
    (butlast lst 7)
    (butlast lst 100)
    ;; nbutlast on a fresh copy
    (let ((copy (copy-sequence lst)))
      (nbutlast copy 2))
    ;; Single element
    (last '(solo))
    (butlast '(solo))
    ;; Empty list
    (last nil)
    (butlast nil)))"#;
    let expect = expect_test::expect![[
        r#""OK ((7) (7) (5 6 7) (1 2 3 4 5 6 7) (1 2 3 4 5 6 7) nil (1 2 3 4 5 6) (1 2 3 4 5) (1 2) nil nil (1 2 3 4 5) (solo) nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 6: take and ntake
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_list_creation_take_ntake() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((lst '(a b c d e f)))
  (list
    ;; take: first n elements (non-destructive)
    (take 0 lst)
    (take 1 lst)
    (take 3 lst)
    (take 6 lst)
    ;; take more than length
    (take 10 lst)
    ;; take from nil
    (take 5 nil)
    ;; ntake on copies (destructive)
    (let ((c1 (copy-sequence lst)))
      (ntake 3 c1))
    (let ((c2 (copy-sequence lst)))
      (ntake 0 c2))
    (let ((c3 (copy-sequence lst)))
      (ntake 6 c3))
    ;; take preserves original
    (progn (take 2 lst) lst)))"#;
    let expect = expect_test::expect![[
        r#""OK (nil (a) (a b c) (a b c d e f) (a b c d e f) nil (a b c) nil (a b c d e f) (a b c d e f))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 7: proper-list-p, safe-length, and circular list detection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_list_creation_proper_list_and_circular() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; proper-list-p on proper lists
  (proper-list-p nil)
  (proper-list-p '(1 2 3))
  (proper-list-p '(a))
  ;; proper-list-p on dotted pairs (not proper)
  (proper-list-p '(a . b))
  (proper-list-p '(1 2 . 3))
  ;; proper-list-p on non-lists
  (proper-list-p 42)
  (proper-list-p "hello")
  (proper-list-p 'sym)
  ;; safe-length on proper lists
  (safe-length nil)
  (safe-length '(1 2 3 4 5))
  ;; safe-length on dotted list
  (safe-length '(a b . c))
  ;; safe-length on circular list
  (let ((c (list 1 2 3)))
    (setcdr (last c) c)
    (safe-length c)))"#;
    let expect = expect_test::expect![[r#""OK (0 3 1 nil nil nil nil nil 0 5 2 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 8: dotted pair operations and conversions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_list_creation_dotted_pair_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Basic dotted pairs
  (car '(a . b))
  (cdr '(a . b))
  ;; Nested dotted pairs
  (car '((1 . 2) . (3 . 4)))
  (cdr '((1 . 2) . (3 . 4)))
  (caar '((1 . 2) . (3 . 4)))
  (cdar '((1 . 2) . (3 . 4)))
  (cadr '((1 . 2) . (3 . 4)))
  (cddr '((1 . 2) . (3 . 4)))
  ;; Improper list with proper prefix
  (car '(x y z . w))
  (cdr '(x y z . w))
  (cddr '(x y z . w))
  (cdddr '(x y z . w))
  ;; Association list with dotted pairs
  (assoc 'b '((a . 1) (b . 2) (c . 3)))
  (rassoc 2 '((a . 1) (b . 2) (c . 3)))
  ;; Converting between dotted and proper
  (append '(1 2) 3)
  (append '(1 2) '(3)))"#;
    let expect = expect_test::expect![[
        r#""OK (a b (1 . 2) (3 . 4) 1 2 3 4 x (y z . w) (z . w) w (b . 2) (b . 2) (1 2 . 3) (1 2 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 9: append with many arguments and edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_list_creation_append_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; No arguments
  (append)
  ;; Single list
  (append '(1 2 3))
  ;; Two lists
  (append '(a b) '(c d))
  ;; Three lists
  (append '(1) '(2) '(3))
  ;; Many lists
  (append '(a) '(b) '(c) '(d) '(e) '(f))
  ;; Empty lists interspersed
  (append nil '(1) nil '(2) nil nil '(3) nil)
  ;; All empty
  (append nil nil nil)
  ;; Last arg is non-list (creates dotted)
  (append '(a b) 'c)
  ;; Last arg is atom
  (append nil 42)
  ;; Nested list in args
  (append '((1 2) (3 4)) '((5 6)))
  ;; append doesn't modify original
  (let ((x '(1 2 3)))
    (append x '(4 5))
    x))"#;
    let expect = expect_test::expect![[
        r#""OK (nil (1 2 3) (a b c d) (1 2 3) (a b c d e f) (1 2 3) nil (a b . c) 42 ((1 2) (3 4) (5 6)) (1 2 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 10: cl-list* and cl-pairlis (require cl-lib)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_list_creation_cl_list_star_pairlis() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; cl-list* with single arg (returns it)
    (cl-list* 42)
    ;; cl-list* builds dotted at end
    (cl-list* 1 2 3)
    ;; cl-list* with list as last arg builds proper list
    (cl-list* 'a 'b '(c d))
    ;; cl-list* single + list
    (cl-list* 'x '(y z))
    ;; cl-pairlis: pair up keys and values
    (cl-pairlis '(a b c) '(1 2 3))
    ;; cl-pairlis with optional alist base
    (cl-pairlis '(x y) '(10 20) '((z . 30)))
    ;; cl-pairlis empty
    (cl-pairlis nil nil)
    ;; Interaction: build list with cl-list*, measure with safe-length
    (safe-length (cl-list* 1 2 3 '(4 5 6)))
    ;; cl-pairlis preserves key order
    (mapcar #'car (cl-pairlis '(first second third) '(1 2 3)))))"#;
    let expect = expect_test::expect![[
        r#""OK (42 (1 2 . 3) (a b c d) (x y z) ((a . 1) (b . 2) (c . 3)) ((x . 10) (y . 20) (z . 30)) nil 6 (first second third))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 11: Building lists in loops and combining operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_list_creation_loop_building() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Build list with push + nreverse idiom
  (let ((result nil))
    (dotimes (i 8)
      (push (* i i) result))
    (nreverse result))
  ;; Build list with cons + nreverse
  (let ((acc nil) (i 1))
    (while (<= i 5)
      (setq acc (cons (list i (* i 10)) acc))
      (setq i (1+ i)))
    (nreverse acc))
  ;; number-sequence + mapcar
  (mapcar (lambda (n) (* n n n)) (number-sequence 1 6))
  ;; Flatten one level via apply + append
  (apply #'append '((1 2) (3 4) (5 6)))
  ;; Remove duplicates from a constructed list
  (delete-dups (append '(1 2 3) '(2 3 4) '(3 4 5)))
  ;; zip two lists via cl-mapcar
  (require 'cl-lib)
  (cl-mapcar #'cons '(a b c d) '(1 2 3 4))
  ;; Partition a number-sequence into evens and odds
  (let ((evens nil) (odds nil))
    (dolist (n (number-sequence 1 10))
      (if (= (mod n 2) 0)
          (push n evens)
        (push n odds)))
    (list (nreverse evens) (nreverse odds)))
  ;; Recursive list reversal
  (let ((my-rev nil))
    (setq my-rev
          (lambda (lst)
            (if (null lst) nil
              (append (funcall my-rev (cdr lst)) (list (car lst))))))
    (funcall my-rev '(5 4 3 2 1))))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 1 4 9 16 25 36 49) ((1 10) (2 20) (3 30) (4 40) (5 50)) (1 8 27 64 125 216) (1 2 3 4 5 6) (1 2 3 4 5) cl-lib ((a . 1) (b . 2) (c . 3) (d . 4)) ((2 4 6 8 10) (1 3 5 7 9)) (1 2 3 4 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 12: Interactions between list operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_list_creation_operation_interactions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((seq (number-sequence 1 10)))
  (list
    ;; take + last
    (last (take 5 seq))
    ;; butlast + nth
    (nth 2 (butlast seq 3))
    ;; nthcdr + length
    (length (nthcdr 4 seq))
    ;; append + make-list + number-sequence
    (append (make-list 3 'x) (number-sequence 1 4))
    ;; safe-length on dotted result of append
    (safe-length (append '(1 2 3) 'end))
    ;; proper-list-p after various constructions
    (proper-list-p (make-list 5 nil))
    (proper-list-p (cons 1 (cons 2 3)))
    (proper-list-p (append '(a) '(b) '(c)))
    ;; Chain: take from reversed number-sequence
    (take 3 (reverse (number-sequence 1 8)))
    ;; nthcdr of butlast
    (nthcdr 2 (butlast seq 2))))"#;
    let expect =
        expect_test::expect![[r#""OK ((5) 3 6 (x x x 1 2 3 4) 3 5 nil 3 (8 7 6) (3 4 5 6 7 8))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
