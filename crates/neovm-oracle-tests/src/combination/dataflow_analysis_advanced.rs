//! Oracle parity tests for advanced dataflow analysis in Elisp:
//! lattice operations (join, meet, bottom, top), transfer functions as alists,
//! worklist algorithm for fixpoint computation, constant propagation,
//! interval analysis, forward and backward analysis directions,
//! chaotic iteration, and very busy expressions analysis.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Lattice operations: join, meet, bottom, top for a flat integer lattice
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dataflow_flat_lattice_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Flat lattice for constants: bottom < any-constant < top
    // join(bottom, x) = x, join(x, bottom) = x, join(x, x) = x,
    // join(x, y) = top when x != y
    // meet(top, x) = x, meet(x, top) = x, meet(x, x) = x,
    // meet(x, y) = bottom when x != y
    let form = r#"(progn
  (fset 'neovm--fl-join
    (lambda (a b)
      (cond
       ((eq a 'bottom) b)
       ((eq b 'bottom) a)
       ((equal a b) a)
       (t 'top))))

  (fset 'neovm--fl-meet
    (lambda (a b)
      (cond
       ((eq a 'top) b)
       ((eq b 'top) a)
       ((equal a b) a)
       (t 'bottom))))

  ;; Join/meet over environment (alist of var -> lattice-value)
  (fset 'neovm--fl-env-join
    (lambda (env1 env2)
      "Pointwise join of two environments."
      (let ((result nil)
            (all-vars nil))
        ;; Collect all variables
        (dolist (pair env1)
          (unless (assq (car pair) all-vars)
            (setq all-vars (cons (cons (car pair) t) all-vars))))
        (dolist (pair env2)
          (unless (assq (car pair) all-vars)
            (setq all-vars (cons (cons (car pair) t) all-vars))))
        ;; Compute join for each variable
        (dolist (v all-vars)
          (let ((val1 (or (cdr (assq (car v) env1)) 'bottom))
                (val2 (or (cdr (assq (car v) env2)) 'bottom)))
            (setq result (cons (cons (car v) (funcall 'neovm--fl-join val1 val2))
                               result))))
        (sort result (lambda (a b) (string< (symbol-name (car a))
                                             (symbol-name (car b))))))))

  (fset 'neovm--fl-env-meet
    (lambda (env1 env2)
      "Pointwise meet of two environments."
      (let ((result nil)
            (all-vars nil))
        (dolist (pair env1)
          (unless (assq (car pair) all-vars)
            (setq all-vars (cons (cons (car pair) t) all-vars))))
        (dolist (pair env2)
          (unless (assq (car pair) all-vars)
            (setq all-vars (cons (cons (car pair) t) all-vars))))
        (dolist (v all-vars)
          (let ((val1 (or (cdr (assq (car v) env1)) 'top))
                (val2 (or (cdr (assq (car v) env2)) 'top)))
            (setq result (cons (cons (car v) (funcall 'neovm--fl-meet val1 val2))
                               result))))
        (sort result (lambda (a b) (string< (symbol-name (car a))
                                             (symbol-name (car b))))))))

  (unwind-protect
      (list
       ;; Scalar lattice operations
       (funcall 'neovm--fl-join 'bottom 5)
       (funcall 'neovm--fl-join 5 'bottom)
       (funcall 'neovm--fl-join 5 5)
       (funcall 'neovm--fl-join 5 7)
       (funcall 'neovm--fl-join 'top 5)
       (funcall 'neovm--fl-meet 'top 5)
       (funcall 'neovm--fl-meet 5 'top)
       (funcall 'neovm--fl-meet 5 5)
       (funcall 'neovm--fl-meet 5 7)
       (funcall 'neovm--fl-meet 'bottom 5)
       ;; Environment operations
       (funcall 'neovm--fl-env-join
                '((x . 5) (y . 3))
                '((x . 5) (y . 7)))
       (funcall 'neovm--fl-env-join
                '((x . 5) (y . bottom))
                '((x . bottom) (y . 3)))
       (funcall 'neovm--fl-env-meet
                '((x . 5) (y . top))
                '((x . 5) (y . 3))))
    (fmakunbound 'neovm--fl-join)
    (fmakunbound 'neovm--fl-meet)
    (fmakunbound 'neovm--fl-env-join)
    (fmakunbound 'neovm--fl-env-meet)))"#;
    let expect = expect_test::expect![[
        r#""OK (5 5 5 top top 5 5 5 bottom bottom ((x . 5) (y . top)) ((x . 5) (y . 3)) ((x . 5) (y . 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Transfer functions as alists for constant propagation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dataflow_transfer_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A transfer function maps an input environment to an output environment.
    // For constant propagation:
    // - (assign x c) sets x to constant c
    // - (assign x (+ y z)) evaluates if y,z are constants
    // - (assign x ?) sets x to top (unknown)
    let form = r#"(progn
  (fset 'neovm--tf-join
    (lambda (a b)
      (cond ((eq a 'bottom) b) ((eq b 'bottom) a)
            ((equal a b) a) (t 'top))))

  (fset 'neovm--tf-eval-expr
    (lambda (expr env)
      "Evaluate an expression in a constant environment."
      (cond
       ((numberp expr) expr)
       ((symbolp expr)
        (or (cdr (assq expr env)) 'top))
       ((and (consp expr) (= (length expr) 3))
        (let ((op (car expr))
              (v1 (funcall 'neovm--tf-eval-expr (cadr expr) env))
              (v2 (funcall 'neovm--tf-eval-expr (nth 2 expr) env)))
          (cond
           ((or (eq v1 'top) (eq v2 'top) (eq v1 'bottom) (eq v2 'bottom)) 'top)
           ((and (numberp v1) (numberp v2))
            (cond
             ((eq op '+) (+ v1 v2))
             ((eq op '-) (- v1 v2))
             ((eq op '*) (* v1 v2))
             (t 'top)))
           (t 'top))))
       (t 'top))))

  ;; Apply a list of instructions as a transfer function
  (fset 'neovm--tf-apply
    (lambda (instrs env)
      "Apply instructions to environment, returning new environment."
      (let ((cur-env (copy-sequence env)))
        (dolist (instr instrs)
          (when (eq (car instr) 'assign)
            (let* ((var (cadr instr))
                   (expr (nth 2 instr))
                   (val (funcall 'neovm--tf-eval-expr expr cur-env)))
              ;; Update environment
              (if (assq var cur-env)
                  (setcdr (assq var cur-env) val)
                (setq cur-env (cons (cons var val) cur-env))))))
        (sort cur-env (lambda (a b) (string< (symbol-name (car a))
                                              (symbol-name (car b))))))))

  (unwind-protect
      (list
       ;; Simple constant assignment
       (funcall 'neovm--tf-apply
                '((assign x 5) (assign y 10))
                nil)
       ;; Constant expression
       (funcall 'neovm--tf-apply
                '((assign x 5) (assign y 10) (assign z (+ x y)))
                nil)
       ;; Overwrite: x=5, then x=x+1
       (funcall 'neovm--tf-apply
                '((assign x 5) (assign x (+ x 1)))
                nil)
       ;; With incoming environment
       (funcall 'neovm--tf-apply
                '((assign z (+ x y)))
                '((x . 3) (y . 7)))
       ;; Top propagation: unknown variable makes result top
       (funcall 'neovm--tf-apply
                '((assign z (+ x y)))
                '((x . 3) (y . top)))
       ;; Chain of computations
       (funcall 'neovm--tf-apply
                '((assign a 2) (assign b 3) (assign c (* a b))
                  (assign d (+ c 1)) (assign e (- d a)))
                nil))
    (fmakunbound 'neovm--tf-join)
    (fmakunbound 'neovm--tf-eval-expr)
    (fmakunbound 'neovm--tf-apply)))"#;
    let expect = expect_test::expect![[
        r#""OK (((x . 5) (y . 10)) ((x . 5) (y . 10) (z . 15)) ((x . 6)) ((x . 3) (y . 7) (z . 10)) ((x . 3) (y . top) (z . top)) ((a . 2) (b . 3) (c . 6) (d . 7) (e . 5)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Worklist algorithm for fixpoint computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dataflow_worklist_algorithm() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Worklist-based fixpoint: instead of iterating over all blocks,
    // maintain a worklist of blocks whose inputs have changed.
    // More efficient than chaotic iteration for sparse CFGs.
    let form = r#"(progn
  (fset 'neovm--wl-set-union
    (lambda (a b)
      (let ((result (copy-sequence a)))
        (dolist (x b)
          (unless (member x result)
            (setq result (cons x result))))
        (sort result (lambda (x y) (string< (format "%s" x) (format "%s" y)))))))

  (fset 'neovm--wl-set-diff
    (lambda (a b)
      (let ((result nil))
        (dolist (x a)
          (unless (member x b)
            (setq result (cons x result))))
        result)))

  (fset 'neovm--wl-set-equal
    (lambda (a b)
      (and (= (length a) (length b))
           (null (funcall 'neovm--wl-set-diff a b)))))

  ;; Worklist-based reaching definitions
  (fset 'neovm--wl-reaching-defs
    (lambda (blocks)
      (let ((all-defs nil))
        (dolist (blk blocks)
          (dolist (instr (cadr blk))
            (when (eq (car instr) 'def)
              (setq all-defs (cons (cons (car blk) (cadr instr)) all-defs)))))
        (let ((gen-map (make-hash-table :test 'eq))
              (kill-map (make-hash-table :test 'eq))
              (in-map (make-hash-table :test 'eq))
              (out-map (make-hash-table :test 'eq))
              (succ-map (make-hash-table :test 'eq))
              (pred-map (make-hash-table :test 'eq)))
          ;; Build succ/pred maps
          (dolist (blk blocks)
            (puthash (car blk) (nth 2 blk) succ-map)
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
                    (dolist (d all-defs)
                      (when (and (eq (cdr d) var) (not (eq (car d) label)))
                        (unless (member d kill) (setq kill (cons d kill)))))
                    (setq gen (cons (cons label var)
                                    (let ((f nil))
                                      (dolist (g gen)
                                        (unless (eq (cdr g) var)
                                          (setq f (cons g f))))
                                      (nreverse f)))))))
              (puthash label gen gen-map)
              (puthash label kill kill-map)
              (puthash label nil in-map)
              (puthash label nil out-map)))
          ;; Worklist: start with all blocks
          (let ((worklist (mapcar #'car blocks))
                (processed 0))
            (while worklist
              (let ((label (car worklist)))
                (setq worklist (cdr worklist))
                (setq processed (1+ processed))
                ;; IN = union of OUT[pred]
                (let ((new-in nil))
                  (dolist (p (gethash label pred-map))
                    (setq new-in (funcall 'neovm--wl-set-union
                                          new-in (gethash p out-map))))
                  (puthash label new-in in-map)
                  ;; OUT = GEN union (IN - KILL)
                  (let ((new-out (funcall 'neovm--wl-set-union
                                          (gethash label gen-map)
                                          (funcall 'neovm--wl-set-diff
                                                   new-in (gethash label kill-map)))))
                    (unless (funcall 'neovm--wl-set-equal
                                     new-out (gethash label out-map))
                      (puthash label new-out out-map)
                      ;; Add successors to worklist
                      (dolist (s (gethash label succ-map))
                        (unless (memq s worklist)
                          (setq worklist (append worklist (list s))))))))))
            ;; Collect results
            (let ((result nil))
              (dolist (blk blocks)
                (let ((label (car blk)))
                  (setq result (cons (list label
                                           :in (gethash label in-map)
                                           :out (gethash label out-map))
                                     result))))
              (list :processed processed
                    :results (nreverse result))))))))

  (unwind-protect
      ;; Test with diamond CFG + back edge
      (funcall 'neovm--wl-reaching-defs
               '((B1 ((def x) (def y))  (B2 B3))
                 (B2 ((def z))          (B4))
                 (B3 ((def x))          (B4))
                 (B4 ((def w))          (B5 B2))
                 (B5 ((use x) (use w))  ())))
    (fmakunbound 'neovm--wl-reaching-defs)
    (fmakunbound 'neovm--wl-set-union)
    (fmakunbound 'neovm--wl-set-diff)
    (fmakunbound 'neovm--wl-set-equal)))"#;
    let expect = expect_test::expect![[
        r#""OK (:processed 7 :results ((B1 :in nil :out ((B1 . x) (B1 . y))) (B2 :in ((B1 . x) (B1 . y) (B2 . z) (B3 . x) (B4 . w)) :out ((B1 . x) (B1 . y) (B2 . z) (B3 . x) (B4 . w))) (B3 :in ((B1 . x) (B1 . y)) :out ((B1 . y) (B3 . x))) (B4 :in ((B1 . x) (B1 . y) (B2 . z) (B3 . x) (B4 . w)) :out ((B1 . x) (B1 . y) (B2 . z) (B3 . x) (B4 . w))) (B5 :in ((B1 . x) (B1 . y) (B2 . z) (B3 . x) (B4 . w)) :out ((B1 . x) (B1 . y) (B2 . z) (B3 . x) (B4 . w)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Constant propagation analysis (full)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dataflow_constant_propagation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--cp-join
    (lambda (a b)
      (cond ((eq a 'bottom) b) ((eq b 'bottom) a)
            ((equal a b) a) (t 'top))))

  (fset 'neovm--cp-env-join
    (lambda (e1 e2 vars)
      "Pointwise join of two environments over given vars."
      (let ((result nil))
        (dolist (v vars)
          (let ((v1 (or (cdr (assq v e1)) 'bottom))
                (v2 (or (cdr (assq v e2)) 'bottom)))
            (setq result (cons (cons v (funcall 'neovm--cp-join v1 v2)) result))))
        (sort result (lambda (a b) (string< (symbol-name (car a))
                                             (symbol-name (car b))))))))

  (fset 'neovm--cp-env-equal
    (lambda (e1 e2)
      (equal e1 e2)))

  (fset 'neovm--cp-eval
    (lambda (expr env)
      (cond
       ((numberp expr) expr)
       ((symbolp expr) (or (cdr (assq expr env)) 'top))
       ((and (consp expr) (= (length expr) 3))
        (let ((v1 (funcall 'neovm--cp-eval (cadr expr) env))
              (v2 (funcall 'neovm--cp-eval (nth 2 expr) env)))
          (if (and (numberp v1) (numberp v2))
              (let ((op (car expr)))
                (cond ((eq op '+) (+ v1 v2))
                      ((eq op '-) (- v1 v2))
                      ((eq op '*) (* v1 v2))
                      (t 'top)))
            'top)))
       (t 'top))))

  ;; Transfer function for a block
  (fset 'neovm--cp-transfer
    (lambda (instrs env)
      (let ((cur (copy-sequence env)))
        (dolist (instr instrs)
          (when (eq (car instr) 'assign)
            (let ((var (cadr instr))
                  (val (funcall 'neovm--cp-eval (nth 2 instr) cur)))
              (if (assq var cur)
                  (setcdr (assq var cur) val)
                (setq cur (cons (cons var val) cur))))))
        (sort cur (lambda (a b) (string< (symbol-name (car a))
                                          (symbol-name (car b))))))))

  (fset 'neovm--cp-analyze
    (lambda (blocks all-vars)
      (let ((in-map (make-hash-table :test 'eq))
            (out-map (make-hash-table :test 'eq))
            (pred-map (make-hash-table :test 'eq))
            (entry (caar blocks))
            (bottom-env (let ((e nil))
                          (dolist (v all-vars) (setq e (cons (cons v 'bottom) e)))
                          (sort e (lambda (a b) (string< (symbol-name (car a))
                                                          (symbol-name (car b))))))))
        ;; Build pred map
        (dolist (blk blocks) (puthash (car blk) nil pred-map))
        (dolist (blk blocks)
          (dolist (s (nth 2 blk))
            (puthash s (cons (car blk) (gethash s pred-map)) pred-map)))
        ;; Initialize
        (dolist (blk blocks)
          (puthash (car blk) (copy-sequence bottom-env) in-map)
          (puthash (car blk) (copy-sequence bottom-env) out-map))
        ;; Fixed-point
        (let ((changed t) (iterations 0))
          (while (and changed (< iterations 100))
            (setq changed nil)
            (setq iterations (1+ iterations))
            (dolist (blk blocks)
              (let ((label (car blk)))
                ;; IN = join of OUT[pred]
                (let ((new-in (if (eq label entry)
                                  (copy-sequence bottom-env)
                                (let ((preds (gethash label pred-map))
                                      (acc (copy-sequence bottom-env)))
                                  (dolist (p preds)
                                    (setq acc (funcall 'neovm--cp-env-join
                                                       acc (gethash p out-map) all-vars)))
                                  acc))))
                  (puthash label new-in in-map)
                  ;; OUT = transfer(instrs, IN)
                  (let ((new-out (funcall 'neovm--cp-transfer (cadr blk) new-in)))
                    (unless (funcall 'neovm--cp-env-equal
                                     new-out (gethash label out-map))
                      (setq changed t))
                    (puthash label new-out out-map))))))
          ;; Collect
          (let ((result nil))
            (dolist (blk blocks)
              (setq result (cons (list (car blk)
                                       :in (gethash (car blk) in-map)
                                       :out (gethash (car blk) out-map))
                                 result)))
            (list :iterations iterations
                  :results (nreverse result)))))))

  (unwind-protect
      ;; Test: x=5, y=3, z=x+y (should propagate to z=8)
      ;; B1: x=5, y=3 -> B2, B3
      ;; B2: z=x+y -> B4
      ;; B3: z=10 -> B4
      ;; B4: w=z+1
      (funcall 'neovm--cp-analyze
               '((B1 ((assign x 5) (assign y 3))  (B2 B3))
                 (B2 ((assign z (+ x y)))          (B4))
                 (B3 ((assign z 10))               (B4))
                 (B4 ((assign w (+ z 1)))          ()))
               '(w x y z))
    (fmakunbound 'neovm--cp-join)
    (fmakunbound 'neovm--cp-env-join)
    (fmakunbound 'neovm--cp-env-equal)
    (fmakunbound 'neovm--cp-eval)
    (fmakunbound 'neovm--cp-transfer)
    (fmakunbound 'neovm--cp-analyze)))"#;
    let expect = expect_test::expect![[
        r#""OK (:iterations 2 :results ((B1 :in ((w . bottom) (x . 5) (y . 3) (z . bottom)) :out ((w . bottom) (x . 5) (y . 3) (z . bottom))) (B2 :in ((w . bottom) (x . 5) (y . 3) (z . 8)) :out ((w . bottom) (x . 5) (y . 3) (z . 8))) (B3 :in ((w . bottom) (x . 5) (y . 3) (z . 10)) :out ((w . bottom) (x . 5) (y . 3) (z . 10))) (B4 :in ((w . top) (x . 5) (y . 3) (z . top)) :out ((w . top) (x . 5) (y . 3) (z . top)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interval analysis (abstract domain: [lo, hi])
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dataflow_interval_analysis() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Abstract domain: intervals [lo, hi], bottom = empty, top = [-inf, inf]
    let form = r#"(progn
  (fset 'neovm--ia-join
    (lambda (a b)
      "Join two intervals."
      (cond
       ((eq a 'bottom) b)
       ((eq b 'bottom) a)
       (t (list (min (car a) (car b))
                (max (cadr a) (cadr b)))))))

  (fset 'neovm--ia-add
    (lambda (a b)
      "Add two intervals."
      (cond
       ((or (eq a 'bottom) (eq b 'bottom)) 'bottom)
       (t (list (+ (car a) (car b))
                (+ (cadr a) (cadr b)))))))

  (fset 'neovm--ia-sub
    (lambda (a b)
      "Subtract two intervals."
      (cond
       ((or (eq a 'bottom) (eq b 'bottom)) 'bottom)
       (t (list (- (car a) (cadr b))
                (- (cadr a) (car b)))))))

  (fset 'neovm--ia-mul
    (lambda (a b)
      "Multiply two intervals."
      (cond
       ((or (eq a 'bottom) (eq b 'bottom)) 'bottom)
       (t (let ((products (list (* (car a) (car b))
                                (* (car a) (cadr b))
                                (* (cadr a) (car b))
                                (* (cadr a) (cadr b)))))
            (list (apply #'min products) (apply #'max products)))))))

  ;; Evaluate expression in interval environment
  (fset 'neovm--ia-eval
    (lambda (expr env)
      (cond
       ((numberp expr) (list expr expr))
       ((symbolp expr) (or (cdr (assq expr env)) 'bottom))
       ((and (consp expr) (= (length expr) 3))
        (let ((v1 (funcall 'neovm--ia-eval (cadr expr) env))
              (v2 (funcall 'neovm--ia-eval (nth 2 expr) env)))
          (cond
           ((eq (car expr) '+) (funcall 'neovm--ia-add v1 v2))
           ((eq (car expr) '-) (funcall 'neovm--ia-sub v1 v2))
           ((eq (car expr) '*) (funcall 'neovm--ia-mul v1 v2))
           (t 'bottom))))
       (t 'bottom))))

  (unwind-protect
      (list
       ;; Join intervals
       (funcall 'neovm--ia-join '(1 5) '(3 8))
       (funcall 'neovm--ia-join 'bottom '(2 4))
       (funcall 'neovm--ia-join '(2 4) 'bottom)
       ;; Add intervals: [1,3] + [2,5] = [3,8]
       (funcall 'neovm--ia-add '(1 3) '(2 5))
       ;; Sub intervals: [5,10] - [1,3] = [2,9]
       (funcall 'neovm--ia-sub '(5 10) '(1 3))
       ;; Mul intervals: [-2,3] * [1,4] = [-8,12]
       (funcall 'neovm--ia-mul '(-2 3) '(1 4))
       ;; Eval in environment
       (funcall 'neovm--ia-eval '(+ x y) '((x . (1 5)) (y . (2 3))))
       (funcall 'neovm--ia-eval '(* x y) '((x . (-1 2)) (y . (3 4))))
       ;; Chain: (+ (* x y) z) with x=[1,2], y=[3,4], z=[10,20]
       (funcall 'neovm--ia-eval '(+ (* x y) z)
                '((x . (1 2)) (y . (3 4)) (z . (10 20)))))
    (fmakunbound 'neovm--ia-join)
    (fmakunbound 'neovm--ia-add)
    (fmakunbound 'neovm--ia-sub)
    (fmakunbound 'neovm--ia-mul)
    (fmakunbound 'neovm--ia-eval)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 8) (2 4) (2 4) (3 8) (2 9) (-8 12) (3 8) (-4 8) (13 28))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Forward vs backward analysis on the same CFG
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dataflow_forward_vs_backward() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Run both reaching definitions (forward) and live variables (backward)
    // on the same CFG and compare the information obtained.
    let form = r#"(progn
  (fset 'neovm--fb-union
    (lambda (a b)
      (let ((result (copy-sequence a)))
        (dolist (x b)
          (unless (member x result) (setq result (cons x result))))
        (sort result (lambda (x y) (string< (format "%s" x) (format "%s" y)))))))

  (fset 'neovm--fb-diff
    (lambda (a b)
      (let ((r nil)) (dolist (x a) (unless (member x b) (setq r (cons x r)))) r)))

  (fset 'neovm--fb-equal
    (lambda (a b) (and (= (length a) (length b)) (null (funcall 'neovm--fb-diff a b)))))

  ;; Forward: reaching definitions
  (fset 'neovm--fb-forward
    (lambda (blocks)
      (let ((all-defs nil))
        (dolist (blk blocks)
          (dolist (instr (cadr blk))
            (when (eq (car instr) 'def)
              (setq all-defs (cons (cons (car blk) (cadr instr)) all-defs)))))
        (let ((gen-map (make-hash-table :test 'eq))
              (kill-map (make-hash-table :test 'eq))
              (out-map (make-hash-table :test 'eq))
              (pred-map (make-hash-table :test 'eq)))
          (dolist (blk blocks) (puthash (car blk) nil pred-map))
          (dolist (blk blocks)
            (dolist (s (nth 2 blk))
              (puthash s (cons (car blk) (gethash s pred-map)) pred-map)))
          (dolist (blk blocks)
            (let ((gen nil) (kill nil) (label (car blk)))
              (dolist (instr (cadr blk))
                (when (eq (car instr) 'def)
                  (let ((var (cadr instr)))
                    (dolist (d all-defs)
                      (when (and (eq (cdr d) var) (not (eq (car d) label)))
                        (unless (member d kill) (setq kill (cons d kill)))))
                    (setq gen (cons (cons label var)
                                    (let ((f nil))
                                      (dolist (g gen)
                                        (unless (eq (cdr g) var) (setq f (cons g f))))
                                      (nreverse f)))))))
              (puthash label gen gen-map)
              (puthash label kill kill-map)
              (puthash label nil out-map)))
          (let ((changed t) (iters 0))
            (while changed
              (setq changed nil) (setq iters (1+ iters))
              (dolist (blk blocks)
                (let ((label (car blk)) (new-in nil))
                  (dolist (p (gethash label pred-map))
                    (setq new-in (funcall 'neovm--fb-union new-in (gethash p out-map))))
                  (let ((new-out (funcall 'neovm--fb-union
                                          (gethash label gen-map)
                                          (funcall 'neovm--fb-diff
                                                   new-in (gethash label kill-map)))))
                    (unless (funcall 'neovm--fb-equal new-out (gethash label out-map))
                      (setq changed t))
                    (puthash label new-out out-map)))))
            (let ((r nil))
              (dolist (blk blocks)
                (setq r (cons (list (car blk) :reaching (gethash (car blk) out-map)) r)))
              (list :fwd-iters iters :forward (nreverse r))))))))

  ;; Backward: live variables
  (fset 'neovm--fb-backward
    (lambda (blocks)
      (let ((use-map (make-hash-table :test 'eq))
            (def-map (make-hash-table :test 'eq))
            (in-map (make-hash-table :test 'eq))
            (succ-map (make-hash-table :test 'eq)))
        (dolist (blk blocks)
          (let ((use-set nil) (def-set nil))
            (dolist (instr (cadr blk))
              (cond
               ((eq (car instr) 'use)
                (unless (memq (cadr instr) def-set)
                  (unless (memq (cadr instr) use-set)
                    (setq use-set (cons (cadr instr) use-set)))))
               ((eq (car instr) 'def)
                (unless (memq (cadr instr) def-set)
                  (setq def-set (cons (cadr instr) def-set))))))
            (puthash (car blk) use-set use-map)
            (puthash (car blk) def-set def-map)
            (puthash (car blk) nil in-map)
            (puthash (car blk) (nth 2 blk) succ-map)))
        (let ((changed t) (iters 0)
              (labels (nreverse (mapcar #'car blocks))))
          (while changed
            (setq changed nil) (setq iters (1+ iters))
            (dolist (label labels)
              (let ((out nil))
                (dolist (s (gethash label succ-map))
                  (setq out (funcall 'neovm--fb-union out (gethash s in-map))))
                (let ((new-in (funcall 'neovm--fb-union
                                       (gethash label use-map)
                                       (funcall 'neovm--fb-diff
                                                out (gethash label def-map)))))
                  (unless (funcall 'neovm--fb-equal new-in (gethash label in-map))
                    (setq changed t))
                  (puthash label new-in in-map)))))
          (let ((r nil))
            (dolist (blk blocks)
              (setq r (cons (list (car blk) :live (gethash (car blk) in-map)) r)))
            (list :bwd-iters iters :backward (nreverse r)))))))

  (unwind-protect
      (let ((cfg '((B1 ((def x) (def y))    (B2 B3))
                   (B2 ((use x) (def z))     (B4))
                   (B3 ((use y) (def z))     (B4))
                   (B4 ((use z) (use x))     ()))))
        (list
         (funcall 'neovm--fb-forward cfg)
         (funcall 'neovm--fb-backward cfg)))
    (fmakunbound 'neovm--fb-union)
    (fmakunbound 'neovm--fb-diff)
    (fmakunbound 'neovm--fb-equal)
    (fmakunbound 'neovm--fb-forward)
    (fmakunbound 'neovm--fb-backward)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:fwd-iters 2 :forward ((B1 :reaching ((B1 . x) (B1 . y))) (B2 :reaching ((B1 . x) (B1 . y) (B2 . z))) (B3 :reaching ((B1 . x) (B1 . y) (B3 . z))) (B4 :reaching ((B1 . x) (B1 . y) (B2 . z) (B3 . z))))) (:bwd-iters 2 :backward ((B1 :live nil) (B2 :live (x)) (B3 :live (x y)) (B4 :live (x z)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Chaotic iteration (random order processing)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dataflow_chaotic_iteration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Chaotic iteration: process blocks in a fixed but non-standard order
    // (reverse order) and verify that fixpoint is the same.
    let form = r#"(progn
  (fset 'neovm--ci-union
    (lambda (a b)
      (let ((result (copy-sequence a)))
        (dolist (x b) (unless (memq x result) (setq result (cons x result))))
        (sort result (lambda (x y) (string< (symbol-name x) (symbol-name y)))))))

  (fset 'neovm--ci-diff
    (lambda (a b)
      (let ((r nil)) (dolist (x a) (unless (memq x b) (setq r (cons x r)))) r)))

  (fset 'neovm--ci-equal
    (lambda (a b) (and (= (length a) (length b)) (null (funcall 'neovm--ci-diff a b)))))

  ;; Live variable analysis with configurable iteration order
  (fset 'neovm--ci-live-vars
    (lambda (blocks order)
      "Backward live variable analysis processing blocks in ORDER."
      (let ((use-map (make-hash-table :test 'eq))
            (def-map (make-hash-table :test 'eq))
            (in-map (make-hash-table :test 'eq))
            (succ-map (make-hash-table :test 'eq)))
        (dolist (blk blocks)
          (let ((use-s nil) (def-s nil))
            (dolist (instr (cadr blk))
              (cond
               ((eq (car instr) 'use)
                (unless (memq (cadr instr) def-s)
                  (unless (memq (cadr instr) use-s)
                    (setq use-s (cons (cadr instr) use-s)))))
               ((eq (car instr) 'def)
                (unless (memq (cadr instr) def-s)
                  (setq def-s (cons (cadr instr) def-s))))))
            (puthash (car blk) use-s use-map)
            (puthash (car blk) def-s def-map)
            (puthash (car blk) nil in-map)
            (puthash (car blk) (nth 2 blk) succ-map)))
        (let ((changed t) (iters 0))
          (while changed
            (setq changed nil) (setq iters (1+ iters))
            (dolist (label order)
              (let ((out nil))
                (dolist (s (gethash label succ-map))
                  (setq out (funcall 'neovm--ci-union out (gethash s in-map))))
                (let ((new-in (funcall 'neovm--ci-union
                                       (gethash label use-map)
                                       (funcall 'neovm--ci-diff
                                                out (gethash label def-map)))))
                  (unless (funcall 'neovm--ci-equal new-in (gethash label in-map))
                    (setq changed t))
                  (puthash label new-in in-map)))))
          ;; Return sorted results for comparison
          (let ((r nil))
            (dolist (blk blocks)
              (let ((label (car blk)))
                (setq r (cons (list label (gethash label in-map)) r))))
            (list :iterations iters :results (nreverse r)))))))

  (unwind-protect
      (let ((cfg '((B1 ((def x) (def y)) (B2 B3))
                   (B2 ((use x) (def z))  (B4))
                   (B3 ((use y) (def w))  (B4))
                   (B4 ((use z) (use w))  ()))))
        ;; Compare forward order vs reverse order
        (list
         :forward-order  (funcall 'neovm--ci-live-vars cfg '(B1 B2 B3 B4))
         :reverse-order  (funcall 'neovm--ci-live-vars cfg '(B4 B3 B2 B1))
         :mixed-order    (funcall 'neovm--ci-live-vars cfg '(B3 B1 B4 B2))))
    (fmakunbound 'neovm--ci-union)
    (fmakunbound 'neovm--ci-diff)
    (fmakunbound 'neovm--ci-equal)
    (fmakunbound 'neovm--ci-live-vars)))"#;
    let expect = expect_test::expect![[
        r#""OK (:forward-order (:iterations 4 :results ((B1 (w z)) (B2 (w x)) (B3 (y z)) (B4 (w z)))) :reverse-order (:iterations 2 :results ((B1 (w z)) (B2 (w x)) (B3 (y z)) (B4 (w z)))) :mixed-order (:iterations 3 :results ((B1 (w z)) (B2 (w x)) (B3 (y z)) (B4 (w z)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Very busy expressions analysis (backward must-analysis)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dataflow_very_busy_expressions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // An expression e is very busy at point p if every path from p must
    // evaluate e before any operand of e is redefined.
    // Backward, intersection (must-analysis).
    // IN[B] = USE_EXPR[B] union (OUT[B] - KILL_EXPR[B])
    // OUT[B] = intersection of IN[succ] (for non-exit blocks)
    let form = r#"(progn
  (fset 'neovm--vb-intersect
    (lambda (a b)
      (let ((r nil))
        (dolist (x a) (when (member x b) (setq r (cons x r))))
        (sort r (lambda (x y) (string< (format "%S" x) (format "%S" y)))))))

  (fset 'neovm--vb-union
    (lambda (a b)
      (let ((result (copy-sequence a)))
        (dolist (x b) (unless (member x result) (setq result (cons x result))))
        (sort result (lambda (x y) (string< (format "%S" x) (format "%S" y)))))))

  (fset 'neovm--vb-diff
    (lambda (a b)
      (let ((r nil)) (dolist (x a) (unless (member x b) (setq r (cons x r)))) r)))

  (fset 'neovm--vb-equal
    (lambda (a b) (and (= (length a) (length b)) (null (funcall 'neovm--vb-diff a b)))))

  ;; Compute USE_EXPR and KILL_EXPR for a block
  (fset 'neovm--vb-gen-kill
    (lambda (instrs all-exprs)
      "Return (USE_EXPR KILL_EXPR)."
      (let ((use-expr nil) (kill-expr nil))
        (dolist (instr instrs)
          (cond
           ((eq (car instr) 'expr)
            ;; (expr target expr-value)
            (let ((var (cadr instr))
                  (ev (nth 2 instr)))
              ;; First: expression ev is used
              (unless (or (member ev kill-expr) (member ev use-expr))
                (setq use-expr (cons ev use-expr)))
              ;; Then: var is defined, killing exprs containing var
              (dolist (e all-exprs)
                (when (memq var (cdr e))
                  (unless (member e kill-expr)
                    (setq kill-expr (cons e kill-expr)))
                  (setq use-expr (delete e use-expr))))))
           ((eq (car instr) 'def)
            (let ((var (cadr instr)))
              (dolist (e all-exprs)
                (when (memq var (cdr e))
                  (unless (member e kill-expr)
                    (setq kill-expr (cons e kill-expr)))
                  (setq use-expr (delete e use-expr))))))))
        (list use-expr kill-expr))))

  (fset 'neovm--vb-analyze
    (lambda (blocks all-exprs)
      (let ((use-map (make-hash-table :test 'eq))
            (kill-map (make-hash-table :test 'eq))
            (in-map (make-hash-table :test 'eq))
            (out-map (make-hash-table :test 'eq))
            (succ-map (make-hash-table :test 'eq)))
        ;; Build succ map and gen/kill
        (dolist (blk blocks)
          (puthash (car blk) (nth 2 blk) succ-map)
          (let ((gk (funcall 'neovm--vb-gen-kill (cadr blk) all-exprs)))
            (puthash (car blk) (car gk) use-map)
            (puthash (car blk) (cadr gk) kill-map))
          ;; Initialize IN to all-exprs (for intersection)
          (puthash (car blk) (copy-sequence all-exprs) in-map)
          (puthash (car blk) (copy-sequence all-exprs) out-map))
        ;; Fixed-point (process in reverse)
        (let ((changed t) (iters 0)
              (labels (nreverse (mapcar #'car blocks))))
          (while changed
            (setq changed nil) (setq iters (1+ iters))
            (dolist (label labels)
              ;; OUT = intersection of IN[succ]
              (let ((succs (gethash label succ-map))
                    (new-out nil))
                (if (null succs)
                    (setq new-out nil)  ;; Exit block: no exprs busy after
                  (setq new-out (copy-sequence (gethash (car succs) in-map)))
                  (dolist (s (cdr succs))
                    (setq new-out (funcall 'neovm--vb-intersect
                                           new-out (gethash s in-map)))))
                (puthash label new-out out-map)
                ;; IN = USE_EXPR union (OUT - KILL_EXPR)
                (let ((new-in (funcall 'neovm--vb-union
                                       (gethash label use-map)
                                       (funcall 'neovm--vb-diff
                                                new-out (gethash label kill-map)))))
                  (unless (funcall 'neovm--vb-equal new-in (gethash label in-map))
                    (setq changed t))
                  (puthash label new-in in-map)))))
          (let ((r nil))
            (dolist (blk blocks)
              (setq r (cons (list (car blk)
                                   :in (gethash (car blk) in-map)
                                   :out (gethash (car blk) out-map))
                             r)))
            (list :iterations iters :results (nreverse r)))))))

  (unwind-protect
      ;; Test:
      ;; B1: nothing -> B2, B3
      ;; B2: t1 = a+b -> B4
      ;; B3: t2 = a+b -> B4
      ;; B4: a = ... -> B5
      ;; B5: t3 = a+b
      ;; Expression (+ a b) is very busy at B1 (both paths use it before any kill)
      (funcall 'neovm--vb-analyze
               '((B1 ()                        (B2 B3))
                 (B2 ((expr t1 (+ a b)))       (B4))
                 (B3 ((expr t2 (+ a b)))       (B4))
                 (B4 ((def a))                 (B5))
                 (B5 ((expr t3 (+ a b)))       ()))
               '((+ a b)))
    (fmakunbound 'neovm--vb-intersect)
    (fmakunbound 'neovm--vb-union)
    (fmakunbound 'neovm--vb-diff)
    (fmakunbound 'neovm--vb-equal)
    (fmakunbound 'neovm--vb-gen-kill)
    (fmakunbound 'neovm--vb-analyze)))"#;
    let expect = expect_test::expect![[
        r#""OK (:iterations 2 :results ((B1 :in ((+ a b)) :out ((+ a b))) (B2 :in ((+ a b)) :out nil) (B3 :in ((+ a b)) :out nil) (B4 :in nil :out ((+ a b))) (B5 :in ((+ a b)) :out nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Liveness-based dead code detection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dataflow_dead_code_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use live variable analysis to detect dead definitions:
    // A definition of variable x in block B is dead if x is not live
    // at the point immediately after the definition.
    let form = r#"(progn
  (fset 'neovm--dc-union
    (lambda (a b)
      (let ((result (copy-sequence a)))
        (dolist (x b) (unless (memq x result) (setq result (cons x result))))
        (sort result (lambda (x y) (string< (symbol-name x) (symbol-name y)))))))

  (fset 'neovm--dc-diff
    (lambda (a b)
      (let ((r nil)) (dolist (x a) (unless (memq x b) (setq r (cons x r)))) r)))

  (fset 'neovm--dc-equal
    (lambda (a b) (and (= (length a) (length b)) (null (funcall 'neovm--dc-diff a b)))))

  ;; Compute liveness at each instruction within a block
  (fset 'neovm--dc-intra-block-liveness
    (lambda (instrs live-out)
      "Walk instructions backward, computing live set at each point.
Return list of (instr live-before live-after is-dead)."
      (let ((current-live (copy-sequence live-out))
            (results nil))
        (dolist (instr (nreverse (copy-sequence instrs)))
          (let ((live-after (copy-sequence current-live))
                (is-dead nil))
            (cond
             ((eq (car instr) 'def)
              (let ((var (cadr instr)))
                ;; Dead if var not in live-after
                (setq is-dead (not (memq var live-after)))
                ;; Remove var from live set (it's defined here)
                (setq current-live (delq var current-live))))
             ((eq (car instr) 'use)
              (let ((var (cadr instr)))
                (unless (memq var current-live)
                  (setq current-live (cons var current-live))))))
            (setq results (cons (list instr current-live live-after is-dead) results))))
        results)))

  ;; Full analysis: live variables + dead code detection
  (fset 'neovm--dc-analyze
    (lambda (blocks)
      ;; First, compute live-out for each block using backward analysis
      (let ((use-map (make-hash-table :test 'eq))
            (def-map (make-hash-table :test 'eq))
            (in-map (make-hash-table :test 'eq))
            (out-map (make-hash-table :test 'eq))
            (succ-map (make-hash-table :test 'eq)))
        (dolist (blk blocks)
          (let ((use-s nil) (def-s nil))
            (dolist (instr (cadr blk))
              (cond
               ((eq (car instr) 'use)
                (unless (memq (cadr instr) def-s)
                  (unless (memq (cadr instr) use-s)
                    (setq use-s (cons (cadr instr) use-s)))))
               ((eq (car instr) 'def)
                (unless (memq (cadr instr) def-s)
                  (setq def-s (cons (cadr instr) def-s))))))
            (puthash (car blk) use-s use-map)
            (puthash (car blk) def-s def-map)
            (puthash (car blk) nil in-map)
            (puthash (car blk) (nth 2 blk) succ-map)
            (puthash (car blk) nil out-map)))
        ;; Fixed-point for liveness
        (let ((changed t))
          (while changed
            (setq changed nil)
            (dolist (blk (nreverse (copy-sequence blocks)))
              (let ((label (car blk)) (out nil))
                (dolist (s (gethash label succ-map))
                  (setq out (funcall 'neovm--dc-union out (gethash s in-map))))
                (puthash label out out-map)
                (let ((new-in (funcall 'neovm--dc-union
                                       (gethash label use-map)
                                       (funcall 'neovm--dc-diff
                                                out (gethash label def-map)))))
                  (unless (funcall 'neovm--dc-equal new-in (gethash label in-map))
                    (setq changed t))
                  (puthash label new-in in-map))))))
        ;; Now detect dead code in each block
        (let ((dead-defs nil) (total-defs 0))
          (dolist (blk blocks)
            (let ((analysis (funcall 'neovm--dc-intra-block-liveness
                                     (cadr blk)
                                     (gethash (car blk) out-map))))
              (dolist (entry analysis)
                (when (eq (caar entry) 'def)
                  (setq total-defs (1+ total-defs))
                  (when (nth 3 entry)
                    (setq dead-defs (cons (list (car blk) (car entry)) dead-defs)))))))
          (list :total-defs total-defs
                :dead-defs (nreverse dead-defs))))))

  (unwind-protect
      ;; Test:
      ;; B1: def x, def y -> B2
      ;; B2: use x, def z, def w -> B3  (w is dead if B3 doesn't use it)
      ;; B3: use z
      (funcall 'neovm--dc-analyze
               '((B1 ((def x) (def y))          (B2))
                 (B2 ((use x) (def z) (def w))  (B3))
                 (B3 ((use z))                   ())))
    (fmakunbound 'neovm--dc-union)
    (fmakunbound 'neovm--dc-diff)
    (fmakunbound 'neovm--dc-equal)
    (fmakunbound 'neovm--dc-intra-block-liveness)
    (fmakunbound 'neovm--dc-analyze)))"#;
    let expect =
        expect_test::expect![[r#""OK (:total-defs 4 :dead-defs ((B1 (def y)) (B2 (def w))))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
