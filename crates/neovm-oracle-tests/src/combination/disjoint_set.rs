//! Oracle parity tests for a disjoint set / union-find data structure with
//! path compression and union by rank, implemented in Elisp. Tests cover
//! make-set/find/union operations, path compression verification, union by
//! rank behavior, connected component counting, Kruskal's MST, and
//! equivalence class enumeration.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Core operations: make-set, find with path compression, union by rank
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_disjoint_set_core_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full disjoint set with path compression and union by rank.
    // Verify basic operations: make-set creates singletons, find returns
    // representative, union merges sets.
    let form = r#"(progn
  ;; DS structure: (parent-ht . rank-ht)
  (fset 'neovm--ds-create
    (lambda ()
      (cons (make-hash-table :test 'equal)
            (make-hash-table :test 'equal))))

  (fset 'neovm--ds-make-set
    (lambda (ds x)
      (puthash x x (car ds))
      (puthash x 0 (cdr ds))
      x))

  ;; Find with path compression (iterative to avoid deep recursion)
  (fset 'neovm--ds-find
    (lambda (ds x)
      (let ((parent (car ds))
            (root x))
        ;; Phase 1: find root
        (while (not (equal (gethash root parent) root))
          (setq root (gethash root parent)))
        ;; Phase 2: path compression - point everything to root
        (let ((curr x))
          (while (not (equal curr root))
            (let ((next (gethash curr parent)))
              (puthash curr root parent)
              (setq curr next))))
        root)))

  ;; Union by rank
  (fset 'neovm--ds-union
    (lambda (ds x y)
      (let ((rx (funcall 'neovm--ds-find ds x))
            (ry (funcall 'neovm--ds-find ds y)))
        (unless (equal rx ry)
          (let ((rank-x (gethash rx (cdr ds)))
                (rank-y (gethash ry (cdr ds))))
            (cond
             ((< rank-x rank-y)
              (puthash rx ry (car ds)))
             ((> rank-x rank-y)
              (puthash ry rx (car ds)))
             (t
              (puthash ry rx (car ds))
              (puthash rx (1+ rank-x) (cdr ds)))))
          t))))

  (fset 'neovm--ds-same-set-p
    (lambda (ds x y)
      (equal (funcall 'neovm--ds-find ds x)
             (funcall 'neovm--ds-find ds y))))

  (unwind-protect
      (let ((ds (funcall 'neovm--ds-create)))
        ;; Create 12 singletons
        (dolist (x '(1 2 3 4 5 6 7 8 9 10 11 12))
          (funcall 'neovm--ds-make-set ds x))

        ;; Initially all disjoint
        (let ((init-checks
               (list (funcall 'neovm--ds-same-set-p ds 1 2)
                     (funcall 'neovm--ds-same-set-p ds 5 6)
                     (funcall 'neovm--ds-same-set-p ds 1 1))))

          ;; Build groups: {1,2,3,4}, {5,6,7,8}, {9,10,11,12}
          (funcall 'neovm--ds-union ds 1 2)
          (funcall 'neovm--ds-union ds 3 4)
          (funcall 'neovm--ds-union ds 1 3)
          (funcall 'neovm--ds-union ds 5 6)
          (funcall 'neovm--ds-union ds 7 8)
          (funcall 'neovm--ds-union ds 5 7)
          (funcall 'neovm--ds-union ds 9 10)
          (funcall 'neovm--ds-union ds 11 12)
          (funcall 'neovm--ds-union ds 9 11)

          ;; Within-group connectivity
          (let ((within
                 (list (funcall 'neovm--ds-same-set-p ds 1 4)
                       (funcall 'neovm--ds-same-set-p ds 2 3)
                       (funcall 'neovm--ds-same-set-p ds 5 8)
                       (funcall 'neovm--ds-same-set-p ds 6 7)
                       (funcall 'neovm--ds-same-set-p ds 9 12)
                       (funcall 'neovm--ds-same-set-p ds 10 11))))

            ;; Cross-group: should be disjoint
            (let ((across
                   (list (funcall 'neovm--ds-same-set-p ds 1 5)
                         (funcall 'neovm--ds-same-set-p ds 1 9)
                         (funcall 'neovm--ds-same-set-p ds 5 9)
                         (funcall 'neovm--ds-same-set-p ds 4 8)
                         (funcall 'neovm--ds-same-set-p ds 8 12))))

              ;; Merge first two groups: {1..8}
              (funcall 'neovm--ds-union ds 4 5)
              (let ((merged
                     (list (funcall 'neovm--ds-same-set-p ds 1 8)
                           (funcall 'neovm--ds-same-set-p ds 3 6)
                           ;; Still separate from third group
                           (funcall 'neovm--ds-same-set-p ds 1 9))))

                ;; Merge all: {1..12}
                (funcall 'neovm--ds-union ds 8 9)
                (let ((all-connected
                       (list (funcall 'neovm--ds-same-set-p ds 1 12)
                             (funcall 'neovm--ds-same-set-p ds 6 10)
                             (funcall 'neovm--ds-same-set-p ds 2 11))))
                  (list 'init init-checks
                        'within within
                        'across across
                        'merged merged
                        'all all-connected)))))))
    (fmakunbound 'neovm--ds-create)
    (fmakunbound 'neovm--ds-make-set)
    (fmakunbound 'neovm--ds-find)
    (fmakunbound 'neovm--ds-union)
    (fmakunbound 'neovm--ds-same-set-p)))"#;
    let expect = expect_test::expect![[
        r#""OK (init (nil nil t) within (t t t t t t) across (nil nil nil nil nil) merged (t t nil) all (t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Path compression verification: check parent pointers flatten
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_disjoint_set_path_compression_verification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a long chain via targeted unions, then verify path compression
    // flattens the tree after find operations.
    let form = r#"(progn
  (fset 'neovm--ds2-create
    (lambda ()
      (cons (make-hash-table :test 'equal)
            (make-hash-table :test 'equal))))
  (fset 'neovm--ds2-make-set
    (lambda (ds x)
      (puthash x x (car ds))
      (puthash x 0 (cdr ds))))
  ;; Non-compressing find (for chain verification)
  (fset 'neovm--ds2-find-raw
    (lambda (ds x)
      (let ((parent (car ds)))
        (if (equal (gethash x parent) x) x
          (funcall 'neovm--ds2-find-raw ds (gethash x parent))))))
  ;; Compressing find
  (fset 'neovm--ds2-find
    (lambda (ds x)
      (let ((parent (car ds))
            (root x))
        (while (not (equal (gethash root parent) root))
          (setq root (gethash root parent)))
        (let ((curr x))
          (while (not (equal curr root))
            (let ((next (gethash curr parent)))
              (puthash curr root parent)
              (setq curr next))))
        root)))
  ;; Simple union: always attach y's root under x's root (no rank)
  (fset 'neovm--ds2-union-simple
    (lambda (ds x y)
      (let ((rx (funcall 'neovm--ds2-find-raw ds x))
            (ry (funcall 'neovm--ds2-find-raw ds y)))
        (unless (equal rx ry)
          (puthash ry rx (car ds))))))
  ;; Read raw parent
  (fset 'neovm--ds2-parent
    (lambda (ds x) (gethash x (car ds))))
  ;; Measure depth of x (number of hops to root via raw parents)
  (fset 'neovm--ds2-depth
    (lambda (ds x)
      (let ((d 0) (curr x) (parent (car ds)))
        (while (not (equal (gethash curr parent) curr))
          (setq d (1+ d))
          (setq curr (gethash curr parent)))
        d)))

  (unwind-protect
      (let ((ds (funcall 'neovm--ds2-create)))
        ;; Create chain: 1 <- 2 <- 3 <- 4 <- 5 <- 6 <- 7 <- 8
        (dolist (x '(1 2 3 4 5 6 7 8))
          (funcall 'neovm--ds2-make-set ds x))
        ;; Build chain by unioning each pair without rank
        (funcall 'neovm--ds2-union-simple ds 1 2)
        (funcall 'neovm--ds2-union-simple ds 1 3)
        (funcall 'neovm--ds2-union-simple ds 1 4)
        (funcall 'neovm--ds2-union-simple ds 1 5)
        (funcall 'neovm--ds2-union-simple ds 1 6)
        (funcall 'neovm--ds2-union-simple ds 1 7)
        (funcall 'neovm--ds2-union-simple ds 1 8)

        ;; Depths before compression
        (let ((depths-before
               (mapcar (lambda (x) (funcall 'neovm--ds2-depth ds x))
                       '(1 2 3 4 5 6 7 8))))

          ;; Now call compressing find on element 8
          (funcall 'neovm--ds2-find ds 8)

          ;; After compression, 8 should point directly to root
          (let ((parent-8-after (funcall 'neovm--ds2-parent ds 8))
                (depth-8-after (funcall 'neovm--ds2-depth ds 8)))

            ;; Compress all
            (dolist (x '(2 3 4 5 6 7))
              (funcall 'neovm--ds2-find ds x))

            ;; After full compression, all depths should be 0 or 1
            (let ((depths-after
                   (mapcar (lambda (x) (funcall 'neovm--ds2-depth ds x))
                           '(1 2 3 4 5 6 7 8))))
              ;; All should point to root (depth <= 1)
              (let ((all-flat (let ((r t))
                                (dolist (d depths-after)
                                  (when (> d 1) (setq r nil)))
                                r)))
                (list 'depths-before depths-before
                      'parent-8-after parent-8-after
                      'depth-8-after depth-8-after
                      'depths-after depths-after
                      'all-flat all-flat))))))
    (fmakunbound 'neovm--ds2-create)
    (fmakunbound 'neovm--ds2-make-set)
    (fmakunbound 'neovm--ds2-find-raw)
    (fmakunbound 'neovm--ds2-find)
    (fmakunbound 'neovm--ds2-union-simple)
    (fmakunbound 'neovm--ds2-parent)
    (fmakunbound 'neovm--ds2-depth)))"#;
    let expect = expect_test::expect![[
        r#""OK (depths-before (0 1 1 1 1 1 1 1) parent-8-after 1 depth-8-after 1 depths-after (0 1 1 1 1 1 1 1) all-flat t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Union by rank: verify rank-based merge decisions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_disjoint_set_union_by_rank() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that union by rank keeps trees balanced by attaching
    // shorter tree under taller tree's root.
    let form = r#"(progn
  (fset 'neovm--ds3-create
    (lambda ()
      (cons (make-hash-table :test 'equal)
            (make-hash-table :test 'equal))))
  (fset 'neovm--ds3-make-set
    (lambda (ds x)
      (puthash x x (car ds))
      (puthash x 0 (cdr ds))))
  (fset 'neovm--ds3-find
    (lambda (ds x)
      (let ((parent (car ds)) (root x))
        (while (not (equal (gethash root parent) root))
          (setq root (gethash root parent)))
        (let ((curr x))
          (while (not (equal curr root))
            (let ((next (gethash curr parent)))
              (puthash curr root parent)
              (setq curr next))))
        root)))
  (fset 'neovm--ds3-union
    (lambda (ds x y)
      (let ((rx (funcall 'neovm--ds3-find ds x))
            (ry (funcall 'neovm--ds3-find ds y)))
        (if (equal rx ry) nil
          (let ((rk-x (gethash rx (cdr ds)))
                (rk-y (gethash ry (cdr ds))))
            (cond
             ((< rk-x rk-y) (puthash rx ry (car ds)))
             ((> rk-x rk-y) (puthash ry rx (car ds)))
             (t (puthash ry rx (car ds))
                (puthash rx (1+ rk-x) (cdr ds))))
            t)))))
  (fset 'neovm--ds3-rank
    (lambda (ds x) (gethash x (cdr ds))))
  (fset 'neovm--ds3-root
    (lambda (ds x) (funcall 'neovm--ds3-find ds x)))

  (unwind-protect
      (let ((ds (funcall 'neovm--ds3-create)))
        (dolist (x '(1 2 3 4 5 6 7 8))
          (funcall 'neovm--ds3-make-set ds x))

        ;; All ranks start at 0
        (let ((init-ranks (mapcar (lambda (x) (funcall 'neovm--ds3-rank ds x))
                                  '(1 2 3 4 5 6 7 8))))

          ;; Union {1,2}: equal rank, rank of root increases to 1
          (funcall 'neovm--ds3-union ds 1 2)
          (let ((root-12 (funcall 'neovm--ds3-root ds 1))
                (rank-after-12 (funcall 'neovm--ds3-rank ds
                                        (funcall 'neovm--ds3-root ds 1))))

            ;; Union {3,4}: another pair, root gets rank 1
            (funcall 'neovm--ds3-union ds 3 4)

            ;; Union {1,2} with {3,4}: both roots rank 1, new root rank 2
            (funcall 'neovm--ds3-union ds 1 3)
            (let ((root-1234 (funcall 'neovm--ds3-root ds 1))
                  (rank-1234 (funcall 'neovm--ds3-rank ds
                                      (funcall 'neovm--ds3-root ds 1))))

              ;; Union {5,6}: rank 1
              (funcall 'neovm--ds3-union ds 5 6)
              ;; Union {5,6} with {1,2,3,4}: rank 1 under rank 2, rank stays 2
              (funcall 'neovm--ds3-union ds 5 1)
              (let ((rank-after-merge (funcall 'neovm--ds3-rank ds
                                               (funcall 'neovm--ds3-root ds 5))))
                (list 'init-ranks init-ranks
                      'root-12 root-12
                      'rank-after-12 rank-after-12
                      'root-1234 root-1234
                      'rank-1234 rank-1234
                      'rank-after-merge rank-after-merge
                      ;; Verify all 1-6 now connected
                      'connected (equal (funcall 'neovm--ds3-root ds 1)
                                        (funcall 'neovm--ds3-root ds 6))))))))
    (fmakunbound 'neovm--ds3-create)
    (fmakunbound 'neovm--ds3-make-set)
    (fmakunbound 'neovm--ds3-find)
    (fmakunbound 'neovm--ds3-union)
    (fmakunbound 'neovm--ds3-rank)
    (fmakunbound 'neovm--ds3-root)))"#;
    let expect = expect_test::expect![[
        r#""OK (init-ranks (0 0 0 0 0 0 0 0) root-12 1 rank-after-12 1 root-1234 1 rank-1234 2 rank-after-merge 2 connected t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Connected components counting with dynamic insertions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_disjoint_set_component_counting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Track number of connected components as edges are added incrementally
    let form = r#"(progn
  (fset 'neovm--ds4-create
    (lambda ()
      (list (make-hash-table :test 'equal)
            (make-hash-table :test 'equal)
            0)))  ;; third element: component count
  (fset 'neovm--ds4-make-set
    (lambda (ds x)
      (puthash x x (nth 0 ds))
      (puthash x 0 (nth 1 ds))
      (setcar (nthcdr 2 ds) (1+ (nth 2 ds)))))
  (fset 'neovm--ds4-find
    (lambda (ds x)
      (let ((parent (nth 0 ds)) (root x))
        (while (not (equal (gethash root parent) root))
          (setq root (gethash root parent)))
        (let ((curr x))
          (while (not (equal curr root))
            (let ((next (gethash curr parent)))
              (puthash curr root parent)
              (setq curr next))))
        root)))
  (fset 'neovm--ds4-union
    (lambda (ds x y)
      (let ((rx (funcall 'neovm--ds4-find ds x))
            (ry (funcall 'neovm--ds4-find ds y)))
        (if (equal rx ry) nil
          (let ((rk-x (gethash rx (nth 1 ds)))
                (rk-y (gethash ry (nth 1 ds))))
            (cond
             ((< rk-x rk-y) (puthash rx ry (nth 0 ds)))
             ((> rk-x rk-y) (puthash ry rx (nth 0 ds)))
             (t (puthash ry rx (nth 0 ds))
                (puthash rx (1+ rk-x) (nth 1 ds)))))
          ;; Decrement component count
          (setcar (nthcdr 2 ds) (1- (nth 2 ds)))
          t))))
  (fset 'neovm--ds4-count
    (lambda (ds) (nth 2 ds)))

  (unwind-protect
      (let ((ds (funcall 'neovm--ds4-create)))
        ;; Add vertices one by one, track component count
        (let ((counts nil))
          (dolist (v '(a b c d e f g h))
            (funcall 'neovm--ds4-make-set ds v)
            (setq counts (cons (funcall 'neovm--ds4-count ds) counts)))
          (let ((initial-counts (nreverse counts)))
            ;; Now add edges and track count
            (setq counts nil)
            (dolist (edge '((a b) (c d) (e f) (g h)))
              (funcall 'neovm--ds4-union ds (car edge) (cadr edge))
              (setq counts (cons (funcall 'neovm--ds4-count ds) counts)))
            (let ((after-pairs (nreverse counts)))
              ;; Merge pairs: {a,b,c,d}, {e,f,g,h}
              (funcall 'neovm--ds4-union ds 'b 'c)
              (funcall 'neovm--ds4-union ds 'f 'g)
              (let ((after-quads (funcall 'neovm--ds4-count ds)))
                ;; Merge all into one component
                (funcall 'neovm--ds4-union ds 'd 'e)
                (let ((final (funcall 'neovm--ds4-count ds)))
                  ;; Redundant union: count stays same
                  (funcall 'neovm--ds4-union ds 'a 'h)
                  (let ((after-redundant (funcall 'neovm--ds4-count ds)))
                    (list 'initial initial-counts
                          'after-pairs after-pairs
                          'after-quads after-quads
                          'final final
                          'after-redundant after-redundant))))))))
    (fmakunbound 'neovm--ds4-create)
    (fmakunbound 'neovm--ds4-make-set)
    (fmakunbound 'neovm--ds4-find)
    (fmakunbound 'neovm--ds4-union)
    (fmakunbound 'neovm--ds4-count)))"#;
    let expect = expect_test::expect![[
        r#""OK (initial (1 2 3 4 5 6 7 8) after-pairs (7 6 5 4) after-quads 2 final 1 after-redundant 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Kruskal's MST using union-find
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_disjoint_set_kruskal_mst() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Kruskal's minimum spanning tree algorithm using disjoint set
    let form = r#"(progn
  (fset 'neovm--ds5-create
    (lambda ()
      (cons (make-hash-table :test 'equal)
            (make-hash-table :test 'equal))))
  (fset 'neovm--ds5-make-set
    (lambda (ds x)
      (puthash x x (car ds))
      (puthash x 0 (cdr ds))))
  (fset 'neovm--ds5-find
    (lambda (ds x)
      (let ((parent (car ds)) (root x))
        (while (not (equal (gethash root parent) root))
          (setq root (gethash root parent)))
        (let ((curr x))
          (while (not (equal curr root))
            (let ((next (gethash curr parent)))
              (puthash curr root parent)
              (setq curr next))))
        root)))
  (fset 'neovm--ds5-union
    (lambda (ds x y)
      (let ((rx (funcall 'neovm--ds5-find ds x))
            (ry (funcall 'neovm--ds5-find ds y)))
        (if (equal rx ry) nil
          (let ((rk-x (gethash rx (cdr ds)))
                (rk-y (gethash ry (cdr ds))))
            (cond
             ((< rk-x rk-y) (puthash rx ry (car ds)))
             ((> rk-x rk-y) (puthash ry rx (car ds)))
             (t (puthash ry rx (car ds))
                (puthash rx (1+ rk-x) (cdr ds)))))
          t))))
  (fset 'neovm--ds5-same-p
    (lambda (ds x y)
      (equal (funcall 'neovm--ds5-find ds x)
             (funcall 'neovm--ds5-find ds y))))

  ;; Insertion sort for edge list (sort by weight)
  (fset 'neovm--ds5-sort-edges
    (lambda (edges)
      (sort (copy-sequence edges)
            (lambda (a b) (< (car a) (car b))))))

  ;; Kruskal's MST
  (fset 'neovm--ds5-kruskal
    (lambda (vertices edges)
      (let ((ds (funcall 'neovm--ds5-create))
            (sorted (funcall 'neovm--ds5-sort-edges edges))
            (mst nil)
            (total 0))
        (dolist (v vertices)
          (funcall 'neovm--ds5-make-set ds v))
        (dolist (e sorted)
          (let ((w (nth 0 e)) (u (nth 1 e)) (v (nth 2 e)))
            (unless (funcall 'neovm--ds5-same-p ds u v)
              (funcall 'neovm--ds5-union ds u v)
              (setq mst (cons e mst))
              (setq total (+ total w)))))
        (list (nreverse mst) total (length mst)))))

  (unwind-protect
      (list
       ;; Graph 1: Simple triangle with extra edges
       ;; Vertices: A, B, C, D
       ;; Edges (weight, from, to): sorted by weight
       (let ((result (funcall 'neovm--ds5-kruskal
                              '(A B C D)
                              '((1 A B) (2 B C) (3 A C) (4 C D) (5 A D) (6 B D)))))
         (list 'mst-edges (nth 0 result)
               'total-weight (nth 1 result)
               'num-edges (nth 2 result)))

       ;; Graph 2: 7 vertices, more complex
       (let ((result (funcall 'neovm--ds5-kruskal
                              '(1 2 3 4 5 6 7)
                              '((1 1 2) (2 2 3) (3 1 3)
                                (1 4 5) (3 5 6) (2 4 6)
                                (4 3 4) (5 6 7) (3 3 7)
                                (6 1 7)))))
         (list 'total (nth 1 result)
               'edges (nth 2 result)))

       ;; Graph 3: Already a tree (MST = input)
       (let ((result (funcall 'neovm--ds5-kruskal
                              '(X Y Z W)
                              '((1 X Y) (2 Y Z) (3 Z W)))))
         (list 'total (nth 1 result)
               'is-full (= (nth 2 result) 3)))

       ;; Graph 4: disconnected - MST is actually a forest
       (let ((result (funcall 'neovm--ds5-kruskal
                              '(A B C D E F)
                              '((1 A B) (2 B C) (1 D E) (2 E F)))))
         (list 'total (nth 1 result)
               'edges (nth 2 result)
               'not-spanning (< (nth 2 result) 5))))
    (fmakunbound 'neovm--ds5-create)
    (fmakunbound 'neovm--ds5-make-set)
    (fmakunbound 'neovm--ds5-find)
    (fmakunbound 'neovm--ds5-union)
    (fmakunbound 'neovm--ds5-same-p)
    (fmakunbound 'neovm--ds5-sort-edges)
    (fmakunbound 'neovm--ds5-kruskal)))"#;
    let expect = expect_test::expect![[
        r#""OK ((mst-edges ((1 A B) (2 B C) (4 C D)) total-weight 7 num-edges 1) (total 13 edges 1) (total 6 is-full nil) (total 6 edges 1 not-spanning t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Equivalence class enumeration
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_disjoint_set_equivalence_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Enumerate all equivalence classes and their members
    let form = r#"(progn
  (fset 'neovm--ds6-create
    (lambda ()
      (cons (make-hash-table :test 'equal)
            (make-hash-table :test 'equal))))
  (fset 'neovm--ds6-make-set
    (lambda (ds x)
      (puthash x x (car ds))
      (puthash x 0 (cdr ds))))
  (fset 'neovm--ds6-find
    (lambda (ds x)
      (let ((parent (car ds)) (root x))
        (while (not (equal (gethash root parent) root))
          (setq root (gethash root parent)))
        (let ((curr x))
          (while (not (equal curr root))
            (let ((next (gethash curr parent)))
              (puthash curr root parent)
              (setq curr next))))
        root)))
  (fset 'neovm--ds6-union
    (lambda (ds x y)
      (let ((rx (funcall 'neovm--ds6-find ds x))
            (ry (funcall 'neovm--ds6-find ds y)))
        (unless (equal rx ry)
          (let ((rk-x (gethash rx (cdr ds)))
                (rk-y (gethash ry (cdr ds))))
            (cond
             ((< rk-x rk-y) (puthash rx ry (car ds)))
             ((> rk-x rk-y) (puthash ry rx (car ds)))
             (t (puthash ry rx (car ds))
                (puthash rx (1+ rk-x) (cdr ds)))))))))

  ;; Get sorted list of equivalence classes as (root . sorted-members)
  (fset 'neovm--ds6-classes
    (lambda (ds elements)
      (let ((groups (make-hash-table :test 'equal)))
        (dolist (e elements)
          (let ((root (funcall 'neovm--ds6-find ds e)))
            (puthash root (cons e (gethash root groups nil)) groups)))
        ;; Convert to sorted list of sorted member lists
        (let ((result nil))
          (maphash (lambda (_k v)
                     (setq result (cons (sort (copy-sequence v) '<) result)))
                   groups)
          ;; Sort classes by smallest element
          (sort result (lambda (a b) (< (car a) (car b))))))))

  ;; Size of class containing x
  (fset 'neovm--ds6-class-size
    (lambda (ds elements x)
      (let ((rx (funcall 'neovm--ds6-find ds x))
            (count 0))
        (dolist (e elements)
          (when (equal (funcall 'neovm--ds6-find ds e) rx)
            (setq count (1+ count))))
        count)))

  (unwind-protect
      (let ((ds (funcall 'neovm--ds6-create))
            (elts '(1 2 3 4 5 6 7 8 9 10 11 12)))
        (dolist (e elts) (funcall 'neovm--ds6-make-set ds e))

        ;; Build equivalence classes:
        ;; Multiples of 3: {3, 6, 9, 12}
        ;; Multiples of 4 (not already in a class): {4, 8}
        ;; Primes: {2, 5, 7, 11}
        ;; Remaining: {1, 10}
        (funcall 'neovm--ds6-union ds 3 6)
        (funcall 'neovm--ds6-union ds 6 9)
        (funcall 'neovm--ds6-union ds 9 12)
        (funcall 'neovm--ds6-union ds 4 8)
        (funcall 'neovm--ds6-union ds 2 5)
        (funcall 'neovm--ds6-union ds 5 7)
        (funcall 'neovm--ds6-union ds 7 11)
        (funcall 'neovm--ds6-union ds 1 10)

        (let ((classes (funcall 'neovm--ds6-classes ds elts))
              (size-3 (funcall 'neovm--ds6-class-size ds elts 3))
              (size-4 (funcall 'neovm--ds6-class-size ds elts 4))
              (size-2 (funcall 'neovm--ds6-class-size ds elts 2))
              (size-1 (funcall 'neovm--ds6-class-size ds elts 1)))
          (list 'classes classes
                'num-classes (length classes)
                'size-multiples-of-3 size-3
                'size-multiples-of-4 size-4
                'size-primes size-2
                'size-remaining size-1
                ;; Verify: total elements across all classes = 12
                'total (apply '+ (mapcar 'length classes)))))
    (fmakunbound 'neovm--ds6-create)
    (fmakunbound 'neovm--ds6-make-set)
    (fmakunbound 'neovm--ds6-find)
    (fmakunbound 'neovm--ds6-union)
    (fmakunbound 'neovm--ds6-classes)
    (fmakunbound 'neovm--ds6-class-size)))"#;
    let expect = expect_test::expect![[
        r#""OK (classes ((1 10) (2 5 7 11) (3 6 9 12) (4 8)) num-classes 4 size-multiples-of-3 4 size-multiples-of-4 2 size-primes 4 size-remaining 2 total 12)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Disjoint set with mixed key types (symbols and integers)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_disjoint_set_mixed_key_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use both symbols and integers as keys in the same disjoint set
    let form = r#"(progn
  (fset 'neovm--ds7-create
    (lambda ()
      (cons (make-hash-table :test 'equal)
            (make-hash-table :test 'equal))))
  (fset 'neovm--ds7-make-set
    (lambda (ds x)
      (puthash x x (car ds))
      (puthash x 0 (cdr ds))))
  (fset 'neovm--ds7-find
    (lambda (ds x)
      (let ((parent (car ds)) (root x))
        (while (not (equal (gethash root parent) root))
          (setq root (gethash root parent)))
        (let ((curr x))
          (while (not (equal curr root))
            (let ((next (gethash curr parent)))
              (puthash curr root parent)
              (setq curr next))))
        root)))
  (fset 'neovm--ds7-union
    (lambda (ds x y)
      (let ((rx (funcall 'neovm--ds7-find ds x))
            (ry (funcall 'neovm--ds7-find ds y)))
        (unless (equal rx ry)
          (let ((rk-x (gethash rx (cdr ds)))
                (rk-y (gethash ry (cdr ds))))
            (cond
             ((< rk-x rk-y) (puthash rx ry (car ds)))
             ((> rk-x rk-y) (puthash ry rx (car ds)))
             (t (puthash ry rx (car ds))
                (puthash rx (1+ rk-x) (cdr ds)))))
          t))))
  (fset 'neovm--ds7-same-p
    (lambda (ds x y)
      (equal (funcall 'neovm--ds7-find ds x)
             (funcall 'neovm--ds7-find ds y))))

  (unwind-protect
      (let ((ds (funcall 'neovm--ds7-create)))
        ;; Mix integers and symbols
        (dolist (x '(1 2 3 alpha beta gamma))
          (funcall 'neovm--ds7-make-set ds x))

        ;; Union integers with symbols
        (funcall 'neovm--ds7-union ds 1 'alpha)
        (funcall 'neovm--ds7-union ds 2 'beta)
        (funcall 'neovm--ds7-union ds 3 'gamma)

        (list
         ;; Verify integer-symbol pairs are connected
         (funcall 'neovm--ds7-same-p ds 1 'alpha)
         (funcall 'neovm--ds7-same-p ds 2 'beta)
         (funcall 'neovm--ds7-same-p ds 3 'gamma)
         ;; Cross-pairs not connected
         (funcall 'neovm--ds7-same-p ds 1 'beta)
         (funcall 'neovm--ds7-same-p ds 'alpha 'gamma)
         ;; Now merge: {1, alpha, 2, beta}
         (progn (funcall 'neovm--ds7-union ds 'alpha 'beta) nil)
         (funcall 'neovm--ds7-same-p ds 1 2)
         (funcall 'neovm--ds7-same-p ds 1 'beta)
         (funcall 'neovm--ds7-same-p ds 'alpha 2)
         ;; {3, gamma} still separate
         (funcall 'neovm--ds7-same-p ds 1 3)
         ;; Merge all
         (progn (funcall 'neovm--ds7-union ds 2 3) nil)
         (funcall 'neovm--ds7-same-p ds 1 'gamma)
         (funcall 'neovm--ds7-same-p ds 'alpha 3)))
    (fmakunbound 'neovm--ds7-create)
    (fmakunbound 'neovm--ds7-make-set)
    (fmakunbound 'neovm--ds7-find)
    (fmakunbound 'neovm--ds7-union)
    (fmakunbound 'neovm--ds7-same-p)))"#;
    let expect = expect_test::expect![[r#""OK (t t t nil nil nil t t t nil nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
