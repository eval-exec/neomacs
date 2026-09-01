//! Oracle parity tests for dataflow analysis patterns in Elisp:
//! reaching definitions, live variable analysis, available expressions,
//! use-def chains. Models a simple basic-block program as a list of
//! instructions, computes IN/OUT sets using fixed-point iteration.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Reaching definitions analysis via fixed-point iteration
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dataflow_reaching_definitions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A program is a list of basic blocks.
    // Each block: (label instructions successors)
    // Each instruction: (def var) or (use var) or (def-use var1 var2 ...)
    // Reaching definitions: which definitions of a variable reach each point.
    // Forward analysis: OUT[B] = GEN[B] union (IN[B] - KILL[B])
    //                   IN[B] = union of OUT[pred] for all predecessors
    let form = r#"(progn
  ;; Set utilities
  (fset 'neovm--test-set-union
    (lambda (a b)
      (let ((result (copy-sequence a)))
        (dolist (x b)
          (unless (member x result)
            (setq result (cons x result))))
        (sort result (lambda (x y) (string< (format "%s" x) (format "%s" y)))))))

  (fset 'neovm--test-set-diff
    (lambda (a b)
      (let ((result nil))
        (dolist (x a)
          (unless (member x b)
            (setq result (cons x result))))
        (sort result (lambda (x y) (string< (format "%s" x) (format "%s" y)))))))

  (fset 'neovm--test-set-equal
    (lambda (a b)
      (and (= (length a) (length b))
           (null (funcall 'neovm--test-set-diff a b)))))

  ;; Compute GEN and KILL sets for a block
  ;; GEN[B] = definitions generated in B (last def of each var)
  ;; KILL[B] = all defs of vars that B defines, from other blocks
  (fset 'neovm--test-compute-gen
    (lambda (block-label instrs)
      "Return list of (block-label . var) for vars defined in this block."
      (let ((defs nil))
        (dolist (instr instrs)
          (when (eq (car instr) 'def)
            (let ((var (cadr instr)))
              ;; Keep only last def of each var
              (setq defs (cons (cons block-label var)
                               (let ((filtered nil))
                                 (dolist (d defs)
                                   (unless (eq (cdr d) var)
                                     (setq filtered (cons d filtered))))
                                 (nreverse filtered)))))))
        defs)))

  (fset 'neovm--test-compute-kill
    (lambda (block-label instrs all-defs)
      "Return all defs of vars defined in block, from OTHER blocks."
      (let ((my-vars nil))
        (dolist (instr instrs)
          (when (eq (car instr) 'def)
            (unless (memq (cadr instr) my-vars)
              (setq my-vars (cons (cadr instr) my-vars)))))
        (let ((killed nil))
          (dolist (d all-defs)
            (when (and (memq (cdr d) my-vars)
                       (not (eq (car d) block-label)))
              (setq killed (cons d killed))))
          killed))))

  ;; Fixed-point reaching definitions
  (fset 'neovm--test-reaching-defs
    (lambda (blocks)
      "BLOCKS: list of (label instrs successors).
Return alist of (label . (IN OUT)) for each block."
      ;; Collect all definitions
      (let ((all-defs nil))
        (dolist (blk blocks)
          (let ((gen (funcall 'neovm--test-compute-gen (car blk) (cadr blk))))
            (setq all-defs (append gen all-defs))))
        ;; Build predecessor map
        (let ((preds (make-hash-table :test 'eq)))
          (dolist (blk blocks)
            (puthash (car blk) nil preds))
          (dolist (blk blocks)
            (dolist (succ (nth 2 blk))
              (puthash succ (cons (car blk) (gethash succ preds)) preds)))
          ;; Compute GEN/KILL for each block
          (let ((gen-map (make-hash-table :test 'eq))
                (kill-map (make-hash-table :test 'eq))
                (in-map (make-hash-table :test 'eq))
                (out-map (make-hash-table :test 'eq)))
            (dolist (blk blocks)
              (puthash (car blk)
                       (funcall 'neovm--test-compute-gen (car blk) (cadr blk))
                       gen-map)
              (puthash (car blk)
                       (funcall 'neovm--test-compute-kill (car blk) (cadr blk) all-defs)
                       kill-map)
              (puthash (car blk) nil in-map)
              (puthash (car blk) nil out-map))
            ;; Fixed-point iteration
            (let ((changed t)
                  (iterations 0))
              (while changed
                (setq changed nil)
                (setq iterations (1+ iterations))
                (dolist (blk blocks)
                  (let ((label (car blk)))
                    ;; IN[B] = union of OUT[p] for p in preds[B]
                    (let ((new-in nil))
                      (dolist (p (gethash label preds))
                        (setq new-in (funcall 'neovm--test-set-union
                                              new-in (gethash p out-map))))
                      (puthash label new-in in-map)
                      ;; OUT[B] = GEN[B] union (IN[B] - KILL[B])
                      (let ((new-out
                             (funcall 'neovm--test-set-union
                                      (gethash label gen-map)
                                      (funcall 'neovm--test-set-diff
                                               new-in
                                               (gethash label kill-map)))))
                        (unless (funcall 'neovm--test-set-equal
                                         new-out (gethash label out-map))
                          (setq changed t))
                        (puthash label new-out out-map))))))
              ;; Collect results
              (let ((result nil))
                (dolist (blk blocks)
                  (let ((label (car blk)))
                    (setq result
                          (cons (list label
                                      :in (gethash label in-map)
                                      :out (gethash label out-map))
                                result))))
                (list :iterations iterations
                      :results (nreverse result)))))))))

  (unwind-protect
      ;; Test program:
      ;; B1: x = ...; y = ...   -> B2, B3
      ;; B2: z = ...; x = ...   -> B4
      ;; B3: y = ...             -> B4
      ;; B4: use x, y, z
      (funcall 'neovm--test-reaching-defs
               '((B1 ((def x) (def y)) (B2 B3))
                 (B2 ((def z) (def x)) (B4))
                 (B3 ((def y))         (B4))
                 (B4 ((use x) (use y) (use z)) ())))
    (fmakunbound 'neovm--test-reaching-defs)
    (fmakunbound 'neovm--test-compute-gen)
    (fmakunbound 'neovm--test-compute-kill)
    (fmakunbound 'neovm--test-set-union)
    (fmakunbound 'neovm--test-set-diff)
    (fmakunbound 'neovm--test-set-equal)))"#;
    let expect = expect_test::expect![[
        r#""OK (:iterations 2 :results ((B1 :in nil :out ((B1 . x) (B1 . y))) (B2 :in ((B1 . x) (B1 . y)) :out ((B1 . y) (B2 . x) (B2 . z))) (B3 :in ((B1 . x) (B1 . y)) :out ((B1 . x) (B3 . y))) (B4 :in ((B1 . x) (B1 . y) (B2 . x) (B2 . z) (B3 . y)) :out ((B1 . x) (B1 . y) (B2 . x) (B2 . z) (B3 . y)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Live variable analysis (backward dataflow)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dataflow_live_variables() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Backward analysis:
    // IN[B] = USE[B] union (OUT[B] - DEF[B])
    // OUT[B] = union of IN[succ] for all successors
    let form = r#"(progn
  (fset 'neovm--test-set-union-syms
    (lambda (a b)
      (let ((result (copy-sequence a)))
        (dolist (x b)
          (unless (memq x result)
            (setq result (cons x result))))
        (sort result (lambda (x y) (string< (symbol-name x) (symbol-name y)))))))

  (fset 'neovm--test-set-diff-syms
    (lambda (a b)
      (let ((result nil))
        (dolist (x a)
          (unless (memq x b)
            (setq result (cons x result))))
        (sort result (lambda (x y) (string< (symbol-name x) (symbol-name y)))))))

  (fset 'neovm--test-set-equal-syms
    (lambda (a b)
      (and (= (length a) (length b))
           (null (funcall 'neovm--test-set-diff-syms a b)))))

  ;; Extract USE and DEF sets from instructions
  (fset 'neovm--test-extract-use-def
    (lambda (instrs)
      "Return (USE-set DEF-set) for a block.
USE = variables used before being defined.
DEF = variables defined in the block."
      (let ((use-set nil) (def-set nil))
        (dolist (instr instrs)
          (cond
           ((eq (car instr) 'use)
            (unless (memq (cadr instr) def-set)
              (unless (memq (cadr instr) use-set)
                (setq use-set (cons (cadr instr) use-set)))))
           ((eq (car instr) 'def)
            (unless (memq (cadr instr) def-set)
              (setq def-set (cons (cadr instr) def-set))))
           ((eq (car instr) 'def-use)
            ;; Uses happen before def
            (dolist (v (cddr instr))
              (unless (memq v def-set)
                (unless (memq v use-set)
                  (setq use-set (cons v use-set)))))
            (unless (memq (cadr instr) def-set)
              (setq def-set (cons (cadr instr) def-set))))))
        (list use-set def-set))))

  ;; Fixed-point backward analysis
  (fset 'neovm--test-live-variables
    (lambda (blocks)
      (let ((use-map (make-hash-table :test 'eq))
            (def-map (make-hash-table :test 'eq))
            (in-map (make-hash-table :test 'eq))
            (out-map (make-hash-table :test 'eq))
            (succ-map (make-hash-table :test 'eq)))
        ;; Initialize
        (dolist (blk blocks)
          (let ((label (car blk))
                (ud (funcall 'neovm--test-extract-use-def (cadr blk))))
            (puthash label (car ud) use-map)
            (puthash label (cadr ud) def-map)
            (puthash label (nth 2 blk) succ-map)
            (puthash label nil in-map)
            (puthash label nil out-map)))
        ;; Fixed-point iteration (process in reverse order for efficiency)
        (let ((changed t) (iterations 0)
              (labels (nreverse (mapcar #'car blocks))))
          (while changed
            (setq changed nil)
            (setq iterations (1+ iterations))
            (dolist (label labels)
              ;; OUT[B] = union of IN[succ]
              (let ((new-out nil))
                (dolist (s (gethash label succ-map))
                  (setq new-out (funcall 'neovm--test-set-union-syms
                                         new-out (gethash s in-map))))
                (puthash label new-out out-map)
                ;; IN[B] = USE[B] union (OUT[B] - DEF[B])
                (let ((new-in
                       (funcall 'neovm--test-set-union-syms
                                (gethash label use-map)
                                (funcall 'neovm--test-set-diff-syms
                                         new-out
                                         (gethash label def-map)))))
                  (unless (funcall 'neovm--test-set-equal-syms
                                   new-in (gethash label in-map))
                    (setq changed t))
                  (puthash label new-in in-map)))))
          ;; Collect
          (let ((result nil))
            (dolist (blk blocks)
              (let ((label (car blk)))
                (setq result (cons (list label
                                         :in (gethash label in-map)
                                         :out (gethash label out-map))
                                   result))))
            (list :iterations iterations
                  :results (nreverse result)))))))

  (unwind-protect
      ;; Test: simple if-then-else
      ;; B1: x = input; y = input  -> B2, B3
      ;; B2: z = x + y             -> B4
      ;; B3: z = x - y             -> B4
      ;; B4: print z
      ;; Expected: x,y live at B2,B3 entry; z live at B4 entry
      (funcall 'neovm--test-live-variables
               '((B1 ((def x) (def y))          (B2 B3))
                 (B2 ((use x) (use y) (def z))  (B4))
                 (B3 ((use x) (use y) (def z))  (B4))
                 (B4 ((use z))                   ())))
    (fmakunbound 'neovm--test-live-variables)
    (fmakunbound 'neovm--test-extract-use-def)
    (fmakunbound 'neovm--test-set-union-syms)
    (fmakunbound 'neovm--test-set-diff-syms)
    (fmakunbound 'neovm--test-set-equal-syms)))"#;
    let expect = expect_test::expect![[
        r#""OK (:iterations 2 :results ((B1 :in nil :out (x y)) (B2 :in (x y) :out (z)) (B3 :in (x y) :out (z)) (B4 :in (z) :out nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Available expressions analysis (forward)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dataflow_available_expressions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Forward analysis with intersection (must-analysis):
    // IN[B] = intersection of OUT[pred] for all predecessors (for non-entry blocks)
    // OUT[B] = GEN[B] union (IN[B] - KILL[B])
    // An expression "x op y" is killed by any def of x or y.
    let form = r#"(progn
  (fset 'neovm--test-set-intersect
    (lambda (a b)
      (let ((result nil))
        (dolist (x a)
          (when (member x b)
            (setq result (cons x result))))
        (sort result (lambda (x y) (string< (format "%s" x) (format "%s" y)))))))

  (fset 'neovm--test-set-union-str
    (lambda (a b)
      (let ((result (copy-sequence a)))
        (dolist (x b)
          (unless (member x result)
            (setq result (cons x result))))
        (sort result (lambda (x y) (string< (format "%s" x) (format "%s" y)))))))

  (fset 'neovm--test-set-diff-str
    (lambda (a b)
      (let ((result nil))
        (dolist (x a)
          (unless (member x b)
            (setq result (cons x result))))
        (sort result (lambda (x y) (string< (format "%s" x) (format "%s" y)))))))

  (fset 'neovm--test-set-equal-str
    (lambda (a b)
      (and (= (length a) (length b))
           (null (funcall 'neovm--test-set-diff-str a b)))))

  ;; An expression: (op var1 var2)
  ;; An instruction: (def var) kills all exprs containing var
  ;;                 (expr var op var2) generates expression (op var2 ...) and defines var

  (fset 'neovm--test-avail-gen-kill
    (lambda (instrs all-exprs)
      "Return (GEN KILL) for a block."
      (let ((gen nil) (kill nil))
        (dolist (instr instrs)
          (cond
           ((eq (car instr) 'expr)
            ;; (expr target-var expr-repr)
            ;; Kills all exprs containing target-var first
            (let ((var (cadr instr))
                  (expr-rep (nth 2 instr)))
              (dolist (e all-exprs)
                (when (memq var (cdr e))
                  (unless (member e kill)
                    (setq kill (cons e kill)))
                  (setq gen (delete e gen))))
              ;; Then generates expr-rep
              (unless (member expr-rep kill)
                (unless (member expr-rep gen)
                  (setq gen (cons expr-rep gen))))))
           ((eq (car instr) 'def)
            ;; Kills all exprs containing var
            (let ((var (cadr instr)))
              (dolist (e all-exprs)
                (when (memq var (cdr e))
                  (unless (member e kill)
                    (setq kill (cons e kill)))
                  (setq gen (delete e gen))))))))
        (list gen kill))))

  (fset 'neovm--test-available-expressions
    (lambda (blocks all-exprs)
      (let ((gen-map (make-hash-table :test 'eq))
            (kill-map (make-hash-table :test 'eq))
            (in-map (make-hash-table :test 'eq))
            (out-map (make-hash-table :test 'eq))
            (pred-map (make-hash-table :test 'eq))
            (entry (caar blocks)))
        ;; Build pred map
        (dolist (blk blocks)
          (puthash (car blk) nil pred-map))
        (dolist (blk blocks)
          (dolist (s (nth 2 blk))
            (puthash s (cons (car blk) (gethash s pred-map)) pred-map)))
        ;; Compute gen/kill
        (dolist (blk blocks)
          (let ((gk (funcall 'neovm--test-avail-gen-kill (cadr blk) all-exprs)))
            (puthash (car blk) (car gk) gen-map)
            (puthash (car blk) (cadr gk) kill-map)))
        ;; Initialize: entry IN=empty, all others OUT=all-exprs (for intersection)
        (dolist (blk blocks)
          (let ((label (car blk)))
            (puthash label nil in-map)
            (if (eq label entry)
                (puthash label nil out-map)
              (puthash label (copy-sequence all-exprs) out-map))))
        ;; Fixed-point
        (let ((changed t) (iterations 0))
          (while changed
            (setq changed nil)
            (setq iterations (1+ iterations))
            (dolist (blk blocks)
              (let ((label (car blk)))
                ;; IN[B] = intersection of OUT[pred] (empty if entry or no preds)
                (let ((preds (gethash label pred-map))
                      (new-in nil))
                  (if (or (eq label entry) (null preds))
                      (setq new-in nil)
                    (setq new-in (copy-sequence (gethash (car preds) out-map)))
                    (dolist (p (cdr preds))
                      (setq new-in (funcall 'neovm--test-set-intersect
                                            new-in (gethash p out-map)))))
                  (puthash label new-in in-map)
                  ;; OUT[B] = GEN[B] union (IN[B] - KILL[B])
                  (let ((new-out
                         (funcall 'neovm--test-set-union-str
                                  (gethash label gen-map)
                                  (funcall 'neovm--test-set-diff-str
                                           new-in (gethash label kill-map)))))
                    (unless (funcall 'neovm--test-set-equal-str
                                     new-out (gethash label out-map))
                      (setq changed t))
                    (puthash label new-out out-map))))))
          ;; Collect
          (let ((result nil))
            (dolist (blk blocks)
              (let ((label (car blk)))
                (setq result (cons (list label
                                         :in (gethash label in-map)
                                         :out (gethash label out-map))
                                   result))))
            (list :iterations iterations
                  :results (nreverse result)))))))

  (unwind-protect
      ;; Test program:
      ;; B1: a = ...; b = ...; t1 = a+b   -> B2
      ;; B2: c = a+b; d = ...; t2 = c+d   -> B3, B4
      ;; B3: a = ...                       -> B5
      ;; B4: (nothing)                     -> B5
      ;; B5: e = a+b
      ;; Expression (+ a b) is killed in B3 (redefines a), still available via B4 path
      (let ((all-exprs '((+ a b) (+ c d))))
        (funcall 'neovm--test-available-expressions
                 '((B1 ((def a) (def b) (expr t1 (+ a b))) (B2))
                   (B2 ((expr c (+ a b)) (def d) (expr t2 (+ c d))) (B3 B4))
                   (B3 ((def a)) (B5))
                   (B4 () (B5))
                   (B5 ((expr e (+ a b))) ()))
                 all-exprs))
    (fmakunbound 'neovm--test-available-expressions)
    (fmakunbound 'neovm--test-avail-gen-kill)
    (fmakunbound 'neovm--test-set-intersect)
    (fmakunbound 'neovm--test-set-union-str)
    (fmakunbound 'neovm--test-set-diff-str)
    (fmakunbound 'neovm--test-set-equal-str)))"#;
    let expect = expect_test::expect![[
        r#""OK (:iterations 2 :results ((B1 :in nil :out nil) (B2 :in nil :out ((+ a b))) (B3 :in ((+ a b)) :out nil) (B4 :in ((+ a b)) :out ((+ a b))) (B5 :in nil :out ((+ a b)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Use-def chains construction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dataflow_use_def_chains() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // For each use of a variable, find all definitions that could reach it.
    // Uses reaching definitions to build the chains.
    let form = r#"(progn
  (fset 'neovm--test-ud-set-union
    (lambda (a b)
      (let ((result (copy-sequence a)))
        (dolist (x b)
          (unless (member x result)
            (setq result (cons x result))))
        (sort result (lambda (x y) (string< (format "%s" x) (format "%s" y)))))))

  (fset 'neovm--test-ud-set-diff
    (lambda (a b)
      (let ((result nil))
        (dolist (x a)
          (unless (member x b)
            (setq result (cons x result))))
        result)))

  (fset 'neovm--test-ud-set-equal
    (lambda (a b)
      (and (= (length a) (length b))
           (null (funcall 'neovm--test-ud-set-diff a b)))))

  ;; Reaching definitions (simplified, returns per-block IN sets)
  (fset 'neovm--test-ud-reaching
    (lambda (blocks)
      (let ((all-defs nil)
            (gen-map (make-hash-table :test 'eq))
            (kill-map (make-hash-table :test 'eq))
            (in-map (make-hash-table :test 'eq))
            (out-map (make-hash-table :test 'eq))
            (pred-map (make-hash-table :test 'eq)))
        ;; Collect all defs
        (dolist (blk blocks)
          (dolist (instr (cadr blk))
            (when (eq (car instr) 'def)
              (setq all-defs (cons (list (car blk) (cadr instr)) all-defs)))))
        ;; Pred map
        (dolist (blk blocks)
          (puthash (car blk) nil pred-map))
        (dolist (blk blocks)
          (dolist (s (nth 2 blk))
            (puthash s (cons (car blk) (gethash s pred-map)) pred-map)))
        ;; Gen/Kill
        (dolist (blk blocks)
          (let ((gen nil) (kill nil) (label (car blk)))
            (dolist (instr (cadr blk))
              (when (eq (car instr) 'def)
                (let ((var (cadr instr)))
                  ;; Kill: other defs of same var
                  (dolist (d all-defs)
                    (when (and (eq (cadr d) var) (not (eq (car d) label)))
                      (unless (member d kill) (setq kill (cons d kill)))))
                  ;; Gen: this def
                  (setq gen (cons (list label var)
                                  (let ((f nil))
                                    (dolist (g gen)
                                      (unless (eq (cadr g) var)
                                        (setq f (cons g f))))
                                    (nreverse f)))))))
            (puthash label gen gen-map)
            (puthash label kill kill-map)
            (puthash label nil in-map)
            (puthash label nil out-map)))
        ;; Fixed-point
        (let ((changed t))
          (while changed
            (setq changed nil)
            (dolist (blk blocks)
              (let ((label (car blk))
                    (new-in nil))
                (dolist (p (gethash label pred-map))
                  (setq new-in (funcall 'neovm--test-ud-set-union
                                        new-in (gethash p out-map))))
                (puthash label new-in in-map)
                (let ((new-out (funcall 'neovm--test-ud-set-union
                                        (gethash label gen-map)
                                        (funcall 'neovm--test-ud-set-diff
                                                 new-in (gethash label kill-map)))))
                  (unless (funcall 'neovm--test-ud-set-equal
                                   new-out (gethash label out-map))
                    (setq changed t))
                  (puthash label new-out out-map))))))
        in-map)))

  ;; Build use-def chains from reaching definitions
  (fset 'neovm--test-use-def-chains
    (lambda (blocks)
      (let ((in-map (funcall 'neovm--test-ud-reaching blocks))
            (chains nil))
        ;; For each block, for each use instruction, find reaching defs
        (dolist (blk blocks)
          (let ((label (car blk))
                (reaching-in (gethash (car blk) in-map))
                (local-defs nil))
            ;; Walk instructions: track local defs that shadow reaching defs
            (dolist (instr (cadr blk))
              (cond
               ((eq (car instr) 'use)
                (let ((var (cadr instr))
                      (defs nil))
                  ;; Check if locally defined first
                  (if (assq var local-defs)
                      (setq defs (list (list label var)))
                    ;; Use reaching definitions at block entry
                    (dolist (d reaching-in)
                      (when (eq (cadr d) var)
                        (setq defs (cons d defs)))))
                  (setq chains (cons (list label var
                                           (sort defs (lambda (a b)
                                                        (string< (format "%s" a)
                                                                 (format "%s" b)))))
                                     chains))))
               ((eq (car instr) 'def)
                (let ((var (cadr instr)))
                  (if (assq var local-defs)
                      (setcdr (assq var local-defs) label)
                    (setq local-defs (cons (cons var label) local-defs)))))))))
        (nreverse chains))))

  (unwind-protect
      ;; Test program:
      ;; B1: x = 1; y = 2       -> B2, B3
      ;; B2: x = 3              -> B4
      ;; B3: (nothing)          -> B4
      ;; B4: use x; use y
      ;; At B4: x could be from B1 (via B3) or B2. y from B1.
      (funcall 'neovm--test-use-def-chains
               '((B1 ((def x) (def y)) (B2 B3))
                 (B2 ((def x))         (B4))
                 (B3 ()                (B4))
                 (B4 ((use x) (use y)) ())))
    (fmakunbound 'neovm--test-use-def-chains)
    (fmakunbound 'neovm--test-ud-reaching)
    (fmakunbound 'neovm--test-ud-set-union)
    (fmakunbound 'neovm--test-ud-set-diff)
    (fmakunbound 'neovm--test-ud-set-equal)))"#;
    let expect = expect_test::expect![[r#""OK ((B4 x ((B1 x) (B2 x))) (B4 y ((B1 y))))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Loop-carried reaching definitions (with back edge)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dataflow_loop_reaching_defs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A program with a loop to test that fixed-point handles back edges.
    // B1: i = 0; sum = 0       -> B2
    // B2: use i; use sum       -> B3, B4  (loop test)
    // B3: sum = sum + i; i = i + 1  -> B2  (back edge)
    // B4: use sum              (exit)
    let form = r#"(progn
  (fset 'neovm--test-lrd-union
    (lambda (a b)
      (let ((result (copy-sequence a)))
        (dolist (x b)
          (unless (member x result)
            (setq result (cons x result))))
        (sort result (lambda (x y) (string< (format "%s" x) (format "%s" y)))))))

  (fset 'neovm--test-lrd-diff
    (lambda (a b)
      (let ((result nil))
        (dolist (x a)
          (unless (member x b)
            (setq result (cons x result))))
        result)))

  (fset 'neovm--test-lrd-equal
    (lambda (a b)
      (and (= (length a) (length b))
           (null (funcall 'neovm--test-lrd-diff a b)))))

  (fset 'neovm--test-loop-reaching
    (lambda (blocks)
      (let ((all-defs nil))
        (dolist (blk blocks)
          (dolist (instr (cadr blk))
            (when (eq (car instr) 'def)
              (setq all-defs (cons (list (car blk) (cadr instr)) all-defs)))))
        (let ((gen-map (make-hash-table :test 'eq))
              (kill-map (make-hash-table :test 'eq))
              (in-map (make-hash-table :test 'eq))
              (out-map (make-hash-table :test 'eq))
              (pred-map (make-hash-table :test 'eq)))
          (dolist (blk blocks)
            (puthash (car blk) nil pred-map))
          (dolist (blk blocks)
            (dolist (s (nth 2 blk))
              (puthash s (cons (car blk) (gethash s pred-map)) pred-map)))
          (dolist (blk blocks)
            (let ((gen nil) (kill nil) (label (car blk)))
              (dolist (instr (cadr blk))
                (when (eq (car instr) 'def)
                  (let ((var (cadr instr)))
                    (dolist (d all-defs)
                      (when (and (eq (cadr d) var) (not (eq (car d) label)))
                        (unless (member d kill) (setq kill (cons d kill)))))
                    (setq gen (cons (list label var)
                                    (let ((f nil))
                                      (dolist (g gen)
                                        (unless (eq (cadr g) var)
                                          (setq f (cons g f))))
                                      (nreverse f)))))))
              (puthash label gen gen-map)
              (puthash label kill kill-map)
              (puthash label nil in-map)
              (puthash label nil out-map)))
          (let ((changed t) (iterations 0))
            (while changed
              (setq changed nil)
              (setq iterations (1+ iterations))
              (dolist (blk blocks)
                (let ((label (car blk)) (new-in nil))
                  (dolist (p (gethash label pred-map))
                    (setq new-in (funcall 'neovm--test-lrd-union
                                          new-in (gethash p out-map))))
                  (puthash label new-in in-map)
                  (let ((new-out (funcall 'neovm--test-lrd-union
                                          (gethash label gen-map)
                                          (funcall 'neovm--test-lrd-diff
                                                   new-in (gethash label kill-map)))))
                    (unless (funcall 'neovm--test-lrd-equal
                                     new-out (gethash label out-map))
                      (setq changed t))
                    (puthash label new-out out-map)))))
            (let ((result nil))
              (dolist (blk blocks)
                (let ((label (car blk)))
                  (setq result (cons (list label
                                           :in (gethash label in-map)
                                           :out (gethash label out-map))
                                     result))))
              (list :iterations iterations
                    :results (nreverse result))))))))

  (unwind-protect
      (funcall 'neovm--test-loop-reaching
               '((B1 ((def i) (def sum))          (B2))
                 (B2 ((use i) (use sum))           (B3 B4))
                 (B3 ((def sum) (def i))           (B2))
                 (B4 ((use sum))                   ())))
    (fmakunbound 'neovm--test-loop-reaching)
    (fmakunbound 'neovm--test-lrd-union)
    (fmakunbound 'neovm--test-lrd-diff)
    (fmakunbound 'neovm--test-lrd-equal)))"#;
    let expect = expect_test::expect![[
        r#""OK (:iterations 3 :results ((B1 :in nil :out ((B1 i) (B1 sum))) (B2 :in ((B1 i) (B1 sum) (B3 i) (B3 sum)) :out ((B1 i) (B1 sum) (B3 i) (B3 sum))) (B3 :in ((B1 i) (B1 sum) (B3 i) (B3 sum)) :out ((B3 i) (B3 sum))) (B4 :in ((B1 i) (B1 sum) (B3 i) (B3 sum)) :out ((B1 i) (B1 sum) (B3 i) (B3 sum)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dominators computation (forward, intersection-based)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dataflow_dominators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // DOM[entry] = {entry}
    // DOM[B] = {B} union (intersection of DOM[pred] for all predecessors)
    let form = r#"(progn
  (fset 'neovm--test-dom-intersect
    (lambda (a b)
      (let ((result nil))
        (dolist (x a)
          (when (memq x b)
            (setq result (cons x result))))
        (sort result (lambda (x y) (string< (symbol-name x) (symbol-name y)))))))

  (fset 'neovm--test-dom-equal
    (lambda (a b)
      (and (= (length a) (length b))
           (let ((ok t))
             (dolist (x a)
               (unless (memq x b) (setq ok nil)))
             ok))))

  (fset 'neovm--test-compute-dominators
    (lambda (blocks)
      (let ((entry (caar blocks))
            (all-labels (mapcar #'car blocks))
            (dom-map (make-hash-table :test 'eq))
            (pred-map (make-hash-table :test 'eq)))
        ;; Build predecessor map
        (dolist (blk blocks)
          (puthash (car blk) nil pred-map))
        (dolist (blk blocks)
          (dolist (s (nth 2 blk))
            (puthash s (cons (car blk) (gethash s pred-map)) pred-map)))
        ;; Initialize: DOM[entry] = {entry}, DOM[other] = all labels
        (dolist (label all-labels)
          (if (eq label entry)
              (puthash label (list entry) dom-map)
            (puthash label (copy-sequence all-labels) dom-map)))
        ;; Fixed-point
        (let ((changed t) (iterations 0))
          (while changed
            (setq changed nil)
            (setq iterations (1+ iterations))
            (dolist (blk blocks)
              (let ((label (car blk)))
                (unless (eq label entry)
                  (let ((preds (gethash label pred-map))
                        (new-dom nil))
                    (when preds
                      (setq new-dom (copy-sequence (gethash (car preds) dom-map)))
                      (dolist (p (cdr preds))
                        (setq new-dom (funcall 'neovm--test-dom-intersect
                                               new-dom (gethash p dom-map)))))
                    ;; Add self
                    (unless (memq label new-dom)
                      (setq new-dom (cons label new-dom)))
                    (setq new-dom (sort new-dom
                                        (lambda (x y) (string< (symbol-name x)
                                                                (symbol-name y)))))
                    (unless (funcall 'neovm--test-dom-equal
                                     new-dom (gethash label dom-map))
                      (setq changed t))
                    (puthash label new-dom dom-map)))))))
          ;; Collect + compute immediate dominators
          (let ((result nil))
            (dolist (label all-labels)
              (let ((doms (gethash label dom-map))
                    (idom nil))
                ;; Immediate dominator: the dominator closest to label
                ;; (dominator with largest dom set that isn't label itself)
                (unless (eq label entry)
                  (let ((best nil) (best-size 0))
                    (dolist (d doms)
                      (unless (eq d label)
                        (let ((d-dom-size (length (gethash d dom-map))))
                          (when (> d-dom-size best-size)
                            (setq best d best-size d-dom-size)))))
                    (setq idom best)))
                (setq result (cons (list label :dom doms :idom idom) result))))
            (list :iterations iterations
                  :results (nreverse result)))))))

  (unwind-protect
      ;; Diamond CFG:
      ;; B1 -> B2, B3
      ;; B2 -> B4
      ;; B3 -> B4
      ;; B4 -> B5
      ;; B5 (exit)
      ;; Expected: B1 dominates everything, B4's idom is B1 (both paths merge)
      (funcall 'neovm--test-compute-dominators
               '((B1 () (B2 B3))
                 (B2 () (B4))
                 (B3 () (B4))
                 (B4 () (B5))
                 (B5 () ())))
    (fmakunbound 'neovm--test-compute-dominators)
    (fmakunbound 'neovm--test-dom-intersect)
    (fmakunbound 'neovm--test-dom-equal)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable iterations)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
