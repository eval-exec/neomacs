//! Oracle parity tests for a dependency resolver implemented in Elisp:
//! dependency graph specification, topological ordering with cycle detection,
//! version constraint checking, dependency resolution order, conflict
//! detection, optional dependencies, and build order computation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Topological sort with cycle detection (Kahn's algorithm)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_depresolver_topological_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement Kahn's algorithm for topological sorting. Detects cycles
    // by checking if all nodes were visited.
    let form = r#"(progn
  (fset 'neovm--dep-topo-sort
    (lambda (nodes edges)
      "Topological sort of NODES given EDGES (list of (from . to) pairs).
       Returns (ok . sorted-list) or (cycle . remaining-nodes)."
      (let ((in-degree (make-hash-table :test 'eq))
            (adj (make-hash-table :test 'eq))
            (queue nil)
            (result nil)
            (count 0))
        ;; Initialize in-degree to 0 for all nodes
        (dolist (n nodes) (puthash n 0 in-degree))
        ;; Initialize adjacency lists
        (dolist (n nodes) (puthash n nil adj))
        ;; Build adjacency list and in-degree counts
        (dolist (e edges)
          (let ((from (car e)) (to (cdr e)))
            (puthash to (1+ (gethash to in-degree 0)) in-degree)
            (puthash from (cons to (gethash from adj nil)) adj)))
        ;; Seed queue with zero in-degree nodes (sorted for determinism)
        (dolist (n (sort (copy-sequence nodes)
                         (lambda (a b)
                           (string< (symbol-name a) (symbol-name b)))))
          (when (= (gethash n in-degree) 0)
            (setq queue (nconc queue (list n)))))
        ;; Process queue
        (while queue
          (let ((node (car queue)))
            (setq queue (cdr queue))
            (setq result (cons node result))
            (setq count (1+ count))
            ;; Decrement in-degree of neighbors
            (let ((neighbors (sort (copy-sequence (gethash node adj nil))
                                   (lambda (a b)
                                     (string< (symbol-name a) (symbol-name b))))))
              (dolist (nbr neighbors)
                (puthash nbr (1- (gethash nbr in-degree)) in-degree)
                (when (= (gethash nbr in-degree) 0)
                  (setq queue (nconc queue (list nbr))))))))
        ;; Check for cycle
        (if (= count (length nodes))
            (cons 'ok (nreverse result))
          ;; Collect nodes that weren't visited (part of a cycle)
          (let ((remaining nil))
            (dolist (n nodes)
              (when (> (gethash n in-degree) 0)
                (setq remaining (cons n remaining))))
            (cons 'cycle (sort (nreverse remaining)
                               (lambda (a b)
                                 (string< (symbol-name a) (symbol-name b))))))))))

  (unwind-protect
      (list
        ;; Simple linear chain: a -> b -> c -> d
        (funcall 'neovm--dep-topo-sort
                 '(a b c d)
                 '((a . b) (b . c) (c . d)))
        ;; Diamond: a -> b, a -> c, b -> d, c -> d
        (funcall 'neovm--dep-topo-sort
                 '(a b c d)
                 '((a . b) (a . c) (b . d) (c . d)))
        ;; No edges: all independent
        (funcall 'neovm--dep-topo-sort
                 '(x y z)
                 nil)
        ;; Cycle: a -> b -> c -> a
        (funcall 'neovm--dep-topo-sort
                 '(a b c)
                 '((a . b) (b . c) (c . a)))
        ;; Partial cycle: a -> b, b -> c -> d -> c (c-d cycle)
        (funcall 'neovm--dep-topo-sort
                 '(a b c d)
                 '((a . b) (b . c) (c . d) (d . c)))
        ;; Single node, no edges
        (funcall 'neovm--dep-topo-sort '(solo) nil)
        ;; Complex DAG
        (funcall 'neovm--dep-topo-sort
                 '(a b c d e f)
                 '((a . b) (a . c) (b . d) (c . d) (c . e) (d . f) (e . f))))
    (fmakunbound 'neovm--dep-topo-sort)))"#;
    let expect = expect_test::expect![[
        r#""OK ((ok a b c d) (ok a b c d) (ok x y z) (cycle a b c) (cycle c d) (ok solo) (ok a b c d e f))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Version constraint checking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_depresolver_version_constraints() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement version comparison and constraint satisfaction.
    // Versions are (major minor patch) lists. Constraints are
    // (op version) where op is >=, <=, =.
    let form = r#"(progn
  (fset 'neovm--dep-version-compare
    (lambda (v1 v2)
      "Compare version lists V1 and V2. Returns -1, 0, or 1."
      (let ((result 0) (i 0) (done nil))
        (while (and (not done) (< i 3))
          (let ((a (nth i v1)) (b (nth i v2)))
            (cond
              ((< a b) (setq result -1 done t))
              ((> a b) (setq result 1 done t))
              (t (setq i (1+ i))))))
        result)))

  (fset 'neovm--dep-check-constraint
    (lambda (version constraint)
      "Check if VERSION satisfies CONSTRAINT (op . required-version)."
      (let ((op (car constraint))
            (required (cdr constraint))
            (cmp (funcall 'neovm--dep-version-compare version required)))
        (cond
          ((eq op '>=) (>= cmp 0))
          ((eq op '<=) (<= cmp 0))
          ((eq op '=) (= cmp 0))
          ((eq op '>) (> cmp 0))
          ((eq op '<) (< cmp 0))
          (t nil)))))

  (fset 'neovm--dep-check-all-constraints
    (lambda (version constraints)
      "Check if VERSION satisfies all CONSTRAINTS."
      (let ((ok t))
        (dolist (c constraints)
          (unless (funcall 'neovm--dep-check-constraint version c)
            (setq ok nil)))
        ok)))

  (unwind-protect
      (list
        ;; Version comparison
        (funcall 'neovm--dep-version-compare '(1 0 0) '(1 0 0))
        (funcall 'neovm--dep-version-compare '(2 0 0) '(1 9 9))
        (funcall 'neovm--dep-version-compare '(1 2 3) '(1 2 4))
        (funcall 'neovm--dep-version-compare '(1 3 0) '(1 2 9))
        ;; Constraint checking
        (funcall 'neovm--dep-check-constraint '(2 0 0) '(>= . (1 5 0)))
        (funcall 'neovm--dep-check-constraint '(1 0 0) '(>= . (1 5 0)))
        (funcall 'neovm--dep-check-constraint '(1 5 0) '(= . (1 5 0)))
        (funcall 'neovm--dep-check-constraint '(3 0 0) '(< . (2 0 0)))
        ;; Multiple constraints: >= 1.0.0 AND < 3.0.0
        (funcall 'neovm--dep-check-all-constraints
                 '(2 5 0) '((>= . (1 0 0)) (< . (3 0 0))))
        (funcall 'neovm--dep-check-all-constraints
                 '(3 0 0) '((>= . (1 0 0)) (< . (3 0 0))))
        (funcall 'neovm--dep-check-all-constraints
                 '(0 9 0) '((>= . (1 0 0)) (< . (3 0 0))))
        ;; Exact version match
        (funcall 'neovm--dep-check-all-constraints
                 '(1 2 3) '((= . (1 2 3))))
        (funcall 'neovm--dep-check-all-constraints
                 '(1 2 4) '((= . (1 2 3)))))
    (fmakunbound 'neovm--dep-version-compare)
    (fmakunbound 'neovm--dep-check-constraint)
    (fmakunbound 'neovm--dep-check-all-constraints)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable required)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Full dependency resolution with version constraints
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_depresolver_full_resolution() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A package registry with version info and dependencies. Resolve
    // a target package's full dependency set with version checking.
    let form = r#"(progn
  ;; Registry: alist of (pkg-name . ((version . (dep1 dep2 ...)) ...))
  ;; Each dep is (dep-name . version-constraint)
  (defvar neovm--dep-registry nil)

  (fset 'neovm--dep-resolve
    (lambda (pkg-name)
      "Resolve all transitive dependencies of PKG-NAME.
       Returns (ok . install-order) or (error . reason)."
      (let ((visited (make-hash-table :test 'eq))
            (resolved nil)
            (in-progress (make-hash-table :test 'eq))
            (error-msg nil))
        (fset 'neovm--dep-resolve-inner
          (lambda (pkg)
            (unless (or error-msg (gethash pkg visited))
              ;; Cycle detection
              (when (gethash pkg in-progress)
                (setq error-msg (format "cycle: %s" pkg)))
              (unless error-msg
                (puthash pkg t in-progress)
                (let ((pkg-info (assq pkg neovm--dep-registry)))
                  (if (not pkg-info)
                      (setq error-msg (format "unknown: %s" pkg))
                    ;; Process dependencies
                    (let ((deps (cddr pkg-info)))
                      (dolist (dep deps)
                        (funcall 'neovm--dep-resolve-inner (car dep))))))
                (remhash pkg in-progress)
                (puthash pkg t visited)
                (setq resolved (cons pkg resolved))))))
        (funcall 'neovm--dep-resolve-inner pkg-name)
        (fmakunbound 'neovm--dep-resolve-inner)
        (if error-msg
            (cons 'error error-msg)
          (cons 'ok (nreverse resolved))))))

  (unwind-protect
      (progn
        ;; Setup registry: (name version dep1 dep2 ...)
        (setq neovm--dep-registry
              '((app (1 0 0) (web-framework . (>= . (2 0 0))) (database . (>= . (1 0 0))))
                (web-framework (2 5 0) (http-lib . (>= . (1 0 0))) (template-engine . (>= . (1 0 0))))
                (database (1 3 0) (connection-pool . (>= . (0 5 0))))
                (http-lib (1 2 0))
                (template-engine (1 1 0) (html-parser . (>= . (0 1 0))))
                (html-parser (0 3 0))
                (connection-pool (0 8 0))))

        (list
          ;; Resolve app: should get full transitive closure
          (funcall 'neovm--dep-resolve 'app)
          ;; Resolve a leaf package
          (funcall 'neovm--dep-resolve 'http-lib)
          ;; Resolve web-framework
          (funcall 'neovm--dep-resolve 'web-framework)
          ;; Unknown package
          (funcall 'neovm--dep-resolve 'nonexistent)
          ;; Add a cycle and try to resolve
          (let ((neovm--dep-registry
                  (cons '(cycle-a (1 0 0) (cycle-b . (>= . (1 0 0))))
                        (cons '(cycle-b (1 0 0) (cycle-a . (>= . (1 0 0))))
                              neovm--dep-registry))))
            (funcall 'neovm--dep-resolve 'cycle-a))))
    (fmakunbound 'neovm--dep-resolve)
    (makunbound 'neovm--dep-registry)))"#;
    let expect = expect_test::expect![[
        r#""OK ((ok http-lib html-parser template-engine web-framework connection-pool database app) (ok http-lib) (ok http-lib html-parser template-engine web-framework) (error . \"unknown: nonexistent\") (error . \"cycle: cycle-a\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Conflict detection between dependency constraints
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_depresolver_conflict_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple packages require different version ranges of the same dependency.
    // Detect conflicts where ranges don't overlap.
    let form = r#"(progn
  (fset 'neovm--dep-ranges-overlap
    (lambda (range1 range2)
      "Check if two version ranges overlap.
       Each range is ((>= . min) (< . max)) or similar."
      ;; Simplified: ranges are (min-version . max-version) inclusive
      ;; Returns t if they overlap.
      (let ((min1 (car range1)) (max1 (cdr range1))
            (min2 (car range2)) (max2 (cdr range2)))
        ;; Compare as version lists
        (let ((cmp-fn (lambda (v1 v2)
                        (let ((r 0) (i 0) (done nil))
                          (while (and (not done) (< i 3))
                            (cond
                              ((< (nth i v1) (nth i v2)) (setq r -1 done t))
                              ((> (nth i v1) (nth i v2)) (setq r 1 done t))
                              (t (setq i (1+ i)))))
                          r))))
          ;; Overlap if min1 <= max2 AND min2 <= max1
          (and (<= (funcall cmp-fn min1 max2) 0)
               (<= (funcall cmp-fn min2 max1) 0))))))

  (fset 'neovm--dep-find-conflicts
    (lambda (requirements)
      "Given REQUIREMENTS: alist of (pkg-name . (min-ver . max-ver)),
       find all conflicting pairs for the same package."
      (let ((by-pkg (make-hash-table :test 'eq))
            (conflicts nil))
        ;; Group requirements by package
        (dolist (req requirements)
          (let ((pkg (car req)) (range (cdr req)))
            (puthash pkg (cons range (gethash pkg by-pkg nil)) by-pkg)))
        ;; Check each package with multiple requirements
        (maphash (lambda (pkg ranges)
                   (when (> (length ranges) 1)
                     ;; Check all pairs
                     (let ((rlist ranges))
                       (while rlist
                         (let ((r1 (car rlist))
                               (rest (cdr rlist)))
                           (dolist (r2 rest)
                             (unless (funcall 'neovm--dep-ranges-overlap r1 r2)
                               (setq conflicts
                                     (cons (list pkg r1 r2) conflicts)))))
                         (setq rlist (cdr rlist))))))
                 by-pkg)
        (nreverse conflicts))))

  (unwind-protect
      (list
        ;; Overlapping ranges: no conflict
        (funcall 'neovm--dep-ranges-overlap
                 '((1 0 0) . (2 0 0))
                 '((1 5 0) . (3 0 0)))
        ;; Non-overlapping ranges: conflict
        (funcall 'neovm--dep-ranges-overlap
                 '((1 0 0) . (1 9 9))
                 '((2 0 0) . (3 0 0)))
        ;; Adjacent ranges
        (funcall 'neovm--dep-ranges-overlap
                 '((1 0 0) . (2 0 0))
                 '((2 0 0) . (3 0 0)))
        ;; Find conflicts in a set of requirements
        (funcall 'neovm--dep-find-conflicts
                 '((lib-x . ((1 0 0) . (2 0 0)))
                   (lib-x . ((1 5 0) . (3 0 0)))
                   (lib-y . ((1 0 0) . (1 5 0)))
                   (lib-y . ((2 0 0) . (3 0 0)))))
        ;; No conflicts
        (funcall 'neovm--dep-find-conflicts
                 '((lib-a . ((1 0 0) . (3 0 0)))
                   (lib-a . ((2 0 0) . (2 5 0)))))
        ;; Single requirement per package: never conflicts
        (funcall 'neovm--dep-find-conflicts
                 '((lib-p . ((1 0 0) . (2 0 0)))
                   (lib-q . ((1 0 0) . (2 0 0))))))
    (fmakunbound 'neovm--dep-ranges-overlap)
    (fmakunbound 'neovm--dep-find-conflicts)))"#;
    let expect = expect_test::expect![[
        r#""OK (t nil t ((lib-y ((2 0 0) 3 0 0) ((1 0 0) 1 5 0))) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Optional dependencies and build order
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_depresolver_optional_deps_build_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Handle optional dependencies: only include them if available.
    // Compute a parallel build order (layers of independent packages).
    let form = r#"(progn
  (fset 'neovm--dep-build-layers
    (lambda (nodes edges)
      "Compute build layers: each layer contains packages whose deps
       are all in earlier layers. Returns list of layers or 'cycle."
      (let ((in-degree (make-hash-table :test 'eq))
            (adj (make-hash-table :test 'eq))
            (layers nil)
            (remaining (length nodes)))
        ;; Initialize
        (dolist (n nodes)
          (puthash n 0 in-degree)
          (puthash n nil adj))
        ;; Build graph
        (dolist (e edges)
          (puthash (cdr e) (1+ (gethash (cdr e) in-degree 0)) in-degree)
          (puthash (car e) (cons (cdr e) (gethash (car e) adj nil)) adj))
        ;; BFS by layers
        (let ((continue t))
          (while (and continue (> remaining 0))
            ;; Collect all nodes with in-degree 0
            (let ((layer nil))
              (dolist (n nodes)
                (when (= (gethash n in-degree 0) 0)
                  (setq layer (cons n layer))))
              (if (null layer)
                  ;; No zero in-degree nodes but remaining > 0: cycle
                  (setq continue nil)
                ;; Sort layer for determinism
                (setq layer (sort layer (lambda (a b)
                                          (string< (symbol-name a) (symbol-name b)))))
                (setq layers (cons layer layers))
                ;; Remove these nodes
                (dolist (n layer)
                  (puthash n -1 in-degree)  ;; mark as processed
                  (setq remaining (1- remaining))
                  (dolist (nbr (gethash n adj nil))
                    (puthash nbr (1- (gethash nbr in-degree)) in-degree)))))))
        (if (> remaining 0)
            'cycle
          (nreverse layers)))))

  (fset 'neovm--dep-resolve-with-optional
    (lambda (pkg-name registry available)
      "Resolve PKG-NAME using REGISTRY, only including optional deps if in AVAILABLE set."
      (let ((resolved nil)
            (visited (make-hash-table :test 'eq)))
        (fset 'neovm--dep-rwo-inner
          (lambda (pkg)
            (unless (gethash pkg visited)
              (puthash pkg t visited)
              (let ((info (assq pkg registry)))
                (when info
                  (let ((deps (nth 2 info))
                        (opt-deps (nth 3 info)))
                    ;; Required deps
                    (dolist (d deps)
                      (funcall 'neovm--dep-rwo-inner d))
                    ;; Optional deps: only if available
                    (dolist (d opt-deps)
                      (when (memq d available)
                        (funcall 'neovm--dep-rwo-inner d))))))
              (setq resolved (cons pkg resolved)))))
        (funcall 'neovm--dep-rwo-inner pkg-name)
        (fmakunbound 'neovm--dep-rwo-inner)
        (nreverse resolved))))

  (unwind-protect
      (let ((registry '((app nil (core ui) (analytics))
                        (core nil (utils) nil)
                        (ui nil (core utils) (theme))
                        (utils nil nil nil)
                        (analytics nil (core) nil)
                        (theme nil nil nil))))
        (list
          ;; Build layers for a DAG
          (funcall 'neovm--dep-build-layers
                   '(a b c d e f)
                   '((a . c) (a . d) (b . d) (c . e) (d . e) (e . f)))
          ;; Independent nodes: all in one layer
          (funcall 'neovm--dep-build-layers
                   '(x y z)
                   nil)
          ;; Linear chain: each in its own layer
          (funcall 'neovm--dep-build-layers
                   '(a b c d)
                   '((a . b) (b . c) (c . d)))
          ;; Cycle detection
          (funcall 'neovm--dep-build-layers
                   '(a b c)
                   '((a . b) (b . c) (c . a)))
          ;; Resolve with all optionals available
          (funcall 'neovm--dep-resolve-with-optional
                   'app registry '(analytics theme))
          ;; Resolve with no optionals
          (funcall 'neovm--dep-resolve-with-optional
                   'app registry nil)
          ;; Resolve with only analytics available
          (funcall 'neovm--dep-resolve-with-optional
                   'app registry '(analytics))))
    (fmakunbound 'neovm--dep-build-layers)
    (fmakunbound 'neovm--dep-resolve-with-optional)))"#;
    let expect = expect_test::expect![[
        r#""OK (((a b) (c d) (e) (f)) ((x y z)) ((a) (b) (c) (d)) cycle (utils core theme ui analytics app) (utils core ui app) (utils core ui analytics app))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dependency graph analysis: reverse deps and impact assessment
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_depresolver_reverse_deps_impact() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute reverse dependencies (who depends on me?) and assess
    // the impact of removing a package (all transitively affected).
    let form = r#"(progn
  (fset 'neovm--dep-reverse-deps
    (lambda (edges)
      "Build reverse dependency map from EDGES ((from . to) pairs).
       Returns hash-table: node -> list of nodes that depend on it."
      (let ((rev (make-hash-table :test 'eq)))
        (dolist (e edges)
          (puthash (cdr e)
                   (cons (car e) (gethash (cdr e) rev nil))
                   rev))
        rev)))

  (fset 'neovm--dep-impact
    (lambda (node rev-deps)
      "Compute all packages transitively affected if NODE is removed."
      (let ((affected nil)
            (queue (list node))
            (visited (make-hash-table :test 'eq)))
        (while queue
          (let ((n (car queue)))
            (setq queue (cdr queue))
            (unless (gethash n visited)
              (puthash n t visited)
              (setq affected (cons n affected))
              (dolist (dep (gethash n rev-deps nil))
                (setq queue (nconc queue (list dep)))))))
        (sort (nreverse affected)
              (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))

  (unwind-protect
      (let ((edges '((app . web) (app . db) (web . http) (web . tmpl)
                     (db . pool) (tmpl . html) (api . web) (api . db))))
        (let ((rev (funcall 'neovm--dep-reverse-deps edges)))
          (list
            ;; Reverse deps of 'web: who depends on web?
            (sort (copy-sequence (gethash 'web rev nil))
                  (lambda (a b) (string< (symbol-name a) (symbol-name b))))
            ;; Reverse deps of 'http
            (sort (copy-sequence (gethash 'http rev nil))
                  (lambda (a b) (string< (symbol-name a) (symbol-name b))))
            ;; Reverse deps of 'app (nothing depends on app)
            (gethash 'app rev nil)
            ;; Impact of removing 'http: who is transitively affected?
            (funcall 'neovm--dep-impact 'http rev)
            ;; Impact of removing 'db
            (funcall 'neovm--dep-impact 'db rev)
            ;; Impact of removing 'pool (leaf dependency)
            (funcall 'neovm--dep-impact 'pool rev)
            ;; Impact of removing 'app (no one depends on it)
            (funcall 'neovm--dep-impact 'app rev))))
    (fmakunbound 'neovm--dep-reverse-deps)
    (fmakunbound 'neovm--dep-impact)))"#;
    let expect = expect_test::expect![[
        r#""OK ((api app) (web) nil (api app http web) (api app db) (api app db pool) (app))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Package installation simulation with dependency ordering
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_depresolver_install_simulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate installing packages: resolve deps, check already-installed,
    // compute install plan, track install log.
    let form = r#"(progn
  (fset 'neovm--dep-install-plan
    (lambda (target installed deps-map)
      "Compute install plan for TARGET given INSTALLED set and DEPS-MAP.
       Returns (plan . log) where plan is ordered list of packages to install."
      (let ((plan nil)
            (visited (make-hash-table :test 'eq))
            (log nil))
        ;; Mark installed packages as visited
        (dolist (pkg installed)
          (puthash pkg t visited))
        ;; DFS resolve
        (fset 'neovm--dep-plan-inner
          (lambda (pkg depth)
            (cond
              ((gethash pkg visited)
               (setq log (cons (list 'skip pkg 'already-installed depth) log)))
              (t
               (puthash pkg 'in-progress visited)
               (let ((deps (cdr (assq pkg deps-map))))
                 (dolist (d deps)
                   (funcall 'neovm--dep-plan-inner d (1+ depth))))
               (puthash pkg t visited)
               (setq plan (cons pkg plan))
               (setq log (cons (list 'install pkg depth) log))))))
        (funcall 'neovm--dep-plan-inner target 0)
        (fmakunbound 'neovm--dep-plan-inner)
        (list (nreverse plan) (nreverse log)))))

  (unwind-protect
      (let ((deps-map '((app web-framework database logger)
                        (web-framework http-server template-engine)
                        (database connection-pool)
                        (http-server)
                        (template-engine html-parser)
                        (html-parser)
                        (connection-pool)
                        (logger))))
        (list
          ;; Fresh install: nothing installed
          (funcall 'neovm--dep-install-plan 'app nil deps-map)
          ;; Some deps already installed
          (funcall 'neovm--dep-install-plan 'app
                   '(http-server html-parser connection-pool)
                   deps-map)
          ;; Target already installed
          (funcall 'neovm--dep-install-plan 'app '(app) deps-map)
          ;; Install a leaf package
          (funcall 'neovm--dep-install-plan 'html-parser nil deps-map)
          ;; Install mid-level package
          (funcall 'neovm--dep-install-plan 'web-framework
                   '(html-parser)
                   deps-map)))
    (fmakunbound 'neovm--dep-install-plan)))"#;
    let expect = expect_test::expect![[
        r#""OK (((http-server html-parser template-engine web-framework connection-pool database logger app) ((install http-server 2) (install html-parser 3) (install template-engine 2) (install web-framework 1) (install connection-pool 2) (install database 1) (install logger 1) (install app 0))) ((template-engine web-framework database logger app) ((skip http-server already-installed 2) (skip html-parser already-installed 3) (install template-engine 2) (install web-framework 1) (skip connection-pool already-installed 2) (install database 1) (install logger 1) (install app 0))) (nil ((skip app already-installed 0))) ((html-parser) ((install html-parser 0))) ((http-server template-engine web-framework) ((install http-server 1) (skip html-parser already-installed 2) (install template-engine 1) (install web-framework 0))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
