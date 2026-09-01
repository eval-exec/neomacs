//! Oracle parity tests for topological sort (Kahn's algorithm) implemented
//! in Elisp: DAG representation, in-degree computation, cycle detection,
//! dependency resolution, task scheduling, and multi-level grouping.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic topological sort — simple DAG with deterministic output
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_toposort_basic_dag() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement Kahn's algorithm. DAG as alist: ((node . (successors ...)) ...)
    // Returns (ok . sorted-list) or (cycle . remaining-nodes).
    // For determinism, always pick the lexicographically smallest among
    // zero-in-degree nodes.
    let form = r#"(progn
  (fset 'neovm--ts-toposort
    (lambda (dag)
      "Topological sort via Kahn's algorithm.
       DAG is an alist of (node . (list of successor nodes)).
       Returns (ok . sorted-list) or (cycle . unsorted-nodes)."
      (let ((in-degree (make-hash-table :test 'equal))
            (adj (make-hash-table :test 'equal))
            (all-nodes nil))
        ;; Collect all nodes and build adjacency + in-degree
        (dolist (entry dag)
          (let ((node (car entry))
                (succs (cdr entry)))
            (unless (gethash node in-degree)
              (puthash node 0 in-degree))
            (puthash node succs adj)
            (setq all-nodes (cons node all-nodes))
            (dolist (s succs)
              (unless (gethash s in-degree)
                (puthash s 0 in-degree))
              (puthash s (1+ (gethash s in-degree 0)) in-degree)
              ;; Ensure successor is in adj even if it has no outgoing edges
              (unless (gethash s adj)
                (puthash s nil adj)
                (setq all-nodes (cons s all-nodes))))))
        ;; Deduplicate all-nodes
        (let ((seen (make-hash-table :test 'equal))
              (unique nil))
          (dolist (n all-nodes)
            (unless (gethash n seen)
              (puthash n t seen)
              (setq unique (cons n unique))))
          (setq all-nodes (sort unique
                                (lambda (a b)
                                  (string< (format "%s" a) (format "%s" b))))))
        ;; Seed queue with zero in-degree nodes (sorted for determinism)
        (let ((queue nil)
              (result nil)
              (count 0)
              (total (length all-nodes)))
          (dolist (n all-nodes)
            (when (= (gethash n in-degree 0) 0)
              (setq queue (nconc queue (list n)))))
          ;; Process
          (while queue
            (let ((node (car queue)))
              (setq queue (cdr queue))
              (setq result (cons node result))
              (setq count (1+ count))
              ;; Decrement in-degree of successors
              (let ((succs (sort (copy-sequence (gethash node adj nil))
                                 (lambda (a b)
                                   (string< (format "%s" a) (format "%s" b))))))
                (dolist (s succs)
                  (puthash s (1- (gethash s in-degree)) in-degree)
                  (when (= (gethash s in-degree) 0)
                    (setq queue (nconc queue (list s))))))))
          ;; Check for cycle
          (if (= count total)
              (cons 'ok (nreverse result))
            ;; Collect remaining (cycle participants)
            (let ((remaining nil))
              (dolist (n all-nodes)
                (when (> (gethash n in-degree 0) 0)
                  (setq remaining (cons n remaining))))
              (cons 'cycle (nreverse remaining))))))))

  (unwind-protect
      (let ((dag '(("a" . ("b" "c"))
                    ("b" . ("d"))
                    ("c" . ("d" "e"))
                    ("d" . ("f"))
                    ("e" . ("f"))
                    ("f" . ()))))
        (funcall 'neovm--ts-toposort dag))
    (fmakunbound 'neovm--ts-toposort)))"#;
    let expect = expect_test::expect![[r#""OK (ok \"a\" \"b\" \"c\" \"d\" \"e\" \"f\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cycle detection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_toposort_cycle_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--ts2-toposort
    (lambda (dag)
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
          (setq all-nodes (sort unique (lambda (a b) (string< (format "%s" a) (format "%s" b))))))
        (let ((queue nil) (result nil) (count 0) (total (length all-nodes)))
          (dolist (n all-nodes)
            (when (= (gethash n in-degree 0) 0)
              (setq queue (nconc queue (list n)))))
          (while queue
            (let ((node (car queue)))
              (setq queue (cdr queue))
              (setq result (cons node result))
              (setq count (1+ count))
              (let ((succs (sort (copy-sequence (gethash node adj nil))
                                 (lambda (a b) (string< (format "%s" a) (format "%s" b))))))
                (dolist (s succs)
                  (puthash s (1- (gethash s in-degree)) in-degree)
                  (when (= (gethash s in-degree) 0)
                    (setq queue (nconc queue (list s))))))))
          (if (= count total)
              (cons 'ok (nreverse result))
            (let ((remaining nil))
              (dolist (n all-nodes)
                (when (> (gethash n in-degree 0) 0)
                  (setq remaining (cons n remaining))))
              (cons 'cycle (nreverse remaining))))))))

  (unwind-protect
      (list
        ;; Graph with a cycle: a->b->c->a
        (funcall 'neovm--ts2-toposort
                 '(("a" . ("b"))
                   ("b" . ("c"))
                   ("c" . ("a"))))
        ;; Graph with a cycle embedded in larger graph:
        ;; x -> a -> b -> c -> a, x -> d
        (funcall 'neovm--ts2-toposort
                 '(("x" . ("a" "d"))
                   ("a" . ("b"))
                   ("b" . ("c"))
                   ("c" . ("a"))
                   ("d" . ())))
        ;; Self-loop
        (funcall 'neovm--ts2-toposort
                 '(("a" . ("a"))))
        ;; No cycle: linear chain
        (funcall 'neovm--ts2-toposort
                 '(("a" . ("b"))
                   ("b" . ("c"))
                   ("c" . ("d"))
                   ("d" . ()))))
    (fmakunbound 'neovm--ts2-toposort)))"#;
    let expect = expect_test::expect![[
        r#""OK ((cycle \"a\" \"b\" \"c\") (cycle \"a\" \"b\" \"c\") (cycle \"a\") (ok \"a\" \"b\" \"c\" \"d\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// In-degree computation standalone
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_toposort_in_degree_computation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute in-degree for each node and return sorted results
    let form = r#"(progn
  (fset 'neovm--ts-in-degrees
    (lambda (dag)
      "Compute in-degree for each node. Returns sorted alist (node . in-degree)."
      (let ((in-degree (make-hash-table :test 'equal)))
        ;; Initialize all declared nodes to 0
        (dolist (entry dag)
          (unless (gethash (car entry) in-degree)
            (puthash (car entry) 0 in-degree)))
        ;; Count incoming edges
        (dolist (entry dag)
          (dolist (succ (cdr entry))
            (puthash succ (1+ (gethash succ in-degree 0)) in-degree)))
        ;; Collect and sort
        (let ((result nil))
          (maphash (lambda (k v) (setq result (cons (cons k v) result))) in-degree)
          (sort result (lambda (a b) (string< (format "%s" (car a)) (format "%s" (car b)))))))))

  (unwind-protect
      (list
        ;; Diamond DAG: a->{b,c}, b->d, c->d
        (funcall 'neovm--ts-in-degrees
                 '(("a" . ("b" "c"))
                   ("b" . ("d"))
                   ("c" . ("d"))
                   ("d" . ())))
        ;; Star topology: center -> {r1, r2, r3, r4, r5}
        (funcall 'neovm--ts-in-degrees
                 '(("center" . ("r1" "r2" "r3" "r4" "r5"))
                   ("r1" . ()) ("r2" . ()) ("r3" . ()) ("r4" . ()) ("r5" . ())))
        ;; Chain: a->b->c->d->e
        (funcall 'neovm--ts-in-degrees
                 '(("a" . ("b"))
                   ("b" . ("c"))
                   ("c" . ("d"))
                   ("d" . ("e"))
                   ("e" . ())))
        ;; Multiple sources converging
        (funcall 'neovm--ts-in-degrees
                 '(("s1" . ("m"))
                   ("s2" . ("m"))
                   ("s3" . ("m"))
                   ("m" . ("t"))
                   ("t" . ()))))
    (fmakunbound 'neovm--ts-in-degrees)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"a\" . 0) (\"b\" . 1) (\"c\" . 1) (\"d\" . 2)) ((\"center\" . 0) (\"r1\" . 1) (\"r2\" . 1) (\"r3\" . 1) (\"r4\" . 1) (\"r5\" . 1)) ((\"a\" . 0) (\"b\" . 1) (\"c\" . 1) (\"d\" . 1) (\"e\" . 1)) ((\"m\" . 3) (\"s1\" . 0) (\"s2\" . 0) (\"s3\" . 0) (\"t\" . 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: dependency resolution for a package manager
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_toposort_package_manager() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a package manager that resolves build order from dependencies
    let form = r#"(progn
  (fset 'neovm--pkg-toposort
    (lambda (dag)
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
        (let ((queue nil) (result nil) (count 0) (total (length all-nodes)))
          (dolist (n all-nodes)
            (when (= (gethash n in-degree 0) 0)
              (setq queue (nconc queue (list n)))))
          (while queue
            (let ((node (car queue)))
              (setq queue (cdr queue))
              (setq result (cons node result))
              (setq count (1+ count))
              (let ((succs (sort (copy-sequence (gethash node adj nil))
                                 (lambda (a b) (string< a b)))))
                (dolist (s succs)
                  (puthash s (1- (gethash s in-degree)) in-degree)
                  (when (= (gethash s in-degree) 0)
                    (setq queue (nconc queue (list s))))))))
          (if (= count total)
              (cons 'ok (nreverse result))
            (let ((remaining nil))
              (dolist (n all-nodes)
                (when (> (gethash n in-degree 0) 0)
                  (setq remaining (cons n remaining))))
              (cons 'cycle (nreverse remaining))))))))

  ;; Package dependency graph for a web app:
  ;; "webapp" depends on "router" and "database"
  ;; "router" depends on "http-lib"
  ;; "database" depends on "sql-driver" and "config"
  ;; "http-lib" depends on "config" and "logging"
  ;; "sql-driver" depends on "logging"
  ;; "config", "logging" have no dependencies
  (fset 'neovm--pkg-build-order
    (lambda (packages)
      "Given package specs ((name . (deps ...)) ...), return build order.
       Note: edges go from dependency TO dependent (reverse of 'depends-on')."
      ;; Build DAG where edges go dep -> pkg (dependency must be built first)
      (let ((dag nil)
            (all-pkgs (make-hash-table :test 'equal)))
        ;; Register all packages
        (dolist (p packages)
          (puthash (car p) t all-pkgs))
        ;; Build adjacency: for each (pkg . deps), add edge dep -> pkg
        ;; This means each dep's successor list includes pkg
        (let ((adj (make-hash-table :test 'equal)))
          (dolist (p packages)
            (unless (gethash (car p) adj)
              (puthash (car p) nil adj))
            (dolist (dep (cdr p))
              (unless (gethash dep adj)
                (puthash dep nil adj))
              (puthash dep (cons (car p) (gethash dep adj nil)) adj)))
          ;; Convert to alist
          (maphash (lambda (k v) (setq dag (cons (cons k v) dag))) adj))
        (funcall 'neovm--pkg-toposort dag))))

  (unwind-protect
      (let ((packages '(("webapp" . ("router" "database"))
                          ("router" . ("http-lib"))
                          ("database" . ("sql-driver" "config"))
                          ("http-lib" . ("config" "logging"))
                          ("sql-driver" . ("logging"))
                          ("config" . ())
                          ("logging" . ()))))
        (let ((build-order (funcall 'neovm--pkg-build-order packages)))
          (list
            ;; Should succeed
            (car build-order)
            ;; Build order
            (cdr build-order)
            ;; Verify: "logging" and "config" should come before everything else
            (let ((order (cdr build-order)))
              (list
                ;; logging before sql-driver
                (< (length (memq "logging" (member "logging" order)))
                   (length (memq "sql-driver" (member "sql-driver" order))))
                ;; config before database
                (< (length (member "config" order))
                   (length (member "database" order)))
                ;; Length should be 7
                (length order))))))
    (fmakunbound 'neovm--pkg-toposort)
    (fmakunbound 'neovm--pkg-build-order)))"#;
    let expect = expect_test::expect![[
        r#""OK (ok (\"config\" \"logging\" \"http-lib\" \"sql-driver\" \"router\" \"database\" \"webapp\") (nil nil 7))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: task scheduling with topological order
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_toposort_task_scheduling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Given tasks with dependencies and durations, compute earliest start times
    let form = r#"(progn
  (fset 'neovm--task-toposort
    (lambda (dag)
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
        (let ((queue nil) (result nil) (count 0) (total (length all-nodes)))
          (dolist (n all-nodes)
            (when (= (gethash n in-degree 0) 0)
              (setq queue (nconc queue (list n)))))
          (while queue
            (let ((node (car queue)))
              (setq queue (cdr queue))
              (setq result (cons node result))
              (setq count (1+ count))
              (let ((succs (sort (copy-sequence (gethash node adj nil))
                                 (lambda (a b) (string< a b)))))
                (dolist (s succs)
                  (puthash s (1- (gethash s in-degree)) in-degree)
                  (when (= (gethash s in-degree) 0)
                    (setq queue (nconc queue (list s))))))))
          (if (= count total)
              (nreverse result)
            nil)))))

  (fset 'neovm--task-schedule
    (lambda (tasks)
      "TASKS: list of (name duration . dependencies).
       Returns ((name . earliest-start) ...) sorted by earliest start."
      ;; Build DAG: edges from dependency -> task
      (let ((adj (make-hash-table :test 'equal))
            (durations (make-hash-table :test 'equal))
            (deps-of (make-hash-table :test 'equal))
            (dag nil))
        ;; Register all tasks
        (dolist (task tasks)
          (let ((name (nth 0 task))
                (dur (nth 1 task))
                (deps (nthcdr 2 task)))
            (puthash name dur durations)
            (puthash name deps deps-of)
            (unless (gethash name adj)
              (puthash name nil adj))
            ;; For each dependency, add edge dep -> name
            (dolist (d deps)
              (unless (gethash d adj)
                (puthash d nil adj))
              (puthash d (cons name (gethash d adj nil)) adj))))
        ;; Convert to alist for toposort
        (maphash (lambda (k v) (setq dag (cons (cons k v) dag))) adj)
        (let ((order (funcall 'neovm--task-toposort dag)))
          (if (null order)
              '(cycle-detected)
            ;; Compute earliest start times
            (let ((start-times (make-hash-table :test 'equal)))
              ;; Initialize all to 0
              (dolist (task order)
                (puthash task 0 start-times))
              ;; For each task in topological order, update successors
              (dolist (task order)
                (let ((finish (+ (gethash task start-times 0)
                                  (gethash task durations 0)))
                      (succs (gethash task adj nil)))
                  (dolist (s succs)
                    (when (> finish (gethash s start-times 0))
                      (puthash s finish start-times)))))
              ;; Collect results
              (let ((result nil))
                (dolist (task order)
                  (setq result (cons (list task
                                           (gethash task start-times 0)
                                           (gethash task durations 0)
                                           (+ (gethash task start-times 0)
                                              (gethash task durations 0)))
                                     result)))
                (let ((total-time 0))
                  (dolist (r result)
                    (when (> (nth 3 r) total-time)
                      (setq total-time (nth 3 r))))
                  (list (nreverse result) total-time)))))))))

  (unwind-protect
      (let ((tasks '(("design"      3)
                      ("frontend"   5 "design")
                      ("backend"    4 "design")
                      ("database"   2 "design")
                      ("api"        3 "backend" "database")
                      ("testing"    2 "frontend" "api")
                      ("deployment" 1 "testing"))))
        (funcall 'neovm--task-schedule tasks))
    (fmakunbound 'neovm--task-toposort)
    (fmakunbound 'neovm--task-schedule)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"design\" 0 3 3) (\"backend\" 3 4 7) (\"database\" 3 2 5) (\"frontend\" 3 5 8) (\"api\" 7 3 10) (\"testing\" 10 2 12) (\"deployment\" 12 1 13)) 13)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: multi-level topological sort (group by levels)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_toposort_level_grouping() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Group nodes by their "level" — nodes at level 0 have no dependencies,
    // level 1 depends only on level 0, etc.
    let form = r#"(progn
  (fset 'neovm--ts-levels
    (lambda (dag)
      "Compute level-grouped topological sort.
       Returns list of levels: ((level-0-nodes) (level-1-nodes) ...) or 'cycle."
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
        ;; Deduplicate
        (let ((seen (make-hash-table :test 'equal)) (unique nil))
          (dolist (n all-nodes)
            (unless (gethash n seen) (puthash n t seen) (setq unique (cons n unique))))
          (setq all-nodes (sort unique (lambda (a b) (string< (format "%s" a) (format "%s" b))))))
        ;; BFS by levels
        (let ((current-level nil)
              (levels nil)
              (processed 0)
              (total (length all-nodes)))
          ;; Seed first level
          (dolist (n all-nodes)
            (when (= (gethash n in-degree 0) 0)
              (setq current-level (cons n current-level))))
          (setq current-level (sort current-level
                                    (lambda (a b) (string< (format "%s" a) (format "%s" b)))))
          ;; Process level by level
          (while current-level
            (setq levels (cons current-level levels))
            (setq processed (+ processed (length current-level)))
            (let ((next-level nil))
              (dolist (node current-level)
                (dolist (s (gethash node adj nil))
                  (puthash s (1- (gethash s in-degree)) in-degree)
                  (when (= (gethash s in-degree) 0)
                    (setq next-level (cons s next-level)))))
              (setq current-level
                    (sort (let ((seen2 (make-hash-table :test 'equal)) (u nil))
                            (dolist (n next-level)
                              (unless (gethash n seen2) (puthash n t seen2) (setq u (cons n u))))
                            u)
                          (lambda (a b) (string< (format "%s" a) (format "%s" b)))))))
          (if (= processed total)
              (nreverse levels)
            'cycle)))))

  (unwind-protect
      (list
        ;; Diamond: a->{b,c}, b->d, c->d
        ;; Level 0: a, Level 1: b,c, Level 2: d
        (funcall 'neovm--ts-levels
                 '(("a" . ("b" "c"))
                   ("b" . ("d"))
                   ("c" . ("d"))
                   ("d" . ())))
        ;; Deep chain: a->b->c->d->e
        ;; Each is its own level
        (funcall 'neovm--ts-levels
                 '(("a" . ("b"))
                   ("b" . ("c"))
                   ("c" . ("d"))
                   ("d" . ("e"))
                   ("e" . ())))
        ;; Wide: {a,b,c,d} all independent -> e
        ;; Level 0: a,b,c,d, Level 1: e
        (funcall 'neovm--ts-levels
                 '(("a" . ("e"))
                   ("b" . ("e"))
                   ("c" . ("e"))
                   ("d" . ("e"))
                   ("e" . ())))
        ;; Complex multi-level
        ;; Level 0: a, b
        ;; Level 1: c (depends on a), d (depends on b)
        ;; Level 2: e (depends on c, d)
        ;; Level 3: f (depends on e)
        (funcall 'neovm--ts-levels
                 '(("a" . ("c"))
                   ("b" . ("d"))
                   ("c" . ("e"))
                   ("d" . ("e"))
                   ("e" . ("f"))
                   ("f" . ())))
        ;; Cycle detection
        (funcall 'neovm--ts-levels
                 '(("a" . ("b"))
                   ("b" . ("c"))
                   ("c" . ("a")))))
    (fmakunbound 'neovm--ts-levels)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"a\") (\"b\" \"c\") (\"d\")) ((\"a\") (\"b\") (\"c\") (\"d\") (\"e\")) ((\"a\" \"b\" \"c\" \"d\") (\"e\")) ((\"a\" \"b\") (\"c\" \"d\") (\"e\") (\"f\")) cycle)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: transitive reduction using topological order
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_toposort_transitive_reduction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Remove redundant edges: if a->c exists and a->b->c exists, remove a->c.
    // Use topological order + reachability.
    let form = r#"(progn
  (fset 'neovm--tr-reachable
    (lambda (adj start exclude-direct)
      "BFS from START, skipping EXCLUDE-DIRECT, return set of reachable nodes."
      (let ((visited (make-hash-table :test 'equal))
            (queue nil))
        ;; Start BFS from successors of start, excluding the direct edge
        (dolist (s (gethash start adj nil))
          (unless (equal s exclude-direct)
            (unless (gethash s visited)
              (puthash s t visited)
              (setq queue (nconc queue (list s))))))
        (while queue
          (let ((node (car queue)))
            (setq queue (cdr queue))
            (dolist (s (gethash node adj nil))
              (unless (gethash s visited)
                (puthash s t visited)
                (setq queue (nconc queue (list s)))))))
        visited)))

  (fset 'neovm--tr-reduce
    (lambda (dag)
      "Compute transitive reduction of DAG."
      (let ((adj (make-hash-table :test 'equal))
            (result nil))
        ;; Build adjacency
        (dolist (entry dag)
          (puthash (car entry) (cdr entry) adj))
        ;; For each node, check each direct successor
        (dolist (entry dag)
          (let ((node (car entry))
                (succs (cdr entry))
                (kept nil))
            (dolist (s succs)
              ;; Check if s is reachable from node through other paths
              (let ((reachable (funcall 'neovm--tr-reachable adj node s)))
                (unless (gethash s reachable)
                  (setq kept (cons s kept)))))
            (setq result (cons (cons node (sort (nreverse kept)
                                                 (lambda (a b) (string< (format "%s" a) (format "%s" b)))))
                               result))))
        (sort result (lambda (a b) (string< (format "%s" (car a)) (format "%s" (car b))))))))

  (unwind-protect
      (list
        ;; a->b, a->c, b->c: a->c is redundant
        (funcall 'neovm--tr-reduce
                 '(("a" . ("b" "c"))
                   ("b" . ("c"))
                   ("c" . ())))
        ;; a->b, a->c, a->d, b->d, c->d: a->d is redundant (via b or c)
        (funcall 'neovm--tr-reduce
                 '(("a" . ("b" "c" "d"))
                   ("b" . ("d"))
                   ("c" . ("d"))
                   ("d" . ())))
        ;; No redundant edges: simple chain
        (funcall 'neovm--tr-reduce
                 '(("a" . ("b"))
                   ("b" . ("c"))
                   ("c" . ()))))
    (fmakunbound 'neovm--tr-reachable)
    (fmakunbound 'neovm--tr-reduce)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"a\" \"b\") (\"b\" \"c\") (\"c\")) ((\"a\" \"b\" \"c\") (\"b\" \"d\") (\"c\" \"d\") (\"d\")) ((\"a\" \"b\") (\"b\" \"c\") (\"c\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
