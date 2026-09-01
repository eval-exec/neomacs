//! Oracle parity tests for difference lists in Elisp:
//! efficient O(1) append using continuation-passing representation,
//! dl-cons (prepend), dl-append, dl-to-list conversion,
//! building lists from streams, and tree flattening.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Core difference list operations: create, cons, snoc, to-list
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_difference_list_core_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A difference list is represented as a function that takes a tail
    // and prepends its contents: (lambda (tail) (cons ... tail))
    // dl-empty: (lambda (tail) tail)
    // dl-singleton x: (lambda (tail) (cons x tail))
    let form = r#"(progn
  ;; Empty difference list
  (fset 'neovm--dl-empty (lambda () (lambda (tail) tail)))

  ;; Singleton difference list
  (fset 'neovm--dl-singleton
    (lambda (x) (lambda (tail) (cons x tail))))

  ;; Convert difference list to regular list
  (fset 'neovm--dl-to-list
    (lambda (dl) (funcall dl nil)))

  ;; Prepend element to difference list (dl-cons)
  (fset 'neovm--dl-cons
    (lambda (x dl)
      (lambda (tail) (cons x (funcall dl tail)))))

  ;; Append element to end of difference list (dl-snoc)
  (fset 'neovm--dl-snoc
    (lambda (dl x)
      (lambda (tail) (funcall dl (cons x tail)))))

  ;; From regular list to difference list
  (fset 'neovm--dl-from-list
    (lambda (lst)
      (lambda (tail) (append lst tail))))

  (unwind-protect
      (list
       ;; Empty dl to list
       (funcall 'neovm--dl-to-list (funcall 'neovm--dl-empty))

       ;; Singleton
       (funcall 'neovm--dl-to-list (funcall 'neovm--dl-singleton 42))

       ;; Cons onto empty
       (funcall 'neovm--dl-to-list
                (funcall 'neovm--dl-cons 1 (funcall 'neovm--dl-empty)))

       ;; Cons multiple
       (funcall 'neovm--dl-to-list
                (funcall 'neovm--dl-cons 1
                         (funcall 'neovm--dl-cons 2
                                  (funcall 'neovm--dl-cons 3
                                           (funcall 'neovm--dl-empty)))))

       ;; Snoc onto empty
       (funcall 'neovm--dl-to-list
                (funcall 'neovm--dl-snoc (funcall 'neovm--dl-empty) 99))

       ;; Snoc multiple
       (funcall 'neovm--dl-to-list
                (funcall 'neovm--dl-snoc
                         (funcall 'neovm--dl-snoc
                                  (funcall 'neovm--dl-snoc (funcall 'neovm--dl-empty) 1)
                                  2)
                         3))

       ;; From list roundtrip
       (funcall 'neovm--dl-to-list
                (funcall 'neovm--dl-from-list '(a b c d)))

       ;; Cons + snoc mixed
       (funcall 'neovm--dl-to-list
                (funcall 'neovm--dl-cons 'first
                         (funcall 'neovm--dl-snoc
                                  (funcall 'neovm--dl-from-list '(middle))
                                  'last))))
    (fmakunbound 'neovm--dl-empty)
    (fmakunbound 'neovm--dl-singleton)
    (fmakunbound 'neovm--dl-to-list)
    (fmakunbound 'neovm--dl-cons)
    (fmakunbound 'neovm--dl-snoc)
    (fmakunbound 'neovm--dl-from-list)))"#;
    let expect = expect_test::expect![[
        r#""OK (nil (42) (1) (1 2 3) (99) (1 2 3) (a b c d) (first middle last))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// O(1) append of two difference lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_difference_list_append() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--dl2-empty (lambda () (lambda (tail) tail)))
  (fset 'neovm--dl2-singleton (lambda (x) (lambda (tail) (cons x tail))))
  (fset 'neovm--dl2-to-list (lambda (dl) (funcall dl nil)))
  (fset 'neovm--dl2-from-list
    (lambda (lst) (lambda (tail) (append lst tail))))

  ;; O(1) append: compose the two functions
  (fset 'neovm--dl2-append
    (lambda (dl1 dl2)
      (lambda (tail) (funcall dl1 (funcall dl2 tail)))))

  (unwind-protect
      (list
       ;; Append two non-empty dls
       (funcall 'neovm--dl2-to-list
                (funcall 'neovm--dl2-append
                         (funcall 'neovm--dl2-from-list '(1 2 3))
                         (funcall 'neovm--dl2-from-list '(4 5 6))))

       ;; Append empty + non-empty
       (funcall 'neovm--dl2-to-list
                (funcall 'neovm--dl2-append
                         (funcall 'neovm--dl2-empty)
                         (funcall 'neovm--dl2-from-list '(a b c))))

       ;; Append non-empty + empty
       (funcall 'neovm--dl2-to-list
                (funcall 'neovm--dl2-append
                         (funcall 'neovm--dl2-from-list '(x y))
                         (funcall 'neovm--dl2-empty)))

       ;; Append empty + empty
       (funcall 'neovm--dl2-to-list
                (funcall 'neovm--dl2-append
                         (funcall 'neovm--dl2-empty)
                         (funcall 'neovm--dl2-empty)))

       ;; Chain of appends: ((1 2) ++ (3 4)) ++ (5 6)
       (funcall 'neovm--dl2-to-list
                (funcall 'neovm--dl2-append
                         (funcall 'neovm--dl2-append
                                  (funcall 'neovm--dl2-from-list '(1 2))
                                  (funcall 'neovm--dl2-from-list '(3 4)))
                         (funcall 'neovm--dl2-from-list '(5 6))))

       ;; Associativity: (a ++ b) ++ c == a ++ (b ++ c)
       (let* ((a (funcall 'neovm--dl2-from-list '(1 2)))
              (b (funcall 'neovm--dl2-from-list '(3 4)))
              (c (funcall 'neovm--dl2-from-list '(5 6)))
              (left (funcall 'neovm--dl2-append
                             (funcall 'neovm--dl2-append a b) c))
              (right (funcall 'neovm--dl2-append
                              a (funcall 'neovm--dl2-append b c))))
         (equal (funcall 'neovm--dl2-to-list left)
                (funcall 'neovm--dl2-to-list right)))

       ;; Many appends (build list of 0..9 by appending singletons)
       (let ((dl (funcall 'neovm--dl2-empty)))
         (dotimes (i 10)
           (setq dl (funcall 'neovm--dl2-append
                             dl (funcall 'neovm--dl2-singleton i))))
         (funcall 'neovm--dl2-to-list dl)))
    (fmakunbound 'neovm--dl2-empty)
    (fmakunbound 'neovm--dl2-singleton)
    (fmakunbound 'neovm--dl2-to-list)
    (fmakunbound 'neovm--dl2-from-list)
    (fmakunbound 'neovm--dl2-append)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6) (a b c) (x y) nil (1 2 3 4 5 6) t (0 1 2 3 4 5 6 7 8 9))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Building lists from streams using difference lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_difference_list_stream_building() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--dl3-empty (lambda () (lambda (tail) tail)))
  (fset 'neovm--dl3-to-list (lambda (dl) (funcall dl nil)))
  (fset 'neovm--dl3-singleton (lambda (x) (lambda (tail) (cons x tail))))
  (fset 'neovm--dl3-append
    (lambda (dl1 dl2)
      (lambda (tail) (funcall dl1 (funcall dl2 tail)))))
  (fset 'neovm--dl3-from-list
    (lambda (lst) (lambda (tail) (append lst tail))))

  ;; Filter: build output dl by appending matching elements
  (fset 'neovm--dl3-filter
    (lambda (pred lst)
      (let ((dl (funcall 'neovm--dl3-empty)))
        (dolist (x lst)
          (when (funcall pred x)
            (setq dl (funcall 'neovm--dl3-append
                              dl (funcall 'neovm--dl3-singleton x)))))
        (funcall 'neovm--dl3-to-list dl))))

  ;; Map: build output dl by appending transformed elements
  (fset 'neovm--dl3-map
    (lambda (f lst)
      (let ((dl (funcall 'neovm--dl3-empty)))
        (dolist (x lst)
          (setq dl (funcall 'neovm--dl3-append
                            dl (funcall 'neovm--dl3-singleton (funcall f x)))))
        (funcall 'neovm--dl3-to-list dl))))

  ;; Flat-map: each element produces a list, append all
  (fset 'neovm--dl3-flatmap
    (lambda (f lst)
      (let ((dl (funcall 'neovm--dl3-empty)))
        (dolist (x lst)
          (setq dl (funcall 'neovm--dl3-append
                            dl (funcall 'neovm--dl3-from-list (funcall f x)))))
        (funcall 'neovm--dl3-to-list dl))))

  (unwind-protect
      (list
       ;; Filter even numbers
       (funcall 'neovm--dl3-filter
                (lambda (x) (= (mod x 2) 0))
                '(1 2 3 4 5 6 7 8 9 10))

       ;; Map: square each element
       (funcall 'neovm--dl3-map
                (lambda (x) (* x x))
                '(1 2 3 4 5))

       ;; Flat-map: each number n -> (n n+1)
       (funcall 'neovm--dl3-flatmap
                (lambda (x) (list x (1+ x)))
                '(10 20 30))

       ;; Chain: filter then map
       (funcall 'neovm--dl3-map
                (lambda (x) (* x 10))
                (funcall 'neovm--dl3-filter
                         (lambda (x) (> x 3))
                         '(1 2 3 4 5 6)))

       ;; Build sequence using dl from accumulation
       (let ((dl (funcall 'neovm--dl3-empty))
             (i 1))
         (while (<= i 5)
           ;; Append (i i*i) pairs
           (setq dl (funcall 'neovm--dl3-append
                             dl (funcall 'neovm--dl3-from-list
                                         (list (list i (* i i))))))
           (setq i (1+ i)))
         (funcall 'neovm--dl3-to-list dl))

       ;; Empty stream
       (funcall 'neovm--dl3-filter (lambda (x) nil) '(1 2 3)))
    (fmakunbound 'neovm--dl3-empty)
    (fmakunbound 'neovm--dl3-to-list)
    (fmakunbound 'neovm--dl3-singleton)
    (fmakunbound 'neovm--dl3-append)
    (fmakunbound 'neovm--dl3-from-list)
    (fmakunbound 'neovm--dl3-filter)
    (fmakunbound 'neovm--dl3-map)
    (fmakunbound 'neovm--dl3-flatmap)))"#;
    let expect = expect_test::expect![[
        r#""OK ((2 4 6 8 10) (1 4 9 16 25) (10 11 20 21 30 31) (40 50 60) ((1 1) (2 4) (3 9) (4 16) (5 25)) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Tree flattening using difference lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_difference_list_tree_flatten() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--dl4-empty (lambda () (lambda (tail) tail)))
  (fset 'neovm--dl4-to-list (lambda (dl) (funcall dl nil)))
  (fset 'neovm--dl4-singleton (lambda (x) (lambda (tail) (cons x tail))))
  (fset 'neovm--dl4-append
    (lambda (dl1 dl2)
      (lambda (tail) (funcall dl1 (funcall dl2 tail)))))

  ;; Flatten a tree (nested list structure) into a flat list using dl
  ;; A leaf is any non-cons value, a node is a cons cell
  (fset 'neovm--dl4-flatten
    (lambda (tree)
      (cond
       ((null tree) (funcall 'neovm--dl4-empty))
       ((not (consp tree)) (funcall 'neovm--dl4-singleton tree))
       (t (funcall 'neovm--dl4-append
                   (funcall 'neovm--dl4-flatten (car tree))
                   (funcall 'neovm--dl4-flatten (cdr tree)))))))

  ;; Flatten and collect: returns regular list
  (fset 'neovm--dl4-flatten-list
    (lambda (tree)
      (funcall 'neovm--dl4-to-list (funcall 'neovm--dl4-flatten tree))))

  (unwind-protect
      (list
       ;; Flat list (already flat)
       (funcall 'neovm--dl4-flatten-list '(1 2 3 4 5))

       ;; Nested list
       (funcall 'neovm--dl4-flatten-list '((1 2) (3 (4 5))))

       ;; Deeply nested
       (funcall 'neovm--dl4-flatten-list '(((1)) ((2 (3))) (4 (5 (6)))))

       ;; Single element
       (funcall 'neovm--dl4-flatten-list 42)

       ;; Empty
       (funcall 'neovm--dl4-flatten-list nil)

       ;; Mixed: atoms and lists at various depths
       (funcall 'neovm--dl4-flatten-list '(a (b c) d (e (f g (h)))))

       ;; Binary tree (left right structure): (value left right)
       ;; Inorder traversal via flatten of (left value right)
       (let ((tree '(4 (2 (1) (3)) (6 (5) (7)))))
         ;; Manually construct inorder: left subtree, root, right subtree
         (fset 'neovm--dl4-inorder
           (lambda (node)
             (if (null node)
                 (funcall 'neovm--dl4-empty)
               (let ((val (car node))
                     (left (cadr node))
                     (right (caddr node)))
                 (funcall 'neovm--dl4-append
                          (funcall 'neovm--dl4-inorder left)
                          (funcall 'neovm--dl4-append
                                   (funcall 'neovm--dl4-singleton val)
                                   (funcall 'neovm--dl4-inorder right)))))))
         (funcall 'neovm--dl4-to-list (funcall 'neovm--dl4-inorder tree))))
    (fmakunbound 'neovm--dl4-empty)
    (fmakunbound 'neovm--dl4-to-list)
    (fmakunbound 'neovm--dl4-singleton)
    (fmakunbound 'neovm--dl4-append)
    (fmakunbound 'neovm--dl4-flatten)
    (fmakunbound 'neovm--dl4-flatten-list)
    (fmakunbound 'neovm--dl4-inorder)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5) (1 2 3 4 5) (1 2 3 4 5 6) (42) nil (a b c d e f g h) (1 2 3 4 5 6 7))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Difference list vs regular append: correctness comparison
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_difference_list_vs_regular_append() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--dl5-empty (lambda () (lambda (tail) tail)))
  (fset 'neovm--dl5-to-list (lambda (dl) (funcall dl nil)))
  (fset 'neovm--dl5-singleton (lambda (x) (lambda (tail) (cons x tail))))
  (fset 'neovm--dl5-append
    (lambda (dl1 dl2)
      (lambda (tail) (funcall dl1 (funcall dl2 tail)))))
  (fset 'neovm--dl5-from-list
    (lambda (lst) (lambda (tail) (append lst tail))))

  ;; Build a list by repeated append, both ways
  (fset 'neovm--build-via-append
    (lambda (chunks)
      "Build list by regular append of chunks."
      (let ((result nil))
        (dolist (chunk chunks)
          (setq result (append result chunk)))
        result)))

  (fset 'neovm--build-via-dl
    (lambda (chunks)
      "Build list by dl-append of chunks."
      (let ((dl (funcall 'neovm--dl5-empty)))
        (dolist (chunk chunks)
          (setq dl (funcall 'neovm--dl5-append
                            dl (funcall 'neovm--dl5-from-list chunk))))
        (funcall 'neovm--dl5-to-list dl))))

  (unwind-protect
      (let ((chunks '((1 2 3) (4 5) (6) (7 8 9 10) nil (11 12))))
        (list
         ;; Both produce same result
         (equal (funcall 'neovm--build-via-append chunks)
                (funcall 'neovm--build-via-dl chunks))

         ;; The actual result
         (funcall 'neovm--build-via-dl chunks)

         ;; Single chunk
         (equal (funcall 'neovm--build-via-append '((a b c)))
                (funcall 'neovm--build-via-dl '((a b c))))

         ;; All empty chunks
         (funcall 'neovm--build-via-dl '(nil nil nil))

         ;; Many single-element chunks
         (funcall 'neovm--build-via-dl '((a) (b) (c) (d) (e)))

         ;; Build numbers 1..20 as individual singleton dls then convert
         (let ((dl (funcall 'neovm--dl5-empty)))
           (dotimes (i 20)
             (setq dl (funcall 'neovm--dl5-append
                               dl (funcall 'neovm--dl5-singleton (1+ i)))))
           (equal (funcall 'neovm--dl5-to-list dl)
                  (number-sequence 1 20)))))
    (fmakunbound 'neovm--dl5-empty)
    (fmakunbound 'neovm--dl5-to-list)
    (fmakunbound 'neovm--dl5-singleton)
    (fmakunbound 'neovm--dl5-append)
    (fmakunbound 'neovm--dl5-from-list)
    (fmakunbound 'neovm--build-via-append)
    (fmakunbound 'neovm--build-via-dl)))"#;
    let expect =
        expect_test::expect![[r#""OK (t (1 2 3 4 5 6 7 8 9 10 11 12) t nil (a b c d e) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: string builder using difference lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_difference_list_string_builder() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Use difference lists to build a list of characters, then concat to string
  ;; This simulates an efficient string builder
  (fset 'neovm--sb-empty (lambda () (lambda (tail) tail)))
  (fset 'neovm--sb-to-list (lambda (dl) (funcall dl nil)))
  (fset 'neovm--sb-append
    (lambda (dl1 dl2)
      (lambda (tail) (funcall dl1 (funcall dl2 tail)))))
  (fset 'neovm--sb-from-list
    (lambda (lst) (lambda (tail) (append lst tail))))

  ;; Add a string's chars as a dl
  (fset 'neovm--sb-add-string
    (lambda (sb str)
      (funcall 'neovm--sb-append
               sb
               (funcall 'neovm--sb-from-list (append str nil)))))

  ;; Convert sb to string
  (fset 'neovm--sb-to-string
    (lambda (sb)
      (apply 'string (funcall 'neovm--sb-to-list sb))))

  (unwind-protect
      (list
       ;; Build "hello world" from parts
       (let ((sb (funcall 'neovm--sb-empty)))
         (setq sb (funcall 'neovm--sb-add-string sb "hello"))
         (setq sb (funcall 'neovm--sb-add-string sb " "))
         (setq sb (funcall 'neovm--sb-add-string sb "world"))
         (funcall 'neovm--sb-to-string sb))

       ;; Build from many small parts
       (let ((sb (funcall 'neovm--sb-empty))
             (parts '("a" "b" "c" "d" "e")))
         (dolist (p parts)
           (setq sb (funcall 'neovm--sb-add-string sb p))
           (setq sb (funcall 'neovm--sb-add-string sb "-")))
         (funcall 'neovm--sb-to-string sb))

       ;; Build CSV line
       (let ((sb (funcall 'neovm--sb-empty))
             (fields '("name" "age" "city"))
             (first t))
         (dolist (f fields)
           (unless first
             (setq sb (funcall 'neovm--sb-add-string sb ",")))
           (setq sb (funcall 'neovm--sb-add-string sb f))
           (setq first nil))
         (funcall 'neovm--sb-to-string sb))

       ;; Empty string builder
       (funcall 'neovm--sb-to-string (funcall 'neovm--sb-empty))

       ;; Build number list as string: "1, 2, 3, 4, 5"
       (let ((sb (funcall 'neovm--sb-empty))
             (nums '(1 2 3 4 5))
             (first t))
         (dolist (n nums)
           (unless first
             (setq sb (funcall 'neovm--sb-add-string sb ", ")))
           (setq sb (funcall 'neovm--sb-add-string sb (number-to-string n)))
           (setq first nil))
         (funcall 'neovm--sb-to-string sb)))
    (fmakunbound 'neovm--sb-empty)
    (fmakunbound 'neovm--sb-to-list)
    (fmakunbound 'neovm--sb-append)
    (fmakunbound 'neovm--sb-from-list)
    (fmakunbound 'neovm--sb-add-string)
    (fmakunbound 'neovm--sb-to-string)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello world\" \"a-b-c-d-e-\" \"name,age,city\" \"\" \"1, 2, 3, 4, 5\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
