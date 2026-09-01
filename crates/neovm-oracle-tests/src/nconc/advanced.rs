//! Oracle parity tests for advanced `nconc` and destructive list mutation:
//! multi-arg nconc, nil positioning, nconc vs append, nconc+nreverse,
//! deep setcar/setcdr, delete/delq semantics, in-place flattening,
//! and mutation aliasing detection.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// nconc with multiple args (3+ lists including empty)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_adv_multi_arg_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // nconc with 5 arguments, some empty, verify structural result
    // Also checks that mutation of first list is visible
    let form = r#"(let* ((a (list 1 2))
                         (b (list 3))
                         (c nil)
                         (d (list 4 5 6))
                         (e (list 7))
                         (result (nconc a b c d e)))
                    (list
                      ;; Result is the full concatenation
                      result
                      ;; a was mutated in place (its cdr chain now extends)
                      (length a)
                      ;; a and result share identity
                      (eq a result)
                      ;; b was also mutated to link to d (c was nil, skipped)
                      (eq (nthcdr 2 a) b)
                      ;; Verify final element
                      (car (last result))))"#;
    let expect = expect_test::expect![[r#""OK ((1 2 3 4 5 6 7) 7 t t 7)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// nconc with nil args in various positions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_adv_nil_args_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Systematic: nil at beginning, middle, end, all nil, single non-nil
    let form = r#"(list
  ;; nil at beginning
  (nconc nil (list 1 2) (list 3))
  ;; nil at end
  (nconc (list 1 2) (list 3) nil)
  ;; nil in middle
  (nconc (list 1) nil nil (list 2) nil (list 3))
  ;; all nil
  (nconc nil nil nil)
  ;; single non-nil among nils
  (nconc nil nil (list 42) nil nil)
  ;; no args
  (nconc)
  ;; single nil
  (nconc nil)
  ;; Non-list last arg (dotted pair result)
  (nconc (list 1 2) 3))"#;
    let expect =
        expect_test::expect![[r#""OK ((1 2 3) (1 2 3) (1 2 3) nil (42) nil nil (1 2 . 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// nconc vs append behavior differences (mutation vs copy)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_adv_vs_append_mutation_diff() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Demonstrate that append copies but nconc mutates
    // After append, original is unchanged; after nconc, original is extended
    let form = r#"(let* ((original1 (list 'a 'b 'c))
                         (original2 (list 'a 'b 'c))
                         (tail (list 'x 'y))
                         ;; append: non-destructive
                         (appended (append original1 tail))
                         ;; nconc: destructive
                         (nconced (nconc original2 (list 'x 'y))))
                    (list
                      ;; append result
                      appended
                      ;; original1 unchanged by append
                      original1
                      (length original1)
                      ;; nconc result
                      nconced
                      ;; original2 WAS mutated by nconc
                      original2
                      (length original2)
                      ;; original2 and nconced are eq
                      (eq original2 nconced)
                      ;; original1 and appended are NOT eq
                      (eq original1 appended)))"#;
    let expect =
        expect_test::expect![[r#""OK ((a b c x y) (a b c) 3 (a b c x y) (a b c x y) 5 t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// nconc + nreverse for efficient list building
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_adv_nreverse_efficient_build() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Classic idiom: cons onto front of accumulator (O(1) each),
    // then nreverse at end — vs nconc at each step (O(n) each).
    // Both should produce same result, but exercise different mutation.
    let form = r#"(let ((data '(1 2 3 4 5 6 7 8 9 10))
                        (remaining nil)
                        (acc-cons nil)
                        (acc-nconc nil))
                    ;; Method 1: cons + nreverse
                    (setq remaining data)
                    (while remaining
                      (let ((x (car remaining)))
                        (when (= 0 (% x 2))
                          (setq acc-cons (cons (* x x) acc-cons))))
                      (setq remaining (cdr remaining)))
                    (setq acc-cons (nreverse acc-cons))
                    ;; Method 2: nconc at tail (less efficient but valid)
                    (setq remaining data)
                    (while remaining
                      (let ((x (car remaining)))
                        (when (= 0 (% x 2))
                          (setq acc-nconc (nconc acc-nconc (list (* x x))))))
                      (setq remaining (cdr remaining)))
                    ;; Both should produce same result
                    (list acc-cons
                          acc-nconc
                          (equal acc-cons acc-nconc)))"#;
    let expect = expect_test::expect![[r#""OK ((4 16 36 64 100) (4 16 36 64 100) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// setcar/setcdr deep mutation patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_adv_setcar_setcdr_deep_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a structure, then mutate it deeply with setcar/setcdr
    // Verify that aliased references see the mutations
    let form = r#"(let* ((inner (list 'x 'y 'z))
                         (outer (list 1 inner 3))
                         ;; Save alias to inner
                         (alias (cadr outer)))
                    ;; Mutate through the alias
                    (setcar alias 'A)
                    (setcdr (cdr alias) '(C D))
                    (let ((r1 (list outer inner alias
                                    (eq (cadr outer) inner)
                                    (eq alias inner))))
                      ;; Now setcdr the outer list to rearrange
                      (setcdr outer (list 'replaced))
                      (let ((r2 (list outer
                                      ;; inner still has our mutations
                                      inner
                                      ;; but outer no longer points to inner
                                      (cadr outer))))
                        ;; Build association list via mutation
                        (let ((alist (list (cons 'a 1) (cons 'b 2) (cons 'c 3))))
                          ;; Update value for key 'b via setcdr
                          (setcdr (assq 'b alist) 99)
                          ;; Add new pair by setcdr on last
                          (setcdr (last alist) (list (cons 'd 4)))
                          (list r1 r2 alist)))))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 replaced) (A y C D) (A y C D) t t) ((1 replaced) (A y C D) replaced) ((a . 1) (b . 99) (c . 3) (d . 4)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// delete vs delq mutation semantics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_adv_delete_delq_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // delq uses eq, delete uses equal — different behavior for strings/lists
    // Both mutate the list structure
    let form = r#"(let* (;; delq with symbols (eq works)
                         (l1 (list 'a 'b 'c 'b 'd))
                         (d1 (delq 'b l1))
                         ;; delq at head
                         (l2 (list 'x 'x 'y 'z))
                         (d2 (delq 'x l2))
                         ;; delete with strings (equal needed)
                         (s1 (list "foo" "bar" "foo" "baz"))
                         (d3 (delete "foo" s1))
                         ;; delq with numbers (eq may not work for large numbers)
                         ;; but works for small fixnums
                         (l3 (list 1 2 3 2 4))
                         (d4 (delq 2 l3))
                         ;; delete with equal comparison for lists
                         (l4 (list '(a 1) '(b 2) '(a 1) '(c 3)))
                         (d5 (delete '(a 1) l4))
                         ;; Deleting all elements
                         (l5 (list 'x 'x 'x))
                         (d6 (delq 'x l5))
                         ;; Deleting from nil
                         (d7 (delq 'a nil)))
                    (list d1 d2 d3 d4 d5 d6 d7))"#;
    let expect = expect_test::expect![[
        r#""OK ((a c d) (y z) (\"bar\" \"baz\") (1 3 4) ((b 2) (c 3)) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: in-place list flattening with nconc
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_adv_inplace_flatten() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Flatten a nested list structure using nconc for in-place joining
    let form = r#"(progn
  (fset 'neovm--test-flatten
    (lambda (tree)
      (cond
        ((null tree) nil)
        ((not (consp tree)) (list tree))
        (t (let ((result nil)
                 (remaining tree))
             (while remaining
               (setq result
                     (nconc result
                            (funcall 'neovm--test-flatten (car remaining))))
               (setq remaining (cdr remaining)))
             result)))))

  (unwind-protect
      (list
        (funcall 'neovm--test-flatten '(1 (2 3) (4 (5 6) 7) 8))
        (funcall 'neovm--test-flatten '((((a))) (b (c)) d))
        (funcall 'neovm--test-flatten nil)
        (funcall 'neovm--test-flatten '(1))
        (funcall 'neovm--test-flatten '(1 2 3))
        ;; Deeply nested
        (funcall 'neovm--test-flatten '((1 (2 (3 (4 (5)))))))
        ;; Mixed atoms and nested
        (funcall 'neovm--test-flatten '(a (b c) () (d (e () f)) g)))
    (fmakunbound 'neovm--test-flatten)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7 8) (a b c d) nil (1) (1 2 3) (1 2 3 4 5) (a b c d e f g))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: mutation aliasing and structure sharing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_adv_aliasing_structure_sharing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // After nconc, tail lists share structure. Mutating through one alias
    // is visible through another. Also: nconc with copy-sequence to break sharing.
    let form = r#"(let* ((shared-tail (list 'x 'y 'z))
                         (list-a (list 'a 'b))
                         (list-b (list 'c 'd))
                         ;; nconc both onto the same shared tail
                         (res-a (nconc list-a (copy-sequence shared-tail)))
                         (res-b (nconc list-b shared-tail)))
                    ;; list-b's tail IS shared-tail
                    ;; list-a's tail is a COPY
                    (let ((before-a (copy-sequence res-a))
                          (before-b (copy-sequence res-b)))
                      ;; Mutate shared-tail
                      (setcar shared-tail 'MUTATED)
                      (list
                        ;; res-b sees the mutation (shares structure)
                        res-b
                        ;; res-a does NOT see it (used copy-sequence)
                        res-a
                        ;; Verify via eq
                        (eq (nthcdr 2 res-b) shared-tail)
                        ;; before snapshots for comparison
                        before-a
                        before-b)))"#;
    let expect =
        expect_test::expect![[r#""OK ((c d MUTATED y z) (a b x y z) t (a b x y z) (c d x y z))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
