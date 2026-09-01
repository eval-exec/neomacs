//! Complex oracle tests for collection manipulation combinations.
//!
//! Tests flatten, tree rotation, permutations, multi-predicate partition,
//! interleave/transpose, and frequency-sorted histograms.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Flatten arbitrarily nested lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coll_flatten_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Recursive flatten that handles arbitrary nesting depth
    let form = r#"(let ((my-flatten nil))
                    (setq my-flatten
                          (lambda (tree)
                            (cond
                             ((null tree) nil)
                             ((not (consp tree)) (list tree))
                             (t (append (funcall my-flatten (car tree))
                                        (funcall my-flatten (cdr tree)))))))
                    (list
                     ;; Already flat
                     (funcall my-flatten '(1 2 3 4 5))
                     ;; One level nesting
                     (funcall my-flatten '(1 (2 3) 4 (5 6)))
                     ;; Deep nesting
                     (funcall my-flatten '(1 (2 (3 (4 (5))))))
                     ;; Mixed nesting depths
                     (funcall my-flatten '((1) ((2)) (((3))) ((((4))))))
                     ;; With symbols and strings
                     (funcall my-flatten '((a (b c)) (d (e (f)))))
                     ;; Empty sublists are removed
                     (funcall my-flatten '(1 () 2 (()) 3 (() ())))
                     ;; Single element
                     (funcall my-flatten '(((((42))))))
                     ;; Flat to begin with
                     (funcall my-flatten nil)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5) (1 2 3 4 5 6) (1 2 3 4 5) (1 2 3 4) (a b c d e f) (1 2 3) (42) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Tree rotation (left and right rotations on binary tree)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coll_tree_rotations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Binary tree as (value left right), rotations for AVL/red-black trees
    let form = r#"(let ((rotate-left nil)
                        (rotate-right nil)
                        (tree-inorder nil))
                    ;; Node: (value left right)
                    ;; Left rotation: pivot right child up
                    (setq rotate-left
                          (lambda (node)
                            (if (or (null node) (null (caddr node)))
                                node
                              (let ((val (car node))
                                    (left (cadr node))
                                    (right (caddr node)))
                                (let ((r-val (car right))
                                      (r-left (cadr right))
                                      (r-right (caddr right)))
                                  (list r-val
                                        (list val left r-left)
                                        r-right))))))
                    ;; Right rotation: pivot left child up
                    (setq rotate-right
                          (lambda (node)
                            (if (or (null node) (null (cadr node)))
                                node
                              (let ((val (car node))
                                    (left (cadr node))
                                    (right (caddr node)))
                                (let ((l-val (car left))
                                      (l-left (cadr left))
                                      (l-right (caddr left)))
                                  (list l-val
                                        l-left
                                        (list val l-right right)))))))
                    ;; Inorder traversal
                    (setq tree-inorder
                          (lambda (node)
                            (if (null node)
                                nil
                              (append (funcall tree-inorder (cadr node))
                                      (list (car node))
                                      (funcall tree-inorder (caddr node))))))
                    ;; Build a tree:       5
                    ;;                    / \
                    ;;                   3   7
                    ;;                  / \   \
                    ;;                 1   4   9
                    (let ((tree '(5 (3 (1 nil nil) (4 nil nil))
                                    (7 nil (9 nil nil)))))
                      (let ((left-rotated (funcall rotate-left tree))
                            (right-rotated (funcall rotate-right tree)))
                        (list
                         ;; Original inorder: 1 3 4 5 7 9
                         (funcall tree-inorder tree)
                         ;; After left rotation, inorder unchanged
                         (funcall tree-inorder left-rotated)
                         ;; After right rotation, inorder unchanged
                         (funcall tree-inorder right-rotated)
                         ;; Structure after left rotation (7 becomes root)
                         left-rotated
                         ;; Structure after right rotation (3 becomes root)
                         right-rotated
                         ;; Double rotation: left then right
                         (funcall tree-inorder
                                  (funcall rotate-right
                                           (funcall rotate-left tree)))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 3 4 5 7 9) (1 3 4 5 7 9) (1 3 4 5 7 9) (7 (5 (3 (1 nil nil) (4 nil nil)) nil) (9 nil nil)) (3 (1 nil nil) (5 (4 nil nil) (7 nil (9 nil nil)))) (1 3 4 5 7 9))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Permutations and combinations generation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coll_permutations_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Generate all permutations and k-combinations of a list
    let form = r#"(let ((permutations nil)
                        (remove-first nil)
                        (combinations nil))
                    ;; Remove first occurrence of an element
                    (setq remove-first
                          (lambda (x lst)
                            (cond
                             ((null lst) nil)
                             ((equal x (car lst)) (cdr lst))
                             (t (cons (car lst)
                                      (funcall remove-first x (cdr lst)))))))
                    ;; All permutations
                    (setq permutations
                          (lambda (lst)
                            (if (null (cdr lst))
                                (list lst)
                              (let ((result nil))
                                (dolist (elem lst)
                                  (let ((rest (funcall remove-first elem lst)))
                                    (dolist (perm (funcall permutations rest))
                                      (setq result
                                            (cons (cons elem perm) result)))))
                                (nreverse result)))))
                    ;; k-combinations (choose k from list)
                    (setq combinations
                          (lambda (lst k)
                            (cond
                             ((= k 0) '(nil))
                             ((null lst) nil)
                             (t (append
                                 ;; Include first element
                                 (mapcar (lambda (c) (cons (car lst) c))
                                         (funcall combinations (cdr lst) (1- k)))
                                 ;; Exclude first element
                                 (funcall combinations (cdr lst) k))))))
                    (list
                     ;; Permutations of (1 2 3)
                     (funcall permutations '(1 2 3))
                     ;; Number of permutations = 3! = 6
                     (length (funcall permutations '(1 2 3)))
                     ;; 2-combinations of (a b c d)
                     (funcall combinations '(a b c d) 2)
                     ;; C(4,2) = 6
                     (length (funcall combinations '(a b c d) 2))
                     ;; C(4,0) = 1
                     (funcall combinations '(a b c d) 0)
                     ;; C(4,4) = 1
                     (funcall combinations '(a b c d) 4)
                     ;; C(4,1) = 4
                     (length (funcall combinations '(a b c d) 1))))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 2 3) (1 3 2) (2 1 3) (2 3 1) (3 1 2) (3 2 1)) 6 ((a b) (a c) (a d) (b c) (b d) (c d)) 6 (nil) ((a b c d)) 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Partition by multiple predicates simultaneously
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coll_multi_partition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Partition a list into buckets based on multiple predicates
    let form = r#"(let ((multi-partition nil))
                    ;; Returns a list of N+1 buckets: one per predicate + remainder
                    (setq multi-partition
                          (lambda (lst preds)
                            (let ((buckets (make-list (1+ (length preds)) nil))
                                  (num-preds (length preds)))
                              (dolist (item lst)
                                (let ((placed nil)
                                      (i 0))
                                  (while (and (not placed) (< i num-preds))
                                    (when (funcall (nth i preds) item)
                                      (let ((bucket (nthcdr i buckets)))
                                        (setcar bucket (cons item (car bucket))))
                                      (setq placed t))
                                    (setq i (1+ i)))
                                  ;; If no predicate matched, goes to remainder
                                  (unless placed
                                    (let ((last-bucket (nthcdr num-preds buckets)))
                                      (setcar last-bucket
                                              (cons item (car last-bucket)))))))
                              ;; Reverse all buckets to maintain order
                              (mapcar #'nreverse buckets))))
                    (list
                     ;; Partition numbers: negative, zero, positive
                     (funcall multi-partition
                              '(-5 0 3 -2 7 0 -1 4 0 8)
                              (list (lambda (x) (< x 0))
                                    (lambda (x) (= x 0))))
                     ;; Partition by type
                     (funcall multi-partition
                              '(1 "a" foo 2 "b" bar 3.0)
                              (list #'integerp
                                    #'stringp
                                    #'symbolp))
                     ;; Partition with overlapping predicates (first match wins)
                     (funcall multi-partition
                              '(1 2 3 4 5 6 7 8 9 10 11 12)
                              (list (lambda (x) (= (% x 3) 0))
                                    (lambda (x) (= (% x 2) 0))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((-5 -2 -1) (0 0 0) (3 7 4 8)) ((1 2) (\"a\" \"b\") (foo bar) (3.0)) ((3 6 9 12) (2 4 8 10) (1 5 7 11)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interleave and transpose operations on lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coll_interleave_transpose() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Interleave multiple lists and transpose a list of lists (matrix)
    let form = r#"(let ((interleave nil)
                        (transpose nil))
                    ;; Interleave: take one element from each list in round-robin
                    (setq interleave
                          (lambda (lists)
                            (let ((result nil)
                                  (remaining lists))
                              (while (let ((any-left nil))
                                       (dolist (l remaining)
                                         (when l (setq any-left t)))
                                       any-left)
                                (let ((new-remaining nil))
                                  (dolist (l remaining)
                                    (when l
                                      (setq result (cons (car l) result))
                                      (setq new-remaining
                                            (cons (cdr l) new-remaining))))
                                  (setq remaining (nreverse new-remaining))))
                              (nreverse result))))
                    ;; Transpose: rows become columns
                    (setq transpose
                          (lambda (matrix)
                            (if (null matrix)
                                nil
                              (let ((ncols (length (car matrix)))
                                    (result nil))
                                (dotimes (j ncols)
                                  (let ((col nil))
                                    (dolist (row matrix)
                                      (setq col (cons (nth j row) col)))
                                    (setq result (cons (nreverse col) result))))
                                (nreverse result)))))
                    (list
                     ;; Interleave two equal-length lists
                     (funcall interleave '((a b c) (1 2 3)))
                     ;; Interleave three lists
                     (funcall interleave '((a b) (1 2) (x y)))
                     ;; Interleave unequal lengths
                     (funcall interleave '((a b c d) (1 2)))
                     ;; Transpose 2x3 matrix
                     (funcall transpose '((1 2 3) (4 5 6)))
                     ;; Transpose 3x2 matrix
                     (funcall transpose '((1 2) (3 4) (5 6)))
                     ;; Transpose then transpose = identity
                     (funcall transpose
                              (funcall transpose '((a b c) (d e f))))
                     ;; Transpose 1x4
                     (funcall transpose '((1 2 3 4)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((a 1 b 2 c 3) (a 1 x b 2 y) (a 1 b 2 c d) ((1 4) (2 5) (3 6)) ((1 3 5) (2 4 6)) ((a b c) (d e f)) ((1) (2) (3) (4)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Frequency-sorted histogram
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coll_frequency_histogram() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Count frequencies and sort by frequency descending, then alphabetically
    let form = r#"(let ((frequency-table nil)
                        (histogram nil))
                    ;; Build frequency alist from a list using hash table
                    (setq frequency-table
                          (lambda (lst)
                            (let ((ht (make-hash-table :test 'equal))
                                  (result nil))
                              (dolist (item lst)
                                (puthash item (1+ (gethash item ht 0)) ht))
                              (maphash (lambda (k v)
                                         (setq result (cons (cons k v) result)))
                                       ht)
                              ;; Sort by frequency desc, then by key asc
                              (sort result
                                    (lambda (a b)
                                      (if (/= (cdr a) (cdr b))
                                          (> (cdr a) (cdr b))
                                        (string< (format "%s" (car a))
                                                 (format "%s" (car b)))))))))
                    ;; Build ASCII histogram string
                    (setq histogram
                          (lambda (freq-alist)
                            (mapcar (lambda (pair)
                                      (cons (car pair)
                                            (make-string (cdr pair) ?#)))
                                    freq-alist)))
                    (let ((data '(apple banana apple cherry banana apple
                                  date cherry apple banana)))
                      (let ((freq (funcall frequency-table data)))
                        (list
                         ;; Frequency table (sorted)
                         freq
                         ;; Histogram
                         (funcall histogram freq)
                         ;; Most frequent item
                         (caar freq)
                         ;; Total count
                         (apply #'+ (mapcar #'cdr freq))
                         ;; Number of unique items
                         (length freq)))))"#;
    let expect = expect_test::expect![[
        r#####""OK (((apple . 4) (banana . 3) (cherry . 2) (date . 1)) ((apple . \"####\") (banana . \"###\") (cherry . \"##\") (date . \"#\")) apple 10 4)""#####
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Zip and unzip operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coll_zip_unzip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // zip pairs up elements; unzip separates pairs back into lists
    let form = r#"(let ((zip nil)
                        (zip-with nil)
                        (unzip nil))
                    ;; zip: pair corresponding elements
                    (setq zip
                          (lambda (l1 l2)
                            (let ((result nil))
                              (while (and l1 l2)
                                (setq result (cons (cons (car l1) (car l2)) result))
                                (setq l1 (cdr l1))
                                (setq l2 (cdr l2)))
                              (nreverse result))))
                    ;; zip-with: combine with a function
                    (setq zip-with
                          (lambda (f l1 l2)
                            (let ((result nil))
                              (while (and l1 l2)
                                (setq result
                                      (cons (funcall f (car l1) (car l2)) result))
                                (setq l1 (cdr l1))
                                (setq l2 (cdr l2)))
                              (nreverse result))))
                    ;; unzip: split pairs into two lists
                    (setq unzip
                          (lambda (pairs)
                            (let ((firsts nil) (seconds nil))
                              (dolist (p pairs)
                                (setq firsts (cons (car p) firsts))
                                (setq seconds (cons (cdr p) seconds)))
                              (list (nreverse firsts) (nreverse seconds)))))
                    (list
                     ;; Basic zip
                     (funcall zip '(a b c) '(1 2 3))
                     ;; Zip unequal lengths (shorter wins)
                     (funcall zip '(a b c d) '(1 2))
                     ;; Zip-with addition
                     (funcall zip-with #'+ '(1 2 3) '(10 20 30))
                     ;; Zip-with string concat
                     (funcall zip-with #'concat '("hello" "good") '(" world" " night"))
                     ;; Unzip
                     (funcall unzip '((a . 1) (b . 2) (c . 3)))
                     ;; Roundtrip: zip then unzip
                     (funcall unzip (funcall zip '(x y z) '(10 20 30)))
                     ;; Zip with itself (duplicate)
                     (funcall zip '(1 2 3) '(1 2 3))))"#;
    let expect = expect_test::expect![[
        r#""OK (((a . 1) (b . 2) (c . 3)) ((a . 1) (b . 2)) (11 22 33) (\"hello world\" \"good night\") ((a b c) (1 2 3)) ((x y z) (10 20 30)) ((1 . 1) (2 . 2) (3 . 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Group-by with key function
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coll_group_by() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Group elements of a list by a key function, return alist of groups
    let form = r#"(let ((group-by nil))
                    (setq group-by
                          (lambda (key-fn lst)
                            (let ((ht (make-hash-table :test 'equal))
                                  (key-order nil)
                                  (result nil))
                              ;; Collect groups
                              (dolist (item lst)
                                (let ((k (funcall key-fn item)))
                                  (unless (gethash k ht)
                                    (setq key-order (cons k key-order)))
                                  (puthash k (cons item (gethash k ht nil)) ht)))
                              ;; Build result in original key order
                              (dolist (k (nreverse key-order))
                                (setq result
                                      (cons (cons k (nreverse (gethash k ht)))
                                            result)))
                              (nreverse result))))
                    (list
                     ;; Group numbers by parity
                     (funcall group-by
                              (lambda (x) (if (= (% x 2) 0) 'even 'odd))
                              '(1 2 3 4 5 6 7 8))
                     ;; Group strings by length
                     (funcall group-by #'length
                              '("a" "bb" "c" "dd" "eee" "f" "ggg"))
                     ;; Group by first character
                     (funcall group-by
                              (lambda (s) (aref s 0))
                              '("apple" "avocado" "banana" "blueberry" "cherry"))
                     ;; Group numbers by magnitude bucket
                     (funcall group-by
                              (lambda (x) (* (/ x 10) 10))
                              '(3 15 7 22 31 8 19 25 2 37))))"#;
    let expect = expect_test::expect![[
        r#""OK (((odd 1 3 5 7) (even 2 4 6 8)) ((1 \"a\" \"c\" \"f\") (2 \"bb\" \"dd\") (3 \"eee\" \"ggg\")) ((97 \"apple\" \"avocado\") (98 \"banana\" \"blueberry\") (99 \"cherry\")) ((0 3 7 8 2) (10 15 19) (20 22 25) (30 31 37)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
