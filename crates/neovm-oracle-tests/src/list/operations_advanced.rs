//! Oracle parity tests for advanced list operations:
//! `cons`/`car`/`cdr` on dotted pairs vs proper lists,
//! `append` with 3+ args including nil,
//! `nconc` mutation vs `append` copy behavior,
//! `member` vs `memq` vs `assoc` vs `assq` differences,
//! `sort` with lambda predicate,
//! `mapcar` + `mapconcat` + `mapc` combined pipeline,
//! list-based set operations, and alist-based database.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// cons/car/cdr on dotted pairs vs proper lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cons_car_cdr_dotted_vs_proper() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Systematic exploration of dotted pairs, proper lists, and their
    // car/cdr decomposition, including nested dotted pairs
    let form = r#"(list
  ;; Dotted pair basics
  (car '(a . b))
  (cdr '(a . b))
  (consp '(a . b))
  (listp '(a . b))
  ;; Proper list: cdr is a list
  (car '(a b c))
  (cdr '(a b c))
  ;; Nested dotted pairs
  (car '((a . b) . (c . d)))
  (cdr '((a . b) . (c . d)))
  (caar '((a . b) . (c . d)))
  (cdar '((a . b) . (c . d)))
  (cadr '((a . b) . (c . d)))
  (cddr '((a . b) . (c . d)))
  ;; cons builds dotted pair when cdr is not a list
  (cons 1 2)
  (cons 1 nil)
  (cons 1 '(2 3))
  ;; Three-element dotted: (1 2 . 3) = (cons 1 (cons 2 3))
  (let ((x (cons 1 (cons 2 3))))
    (list (car x) (cadr x) (cddr x)
          (proper-list-p x)
          (listp x)
          (consp x)))
  ;; car/cdr of nil
  (list (car nil) (cdr nil) (car-safe 5) (cdr-safe 5)))"#;
    let expect = expect_test::expect![[
        r#""OK (a b t t a (b c) (a . b) (c . d) a b c d (1 . 2) (1) (1 2 3) (1 2 3 nil t t) (nil nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// append with 3+ args including nil
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_append_multi_args_with_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // append with various combinations of nil, lists, and non-list last arg
    let form = r#"(let* ((a '(1 2))
                         (b '(3 4))
                         (c '(5))
                         (d nil))
  (list
   ;; 3 proper lists
   (append a b c)
   ;; With nils interspersed
   (append nil a nil b nil c nil)
   ;; All nils
   (append nil nil nil)
   ;; Single list
   (append a)
   ;; Empty and non-empty
   (append nil '(1) nil '(2) nil)
   ;; Last arg is non-list (creates dotted result)
   (append '(1 2) 3)
   (append '(1) '(2) 3)
   ;; Last arg is string (also works in Emacs)
   (append nil nil)
   ;; Append doesn't mutate originals
   (let ((orig (list 'x 'y)))
     (let ((result (append orig '(z))))
       (list orig result (eq orig result)
             (length orig) (length result))))
   ;; Nested lists in append
   (append '((a 1) (b 2)) '((c 3)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5) (1 2 3 4 5) nil (1 2) (1 2) (1 2 . 3) (1 2 . 3) nil ((x y) (x y z) nil 2 3) ((a 1) (b 2) (c 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// nconc mutation vs append copy behavior
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_vs_append_behavior() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Side-by-side comparison showing mutation differences
    let form = r#"(let* ((l1 (list 'a 'b))
                         (l2 (list 'c 'd))
                         (l3 (list 'a 'b))
                         (l4 (list 'c 'd))
                         ;; append: copies l1, shares l2
                         (app-result (append l1 l2))
                         ;; nconc: mutates l3, shares l4
                         (nconc-result (nconc l3 l4)))
  (list
   ;; Results are equal
   (equal app-result nconc-result)
   ;; But identity differs
   (eq l1 app-result)        ;; nil: append copies
   (eq l3 nconc-result)      ;; t: nconc mutates in place
   ;; l1 still has original length
   (length l1)
   ;; l3 now has extended length
   (length l3)
   ;; Last arg is shared in both cases
   (eq (nthcdr 2 app-result) l2)
   (eq (nthcdr 2 nconc-result) l4)
   ;; Mutation via nconc is visible through l3
   l3
   ;; Append result
   app-result
   nconc-result))"#;
    let expect = expect_test::expect![[r#""OK (t nil t 2 4 t t (a b c d) (a b c d) (a b c d))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// member vs memq vs assoc vs assq differences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_member_memq_assoc_assq_differences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // member uses equal, memq uses eq
    // assoc uses equal on car, assq uses eq on car
    let form = r#"(let ((sym-list '(a b c d e))
                        (str-list (list "foo" "bar" "baz"))
                        (num-list '(1 2 3 4 5))
                        (alist '((a . 1) (b . 2) (c . 3) ("key" . 4))))
  (list
   ;; memq with symbols (eq works for symbols)
   (memq 'c sym-list)
   (memq 'z sym-list)
   ;; member with symbols (equal also works)
   (member 'c sym-list)
   ;; memq with strings: eq fails for distinct string objects
   (memq "foo" str-list)
   ;; member with strings: equal succeeds
   (member "foo" str-list)
   ;; memq vs member with numbers (small fixnums: both work)
   (memq 3 num-list)
   (member 3 num-list)
   ;; assq with symbol keys (eq works)
   (assq 'b alist)
   (assq 'z alist)
   ;; assoc with symbol keys (equal also works)
   (assoc 'b alist)
   ;; assq with string key: eq fails
   (assq "key" alist)
   ;; assoc with string key: equal succeeds
   (assoc "key" alist)
   ;; Nested alist lookup
   (let ((nested '((x . ((a . 1) (b . 2)))
                    (y . ((c . 3) (d . 4))))))
     (list (cdr (assq 'a (cdr (assq 'x nested))))
           (cdr (assq 'd (cdr (assq 'y nested))))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((c d e) nil (c d e) nil (\"foo\" \"bar\" \"baz\") (3 4 5) (3 4 5) (b . 2) nil (b . 2) nil (\"key\" . 4) (1 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// sort with lambda predicate (stable sort patterns)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_with_lambda_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Sort with custom lambda predicates for various data types
    let form = r#"(list
  ;; Sort by absolute value
  (sort (list -5 3 -1 4 -2)
        (lambda (a b) (< (abs a) (abs b))))
  ;; Sort strings by length
  (sort (list "cherry" "a" "banana" "kiwi" "fig")
        (lambda (a b) (< (length a) (length b))))
  ;; Sort alist by values (cdr)
  (sort (list '(a . 3) '(b . 1) '(c . 2))
        (lambda (x y) (< (cdr x) (cdr y))))
  ;; Reverse sort
  (sort (list 1 5 3 2 4)
        (lambda (a b) (> a b)))
  ;; Sort by second element of sub-lists
  (sort (list '(x 30) '(y 10) '(z 20))
        (lambda (a b) (< (cadr a) (cadr b))))
  ;; Sort mixed positive/negative, even first then odd
  (sort (list 5 2 8 1 4 7 6 3)
        (lambda (a b)
          (let ((a-even (= 0 (% a 2)))
                (b-even (= 0 (% b 2))))
            (if (eq a-even b-even)
                (< a b)
              a-even)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((-1 -2 3 4 -5) (\"a\" \"fig\" \"kiwi\" \"cherry\" \"banana\") ((b . 1) (c . 2) (a . 3)) (5 4 3 2 1) ((y 10) (z 20) (x 30)) (2 4 6 8 1 3 5 7))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapcar + mapconcat + mapc combined pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapcar_mapconcat_mapc_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Chain mapcar, mapconcat, and mapc in a data processing pipeline
    let form = r#"(let* ((data '(1 2 3 4 5 6 7 8 9 10))
                         ;; mapcar: square each element
                         (squared (mapcar (lambda (x) (* x x)) data))
                         ;; mapcar: filter to keep only > 10
                         (big (let ((acc nil))
                                (mapc (lambda (x)
                                        (when (> x 10) (setq acc (cons x acc))))
                                      squared)
                                (nreverse acc)))
                         ;; mapconcat: join as comma-separated string
                         (joined (mapconcat 'number-to-string big ", "))
                         ;; mapcar: convert back from strings
                         (words (split-string joined ", "))
                         (nums (mapcar 'string-to-number words))
                         ;; mapc with side effect: sum
                         (total 0))
  (mapc (lambda (n) (setq total (+ total n))) nums)
  (list squared big joined nums total
        ;; Verify roundtrip
        (equal nums big)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 4 9 16 25 36 49 64 81 100) (16 25 36 49 64 81 100) \"16, 25, 36, 49, 64, 81, 100\" (16 25 36 49 64 81 100) 371 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: list-based set operations (union, intersection, difference)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_list_set_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement union, intersection, and set-difference using list primitives
    let form = r#"(progn
  ;; Set union (no duplicates, preserving order of first set)
  (fset 'neovm--set-union
    (lambda (a b)
      (let ((result (copy-sequence a)))
        (mapc (lambda (x)
                (unless (member x result)
                  (setq result (nconc result (list x)))))
              b)
        result)))
  ;; Set intersection
  (fset 'neovm--set-intersect
    (lambda (a b)
      (let ((result nil))
        (mapc (lambda (x)
                (when (member x b)
                  (setq result (cons x result))))
              a)
        (nreverse result))))
  ;; Set difference (in a but not in b)
  (fset 'neovm--set-diff
    (lambda (a b)
      (let ((result nil))
        (mapc (lambda (x)
                (unless (member x b)
                  (setq result (cons x result))))
              a)
        (nreverse result))))
  (unwind-protect
      (let ((s1 '(1 2 3 4 5))
            (s2 '(3 4 5 6 7))
            (s3 '(5 6 7 8 9)))
        (list
         ;; Union
         (funcall 'neovm--set-union s1 s2)
         ;; Intersection
         (funcall 'neovm--set-intersect s1 s2)
         ;; Difference
         (funcall 'neovm--set-diff s1 s2)
         (funcall 'neovm--set-diff s2 s1)
         ;; Three-way intersection
         (funcall 'neovm--set-intersect
                  (funcall 'neovm--set-intersect s1 s2) s3)
         ;; Union of all three
         (funcall 'neovm--set-union
                  (funcall 'neovm--set-union s1 s2) s3)
         ;; Symmetric difference: (a-b) union (b-a)
         (funcall 'neovm--set-union
                  (funcall 'neovm--set-diff s1 s2)
                  (funcall 'neovm--set-diff s2 s1))
         ;; Edge cases
         (funcall 'neovm--set-union nil s1)
         (funcall 'neovm--set-intersect nil s1)
         (funcall 'neovm--set-diff s1 s1)))
    (fmakunbound 'neovm--set-union)
    (fmakunbound 'neovm--set-intersect)
    (fmakunbound 'neovm--set-diff)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7) (3 4 5) (1 2) (6 7) (5) (1 2 3 4 5 6 7 8 9) (1 2 6 7) (1 2 3 4 5) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: alist-based database with query/update/delete
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_database() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a mini database using alists with CRUD operations
    let form = r#"(progn
  ;; Database is an alist of (id . record) where record is an alist
  (fset 'neovm--db-insert
    (lambda (db id record)
      (cons (cons id record) db)))

  (fset 'neovm--db-find
    (lambda (db id)
      (cdr (assq id db))))

  (fset 'neovm--db-update-field
    (lambda (db id field value)
      (let ((entry (assq id db)))
        (when entry
          (let ((field-entry (assq field (cdr entry))))
            (if field-entry
                (setcdr field-entry value)
              (setcdr entry (cons (cons field value) (cdr entry))))))
        db)))

  (fset 'neovm--db-delete
    (lambda (db id)
      (let ((result nil))
        (mapc (lambda (entry)
                (unless (eq (car entry) id)
                  (setq result (cons entry result))))
              db)
        (nreverse result))))

  (fset 'neovm--db-query
    (lambda (db field value)
      (let ((results nil))
        (mapc (lambda (entry)
                (let ((fv (cdr (assq field (cdr entry)))))
                  (when (equal fv value)
                    (setq results (cons (car entry) results)))))
              db)
        (nreverse results))))

  (unwind-protect
      (let ((db nil))
        ;; Insert records
        (setq db (funcall 'neovm--db-insert db 'alice
                          '((name . "Alice") (age . 30) (role . "engineer"))))
        (setq db (funcall 'neovm--db-insert db 'bob
                          '((name . "Bob") (age . 25) (role . "designer"))))
        (setq db (funcall 'neovm--db-insert db 'carol
                          '((name . "Carol") (age . 35) (role . "engineer"))))
        (let ((r1 (funcall 'neovm--db-find db 'alice))
              (r2 (funcall 'neovm--db-find db 'bob))
              (r3 (funcall 'neovm--db-find db 'nobody)))
          ;; Update Bob's age
          (setq db (funcall 'neovm--db-update-field db 'bob 'age 26))
          (let ((r4 (cdr (assq 'age (funcall 'neovm--db-find db 'bob)))))
            ;; Query all engineers
            (let ((engineers (funcall 'neovm--db-query db 'role "engineer")))
              ;; Delete Bob
              (setq db (funcall 'neovm--db-delete db 'bob))
              (let ((r5 (funcall 'neovm--db-find db 'bob))
                    (r6 (length db)))
                (list
                 (cdr (assq 'name r1))
                 (cdr (assq 'role r2))
                 r3
                 r4
                 engineers
                 r5
                 r6))))))
    (fmakunbound 'neovm--db-insert)
    (fmakunbound 'neovm--db-find)
    (fmakunbound 'neovm--db-update-field)
    (fmakunbound 'neovm--db-delete)
    (fmakunbound 'neovm--db-query)))"#;
    let expect =
        expect_test::expect![[r#""OK (\"Alice\" \"designer\" nil 26 (carol alice) nil 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
