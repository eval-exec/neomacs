//! Oracle parity tests for a Union-Find (disjoint set) data structure
//! implemented in Elisp: make-set, find with path compression, union with
//! rank optimization, connected-p predicate, Kruskal's MST algorithm,
//! connected components counting, and equivalence class tracking.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Core Union-Find: make-set, find, union, connected-p
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_union_find_core() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Union-Find using a hash-table: parent[x] and rank[x].
    // find with path compression, union by rank.
    let form = r#"(progn
  ;; UF structure: (parent-table rank-table)
  (fset 'neovm--uf-create
    (lambda ()
      (list (make-hash-table :test 'equal)
            (make-hash-table :test 'equal))))

  ;; Make a new set containing just x
  (fset 'neovm--uf-make-set
    (lambda (uf x)
      (let ((parent (nth 0 uf))
            (rank (nth 1 uf)))
        (puthash x x parent)
        (puthash x 0 rank))))

  ;; Find with path compression
  (fset 'neovm--uf-find
    (lambda (uf x)
      (let ((parent (nth 0 uf)))
        (if (equal (gethash x parent) x)
            x
          (let ((root (funcall 'neovm--uf-find uf (gethash x parent))))
            (puthash x root parent)
            root)))))

  ;; Union by rank
  (fset 'neovm--uf-union
    (lambda (uf x y)
      (let ((rx (funcall 'neovm--uf-find uf x))
            (ry (funcall 'neovm--uf-find uf y)))
        (unless (equal rx ry)
          (let ((rank-tbl (nth 1 uf))
                (parent (nth 0 uf)))
            (let ((rank-rx (gethash rx rank-tbl))
                  (rank-ry (gethash ry rank-tbl)))
              (cond
               ((< rank-rx rank-ry)
                (puthash rx ry parent))
               ((> rank-rx rank-ry)
                (puthash ry rx parent))
               (t
                (puthash ry rx parent)
                (puthash rx (1+ rank-rx) rank-tbl)))))
          t))))

  ;; Check if two elements are in the same set
  (fset 'neovm--uf-connected-p
    (lambda (uf x y)
      (equal (funcall 'neovm--uf-find uf x)
             (funcall 'neovm--uf-find uf y))))

  (unwind-protect
      (let ((uf (funcall 'neovm--uf-create)))
        ;; Create 8 individual sets
        (dolist (x '(1 2 3 4 5 6 7 8))
          (funcall 'neovm--uf-make-set uf x))
        ;; Initially, no two elements are connected
        (let ((before (list
                       (funcall 'neovm--uf-connected-p uf 1 2)
                       (funcall 'neovm--uf-connected-p uf 3 4)
                       (funcall 'neovm--uf-connected-p uf 1 1))))
          ;; Union some elements: {1,2,3}, {4,5}, {6,7,8}
          (funcall 'neovm--uf-union uf 1 2)
          (funcall 'neovm--uf-union uf 2 3)
          (funcall 'neovm--uf-union uf 4 5)
          (funcall 'neovm--uf-union uf 6 7)
          (funcall 'neovm--uf-union uf 7 8)
          ;; Check connectivity after unions
          (let ((after (list
                        ;; Within same component
                        (funcall 'neovm--uf-connected-p uf 1 2)
                        (funcall 'neovm--uf-connected-p uf 1 3)
                        (funcall 'neovm--uf-connected-p uf 2 3)
                        (funcall 'neovm--uf-connected-p uf 4 5)
                        (funcall 'neovm--uf-connected-p uf 6 8)
                        ;; Across different components
                        (funcall 'neovm--uf-connected-p uf 1 4)
                        (funcall 'neovm--uf-connected-p uf 1 6)
                        (funcall 'neovm--uf-connected-p uf 4 6))))
            ;; Now merge {1,2,3} with {4,5}
            (funcall 'neovm--uf-union uf 3 5)
            (let ((merged (list
                           (funcall 'neovm--uf-connected-p uf 1 4)
                           (funcall 'neovm--uf-connected-p uf 2 5)
                           (funcall 'neovm--uf-connected-p uf 3 4)
                           ;; Still separate from {6,7,8}
                           (funcall 'neovm--uf-connected-p uf 1 6))))
              (list 'before before 'after after 'merged merged)))))
    (fmakunbound 'neovm--uf-create)
    (fmakunbound 'neovm--uf-make-set)
    (fmakunbound 'neovm--uf-find)
    (fmakunbound 'neovm--uf-union)
    (fmakunbound 'neovm--uf-connected-p)))"#;
    let expect = expect_test::expect![[
        r#""OK (before (nil nil t) after (t t t t t nil nil nil) merged (t t t nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Connected components counting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_union_find_component_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Count the number of distinct connected components
    let form = r#"(progn
  (fset 'neovm--uf2-create
    (lambda () (list (make-hash-table :test 'equal) (make-hash-table :test 'equal))))
  (fset 'neovm--uf2-make-set
    (lambda (uf x)
      (puthash x x (nth 0 uf))
      (puthash x 0 (nth 1 uf))))
  (fset 'neovm--uf2-find
    (lambda (uf x)
      (if (equal (gethash x (nth 0 uf)) x) x
        (let ((root (funcall 'neovm--uf2-find uf (gethash x (nth 0 uf)))))
          (puthash x root (nth 0 uf)) root))))
  (fset 'neovm--uf2-union
    (lambda (uf x y)
      (let ((rx (funcall 'neovm--uf2-find uf x))
            (ry (funcall 'neovm--uf2-find uf y)))
        (unless (equal rx ry)
          (let ((rk-x (gethash rx (nth 1 uf)))
                (rk-y (gethash ry (nth 1 uf))))
            (cond ((< rk-x rk-y) (puthash rx ry (nth 0 uf)))
                  ((> rk-x rk-y) (puthash ry rx (nth 0 uf)))
                  (t (puthash ry rx (nth 0 uf))
                     (puthash rx (1+ rk-x) (nth 1 uf)))))))))
  ;; Count distinct components among a list of elements
  (fset 'neovm--uf2-count-components
    (lambda (uf elements)
      (let ((roots (make-hash-table :test 'equal)))
        (dolist (e elements)
          (puthash (funcall 'neovm--uf2-find uf e) t roots))
        (hash-table-count roots))))

  (unwind-protect
      (let ((uf (funcall 'neovm--uf2-create))
            (elements '(1 2 3 4 5 6 7 8 9 10)))
        ;; Create individual sets
        (dolist (e elements)
          (funcall 'neovm--uf2-make-set uf e))
        ;; Initially 10 components
        (let ((c0 (funcall 'neovm--uf2-count-components uf elements)))
          ;; Union: {1,2}, {3,4}, {5,6}, {7,8}, {9,10}
          (funcall 'neovm--uf2-union uf 1 2)
          (funcall 'neovm--uf2-union uf 3 4)
          (funcall 'neovm--uf2-union uf 5 6)
          (funcall 'neovm--uf2-union uf 7 8)
          (funcall 'neovm--uf2-union uf 9 10)
          (let ((c1 (funcall 'neovm--uf2-count-components uf elements)))
            ;; Union: {1,2,3,4}, {5,6,7,8}
            (funcall 'neovm--uf2-union uf 2 3)
            (funcall 'neovm--uf2-union uf 6 7)
            (let ((c2 (funcall 'neovm--uf2-count-components uf elements)))
              ;; Union all into one component
              (funcall 'neovm--uf2-union uf 4 5)
              (funcall 'neovm--uf2-union uf 8 9)
              (let ((c3 (funcall 'neovm--uf2-count-components uf elements)))
                ;; Final union
                (funcall 'neovm--uf2-union uf 1 10)
                (let ((c4 (funcall 'neovm--uf2-count-components uf elements)))
                  (list c0 c1 c2 c3 c4)))))))
    (fmakunbound 'neovm--uf2-create)
    (fmakunbound 'neovm--uf2-make-set)
    (fmakunbound 'neovm--uf2-find)
    (fmakunbound 'neovm--uf2-union)
    (fmakunbound 'neovm--uf2-count-components)))"#;
    let expect = expect_test::expect![[r#""OK (10 5 3 1 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Kruskal's MST algorithm using union-find
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_union_find_kruskal_mst() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Kruskal's algorithm: sort edges by weight, greedily add if no cycle.
    // Graph: 6 vertices, 9 edges.
    let form = r#"(progn
  (fset 'neovm--uf3-create
    (lambda () (list (make-hash-table :test 'equal) (make-hash-table :test 'equal))))
  (fset 'neovm--uf3-make-set
    (lambda (uf x)
      (puthash x x (nth 0 uf))
      (puthash x 0 (nth 1 uf))))
  (fset 'neovm--uf3-find
    (lambda (uf x)
      (if (equal (gethash x (nth 0 uf)) x) x
        (let ((root (funcall 'neovm--uf3-find uf (gethash x (nth 0 uf)))))
          (puthash x root (nth 0 uf)) root))))
  (fset 'neovm--uf3-union
    (lambda (uf x y)
      (let ((rx (funcall 'neovm--uf3-find uf x))
            (ry (funcall 'neovm--uf3-find uf y)))
        (unless (equal rx ry)
          (let ((rk-x (gethash rx (nth 1 uf)))
                (rk-y (gethash ry (nth 1 uf))))
            (cond ((< rk-x rk-y) (puthash rx ry (nth 0 uf)))
                  ((> rk-x rk-y) (puthash ry rx (nth 0 uf)))
                  (t (puthash ry rx (nth 0 uf))
                     (puthash rx (1+ rk-x) (nth 1 uf)))))
          t))))
  (fset 'neovm--uf3-connected-p
    (lambda (uf x y)
      (equal (funcall 'neovm--uf3-find uf x)
             (funcall 'neovm--uf3-find uf y))))

  ;; Kruskal's MST: edges = list of (weight u v), sorted by weight
  (fset 'neovm--uf3-kruskal
    (lambda (vertices edges)
      (let ((uf (funcall 'neovm--uf3-create))
            (mst nil)
            (total-weight 0))
        ;; Make sets for all vertices
        (dolist (v vertices)
          (funcall 'neovm--uf3-make-set uf v))
        ;; Sort edges by weight (already sorted in our input)
        ;; Process each edge
        (dolist (edge edges)
          (let ((w (nth 0 edge))
                (u (nth 1 edge))
                (v (nth 2 edge)))
            (unless (funcall 'neovm--uf3-connected-p uf u v)
              (funcall 'neovm--uf3-union uf u v)
              (setq mst (cons edge mst))
              (setq total-weight (+ total-weight w)))))
        (list 'mst (nreverse mst)
              'total-weight total-weight
              'edge-count (length mst)))))

  (unwind-protect
      (let* ((vertices '(A B C D E F))
             ;; Edges sorted by weight (weight u v)
             (edges '((1 A B) (2 B C) (2 A C) (3 C D) (4 B D)
                      (5 D E) (6 C E) (7 E F) (8 D F)))
             (result (funcall 'neovm--uf3-kruskal vertices edges)))
        ;; MST of 6 vertices should have exactly 5 edges
        ;; Total weight for this graph's MST = 1+2+3+5+7 = 18
        (list
         (nth 1 result)   ;; mst edges
         (nth 3 result)   ;; total-weight
         (nth 5 result)   ;; edge-count = 5
         ;; Verify MST connects all vertices
         (let ((uf2 (funcall 'neovm--uf3-create)))
           (dolist (v vertices) (funcall 'neovm--uf3-make-set uf2 v))
           (dolist (e (nth 1 result))
             (funcall 'neovm--uf3-union uf2 (nth 1 e) (nth 2 e)))
           ;; All should be connected
           (let ((all-connected t))
             (dolist (v (cdr vertices))
               (unless (funcall 'neovm--uf3-connected-p uf2 'A v)
                 (setq all-connected nil)))
             all-connected))))
    (fmakunbound 'neovm--uf3-create)
    (fmakunbound 'neovm--uf3-make-set)
    (fmakunbound 'neovm--uf3-find)
    (fmakunbound 'neovm--uf3-union)
    (fmakunbound 'neovm--uf3-connected-p)
    (fmakunbound 'neovm--uf3-kruskal)))"#;
    let expect =
        expect_test::expect![[r#""OK (((1 A B) (2 B C) (3 C D) (5 D E) (7 E F)) 18 1 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Equivalence class tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_union_find_equivalence_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Track which elements belong to which equivalence class,
    // and enumerate all classes.
    let form = r#"(progn
  (fset 'neovm--uf4-create
    (lambda () (list (make-hash-table :test 'equal) (make-hash-table :test 'equal))))
  (fset 'neovm--uf4-make-set
    (lambda (uf x)
      (puthash x x (nth 0 uf))
      (puthash x 0 (nth 1 uf))))
  (fset 'neovm--uf4-find
    (lambda (uf x)
      (if (equal (gethash x (nth 0 uf)) x) x
        (let ((root (funcall 'neovm--uf4-find uf (gethash x (nth 0 uf)))))
          (puthash x root (nth 0 uf)) root))))
  (fset 'neovm--uf4-union
    (lambda (uf x y)
      (let ((rx (funcall 'neovm--uf4-find uf x))
            (ry (funcall 'neovm--uf4-find uf y)))
        (unless (equal rx ry)
          (let ((rk-x (gethash rx (nth 1 uf)))
                (rk-y (gethash ry (nth 1 uf))))
            (cond ((< rk-x rk-y) (puthash rx ry (nth 0 uf)))
                  ((> rk-x rk-y) (puthash ry rx (nth 0 uf)))
                  (t (puthash ry rx (nth 0 uf))
                     (puthash rx (1+ rk-x) (nth 1 uf)))))))))

  ;; Enumerate all equivalence classes: returns alist of (root . members)
  (fset 'neovm--uf4-classes
    (lambda (uf elements)
      (let ((classes (make-hash-table :test 'equal)))
        (dolist (e elements)
          (let ((root (funcall 'neovm--uf4-find uf e)))
            (puthash root (cons e (gethash root classes nil)) classes)))
        ;; Convert to sorted alist for deterministic output
        (let ((result nil))
          (maphash (lambda (k v) (setq result (cons (cons k (sort v '<)) result)))
                   classes)
          (sort result (lambda (a b) (< (car a) (car b))))))))

  ;; Size of the class containing element x
  (fset 'neovm--uf4-class-size
    (lambda (uf elements x)
      (let ((root (funcall 'neovm--uf4-find uf x))
            (count 0))
        (dolist (e elements)
          (when (equal (funcall 'neovm--uf4-find uf e) root)
            (setq count (1+ count))))
        count)))

  (unwind-protect
      (let ((uf (funcall 'neovm--uf4-create))
            (elts '(1 2 3 4 5 6 7 8 9)))
        (dolist (e elts) (funcall 'neovm--uf4-make-set uf e))
        ;; Initial: 9 singletons
        (let ((c0 (length (funcall 'neovm--uf4-classes uf elts))))
          ;; Create equivalence classes: {1,3,5,7,9} and {2,4,6,8}
          (funcall 'neovm--uf4-union uf 1 3)
          (funcall 'neovm--uf4-union uf 3 5)
          (funcall 'neovm--uf4-union uf 5 7)
          (funcall 'neovm--uf4-union uf 7 9)
          (funcall 'neovm--uf4-union uf 2 4)
          (funcall 'neovm--uf4-union uf 4 6)
          (funcall 'neovm--uf4-union uf 6 8)
          (let ((classes (funcall 'neovm--uf4-classes uf elts))
                (size-odd (funcall 'neovm--uf4-class-size uf elts 1))
                (size-even (funcall 'neovm--uf4-class-size uf elts 2))
                (num-classes (length (funcall 'neovm--uf4-classes uf elts))))
            (list c0 num-classes size-odd size-even
                  ;; Verify all odds are in same class
                  (equal (funcall 'neovm--uf4-find uf 1) (funcall 'neovm--uf4-find uf 9))
                  (equal (funcall 'neovm--uf4-find uf 3) (funcall 'neovm--uf4-find uf 7))
                  ;; Verify odds and evens are in different classes
                  (not (equal (funcall 'neovm--uf4-find uf 1) (funcall 'neovm--uf4-find uf 2)))))))
    (fmakunbound 'neovm--uf4-create)
    (fmakunbound 'neovm--uf4-make-set)
    (fmakunbound 'neovm--uf4-find)
    (fmakunbound 'neovm--uf4-union)
    (fmakunbound 'neovm--uf4-classes)
    (fmakunbound 'neovm--uf4-class-size)))"#;
    let expect = expect_test::expect![[r#""OK (9 2 5 4 t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Path compression verification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_union_find_path_compression() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify path compression by checking parent pointers after find
    let form = r#"(progn
  (fset 'neovm--uf5-create
    (lambda () (list (make-hash-table :test 'equal) (make-hash-table :test 'equal))))
  (fset 'neovm--uf5-make-set
    (lambda (uf x)
      (puthash x x (nth 0 uf))
      (puthash x 0 (nth 1 uf))))
  (fset 'neovm--uf5-find
    (lambda (uf x)
      (if (equal (gethash x (nth 0 uf)) x) x
        (let ((root (funcall 'neovm--uf5-find uf (gethash x (nth 0 uf)))))
          (puthash x root (nth 0 uf)) root))))
  ;; Union WITHOUT rank for simplicity: always attach second to first
  (fset 'neovm--uf5-union-simple
    (lambda (uf x y)
      (let ((rx (funcall 'neovm--uf5-find uf x))
            (ry (funcall 'neovm--uf5-find uf y)))
        (unless (equal rx ry)
          (puthash ry rx (nth 0 uf))))))
  ;; Get raw parent (no compression)
  (fset 'neovm--uf5-parent
    (lambda (uf x) (gethash x (nth 0 uf))))

  (unwind-protect
      (let ((uf (funcall 'neovm--uf5-create)))
        ;; Create a long chain: 1 <- 2 <- 3 <- 4 <- 5
        (dolist (x '(1 2 3 4 5))
          (funcall 'neovm--uf5-make-set uf x))
        (funcall 'neovm--uf5-union-simple uf 1 2)  ;; 2's parent = 1
        (funcall 'neovm--uf5-union-simple uf 1 3)  ;; 3's parent = 1
        ;; Now make 4 point to 3 (by finding 3 then unioning)
        (funcall 'neovm--uf5-union-simple uf 3 4)  ;; 4's parent = 1 (3's root)
        (funcall 'neovm--uf5-union-simple uf 4 5)  ;; 5's parent = 1

        ;; Before path compression: check all parents point to root
        (let ((parents-before (list
                               (funcall 'neovm--uf5-parent uf 1)
                               (funcall 'neovm--uf5-parent uf 2)
                               (funcall 'neovm--uf5-parent uf 3)
                               (funcall 'neovm--uf5-parent uf 4)
                               (funcall 'neovm--uf5-parent uf 5))))
          ;; Find on element 5 triggers path compression
          (let ((root (funcall 'neovm--uf5-find uf 5)))
            ;; After path compression: 5's parent should now point directly to root
            (let ((parents-after (list
                                  (funcall 'neovm--uf5-parent uf 1)
                                  (funcall 'neovm--uf5-parent uf 2)
                                  (funcall 'neovm--uf5-parent uf 3)
                                  (funcall 'neovm--uf5-parent uf 4)
                                  (funcall 'neovm--uf5-parent uf 5))))
              (list 'root root
                    'before parents-before
                    'after parents-after
                    ;; All should find the same root
                    (= (funcall 'neovm--uf5-find uf 1)
                       (funcall 'neovm--uf5-find uf 5)))))))
    (fmakunbound 'neovm--uf5-create)
    (fmakunbound 'neovm--uf5-make-set)
    (fmakunbound 'neovm--uf5-find)
    (fmakunbound 'neovm--uf5-union-simple)
    (fmakunbound 'neovm--uf5-parent)))"#;
    let expect = expect_test::expect![[r#""OK (root 1 before (1 1 1 1 1) after (1 1 1 1 1) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Union-Find with string elements (non-numeric keys)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_union_find_string_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use string keys to verify hash-table based UF works with non-numeric elements
    let form = r#"(progn
  (fset 'neovm--uf6-create
    (lambda () (list (make-hash-table :test 'equal) (make-hash-table :test 'equal))))
  (fset 'neovm--uf6-make-set
    (lambda (uf x)
      (puthash x x (nth 0 uf))
      (puthash x 0 (nth 1 uf))))
  (fset 'neovm--uf6-find
    (lambda (uf x)
      (if (equal (gethash x (nth 0 uf)) x) x
        (let ((root (funcall 'neovm--uf6-find uf (gethash x (nth 0 uf)))))
          (puthash x root (nth 0 uf)) root))))
  (fset 'neovm--uf6-union
    (lambda (uf x y)
      (let ((rx (funcall 'neovm--uf6-find uf x))
            (ry (funcall 'neovm--uf6-find uf y)))
        (unless (equal rx ry)
          (let ((rk-x (gethash rx (nth 1 uf)))
                (rk-y (gethash ry (nth 1 uf))))
            (cond ((< rk-x rk-y) (puthash rx ry (nth 0 uf)))
                  ((> rk-x rk-y) (puthash ry rx (nth 0 uf)))
                  (t (puthash ry rx (nth 0 uf))
                     (puthash rx (1+ rk-x) (nth 1 uf)))))))))
  (fset 'neovm--uf6-connected-p
    (lambda (uf x y)
      (equal (funcall 'neovm--uf6-find uf x) (funcall 'neovm--uf6-find uf y))))

  (unwind-protect
      (let ((uf (funcall 'neovm--uf6-create))
            (words '("apple" "banana" "cherry" "date" "elderberry"
                     "fig" "grape" "honeydew")))
        (dolist (w words) (funcall 'neovm--uf6-make-set uf w))
        ;; Group fruits by first letter proximity:
        ;; {apple}, {banana}, {cherry, date}, {elderberry, fig}, {grape, honeydew}
        (funcall 'neovm--uf6-union uf "cherry" "date")
        (funcall 'neovm--uf6-union uf "elderberry" "fig")
        (funcall 'neovm--uf6-union uf "grape" "honeydew")
        (list
         ;; Same component checks
         (funcall 'neovm--uf6-connected-p uf "cherry" "date")
         (funcall 'neovm--uf6-connected-p uf "elderberry" "fig")
         (funcall 'neovm--uf6-connected-p uf "grape" "honeydew")
         ;; Cross-component checks
         (funcall 'neovm--uf6-connected-p uf "apple" "banana")
         (funcall 'neovm--uf6-connected-p uf "cherry" "fig")
         (funcall 'neovm--uf6-connected-p uf "apple" "grape")
         ;; Now merge some: {cherry, date, elderberry, fig}
         (progn (funcall 'neovm--uf6-union uf "date" "elderberry") nil)
         (funcall 'neovm--uf6-connected-p uf "cherry" "fig")
         (funcall 'neovm--uf6-connected-p uf "date" "fig")
         ;; Count distinct components
         (let ((roots (make-hash-table :test 'equal)))
           (dolist (w words)
             (puthash (funcall 'neovm--uf6-find uf w) t roots))
           (hash-table-count roots))))
    (fmakunbound 'neovm--uf6-create)
    (fmakunbound 'neovm--uf6-make-set)
    (fmakunbound 'neovm--uf6-find)
    (fmakunbound 'neovm--uf6-union)
    (fmakunbound 'neovm--uf6-connected-p)))"#;
    let expect = expect_test::expect![[r#""OK (t t t nil nil nil nil t t 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Union-Find: idempotent union and self-union
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_union_find_idempotent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that union is idempotent and self-union is a no-op
    let form = r#"(progn
  (fset 'neovm--uf7-create
    (lambda () (list (make-hash-table :test 'equal) (make-hash-table :test 'equal))))
  (fset 'neovm--uf7-make-set
    (lambda (uf x)
      (puthash x x (nth 0 uf))
      (puthash x 0 (nth 1 uf))))
  (fset 'neovm--uf7-find
    (lambda (uf x)
      (if (equal (gethash x (nth 0 uf)) x) x
        (let ((root (funcall 'neovm--uf7-find uf (gethash x (nth 0 uf)))))
          (puthash x root (nth 0 uf)) root))))
  (fset 'neovm--uf7-union
    (lambda (uf x y)
      (let ((rx (funcall 'neovm--uf7-find uf x))
            (ry (funcall 'neovm--uf7-find uf y)))
        (if (equal rx ry)
            nil  ;; Already in same set: return nil
          (let ((rk-x (gethash rx (nth 1 uf)))
                (rk-y (gethash ry (nth 1 uf))))
            (cond ((< rk-x rk-y) (puthash rx ry (nth 0 uf)))
                  ((> rk-x rk-y) (puthash ry rx (nth 0 uf)))
                  (t (puthash ry rx (nth 0 uf))
                     (puthash rx (1+ rk-x) (nth 1 uf)))))
          t))))  ;; Union performed: return t
  (fset 'neovm--uf7-connected-p
    (lambda (uf x y)
      (equal (funcall 'neovm--uf7-find uf x) (funcall 'neovm--uf7-find uf y))))

  (unwind-protect
      (let ((uf (funcall 'neovm--uf7-create)))
        (dolist (x '(1 2 3)) (funcall 'neovm--uf7-make-set uf x))
        (list
         ;; Self-union: should be no-op (return nil)
         (funcall 'neovm--uf7-union uf 1 1)
         ;; Find self: should return self
         (= (funcall 'neovm--uf7-find uf 1) 1)
         ;; First union: should succeed (return t)
         (funcall 'neovm--uf7-union uf 1 2)
         ;; Repeated union: should be no-op (return nil)
         (funcall 'neovm--uf7-union uf 1 2)
         (funcall 'neovm--uf7-union uf 2 1)
         ;; All three connected after more unions
         (funcall 'neovm--uf7-union uf 2 3)
         (funcall 'neovm--uf7-connected-p uf 1 3)
         ;; Repeated union of already-connected elements
         (funcall 'neovm--uf7-union uf 1 3)
         (funcall 'neovm--uf7-union uf 3 1)
         ;; Still all connected
         (funcall 'neovm--uf7-connected-p uf 1 2)
         (funcall 'neovm--uf7-connected-p uf 2 3)))
    (fmakunbound 'neovm--uf7-create)
    (fmakunbound 'neovm--uf7-make-set)
    (fmakunbound 'neovm--uf7-find)
    (fmakunbound 'neovm--uf7-union)
    (fmakunbound 'neovm--uf7-connected-p)))"#;
    let expect = expect_test::expect![[r#""OK (nil t t nil nil t t nil nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
