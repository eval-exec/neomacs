//! Oracle parity tests for SSA (Static Single Assignment) form
//! transformations in Elisp: variable renaming with subscripts,
//! phi function insertion at dominance frontiers, dominance tree
//! construction, SSA-based constant propagation, SSA-based dead code
//! elimination, SSA destruction (phi elimination), and copy propagation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Variable renaming with subscripts
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ssa_variable_renaming() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Convert a sequence of assignments into SSA form by appending
    // subscripts to each variable definition. Each definition gets
    // a unique subscript; uses reference the most recent definition.
    let form = r#"(progn
  (fset 'neovm--ssa-rename-vars
    (lambda (instrs)
      ;; instrs: list of (assign var expr) | (use var) | (if-branch var label)
      ;; Returns renamed instructions with subscripted variable names.
      (let ((counters (make-hash-table))
            (stacks (make-hash-table))  ;; var -> stack of current subscript
            (result nil))

        (fset 'neovm--ssa-fresh-subscript
          (lambda (var)
            (let ((c (or (gethash var counters) 0)))
              (puthash var (1+ c) counters)
              (intern (format "%s_%d" (symbol-name var) c)))))

        (fset 'neovm--ssa-current-name
          (lambda (var)
            (let ((stack (gethash var stacks)))
              (if stack (car stack)
                ;; First use without definition: use var_0
                (let ((name (funcall 'neovm--ssa-fresh-subscript var)))
                  (puthash var (list name) stacks)
                  name)))))

        (dolist (instr instrs)
          (cond
           ((eq (car instr) 'assign)
            (let* ((var (nth 1 instr))
                   (expr (nth 2 instr))
                   ;; Rename uses in expr first
                   (renamed-expr
                    (if (and (listp expr) (memq (car expr) '(+ - * /)))
                        (list (car expr)
                              (funcall 'neovm--ssa-current-name (nth 1 expr))
                              (if (symbolp (nth 2 expr))
                                  (funcall 'neovm--ssa-current-name (nth 2 expr))
                                (nth 2 expr)))
                      (if (symbolp expr)
                          (funcall 'neovm--ssa-current-name expr)
                        expr)))
                   ;; Create new subscript for the definition
                   (new-name (funcall 'neovm--ssa-fresh-subscript var)))
              (puthash var (cons new-name (or (gethash var stacks) nil)) stacks)
              (push (list 'assign new-name renamed-expr) result)))
           ((eq (car instr) 'use)
            (push (list 'use (funcall 'neovm--ssa-current-name (nth 1 instr))) result))
           ((eq (car instr) 'return)
            (push (list 'return (funcall 'neovm--ssa-current-name (nth 1 instr))) result))
           (t (push instr result))))

        (fmakunbound 'neovm--ssa-fresh-subscript)
        (fmakunbound 'neovm--ssa-current-name)
        (nreverse result))))

  (unwind-protect
      (let* (;; x = 1; x = x + 2; y = x; x = x * y; return x
             (prog1 '((assign x 1)
                       (assign x (+ x 2))
                       (assign y x)
                       (assign x (* x y))
                       (return x)))
             (ssa1 (funcall 'neovm--ssa-rename-vars prog1))
             ;; a = 5; b = a; a = b + a; b = a; return b
             (prog2 '((assign a 5)
                       (assign b a)
                       (assign a (+ b a))
                       (assign b a)
                       (return b)))
             (ssa2 (funcall 'neovm--ssa-rename-vars prog2)))
        (list ssa1 ssa2
              ;; Verify all definitions are unique
              (let ((defs nil) (unique t))
                (dolist (i ssa1)
                  (when (eq (car i) 'assign)
                    (if (memq (nth 1 i) defs)
                        (setq unique nil)
                      (push (nth 1 i) defs))))
                unique)
              (length ssa1) (length ssa2)))
    (fmakunbound 'neovm--ssa-rename-vars)))"#;
    let expect = expect_test::expect![[
        r#""OK (((assign x_0 1) (assign x_1 (+ x_0 2)) (assign y_0 x_1) (assign x_2 (* x_1 y_0)) (return x_2)) ((assign a_0 5) (assign b_0 a_0) (assign a_1 (+ b_0 a_0)) (assign b_1 a_1) (return b_1)) t 5 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Phi function insertion at dominance frontiers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ssa_phi_insertion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Given a CFG with variable definitions in different blocks,
    // insert phi functions at join points (dominance frontiers).
    let form = r#"(progn
  ;; CFG: alist of (block-id . (successors defs uses))
  ;; defs/uses: lists of variable names

  (fset 'neovm--ssa-compute-dom-frontiers
    (lambda (cfg idom)
      ;; idom: alist of (block . immediate-dominator)
      ;; Returns alist of (block . dominance-frontier-blocks)
      (let ((df (make-hash-table)))
        ;; Initialize empty frontiers
        (dolist (node cfg)
          (puthash (car node) nil df))
        ;; For each edge (a -> b): walk up from a in dominator tree
        ;; until we reach idom(b), adding b to DF of each node along the way
        (dolist (node cfg)
          (let ((a (car node))
                (succs (nth 0 (cdr node))))
            (dolist (b succs)
              (let ((runner a))
                (while (and runner (not (eq runner (cdr (assq b idom)))))
                  (unless (memq b (gethash runner df))
                    (puthash runner (cons b (gethash runner df)) df))
                  (setq runner (cdr (assq runner idom))))))))
        ;; Convert to alist
        (let ((result nil))
          (maphash (lambda (k v) (push (cons k (sort v #'string<)) result)) df)
          (sort result (lambda (a b) (string< (symbol-name (car a))
                                               (symbol-name (car b)))))))))

  (fset 'neovm--ssa-insert-phis
    (lambda (cfg dom-frontiers)
      ;; For each variable defined in a block, insert phi at its dominance frontier.
      ;; Returns alist of (block . list-of-phi-vars)
      (let ((phi-sites (make-hash-table))
            (var-defs (make-hash-table)))  ;; var -> blocks that define it
        ;; Collect definition sites
        (dolist (node cfg)
          (let ((block (car node))
                (defs (nth 1 (cdr node))))
            (dolist (var defs)
              (puthash var (cons block (or (gethash var var-defs) nil)) var-defs))))
        ;; For each variable, insert phis at dominance frontiers iteratively
        (maphash
         (lambda (var def-blocks)
           (let ((worklist (copy-sequence def-blocks))
                 (processed nil))
             (while worklist
               (let* ((block (pop worklist))
                      (frontiers (cdr (assq block dom-frontiers))))
                 (dolist (df-block frontiers)
                   (unless (memq var (gethash df-block phi-sites))
                     (puthash df-block
                              (cons var (or (gethash df-block phi-sites) nil))
                              phi-sites)
                     ;; Phi counts as a definition -> add to worklist
                     (unless (memq df-block processed)
                       (push df-block worklist)
                       (push df-block processed))))))))
         var-defs)
        ;; Convert to alist
        (let ((result nil))
          (maphash (lambda (k v) (push (cons k (sort v #'string<)) result)) phi-sites)
          (sort result (lambda (a b) (string< (symbol-name (car a))
                                               (symbol-name (car b)))))))))

  (unwind-protect
      (let* (;; Diamond CFG:  entry -> B1, entry -> B2, B1 -> merge, B2 -> merge
             ;; B1 defines x, B2 defines x -> phi(x) needed at merge
             (cfg '((entry  . ((B1 B2) () ()))
                     (B1     . ((merge) (x) ()))
                     (B2     . ((merge) (x y) ()))
                     (merge  . (() () (x y)))))
             (idom '((entry . nil) (B1 . entry) (B2 . entry) (merge . entry)))
             (df (funcall 'neovm--ssa-compute-dom-frontiers cfg idom))
             (phis (funcall 'neovm--ssa-insert-phis cfg df))
             ;; More complex: loop with if-else inside
             (cfg2 '((entry . ((header) (i) ()))
                      (header . ((body exit) () (i)))
                      (body   . ((left right) () ()))
                      (left   . ((latch) (x) ()))
                      (right  . ((latch) (x) ()))
                      (latch  . ((header) (i) (x)))
                      (exit   . (() () (i x)))))
             (idom2 '((entry . nil) (header . entry) (body . header)
                       (left . body) (right . body) (latch . header) (exit . header)))
             (df2 (funcall 'neovm--ssa-compute-dom-frontiers cfg2 idom2))
             (phis2 (funcall 'neovm--ssa-insert-phis cfg2 df2)))
        (list df phis df2 phis2))
    (fmakunbound 'neovm--ssa-compute-dom-frontiers)
    (fmakunbound 'neovm--ssa-insert-phis)))"#;
    let expect = expect_test::expect![[
        r#""OK (((B1 merge) (B2 merge) (entry) (merge)) ((merge x y)) ((body latch) (entry) (exit) (header header) (latch header) (left latch) (right latch)) ((header i x) (latch x)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dominance tree construction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ssa_dominance_tree() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute the dominator tree for a CFG using iterative data-flow.
    // dom(n) = {n} union intersection(dom(p) for p in preds(n))
    let form = r#"(progn
  (fset 'neovm--ssa-compute-dominators
    (lambda (nodes entry preds-table)
      ;; nodes: list of block names
      ;; preds-table: alist of (block . predecessor-list)
      ;; Returns alist of (block . dominator-set)
      (let ((dom (make-hash-table)))
        ;; Initialize: entry dominated by itself, others by all nodes
        (puthash entry (list entry) dom)
        (dolist (n nodes)
          (unless (eq n entry)
            (puthash n (copy-sequence nodes) dom)))
        ;; Iterate until fixed point
        (let ((changed t))
          (while changed
            (setq changed nil)
            (dolist (n nodes)
              (unless (eq n entry)
                (let* ((preds (cdr (assq n preds-table)))
                       ;; Intersect dom sets of all predecessors
                       (new-dom (if preds
                                    (let ((inter (copy-sequence (gethash (car preds) dom))))
                                      (dolist (p (cdr preds))
                                        (setq inter
                                              (seq-filter (lambda (x) (memq x (gethash p dom)))
                                                          inter)))
                                      inter)
                                  nil)))
                  ;; Add n itself
                  (unless (memq n new-dom)
                    (push n new-dom))
                  (setq new-dom (sort new-dom (lambda (a b)
                                                (string< (symbol-name a) (symbol-name b)))))
                  (unless (equal new-dom (sort (copy-sequence (gethash n dom))
                                               (lambda (a b)
                                                 (string< (symbol-name a) (symbol-name b)))))
                    (puthash n new-dom dom)
                    (setq changed t)))))))
        ;; Convert to sorted alist
        (let ((result nil))
          (dolist (n nodes)
            (push (cons n (sort (copy-sequence (gethash n dom))
                                (lambda (a b) (string< (symbol-name a) (symbol-name b)))))
                  result))
          (nreverse result)))))

  (fset 'neovm--ssa-idom-from-dom
    (lambda (dom-alist entry)
      ;; Compute immediate dominator from dominator sets.
      ;; idom(n) = the dominator of n (other than n itself) that is
      ;; dominated by all other dominators of n.
      (let ((result nil))
        (dolist (entry-pair dom-alist)
          (let* ((n (car entry-pair))
                 (doms (cdr entry-pair))
                 (strict-doms (seq-filter (lambda (d) (not (eq d n))) doms))
                 (idom-node nil))
            (if (eq n entry)
                (push (cons n nil) result)
              ;; idom is the strict dominator with the largest dom set
              ;; (closest to n in the tree)
              (let ((best nil) (best-size 0))
                (dolist (d strict-doms)
                  (let ((d-dom-size (length (cdr (assq d dom-alist)))))
                    (when (> d-dom-size best-size)
                      (setq best d best-size d-dom-size))))
                (push (cons n best) result)))))
        (nreverse result))))

  (unwind-protect
      (let* ((nodes '(A B C D E F))
             (preds '((A . ())
                       (B . (A))
                       (C . (A))
                       (D . (B C))
                       (E . (D))
                       (F . (D E))))
             (dom (funcall 'neovm--ssa-compute-dominators nodes 'A preds))
             (idom (funcall 'neovm--ssa-idom-from-dom dom 'A))
             ;; Verify properties:
             ;; 1. Entry dominates everything
             (entry-dom-all (let ((ok t))
                              (dolist (pair dom)
                                (unless (memq 'A (cdr pair))
                                  (setq ok nil)))
                              ok))
             ;; 2. Each node dominates itself
             (self-dom (let ((ok t))
                         (dolist (pair dom)
                           (unless (memq (car pair) (cdr pair))
                             (setq ok nil)))
                         ok)))
        (list dom idom entry-dom-all self-dom))
    (fmakunbound 'neovm--ssa-compute-dominators)
    (fmakunbound 'neovm--ssa-idom-from-dom)))"#;
    let expect = expect_test::expect![[
        r#""OK (((A A) (B A B) (C A C) (D A D) (E A D E) (F A D F)) ((A) (B . A) (C . A) (D . A) (E . D) (F . D)) t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// SSA-based constant propagation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ssa_constant_propagation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Sparse conditional constant propagation on SSA form:
    // Track lattice values (top, constant, bottom) for each SSA variable.
    // Propagate constants through assignments and phi functions.
    let form = r#"(progn
  ;; SSA instruction: (assign var_N expr) | (phi var_N (var_a var_b ...)) | (return var_N)
  ;; Lattice: 'top (undefined), (const . val), 'bottom (non-constant)

  (fset 'neovm--ssa-cp-lattice-meet
    (lambda (a b)
      ;; Meet operation: top ^ x = x, const ^ const = const if equal else bottom
      (cond
       ((eq a 'top) b)
       ((eq b 'top) a)
       ((eq a 'bottom) 'bottom)
       ((eq b 'bottom) 'bottom)
       ((and (consp a) (consp b) (eq (car a) 'const) (eq (car b) 'const))
        (if (equal (cdr a) (cdr b)) a 'bottom))
       (t 'bottom))))

  (fset 'neovm--ssa-cp-eval-expr
    (lambda (expr lattice)
      ;; Evaluate expression using lattice values.
      (cond
       ((numberp expr) (cons 'const expr))
       ((symbolp expr) (or (gethash expr lattice) 'top))
       ((and (listp expr) (memq (car expr) '(+ - * /)))
        (let ((l (funcall 'neovm--ssa-cp-eval-expr (nth 1 expr) lattice))
              (r (funcall 'neovm--ssa-cp-eval-expr (nth 2 expr) lattice)))
          (cond
           ((or (eq l 'bottom) (eq r 'bottom)) 'bottom)
           ((or (eq l 'top) (eq r 'top)) 'top)
           ((and (consp l) (consp r))
            (let ((lv (cdr l)) (rv (cdr r)))
              (cons 'const
                    (cond ((eq (car expr) '+) (+ lv rv))
                          ((eq (car expr) '-) (- lv rv))
                          ((eq (car expr) '*) (* lv rv))
                          ((eq (car expr) '/) (if (= rv 0) 'bottom (/ lv rv)))))))
           (t 'top))))
       (t 'bottom))))

  (fset 'neovm--ssa-cp-propagate
    (lambda (instrs)
      ;; Iterative constant propagation on SSA instructions.
      (let ((lattice (make-hash-table))
            (changed t)
            (max-iters 20)
            (iters 0))
        ;; Initialize all variables to top
        (dolist (instr instrs)
          (when (or (eq (car instr) 'assign) (eq (car instr) 'phi))
            (puthash (nth 1 instr) 'top lattice)))
        ;; Iterate
        (while (and changed (< iters max-iters))
          (setq changed nil iters (1+ iters))
          (dolist (instr instrs)
            (cond
             ((eq (car instr) 'assign)
              (let* ((var (nth 1 instr))
                     (new-val (funcall 'neovm--ssa-cp-eval-expr (nth 2 instr) lattice))
                     (old-val (gethash var lattice)))
                (unless (equal new-val old-val)
                  (puthash var new-val lattice)
                  (setq changed t))))
             ((eq (car instr) 'phi)
              (let* ((var (nth 1 instr))
                     (args (nth 2 instr))
                     (new-val 'top))
                (dolist (arg args)
                  (setq new-val (funcall 'neovm--ssa-cp-lattice-meet
                                         new-val
                                         (or (gethash arg lattice) 'top))))
                (unless (equal new-val (gethash var lattice))
                  (puthash var new-val lattice)
                  (setq changed t)))))))
        ;; Return sorted lattice entries
        (let ((result nil))
          (maphash (lambda (k v) (push (cons k v) result)) lattice)
          (sort result (lambda (a b) (string< (symbol-name (car a))
                                               (symbol-name (car b)))))))))

  (unwind-protect
      (let* (;; x_0 = 5; y_0 = 10; z_0 = x_0 + y_0; w_0 = z_0 * 2; return w_0
             ;; All should be constant
             (prog1 '((assign x_0 5)
                       (assign y_0 10)
                       (assign z_0 (+ x_0 y_0))
                       (assign w_0 (* z_0 2))
                       (return w_0)))
             (cp1 (funcall 'neovm--ssa-cp-propagate prog1))
             ;; With phi: x_0 = 5 in one branch, x_1 = 5 in other,
             ;; x_2 = phi(x_0, x_1) -> should be constant 5
             (prog2 '((assign x_0 5)
                       (assign x_1 5)
                       (phi x_2 (x_0 x_1))
                       (assign y_0 (+ x_2 1))
                       (return y_0)))
             (cp2 (funcall 'neovm--ssa-cp-propagate prog2))
             ;; Phi with different values -> bottom
             (prog3 '((assign x_0 5)
                       (assign x_1 10)
                       (phi x_2 (x_0 x_1))
                       (assign y_0 (+ x_2 1))
                       (return y_0)))
             (cp3 (funcall 'neovm--ssa-cp-propagate prog3)))
        (list cp1 cp2 cp3))
    (fmakunbound 'neovm--ssa-cp-lattice-meet)
    (fmakunbound 'neovm--ssa-cp-eval-expr)
    (fmakunbound 'neovm--ssa-cp-propagate)))"#;
    let expect = expect_test::expect![[
        r#""OK (((w_0 const . 30) (x_0 const . 5) (y_0 const . 10) (z_0 const . 15)) ((x_0 const . 5) (x_1 const . 5) (x_2 const . 5) (y_0 const . 6)) ((x_0 const . 5) (x_1 const . 10) (x_2 . bottom) (y_0 . bottom)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// SSA-based dead code elimination
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ssa_dead_code_elimination() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // In SSA, dead code elimination is simpler because each variable
    // has exactly one definition. If a variable is never used, its
    // definition (and phi) can be removed.
    let form = r#"(progn
  (fset 'neovm--ssa-dce-collect-uses
    (lambda (instrs)
      ;; Collect all variables that appear in uses (not as definition targets).
      (let ((used (make-hash-table)))
        (dolist (instr instrs)
          (cond
           ((eq (car instr) 'assign)
            ;; Scan expression for variable references
            (let ((expr (nth 2 instr)))
              (cond
               ((symbolp expr) (puthash expr t used))
               ((and (listp expr) (memq (car expr) '(+ - * /)))
                (when (symbolp (nth 1 expr)) (puthash (nth 1 expr) t used))
                (when (symbolp (nth 2 expr)) (puthash (nth 2 expr) t used))))))
           ((eq (car instr) 'phi)
            (dolist (arg (nth 2 instr))
              (puthash arg t used)))
           ((eq (car instr) 'return)
            (when (symbolp (nth 1 instr))
              (puthash (nth 1 instr) t used)))))
        used)))

  (fset 'neovm--ssa-dce-eliminate
    (lambda (instrs)
      ;; Iteratively remove dead definitions until stable.
      (let ((changed t)
            (current instrs))
        (while changed
          (setq changed nil)
          (let ((used (funcall 'neovm--ssa-dce-collect-uses current))
                (new-instrs nil))
            (dolist (instr current)
              (if (and (memq (car instr) '(assign phi))
                       (not (gethash (nth 1 instr) used)))
                  (setq changed t)  ;; Dead, skip
                (push instr new-instrs)))
            (setq current (nreverse new-instrs))))
        current)))

  (unwind-protect
      (let* (;; x_0 = 1; y_0 = 2; z_0 = x_0 + y_0; dead_0 = 999; return z_0
             ;; dead_0 should be eliminated
             (prog1 '((assign x_0 1)
                       (assign y_0 2)
                       (assign z_0 (+ x_0 y_0))
                       (assign dead_0 999)
                       (return z_0)))
             (dce1 (funcall 'neovm--ssa-dce-eliminate prog1))
             ;; Chain of dead defs: a_0 = 1; b_0 = a_0; c_0 = b_0; return x_0
             ;; where x_0 is defined elsewhere - all of a,b,c are dead
             (prog2 '((assign x_0 42)
                       (assign a_0 1)
                       (assign b_0 a_0)
                       (assign c_0 b_0)
                       (return x_0)))
             (dce2 (funcall 'neovm--ssa-dce-eliminate prog2))
             ;; Dead phi: phi x_2 = phi(x_0, x_1) but x_2 never used
             (prog3 '((assign x_0 5)
                       (assign x_1 10)
                       (phi x_2 (x_0 x_1))
                       (assign y_0 7)
                       (return y_0)))
             (dce3 (funcall 'neovm--ssa-dce-eliminate prog3)))
        (list
         (length dce1) dce1
         (length dce2) dce2
         (length dce3) dce3
         ;; Verify dead_0 is gone from prog1
         (not (seq-find (lambda (i) (and (eq (car i) 'assign)
                                          (eq (nth 1 i) 'dead_0)))
                        dce1))
         ;; Verify a_0, b_0, c_0 all gone from prog2
         (not (seq-find (lambda (i) (and (eq (car i) 'assign)
                                          (memq (nth 1 i) '(a_0 b_0 c_0))))
                        dce2))))
    (fmakunbound 'neovm--ssa-dce-collect-uses)
    (fmakunbound 'neovm--ssa-dce-eliminate)))"#;
    let expect = expect_test::expect![[
        r#""OK (4 ((assign x_0 1) (assign y_0 2) (assign z_0 (+ x_0 y_0)) (return z_0)) 2 ((assign x_0 42) (return x_0)) 2 ((assign y_0 7) (return y_0)) t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// SSA destruction: phi elimination via copy insertion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ssa_destruction_phi_elimination() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Convert out of SSA by replacing phi functions with copy instructions.
    // Each phi (phi x_2 (x_0 x_1)) in block B with predecessors P1, P2
    // becomes: insert "x_2 = x_0" at end of P1, "x_2 = x_1" at end of P2.
    let form = r#"(progn
  (fset 'neovm--ssa-destroy-phis
    (lambda (blocks)
      ;; blocks: alist of (block-id predecessors instrs)
      ;; Returns new blocks with phis replaced by copies.
      (let ((copies-to-insert (make-hash-table))  ;; block -> list of (assign dest src)
            (new-blocks nil))
        ;; First pass: find phis and schedule copies
        (dolist (block blocks)
          (let ((block-id (nth 0 block))
                (preds (nth 1 block))
                (instrs (nth 2 block))
                (non-phi-instrs nil))
            (dolist (instr instrs)
              (if (eq (car instr) 'phi)
                  ;; phi x_2 (x_0 x_1) with preds (P1 P2)
                  ;; -> insert copy x_2=x_0 in P1, x_2=x_1 in P2
                  (let ((dest (nth 1 instr))
                        (args (nth 2 instr))
                        (i 0))
                    (dolist (arg args)
                      (let* ((pred (nth i preds))
                             (existing (or (gethash pred copies-to-insert) nil)))
                        (puthash pred (append existing (list (list 'copy dest arg)))
                                 copies-to-insert))
                      (setq i (1+ i))))
                (push instr non-phi-instrs)))
            (push (list block-id preds (nreverse non-phi-instrs)) new-blocks)))
        ;; Second pass: insert copies at end of predecessor blocks
        (let ((final-blocks nil))
          (dolist (block (nreverse new-blocks))
            (let* ((block-id (nth 0 block))
                   (preds (nth 1 block))
                   (instrs (nth 2 block))
                   (copies (gethash block-id copies-to-insert))
                   (new-instrs (if copies
                                   (append instrs copies)
                                 instrs)))
              (push (list block-id preds new-instrs) final-blocks)))
          (nreverse final-blocks)))))

  (unwind-protect
      (let* (;; Diamond CFG:
             ;; entry -> B1, entry -> B2, B1 -> merge, B2 -> merge
             ;; merge has phi(x_2, (x_0, x_1))
             (blocks '((entry () ((assign x_init 0) (branch-cond)))
                        (B1 (entry) ((assign x_0 5)))
                        (B2 (entry) ((assign x_1 10)))
                        (merge (B1 B2) ((phi x_2 (x_0 x_1))
                                        (assign result (+ x_2 1))
                                        (return result)))))
             (destroyed (funcall 'neovm--ssa-destroy-phis blocks))
             ;; Loop with phi at header:
             ;; entry -> header, latch -> header
             ;; header has phi(i_2, (i_0, i_1))
             (loop-blocks '((entry () ((assign i_0 0)))
                             (header (entry latch)
                                     ((phi i_2 (i_0 i_1))
                                      (branch-if (< i_2 10) body exit)))
                             (body (header) ((assign tmp (+ i_2 1))))
                             (latch (body) ((assign i_1 tmp)))
                             (exit (header) ((return i_2)))))
             (destroyed2 (funcall 'neovm--ssa-destroy-phis loop-blocks))
             ;; Verify no phis remain
             (no-phis (let ((found nil))
                        (dolist (b destroyed)
                          (dolist (i (nth 2 b))
                            (when (eq (car i) 'phi)
                              (setq found t))))
                        (not found)))
             ;; Verify copies were inserted
             (b1-has-copy (seq-find (lambda (i) (eq (car i) 'copy))
                                     (nth 2 (nth 1 destroyed))))
             (b2-has-copy (seq-find (lambda (i) (eq (car i) 'copy))
                                     (nth 2 (nth 2 destroyed)))))
        (list destroyed destroyed2
              no-phis
              (and b1-has-copy t)
              (and b2-has-copy t)))
    (fmakunbound 'neovm--ssa-destroy-phis)))"#;
    let expect = expect_test::expect![[
        r#""OK (((entry nil ((assign x_init 0) (branch-cond))) (B1 (entry) ((assign x_0 5) (copy x_2 x_0))) (B2 (entry) ((assign x_1 10) (copy x_2 x_1))) (merge (B1 B2) ((assign result (+ x_2 1)) (return result)))) ((entry nil ((assign i_0 0) (copy i_2 i_0))) (header (entry latch) ((branch-if (< i_2 10) body exit))) (body (header) ((assign tmp (+ i_2 1)))) (latch (body) ((assign i_1 tmp) (copy i_2 i_1))) (exit (header) ((return i_2)))) t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Copy propagation on SSA form
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ssa_copy_propagation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // In SSA, copy propagation replaces uses of a variable defined as
    // a simple copy (x = y) with the source variable y.
    // This simplifies the code after phi elimination.
    let form = r#"(progn
  (fset 'neovm--ssa-copy-prop
    (lambda (instrs)
      ;; Find all copies: (assign dest src) where src is a symbol
      ;; or (copy dest src).
      ;; Build mapping dest -> src, then replace all uses.
      (let ((copy-map (make-hash-table)))
        ;; First pass: identify copies
        (dolist (instr instrs)
          (when (and (memq (car instr) '(assign copy))
                     (symbolp (nth 2 instr)))
            (puthash (nth 1 instr) (nth 2 instr) copy-map)))
        ;; Chase copy chains: if a -> b -> c, resolve a -> c
        (let ((changed t))
          (while changed
            (setq changed nil)
            (maphash (lambda (k v)
                       (let ((target (gethash v copy-map)))
                         (when (and target (not (eq target v)))
                           (puthash k target copy-map)
                           (setq changed t))))
                     copy-map)))

        (fset 'neovm--ssa-cp-subst
          (lambda (sym)
            (or (gethash sym copy-map) sym)))

        ;; Second pass: substitute in all instructions
        (let ((result nil))
          (dolist (instr instrs)
            (cond
             ;; Skip pure copies (they're eliminated)
             ((and (memq (car instr) '(assign copy))
                   (symbolp (nth 2 instr))
                   (gethash (nth 1 instr) copy-map))
              nil)  ;; Eliminate the copy
             ;; Substitute in expressions
             ((eq (car instr) 'assign)
              (let ((expr (nth 2 instr)))
                (if (and (listp expr) (memq (car expr) '(+ - * /)))
                    (push (list 'assign (nth 1 instr)
                                (list (car expr)
                                      (funcall 'neovm--ssa-cp-subst (nth 1 expr))
                                      (if (symbolp (nth 2 expr))
                                          (funcall 'neovm--ssa-cp-subst (nth 2 expr))
                                        (nth 2 expr))))
                          result)
                  (push instr result))))
             ((eq (car instr) 'return)
              (push (list 'return (funcall 'neovm--ssa-cp-subst (nth 1 instr))) result))
             (t (push instr result))))
          (fmakunbound 'neovm--ssa-cp-subst)
          (nreverse result)))))

  (unwind-protect
      (let* (;; a = 5; b = a; c = b; d = c + 1; return d
             ;; After copy prop: a = 5; d = a + 1; return d
             (prog1 '((assign a 5)
                       (assign b a)
                       (assign c b)
                       (assign d (+ c 1))
                       (return d)))
             (cp1 (funcall 'neovm--ssa-copy-prop prog1))
             ;; Copy chain from phi elimination:
             ;; x_0 = 5; x_copy = x_0; y = x_copy + 10; return y
             ;; After: x_0 = 5; y = x_0 + 10; return y
             (prog2 '((assign x_0 5)
                       (copy x_copy x_0)
                       (assign y (+ x_copy 10))
                       (return y)))
             (cp2 (funcall 'neovm--ssa-copy-prop prog2))
             ;; Multiple independent copies
             (prog3 '((assign a 1)
                       (assign b 2)
                       (assign a2 a)
                       (assign b2 b)
                       (assign c (+ a2 b2))
                       (return c)))
             (cp3 (funcall 'neovm--ssa-copy-prop prog3)))
        (list cp1 (length cp1)
              cp2 (length cp2)
              cp3 (length cp3)
              ;; Verify copies are eliminated
              (not (seq-find (lambda (i) (memq (car i) '(copy))) cp1))
              (not (seq-find (lambda (i) (memq (car i) '(copy))) cp2))))
    (fmakunbound 'neovm--ssa-copy-prop)))"#;
    let expect = expect_test::expect![[
        r#""OK (((assign a 5) (assign d (+ a 1)) (return d)) 3 ((assign x_0 5) (assign y (+ x_0 10)) (return y)) 3 ((assign a 1) (assign b 2) (assign c (+ a b)) (return c)) 4 t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
