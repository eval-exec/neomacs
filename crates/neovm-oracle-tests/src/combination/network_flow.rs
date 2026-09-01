//! Complex oracle parity tests implementing network flow algorithms in Elisp:
//! graph/network representation with capacities, BFS-based augmenting path
//! finding (Edmonds-Karp), residual graph computation, maximum flow,
//! and min-cut derivation from max-flow.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Network representation and capacity graph construction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_network_flow_graph_construction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a capacity graph from edge list, query capacities and neighbors
    let form = r#"(progn
  ;; capacity key: "u->v" string
  (fset 'neovm--nf-edge-key
    (lambda (u v)
      (concat (symbol-name u) "->" (symbol-name v))))

  (fset 'neovm--nf-build-graph
    (lambda (edges)
      (let ((cap (make-hash-table :test 'equal))
            (adj (make-hash-table)))
        (dolist (e edges)
          (let ((u (nth 0 e)) (v (nth 1 e)) (c (nth 2 e)))
            ;; Set forward capacity
            (puthash (funcall 'neovm--nf-edge-key u v) c cap)
            ;; Ensure reverse edge exists with 0 capacity if not set
            (unless (gethash (funcall 'neovm--nf-edge-key v u) cap)
              (puthash (funcall 'neovm--nf-edge-key v u) 0 cap))
            ;; Add to adjacency list (both directions for residual graph)
            (unless (memq v (gethash u adj nil))
              (puthash u (cons v (gethash u adj nil)) adj))
            (unless (memq u (gethash v adj nil))
              (puthash v (cons u (gethash v adj nil)) adj))))
        (list cap adj))))

  (unwind-protect
      (let* ((edges '((s a 10) (s b 8) (a b 5) (a c 7) (b d 10)
                      (c t 10) (d t 7) (c d 6)))
             (graph (funcall 'neovm--nf-build-graph edges))
             (cap (car graph))
             (adj (cadr graph)))
        (list
         ;; Verify capacities
         (gethash (funcall 'neovm--nf-edge-key 's 'a) cap)
         (gethash (funcall 'neovm--nf-edge-key 's 'b) cap)
         (gethash (funcall 'neovm--nf-edge-key 'a 'c) cap)
         (gethash (funcall 'neovm--nf-edge-key 'c 't) cap)
         ;; Reverse edges have 0 capacity
         (gethash (funcall 'neovm--nf-edge-key 'a 's) cap)
         (gethash (funcall 'neovm--nf-edge-key 'b 's) cap)
         ;; Adjacency list sizes
         (length (gethash 's adj))
         (length (gethash 'a adj))
         ;; Total edges in capacity map
         (hash-table-count cap)))
    (fmakunbound 'neovm--nf-edge-key)
    (fmakunbound 'neovm--nf-build-graph)))"#;
    let expect = expect_test::expect![[r#""OK (10 8 7 10 0 0 2 3 16)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// BFS for augmenting path
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_network_flow_bfs_augmenting_path() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // BFS that finds a path from source to sink in the residual graph
    let form = r#"(progn
  (fset 'neovm--nf-ek
    (lambda (u v) (concat (symbol-name u) "->" (symbol-name v))))

  ;; BFS: returns parent map or nil if no path
  (fset 'neovm--nf-bfs
    (lambda (adj cap source sink)
      (let ((visited (make-hash-table))
            (parent (make-hash-table))
            (queue (list source)))
        (puthash source t visited)
        (let ((found nil))
          (while (and queue (not found))
            (let ((u (car queue)))
              (setq queue (cdr queue))
              (dolist (v (gethash u adj nil))
                (let ((residual (- (gethash (funcall 'neovm--nf-ek u v) cap 0)
                                   (gethash (funcall 'neovm--nf-ek v u) cap 0))))
                  ;; Wait, residual = cap[u->v] - flow[u->v]. Let's use cap as residual directly.
                  (let ((res-cap (gethash (funcall 'neovm--nf-ek u v) cap 0)))
                    (when (and (> res-cap 0) (not (gethash v visited)))
                      (puthash v t visited)
                      (puthash v u parent)
                      (setq queue (append queue (list v)))
                      (when (eq v sink)
                        (setq found t))))))))
          (if found parent nil)))))

  (unwind-protect
      (let ((cap (make-hash-table :test 'equal))
            (adj (make-hash-table)))
        ;; Simple graph: s -> a -> b -> t, s -> b (direct)
        (dolist (e '((s a 10) (a b 5) (b t 10) (s b 7)))
          (let ((u (nth 0 e)) (v (nth 1 e)) (c (nth 2 e)))
            (puthash (funcall 'neovm--nf-ek u v) c cap)
            (unless (gethash (funcall 'neovm--nf-ek v u) cap)
              (puthash (funcall 'neovm--nf-ek v u) 0 cap))
            (unless (memq v (gethash u adj nil))
              (puthash u (cons v (gethash u adj nil)) adj))
            (unless (memq u (gethash v adj nil))
              (puthash v (cons u (gethash v adj nil)) adj))))
        (let ((parent-map (funcall 'neovm--nf-bfs adj cap 's 't)))
          (if parent-map
              ;; Reconstruct path
              (let ((path nil) (node 't))
                (while node
                  (setq path (cons node path))
                  (setq node (gethash node parent-map)))
                (list 'found path (length path)))
            'no-path)))
    (fmakunbound 'neovm--nf-ek)
    (fmakunbound 'neovm--nf-bfs)))"#;
    let expect = expect_test::expect![[r#""OK (found (s b t) 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Edmonds-Karp maximum flow (full algorithm)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_network_flow_edmonds_karp_simple() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--ek-key
    (lambda (u v) (concat (symbol-name u) "->" (symbol-name v))))

  (fset 'neovm--ek-bfs
    (lambda (adj rcap source sink)
      (let ((visited (make-hash-table))
            (parent (make-hash-table))
            (queue (list source)))
        (puthash source t visited)
        (let ((found nil))
          (while (and queue (not found))
            (let ((u (car queue)))
              (setq queue (cdr queue))
              (dolist (v (gethash u adj nil))
                (when (and (> (gethash (funcall 'neovm--ek-key u v) rcap 0) 0)
                           (not (gethash v visited)))
                  (puthash v t visited)
                  (puthash v u parent)
                  (setq queue (append queue (list v)))
                  (when (eq v sink)
                    (setq found t))))))
          (if found parent nil)))))

  (fset 'neovm--ek-maxflow
    (lambda (edges source sink)
      (let ((rcap (make-hash-table :test 'equal))
            (adj (make-hash-table))
            (total-flow 0))
        ;; Build residual capacity graph
        (dolist (e edges)
          (let ((u (nth 0 e)) (v (nth 1 e)) (c (nth 2 e)))
            (puthash (funcall 'neovm--ek-key u v)
                     (+ (gethash (funcall 'neovm--ek-key u v) rcap 0) c) rcap)
            (unless (gethash (funcall 'neovm--ek-key v u) rcap)
              (puthash (funcall 'neovm--ek-key v u) 0 rcap))
            (unless (memq v (gethash u adj nil))
              (puthash u (cons v (gethash u adj nil)) adj))
            (unless (memq u (gethash v adj nil))
              (puthash v (cons u (gethash v adj nil)) adj))))
        ;; Repeatedly find augmenting paths
        (let ((continue t))
          (while continue
            (let ((parent (funcall 'neovm--ek-bfs adj rcap source sink)))
              (if (null parent)
                  (setq continue nil)
                ;; Find bottleneck
                (let ((bottleneck most-positive-fixnum)
                      (v sink))
                  (while (not (eq v source))
                    (let ((u (gethash v parent)))
                      (setq bottleneck
                            (min bottleneck
                                 (gethash (funcall 'neovm--ek-key u v) rcap 0)))
                      (setq v u)))
                  ;; Update residual capacities
                  (setq v sink)
                  (while (not (eq v source))
                    (let ((u (gethash v parent)))
                      (puthash (funcall 'neovm--ek-key u v)
                               (- (gethash (funcall 'neovm--ek-key u v) rcap) bottleneck) rcap)
                      (puthash (funcall 'neovm--ek-key v u)
                               (+ (gethash (funcall 'neovm--ek-key v u) rcap) bottleneck) rcap)
                      (setq v u)))
                  (setq total-flow (+ total-flow bottleneck)))))))
        total-flow)))

  (unwind-protect
      ;; Classic textbook example:
      ;;   s --10--> a --7--> c --10--> t
      ;;   s --8-->  b        c --6-->  d
      ;;   a --5-->  b        d --7-->  t
      ;;   b --10--> d
      (let ((edges '((s a 10) (s b 8) (a b 5) (a c 7) (b d 10)
                     (c t 10) (d t 7) (c d 6))))
        (funcall 'neovm--ek-maxflow edges 's 't))
    (fmakunbound 'neovm--ek-key)
    (fmakunbound 'neovm--ek-bfs)
    (fmakunbound 'neovm--ek-maxflow)))"#;
    let expect = expect_test::expect![[r#""OK 14""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Max flow on a larger network
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_network_flow_larger_network() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--ek2-key
    (lambda (u v) (concat (symbol-name u) "->" (symbol-name v))))

  (fset 'neovm--ek2-bfs
    (lambda (adj rcap source sink)
      (let ((visited (make-hash-table))
            (parent (make-hash-table))
            (queue (list source)))
        (puthash source t visited)
        (let ((found nil))
          (while (and queue (not found))
            (let ((u (car queue)))
              (setq queue (cdr queue))
              (dolist (v (gethash u adj nil))
                (when (and (> (gethash (funcall 'neovm--ek2-key u v) rcap 0) 0)
                           (not (gethash v visited)))
                  (puthash v t visited)
                  (puthash v u parent)
                  (setq queue (append queue (list v)))
                  (when (eq v sink) (setq found t))))))
          (if found parent nil)))))

  (fset 'neovm--ek2-maxflow
    (lambda (edges source sink)
      (let ((rcap (make-hash-table :test 'equal))
            (adj (make-hash-table))
            (total-flow 0))
        (dolist (e edges)
          (let ((u (nth 0 e)) (v (nth 1 e)) (c (nth 2 e)))
            (puthash (funcall 'neovm--ek2-key u v)
                     (+ (gethash (funcall 'neovm--ek2-key u v) rcap 0) c) rcap)
            (unless (gethash (funcall 'neovm--ek2-key v u) rcap)
              (puthash (funcall 'neovm--ek2-key v u) 0 rcap))
            (unless (memq v (gethash u adj nil))
              (puthash u (cons v (gethash u adj nil)) adj))
            (unless (memq u (gethash v adj nil))
              (puthash v (cons u (gethash v adj nil)) adj))))
        (let ((continue t))
          (while continue
            (let ((parent (funcall 'neovm--ek2-bfs adj rcap source sink)))
              (if (null parent) (setq continue nil)
                (let ((bottleneck most-positive-fixnum) (v sink))
                  (while (not (eq v source))
                    (let ((u (gethash v parent)))
                      (setq bottleneck (min bottleneck (gethash (funcall 'neovm--ek2-key u v) rcap 0)))
                      (setq v u)))
                  (setq v sink)
                  (while (not (eq v source))
                    (let ((u (gethash v parent)))
                      (puthash (funcall 'neovm--ek2-key u v)
                               (- (gethash (funcall 'neovm--ek2-key u v) rcap) bottleneck) rcap)
                      (puthash (funcall 'neovm--ek2-key v u)
                               (+ (gethash (funcall 'neovm--ek2-key v u) rcap) bottleneck) rcap)
                      (setq v u)))
                  (setq total-flow (+ total-flow bottleneck)))))))
        total-flow)))

  (unwind-protect
      (let ((edges '((s a 16) (s c 13)
                     (a b 12) (a c 4)
                     (b t 20)
                     (c a 10) (c d 14)
                     (d b 7) (d t 4))))
        ;; Known max-flow for this graph is 23
        (funcall 'neovm--ek2-maxflow edges 's 't))
    (fmakunbound 'neovm--ek2-key)
    (fmakunbound 'neovm--ek2-bfs)
    (fmakunbound 'neovm--ek2-maxflow)))"#;
    let expect = expect_test::expect![[r#""OK 23""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Residual graph inspection after max-flow
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_network_flow_residual_graph() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--nfr-key
    (lambda (u v) (concat (symbol-name u) "->" (symbol-name v))))

  (fset 'neovm--nfr-bfs
    (lambda (adj rcap source sink)
      (let ((visited (make-hash-table))
            (parent (make-hash-table))
            (queue (list source)))
        (puthash source t visited)
        (let ((found nil))
          (while (and queue (not found))
            (let ((u (car queue)))
              (setq queue (cdr queue))
              (dolist (v (gethash u adj nil))
                (when (and (> (gethash (funcall 'neovm--nfr-key u v) rcap 0) 0)
                           (not (gethash v visited)))
                  (puthash v t visited)
                  (puthash v u parent)
                  (setq queue (append queue (list v)))
                  (when (eq v sink) (setq found t))))))
          (if found parent nil)))))

  (fset 'neovm--nfr-maxflow-with-residual
    (lambda (edges source sink)
      (let ((rcap (make-hash-table :test 'equal))
            (adj (make-hash-table))
            (total-flow 0)
            (original-cap (make-hash-table :test 'equal)))
        (dolist (e edges)
          (let ((u (nth 0 e)) (v (nth 1 e)) (c (nth 2 e)))
            (puthash (funcall 'neovm--nfr-key u v)
                     (+ (gethash (funcall 'neovm--nfr-key u v) rcap 0) c) rcap)
            (puthash (funcall 'neovm--nfr-key u v)
                     (+ (gethash (funcall 'neovm--nfr-key u v) original-cap 0) c) original-cap)
            (unless (gethash (funcall 'neovm--nfr-key v u) rcap)
              (puthash (funcall 'neovm--nfr-key v u) 0 rcap))
            (unless (gethash (funcall 'neovm--nfr-key v u) original-cap)
              (puthash (funcall 'neovm--nfr-key v u) 0 original-cap))
            (unless (memq v (gethash u adj nil))
              (puthash u (cons v (gethash u adj nil)) adj))
            (unless (memq u (gethash v adj nil))
              (puthash v (cons u (gethash v adj nil)) adj))))
        (let ((continue t))
          (while continue
            (let ((parent (funcall 'neovm--nfr-bfs adj rcap source sink)))
              (if (null parent) (setq continue nil)
                (let ((bottleneck most-positive-fixnum) (v sink))
                  (while (not (eq v source))
                    (let ((u (gethash v parent)))
                      (setq bottleneck (min bottleneck (gethash (funcall 'neovm--nfr-key u v) rcap 0)))
                      (setq v u)))
                  (setq v sink)
                  (while (not (eq v source))
                    (let ((u (gethash v parent)))
                      (puthash (funcall 'neovm--nfr-key u v)
                               (- (gethash (funcall 'neovm--nfr-key u v) rcap) bottleneck) rcap)
                      (puthash (funcall 'neovm--nfr-key v u)
                               (+ (gethash (funcall 'neovm--nfr-key v u) rcap) bottleneck) rcap)
                      (setq v u)))
                  (setq total-flow (+ total-flow bottleneck)))))))
        (list total-flow rcap original-cap))))

  (unwind-protect
      (let* ((edges '((s a 10) (s b 8) (a b 5) (a c 7) (b d 10) (c t 10) (d t 7) (c d 6)))
             (result (funcall 'neovm--nfr-maxflow-with-residual edges 's 't))
             (flow (nth 0 result))
             (rcap (nth 1 result))
             (ocap (nth 2 result)))
        ;; Compute flow on each original edge: flow = original_cap - residual_cap
        (let ((edge-flows nil))
          (dolist (e edges)
            (let* ((u (nth 0 e)) (v (nth 1 e))
                   (key (funcall 'neovm--nfr-key u v))
                   (orig (gethash key ocap 0))
                   (resid (gethash key rcap 0))
                   (edge-flow (- orig resid)))
              (setq edge-flows (cons (list u v edge-flow orig) edge-flows))))
          ;; Verify: sum of flows out of source = total flow
          (let ((source-out 0))
            (dolist (ef edge-flows)
              (when (eq (car ef) 's)
                (setq source-out (+ source-out (nth 2 ef)))))
            (list flow
                  (= source-out flow)
                  (sort (nreverse edge-flows)
                        (lambda (a b) (string< (format "%s%s" (car a) (cadr a))
                                               (format "%s%s" (car b) (cadr b)))))))))
    (fmakunbound 'neovm--nfr-key)
    (fmakunbound 'neovm--nfr-bfs)
    (fmakunbound 'neovm--nfr-maxflow-with-residual)))"#;
    let expect = expect_test::expect![[
        r#""OK (14 t ((a b 0 5) (a c 7 7) (b d 7 10) (c d 0 6) (c t 7 10) (d t 7 7) (s a 7 10) (s b 7 8)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Min-cut from max-flow
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_network_flow_min_cut() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // After max-flow, the min-cut is: nodes reachable from source in residual graph
    // form the S-side. All edges from S-side to T-side are saturated (cut edges).
    let form = r#"(progn
  (fset 'neovm--mc-key
    (lambda (u v) (concat (symbol-name u) "->" (symbol-name v))))

  (fset 'neovm--mc-bfs
    (lambda (adj rcap source sink)
      (let ((visited (make-hash-table))
            (parent (make-hash-table))
            (queue (list source)))
        (puthash source t visited)
        (let ((found nil))
          (while (and queue (not found))
            (let ((u (car queue)))
              (setq queue (cdr queue))
              (dolist (v (gethash u adj nil))
                (when (and (> (gethash (funcall 'neovm--mc-key u v) rcap 0) 0)
                           (not (gethash v visited)))
                  (puthash v t visited)
                  (puthash v u parent)
                  (setq queue (append queue (list v)))
                  (when (eq v sink) (setq found t))))))
          (if found parent nil)))))

  ;; BFS reachability in residual graph (no sink target, just explore)
  (fset 'neovm--mc-reachable
    (lambda (adj rcap source)
      (let ((visited (make-hash-table))
            (queue (list source)))
        (puthash source t visited)
        (while queue
          (let ((u (car queue)))
            (setq queue (cdr queue))
            (dolist (v (gethash u adj nil))
              (when (and (> (gethash (funcall 'neovm--mc-key u v) rcap 0) 0)
                         (not (gethash v visited)))
                (puthash v t visited)
                (setq queue (append queue (list v)))))))
        visited)))

  (fset 'neovm--mc-compute
    (lambda (edges source sink all-nodes)
      (let ((rcap (make-hash-table :test 'equal))
            (adj (make-hash-table))
            (total-flow 0))
        (dolist (e edges)
          (let ((u (nth 0 e)) (v (nth 1 e)) (c (nth 2 e)))
            (puthash (funcall 'neovm--mc-key u v)
                     (+ (gethash (funcall 'neovm--mc-key u v) rcap 0) c) rcap)
            (unless (gethash (funcall 'neovm--mc-key v u) rcap)
              (puthash (funcall 'neovm--mc-key v u) 0 rcap))
            (unless (memq v (gethash u adj nil))
              (puthash u (cons v (gethash u adj nil)) adj))
            (unless (memq u (gethash v adj nil))
              (puthash v (cons u (gethash v adj nil)) adj))))
        ;; Run max-flow
        (let ((continue t))
          (while continue
            (let ((parent (funcall 'neovm--mc-bfs adj rcap source sink)))
              (if (null parent) (setq continue nil)
                (let ((bottleneck most-positive-fixnum) (v sink))
                  (while (not (eq v source))
                    (let ((u (gethash v parent)))
                      (setq bottleneck (min bottleneck (gethash (funcall 'neovm--mc-key u v) rcap 0)))
                      (setq v u)))
                  (setq v sink)
                  (while (not (eq v source))
                    (let ((u (gethash v parent)))
                      (puthash (funcall 'neovm--mc-key u v)
                               (- (gethash (funcall 'neovm--mc-key u v) rcap) bottleneck) rcap)
                      (puthash (funcall 'neovm--mc-key v u)
                               (+ (gethash (funcall 'neovm--mc-key v u) rcap) bottleneck) rcap)
                      (setq v u)))
                  (setq total-flow (+ total-flow bottleneck)))))))
        ;; Find min-cut: S-side = reachable from source in residual
        (let* ((reachable (funcall 'neovm--mc-reachable adj rcap source))
               (s-side nil) (t-side nil))
          (dolist (n all-nodes)
            (if (gethash n reachable) (setq s-side (cons n s-side))
              (setq t-side (cons n t-side))))
          ;; Cut capacity = sum of original capacities crossing from S to T
          (let ((cut-cap 0))
            (dolist (e edges)
              (let ((u (nth 0 e)) (v (nth 1 e)) (c (nth 2 e)))
                (when (and (gethash u reachable) (not (gethash v reachable)))
                  (setq cut-cap (+ cut-cap c)))))
            (list total-flow cut-cap
                  (= total-flow cut-cap)  ;; max-flow min-cut theorem
                  (sort s-side (lambda (a b) (string< (symbol-name a) (symbol-name b))))
                  (sort t-side (lambda (a b) (string< (symbol-name a) (symbol-name b))))))))))

  (unwind-protect
      (funcall 'neovm--mc-compute
               '((s a 10) (s b 8) (a b 5) (a c 7) (b d 10) (c t 10) (d t 7) (c d 6))
               's 't '(s a b c d t))
    (fmakunbound 'neovm--mc-key)
    (fmakunbound 'neovm--mc-bfs)
    (fmakunbound 'neovm--mc-reachable)
    (fmakunbound 'neovm--mc-compute)))"#;
    let expect = expect_test::expect![[r#""OK (14 14 t (a b d s) (c t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple sources / sinks via super-source / super-sink
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_network_flow_multi_source_sink() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Reduce multi-source/multi-sink to single source/sink by adding
    // super-source and super-sink with infinite-ish capacity
    let form = r#"(progn
  (fset 'neovm--ms-key
    (lambda (u v) (concat (symbol-name u) "->" (symbol-name v))))

  (fset 'neovm--ms-bfs
    (lambda (adj rcap source sink)
      (let ((visited (make-hash-table))
            (parent (make-hash-table))
            (queue (list source)))
        (puthash source t visited)
        (let ((found nil))
          (while (and queue (not found))
            (let ((u (car queue)))
              (setq queue (cdr queue))
              (dolist (v (gethash u adj nil))
                (when (and (> (gethash (funcall 'neovm--ms-key u v) rcap 0) 0)
                           (not (gethash v visited)))
                  (puthash v t visited)
                  (puthash v u parent)
                  (setq queue (append queue (list v)))
                  (when (eq v sink) (setq found t))))))
          (if found parent nil)))))

  (fset 'neovm--ms-maxflow
    (lambda (edges source sink)
      (let ((rcap (make-hash-table :test 'equal))
            (adj (make-hash-table))
            (total-flow 0))
        (dolist (e edges)
          (let ((u (nth 0 e)) (v (nth 1 e)) (c (nth 2 e)))
            (puthash (funcall 'neovm--ms-key u v)
                     (+ (gethash (funcall 'neovm--ms-key u v) rcap 0) c) rcap)
            (unless (gethash (funcall 'neovm--ms-key v u) rcap)
              (puthash (funcall 'neovm--ms-key v u) 0 rcap))
            (unless (memq v (gethash u adj nil))
              (puthash u (cons v (gethash u adj nil)) adj))
            (unless (memq u (gethash v adj nil))
              (puthash v (cons u (gethash v adj nil)) adj))))
        (let ((continue t))
          (while continue
            (let ((parent (funcall 'neovm--ms-bfs adj rcap source sink)))
              (if (null parent) (setq continue nil)
                (let ((bottleneck most-positive-fixnum) (v sink))
                  (while (not (eq v source))
                    (let ((u (gethash v parent)))
                      (setq bottleneck (min bottleneck (gethash (funcall 'neovm--ms-key u v) rcap 0)))
                      (setq v u)))
                  (setq v sink)
                  (while (not (eq v source))
                    (let ((u (gethash v parent)))
                      (puthash (funcall 'neovm--ms-key u v)
                               (- (gethash (funcall 'neovm--ms-key u v) rcap) bottleneck) rcap)
                      (puthash (funcall 'neovm--ms-key v u)
                               (+ (gethash (funcall 'neovm--ms-key v u) rcap) bottleneck) rcap)
                      (setq v u)))
                  (setq total-flow (+ total-flow bottleneck)))))))
        total-flow)))

  (unwind-protect
      ;; Two sources s1, s2; two sinks t1, t2; internal nodes a, b, c
      ;; Add super-source S connecting to s1 (cap 100) and s2 (cap 100)
      ;; Add super-sink T connected from t1 (cap 100) and t2 (cap 100)
      (let ((edges '(;; Super-source/sink edges
                     (S s1 100) (S s2 100)
                     (t1 T 100) (t2 T 100)
                     ;; Original network
                     (s1 a 10) (s1 b 5)
                     (s2 b 8) (s2 c 12)
                     (a t1 7) (a b 3)
                     (b t1 6) (b t2 4)
                     (c t2 9))))
        (let ((flow (funcall 'neovm--ms-maxflow edges 'S 'T)))
          ;; Total flow should equal sum of what can get through
          (list flow (> flow 0))))
    (fmakunbound 'neovm--ms-key)
    (fmakunbound 'neovm--ms-bfs)
    (fmakunbound 'neovm--ms-maxflow)))"#;
    let expect = expect_test::expect![[r#""OK (26 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bipartite matching via max-flow reduction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_network_flow_bipartite_matching() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Reduce bipartite matching to max-flow:
    // source -> left nodes (cap 1), left->right (cap 1 if edge exists), right -> sink (cap 1)
    let form = r#"(progn
  (fset 'neovm--bm-key
    (lambda (u v) (concat (prin1-to-string u) "->" (prin1-to-string v))))

  (fset 'neovm--bm-bfs
    (lambda (adj rcap source sink)
      (let ((visited (make-hash-table :test 'equal))
            (parent (make-hash-table :test 'equal))
            (queue (list source)))
        (puthash source t visited)
        (let ((found nil))
          (while (and queue (not found))
            (let ((u (car queue)))
              (setq queue (cdr queue))
              (dolist (v (gethash u adj nil))
                (when (and (> (gethash (funcall 'neovm--bm-key u v) rcap 0) 0)
                           (not (gethash v visited)))
                  (puthash v t visited)
                  (puthash v u parent)
                  (setq queue (append queue (list v)))
                  (when (equal v sink) (setq found t))))))
          (if found parent nil)))))

  (fset 'neovm--bm-maxflow
    (lambda (edges source sink)
      (let ((rcap (make-hash-table :test 'equal))
            (adj (make-hash-table :test 'equal))
            (total-flow 0))
        (dolist (e edges)
          (let ((u (nth 0 e)) (v (nth 1 e)) (c (nth 2 e)))
            (puthash (funcall 'neovm--bm-key u v)
                     (+ (gethash (funcall 'neovm--bm-key u v) rcap 0) c) rcap)
            (unless (gethash (funcall 'neovm--bm-key v u) rcap)
              (puthash (funcall 'neovm--bm-key v u) 0 rcap))
            (let ((u-adj (gethash u adj nil)))
              (unless (member v u-adj)
                (puthash u (cons v u-adj) adj)))
            (let ((v-adj (gethash v adj nil)))
              (unless (member u v-adj)
                (puthash v (cons u v-adj) adj)))))
        (let ((continue t))
          (while continue
            (let ((parent (funcall 'neovm--bm-bfs adj rcap source sink)))
              (if (null parent) (setq continue nil)
                (let ((bottleneck most-positive-fixnum) (v sink))
                  (while (not (equal v source))
                    (let ((u (gethash v parent)))
                      (setq bottleneck (min bottleneck (gethash (funcall 'neovm--bm-key u v) rcap 0)))
                      (setq v u)))
                  (setq v sink)
                  (while (not (equal v source))
                    (let ((u (gethash v parent)))
                      (puthash (funcall 'neovm--bm-key u v)
                               (- (gethash (funcall 'neovm--bm-key u v) rcap) bottleneck) rcap)
                      (puthash (funcall 'neovm--bm-key v u)
                               (+ (gethash (funcall 'neovm--bm-key v u) rcap) bottleneck) rcap)
                      (setq v u)))
                  (setq total-flow (+ total-flow bottleneck)))))))
        total-flow)))

  (unwind-protect
      ;; Bipartite graph: workers {w1,w2,w3} and jobs {j1,j2,j3}
      ;; w1 can do j1,j2; w2 can do j2,j3; w3 can do j1,j3
      (let* ((left '(w1 w2 w3))
             (right '(j1 j2 j3))
             (matches '((w1 j1) (w1 j2) (w2 j2) (w2 j3) (w3 j1) (w3 j3)))
             (edges nil))
        ;; source -> left (cap 1 each)
        (dolist (l left)
          (setq edges (cons (list 'source l 1) edges)))
        ;; left -> right (cap 1 for each match)
        (dolist (m matches)
          (setq edges (cons (list (car m) (cadr m) 1) edges)))
        ;; right -> sink (cap 1 each)
        (dolist (r right)
          (setq edges (cons (list r 'sink 1) edges)))
        ;; Max matching = max flow
        (let ((matching (funcall 'neovm--bm-maxflow edges 'source 'sink)))
          (list matching
                ;; Perfect matching exists if flow = |left| = |right|
                (= matching (length left))
                (= matching (length right)))))
    (fmakunbound 'neovm--bm-key)
    (fmakunbound 'neovm--bm-bfs)
    (fmakunbound 'neovm--bm-maxflow)))"#;
    let expect = expect_test::expect![[r#""OK (3 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
