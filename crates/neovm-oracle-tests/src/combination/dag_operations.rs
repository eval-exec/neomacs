//! Oracle parity tests for a directed acyclic graph (DAG) implementation in
//! Elisp: node/edge representation, topological ordering, longest path,
//! all paths between two nodes, DAG-based task scheduling with dependencies,
//! and transitive reduction.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// DAG node/edge representation and basic queries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dag_representation_and_queries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // DAG represented as adjacency list in a hash table.
    // Nodes have labels and optional weights stored in a separate table.
    let form = r#"(progn
  (fset 'neovm--dag-create
    (lambda ()
      "Create an empty DAG: (adj-table . node-data-table)."
      (cons (make-hash-table :test 'equal)
            (make-hash-table :test 'equal))))

  (fset 'neovm--dag-add-node
    (lambda (dag node &optional data)
      "Add a node with optional data."
      (let ((adj (car dag))
            (ndata (cdr dag)))
        (unless (gethash node adj)
          (puthash node nil adj))
        (when data
          (puthash node data ndata)))))

  (fset 'neovm--dag-add-edge
    (lambda (dag from to)
      "Add directed edge from FROM to TO."
      (let ((adj (car dag)))
        (unless (gethash from adj) (puthash from nil adj))
        (unless (gethash to adj) (puthash to nil adj))
        (let ((succs (gethash from adj)))
          (unless (member to succs)
            (puthash from (cons to succs) adj))))))

  (fset 'neovm--dag-nodes
    (lambda (dag)
      "Return sorted list of all nodes."
      (let ((nodes nil))
        (maphash (lambda (k _v) (setq nodes (cons k nodes))) (car dag))
        (sort nodes (lambda (a b) (string< (format "%s" a) (format "%s" b)))))))

  (fset 'neovm--dag-successors
    (lambda (dag node)
      "Return sorted list of successors of NODE."
      (sort (copy-sequence (gethash node (car dag) nil))
            (lambda (a b) (string< (format "%s" a) (format "%s" b))))))

  (fset 'neovm--dag-predecessors
    (lambda (dag node)
      "Return sorted list of predecessors of NODE."
      (let ((preds nil))
        (maphash (lambda (k v)
                   (when (member node v)
                     (setq preds (cons k preds))))
                 (car dag))
        (sort preds (lambda (a b) (string< (format "%s" a) (format "%s" b)))))))

  (fset 'neovm--dag-edge-count
    (lambda (dag)
      "Count total number of edges."
      (let ((count 0))
        (maphash (lambda (_k v) (setq count (+ count (length v)))) (car dag))
        count)))

  (unwind-protect
      (let ((g (funcall 'neovm--dag-create)))
        ;; Build a DAG:
        ;;   a -> b, a -> c
        ;;   b -> d, b -> e
        ;;   c -> e, c -> f
        ;;   d -> g
        ;;   e -> g
        ;;   f -> g
        (dolist (n '("a" "b" "c" "d" "e" "f" "g"))
          (funcall 'neovm--dag-add-node g n (list 'weight (length n))))
        (dolist (edge '(("a" "b") ("a" "c") ("b" "d") ("b" "e")
                        ("c" "e") ("c" "f") ("d" "g") ("e" "g") ("f" "g")))
          (funcall 'neovm--dag-add-edge g (car edge) (cadr edge)))
        (list
          (funcall 'neovm--dag-nodes g)
          (funcall 'neovm--dag-successors g "a")
          (funcall 'neovm--dag-successors g "b")
          (funcall 'neovm--dag-successors g "g")
          (funcall 'neovm--dag-predecessors g "g")
          (funcall 'neovm--dag-predecessors g "a")
          (funcall 'neovm--dag-predecessors g "e")
          (funcall 'neovm--dag-edge-count g)
          (length (funcall 'neovm--dag-nodes g))))
    (fmakunbound 'neovm--dag-create)
    (fmakunbound 'neovm--dag-add-node)
    (fmakunbound 'neovm--dag-add-edge)
    (fmakunbound 'neovm--dag-nodes)
    (fmakunbound 'neovm--dag-successors)
    (fmakunbound 'neovm--dag-predecessors)
    (fmakunbound 'neovm--dag-edge-count)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"a\" \"b\" \"c\" \"d\" \"e\" \"f\" \"g\") (\"b\" \"c\") (\"d\" \"e\") nil (\"d\" \"e\" \"f\") nil (\"b\" \"c\") 9 7)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Topological ordering via DFS-based algorithm
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dag_topological_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--dag-topo-dfs
    (lambda (adj-table)
      "DFS-based topological sort. Returns sorted list or 'cycle."
      (let ((visited (make-hash-table :test 'equal))
            (in-stack (make-hash-table :test 'equal))
            (result nil)
            (has-cycle nil)
            (all-nodes nil))
        ;; Collect all nodes
        (maphash (lambda (k _v) (setq all-nodes (cons k all-nodes))) adj-table)
        (setq all-nodes (sort all-nodes
                              (lambda (a b) (string< (format "%s" a) (format "%s" b)))))
        ;; DFS visit function (iterative to avoid stack overflow)
        (fset 'neovm--dag-topo-visit
          (lambda (start)
            (let ((stack (list (cons start 'enter))))
              (while (and stack (not has-cycle))
                (let* ((item (car stack))
                       (node (car item))
                       (phase (cdr item)))
                  (setq stack (cdr stack))
                  (cond
                    ((eq phase 'exit)
                     (puthash node nil in-stack)
                     (setq result (cons node result)))
                    ((gethash node visited)
                     ;; Already fully processed
                     nil)
                    ((gethash node in-stack)
                     ;; Back edge = cycle
                     (setq has-cycle t))
                    (t
                     (puthash node t visited)
                     (puthash node t in-stack)
                     ;; Push exit marker first (processed after children)
                     (setq stack (cons (cons node 'exit) stack))
                     ;; Push children in reverse sorted order so smallest is processed first
                     (let ((succs (sort (copy-sequence (gethash node adj-table nil))
                                        (lambda (a b) (string> (format "%s" a) (format "%s" b))))))
                       (dolist (s succs)
                         (setq stack (cons (cons s 'enter) stack)))))))))))
        ;; Visit all nodes
        (dolist (node all-nodes)
          (unless (or has-cycle (gethash node visited))
            (funcall 'neovm--dag-topo-visit node)))
        (fmakunbound 'neovm--dag-topo-visit)
        (if has-cycle 'cycle result))))

  (unwind-protect
      (let ((adj (make-hash-table :test 'equal)))
        ;; Build diamond DAG: a->{b,c}, b->d, c->d, d->e
        (puthash "a" '("b" "c") adj)
        (puthash "b" '("d") adj)
        (puthash "c" '("d") adj)
        (puthash "d" '("e") adj)
        (puthash "e" nil adj)
        (let ((topo1 (funcall 'neovm--dag-topo-dfs adj)))
          ;; Verify ordering properties
          (let ((pos (make-hash-table :test 'equal))
                (i 0))
            (dolist (n topo1)
              (puthash n i pos)
              (setq i (1+ i)))
            (list
              topo1
              ;; a before b and c
              (< (gethash "a" pos) (gethash "b" pos))
              (< (gethash "a" pos) (gethash "c" pos))
              ;; b and c before d
              (< (gethash "b" pos) (gethash "d" pos))
              (< (gethash "c" pos) (gethash "d" pos))
              ;; d before e
              (< (gethash "d" pos) (gethash "e" pos))
              ;; Test with cycle
              (let ((cyclic (make-hash-table :test 'equal)))
                (puthash "x" '("y") cyclic)
                (puthash "y" '("z") cyclic)
                (puthash "z" '("x") cyclic)
                (funcall 'neovm--dag-topo-dfs cyclic))))))
    (fmakunbound 'neovm--dag-topo-dfs)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"a\" \"c\" \"b\" \"d\" \"e\") t t t t t (\"x\" \"y\" \"z\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Longest path in a weighted DAG
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dag_longest_path() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--dag-longest-path
    (lambda (adj-table weights)
      "Find longest path length in weighted DAG using topological order + DP.
       ADJ-TABLE: node -> successors. WEIGHTS: node -> weight (integer).
       Returns (max-length . path-as-list)."
      ;; First, topological sort (Kahn's)
      (let ((in-degree (make-hash-table :test 'equal))
            (all-nodes nil))
        (maphash (lambda (k _v)
                   (unless (gethash k in-degree) (puthash k 0 in-degree))
                   (setq all-nodes (cons k all-nodes)))
                 adj-table)
        (maphash (lambda (_k succs)
                   (dolist (s succs)
                     (puthash s (1+ (gethash s in-degree 0)) in-degree)
                     (unless (member s all-nodes) (setq all-nodes (cons s all-nodes)))))
                 adj-table)
        ;; Deduplicate
        (let ((seen (make-hash-table :test 'equal)) (unique nil))
          (dolist (n all-nodes)
            (unless (gethash n seen) (puthash n t seen) (setq unique (cons n unique))))
          (setq all-nodes (sort unique (lambda (a b) (string< (format "%s" a) (format "%s" b))))))
        ;; Kahn's
        (let ((queue nil) (topo nil))
          (dolist (n all-nodes)
            (when (= 0 (gethash n in-degree 0))
              (setq queue (nconc queue (list n)))))
          (while queue
            (let ((node (car queue)))
              (setq queue (cdr queue))
              (setq topo (cons node topo))
              (dolist (s (sort (copy-sequence (gethash node adj-table nil))
                               (lambda (a b) (string< (format "%s" a) (format "%s" b)))))
                (puthash s (1- (gethash s in-degree)) in-degree)
                (when (= 0 (gethash s in-degree))
                  (setq queue (nconc queue (list s)))))))
          (setq topo (nreverse topo))
          ;; DP: dist[v] = max distance ending at v, prev[v] = predecessor
          (let ((dist (make-hash-table :test 'equal))
                (prev (make-hash-table :test 'equal)))
            (dolist (n topo)
              (puthash n (gethash n weights 0) dist))
            (dolist (u topo)
              (dolist (v (gethash u adj-table nil))
                (let ((new-dist (+ (gethash u dist 0) (gethash v weights 0))))
                  (when (> new-dist (gethash v dist 0))
                    (puthash v new-dist dist)
                    (puthash v u prev)))))
            ;; Find node with maximum distance
            (let ((max-node nil) (max-dist -1))
              (dolist (n topo)
                (when (> (gethash n dist 0) max-dist)
                  (setq max-dist (gethash n dist 0))
                  (setq max-node n)))
              ;; Reconstruct path
              (let ((path nil) (cur max-node))
                (while cur
                  (setq path (cons cur path))
                  (setq cur (gethash cur prev nil)))
                (cons max-dist path))))))))

  (unwind-protect
      (let ((adj (make-hash-table :test 'equal))
            (weights (make-hash-table :test 'equal)))
        ;; DAG: a(3)->b(5)->d(2)->f(4)
        ;;       a(3)->c(7)->e(1)->f(4)
        ;;       b(5)->e(1)
        (puthash "a" '("b" "c") adj)
        (puthash "b" '("d" "e") adj)
        (puthash "c" '("e") adj)
        (puthash "d" '("f") adj)
        (puthash "e" '("f") adj)
        (puthash "f" nil adj)
        (puthash "a" 3 weights)
        (puthash "b" 5 weights)
        (puthash "c" 7 weights)
        (puthash "d" 2 weights)
        (puthash "e" 1 weights)
        (puthash "f" 4 weights)
        (let ((result (funcall 'neovm--dag-longest-path adj weights)))
          (list
            ;; Max path length
            (car result)
            ;; Path itself
            (cdr result)
            ;; Verify: a(3)->c(7)->e(1)->f(4) = 15
            ;; vs a(3)->b(5)->d(2)->f(4) = 14
            ;; vs a(3)->b(5)->e(1)->f(4) = 13
            (= (car result) 15))))
    (fmakunbound 'neovm--dag-longest-path)))"#;
    let expect = expect_test::expect![[r#""OK (15 (\"a\" \"c\" \"e\" \"f\") t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// All paths between two nodes in a DAG
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dag_all_paths() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--dag-all-paths
    (lambda (adj-table start end)
      "Find all paths from START to END in DAG. Returns list of paths."
      (let ((all-paths nil))
        (fset 'neovm--dag-dfs-paths
          (lambda (current path)
            (let ((new-path (append path (list current))))
              (if (equal current end)
                  (setq all-paths (cons new-path all-paths))
                (dolist (succ (sort (copy-sequence (gethash current adj-table nil))
                                    (lambda (a b) (string< (format "%s" a) (format "%s" b)))))
                  (funcall 'neovm--dag-dfs-paths succ new-path))))))
        (funcall 'neovm--dag-dfs-paths start nil)
        (fmakunbound 'neovm--dag-dfs-paths)
        ;; Sort paths lexicographically for deterministic output
        (sort all-paths
              (lambda (a b)
                (let ((sa (mapconcat (lambda (x) (format "%s" x)) a ","))
                      (sb (mapconcat (lambda (x) (format "%s" x)) b ",")))
                  (string< sa sb)))))))

  (unwind-protect
      (let ((adj (make-hash-table :test 'equal)))
        ;; DAG with multiple paths:
        ;;   s -> a -> c -> t
        ;;   s -> a -> d -> t
        ;;   s -> b -> d -> t
        ;;   s -> b -> e -> t
        (puthash "s" '("a" "b") adj)
        (puthash "a" '("c" "d") adj)
        (puthash "b" '("d" "e") adj)
        (puthash "c" '("t") adj)
        (puthash "d" '("t") adj)
        (puthash "e" '("t") adj)
        (puthash "t" nil adj)
        (let ((paths (funcall 'neovm--dag-all-paths adj "s" "t")))
          (list
            ;; All paths
            paths
            ;; Number of paths
            (length paths)
            ;; Shortest path length
            (apply #'min (mapcar #'length paths))
            ;; Longest path length
            (apply #'max (mapcar #'length paths))
            ;; No path from t to s (wrong direction)
            (funcall 'neovm--dag-all-paths adj "t" "s")
            ;; Path from s to s (trivial)
            (funcall 'neovm--dag-all-paths adj "s" "s")
            ;; Paths from a to t
            (funcall 'neovm--dag-all-paths adj "a" "t"))))
    (fmakunbound 'neovm--dag-all-paths)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"s\" \"a\" \"c\" \"t\") (\"s\" \"a\" \"d\" \"t\") (\"s\" \"b\" \"d\" \"t\") (\"s\" \"b\" \"e\" \"t\")) 4 4 4 nil ((\"s\")) ((\"a\" \"c\" \"t\") (\"a\" \"d\" \"t\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// DAG-based task scheduling with dependencies and critical path
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dag_task_scheduling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--dag-schedule
    (lambda (tasks)
      "Schedule tasks respecting dependencies.
       TASKS: list of (name duration . dep-names).
       Returns ((name start end) ...) sorted by start time, then name."
      (let ((adj (make-hash-table :test 'equal))
            (dur (make-hash-table :test 'equal))
            (in-degree (make-hash-table :test 'equal))
            (all-tasks nil))
        ;; Build reverse DAG: dep -> task (dep must finish before task starts)
        (dolist (task tasks)
          (let ((name (nth 0 task))
                (duration (nth 1 task))
                (deps (nthcdr 2 task)))
            (puthash name duration dur)
            (unless (gethash name adj) (puthash name nil adj))
            (puthash name 0 in-degree)
            (setq all-tasks (cons name all-tasks))
            (dolist (d deps)
              (unless (gethash d adj) (puthash d nil adj))
              (puthash d (cons name (gethash d adj nil)) adj)
              (puthash name (1+ (gethash name in-degree 0)) in-degree))))
        (setq all-tasks (sort all-tasks (lambda (a b) (string< a b))))
        ;; Topological sort
        (let ((queue nil) (order nil))
          (dolist (t all-tasks)
            (when (= 0 (gethash t in-degree 0))
              (setq queue (nconc queue (list t)))))
          (while queue
            (let ((node (car queue)))
              (setq queue (cdr queue))
              (setq order (cons node order))
              (dolist (s (sort (copy-sequence (gethash node adj nil))
                               (lambda (a b) (string< a b))))
                (puthash s (1- (gethash s in-degree)) in-degree)
                (when (= 0 (gethash s in-degree))
                  (setq queue (nconc queue (list s)))))))
          (setq order (nreverse order))
          ;; Compute earliest start times
          (let ((start-time (make-hash-table :test 'equal)))
            (dolist (t order) (puthash t 0 start-time))
            (dolist (t order)
              (let ((finish (+ (gethash t start-time 0) (gethash t dur 0))))
                (dolist (s (gethash t adj nil))
                  (when (> finish (gethash s start-time 0))
                    (puthash s finish start-time)))))
            ;; Build result
            (let ((result nil)
                  (makespan 0))
              (dolist (t order)
                (let ((s (gethash t start-time 0))
                      (d (gethash t dur 0)))
                  (setq result (cons (list t s (+ s d)) result))
                  (when (> (+ s d) makespan)
                    (setq makespan (+ s d)))))
              (list
                (sort (nreverse result)
                      (lambda (a b)
                        (if (= (nth 1 a) (nth 1 b))
                            (string< (nth 0 a) (nth 0 b))
                          (< (nth 1 a) (nth 1 b)))))
                makespan)))))))

  (unwind-protect
      (let ((tasks '(("compile"   2)
                      ("lint"      1)
                      ("test-unit" 3 "compile")
                      ("test-int"  4 "compile")
                      ("docs"      2 "lint")
                      ("package"   1 "test-unit" "test-int" "docs")
                      ("deploy"    2 "package"))))
        (let ((schedule (funcall 'neovm--dag-schedule tasks)))
          (let ((timeline (car schedule))
                (makespan (cadr schedule)))
            (list
              timeline
              makespan
              ;; Verify: compile starts at 0
              (nth 1 (assoc "compile" timeline))
              ;; lint starts at 0 (no deps)
              (nth 1 (assoc "lint" timeline))
              ;; test-unit starts after compile finishes (t=2)
              (nth 1 (assoc "test-unit" timeline))
              ;; package starts after all 3 deps
              (nth 1 (assoc "package" timeline))))))
    (fmakunbound 'neovm--dag-schedule)))"#;
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Transitive reduction of a DAG
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dag_transitive_reduction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--dag-trans-reduce
    (lambda (adj-table)
      "Compute transitive reduction: remove edge u->v if there exists
       a longer path u->...->v. Returns new adjacency table."
      ;; For each node u and each successor v, check if v is reachable
      ;; from u through other successors (without using the direct u->v edge)
      (let ((result (make-hash-table :test 'equal)))
        ;; Initialize result with all nodes
        (maphash (lambda (k _v) (puthash k nil result)) adj-table)
        ;; For each edge u->v, BFS from u excluding direct edge to v
        (maphash
          (lambda (u succs)
            (dolist (v succs)
              ;; BFS from u's OTHER successors to see if v is reachable
              (let ((visited (make-hash-table :test 'equal))
                    (queue nil)
                    (reachable nil))
                ;; Start from u's successors except v
                (dolist (s succs)
                  (unless (equal s v)
                    (unless (gethash s visited)
                      (puthash s t visited)
                      (setq queue (nconc queue (list s))))))
                ;; BFS
                (while (and queue (not reachable))
                  (let ((cur (car queue)))
                    (setq queue (cdr queue))
                    (when (equal cur v)
                      (setq reachable t))
                    (dolist (s (gethash cur adj-table nil))
                      (unless (gethash s visited)
                        (puthash s t visited)
                        (setq queue (nconc queue (list s)))))))
                ;; Keep edge only if v not reachable through other paths
                (unless reachable
                  (puthash u (cons v (gethash u result nil)) result)))))
          adj-table)
        ;; Sort successor lists for determinism
        (let ((sorted-result nil))
          (maphash (lambda (k v)
                     (setq sorted-result
                           (cons (cons k (sort v (lambda (a b) (string< (format "%s" a) (format "%s" b)))))
                                 sorted-result)))
                   result)
          (sort sorted-result (lambda (a b) (string< (format "%s" (car a)) (format "%s" (car b)))))))))

  (unwind-protect
      (list
        ;; Case 1: a->b, a->c, b->c => a->c is redundant (via b)
        (let ((adj1 (make-hash-table :test 'equal)))
          (puthash "a" '("b" "c") adj1)
          (puthash "b" '("c") adj1)
          (puthash "c" nil adj1)
          (funcall 'neovm--dag-trans-reduce adj1))
        ;; Case 2: diamond with shortcut
        ;; a->b, a->c, a->d, b->d, c->d => a->d redundant
        (let ((adj2 (make-hash-table :test 'equal)))
          (puthash "a" '("b" "c" "d") adj2)
          (puthash "b" '("d") adj2)
          (puthash "c" '("d") adj2)
          (puthash "d" nil adj2)
          (funcall 'neovm--dag-trans-reduce adj2))
        ;; Case 3: chain with all shortcuts
        ;; a->b->c->d, plus a->c, a->d, b->d
        (let ((adj3 (make-hash-table :test 'equal)))
          (puthash "a" '("b" "c" "d") adj3)
          (puthash "b" '("c" "d") adj3)
          (puthash "c" '("d") adj3)
          (puthash "d" nil adj3)
          (funcall 'neovm--dag-trans-reduce adj3))
        ;; Case 4: no redundant edges (already minimal)
        (let ((adj4 (make-hash-table :test 'equal)))
          (puthash "a" '("b") adj4)
          (puthash "b" '("c") adj4)
          (puthash "c" nil adj4)
          (funcall 'neovm--dag-trans-reduce adj4))
        ;; Case 5: complex with multiple redundancies
        ;; a->{b,c,e}, b->{d}, c->{d,e}, d->{e}
        ;; a->e redundant (via b->d->e or c->e)
        ;; c->e redundant? c->d->e, so yes
        (let ((adj5 (make-hash-table :test 'equal)))
          (puthash "a" '("b" "c" "e") adj5)
          (puthash "b" '("d") adj5)
          (puthash "c" '("d" "e") adj5)
          (puthash "d" '("e") adj5)
          (puthash "e" nil adj5)
          (funcall 'neovm--dag-trans-reduce adj5)))
    (fmakunbound 'neovm--dag-trans-reduce)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"a\" \"b\") (\"b\" \"c\") (\"c\")) ((\"a\" \"b\" \"c\") (\"b\" \"d\") (\"c\" \"d\") (\"d\")) ((\"a\" \"b\") (\"b\" \"c\") (\"c\" \"d\") (\"d\")) ((\"a\" \"b\") (\"b\" \"c\") (\"c\")) ((\"a\" \"b\" \"c\") (\"b\" \"d\") (\"c\" \"d\") (\"d\" \"e\") (\"e\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// DAG depth and width computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dag_depth_and_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute the depth (longest path from any source to any sink)
    // and width (maximum number of nodes at the same level) of a DAG
    let form = r#"(progn
  (fset 'neovm--dag-depth-width
    (lambda (adj-table)
      "Compute depth (longest path length) and width (max nodes per level).
       Returns (depth width levels)."
      (let ((in-degree (make-hash-table :test 'equal))
            (all-nodes nil))
        ;; Compute in-degrees
        (maphash (lambda (k _v) (puthash k 0 in-degree) (setq all-nodes (cons k all-nodes)))
                 adj-table)
        (maphash (lambda (_k succs)
                   (dolist (s succs)
                     (puthash s (1+ (gethash s in-degree 0)) in-degree)))
                 adj-table)
        ;; BFS by levels
        (let ((current nil)
              (levels nil)
              (depth 0))
          ;; Sources: in-degree 0
          (dolist (n all-nodes)
            (when (= 0 (gethash n in-degree 0))
              (setq current (cons n current))))
          (setq current (sort current (lambda (a b) (string< (format "%s" a) (format "%s" b)))))
          (while current
            (setq levels (cons current levels))
            (setq depth (1+ depth))
            (let ((next nil))
              (dolist (node current)
                (dolist (s (gethash node adj-table nil))
                  (puthash s (1- (gethash s in-degree)) in-degree)
                  (when (= 0 (gethash s in-degree))
                    (setq next (cons s next)))))
              ;; Deduplicate and sort
              (let ((seen (make-hash-table :test 'equal)) (u nil))
                (dolist (n next)
                  (unless (gethash n seen) (puthash n t seen) (setq u (cons n u))))
                (setq current (sort u (lambda (a b) (string< (format "%s" a) (format "%s" b))))))))
          (let ((width 0))
            (dolist (level levels)
              (when (> (length level) width)
                (setq width (length level))))
            (list depth width (nreverse levels)))))))

  (unwind-protect
      (list
        ;; Linear chain: depth=5, width=1
        (let ((adj (make-hash-table :test 'equal)))
          (puthash "a" '("b") adj)
          (puthash "b" '("c") adj)
          (puthash "c" '("d") adj)
          (puthash "d" '("e") adj)
          (puthash "e" nil adj)
          (funcall 'neovm--dag-depth-width adj))
        ;; Wide and shallow: depth=2, width=4
        (let ((adj (make-hash-table :test 'equal)))
          (puthash "root" '("a" "b" "c" "d") adj)
          (puthash "a" nil adj) (puthash "b" nil adj)
          (puthash "c" nil adj) (puthash "d" nil adj)
          (funcall 'neovm--dag-depth-width adj))
        ;; Diamond: depth=3, width=2
        (let ((adj (make-hash-table :test 'equal)))
          (puthash "top" '("left" "right") adj)
          (puthash "left" '("bottom") adj)
          (puthash "right" '("bottom") adj)
          (puthash "bottom" nil adj)
          (funcall 'neovm--dag-depth-width adj))
        ;; Two independent chains: depth=3, width=2
        (let ((adj (make-hash-table :test 'equal)))
          (puthash "a1" '("a2") adj) (puthash "a2" '("a3") adj) (puthash "a3" nil adj)
          (puthash "b1" '("b2") adj) (puthash "b2" '("b3") adj) (puthash "b3" nil adj)
          (funcall 'neovm--dag-depth-width adj)))
    (fmakunbound 'neovm--dag-depth-width)))"#;
    let expect = expect_test::expect![[
        r#""OK ((5 1 ((\"a\") (\"b\") (\"c\") (\"d\") (\"e\"))) (2 4 ((\"root\") (\"a\" \"b\" \"c\" \"d\"))) (3 2 ((\"top\") (\"left\" \"right\") (\"bottom\"))) (3 2 ((\"a1\" \"b1\") (\"a2\" \"b2\") (\"a3\" \"b3\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
