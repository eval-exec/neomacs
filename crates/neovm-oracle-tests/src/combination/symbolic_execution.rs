//! Oracle parity tests for symbolic execution concepts implemented in Elisp.
//!
//! Tests symbolic value representation, path condition management, constraint
//! collection along execution paths, branch exploration (both true/false),
//! loop bound handling, symbolic array access, and feasibility checking via
//! constraint solving.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Symbolic value representation and basic constraint generation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_symbolic_value_representation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Symbolic values: (sym NAME) for variables, (const VAL) for concrete,
  ;; (binop OP L R) for binary operations, (unop OP V) for unary.
  (fset 'neovm--sym-make (lambda (name) (list 'sym name)))
  (fset 'neovm--sym-const (lambda (val) (list 'const val)))
  (fset 'neovm--sym-binop (lambda (op l r) (list 'binop op l r)))
  (fset 'neovm--sym-unop (lambda (op v) (list 'unop op v)))
  (fset 'neovm--sym-type (lambda (v) (car v)))

  ;; Simplify: constant folding for known concrete values
  (fset 'neovm--sym-simplify
    (lambda (expr)
      (cond
       ((eq (car expr) 'const) expr)
       ((eq (car expr) 'sym) expr)
       ((eq (car expr) 'binop)
        (let ((op (nth 1 expr))
              (l (funcall 'neovm--sym-simplify (nth 2 expr)))
              (r (funcall 'neovm--sym-simplify (nth 3 expr))))
          (if (and (eq (car l) 'const) (eq (car r) 'const))
              (let ((lv (nth 1 l)) (rv (nth 1 r)))
                (funcall 'neovm--sym-const
                         (cond ((eq op '+) (+ lv rv))
                               ((eq op '-) (- lv rv))
                               ((eq op '*) (* lv rv))
                               (t (list op lv rv)))))
            (funcall 'neovm--sym-binop op l r))))
       ((eq (car expr) 'unop)
        (let ((op (nth 1 expr))
              (v (funcall 'neovm--sym-simplify (nth 2 expr))))
          (if (eq (car v) 'const)
              (funcall 'neovm--sym-const
                       (cond ((eq op 'neg) (- (nth 1 v)))
                             ((eq op 'not) (not (nth 1 v)))
                             (t (list op (nth 1 v)))))
            (funcall 'neovm--sym-unop op v))))
       (t expr))))

  (unwind-protect
      (let ((x (funcall 'neovm--sym-make 'x))
            (y (funcall 'neovm--sym-make 'y))
            (c5 (funcall 'neovm--sym-const 5))
            (c3 (funcall 'neovm--sym-const 3)))
        (list
         ;; Basic symbolic expressions
         x
         (funcall 'neovm--sym-binop '+ x c5)
         (funcall 'neovm--sym-binop '* y c3)
         ;; Constant folding
         (funcall 'neovm--sym-simplify
                  (funcall 'neovm--sym-binop '+ c5 c3))
         ;; Partial simplification (one side concrete)
         (funcall 'neovm--sym-simplify
                  (funcall 'neovm--sym-binop '+ x c5))
         ;; Nested constant folding
         (funcall 'neovm--sym-simplify
                  (funcall 'neovm--sym-binop '*
                           (funcall 'neovm--sym-binop '+ c5 c3)
                           (funcall 'neovm--sym-const 2)))
         ;; Unary negation of constant
         (funcall 'neovm--sym-simplify
                  (funcall 'neovm--sym-unop 'neg c5))
         ;; Unary negation of symbolic stays symbolic
         (funcall 'neovm--sym-simplify
                  (funcall 'neovm--sym-unop 'neg x))))
    (fmakunbound 'neovm--sym-make)
    (fmakunbound 'neovm--sym-const)
    (fmakunbound 'neovm--sym-binop)
    (fmakunbound 'neovm--sym-unop)
    (fmakunbound 'neovm--sym-type)
    (fmakunbound 'neovm--sym-simplify)))"#;
    let expect = expect_test::expect![[
        r#""OK ((sym x) (binop + (sym x) (const 5)) (binop * (sym y) (const 3)) (const 8) (binop + (sym x) (const 5)) (const 16) (const -5) (unop neg (sym x)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Path condition management: collecting constraints along execution paths
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_symbolic_path_conditions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Path condition: list of constraints (assume clauses)
  ;; Constraint: (cmp OP EXPR1 EXPR2) e.g., (cmp > (sym x) (const 0))
  (fset 'neovm--pc-empty (lambda () nil))
  (fset 'neovm--pc-add (lambda (pc constraint) (cons constraint pc)))
  (fset 'neovm--pc-constraints (lambda (pc) (reverse pc)))

  ;; Fork a path condition at a branch: returns (true-pc . false-pc)
  (fset 'neovm--pc-fork
    (lambda (pc condition)
      (let ((neg-condition (list 'not condition)))
        (cons (funcall 'neovm--pc-add pc condition)
              (funcall 'neovm--pc-add pc neg-condition)))))

  ;; Pretty-print a path condition as a conjunction string
  (fset 'neovm--pc-to-string
    (lambda (pc)
      (if (null pc)
          "true"
        (mapconcat
         (lambda (c) (prin1-to-string c))
         (funcall 'neovm--pc-constraints pc)
         " AND "))))

  (unwind-protect
      (let ((pc (funcall 'neovm--pc-empty)))
        ;; Add: x > 0
        (setq pc (funcall 'neovm--pc-add pc '(cmp > (sym x) (const 0))))
        ;; Add: y < 10
        (setq pc (funcall 'neovm--pc-add pc '(cmp < (sym y) (const 10))))
        (let ((before-fork (funcall 'neovm--pc-to-string pc)))
          ;; Fork on: x + y = 5
          (let* ((fork-result (funcall 'neovm--pc-fork pc '(cmp = (binop + (sym x) (sym y)) (const 5))))
                 (true-pc (car fork-result))
                 (false-pc (cdr fork-result)))
            (list
             ;; Constraints before fork
             before-fork
             ;; Number of constraints on each branch
             (length (funcall 'neovm--pc-constraints true-pc))
             (length (funcall 'neovm--pc-constraints false-pc))
             ;; True branch has the positive condition
             (funcall 'neovm--pc-to-string true-pc)
             ;; False branch has the negated condition
             (funcall 'neovm--pc-to-string false-pc)
             ;; Nested fork: on true branch, fork again on x = 3
             (let* ((nested (funcall 'neovm--pc-fork true-pc '(cmp = (sym x) (const 3))))
                    (tt-pc (car nested)))
               (length (funcall 'neovm--pc-constraints tt-pc)))))))
    (fmakunbound 'neovm--pc-empty)
    (fmakunbound 'neovm--pc-add)
    (fmakunbound 'neovm--pc-constraints)
    (fmakunbound 'neovm--pc-fork)
    (fmakunbound 'neovm--pc-to-string)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"(cmp > (sym x) (const 0)) AND (cmp < (sym y) (const 10))\" 3 3 \"(cmp > (sym x) (const 0)) AND (cmp < (sym y) (const 10)) AND (cmp = (binop + (sym x) (sym y)) (const 5))\" \"(cmp > (sym x) (const 0)) AND (cmp < (sym y) (const 10)) AND (not (cmp = (binop + (sym x) (sym y)) (const 5)))\" 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Branch exploration: symbolic execution of if-then-else
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_symbolic_branch_exploration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Symbolic execution state: (env path-condition result-trace)
  ;; env: alist of (name . symbolic-value)
  ;; Execute a simple program symbolically, exploring both branches of if.

  (fset 'neovm--se-state (lambda (env pc trace) (list env pc trace)))
  (fset 'neovm--se-env (lambda (s) (nth 0 s)))
  (fset 'neovm--se-pc (lambda (s) (nth 1 s)))
  (fset 'neovm--se-trace (lambda (s) (nth 2 s)))

  ;; Symbolically execute a mini-language:
  ;; (assign VAR EXPR) | (if-sym COND THEN ELSE) | (seq S1 S2) | (return EXPR)
  ;; Returns list of final states (one per explored path)
  (fset 'neovm--se-exec
    (lambda (stmt state)
      (cond
       ((eq (car stmt) 'assign)
        (let* ((var (nth 1 stmt))
               (expr (nth 2 stmt))
               (env (funcall 'neovm--se-env state))
               (new-env (cons (cons var expr) env)))
          (list (funcall 'neovm--se-state
                         new-env
                         (funcall 'neovm--se-pc state)
                         (funcall 'neovm--se-trace state)))))
       ((eq (car stmt) 'if-sym)
        (let* ((cond-expr (nth 1 stmt))
               (then-stmt (nth 2 stmt))
               (else-stmt (nth 3 stmt))
               (pc (funcall 'neovm--se-pc state))
               (true-pc (cons cond-expr pc))
               (false-pc (cons (list 'not cond-expr) pc))
               (true-state (funcall 'neovm--se-state
                                    (funcall 'neovm--se-env state)
                                    true-pc
                                    (funcall 'neovm--se-trace state)))
               (false-state (funcall 'neovm--se-state
                                     (funcall 'neovm--se-env state)
                                     false-pc
                                     (funcall 'neovm--se-trace state))))
          (append (funcall 'neovm--se-exec then-stmt true-state)
                  (funcall 'neovm--se-exec else-stmt false-state))))
       ((eq (car stmt) 'seq)
        (let ((states (funcall 'neovm--se-exec (nth 1 stmt) state))
              (all-results nil))
          (dolist (s states)
            (setq all-results
                  (append all-results
                          (funcall 'neovm--se-exec (nth 2 stmt) s))))
          all-results))
       ((eq (car stmt) 'return)
        (let ((expr (nth 1 stmt)))
          (list (funcall 'neovm--se-state
                         (funcall 'neovm--se-env state)
                         (funcall 'neovm--se-pc state)
                         (cons expr (funcall 'neovm--se-trace state))))))
       (t (list state)))))

  (unwind-protect
      (let ((init (funcall 'neovm--se-state
                           '((x . (sym x)) (y . (sym y)))
                           nil nil)))
        ;; Program: if x > 0 then z = x + 1 else z = x - 1; return z
        (let ((prog '(seq
                      (if-sym (cmp > (sym x) (const 0))
                              (assign z (binop + (sym x) (const 1)))
                              (assign z (binop - (sym x) (const 1))))
                      (return (sym z)))))
          (let ((results (funcall 'neovm--se-exec prog init)))
            (list
             ;; Number of explored paths
             (length results)
             ;; Path conditions of each result
             (mapcar (lambda (s) (funcall 'neovm--se-pc s)) results)
             ;; Environments of each result
             (mapcar (lambda (s)
                       (cdr (assq 'z (funcall 'neovm--se-env s))))
                     results)
             ;; Nested if: 2 branches x 2 branches = 4 paths
             (let ((prog2 '(seq
                            (if-sym (cmp > (sym x) (const 0))
                                    (if-sym (cmp > (sym y) (const 0))
                                            (assign r (const 1))
                                            (assign r (const 2)))
                                    (if-sym (cmp > (sym y) (const 0))
                                            (assign r (const 3))
                                            (assign r (const 4))))
                            (return (sym r)))))
               (length (funcall 'neovm--se-exec prog2 init)))))))
    (fmakunbound 'neovm--se-state)
    (fmakunbound 'neovm--se-env)
    (fmakunbound 'neovm--se-pc)
    (fmakunbound 'neovm--se-trace)
    (fmakunbound 'neovm--se-exec)))"#;
    let expect = expect_test::expect![[
        r#""OK (2 (((cmp > (sym x) (const 0))) ((not (cmp > (sym x) (const 0))))) ((binop + (sym x) (const 1)) (binop - (sym x) (const 1))) 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Loop bound handling: symbolic execution with bounded unrolling
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_symbolic_loop_bounds() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Bounded loop unrolling: symbolically execute a loop up to K iterations.
  ;; Loop: (loop-sym COND BODY MAX-UNROLL)
  ;; We transform into nested if-sym.

  (fset 'neovm--unroll-loop
    (lambda (cond-expr body-stmt k)
      (if (= k 0)
          '(nop)
        (list 'if-sym cond-expr
              (list 'seq body-stmt
                    (funcall 'neovm--unroll-loop cond-expr body-stmt (1- k)))
              '(nop)))))

  (unwind-protect
      (list
       ;; Unroll 0 times: just nop
       (funcall 'neovm--unroll-loop '(cmp > (sym i) (const 0)) '(assign i (binop - (sym i) (const 1))) 0)
       ;; Unroll 1 time: one if-sym
       (funcall 'neovm--unroll-loop '(cmp > (sym i) (const 0)) '(assign i (binop - (sym i) (const 1))) 1)
       ;; Unroll 2 times: nested
       (let ((unrolled (funcall 'neovm--unroll-loop
                                '(cmp > (sym i) (const 0))
                                '(assign i (binop - (sym i) (const 1)))
                                2)))
         ;; Verify structure: it's an if-sym at the top
         (car unrolled))
       ;; Unroll 3 times and check depth
       (let ((count-depth nil))
         (fset 'neovm--count-depth
           (lambda (tree)
             (if (or (atom tree) (eq (car tree) 'nop))
                 0
               (1+ (apply #'max (mapcar (lambda (sub)
                                          (if (listp sub)
                                              (funcall 'neovm--count-depth sub)
                                            0))
                                        (cdr tree)))))))
         (unwind-protect
             (funcall 'neovm--count-depth
                      (funcall 'neovm--unroll-loop
                               '(cmp > (sym i) (const 0))
                               '(assign i (const 0))
                               3))
           (fmakunbound 'neovm--count-depth))))
    (fmakunbound 'neovm--unroll-loop)))"#;
    let expect = expect_test::expect![[
        r#""OK ((nop) (if-sym (cmp > (sym i) (const 0)) (seq (assign i (binop - (sym i) (const 1))) (nop)) (nop)) if-sym 8)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Symbolic array access: modeling reads/writes to symbolic arrays
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_symbolic_array_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Symbolic array: list of (write INDEX VALUE) entries (most recent first).
  ;; A read at a symbolic index generates conditional expressions.

  (fset 'neovm--sa-empty (lambda () nil))

  (fset 'neovm--sa-write
    (lambda (arr idx val) (cons (list 'write idx val) arr)))

  ;; Read: walk writes from newest to oldest.
  ;; If index matches concretely, return value directly.
  ;; If index is symbolic, build an if-then-else chain.
  (fset 'neovm--sa-read
    (lambda (arr idx default)
      (if (null arr)
          default
        (let* ((entry (car arr))
               (w-idx (nth 1 entry))
               (w-val (nth 2 entry)))
          ;; If both concrete and equal, definite hit
          (if (and (eq (car w-idx) 'const)
                   (eq (car idx) 'const)
                   (equal (nth 1 w-idx) (nth 1 idx)))
              w-val
            ;; If both concrete and not equal, skip
            (if (and (eq (car w-idx) 'const)
                     (eq (car idx) 'const)
                     (not (equal (nth 1 w-idx) (nth 1 idx))))
                (funcall 'neovm--sa-read (cdr arr) idx default)
              ;; Otherwise, symbolic: conditional
              (list 'ite
                    (list 'cmp '= idx w-idx)
                    w-val
                    (funcall 'neovm--sa-read (cdr arr) idx default))))))))

  (unwind-protect
      (let ((arr (funcall 'neovm--sa-empty)))
        ;; Write concrete index 0 = 100
        (setq arr (funcall 'neovm--sa-write arr '(const 0) '(const 100)))
        ;; Write concrete index 1 = 200
        (setq arr (funcall 'neovm--sa-write arr '(const 1) '(const 200)))

        (list
         ;; Read concrete index 1: should be 200
         (funcall 'neovm--sa-read arr '(const 1) '(const 0))
         ;; Read concrete index 0: should be 100
         (funcall 'neovm--sa-read arr '(const 0) '(const 0))
         ;; Read concrete index 2 (not written): default
         (funcall 'neovm--sa-read arr '(const 2) '(const -1))
         ;; Read symbolic index: generates ite chain
         (funcall 'neovm--sa-read arr '(sym i) '(const -1))
         ;; Write at symbolic index, then read at concrete
         (let ((arr2 (funcall 'neovm--sa-write arr '(sym j) '(const 999))))
           (funcall 'neovm--sa-read arr2 '(const 0) '(const -1)))
         ;; Overwrite: write index 0 again, read should get new value
         (let ((arr3 (funcall 'neovm--sa-write arr '(const 0) '(const 777))))
           (funcall 'neovm--sa-read arr3 '(const 0) '(const -1)))))
    (fmakunbound 'neovm--sa-empty)
    (fmakunbound 'neovm--sa-write)
    (fmakunbound 'neovm--sa-read)))"#;
    let expect = expect_test::expect![[
        r#""OK ((const 200) (const 100) (const -1) (ite (cmp = (sym i) (const 1)) (const 200) (ite (cmp = (sym i) (const 0)) (const 100) (const -1))) (ite (cmp = (const 0) (sym j)) (const 999) (const 100)) (const 777))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Feasibility checking via simple constraint solving
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_symbolic_constraint_solver() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Simple interval-based constraint solver for integer variables.
  ;; State: alist of (var . (lo . hi)) representing var in [lo, hi].
  ;; Constraints: (> var const), (< var const), (= var const), (>= var const), (<= var const)

  (fset 'neovm--cs-init
    (lambda (vars lo hi)
      (mapcar (lambda (v) (cons v (cons lo hi))) vars)))

  (fset 'neovm--cs-get
    (lambda (state var)
      (cdr (assq var state))))

  (fset 'neovm--cs-set
    (lambda (state var lo hi)
      (let ((pair (assq var state)))
        (if pair
            (progn (setcdr pair (cons lo hi)) state)
          (cons (cons var (cons lo hi)) state)))))

  ;; Apply constraint, return updated state or nil if infeasible
  (fset 'neovm--cs-constrain
    (lambda (state constraint)
      (let* ((op (nth 0 constraint))
             (var (nth 1 constraint))
             (val (nth 2 constraint))
             (range (funcall 'neovm--cs-get state var))
             (lo (car range))
             (hi (cdr range)))
        (cond
         ((eq op '>) (let ((new-lo (max lo (1+ val))))
                       (if (> new-lo hi) nil
                         (funcall 'neovm--cs-set state var new-lo hi))))
         ((eq op '>=) (let ((new-lo (max lo val)))
                        (if (> new-lo hi) nil
                          (funcall 'neovm--cs-set state var new-lo hi))))
         ((eq op '<) (let ((new-hi (min hi (1- val))))
                       (if (< new-hi lo) nil
                         (funcall 'neovm--cs-set state var lo new-hi))))
         ((eq op '<=) (let ((new-hi (min hi val)))
                        (if (< new-hi lo) nil
                          (funcall 'neovm--cs-set state var lo new-hi))))
         ((eq op '=) (if (and (<= lo val) (<= val hi))
                         (funcall 'neovm--cs-set state var val val)
                       nil))
         (t state)))))

  ;; Apply a list of constraints
  (fset 'neovm--cs-solve
    (lambda (state constraints)
      (let ((current state)
            (feasible t))
        (dolist (c constraints)
          (when feasible
            (let ((result (funcall 'neovm--cs-constrain current c)))
              (if result
                  (setq current result)
                (setq feasible nil)))))
        (if feasible current nil))))

  (unwind-protect
      (let ((state (funcall 'neovm--cs-init '(x y z) -100 100)))
        (list
         ;; Single constraint: x > 5
         (funcall 'neovm--cs-solve state '((> x 5)))
         ;; Two constraints: x > 0, x < 10
         (funcall 'neovm--cs-solve state '((> x 0) (< x 10)))
         ;; Infeasible: x > 50 and x < 20
         (funcall 'neovm--cs-solve state '((> x 50) (< x 20)))
         ;; Equality: x = 42
         (funcall 'neovm--cs-get
                  (funcall 'neovm--cs-solve state '((= x 42)))
                  'x)
         ;; Multi-variable: x > 0, y < 0, z = 7
         (let ((result (funcall 'neovm--cs-solve state
                                '((> x 0) (< y 0) (= z 7)))))
           (list (funcall 'neovm--cs-get result 'x)
                 (funcall 'neovm--cs-get result 'y)
                 (funcall 'neovm--cs-get result 'z)))
         ;; Tightening: x >= 10, x <= 20, x >= 15
         (funcall 'neovm--cs-get
                  (funcall 'neovm--cs-solve state '((>= x 10) (<= x 20) (>= x 15)))
                  'x)
         ;; Point interval: x >= 5, x <= 5
         (funcall 'neovm--cs-get
                  (funcall 'neovm--cs-solve state '((>= x 5) (<= x 5)))
                  'x)))
    (fmakunbound 'neovm--cs-init)
    (fmakunbound 'neovm--cs-get)
    (fmakunbound 'neovm--cs-set)
    (fmakunbound 'neovm--cs-constrain)
    (fmakunbound 'neovm--cs-solve)))"#;
    let expect = expect_test::expect![[
        r#""OK (((x 6 . 9) (y -100 . -1) (z 7 . 7)) ((x 6 . 9) (y -100 . -1) (z 7 . 7)) nil nil ((6 . 9) (-100 . -1) (7 . 7)) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Full symbolic execution pipeline: program -> paths -> constraints -> check
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_symbolic_full_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Combine branch exploration with constraint checking.
  ;; Given a program with symbolic inputs, enumerate paths and check
  ;; which paths are feasible (have satisfiable path conditions).

  ;; Mini symbolic executor that collects path conditions as
  ;; simple interval constraints for checking.

  ;; Execute: returns list of (path-condition . final-value) pairs
  (fset 'neovm--pipe-exec
    (lambda (program pc)
      (cond
       ((eq (car program) 'val)
        (list (cons pc (nth 1 program))))
       ((eq (car program) 'if-check)
        ;; (if-check VAR OP CONST THEN ELSE)
        (let* ((var (nth 1 program))
               (op (nth 2 program))
               (val (nth 3 program))
               (then-branch (nth 4 program))
               (else-branch (nth 5 program))
               (pos-cond (list op var val))
               (neg-op (cond ((eq op '>) '<=) ((eq op '<) '>=)
                             ((eq op '>=) '<) ((eq op '<=) '>)
                             ((eq op '=) '/=) (t 'unknown)))
               (neg-cond (list neg-op var val)))
          (append
           (funcall 'neovm--pipe-exec then-branch (cons pos-cond pc))
           (funcall 'neovm--pipe-exec else-branch (cons neg-cond pc)))))
       (t (list (cons pc 'error))))))

  ;; Check feasibility of a path condition (list of interval constraints)
  ;; Uses simple interval logic. Returns t if all constraints can be satisfied.
  (fset 'neovm--pipe-feasible
    (lambda (pc)
      (let ((ranges (make-hash-table :test 'eq))
            (ok t))
        (dolist (c pc)
          (when ok
            (let* ((op (nth 0 c))
                   (var (nth 1 c))
                   (val (nth 2 c))
                   (range (gethash var ranges (cons -1000 1000)))
                   (lo (car range))
                   (hi (cdr range)))
              (cond
               ((eq op '>) (setq lo (max lo (1+ val))))
               ((eq op '>=) (setq lo (max lo val)))
               ((eq op '<) (setq hi (min hi (1- val))))
               ((eq op '<=) (setq hi (min hi val)))
               ((eq op '=) (setq lo (max lo val)) (setq hi (min hi val)))
               (t nil))
              (if (> lo hi) (setq ok nil)
                (puthash var (cons lo hi) ranges)))))
        ok)))

  (unwind-protect
      (let ((prog1 '(if-check x > 0
                      (if-check x < 100
                        (val path-a)
                        (val path-b))
                      (if-check x > -50
                        (val path-c)
                        (val path-d)))))
        (let ((paths (funcall 'neovm--pipe-exec prog1 nil)))
          (list
           ;; Number of paths explored
           (length paths)
           ;; All path results
           (mapcar #'cdr paths)
           ;; Feasibility of each path
           (mapcar (lambda (p) (funcall 'neovm--pipe-feasible (car p))) paths)
           ;; An infeasible program: x > 10 AND x < 5
           (let ((prog2 '(if-check x > 10
                           (if-check x < 5
                             (val impossible)
                             (val reachable))
                           (val also-reachable))))
             (let ((paths2 (funcall 'neovm--pipe-exec prog2 nil)))
               (mapcar (lambda (p)
                         (list (cdr p) (funcall 'neovm--pipe-feasible (car p))))
                       paths2))))))
    (fmakunbound 'neovm--pipe-exec)
    (fmakunbound 'neovm--pipe-feasible)))"#;
    let expect = expect_test::expect![[
        r#""OK (4 (path-a path-b path-c path-d) (t t t t) ((impossible nil) (reachable t) (also-reachable t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
