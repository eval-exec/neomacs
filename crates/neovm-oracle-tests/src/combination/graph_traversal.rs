//! Oracle parity tests for graph traversal algorithms implemented in
//! pure Elisp using alists as adjacency lists: BFS, DFS (iterative
//! with explicit stack), topological sort, cycle detection, shortest
//! path (unweighted BFS), and connected components.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// BFS with alist adjacency lists and distance tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_traversal_bfs_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a graph as an alist of (node . neighbors), perform BFS from
    // a source, and return visit order, distances, and parent map.
    let form = r#"(progn
  (fset 'neovm--gt-neighbors
    (lambda (graph node)
      (cdr (assq node graph))))
  (fset 'neovm--gt-bfs
    (lambda (graph source)
      (let ((visited (list source))
            (dist-alist (list (cons source 0)))
            (parent-alist nil)
            (queue (list (cons source 0)))
            (visit-order nil))
        (while queue
          (let* ((item (car queue))
                 (node (car item))
                 (d (cdr item)))
            (setq queue (cdr queue))
            (setq visit-order (cons node visit-order))
            (dolist (nbr (funcall 'neovm--gt-neighbors graph node))
              (unless (memq nbr visited)
                (setq visited (cons nbr visited))
                (setq dist-alist (cons (cons nbr (1+ d)) dist-alist))
                (setq parent-alist (cons (cons nbr node) parent-alist))
                (setq queue (append queue (list (cons nbr (1+ d)))))))))
        (list (nreverse visit-order)
              (sort dist-alist (lambda (a b)
                                 (string< (symbol-name (car a))
                                          (symbol-name (car b)))))
              (sort parent-alist (lambda (a b)
                                   (string< (symbol-name (car a))
                                            (symbol-name (car b)))))))))
  (unwind-protect
      (let ((graph '((a . (b c d))
                     (b . (a e))
                     (c . (a f g))
                     (d . (a g))
                     (e . (b h))
                     (f . (c))
                     (g . (c d h))
                     (h . (e g)))))
        (let ((result (funcall 'neovm--gt-bfs graph 'a)))
          (list (car result)       ;; visit order
                (cadr result)      ;; distances
                (caddr result)     ;; parents
                (length (car result)))))  ;; should visit all 8 nodes
    (fmakunbound 'neovm--gt-neighbors)
    (fmakunbound 'neovm--gt-bfs)))"#;
    let expect = expect_test::expect![[
        r#""OK ((a b c d e f g h) ((a . 0) (b . 1) (c . 1) (d . 1) (e . 2) (f . 2) (g . 2) (h . 3)) ((b . a) (c . a) (d . a) (e . b) (f . c) (g . c) (h . e)) 8)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Iterative DFS with explicit stack and discovery/finish times
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_traversal_dfs_iterative_stack() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Iterative DFS using an explicit stack. Track discovery and finish
    // ordering. For a directed graph, also classify edges.
    let form = r#"(progn
  (fset 'neovm--gt-dfs-iterative
    (lambda (graph nodes)
      (let ((color (make-hash-table))
            (discovery nil)
            (finish nil)
            (time 0))
        ;; Initialize all white
        (dolist (n nodes) (puthash n 'white color))
        ;; For each unvisited node, run DFS
        (dolist (start nodes)
          (when (eq (gethash start color) 'white)
            ;; Stack holds (node . phase): phase=enter or phase=exit
            (let ((stack (list (cons start 'enter))))
              (while stack
                (let* ((top (car stack))
                       (node (car top))
                       (phase (cdr top)))
                  (setq stack (cdr stack))
                  (cond
                    ((eq phase 'exit)
                     (setq time (1+ time))
                     (setq finish (cons (cons node time) finish))
                     (puthash node 'black color))
                    ((eq (gethash node color) 'white)
                     (puthash node 'gray color)
                     (setq time (1+ time))
                     (setq discovery (cons (cons node time) discovery))
                     ;; Push exit marker, then neighbors (reversed for order)
                     (setq stack (cons (cons node 'exit) stack))
                     (let ((neighbors (cdr (assq node graph))))
                       (dolist (nbr (reverse neighbors))
                         (when (eq (gethash nbr color 'white) 'white)
                           (setq stack (cons (cons nbr 'enter) stack))))))))))))
        (list (sort (nreverse discovery)
                    (lambda (a b) (< (cdr a) (cdr b))))
              (sort (nreverse finish)
                    (lambda (a b) (< (cdr a) (cdr b))))))))
  (unwind-protect
      (let ((graph '((a . (b c))
                     (b . (d e))
                     (c . (f))
                     (d . ())
                     (e . (f))
                     (f . ()))))
        (let ((result (funcall 'neovm--gt-dfs-iterative
                               graph '(a b c d e f))))
          (list (car result)     ;; discovery times
                (cadr result)    ;; finish times
                ;; Verify all nodes discovered
                (= (length (car result)) 6)
                ;; Verify all nodes finished
                (= (length (cadr result)) 6))))
    (fmakunbound 'neovm--gt-dfs-iterative)))"#;
    let expect = expect_test::expect![[
        r#""OK (((a . 1) (b . 2) (d . 3) (e . 5) (f . 6) (c . 10)) ((d . 4) (f . 7) (e . 8) (b . 9) (c . 11) (a . 12)) t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Topological sort via DFS post-order (Tarjan-style) using alists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_traversal_topological_sort_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Topological sort on a DAG represented as alist. Uses DFS and
    // collects nodes in reverse post-order. Verifies the ordering
    // is valid: for every edge (u,v), u precedes v.
    let form = r#"(progn
  (fset 'neovm--gt-topo-sort
    (lambda (graph nodes)
      (let ((visited (make-hash-table))
            (result nil))
        (fset 'neovm--gt-topo-visit
          (lambda (node)
            (unless (gethash node visited)
              (puthash node t visited)
              (dolist (nbr (cdr (assq node graph)))
                (funcall 'neovm--gt-topo-visit nbr))
              (setq result (cons node result)))))
        (dolist (n nodes)
          (funcall 'neovm--gt-topo-visit n))
        result)))

  ;; Verify topological order
  (fset 'neovm--gt-verify-topo
    (lambda (graph order)
      (let ((pos (make-hash-table))
            (idx 0)
            (valid t))
        (dolist (n order)
          (puthash n idx pos)
          (setq idx (1+ idx)))
        (dolist (entry graph)
          (let ((u (car entry)))
            (dolist (v (cdr entry))
              (when (>= (gethash u pos) (gethash v pos))
                (setq valid nil)))))
        valid)))

  (unwind-protect
      (let* (;; Task dependency graph (build system)
             (graph '((compile-core . (link))
                      (compile-ui . (link))
                      (compile-tests . (run-tests))
                      (link . (run-tests package))
                      (run-tests . (deploy))
                      (package . (deploy))
                      (deploy . ())))
             (nodes '(compile-core compile-ui compile-tests
                      link run-tests package deploy))
             (order (funcall 'neovm--gt-topo-sort graph nodes))
             (valid (funcall 'neovm--gt-verify-topo graph order)))
        (list order
              valid
              (length order)
              (= (length order) (length nodes))))
    (fmakunbound 'neovm--gt-topo-sort)
    (fmakunbound 'neovm--gt-topo-visit)
    (fmakunbound 'neovm--gt-verify-topo)))"#;
    let expect = expect_test::expect![[
        r#""OK ((compile-tests compile-ui compile-core link package run-tests deploy) t 7 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cycle detection in directed graph with cycle path reporting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_traversal_cycle_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Detect cycles in a directed graph using DFS with tri-color marking.
    // When a back edge is found (gray -> gray), report it. Test on both
    // a DAG (no cycle) and a graph with multiple cycles.
    let form = r#"(progn
  (fset 'neovm--gt-detect-cycles
    (lambda (graph nodes)
      (let ((color (make-hash-table))
            (back-edges nil))
        (dolist (n nodes) (puthash n 'white color))
        (fset 'neovm--gt-cycle-visit
          (lambda (u)
            (puthash u 'gray color)
            (dolist (v (cdr (assq u graph)))
              (let ((c (gethash v color 'white)))
                (cond
                  ((eq c 'gray)
                   (setq back-edges (cons (cons u v) back-edges)))
                  ((eq c 'white)
                   (funcall 'neovm--gt-cycle-visit v)))))
            (puthash u 'black color)))
        (dolist (n nodes)
          (when (eq (gethash n color) 'white)
            (funcall 'neovm--gt-cycle-visit n)))
        (list (not (null back-edges))
              (nreverse back-edges)))))

  (unwind-protect
      (let ((results nil))
        ;; DAG: no cycles
        (let ((dag '((a . (b c))
                     (b . (d))
                     (c . (d e))
                     (d . (f))
                     (e . (f))
                     (f . ()))))
          (setq results
                (cons (list 'dag
                            (funcall 'neovm--gt-detect-cycles
                                     dag '(a b c d e f)))
                      results)))
        ;; Graph with cycle: a->b->c->a and d->e->d
        (let ((cyclic '((a . (b))
                        (b . (c))
                        (c . (a d))
                        (d . (e))
                        (e . (d f))
                        (f . ()))))
          (setq results
                (cons (list 'cyclic
                            (funcall 'neovm--gt-detect-cycles
                                     cyclic '(a b c d e f)))
                      results)))
        ;; Self-loop
        (let ((self-loop '((x . (x y))
                           (y . (z))
                           (z . ()))))
          (setq results
                (cons (list 'self-loop
                            (funcall 'neovm--gt-detect-cycles
                                     self-loop '(x y z)))
                      results)))
        (nreverse results))
    (fmakunbound 'neovm--gt-detect-cycles)
    (fmakunbound 'neovm--gt-cycle-visit)))"#;
    let expect = expect_test::expect![[
        r#""OK ((dag (nil nil)) (cyclic (t ((c . a) (e . d)))) (self-loop (t ((x . x)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Shortest path (unweighted BFS) with full path reconstruction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_traversal_shortest_path_bfs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // BFS-based shortest path on an undirected graph represented as
    // an alist. Compute distances and reconstruct actual paths for
    // multiple source-target pairs.
    let form = r#"(progn
  (fset 'neovm--gt-build-undirected
    (lambda (edges)
      (let ((graph nil))
        (dolist (edge edges)
          (let ((u (car edge)) (v (cadr edge)))
            ;; Add u->v
            (let ((entry (assq u graph)))
              (if entry
                  (setcdr entry (cons v (cdr entry)))
                (setq graph (cons (cons u (list v)) graph))))
            ;; Add v->u
            (let ((entry (assq v graph)))
              (if entry
                  (setcdr entry (cons u (cdr entry)))
                (setq graph (cons (cons v (list u)) graph))))))
        graph)))

  (fset 'neovm--gt-bfs-paths
    (lambda (graph source)
      (let ((dist (make-hash-table))
            (parent (make-hash-table))
            (queue (list source)))
        (puthash source 0 dist)
        (while queue
          (let ((u (car queue)))
            (setq queue (cdr queue))
            (dolist (v (cdr (assq u graph)))
              (unless (gethash v dist)
                (puthash v (1+ (gethash u dist)) dist)
                (puthash v u parent)
                (setq queue (append queue (list v)))))))
        (cons dist parent))))

  (fset 'neovm--gt-reconstruct
    (lambda (parent-map source target)
      (if (eq source target)
          (list source)
        (let ((path nil) (cur target))
          (while (and cur (not (eq cur source)))
            (setq path (cons cur path))
            (setq cur (gethash cur parent-map)))
          (if cur (cons source path) nil)))))

  (unwind-protect
      (let* ((edges '((a b) (a c) (b d) (b e) (c f)
                      (d g) (e g) (e h) (f h) (g i) (h i)))
             (graph (funcall 'neovm--gt-build-undirected edges))
             (bfs-result (funcall 'neovm--gt-bfs-paths graph 'a))
             (dist-map (car bfs-result))
             (parent-map (cdr bfs-result)))
        ;; Collect all distances
        (let ((all-dist nil))
          (dolist (n '(a b c d e f g h i))
            (setq all-dist (cons (cons n (gethash n dist-map -1))
                                 all-dist)))
          ;; Reconstruct paths for specific pairs
          (let ((path-a-i (funcall 'neovm--gt-reconstruct parent-map 'a 'i))
                (path-a-h (funcall 'neovm--gt-reconstruct parent-map 'a 'h))
                (path-a-g (funcall 'neovm--gt-reconstruct parent-map 'a 'g))
                (path-a-a (funcall 'neovm--gt-reconstruct parent-map 'a 'a)))
            (list (sort (nreverse all-dist)
                        (lambda (a b) (string< (symbol-name (car a))
                                               (symbol-name (car b)))))
                  (list 'path-a-i path-a-i (length path-a-i))
                  (list 'path-a-h path-a-h (length path-a-h))
                  (list 'path-a-g path-a-g (length path-a-g))
                  (list 'path-a-a path-a-a)))))
    (fmakunbound 'neovm--gt-build-undirected)
    (fmakunbound 'neovm--gt-bfs-paths)
    (fmakunbound 'neovm--gt-reconstruct)))"#;
    let expect = expect_test::expect![[
        r#""OK (((a . 0) (b . 1) (c . 1) (d . 2) (e . 2) (f . 2) (g . 3) (h . 3) (i . 4)) (path-a-i (a c f h i) 5) (path-a-h (a c f h) 4) (path-a-g (a b e g) 4) (path-a-a (a)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Connected components with component labeling and statistics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_traversal_connected_components() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Find connected components in an undirected graph using BFS.
    // Label each node with its component ID, compute sizes, find
    // the largest and smallest components, and count isolated nodes.
    let form = r#"(progn
  (fset 'neovm--gt-find-cc
    (lambda (graph all-nodes)
      (let ((component-id (make-hash-table))
            (components nil)
            (next-id 0))
        (dolist (start all-nodes)
          (unless (gethash start component-id)
            (let ((queue (list start))
                  (members nil))
              (puthash start next-id component-id)
              (while queue
                (let ((u (car queue)))
                  (setq queue (cdr queue))
                  (setq members (cons u members))
                  (dolist (v (cdr (assq u graph)))
                    (unless (gethash v component-id)
                      (puthash v next-id component-id)
                      (setq queue (append queue (list v)))))))
              (setq components
                    (cons (cons next-id (sort (nreverse members)
                                              (lambda (a b) (< a b))))
                          components))
              (setq next-id (1+ next-id)))))
        (list (nreverse components) component-id))))

  (unwind-protect
      (let* (;; 4 components: {1,2,3}, {4,5,6,7}, {8,9}, {10} (isolated)
             (edges '((1 2) (2 3) (1 3)
                      (4 5) (5 6) (6 7) (4 7)
                      (8 9)))
             (all-nodes '(1 2 3 4 5 6 7 8 9 10))
             ;; Build undirected alist
             (graph nil))
        (dolist (n all-nodes)
          (setq graph (cons (cons n nil) graph)))
        (dolist (edge edges)
          (let ((u (car edge)) (v (cadr edge)))
            (let ((eu (assq u graph)))
              (setcdr eu (cons v (cdr eu))))
            (let ((ev (assq v graph)))
              (setcdr ev (cons u (cdr ev))))))
        (let* ((result (funcall 'neovm--gt-find-cc graph all-nodes))
               (components (car result))
               (sizes (mapcar (lambda (c) (length (cdr c))) components))
               (largest (car (sort (copy-sequence components)
                                   (lambda (a b)
                                     (> (length (cdr a))
                                        (length (cdr b)))))))
               (smallest (car (sort (copy-sequence components)
                                    (lambda (a b)
                                      (< (length (cdr a))
                                         (length (cdr b)))))))
               (isolated (let ((iso nil))
                           (dolist (c components)
                             (when (= (length (cdr c)) 1)
                               (setq iso (append iso (cdr c)))))
                           iso)))
          (list (list 'num-components (length components))
                (list 'sizes (sort (copy-sequence sizes) #'>))
                (list 'largest (cdr largest))
                (list 'smallest (cdr smallest))
                (list 'isolated isolated)
                (list 'all-components
                      (mapcar (lambda (c) (cdr c)) components)))))
    (fmakunbound 'neovm--gt-find-cc)))"#;
    let expect = expect_test::expect![[
        r#""OK ((num-components 4) (sizes (4 3 2 1)) (largest (4 5 6 7)) (smallest (10)) (isolated (10)) (all-components ((1 2 3) (4 5 6 7) (8 9) (10))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Graph with multiple topologies: star, ring, complete, tree
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_graph_traversal_topology_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build several graph topologies as alists and compute properties:
    // diameter (max shortest path), is-tree check (connected + n-1 edges),
    // degree sequence.
    let form = r#"(progn
  (fset 'neovm--gt-make-undirected-alist
    (lambda (edges nodes)
      (let ((graph nil))
        (dolist (n nodes)
          (setq graph (cons (cons n nil) graph)))
        (dolist (e edges)
          (let ((u (car e)) (v (cadr e)))
            (let ((eu (assq u graph)))
              (setcdr eu (cons v (cdr eu))))
            (let ((ev (assq v graph)))
              (setcdr ev (cons u (cdr ev))))))
        graph)))

  (fset 'neovm--gt-degree-seq
    (lambda (graph nodes)
      (let ((degrees nil))
        (dolist (n nodes)
          (setq degrees (cons (length (cdr (assq n graph))) degrees)))
        (sort (nreverse degrees) #'>))))

  (fset 'neovm--gt-bfs-max-dist
    (lambda (graph source)
      (let ((dist (make-hash-table))
            (queue (list source))
            (max-d 0))
        (puthash source 0 dist)
        (while queue
          (let ((u (car queue)))
            (setq queue (cdr queue))
            (dolist (v (cdr (assq u graph)))
              (unless (gethash v dist)
                (let ((d (1+ (gethash u dist))))
                  (puthash v d dist)
                  (when (> d max-d) (setq max-d d))
                  (setq queue (append queue (list v))))))))
        max-d)))

  (fset 'neovm--gt-diameter
    (lambda (graph nodes)
      (let ((max-dist 0))
        (dolist (n nodes)
          (let ((d (funcall 'neovm--gt-bfs-max-dist graph n)))
            (when (> d max-dist) (setq max-dist d))))
        max-dist)))

  (unwind-protect
      (let ((results nil))
        ;; Star graph: center=1, spokes to 2,3,4,5
        (let* ((star-nodes '(1 2 3 4 5))
               (star-edges '((1 2) (1 3) (1 4) (1 5)))
               (star (funcall 'neovm--gt-make-undirected-alist
                              star-edges star-nodes)))
          (setq results
                (cons (list 'star
                            (funcall 'neovm--gt-degree-seq star star-nodes)
                            (funcall 'neovm--gt-diameter star star-nodes)
                            (length star-edges))
                      results)))
        ;; Ring graph: 1-2-3-4-5-1
        (let* ((ring-nodes '(1 2 3 4 5))
               (ring-edges '((1 2) (2 3) (3 4) (4 5) (5 1)))
               (ring (funcall 'neovm--gt-make-undirected-alist
                              ring-edges ring-nodes)))
          (setq results
                (cons (list 'ring
                            (funcall 'neovm--gt-degree-seq ring ring-nodes)
                            (funcall 'neovm--gt-diameter ring ring-nodes)
                            (length ring-edges))
                      results)))
        ;; Complete graph K4: 1-2, 1-3, 1-4, 2-3, 2-4, 3-4
        (let* ((k4-nodes '(1 2 3 4))
               (k4-edges '((1 2) (1 3) (1 4) (2 3) (2 4) (3 4)))
               (k4 (funcall 'neovm--gt-make-undirected-alist
                            k4-edges k4-nodes)))
          (setq results
                (cons (list 'complete-k4
                            (funcall 'neovm--gt-degree-seq k4 k4-nodes)
                            (funcall 'neovm--gt-diameter k4 k4-nodes)
                            (length k4-edges))
                      results)))
        ;; Binary tree: 1->{2,3}, 2->{4,5}, 3->{6,7}
        (let* ((tree-nodes '(1 2 3 4 5 6 7))
               (tree-edges '((1 2) (1 3) (2 4) (2 5) (3 6) (3 7)))
               (tree (funcall 'neovm--gt-make-undirected-alist
                              tree-edges tree-nodes)))
          (setq results
                (cons (list 'binary-tree
                            (funcall 'neovm--gt-degree-seq tree tree-nodes)
                            (funcall 'neovm--gt-diameter tree tree-nodes)
                            (length tree-edges)
                            ;; Tree check: n-1 edges
                            (= (length tree-edges)
                               (1- (length tree-nodes))))
                      results)))
        (nreverse results))
    (fmakunbound 'neovm--gt-make-undirected-alist)
    (fmakunbound 'neovm--gt-degree-seq)
    (fmakunbound 'neovm--gt-bfs-max-dist)
    (fmakunbound 'neovm--gt-diameter)))"#;
    let expect = expect_test::expect![[
        r#""OK ((star (4 1 1 1 1) 2 4) (ring (2 2 2 2 2) 2 5) (complete-k4 (3 3 3 3) 1 6) (binary-tree (3 3 2 1 1 1 1) 4 6 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
