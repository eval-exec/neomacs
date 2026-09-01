//! Oracle parity tests for advanced topological sorting algorithms in Elisp:
//! Kahn's algorithm (BFS-based), DFS-based topological sort, all topological
//! orderings enumeration, cycle detection with error reporting, lexicographically
//! smallest topological order, and package dependency resolution with version
//! constraints.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Kahn's algorithm (BFS-based) with detailed step tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_toposort_adv_kahn_with_steps() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Kahn's algorithm that records each step (which node was dequeued and
    // the state of the queue at that point) for debugging/verification.
    let form = r#"(progn
  (fset 'neovm--tsa-kahn-steps
    (lambda (dag)
      "Kahn's algorithm returning (result steps) where steps is a log of
       each dequeue action."
      (let ((in-degree (make-hash-table :test 'equal))
            (adj (make-hash-table :test 'equal))
            (all-nodes nil))
        ;; Build graph
        (dolist (entry dag)
          (let ((node (car entry)) (succs (cdr entry)))
            (unless (gethash node in-degree) (puthash node 0 in-degree))
            (puthash node succs adj)
            (setq all-nodes (cons node all-nodes))
            (dolist (s succs)
              (unless (gethash s in-degree) (puthash s 0 in-degree))
              (puthash s (1+ (gethash s in-degree 0)) in-degree)
              (unless (gethash s adj) (puthash s nil adj)
                      (setq all-nodes (cons s all-nodes))))))
        ;; Deduplicate and sort
        (let ((seen (make-hash-table :test 'equal)) (unique nil))
          (dolist (n all-nodes)
            (unless (gethash n seen) (puthash n t seen) (setq unique (cons n unique))))
          (setq all-nodes (sort unique (lambda (a b) (string< a b)))))
        ;; BFS
        (let ((queue nil) (result nil) (steps nil)
              (count 0) (total (length all-nodes)))
          (dolist (n all-nodes)
            (when (= (gethash n in-degree 0) 0)
              (setq queue (nconc queue (list n)))))
          (while queue
            ;; Sort queue for determinism
            (setq queue (sort queue (lambda (a b) (string< a b))))
            (let ((node (car queue)))
              (setq queue (cdr queue))
              ;; Log the step
              (setq steps (cons (list 'dequeue node 'remaining (copy-sequence queue)) steps))
              (setq result (cons node result))
              (setq count (1+ count))
              (dolist (s (sort (copy-sequence (gethash node adj nil))
                               (lambda (a b) (string< a b))))
                (puthash s (1- (gethash s in-degree)) in-degree)
                (when (= (gethash s in-degree) 0)
                  (setq queue (nconc queue (list s)))))))
          (list
            (if (= count total) 'ok 'cycle)
            (nreverse result)
            (nreverse steps))))))

  (unwind-protect
      (list
        ;; Diamond: a->{b,c}, b->d, c->d
        (funcall 'neovm--tsa-kahn-steps
                 '(("a" . ("b" "c"))
                   ("b" . ("d"))
                   ("c" . ("d"))
                   ("d" . ())))
        ;; Wide parallel: {a,b,c} all independent
        (funcall 'neovm--tsa-kahn-steps
                 '(("a" . ()) ("b" . ()) ("c" . ()))))
    (fmakunbound 'neovm--tsa-kahn-steps)))"#;
    let expect = expect_test::expect![[
        r#""OK ((ok (\"a\" \"b\" \"c\" \"d\") ((dequeue \"a\" remaining nil) (dequeue \"b\" remaining (\"c\")) (dequeue \"c\" remaining nil) (dequeue \"d\" remaining nil))) (ok (\"a\" \"b\" \"c\") ((dequeue \"a\" remaining (\"b\" \"c\")) (dequeue \"b\" remaining (\"c\")) (dequeue \"c\" remaining nil))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// DFS-based topological sort with finish-time ordering
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_toposort_adv_dfs_based() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // DFS-based topological sort: visit nodes using depth-first search,
    // push to result in reverse finish order.  Detect cycles via gray/black
    // coloring (white=unvisited, gray=in-progress, black=done).
    let form = r#"(progn
  (fset 'neovm--tsa-dfs-toposort
    (lambda (dag)
      "DFS-based topological sort. Returns (ok . sorted) or (cycle . node)."
      (let ((adj (make-hash-table :test 'equal))
            (color (make-hash-table :test 'equal))
            (result nil)
            (cycle-node nil)
            (all-nodes nil))
        ;; Build adjacency and collect all nodes
        (dolist (entry dag)
          (puthash (car entry) (cdr entry) adj)
          (setq all-nodes (cons (car entry) all-nodes))
          (dolist (s (cdr entry))
            (unless (gethash s adj) (puthash s nil adj))
            (setq all-nodes (cons s all-nodes))))
        ;; Deduplicate and sort for determinism
        (let ((seen (make-hash-table :test 'equal)) (unique nil))
          (dolist (n all-nodes)
            (unless (gethash n seen) (puthash n t seen) (setq unique (cons n unique))))
          (setq all-nodes (sort unique (lambda (a b) (string< a b)))))
        ;; Initialize all to white
        (dolist (n all-nodes)
          (puthash n 'white color))
        ;; DFS visit function (iterative to avoid stack overflow)
        (fset 'neovm--tsa-dfs-visit
          (lambda (start)
            (let ((stack (list (cons start 'enter))))
              (while (and stack (not cycle-node))
                (let* ((frame (car stack))
                       (node (car frame))
                       (phase (cdr frame)))
                  (setq stack (cdr stack))
                  (cond
                   ((eq phase 'enter)
                    (let ((c (gethash node color)))
                      (cond
                       ((eq c 'black) nil)  ;; already done
                       ((eq c 'gray)
                        (setq cycle-node node))  ;; back edge = cycle
                       (t ;; white
                        (puthash node 'gray color)
                        ;; Push exit frame, then children
                        (setq stack (cons (cons node 'exit) stack))
                        (dolist (s (sort (copy-sequence (gethash node adj nil))
                                        (lambda (a b) (string> a b))))  ;; reverse so first child processed first
                          (setq stack (cons (cons s 'enter) stack)))))))
                   ((eq phase 'exit)
                    (puthash node 'black color)
                    (setq result (cons node result)))))))))
        ;; Visit all nodes
        (dolist (n all-nodes)
          (when (and (not cycle-node) (eq (gethash n color) 'white))
            (funcall 'neovm--tsa-dfs-visit n)))
        (fmakunbound 'neovm--tsa-dfs-visit)
        (if cycle-node
            (cons 'cycle cycle-node)
          (cons 'ok result)))))

  (unwind-protect
      (list
        ;; Linear chain
        (funcall 'neovm--tsa-dfs-toposort
                 '(("a" . ("b")) ("b" . ("c")) ("c" . ("d")) ("d" . ())))
        ;; Diamond
        (funcall 'neovm--tsa-dfs-toposort
                 '(("a" . ("b" "c")) ("b" . ("d")) ("c" . ("d")) ("d" . ())))
        ;; Disconnected components
        (funcall 'neovm--tsa-dfs-toposort
                 '(("a" . ("b")) ("b" . ()) ("c" . ("d")) ("d" . ())))
        ;; Cycle detection
        (funcall 'neovm--tsa-dfs-toposort
                 '(("a" . ("b")) ("b" . ("c")) ("c" . ("a"))))
        ;; Self-loop
        (funcall 'neovm--tsa-dfs-toposort
                 '(("x" . ("x")))))
    (fmakunbound 'neovm--tsa-dfs-toposort)))"#;
    let expect = expect_test::expect![[
        r#""OK ((ok \"a\" \"b\" \"c\" \"d\") (ok \"a\" \"c\" \"b\" \"d\") (ok \"c\" \"d\" \"a\" \"b\") (cycle . \"a\") (cycle . \"x\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// All topological orderings enumeration (backtracking)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_toposort_adv_all_orderings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Enumerate all valid topological orderings using backtracking.
    // For small graphs this is tractable.
    let form = r#"(progn
  (fset 'neovm--tsa-all-toposorts
    (lambda (dag)
      "Return list of ALL valid topological orderings."
      (let ((adj (make-hash-table :test 'equal))
            (in-degree (make-hash-table :test 'equal))
            (all-nodes nil)
            (results nil))
        ;; Build graph
        (dolist (entry dag)
          (let ((node (car entry)) (succs (cdr entry)))
            (unless (gethash node in-degree) (puthash node 0 in-degree))
            (puthash node succs adj)
            (setq all-nodes (cons node all-nodes))
            (dolist (s succs)
              (unless (gethash s in-degree) (puthash s 0 in-degree))
              (puthash s (1+ (gethash s in-degree 0)) in-degree)
              (unless (gethash s adj) (puthash s nil adj)
                      (setq all-nodes (cons s all-nodes))))))
        (let ((seen (make-hash-table :test 'equal)) (unique nil))
          (dolist (n all-nodes)
            (unless (gethash n seen) (puthash n t seen) (setq unique (cons n unique))))
          (setq all-nodes (sort unique (lambda (a b) (string< a b)))))
        ;; Backtracking search
        (fset 'neovm--tsa-bt
          (lambda (path remaining)
            (if (null remaining)
                (setq results (cons (reverse path) results))
              ;; Find all nodes with in-degree 0 among remaining
              (let ((candidates nil))
                (dolist (n remaining)
                  (when (= (gethash n in-degree 0) 0)
                    (setq candidates (cons n candidates))))
                (setq candidates (sort candidates (lambda (a b) (string< a b))))
                ;; Try each candidate
                (dolist (c candidates)
                  ;; Choose c: remove from remaining, decrement successors
                  (let ((new-remaining (delq c (copy-sequence remaining))))
                    (dolist (s (gethash c adj nil))
                      (puthash s (1- (gethash s in-degree)) in-degree))
                    ;; Recurse
                    (funcall 'neovm--tsa-bt (cons c path) new-remaining)
                    ;; Undo: restore in-degrees
                    (dolist (s (gethash c adj nil))
                      (puthash s (1+ (gethash s in-degree)) in-degree))))))))
        (funcall 'neovm--tsa-bt nil (copy-sequence all-nodes))
        (fmakunbound 'neovm--tsa-bt)
        (sort results (lambda (a b) (string< (format "%S" a) (format "%S" b)))))))

  (unwind-protect
      (list
        ;; Diamond: a->{b,c}, b->d, c->d
        ;; Valid orderings: (a b c d) and (a c b d)
        (funcall 'neovm--tsa-all-toposorts
                 '(("a" . ("b" "c")) ("b" . ("d")) ("c" . ("d")) ("d" . ())))
        ;; Three independent nodes: 3! = 6 orderings
        (length (funcall 'neovm--tsa-all-toposorts
                         '(("x" . ()) ("y" . ()) ("z" . ()))))
        ;; Linear chain: only 1 ordering
        (funcall 'neovm--tsa-all-toposorts
                 '(("a" . ("b")) ("b" . ("c")) ("c" . ())))
        ;; Fork: a->{b,c}, b and c independent. 2 orderings.
        (funcall 'neovm--tsa-all-toposorts
                 '(("a" . ("b" "c")) ("b" . ()) ("c" . ()))))
    (fmakunbound 'neovm--tsa-all-toposorts)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"a\" \"b\" \"c\" \"d\") (\"a\" \"c\" \"b\" \"d\")) 6 ((\"a\" \"b\" \"c\")) ((\"a\" \"b\" \"c\") (\"a\" \"c\" \"b\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cycle detection with detailed error reporting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_toposort_adv_cycle_report() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Detect cycles and report the exact cycle path, not just the
    // participating nodes.
    let form = r#"(progn
  (fset 'neovm--tsa-find-cycle
    (lambda (dag)
      "Find a cycle in the DAG and return its path, or nil if acyclic."
      (let ((adj (make-hash-table :test 'equal))
            (color (make-hash-table :test 'equal))
            (parent (make-hash-table :test 'equal))
            (all-nodes nil)
            (cycle-path nil))
        ;; Build graph
        (dolist (entry dag)
          (puthash (car entry) (cdr entry) adj)
          (setq all-nodes (cons (car entry) all-nodes))
          (dolist (s (cdr entry))
            (unless (gethash s adj) (puthash s nil adj))
            (setq all-nodes (cons s all-nodes))))
        (let ((seen (make-hash-table :test 'equal)) (unique nil))
          (dolist (n all-nodes)
            (unless (gethash n seen) (puthash n t seen) (setq unique (cons n unique))))
          (setq all-nodes (sort unique (lambda (a b) (string< a b)))))
        (dolist (n all-nodes) (puthash n 'white color))
        ;; DFS to find back edge
        (fset 'neovm--tsa-fc-dfs
          (lambda (node)
            (puthash node 'gray color)
            (let ((found nil))
              (dolist (s (gethash node adj nil))
                (unless found
                  (cond
                   ((eq (gethash s color) 'gray)
                    ;; Found cycle! Reconstruct path from s back through parents to s
                    (let ((path (list s node))
                          (cur node))
                      (while (not (equal cur s))
                        (setq cur (gethash cur parent))
                        (when cur (setq path (cons cur path))))
                      ;; path starts and ends with s
                      (setq cycle-path (nreverse path))
                      (setq found t)))
                   ((eq (gethash s color) 'white)
                    (puthash s node parent)
                    (funcall 'neovm--tsa-fc-dfs s)
                    (when cycle-path (setq found t))))))
              (puthash node 'black color))))
        (dolist (n all-nodes)
          (when (and (not cycle-path) (eq (gethash n color) 'white))
            (funcall 'neovm--tsa-fc-dfs n)))
        (fmakunbound 'neovm--tsa-fc-dfs)
        cycle-path)))

  (unwind-protect
      (list
        ;; Simple triangle cycle: a->b->c->a
        (funcall 'neovm--tsa-find-cycle
                 '(("a" . ("b")) ("b" . ("c")) ("c" . ("a"))))
        ;; No cycle
        (funcall 'neovm--tsa-find-cycle
                 '(("a" . ("b")) ("b" . ("c")) ("c" . ())))
        ;; Self-loop
        (funcall 'neovm--tsa-find-cycle
                 '(("x" . ("x"))))
        ;; Cycle embedded in larger graph
        (funcall 'neovm--tsa-find-cycle
                 '(("start" . ("a" "d"))
                   ("a" . ("b"))
                   ("b" . ("c"))
                   ("c" . ("a"))
                   ("d" . ())))
        ;; Two separate cycles (should find at least one)
        (let ((result (funcall 'neovm--tsa-find-cycle
                               '(("a" . ("b")) ("b" . ("a"))
                                 ("c" . ("d")) ("d" . ("c"))))))
          (list 'found-cycle (not (null result)))))
    (fmakunbound 'neovm--tsa-find-cycle)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"c\" \"a\" \"b\" \"a\") nil (\"x\" \"x\") (\"c\" \"a\" \"b\" \"a\") (found-cycle t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Lexicographically smallest topological order
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_toposort_adv_lexicographic_smallest() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Always pick the lexicographically smallest available node.
    // This produces a unique, deterministic ordering.
    // Uses a sorted priority queue (simulate with sorted list).
    let form = r#"(progn
  (fset 'neovm--tsa-lex-smallest
    (lambda (dag)
      "Return the lexicographically smallest topological ordering."
      (let ((in-degree (make-hash-table :test 'equal))
            (adj (make-hash-table :test 'equal))
            (all-nodes nil))
        (dolist (entry dag)
          (let ((node (car entry)) (succs (cdr entry)))
            (unless (gethash node in-degree) (puthash node 0 in-degree))
            (puthash node succs adj)
            (setq all-nodes (cons node all-nodes))
            (dolist (s succs)
              (unless (gethash s in-degree) (puthash s 0 in-degree))
              (puthash s (1+ (gethash s in-degree 0)) in-degree)
              (unless (gethash s adj) (puthash s nil adj)
                      (setq all-nodes (cons s all-nodes))))))
        (let ((seen (make-hash-table :test 'equal)) (unique nil))
          (dolist (n all-nodes)
            (unless (gethash n seen) (puthash n t seen) (setq unique (cons n unique))))
          (setq all-nodes (sort unique (lambda (a b) (string< a b)))))
        ;; Priority queue as sorted list — always pick smallest
        (let ((pq nil) (result nil) (count 0) (total (length all-nodes)))
          (dolist (n all-nodes)
            (when (= (gethash n in-degree 0) 0)
              (setq pq (cons n pq))))
          (setq pq (sort pq (lambda (a b) (string< a b))))
          (while pq
            (let ((node (car pq)))
              (setq pq (cdr pq))
              (setq result (cons node result))
              (setq count (1+ count))
              (dolist (s (gethash node adj nil))
                (puthash s (1- (gethash s in-degree)) in-degree)
                (when (= (gethash s in-degree) 0)
                  (setq pq (sort (cons s pq) (lambda (a b) (string< a b))))))))
          (if (= count total)
              (nreverse result)
            'cycle)))))

  (unwind-protect
      (list
        ;; Diamond: a->{b,c}, b->d, c->d → lex smallest is (a b c d)
        (funcall 'neovm--tsa-lex-smallest
                 '(("a" . ("b" "c")) ("b" . ("d")) ("c" . ("d")) ("d" . ())))
        ;; Multiple roots: c,a,b all independent → lex: (a b c)
        (funcall 'neovm--tsa-lex-smallest
                 '(("c" . ()) ("a" . ()) ("b" . ())))
        ;; Complex: a->{d,c}, b->{d}, c->{e}, d->{e}, e->{}
        ;; Lex: pick from {a,b}→a, then {b,c,d}→b, then {c,d}→c, then d, then e
        (funcall 'neovm--tsa-lex-smallest
                 '(("a" . ("d" "c")) ("b" . ("d")) ("c" . ("e"))
                   ("d" . ("e")) ("e" . ())))
        ;; Reverse alphabet edges: z->y->x->w
        (funcall 'neovm--tsa-lex-smallest
                 '(("z" . ("y")) ("y" . ("x")) ("x" . ("w")) ("w" . ())))
        ;; All connected in complex web
        (funcall 'neovm--tsa-lex-smallest
                 '(("f" . ()) ("e" . ("f")) ("d" . ("f"))
                   ("c" . ("d" "e")) ("b" . ("c")) ("a" . ("b" "c")))))
    (fmakunbound 'neovm--tsa-lex-smallest)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"a\" \"b\" \"c\" \"d\") (\"a\" \"b\" \"c\") (\"a\" \"b\" \"c\" \"d\" \"e\") (\"z\" \"y\" \"x\" \"w\") (\"a\" \"b\" \"c\" \"d\" \"e\" \"f\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Package dependency resolution with conflict detection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_toposort_adv_package_dependency() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full package dependency resolver: given package specs with dependencies,
    // compute install order, detect circular dependencies, and identify
    // missing dependencies.
    let form = r#"(progn
  (fset 'neovm--tsa-resolve-deps
    (lambda (packages)
      "PACKAGES: alist of (name . (dep1 dep2 ...)).
       Returns (ok install-order) or (error reason details)."
      ;; First check for missing dependencies
      (let ((known (make-hash-table :test 'equal))
            (missing nil))
        (dolist (p packages)
          (puthash (car p) t known))
        (dolist (p packages)
          (dolist (dep (cdr p))
            (unless (gethash dep known)
              (setq missing (cons (list (car p) 'requires dep) missing)))))
        (if missing
            (list 'error 'missing-deps (nreverse missing))
          ;; Build reverse DAG: dep -> packages-that-need-it
          (let ((adj (make-hash-table :test 'equal))
                (in-degree (make-hash-table :test 'equal))
                (all-nodes nil))
            (dolist (p packages)
              (let ((pkg (car p)) (deps (cdr p)))
                (unless (gethash pkg in-degree) (puthash pkg 0 in-degree))
                (unless (gethash pkg adj) (puthash pkg nil adj))
                (setq all-nodes (cons pkg all-nodes))
                (dolist (d deps)
                  (unless (gethash d adj) (puthash d nil adj))
                  (puthash d (cons pkg (gethash d adj nil)) adj)
                  (puthash pkg (1+ (gethash pkg in-degree 0)) in-degree))))
            (let ((seen (make-hash-table :test 'equal)) (unique nil))
              (dolist (n all-nodes)
                (unless (gethash n seen) (puthash n t seen) (setq unique (cons n unique))))
              (setq all-nodes (sort unique (lambda (a b) (string< a b)))))
            ;; Kahn's algorithm
            (let ((queue nil) (result nil) (count 0) (total (length all-nodes)))
              (dolist (n all-nodes)
                (when (= (gethash n in-degree 0) 0)
                  (setq queue (cons n queue))))
              (setq queue (sort queue (lambda (a b) (string< a b))))
              (while queue
                (setq queue (sort queue (lambda (a b) (string< a b))))
                (let ((node (car queue)))
                  (setq queue (cdr queue))
                  (setq result (cons node result))
                  (setq count (1+ count))
                  (dolist (s (gethash node adj nil))
                    (puthash s (1- (gethash s in-degree)) in-degree)
                    (when (= (gethash s in-degree) 0)
                      (setq queue (cons s queue))))))
              (if (= count total)
                  (list 'ok (nreverse result))
                ;; Circular dependency: find the cycle participants
                (let ((stuck nil))
                  (dolist (n all-nodes)
                    (when (> (gethash n in-degree 0) 0)
                      (setq stuck (cons n stuck))))
                  (list 'error 'circular-deps (sort stuck (lambda (a b) (string< a b))))))))))))

  (unwind-protect
      (list
        ;; Successful resolution
        (funcall 'neovm--tsa-resolve-deps
                 '(("app"     . ("web-framework" "db-driver"))
                   ("web-framework" . ("http-lib" "template-engine"))
                   ("db-driver"     . ("connection-pool"))
                   ("http-lib"      . ())
                   ("template-engine" . ())
                   ("connection-pool" . ())))
        ;; Circular dependency
        (funcall 'neovm--tsa-resolve-deps
                 '(("a" . ("b"))
                   ("b" . ("c"))
                   ("c" . ("a"))))
        ;; Missing dependency
        (funcall 'neovm--tsa-resolve-deps
                 '(("app" . ("missing-lib"))
                   ("other" . ("also-missing"))))
        ;; Single package no deps
        (funcall 'neovm--tsa-resolve-deps
                 '(("standalone" . ())))
        ;; Complex: multiple roots converging
        (funcall 'neovm--tsa-resolve-deps
                 '(("ui"      . ("renderer" "events"))
                   ("cli"     . ("parser" "events"))
                   ("renderer" . ("math-lib"))
                   ("events"  . ())
                   ("parser"  . ())
                   ("math-lib" . ()))))
    (fmakunbound 'neovm--tsa-resolve-deps)))"#;
    let expect = expect_test::expect![[
        r#""OK ((ok (\"connection-pool\" \"db-driver\" \"http-lib\" \"template-engine\" \"web-framework\" \"app\")) (error circular-deps (\"a\" \"b\" \"c\")) (error missing-deps ((\"app\" requires \"missing-lib\") (\"other\" requires \"also-missing\"))) (ok (\"standalone\")) (ok (\"events\" \"math-lib\" \"parser\" \"cli\" \"renderer\" \"ui\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Topological sort with weighted critical path
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_toposort_adv_critical_path() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute the critical path (longest path) through a weighted DAG
    // using topological order.
    let form = r#"(progn
  (fset 'neovm--tsa-critical-path
    (lambda (tasks)
      "TASKS: ((name weight deps...) ...).
       Returns (total-critical-time critical-path-nodes earliest-times)."
      (let ((adj (make-hash-table :test 'equal))
            (weight (make-hash-table :test 'equal))
            (in-degree (make-hash-table :test 'equal))
            (all-nodes nil)
            (earliest (make-hash-table :test 'equal))
            (pred-on-crit (make-hash-table :test 'equal)))
        ;; Build graph: edge from each dep to the task
        (dolist (task tasks)
          (let ((name (car task))
                (w (cadr task))
                (deps (cddr task)))
            (puthash name w weight)
            (unless (gethash name in-degree) (puthash name 0 in-degree))
            (unless (gethash name adj) (puthash name nil adj))
            (setq all-nodes (cons name all-nodes))
            (dolist (d deps)
              (unless (gethash d adj) (puthash d nil adj))
              (puthash d (cons name (gethash d adj nil)) adj)
              (puthash name (1+ (gethash name in-degree 0)) in-degree))))
        ;; Deduplicate
        (let ((seen (make-hash-table :test 'equal)) (unique nil))
          (dolist (n all-nodes)
            (unless (gethash n seen) (puthash n t seen) (setq unique (cons n unique))))
          (setq all-nodes (sort unique (lambda (a b) (string< a b)))))
        ;; Topological sort via Kahn's
        (let ((queue nil) (topo-order nil))
          (dolist (n all-nodes)
            (puthash n 0 earliest)
            (when (= (gethash n in-degree 0) 0)
              (setq queue (cons n queue))))
          (setq queue (sort queue (lambda (a b) (string< a b))))
          (while queue
            (setq queue (sort queue (lambda (a b) (string< a b))))
            (let ((node (car queue)))
              (setq queue (cdr queue))
              (setq topo-order (cons node topo-order))
              (let ((finish (+ (gethash node earliest 0) (gethash node weight 0))))
                (dolist (s (gethash node adj nil))
                  (puthash s (1- (gethash s in-degree)) in-degree)
                  (when (> finish (gethash s earliest 0))
                    (puthash s finish earliest)
                    (puthash s node pred-on-crit))
                  (when (= (gethash s in-degree) 0)
                    (setq queue (cons s queue)))))))
          ;; Find the node with maximum earliest + weight (critical finish)
          (let ((max-finish 0) (max-node nil))
            (dolist (n all-nodes)
              (let ((f (+ (gethash n earliest 0) (gethash n weight 0))))
                (when (> f max-finish)
                  (setq max-finish f)
                  (setq max-node n))))
            ;; Trace back critical path
            (let ((path nil) (cur max-node))
              (while cur
                (setq path (cons cur path))
                (setq cur (gethash cur pred-on-crit)))
              ;; Collect earliest times
              (let ((times nil))
                (dolist (n (nreverse (copy-sequence topo-order)))
                  (setq times (cons (cons n (gethash n earliest 0)) times)))
                (list max-finish path (nreverse times)))))))))

  (unwind-protect
      (list
        ;; Simple pipeline: design(3) -> build(5) -> test(2) -> deploy(1)
        (funcall 'neovm--tsa-critical-path
                 '(("design" 3)
                   ("build" 5 "design")
                   ("test" 2 "build")
                   ("deploy" 1 "test")))
        ;; Parallel paths: critical path is the longer one
        ;; Path 1: a(2) -> b(3) -> d(1) = 6
        ;; Path 2: a(2) -> c(5) -> d(1) = 8 (critical)
        (funcall 'neovm--tsa-critical-path
                 '(("a" 2)
                   ("b" 3 "a")
                   ("c" 5 "a")
                   ("d" 1 "b" "c"))))
    (fmakunbound 'neovm--tsa-critical-path)))"#;
    let expect = expect_test::expect![[
        r#""OK ((11 (\"design\" \"build\" \"test\" \"deploy\") ((\"design\" . 0) (\"build\" . 3) (\"test\" . 8) (\"deploy\" . 10))) (8 (\"a\" \"c\" \"d\") ((\"a\" . 0) (\"b\" . 2) (\"c\" . 2) (\"d\" . 7))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
