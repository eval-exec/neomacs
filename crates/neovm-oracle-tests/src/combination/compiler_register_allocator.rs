//! Oracle parity tests for register allocation simulation in pure Elisp.
//!
//! Covers: liveness analysis (def/use chains), interference graph
//! construction, graph coloring with Chaitin's algorithm, spill cost
//! heuristic, coalescing move-related nodes, split-everywhere strategy,
//! and register assignment verification.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Liveness analysis: def/use chains
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_regalloc_liveness_defuse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build def/use chains from instruction sequences.
    // Each instruction: (op dest src1 src2) or (op dest src) or (ret src)
    let form = r#"(progn (require (quote cl-lib))
(progn
  (fset 'neovm--ra-extract-defs
    (lambda (instr)
      "Return list of variables defined by INSTR."
      (cond
        ((memq (car instr) '(add sub mul load mov))
         (list (nth 1 instr)))
        (t nil))))

  (fset 'neovm--ra-extract-uses
    (lambda (instr)
      "Return list of variables used by INSTR."
      (cond
        ((memq (car instr) '(add sub mul))
         (list (nth 2 instr) (nth 3 instr)))
        ((eq (car instr) 'mov)
         (list (nth 2 instr)))
        ((eq (car instr) 'load)
         nil)
        ((eq (car instr) 'ret)
         (list (nth 1 instr)))
        (t nil))))

  (fset 'neovm--ra-liveness
    (lambda (instrs)
      "Compute live-in and live-out for each instruction.
       Returns list of (instr live-in live-out defs uses)."
      (let* ((n (length instrs))
             (live-in (make-vector n nil))
             (live-out (make-vector n nil))
             (changed t))
        (while changed
          (setq changed nil)
          (let ((i (1- n)))
            (while (>= i 0)
              (let* ((instr (nth i instrs))
                     (defs (funcall 'neovm--ra-extract-defs instr))
                     (uses (funcall 'neovm--ra-extract-uses instr))
                     ;; live-out = union of live-in of successors (simple: just i+1)
                     (new-out (if (< (1+ i) n) (aref live-in (1+ i)) nil))
                     ;; live-in = uses | (live-out - defs)
                     (out-minus-defs
                       (cl-remove-if (lambda (v) (memq v defs))
                                     (copy-sequence new-out)))
                     (new-in (cl-remove-duplicates
                               (append (cl-remove-if #'null uses)
                                       out-minus-defs))))
                (unless (equal (sort (copy-sequence new-out) #'string<)
                               (sort (copy-sequence (aref live-out i)) #'string<))
                  (aset live-out i new-out)
                  (setq changed t))
                (unless (equal (sort (copy-sequence new-in) #'string<)
                               (sort (copy-sequence (aref live-in i)) #'string<))
                  (aset live-in i new-in)
                  (setq changed t)))
              (setq i (1- i)))))
        ;; Build result
        (let ((result nil) (i 0))
          (dolist (instr instrs)
            (push (list instr
                        (sort (copy-sequence (aref live-in i)) #'string<)
                        (sort (copy-sequence (aref live-out i)) #'string<)
                        (funcall 'neovm--ra-extract-defs instr)
                        (funcall 'neovm--ra-extract-uses instr))
                  result)
            (setq i (1+ i)))
          (nreverse result)))))

  (unwind-protect
      (let* ((prog1 '((load a)         ;; a = mem
                       (load b)         ;; b = mem
                       (add c a b)      ;; c = a + b
                       (mul d c a)      ;; d = c * a
                       (ret d)))        ;; return d
             (analysis (funcall 'neovm--ra-liveness prog1)))
        (list
          ;; Number of instructions
          (length analysis)
          ;; Live-in for each instruction
          (mapcar (lambda (entry) (nth 1 entry)) analysis)
          ;; Live-out for each instruction
          (mapcar (lambda (entry) (nth 2 entry)) analysis)
          ;; At ret, d is used but nothing is live-out
          (nth 2 (nth 4 analysis))
          ;; At load a, nothing is live-in (first def)
          (nth 1 (nth 0 analysis))))

    (fmakunbound 'neovm--ra-extract-defs)
    (fmakunbound 'neovm--ra-extract-uses)
    (fmakunbound 'neovm--ra-liveness))))"#;
    let expect = expect_test::expect![[
        r#""OK (5 (nil (a) (a b) (a c) (d)) ((a) (a b) (a c) (d) nil) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interference graph construction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_regalloc_interference_graph() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Two variables interfere if they are both live at the same program point
    let form = r#"(progn (require (quote cl-lib))
(progn
  (fset 'neovm--ra-build-interference
    (lambda (live-sets)
      "Build interference graph from a list of live variable sets.
       Returns sorted adjacency list: ((var . (neighbors...)) ...)."
      (let ((edges (make-hash-table :test 'equal)))
        ;; For each live set, every pair of variables interferes
        (dolist (live-set live-sets)
          (let ((vars (sort (copy-sequence live-set) #'string<)))
            (dolist (v1 vars)
              (dolist (v2 vars)
                (unless (eq v1 v2)
                  (let ((key (symbol-name v1)))
                    (puthash key
                             (cl-adjoin v2 (gethash key edges nil))
                             edges)))))))
        ;; Convert to sorted alist
        (let ((result nil))
          (maphash
            (lambda (k v)
              (push (cons (intern k)
                          (sort (copy-sequence v)
                                (lambda (a b) (string< (symbol-name a)
                                                       (symbol-name b)))))
                    result))
            edges)
          (sort result (lambda (a b) (string< (symbol-name (car a))
                                              (symbol-name (car b)))))))))

  (unwind-protect
      (let* (;; Live sets at each point for: a=load, b=load, c=a+b, d=c*a, ret d
             ;; After load a: {a}
             ;; After load b: {a, b}
             ;; After add c a b: {a, c} (b dies)
             ;; After mul d c a: {d} (a, c die)
             ;; ret d: {}
             (live-sets '((a) (a b) (a c) (d)))
             (graph (funcall 'neovm--ra-build-interference live-sets))
             ;; Count edges (each edge counted once per direction)
             (total-edges (apply '+ (mapcar (lambda (entry)
                                              (length (cdr entry)))
                                            graph))))
        (list
          graph
          ;; Number of nodes
          (length graph)
          ;; a interferes with b and c (live together at some point)
          (cdr (assq 'a graph))
          ;; d interferes with nothing (live alone)
          (cdr (assq 'd graph))
          ;; Total directed edges
          total-edges))

    (fmakunbound 'neovm--ra-build-interference))))"#;
    let expect = expect_test::expect![[r#""OK (((a b c) (b a) (c a)) 3 (b c) nil 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Graph coloring with Chaitin's algorithm
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_regalloc_graph_coloring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Chaitin's algorithm: iteratively remove nodes with degree < k,
    // push onto stack, then pop and assign colors.
    let form = r#"(progn (require (quote cl-lib))
(progn
  (fset 'neovm--ra-degree
    (lambda (node graph)
      "Return degree of NODE in adjacency-list GRAPH."
      (length (cdr (assq node graph)))))

  (fset 'neovm--ra-remove-node
    (lambda (node graph)
      "Remove NODE from GRAPH (remove its entry and all references to it)."
      (let ((result nil))
        (dolist (entry graph)
          (unless (eq (car entry) node)
            (push (cons (car entry)
                        (cl-remove node (cdr entry)))
                  result)))
        (nreverse result))))

  (fset 'neovm--ra-color-graph
    (lambda (graph k)
      "Color GRAPH using at most K colors (0..k-1).
       Returns (success-p coloring) where coloring is ((var . color) ...)
       or nil if coloring impossible."
      (if (null graph)
          (list t nil)
        ;; Find node with degree < k (simplifiable)
        (let ((simplifiable nil))
          (dolist (entry graph)
            (when (and (not simplifiable)
                       (< (length (cdr entry)) k))
              (setq simplifiable (car entry))))
          (if simplifiable
              ;; Remove and recurse
              (let* ((neighbors (cdr (assq simplifiable graph)))
                     (reduced (funcall 'neovm--ra-remove-node simplifiable graph))
                     (sub-result (funcall 'neovm--ra-color-graph reduced k)))
                (if (car sub-result)
                    ;; Assign smallest color not used by neighbors
                    (let ((used-colors (mapcar
                                         (lambda (n)
                                           (cdr (assq n (cadr sub-result))))
                                         neighbors))
                          (color 0))
                      (while (memq color used-colors)
                        (setq color (1+ color)))
                      (list t (cons (cons simplifiable color)
                                    (cadr sub-result))))
                  ;; Sub-coloring failed
                  (list nil nil)))
            ;; No simplifiable node: potential spill needed
            (list nil nil))))))

  (unwind-protect
      (let* (;; Triangle graph: a-b, b-c, a-c (chromatic number = 3)
             (g1 '((a b c) (b a c) (c a b)))
             ;; Color with 3 registers: should succeed
             (r1 (funcall 'neovm--ra-color-graph g1 3))
             ;; Color with 2 registers: should fail (triangle needs 3)
             (r2 (funcall 'neovm--ra-color-graph g1 2))
             ;; Path graph: a-b-c (chromatic number = 2)
             (g2 '((a b) (b a c) (c b)))
             (r3 (funcall 'neovm--ra-color-graph g2 2))
             ;; Independent nodes: no edges (chromatic number = 1)
             (g3 '((x) (y) (z)))
             (r4 (funcall 'neovm--ra-color-graph g3 1))
             ;; Square graph: a-b-c-d-a (chromatic number = 2)
             (g4 '((a b d) (b a c) (c b d) (d c a)))
             (r5 (funcall 'neovm--ra-color-graph g4 2)))
        (list
          ;; Triangle with 3 colors: success
          (car r1)
          ;; All colors distinct for neighbors
          (let ((coloring (cadr r1)))
            (and (not (= (cdr (assq 'a coloring)) (cdr (assq 'b coloring))))
                 (not (= (cdr (assq 'b coloring)) (cdr (assq 'c coloring))))
                 (not (= (cdr (assq 'a coloring)) (cdr (assq 'c coloring))))))
          ;; Triangle with 2 colors: fail
          (car r2)
          ;; Path with 2 colors: success
          (car r3)
          ;; Independent with 1 color: success
          (car r4)
          ;; Square with 2 colors: success
          (car r5)))

    (fmakunbound 'neovm--ra-degree)
    (fmakunbound 'neovm--ra-remove-node)
    (fmakunbound 'neovm--ra-color-graph))))"#;
    let expect = expect_test::expect![[r#""OK (t t nil t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Spill cost heuristic
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_regalloc_spill_cost() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Spill cost = (uses + defs) * loop_depth / degree
    // Lower spill cost = better candidate for spilling
    let form = r#"(progn (require (quote cl-lib))
(progn
  (fset 'neovm--ra-spill-cost
    (lambda (var-info graph)
      "Compute spill cost for each variable.
       VAR-INFO: alist of (var uses defs loop-depth).
       GRAPH: interference graph (adjacency list).
       Returns sorted list of (var . cost), ascending cost."
      (let ((costs nil))
        (dolist (info var-info)
          (let* ((var (nth 0 info))
                 (uses (nth 1 info))
                 (defs (nth 2 info))
                 (depth (nth 3 info))
                 (degree (length (cdr (assq var graph))))
                 ;; Avoid division by zero
                 (weight (* (+ uses defs) (expt 10 depth)))
                 (cost (if (> degree 0)
                           (/ (* weight 100) degree)  ;; scaled by 100
                         most-positive-fixnum)))
            (push (cons var cost) costs)))
        (sort costs (lambda (a b) (< (cdr a) (cdr b)))))))

  (unwind-protect
      (let* (;; Variable info: (var uses defs loop-depth)
             (info '((a 5 1 0)    ;; used 5x, defined 1x, no loop
                     (b 2 1 2)    ;; used 2x, defined 1x, nested loop depth 2
                     (c 10 1 0)   ;; used 10x, defined 1x, no loop
                     (d 1 1 0)    ;; used 1x, defined 1x, no loop
                     (e 3 2 1)))  ;; used 3x, defined 2x, loop depth 1
             ;; Full interference graph
             (graph '((a b c d e) (b a c) (c a b e) (d a) (e a c)))
             (costs (funcall 'neovm--ra-spill-cost info graph)))
        (list
          ;; Sorted by spill cost (ascending = best to spill first)
          costs
          ;; The cheapest to spill (first in sorted list)
          (car (car costs))
          ;; d should be cheapest: low uses, no loop, low degree
          ;; b should be expensive: loop depth 2 makes weight huge
          (< (cdr (assq 'd costs)) (cdr (assq 'b costs)))))

    (fmakunbound 'neovm--ra-spill-cost))))"#;
    let expect = expect_test::expect![[
        r#""OK (((a . 150) (d . 200) (c . 366) (e . 2500) (b . 15000)) a t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Coalescing move-related nodes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_regalloc_coalescing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Coalescing: merge two non-interfering move-related variables
    // into one to eliminate MOV instructions.
    let form = r#"(progn (require (quote cl-lib))
(progn
  (fset 'neovm--ra-interferes-p
    (lambda (v1 v2 graph)
      "Check if V1 and V2 interfere in GRAPH."
      (memq v2 (cdr (assq v1 graph)))))

  (fset 'neovm--ra-coalesce
    (lambda (moves graph)
      "Try to coalesce move-related pairs.
       MOVES: list of (dest . src).
       Returns (coalesced-pairs remaining-moves merged-graph rename-map)."
      (let ((coalesced nil)
            (remaining nil)
            (rename-map nil)
            (current-graph (copy-sequence graph)))
        (dolist (mov moves)
          (let ((dest (car mov))
                (src (cdr mov)))
            ;; Can coalesce if they don't interfere
            (if (not (funcall 'neovm--ra-interferes-p dest src current-graph))
                (progn
                  (push mov coalesced)
                  (push (cons dest src) rename-map)
                  ;; Merge: replace all occurrences of dest with src in graph
                  (let ((dest-neighbors (cdr (assq dest current-graph)))
                        (new-graph nil))
                    (dolist (entry current-graph)
                      (unless (eq (car entry) dest)
                        (let ((new-neighbors
                                (mapcar (lambda (n) (if (eq n dest) src n))
                                        (cdr entry))))
                          (if (eq (car entry) src)
                              ;; Merge neighbor lists
                              (push (cons src (cl-remove-duplicates
                                                (cl-remove src
                                                  (append new-neighbors dest-neighbors))))
                                    new-graph)
                            (push (cons (car entry)
                                        (cl-remove-duplicates new-neighbors))
                                  new-graph)))))
                    (setq current-graph (nreverse new-graph))))
              ;; Can't coalesce: they interfere
              (push mov remaining))))
        (list (nreverse coalesced)
              (nreverse remaining)
              current-graph
              (nreverse rename-map)))))

  (unwind-protect
      (let* (;; Graph: a-c, b-c (a and b don't interfere)
             ;; Move: (a . b) => can coalesce a into b
             (graph '((a c) (b c) (c a b)))
             (moves '((a . b)))
             (r1 (funcall 'neovm--ra-coalesce moves graph))
             ;; Graph: a-b, b-c
             ;; Move: (a . b) => can't coalesce (they interfere)
             (graph2 '((a b) (b a c) (c b)))
             (moves2 '((a . b)))
             (r2 (funcall 'neovm--ra-coalesce moves2 graph2))
             ;; Multiple moves: (a . b) and (c . d), graph: a-c, b-d
             (graph3 '((a c) (b d) (c a) (d b)))
             (moves3 '((a . b) (c . d)))
             (r3 (funcall 'neovm--ra-coalesce moves3 graph3)))
        (list
          ;; r1: a coalesced into b
          (nth 0 r1)  ;; coalesced pairs
          (nth 1 r1)  ;; remaining moves (should be empty)
          (nth 3 r1)  ;; rename map
          ;; r2: can't coalesce (interfering)
          (nth 0 r2)  ;; empty
          (nth 1 r2)  ;; original move remains
          ;; r3: both pairs coalesced
          (length (nth 0 r3))))

    (fmakunbound 'neovm--ra-interferes-p)
    (fmakunbound 'neovm--ra-coalesce))))"#;
    let expect = expect_test::expect![[r#""OK (((a . b)) nil ((a . b)) nil ((a . b)) 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Split-everywhere strategy
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_regalloc_split_everywhere() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Split-everywhere: for each use of a spilled variable, insert a reload
    // before the use and a spill after the definition.
    let form = r#"(progn (require (quote cl-lib))
(progn
  (fset 'neovm--ra-split-everywhere
    (lambda (instrs spilled-var)
      "Insert spill/reload instructions for SPILLED-VAR.
       After each def of SPILLED-VAR, insert (spill SPILLED-VAR).
       Before each use of SPILLED-VAR, insert (reload SPILLED-VAR).
       Returns the modified instruction list."
      (let ((result nil))
        (dolist (instr instrs)
          (let ((defs (cond
                        ((memq (car instr) '(add sub mul load mov))
                         (list (nth 1 instr)))
                        (t nil)))
                (uses (cond
                        ((memq (car instr) '(add sub mul))
                         (list (nth 2 instr) (nth 3 instr)))
                        ((eq (car instr) 'mov)
                         (list (nth 2 instr)))
                        ((eq (car instr) 'ret)
                         (list (nth 1 instr)))
                        (t nil))))
            ;; Reload before use if spilled var is used
            (when (memq spilled-var uses)
              (push (list 'reload spilled-var) result))
            ;; Keep original instruction
            (push instr result)
            ;; Spill after def if spilled var is defined
            (when (memq spilled-var defs)
              (push (list 'spill spilled-var) result))))
        (nreverse result))))

  (fset 'neovm--ra-count-spill-ops
    (lambda (instrs)
      "Count spill and reload operations."
      (let ((spills 0) (reloads 0))
        (dolist (instr instrs)
          (cond
            ((eq (car instr) 'spill) (setq spills (1+ spills)))
            ((eq (car instr) 'reload) (setq reloads (1+ reloads)))))
        (list spills reloads))))

  (unwind-protect
      (let* ((prog1 '((load a)
                       (load b)
                       (add c a b)
                       (mul d c a)
                       (ret d)))
             ;; Spill variable 'a' (defined once, used twice)
             (split-a (funcall 'neovm--ra-split-everywhere prog1 'a))
             (counts-a (funcall 'neovm--ra-count-spill-ops split-a))
             ;; Spill variable 'c' (defined once, used once)
             (split-c (funcall 'neovm--ra-split-everywhere prog1 'c))
             (counts-c (funcall 'neovm--ra-count-spill-ops split-c))
             ;; Spill variable 'd' (defined once, used once at ret)
             (split-d (funcall 'neovm--ra-split-everywhere prog1 'd))
             (counts-d (funcall 'neovm--ra-count-spill-ops split-d)))
        (list
          ;; Spilling 'a': 1 spill (after load a), 2 reloads (before add, before mul)
          split-a
          counts-a
          ;; Spilling 'c': 1 spill, 1 reload
          counts-c
          ;; Spilling 'd': 1 spill, 1 reload
          counts-d
          ;; Total instruction count grows
          (length split-a)
          (length split-c)
          ;; Original had 5 instructions
          (length prog1)))

    (fmakunbound 'neovm--ra-split-everywhere)
    (fmakunbound 'neovm--ra-count-spill-ops))))"#;
    let expect = expect_test::expect![[
        r#""OK (((load a) (spill a) (load b) (reload a) (add c a b) (reload a) (mul d c a) (ret d)) (1 2) (1 1) (1 1) 8 7 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Register assignment verification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_regalloc_assignment_verification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that a register assignment is valid: no two simultaneously
    // live variables share the same register.
    let form = r#"(progn (require (quote cl-lib))
(progn
  (fset 'neovm--ra-verify-assignment
    (lambda (live-sets assignment)
      "Verify that ASSIGNMENT (alist of (var . reg)) is valid given LIVE-SETS.
       Returns (valid-p conflicts) where conflicts is list of conflicting pairs."
      (let ((valid t)
            (conflicts nil))
        (dolist (ls live-sets)
          ;; For every pair of simultaneously live variables
          (let ((vars (copy-sequence ls)))
            (while vars
              (let ((v1 (car vars)))
                (dolist (v2 (cdr vars))
                  (let ((r1 (cdr (assq v1 assignment)))
                        (r2 (cdr (assq v2 assignment))))
                    (when (and r1 r2 (= r1 r2))
                      (setq valid nil)
                      (unless (or (member (list v1 v2) conflicts)
                                  (member (list v2 v1) conflicts))
                        (push (list v1 v2 r1) conflicts))))))
              (setq vars (cdr vars)))))
        (list valid (nreverse conflicts)))))

  (fset 'neovm--ra-min-registers
    (lambda (live-sets)
      "Find the maximum number of simultaneously live variables.
       This is the minimum number of registers needed."
      (apply #'max (mapcar #'length live-sets))))

  (unwind-protect
      (let* (;; Live sets: {a,b}, {a,c}, {c,d}, {d}
             (live-sets '((a b) (a c) (c d) (d)))
             ;; Valid assignment: a=R0, b=R1, c=R1, d=R0
             ;; (b and c never live together, a and d never live together)
             (good-assignment '((a . 0) (b . 1) (c . 1) (d . 0)))
             (v1 (funcall 'neovm--ra-verify-assignment live-sets good-assignment))
             ;; Invalid assignment: a=R0, b=R0 (both live in first set)
             (bad-assignment '((a . 0) (b . 0) (c . 1) (d . 1)))
             (v2 (funcall 'neovm--ra-verify-assignment live-sets bad-assignment))
             ;; Minimum registers needed
             (min-regs (funcall 'neovm--ra-min-registers live-sets))
             ;; More complex: {a,b,c}, {b,c,d}, {d,e}
             (complex-live '((a b c) (b c d) (d e)))
             (min-regs2 (funcall 'neovm--ra-min-registers complex-live))
             ;; Valid 3-color assignment for complex case
             (good3 '((a . 0) (b . 1) (c . 2) (d . 0) (e . 1)))
             (v3 (funcall 'neovm--ra-verify-assignment complex-live good3)))
        (list
          ;; Good assignment: valid
          (car v1)
          (null (cadr v1))  ;; no conflicts
          ;; Bad assignment: invalid
          (car v2)
          (cadr v2)  ;; shows conflict
          ;; Minimum registers
          min-regs   ;; 2
          min-regs2  ;; 3
          ;; Complex valid
          (car v3)))

    (fmakunbound 'neovm--ra-verify-assignment)
    (fmakunbound 'neovm--ra-min-registers))))"#;
    let expect = expect_test::expect![[r#""OK (t t nil ((a b 0) (c d 1)) 2 3 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
