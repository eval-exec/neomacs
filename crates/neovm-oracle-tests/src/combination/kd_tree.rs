//! Oracle parity tests for a k-d tree implementation in Elisp.
//!
//! Tests 2D point insertion, nearest neighbor search, range queries
//! (rectangle), building balanced k-d trees from point sets, k nearest
//! neighbors, and tree depth/balance statistics.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// k-d tree: insertion and basic structure
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_kd_tree_insertion_and_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Node: (point left right split-dim)
  ;; point: (x y), split-dim: 0=x, 1=y
  (fset 'neovm--kd-make-node
    (lambda (point left right dim)
      (list point left right dim)))

  (fset 'neovm--kd-point (lambda (node) (nth 0 node)))
  (fset 'neovm--kd-left  (lambda (node) (nth 1 node)))
  (fset 'neovm--kd-right (lambda (node) (nth 2 node)))
  (fset 'neovm--kd-dim   (lambda (node) (nth 3 node)))

  (fset 'neovm--kd-insert
    (lambda (node point depth)
      "Insert POINT into k-d tree rooted at NODE. DEPTH tracks split dimension."
      (if (null node)
          (funcall 'neovm--kd-make-node point nil nil (% depth 2))
        (let* ((dim (% depth 2))
               (node-pt (funcall 'neovm--kd-point node))
               (cmp-val (nth dim point))
               (node-val (nth dim node-pt)))
          (if (< cmp-val node-val)
              (funcall 'neovm--kd-make-node
                       node-pt
                       (funcall 'neovm--kd-insert
                                (funcall 'neovm--kd-left node) point (1+ depth))
                       (funcall 'neovm--kd-right node)
                       dim)
            (funcall 'neovm--kd-make-node
                     node-pt
                     (funcall 'neovm--kd-left node)
                     (funcall 'neovm--kd-insert
                              (funcall 'neovm--kd-right node) point (1+ depth))
                     dim))))))

  ;; Build tree from list of points by sequential insertion
  (fset 'neovm--kd-build-sequential
    (lambda (points)
      (let ((tree nil))
        (dolist (pt points)
          (setq tree (funcall 'neovm--kd-insert tree pt 0)))
        tree)))

  ;; In-order traversal
  (fset 'neovm--kd-inorder
    (lambda (node)
      (if (null node) nil
        (append (funcall 'neovm--kd-inorder (funcall 'neovm--kd-left node))
                (list (funcall 'neovm--kd-point node))
                (funcall 'neovm--kd-inorder (funcall 'neovm--kd-right node))))))

  ;; Count nodes
  (fset 'neovm--kd-size
    (lambda (node)
      (if (null node) 0
        (+ 1
           (funcall 'neovm--kd-size (funcall 'neovm--kd-left node))
           (funcall 'neovm--kd-size (funcall 'neovm--kd-right node))))))

  (unwind-protect
      (let ((tree (funcall 'neovm--kd-build-sequential
                            '((7 2) (5 4) (9 6) (2 3) (4 7) (8 1)))))
        (list
         ;; Root point
         (funcall 'neovm--kd-point tree)
         ;; Size
         (funcall 'neovm--kd-size tree)
         ;; In-order traversal
         (funcall 'neovm--kd-inorder tree)
         ;; Root splits on x (dim=0)
         (funcall 'neovm--kd-dim tree)
         ;; Insert a new point and verify size
         (let ((tree2 (funcall 'neovm--kd-insert tree '(3 5) 0)))
           (list (funcall 'neovm--kd-size tree2)
                 (funcall 'neovm--kd-inorder tree2)))))
    (fmakunbound 'neovm--kd-make-node)
    (fmakunbound 'neovm--kd-point)
    (fmakunbound 'neovm--kd-left)
    (fmakunbound 'neovm--kd-right)
    (fmakunbound 'neovm--kd-dim)
    (fmakunbound 'neovm--kd-insert)
    (fmakunbound 'neovm--kd-build-sequential)
    (fmakunbound 'neovm--kd-inorder)
    (fmakunbound 'neovm--kd-size)))"#;
    let expect = expect_test::expect![[
        r#""OK ((7 2) 6 ((2 3) (5 4) (4 7) (7 2) (8 1) (9 6)) 0 (7 ((2 3) (5 4) (3 5) (4 7) (7 2) (8 1) (9 6))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// k-d tree: nearest neighbor search
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_kd_tree_nearest_neighbor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--kd-make-node
    (lambda (point left right dim) (list point left right dim)))
  (fset 'neovm--kd-point (lambda (n) (nth 0 n)))
  (fset 'neovm--kd-left  (lambda (n) (nth 1 n)))
  (fset 'neovm--kd-right (lambda (n) (nth 2 n)))
  (fset 'neovm--kd-dim   (lambda (n) (nth 3 n)))

  (fset 'neovm--kd-dist-sq
    (lambda (a b)
      (+ (* (- (car a) (car b)) (- (car a) (car b)))
         (* (- (cadr a) (cadr b)) (- (cadr a) (cadr b))))))

  (fset 'neovm--kd-insert
    (lambda (node point depth)
      (if (null node)
          (funcall 'neovm--kd-make-node point nil nil (% depth 2))
        (let* ((dim (% depth 2))
               (cmp (nth dim point))
               (nval (nth dim (funcall 'neovm--kd-point node))))
          (if (< cmp nval)
              (funcall 'neovm--kd-make-node
                       (funcall 'neovm--kd-point node)
                       (funcall 'neovm--kd-insert (funcall 'neovm--kd-left node) point (1+ depth))
                       (funcall 'neovm--kd-right node) dim)
            (funcall 'neovm--kd-make-node
                     (funcall 'neovm--kd-point node)
                     (funcall 'neovm--kd-left node)
                     (funcall 'neovm--kd-insert (funcall 'neovm--kd-right node) point (1+ depth))
                     dim))))))

  (fset 'neovm--kd-build
    (lambda (points)
      (let ((tree nil))
        (dolist (pt points)
          (setq tree (funcall 'neovm--kd-insert tree pt 0)))
        tree)))

  ;; Nearest neighbor search
  ;; Uses a mutable best via list wrapping: (best-point best-dist-sq)
  (fset 'neovm--kd-nn-helper
    (lambda (node target best depth)
      "Search for nearest neighbor. BEST is (point dist-sq) as a cons."
      (when node
        (let* ((pt (funcall 'neovm--kd-point node))
               (d (funcall 'neovm--kd-dist-sq target pt)))
          ;; Update best if closer
          (when (< d (cdr best))
            (setcar best pt)
            (setcdr best d))
          (let* ((dim (% depth 2))
                 (diff (- (nth dim target) (nth dim pt)))
                 (near (if (< diff 0) (funcall 'neovm--kd-left node) (funcall 'neovm--kd-right node)))
                 (far  (if (< diff 0) (funcall 'neovm--kd-right node) (funcall 'neovm--kd-left node))))
            ;; Search near side first
            (funcall 'neovm--kd-nn-helper near target best (1+ depth))
            ;; Search far side if splitting plane is closer than current best
            (when (< (* diff diff) (cdr best))
              (funcall 'neovm--kd-nn-helper far target best (1+ depth))))))))

  (fset 'neovm--kd-nearest
    (lambda (tree target)
      "Return (nearest-point squared-distance)."
      (let ((best (cons nil 999999999)))
        (funcall 'neovm--kd-nn-helper tree target best 0)
        best)))

  ;; Brute-force for verification
  (fset 'neovm--kd-brute-nearest
    (lambda (points target)
      (let ((best-pt nil) (best-d 999999999))
        (dolist (pt points)
          (let ((d (funcall 'neovm--kd-dist-sq target pt)))
            (when (< d best-d)
              (setq best-pt pt best-d d))))
        (cons best-pt best-d))))

  (unwind-protect
      (let* ((points '((2 3) (5 4) (9 6) (4 7) (8 1) (7 2)
                        (1 8) (6 5) (3 1) (0 0) (10 10) (5 5)))
             (tree (funcall 'neovm--kd-build points)))
        (let ((queries '((3 3) (0 0) (10 10) (5 5) (7 3) (1 1) (6 6) (9 9))))
          (list
           ;; For each query, compare kd-tree vs brute force
           (mapcar (lambda (q)
                     (let ((kd-res (funcall 'neovm--kd-nearest tree q))
                           (bf-res (funcall 'neovm--kd-brute-nearest points q)))
                       (list q
                             (car kd-res) (cdr kd-res)
                             (car bf-res) (cdr bf-res)
                             ;; Same distance (point might differ if equidistant)
                             (= (cdr kd-res) (cdr bf-res)))))
                   queries))))
    (fmakunbound 'neovm--kd-make-node)
    (fmakunbound 'neovm--kd-point)
    (fmakunbound 'neovm--kd-left)
    (fmakunbound 'neovm--kd-right)
    (fmakunbound 'neovm--kd-dim)
    (fmakunbound 'neovm--kd-dist-sq)
    (fmakunbound 'neovm--kd-insert)
    (fmakunbound 'neovm--kd-build)
    (fmakunbound 'neovm--kd-nn-helper)
    (fmakunbound 'neovm--kd-nearest)
    (fmakunbound 'neovm--kd-brute-nearest)))"#;
    let expect = expect_test::expect![[
        r#""OK ((((3 3) (2 3) 1 (2 3) 1 t) ((0 0) (0 0) 0 (0 0) 0 t) ((10 10) (10 10) 0 (10 10) 0 t) ((5 5) (5 5) 0 (5 5) 0 t) ((7 3) (7 2) 1 (7 2) 1 t) ((1 1) (0 0) 2 (0 0) 2 t) ((6 6) (6 5) 1 (6 5) 1 t) ((9 9) (10 10) 2 (10 10) 2 t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// k-d tree: range search (rectangle query)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_kd_tree_range_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--kd-make-node
    (lambda (pt l r d) (list pt l r d)))
  (fset 'neovm--kd-point (lambda (n) (nth 0 n)))
  (fset 'neovm--kd-left  (lambda (n) (nth 1 n)))
  (fset 'neovm--kd-right (lambda (n) (nth 2 n)))
  (fset 'neovm--kd-dim   (lambda (n) (nth 3 n)))

  (fset 'neovm--kd-insert
    (lambda (node pt depth)
      (if (null node)
          (funcall 'neovm--kd-make-node pt nil nil (% depth 2))
        (let* ((dim (% depth 2))
               (cv (nth dim pt))
               (nv (nth dim (funcall 'neovm--kd-point node))))
          (if (< cv nv)
              (funcall 'neovm--kd-make-node
                       (funcall 'neovm--kd-point node)
                       (funcall 'neovm--kd-insert (funcall 'neovm--kd-left node) pt (1+ depth))
                       (funcall 'neovm--kd-right node) dim)
            (funcall 'neovm--kd-make-node
                     (funcall 'neovm--kd-point node)
                     (funcall 'neovm--kd-left node)
                     (funcall 'neovm--kd-insert (funcall 'neovm--kd-right node) pt (1+ depth))
                     dim))))))

  (fset 'neovm--kd-build
    (lambda (points)
      (let ((tree nil))
        (dolist (pt points)
          (setq tree (funcall 'neovm--kd-insert tree pt 0)))
        tree)))

  ;; Range search: find all points within rectangle [x-lo, x-hi] x [y-lo, y-hi]
  (fset 'neovm--kd-range-search
    (lambda (node x-lo x-hi y-lo y-hi depth)
      (if (null node) nil
        (let* ((pt (funcall 'neovm--kd-point node))
               (dim (% depth 2))
               (split-val (nth dim pt))
               (lo (if (= dim 0) x-lo y-lo))
               (hi (if (= dim 0) x-hi y-hi))
               (result nil))
          ;; Check if current point is in range
          (when (and (<= x-lo (car pt)) (<= (car pt) x-hi)
                     (<= y-lo (cadr pt)) (<= (cadr pt) y-hi))
            (setq result (list pt)))
          ;; Search left subtree if range extends below split
          (when (<= lo split-val)
            (setq result
                  (append result
                          (funcall 'neovm--kd-range-search
                                   (funcall 'neovm--kd-left node)
                                   x-lo x-hi y-lo y-hi (1+ depth)))))
          ;; Search right subtree if range extends above split
          (when (>= hi split-val)
            (setq result
                  (append result
                          (funcall 'neovm--kd-range-search
                                   (funcall 'neovm--kd-right node)
                                   x-lo x-hi y-lo y-hi (1+ depth)))))
          result))))

  ;; Brute-force range search for verification
  (fset 'neovm--kd-brute-range
    (lambda (points x-lo x-hi y-lo y-hi)
      (seq-filter (lambda (pt)
                    (and (<= x-lo (car pt)) (<= (car pt) x-hi)
                         (<= y-lo (cadr pt)) (<= (cadr pt) y-hi)))
                  points)))

  (unwind-protect
      (let* ((points '((2 3) (5 4) (9 6) (4 7) (8 1) (7 2)
                        (1 8) (6 5) (3 1) (0 0) (10 10) (5 5)))
             (tree (funcall 'neovm--kd-build points)))
        (let ((queries '((0 5 0 5)    ;; lower-left quadrant
                          (5 10 5 10)  ;; upper-right quadrant
                          (3 7 2 6)    ;; middle rectangle
                          (0 10 0 10)  ;; everything
                          (4 4 4 4)    ;; single point query
                          (100 200 100 200)))) ;; empty result
          (list
           (mapcar (lambda (q)
                     (let* ((x-lo (nth 0 q)) (x-hi (nth 1 q))
                            (y-lo (nth 2 q)) (y-hi (nth 3 q))
                            (kd-res (sort (funcall 'neovm--kd-range-search
                                                    tree x-lo x-hi y-lo y-hi 0)
                                          (lambda (a b) (or (< (car a) (car b))
                                                            (and (= (car a) (car b))
                                                                 (< (cadr a) (cadr b)))))))
                            (bf-res (sort (funcall 'neovm--kd-brute-range
                                                    points x-lo x-hi y-lo y-hi)
                                          (lambda (a b) (or (< (car a) (car b))
                                                            (and (= (car a) (car b))
                                                                 (< (cadr a) (cadr b))))))))
                       (list q kd-res bf-res (equal kd-res bf-res))))
                   queries))))
    (fmakunbound 'neovm--kd-make-node)
    (fmakunbound 'neovm--kd-point)
    (fmakunbound 'neovm--kd-left)
    (fmakunbound 'neovm--kd-right)
    (fmakunbound 'neovm--kd-dim)
    (fmakunbound 'neovm--kd-insert)
    (fmakunbound 'neovm--kd-build)
    (fmakunbound 'neovm--kd-range-search)
    (fmakunbound 'neovm--kd-brute-range)))"#;
    let expect = expect_test::expect![[
        r#""OK ((((0 5 0 5) ((0 0) (2 3) (3 1) (5 4) (5 5)) ((0 0) (2 3) (3 1) (5 4) (5 5)) t) ((5 10 5 10) ((5 5) (6 5) (9 6) (10 10)) ((5 5) (6 5) (9 6) (10 10)) t) ((3 7 2 6) ((5 4) (5 5) (6 5) (7 2)) ((5 4) (5 5) (6 5) (7 2)) t) ((0 10 0 10) ((0 0) (1 8) (2 3) (3 1) (4 7) (5 4) (5 5) (6 5) (7 2) (8 1) (9 6) (10 10)) ((0 0) (1 8) (2 3) (3 1) (4 7) (5 4) (5 5) (6 5) (7 2) (8 1) (9 6) (10 10)) t) ((4 4 4 4) nil nil t) ((100 200 100 200) nil nil t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// k-d tree: balanced construction from point set (median split)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_kd_tree_balanced_construction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--kd-make-node
    (lambda (pt l r d) (list pt l r d)))
  (fset 'neovm--kd-point (lambda (n) (nth 0 n)))
  (fset 'neovm--kd-left  (lambda (n) (nth 1 n)))
  (fset 'neovm--kd-right (lambda (n) (nth 2 n)))
  (fset 'neovm--kd-dim   (lambda (n) (nth 3 n)))

  ;; Build balanced k-d tree by sorting and splitting at median
  (fset 'neovm--kd-build-balanced
    (lambda (points depth)
      (if (null points) nil
        (let* ((dim (% depth 2))
               (sorted (sort (copy-sequence points)
                             (lambda (a b) (< (nth dim a) (nth dim b)))))
               (mid (/ (length sorted) 2))
               (median-pt (nth mid sorted))
               (left-pts (take mid sorted))
               (right-pts (nthcdr (1+ mid) sorted)))
          (funcall 'neovm--kd-make-node
                   median-pt
                   (funcall 'neovm--kd-build-balanced left-pts (1+ depth))
                   (funcall 'neovm--kd-build-balanced right-pts (1+ depth))
                   dim)))))

  ;; Tree depth
  (fset 'neovm--kd-depth
    (lambda (node)
      (if (null node) 0
        (1+ (max (funcall 'neovm--kd-depth (funcall 'neovm--kd-left node))
                 (funcall 'neovm--kd-depth (funcall 'neovm--kd-right node)))))))

  ;; Size
  (fset 'neovm--kd-size
    (lambda (node)
      (if (null node) 0
        (+ 1
           (funcall 'neovm--kd-size (funcall 'neovm--kd-left node))
           (funcall 'neovm--kd-size (funcall 'neovm--kd-right node))))))

  ;; In-order traversal
  (fset 'neovm--kd-inorder
    (lambda (node)
      (if (null node) nil
        (append (funcall 'neovm--kd-inorder (funcall 'neovm--kd-left node))
                (list (funcall 'neovm--kd-point node))
                (funcall 'neovm--kd-inorder (funcall 'neovm--kd-right node))))))

  ;; Sequential insertion (for comparison)
  (fset 'neovm--kd-insert
    (lambda (node pt depth)
      (if (null node)
          (funcall 'neovm--kd-make-node pt nil nil (% depth 2))
        (let* ((dim (% depth 2))
               (cv (nth dim pt))
               (nv (nth dim (funcall 'neovm--kd-point node))))
          (if (< cv nv)
              (funcall 'neovm--kd-make-node
                       (funcall 'neovm--kd-point node)
                       (funcall 'neovm--kd-insert (funcall 'neovm--kd-left node) pt (1+ depth))
                       (funcall 'neovm--kd-right node) dim)
            (funcall 'neovm--kd-make-node
                     (funcall 'neovm--kd-point node)
                     (funcall 'neovm--kd-left node)
                     (funcall 'neovm--kd-insert (funcall 'neovm--kd-right node) pt (1+ depth))
                     dim))))))

  (fset 'neovm--kd-build-seq
    (lambda (points)
      (let ((tree nil))
        (dolist (pt points) (setq tree (funcall 'neovm--kd-insert tree pt 0)))
        tree)))

  (unwind-protect
      (let* ((points '((2 3) (5 4) (9 6) (4 7) (8 1) (7 2)
                        (1 8) (6 5) (3 1) (0 0) (10 10) (5 5)
                        (1 1) (9 9) (3 7) (7 3)))
             (balanced (funcall 'neovm--kd-build-balanced points 0))
             (sequential (funcall 'neovm--kd-build-seq points)))
        (list
         ;; Both contain same number of points
         (funcall 'neovm--kd-size balanced)
         (funcall 'neovm--kd-size sequential)
         ;; Balanced tree is shallower
         (funcall 'neovm--kd-depth balanced)
         (funcall 'neovm--kd-depth sequential)
         ;; Balanced tree depth is O(log n)
         (<= (funcall 'neovm--kd-depth balanced)
             (1+ (ceiling (log (length points) 2))))
         ;; Both contain the same set of points (sorted)
         (equal (sort (funcall 'neovm--kd-inorder balanced)
                      (lambda (a b) (or (< (car a) (car b))
                                        (and (= (car a) (car b))
                                             (< (cadr a) (cadr b))))))
                (sort (funcall 'neovm--kd-inorder sequential)
                      (lambda (a b) (or (< (car a) (car b))
                                        (and (= (car a) (car b))
                                             (< (cadr a) (cadr b)))))))
         ;; Root of balanced tree (should be median on x)
         (funcall 'neovm--kd-point balanced)))
    (fmakunbound 'neovm--kd-make-node)
    (fmakunbound 'neovm--kd-point)
    (fmakunbound 'neovm--kd-left)
    (fmakunbound 'neovm--kd-right)
    (fmakunbound 'neovm--kd-dim)
    (fmakunbound 'neovm--kd-build-balanced)
    (fmakunbound 'neovm--kd-depth)
    (fmakunbound 'neovm--kd-size)
    (fmakunbound 'neovm--kd-inorder)
    (fmakunbound 'neovm--kd-insert)
    (fmakunbound 'neovm--kd-build-seq)))"#;
    let expect = expect_test::expect![[r#""OK (16 16 5 6 t t (5 5))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// k-d tree: k nearest neighbors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_kd_tree_k_nearest_neighbors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--kd-make-node
    (lambda (pt l r d) (list pt l r d)))
  (fset 'neovm--kd-point (lambda (n) (nth 0 n)))
  (fset 'neovm--kd-left  (lambda (n) (nth 1 n)))
  (fset 'neovm--kd-right (lambda (n) (nth 2 n)))
  (fset 'neovm--kd-dim   (lambda (n) (nth 3 n)))

  (fset 'neovm--kd-dist-sq
    (lambda (a b)
      (+ (* (- (car a) (car b)) (- (car a) (car b)))
         (* (- (cadr a) (cadr b)) (- (cadr a) (cadr b))))))

  (fset 'neovm--kd-build-balanced
    (lambda (points depth)
      (if (null points) nil
        (let* ((dim (% depth 2))
               (sorted (sort (copy-sequence points)
                             (lambda (a b) (< (nth dim a) (nth dim b)))))
               (mid (/ (length sorted) 2))
               (median-pt (nth mid sorted))
               (left-pts (take mid sorted))
               (right-pts (nthcdr (1+ mid) sorted)))
          (funcall 'neovm--kd-make-node
                   median-pt
                   (funcall 'neovm--kd-build-balanced left-pts (1+ depth))
                   (funcall 'neovm--kd-build-balanced right-pts (1+ depth))
                   dim)))))

  ;; k-nearest neighbors: collect up to k closest points
  ;; heap is a sorted list of (dist-sq . point), max-first
  (fset 'neovm--kd-knn-helper
    (lambda (node target k heap depth)
      (when node
        (let* ((pt (funcall 'neovm--kd-point node))
               (d (funcall 'neovm--kd-dist-sq target pt)))
          ;; Add to heap if room or closer than farthest
          (when (or (< (length heap) k)
                    (< d (caar heap)))
            (setq heap (cons (cons d pt) heap))
            (setq heap (sort heap (lambda (a b) (> (car a) (car b)))))
            (when (> (length heap) k)
              (setq heap (cdr heap))))
          (let* ((dim (% depth 2))
                 (diff (- (nth dim target) (nth dim pt)))
                 (near (if (< diff 0) (funcall 'neovm--kd-left node) (funcall 'neovm--kd-right node)))
                 (far  (if (< diff 0) (funcall 'neovm--kd-right node) (funcall 'neovm--kd-left node))))
            (setq heap (funcall 'neovm--kd-knn-helper near target k heap (1+ depth)))
            (when (or (< (length heap) k)
                      (< (* diff diff) (caar heap)))
              (setq heap (funcall 'neovm--kd-knn-helper far target k heap (1+ depth)))))))
      heap))

  (fset 'neovm--kd-knn
    (lambda (tree target k)
      "Return k nearest points sorted by distance."
      (let ((heap (funcall 'neovm--kd-knn-helper tree target k nil 0)))
        (mapcar #'cdr (sort heap (lambda (a b) (< (car a) (car b))))))))

  ;; Brute force k-nn
  (fset 'neovm--kd-brute-knn
    (lambda (points target k)
      (let ((dists (mapcar (lambda (pt)
                             (cons (funcall 'neovm--kd-dist-sq target pt) pt))
                           points)))
        (setq dists (sort dists (lambda (a b) (< (car a) (car b)))))
        (mapcar #'cdr (take k dists)))))

  (unwind-protect
      (let* ((points '((2 3) (5 4) (9 6) (4 7) (8 1) (7 2)
                        (1 8) (6 5) (3 1) (0 0) (10 10) (5 5)))
             (tree (funcall 'neovm--kd-build-balanced points 0)))
        (list
         ;; k=1 nearest neighbor for (5 5)
         (funcall 'neovm--kd-knn tree '(5 5) 1)
         ;; k=3 nearest to (0 0)
         (let ((kd-res (funcall 'neovm--kd-knn tree '(0 0) 3))
               (bf-res (funcall 'neovm--kd-brute-knn points '(0 0) 3)))
           (list kd-res bf-res (equal kd-res bf-res)))
         ;; k=5 nearest to (5 4)
         (let ((kd-res (funcall 'neovm--kd-knn tree '(5 4) 5))
               (bf-res (funcall 'neovm--kd-brute-knn points '(5 4) 5)))
           (list kd-res bf-res (equal kd-res bf-res)))
         ;; k larger than number of points returns all points
         (length (funcall 'neovm--kd-knn tree '(0 0) 100))
         ;; k=0 returns empty
         (funcall 'neovm--kd-knn tree '(5 5) 0)
         ;; Verify all knn results match brute force
         (let ((all-ok t))
           (dolist (q '((3 3) (7 7) (0 10) (10 0)))
             (dolist (k '(1 2 4))
               (let ((kd-r (funcall 'neovm--kd-knn tree q k))
                     (bf-r (funcall 'neovm--kd-brute-knn points q k)))
                 (unless (equal kd-r bf-r)
                   (setq all-ok nil)))))
           all-ok)))
    (fmakunbound 'neovm--kd-make-node)
    (fmakunbound 'neovm--kd-point)
    (fmakunbound 'neovm--kd-left)
    (fmakunbound 'neovm--kd-right)
    (fmakunbound 'neovm--kd-dim)
    (fmakunbound 'neovm--kd-dist-sq)
    (fmakunbound 'neovm--kd-build-balanced)
    (fmakunbound 'neovm--kd-knn-helper)
    (fmakunbound 'neovm--kd-knn)
    (fmakunbound 'neovm--kd-brute-knn)))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// k-d tree: tree depth and balance statistics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_kd_tree_depth_balance_stats() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--kd-make-node
    (lambda (pt l r d) (list pt l r d)))
  (fset 'neovm--kd-point (lambda (n) (nth 0 n)))
  (fset 'neovm--kd-left  (lambda (n) (nth 1 n)))
  (fset 'neovm--kd-right (lambda (n) (nth 2 n)))
  (fset 'neovm--kd-dim   (lambda (n) (nth 3 n)))

  (fset 'neovm--kd-insert
    (lambda (node pt depth)
      (if (null node)
          (funcall 'neovm--kd-make-node pt nil nil (% depth 2))
        (let* ((dim (% depth 2))
               (cv (nth dim pt))
               (nv (nth dim (funcall 'neovm--kd-point node))))
          (if (< cv nv)
              (funcall 'neovm--kd-make-node
                       (funcall 'neovm--kd-point node)
                       (funcall 'neovm--kd-insert (funcall 'neovm--kd-left node) pt (1+ depth))
                       (funcall 'neovm--kd-right node) dim)
            (funcall 'neovm--kd-make-node
                     (funcall 'neovm--kd-point node)
                     (funcall 'neovm--kd-left node)
                     (funcall 'neovm--kd-insert (funcall 'neovm--kd-right node) pt (1+ depth))
                     dim))))))

  (fset 'neovm--kd-build-balanced
    (lambda (points depth)
      (if (null points) nil
        (let* ((dim (% depth 2))
               (sorted (sort (copy-sequence points)
                             (lambda (a b) (< (nth dim a) (nth dim b)))))
               (mid (/ (length sorted) 2))
               (median-pt (nth mid sorted))
               (left-pts (take mid sorted))
               (right-pts (nthcdr (1+ mid) sorted)))
          (funcall 'neovm--kd-make-node
                   median-pt
                   (funcall 'neovm--kd-build-balanced left-pts (1+ depth))
                   (funcall 'neovm--kd-build-balanced right-pts (1+ depth))
                   dim)))))

  (fset 'neovm--kd-depth
    (lambda (n) (if (null n) 0
                  (1+ (max (funcall 'neovm--kd-depth (funcall 'neovm--kd-left n))
                           (funcall 'neovm--kd-depth (funcall 'neovm--kd-right n)))))))

  (fset 'neovm--kd-min-depth
    (lambda (n) (if (null n) 0
                  (1+ (min (funcall 'neovm--kd-min-depth (funcall 'neovm--kd-left n))
                           (funcall 'neovm--kd-min-depth (funcall 'neovm--kd-right n)))))))

  (fset 'neovm--kd-size
    (lambda (n) (if (null n) 0
                  (+ 1 (funcall 'neovm--kd-size (funcall 'neovm--kd-left n))
                       (funcall 'neovm--kd-size (funcall 'neovm--kd-right n))))))

  ;; Count leaf nodes
  (fset 'neovm--kd-leaf-count
    (lambda (n)
      (cond ((null n) 0)
            ((and (null (funcall 'neovm--kd-left n))
                  (null (funcall 'neovm--kd-right n)))
             1)
            (t (+ (funcall 'neovm--kd-leaf-count (funcall 'neovm--kd-left n))
                  (funcall 'neovm--kd-leaf-count (funcall 'neovm--kd-right n)))))))

  ;; Count internal nodes
  (fset 'neovm--kd-internal-count
    (lambda (n)
      (cond ((null n) 0)
            ((and (null (funcall 'neovm--kd-left n))
                  (null (funcall 'neovm--kd-right n)))
             0)
            (t (+ 1 (funcall 'neovm--kd-internal-count (funcall 'neovm--kd-left n))
                    (funcall 'neovm--kd-internal-count (funcall 'neovm--kd-right n)))))))

  (unwind-protect
      (let* ((points '((2 3) (5 4) (9 6) (4 7) (8 1) (7 2)
                        (1 8) (6 5) (3 1) (0 0) (10 10) (5 5)
                        (1 1) (9 9) (3 7) (7 3)))
             (balanced (funcall 'neovm--kd-build-balanced points 0))
             ;; Worst case: sorted input for sequential insertion
             (sorted-pts (sort (copy-sequence points)
                               (lambda (a b) (< (car a) (car b)))))
             (degenerate (let ((tree nil))
                           (dolist (pt sorted-pts)
                             (setq tree (funcall 'neovm--kd-insert tree pt 0)))
                           tree)))
        (list
         ;; Balanced tree stats
         (list (funcall 'neovm--kd-size balanced)
               (funcall 'neovm--kd-depth balanced)
               (funcall 'neovm--kd-min-depth balanced)
               (funcall 'neovm--kd-leaf-count balanced)
               (funcall 'neovm--kd-internal-count balanced))
         ;; Degenerate tree stats
         (list (funcall 'neovm--kd-size degenerate)
               (funcall 'neovm--kd-depth degenerate)
               (funcall 'neovm--kd-min-depth degenerate)
               (funcall 'neovm--kd-leaf-count degenerate)
               (funcall 'neovm--kd-internal-count degenerate))
         ;; Balance ratio: max-depth / min-depth
         ;; Balanced should be close to 1, degenerate much higher
         (let ((b-ratio (if (> (funcall 'neovm--kd-min-depth balanced) 0)
                            (/ (float (funcall 'neovm--kd-depth balanced))
                               (funcall 'neovm--kd-min-depth balanced))
                          999.0))
               (d-ratio (if (> (funcall 'neovm--kd-min-depth degenerate) 0)
                            (/ (float (funcall 'neovm--kd-depth degenerate))
                               (funcall 'neovm--kd-min-depth degenerate))
                          999.0)))
           (list (< b-ratio 2.0)
                 (> d-ratio b-ratio)))
         ;; leaf + internal = total
         (= (+ (funcall 'neovm--kd-leaf-count balanced)
               (funcall 'neovm--kd-internal-count balanced))
            (funcall 'neovm--kd-size balanced))
         ;; Empty tree stats
         (list (funcall 'neovm--kd-size nil)
               (funcall 'neovm--kd-depth nil)
               (funcall 'neovm--kd-leaf-count nil))
         ;; Single point tree stats
         (let ((single (funcall 'neovm--kd-build-balanced '((5 5)) 0)))
           (list (funcall 'neovm--kd-size single)
                 (funcall 'neovm--kd-depth single)
                 (funcall 'neovm--kd-leaf-count single)
                 (funcall 'neovm--kd-internal-count single)))))
    (fmakunbound 'neovm--kd-make-node)
    (fmakunbound 'neovm--kd-point)
    (fmakunbound 'neovm--kd-left)
    (fmakunbound 'neovm--kd-right)
    (fmakunbound 'neovm--kd-dim)
    (fmakunbound 'neovm--kd-insert)
    (fmakunbound 'neovm--kd-build-balanced)
    (fmakunbound 'neovm--kd-depth)
    (fmakunbound 'neovm--kd-min-depth)
    (fmakunbound 'neovm--kd-size)
    (fmakunbound 'neovm--kd-leaf-count)
    (fmakunbound 'neovm--kd-internal-count)))"#;
    let expect =
        expect_test::expect![[r#""OK ((16 5 4 8 8) (16 10 1 4 12) (t t) t (0 0 0) (1 1 1 0))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// k-d tree: points on a line and collinear degeneration
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_kd_tree_collinear_points() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--kd-make-node
    (lambda (pt l r d) (list pt l r d)))
  (fset 'neovm--kd-point (lambda (n) (nth 0 n)))
  (fset 'neovm--kd-left  (lambda (n) (nth 1 n)))
  (fset 'neovm--kd-right (lambda (n) (nth 2 n)))

  (fset 'neovm--kd-build-balanced
    (lambda (points depth)
      (if (null points) nil
        (let* ((dim (% depth 2))
               (sorted (sort (copy-sequence points)
                             (lambda (a b) (< (nth dim a) (nth dim b)))))
               (mid (/ (length sorted) 2))
               (median-pt (nth mid sorted))
               (left-pts (take mid sorted))
               (right-pts (nthcdr (1+ mid) sorted)))
          (funcall 'neovm--kd-make-node
                   median-pt
                   (funcall 'neovm--kd-build-balanced left-pts (1+ depth))
                   (funcall 'neovm--kd-build-balanced right-pts (1+ depth))
                   dim)))))

  (fset 'neovm--kd-dist-sq
    (lambda (a b)
      (+ (* (- (car a) (car b)) (- (car a) (car b)))
         (* (- (cadr a) (cadr b)) (- (cadr a) (cadr b))))))

  (fset 'neovm--kd-nn-helper
    (lambda (node target best depth)
      (when node
        (let* ((pt (funcall 'neovm--kd-point node))
               (d (funcall 'neovm--kd-dist-sq target pt)))
          (when (< d (cdr best))
            (setcar best pt) (setcdr best d))
          (let* ((dim (% depth 2))
                 (diff (- (nth dim target) (nth dim pt)))
                 (near (if (< diff 0) (funcall 'neovm--kd-left node)
                         (funcall 'neovm--kd-right node)))
                 (far  (if (< diff 0) (funcall 'neovm--kd-right node)
                         (funcall 'neovm--kd-left node))))
            (funcall 'neovm--kd-nn-helper near target best (1+ depth))
            (when (< (* diff diff) (cdr best))
              (funcall 'neovm--kd-nn-helper far target best (1+ depth))))))
      best))

  (fset 'neovm--kd-nearest
    (lambda (tree target)
      (let ((best (cons nil 999999999)))
        (funcall 'neovm--kd-nn-helper tree target best 0)
        best)))

  (fset 'neovm--kd-depth
    (lambda (n) (if (null n) 0
                  (1+ (max (funcall 'neovm--kd-depth (funcall 'neovm--kd-left n))
                           (funcall 'neovm--kd-depth (funcall 'neovm--kd-right n)))))))

  (fset 'neovm--kd-size
    (lambda (n) (if (null n) 0
                  (+ 1 (funcall 'neovm--kd-size (funcall 'neovm--kd-left n))
                       (funcall 'neovm--kd-size (funcall 'neovm--kd-right n))))))

  (unwind-protect
      (let* (;; Points along y=x diagonal
             (diagonal (mapcar (lambda (i) (list i i)) '(0 1 2 3 4 5 6 7 8 9)))
             ;; Points on horizontal line y=5
             (horizontal (mapcar (lambda (i) (list i 5)) '(0 2 4 6 8 10)))
             ;; Points on vertical line x=3
             (vertical (mapcar (lambda (i) (list 3 i)) '(1 3 5 7 9)))
             ;; Duplicate points
             (dupes '((5 5) (5 5) (5 5) (3 3) (3 3) (7 7)))
             (tree-diag (funcall 'neovm--kd-build-balanced diagonal 0))
             (tree-horiz (funcall 'neovm--kd-build-balanced horizontal 0))
             (tree-vert (funcall 'neovm--kd-build-balanced vertical 0))
             (tree-dupes (funcall 'neovm--kd-build-balanced dupes 0)))
        (list
         ;; Diagonal tree stats
         (list (funcall 'neovm--kd-size tree-diag)
               (funcall 'neovm--kd-depth tree-diag))
         ;; Nearest on diagonal to off-diagonal point
         (funcall 'neovm--kd-nearest tree-diag '(3 5))
         (funcall 'neovm--kd-nearest tree-diag '(0 9))
         ;; Horizontal line: nearest to (5 5) should be (4 5) or (6 5)
         (let ((res (funcall 'neovm--kd-nearest tree-horiz '(5 5))))
           (list (car res) (cdr res)))
         ;; Vertical line tree
         (list (funcall 'neovm--kd-size tree-vert)
               (funcall 'neovm--kd-depth tree-vert))
         ;; Duplicate points tree
         (list (funcall 'neovm--kd-size tree-dupes)
               (funcall 'neovm--kd-depth tree-dupes))
         ;; Nearest to (5 5) in dupes tree: dist should be 0
         (cdr (funcall 'neovm--kd-nearest tree-dupes '(5 5)))))
    (fmakunbound 'neovm--kd-make-node)
    (fmakunbound 'neovm--kd-point)
    (fmakunbound 'neovm--kd-left)
    (fmakunbound 'neovm--kd-right)
    (fmakunbound 'neovm--kd-build-balanced)
    (fmakunbound 'neovm--kd-dist-sq)
    (fmakunbound 'neovm--kd-nn-helper)
    (fmakunbound 'neovm--kd-nearest)
    (fmakunbound 'neovm--kd-depth)
    (fmakunbound 'neovm--kd-size)))"#;
    let expect =
        expect_test::expect![[r#""OK ((10 4) ((4 4) . 2) ((5 5) . 41) ((6 5) 1) (5 3) (6 3) 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
