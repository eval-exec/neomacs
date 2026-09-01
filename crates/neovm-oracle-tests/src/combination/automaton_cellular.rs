//! Oracle parity tests for cellular automata implementations in Elisp.
//!
//! Tests 1D elementary cellular automata (Rule 30, Rule 110, Rule 90,
//! Rule 184), Conway's Game of Life (2D), generation stepping,
//! pattern detection (still lifes, oscillators), population counting,
//! and grid operations.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// 1D elementary cellular automata: multiple rules compared
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cellular_elementary_multi_rule() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--ca-step-1d
    (lambda (cells rule)
      "Apply one step of elementary CA with given rule number.
       CELLS is a vector of 0/1. Wrapping boundary conditions."
      (let* ((n (length cells))
             (next (make-vector n 0)))
        (dotimes (i n)
          (let* ((left (aref cells (% (+ i n -1) n)))
                 (center (aref cells i))
                 (right (aref cells (% (+ i 1) n)))
                 (neighborhood (+ (* left 4) (* center 2) right))
                 (new-val (if (= (logand rule (ash 1 neighborhood)) 0) 0 1)))
            (aset next i new-val)))
        next)))

  (fset 'neovm--ca-run-1d
    (lambda (init rule steps)
      "Run 1D CA for STEPS. Return list of cell vectors."
      (let ((cells (copy-sequence init))
            (history (list (append init nil))))
        (dotimes (_ steps)
          (setq cells (funcall 'neovm--ca-step-1d cells rule))
          (setq history (cons (append cells nil) history)))
        (nreverse history))))

  (fset 'neovm--ca-population
    (lambda (cells)
      (let ((count 0))
        (dotimes (i (length cells))
          (when (= (aref cells i) 1) (setq count (1+ count))))
        count)))

  (fset 'neovm--ca-to-string
    (lambda (cells)
      (let ((chars nil))
        (dotimes (i (length cells))
          (setq chars (cons (if (= (aref cells i) 1) ?# ?.) chars)))
        (concat (nreverse chars)))))

  (unwind-protect
      (let* ((width 19)
             (init (make-vector width 0)))
        (aset init (/ width 2) 1) ;; single cell in center
        ;; Compare 4 different rules
        (let ((rules '(30 90 110 184))
              (steps 8))
          (list
           ;; Run each rule and collect final state + population history
           (mapcar
            (lambda (rule)
              (let ((cells (copy-sequence init))
                    (pops nil))
                (setq pops (cons (funcall 'neovm--ca-population cells) pops))
                (dotimes (_ steps)
                  (setq cells (funcall 'neovm--ca-step-1d cells rule))
                  (setq pops (cons (funcall 'neovm--ca-population cells) pops)))
                (list rule
                      (funcall 'neovm--ca-to-string cells)
                      (nreverse pops))))
            rules)
           ;; Rule 30 first 5 generations
           (let ((hist (funcall 'neovm--ca-run-1d init 30 4)))
             (mapcar (lambda (row) (mapcar #'identity row)) hist))
           ;; Rule 90 symmetry check — Rule 90 produces symmetric patterns
           (let ((hist (funcall 'neovm--ca-run-1d init 90 6)))
             (mapcar (lambda (row) (equal row (reverse row))) hist))
           ;; Rule 184: traffic flow — density conservation
           ;; Total population should be conserved
           (let* ((traffic-init (vconcat '(1 0 1 1 0 0 1 0 1 0 0 1 1 0 1 0 0 0 1)))
                  (initial-pop (funcall 'neovm--ca-population traffic-init))
                  (cells (copy-sequence traffic-init))
                  (all-same t))
             (dotimes (_ 10)
               (setq cells (funcall 'neovm--ca-step-1d cells 184))
               (unless (= (funcall 'neovm--ca-population cells) initial-pop)
                 (setq all-same nil)))
             (list all-same initial-pop)))))
    (fmakunbound 'neovm--ca-step-1d)
    (fmakunbound 'neovm--ca-run-1d)
    (fmakunbound 'neovm--ca-population)
    (fmakunbound 'neovm--ca-to-string)))"#;
    let expect = expect_test::expect![[
        r#""OK (((30 \".##..#...###.....#.\" (1 3 3 6 4 9 5 12 7)) (90 \".#...............#.\" (1 2 2 4 2 4 4 8 2)) (110 \".#######.#.........\" (1 2 3 3 5 3 5 6 8)) (184 \".................#.\" (1 1 1 1 1 1 1 1 1))) ((0 0 0 0 0 0 0 0 0 1 0 0 0 0 0 0 0 0 0) (0 0 0 0 0 0 0 0 1 1 1 0 0 0 0 0 0 0 0) (0 0 0 0 0 0 0 1 1 0 0 1 0 0 0 0 0 0 0) (0 0 0 0 0 0 1 1 0 1 1 1 1 0 0 0 0 0 0) (0 0 0 0 0 1 1 0 0 1 0 0 0 1 0 0 0 0 0)) (t t t t t t t) (t 9))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 1D CA: Rule 110 (Turing-complete) with detailed pattern analysis
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cellular_rule110_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--ca-step-1d
    (lambda (cells rule)
      (let* ((n (length cells))
             (next (make-vector n 0)))
        (dotimes (i n)
          (let* ((l (aref cells (% (+ i n -1) n)))
                 (c (aref cells i))
                 (r (aref cells (% (+ i 1) n)))
                 (nb (+ (* l 4) (* c 2) r)))
            (aset next i (if (= (logand rule (ash 1 nb)) 0) 0 1))))
        next)))

  (fset 'neovm--ca-population
    (lambda (cells)
      (let ((count 0))
        (dotimes (i (length cells)) (when (= (aref cells i) 1) (setq count (1+ count))))
        count)))

  ;; Detect if the CA has reached a fixed point (same as previous step)
  (fset 'neovm--ca-is-fixed-point
    (lambda (cells rule)
      (equal (append cells nil)
             (append (funcall 'neovm--ca-step-1d cells rule) nil))))

  ;; Detect period: run until state repeats, return period length
  (fset 'neovm--ca-find-period
    (lambda (init rule max-steps)
      (let ((seen (make-hash-table :test 'equal))
            (cells (copy-sequence init))
            (step 0)
            (period nil))
        (puthash (prin1-to-string (append cells nil)) 0 seen)
        (while (and (< step max-steps) (null period))
          (setq cells (funcall 'neovm--ca-step-1d cells rule))
          (setq step (1+ step))
          (let* ((key (prin1-to-string (append cells nil)))
                 (prev (gethash key seen)))
            (if prev
                (setq period (- step prev))
              (puthash key step seen))))
        (or period 0))))

  (unwind-protect
      (let* ((width 25)
             (init (make-vector width 0)))
        (aset init (1- width) 1) ;; rightmost cell
        (list
         ;; Run Rule 110 for 15 steps, track populations
         (let ((cells (copy-sequence init))
               (pops nil))
           (dotimes (i 15)
             (setq cells (funcall 'neovm--ca-step-1d cells 110))
             (setq pops (cons (funcall 'neovm--ca-population cells) pops)))
           (nreverse pops))
         ;; Check: is any generation a fixed point?
         (let ((cells (copy-sequence init))
               (any-fixed nil))
           (dotimes (_ 15)
             (when (funcall 'neovm--ca-is-fixed-point cells 110)
               (setq any-fixed t))
             (setq cells (funcall 'neovm--ca-step-1d cells 110)))
           any-fixed)
         ;; Different initial conditions
         ;; All ones
         (let* ((all-ones (make-vector 15 1))
                (period (funcall 'neovm--ca-find-period all-ones 110 50)))
           (list period
                 (funcall 'neovm--ca-is-fixed-point all-ones 110)))
         ;; Alternating 1010...
         (let* ((alt (make-vector 16 0)))
           (dotimes (i 16) (when (= (% i 2) 0) (aset alt i 1)))
           (let ((period (funcall 'neovm--ca-find-period alt 110 50)))
             (list period (funcall 'neovm--ca-population alt))))
         ;; Rule 110 is NOT a simple rule — verify it doesn't converge quickly
         (let ((cells (copy-sequence init))
               (converged nil))
           (dotimes (_ 10)
             (let ((next (funcall 'neovm--ca-step-1d cells 110)))
               (when (equal (append cells nil) (append next nil))
                 (setq converged t))
               (setq cells next)))
           converged)))
    (fmakunbound 'neovm--ca-step-1d)
    (fmakunbound 'neovm--ca-population)
    (fmakunbound 'neovm--ca-is-fixed-point)
    (fmakunbound 'neovm--ca-find-period)))"#;
    let expect =
        expect_test::expect![[r#""OK ((2 3 3 5 3 5 6 8 5 6 8 8 8 11 11) nil (1 nil) (1 8) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Conway's Game of Life (2D): basic stepping and population
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cellular_game_of_life_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Grid is a hash-table mapping (row . col) -> 1 for live cells (sparse)
  (fset 'neovm--gol-make-grid
    (lambda (live-cells)
      "Create grid from list of (row col) pairs."
      (let ((grid (make-hash-table :test 'equal)))
        (dolist (cell live-cells)
          (puthash (cons (car cell) (cadr cell)) 1 grid))
        grid)))

  (fset 'neovm--gol-alive-p
    (lambda (grid r c)
      (gethash (cons r c) grid nil)))

  (fset 'neovm--gol-count-neighbors
    (lambda (grid r c)
      (let ((count 0))
        (dolist (dr '(-1 0 1))
          (dolist (dc '(-1 0 1))
            (unless (and (= dr 0) (= dc 0))
              (when (funcall 'neovm--gol-alive-p grid (+ r dr) (+ c dc))
                (setq count (1+ count))))))
        count)))

  (fset 'neovm--gol-step
    (lambda (grid)
      "Advance one generation. Returns new grid."
      ;; Collect all cells that need to be checked (live cells + their neighbors)
      (let ((candidates (make-hash-table :test 'equal))
            (next (make-hash-table :test 'equal)))
        (maphash (lambda (pos _)
                   (let ((r (car pos)) (c (cdr pos)))
                     (dolist (dr '(-1 0 1))
                       (dolist (dc '(-1 0 1))
                         (puthash (cons (+ r dr) (+ c dc)) t candidates)))))
                 grid)
        (maphash (lambda (pos _)
                   (let* ((r (car pos)) (c (cdr pos))
                          (neighbors (funcall 'neovm--gol-count-neighbors grid r c))
                          (alive (funcall 'neovm--gol-alive-p grid r c)))
                     (when (or (= neighbors 3)
                               (and alive (= neighbors 2)))
                       (puthash pos 1 next))))
                 candidates)
        next)))

  (fset 'neovm--gol-population
    (lambda (grid)
      (hash-table-count grid)))

  (fset 'neovm--gol-to-sorted-list
    (lambda (grid)
      "Return sorted list of live cell coordinates."
      (let ((cells nil))
        (maphash (lambda (pos _)
                   (setq cells (cons (list (car pos) (cdr pos)) cells)))
                 grid)
        (sort cells (lambda (a b)
                      (or (< (car a) (car b))
                          (and (= (car a) (car b)) (< (cadr a) (cadr b)))))))))

  (unwind-protect
      (list
       ;; Blinker: period-2 oscillator
       ;; Step 0: vertical    Step 1: horizontal
       (let* ((blinker (funcall 'neovm--gol-make-grid '((0 1) (1 1) (2 1))))
              (step1 (funcall 'neovm--gol-step blinker))
              (step2 (funcall 'neovm--gol-step step1)))
         (list (funcall 'neovm--gol-to-sorted-list blinker)
               (funcall 'neovm--gol-to-sorted-list step1)
               (funcall 'neovm--gol-to-sorted-list step2)
               ;; Period 2: step2 == step0
               (equal (funcall 'neovm--gol-to-sorted-list step2)
                      (funcall 'neovm--gol-to-sorted-list blinker))))
       ;; Block: still life (period 1)
       (let* ((block (funcall 'neovm--gol-make-grid '((0 0) (0 1) (1 0) (1 1))))
              (step1 (funcall 'neovm--gol-step block)))
         (list (equal (funcall 'neovm--gol-to-sorted-list block)
                      (funcall 'neovm--gol-to-sorted-list step1))
               (funcall 'neovm--gol-population block)))
       ;; Glider: moves diagonally, period 4 with displacement
       (let* ((glider (funcall 'neovm--gol-make-grid
                                '((0 1) (1 2) (2 0) (2 1) (2 2))))
              (pops nil)
              (grid glider))
         (dotimes (_ 4)
           (setq grid (funcall 'neovm--gol-step grid))
           (setq pops (cons (funcall 'neovm--gol-population grid) pops)))
         (list (nreverse pops)
               ;; Population stays constant (5 cells)
               (funcall 'neovm--gol-population glider)
               (funcall 'neovm--gol-population grid)))
       ;; Empty grid stays empty
       (let* ((empty (funcall 'neovm--gol-make-grid nil))
              (step1 (funcall 'neovm--gol-step empty)))
         (funcall 'neovm--gol-population step1))
       ;; Single cell dies
       (let* ((single (funcall 'neovm--gol-make-grid '((5 5))))
              (step1 (funcall 'neovm--gol-step single)))
         (funcall 'neovm--gol-population step1)))
    (fmakunbound 'neovm--gol-make-grid)
    (fmakunbound 'neovm--gol-alive-p)
    (fmakunbound 'neovm--gol-count-neighbors)
    (fmakunbound 'neovm--gol-step)
    (fmakunbound 'neovm--gol-population)
    (fmakunbound 'neovm--gol-to-sorted-list)))"#;
    let expect = expect_test::expect![[
        r#""OK ((((0 1) (1 1) (2 1)) ((1 0) (1 1) (1 2)) ((0 1) (1 1) (2 1)) t) (t 4) ((5 5 5 5) 5 5) 0 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Game of Life: pattern detection (still lifes, oscillators)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cellular_gol_pattern_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--gol-make-grid
    (lambda (live-cells)
      (let ((grid (make-hash-table :test 'equal)))
        (dolist (cell live-cells)
          (puthash (cons (car cell) (cadr cell)) 1 grid))
        grid)))

  (fset 'neovm--gol-alive-p
    (lambda (grid r c) (gethash (cons r c) grid nil)))

  (fset 'neovm--gol-count-neighbors
    (lambda (grid r c)
      (let ((count 0))
        (dolist (dr '(-1 0 1))
          (dolist (dc '(-1 0 1))
            (unless (and (= dr 0) (= dc 0))
              (when (funcall 'neovm--gol-alive-p grid (+ r dr) (+ c dc))
                (setq count (1+ count))))))
        count)))

  (fset 'neovm--gol-step
    (lambda (grid)
      (let ((candidates (make-hash-table :test 'equal))
            (next (make-hash-table :test 'equal)))
        (maphash (lambda (pos _)
                   (dolist (dr '(-1 0 1))
                     (dolist (dc '(-1 0 1))
                       (puthash (cons (+ (car pos) dr) (+ (cdr pos) dc)) t candidates))))
                 grid)
        (maphash (lambda (pos _)
                   (let* ((n (funcall 'neovm--gol-count-neighbors grid (car pos) (cdr pos)))
                          (alive (funcall 'neovm--gol-alive-p grid (car pos) (cdr pos))))
                     (when (or (= n 3) (and alive (= n 2)))
                       (puthash pos 1 next))))
                 candidates)
        next)))

  (fset 'neovm--gol-to-sorted-list
    (lambda (grid)
      (let ((cells nil))
        (maphash (lambda (pos _) (setq cells (cons (list (car pos) (cdr pos)) cells))) grid)
        (sort cells (lambda (a b) (or (< (car a) (car b))
                                      (and (= (car a) (car b)) (< (cadr a) (cadr b)))))))))

  (fset 'neovm--gol-population
    (lambda (grid) (hash-table-count grid)))

  ;; Detect pattern type: still-life, oscillator (with period), or other
  (fset 'neovm--gol-classify
    (lambda (initial max-period)
      "Classify pattern. Returns (type period) where type is still/oscillator/other."
      (let ((states (list (funcall 'neovm--gol-to-sorted-list initial)))
            (grid initial)
            (found nil))
        (dotimes (i max-period)
          (setq grid (funcall 'neovm--gol-step grid))
          (let ((current (funcall 'neovm--gol-to-sorted-list grid)))
            ;; Check against all previous states
            (let ((j 0) (match nil))
              (dolist (prev states)
                (when (equal current prev)
                  (setq match j))
                (setq j (1+ j)))
              (when match
                (unless found
                  (setq found (list (if (= (1+ i) 1) 'still 'oscillator)
                                    (- (1+ i) match))))))
            (setq states (append states (list current)))))
        (or found (list 'other 0)))))

  (unwind-protect
      (list
       ;; Still lifes
       ;; Block
       (funcall 'neovm--gol-classify
                (funcall 'neovm--gol-make-grid '((0 0) (0 1) (1 0) (1 1))) 5)
       ;; Beehive
       (funcall 'neovm--gol-classify
                (funcall 'neovm--gol-make-grid
                         '((0 1) (0 2) (1 0) (1 3) (2 1) (2 2))) 5)
       ;; Loaf
       (funcall 'neovm--gol-classify
                (funcall 'neovm--gol-make-grid
                         '((0 1) (0 2) (1 0) (1 3) (2 1) (2 3) (3 2))) 5)
       ;; Boat
       (funcall 'neovm--gol-classify
                (funcall 'neovm--gol-make-grid
                         '((0 0) (0 1) (1 0) (1 2) (2 1))) 5)
       ;; Oscillators
       ;; Blinker (period 2)
       (funcall 'neovm--gol-classify
                (funcall 'neovm--gol-make-grid '((0 0) (0 1) (0 2))) 5)
       ;; Toad (period 2)
       (funcall 'neovm--gol-classify
                (funcall 'neovm--gol-make-grid
                         '((1 0) (1 1) (1 2) (0 1) (0 2) (0 3))) 5)
       ;; Beacon (period 2)
       (funcall 'neovm--gol-classify
                (funcall 'neovm--gol-make-grid
                         '((0 0) (0 1) (1 0) (2 3) (3 2) (3 3))) 5)
       ;; Dying pattern: line of 2 (dies in 1 step)
       (let* ((grid (funcall 'neovm--gol-make-grid '((0 0) (0 1))))
              (step1 (funcall 'neovm--gol-step grid)))
         (list (funcall 'neovm--gol-population grid)
               (funcall 'neovm--gol-population step1))))
    (fmakunbound 'neovm--gol-make-grid)
    (fmakunbound 'neovm--gol-alive-p)
    (fmakunbound 'neovm--gol-count-neighbors)
    (fmakunbound 'neovm--gol-step)
    (fmakunbound 'neovm--gol-to-sorted-list)
    (fmakunbound 'neovm--gol-population)
    (fmakunbound 'neovm--gol-classify)))"#;
    let expect = expect_test::expect![[
        r#""OK ((still 1) (still 1) (still 1) (still 1) (oscillator 2) (oscillator 2) (oscillator 2) (2 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Game of Life: generation stepping with population tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cellular_gol_generation_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--gol-make-grid
    (lambda (live-cells)
      (let ((grid (make-hash-table :test 'equal)))
        (dolist (cell live-cells)
          (puthash (cons (car cell) (cadr cell)) 1 grid))
        grid)))

  (fset 'neovm--gol-count-neighbors
    (lambda (grid r c)
      (let ((count 0))
        (dolist (dr '(-1 0 1))
          (dolist (dc '(-1 0 1))
            (unless (and (= dr 0) (= dc 0))
              (when (gethash (cons (+ r dr) (+ c dc)) grid)
                (setq count (1+ count))))))
        count)))

  (fset 'neovm--gol-step
    (lambda (grid)
      (let ((candidates (make-hash-table :test 'equal))
            (next (make-hash-table :test 'equal)))
        (maphash (lambda (pos _)
                   (dolist (dr '(-1 0 1))
                     (dolist (dc '(-1 0 1))
                       (puthash (cons (+ (car pos) dr) (+ (cdr pos) dc)) t candidates))))
                 grid)
        (maphash (lambda (pos _)
                   (let* ((n (funcall 'neovm--gol-count-neighbors grid (car pos) (cdr pos)))
                          (alive (gethash pos grid)))
                     (when (or (= n 3) (and alive (= n 2)))
                       (puthash pos 1 next))))
                 candidates)
        next)))

  (fset 'neovm--gol-population (lambda (grid) (hash-table-count grid)))

  (fset 'neovm--gol-bounding-box
    (lambda (grid)
      "Return (min-r min-c max-r max-c) or nil for empty grid."
      (if (= (hash-table-count grid) 0) nil
        (let ((min-r 999) (min-c 999) (max-r -999) (max-c -999))
          (maphash (lambda (pos _)
                     (setq min-r (min min-r (car pos)))
                     (setq min-c (min min-c (cdr pos)))
                     (setq max-r (max max-r (car pos)))
                     (setq max-c (max max-c (cdr pos))))
                   grid)
          (list min-r min-c max-r max-c)))))

  (unwind-protect
      ;; R-pentomino: famous long-lived pattern
      (let* ((r-pentomino (funcall 'neovm--gol-make-grid
                                    '((0 1) (0 2) (1 0) (1 1) (2 1))))
             (grid r-pentomino)
             (pop-history nil)
             (box-history nil))
        ;; Run for 20 generations
        (setq pop-history (cons (funcall 'neovm--gol-population grid) pop-history))
        (setq box-history (cons (funcall 'neovm--gol-bounding-box grid) box-history))
        (dotimes (_ 20)
          (setq grid (funcall 'neovm--gol-step grid))
          (setq pop-history (cons (funcall 'neovm--gol-population grid) pop-history))
          (setq box-history (cons (funcall 'neovm--gol-bounding-box grid) box-history)))
        (list
         ;; Population history (21 entries, reversed chronological)
         (nreverse pop-history)
         ;; Bounding box growth
         (nreverse box-history)
         ;; Initial population
         (funcall 'neovm--gol-population r-pentomino)
         ;; Population after 20 steps
         (funcall 'neovm--gol-population grid)
         ;; R-pentomino grows (population increases)
         (> (funcall 'neovm--gol-population grid)
            (funcall 'neovm--gol-population r-pentomino))
         ;; Bounding box expands
         (let ((init-box (funcall 'neovm--gol-bounding-box r-pentomino))
               (final-box (funcall 'neovm--gol-bounding-box grid)))
           (> (- (nth 2 final-box) (nth 0 final-box))
              (- (nth 2 init-box) (nth 0 init-box))))))
    (fmakunbound 'neovm--gol-make-grid)
    (fmakunbound 'neovm--gol-count-neighbors)
    (fmakunbound 'neovm--gol-step)
    (fmakunbound 'neovm--gol-population)
    (fmakunbound 'neovm--gol-bounding-box)))"#;
    let expect = expect_test::expect![[
        r#""OK ((5 6 7 9 8 9 12 11 18 11 11 10 13 16 19 19 23 25 35 25 32) ((0 0 2 2) (0 0 2 2) (-1 -1 2 2) (-1 -1 2 2) (-1 -1 2 2) (-1 -1 2 3) (-1 -1 2 3) (-2 -1 2 3) (-2 -2 3 3) (-3 -2 3 3) (-1 -3 4 1) (-1 -3 3 2) (-2 -4 3 2) (-2 -4 4 2) (-2 -5 4 2) (-2 -5 4 2) (-2 -6 4 2) (-3 -6 4 2) (-3 -7 4 3) (-4 -7 5 3) (-2 -8 5 3)) 5 32 t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Grid operations: rotation, reflection, translation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cellular_grid_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--gol-make-grid
    (lambda (live-cells)
      (let ((grid (make-hash-table :test 'equal)))
        (dolist (cell live-cells)
          (puthash (cons (car cell) (cadr cell)) 1 grid))
        grid)))

  (fset 'neovm--gol-to-sorted-list
    (lambda (grid)
      (let ((cells nil))
        (maphash (lambda (pos _) (setq cells (cons (list (car pos) (cdr pos)) cells))) grid)
        (sort cells (lambda (a b) (or (< (car a) (car b))
                                      (and (= (car a) (car b)) (< (cadr a) (cadr b)))))))))

  ;; Translate pattern so minimum coordinate is (0,0)
  (fset 'neovm--gol-normalize
    (lambda (grid)
      (let ((cells (funcall 'neovm--gol-to-sorted-list grid)))
        (if (null cells) grid
          (let ((min-r (apply #'min (mapcar #'car cells)))
                (min-c (apply #'min (mapcar #'cadr cells))))
            (funcall 'neovm--gol-make-grid
                     (mapcar (lambda (c) (list (- (car c) min-r) (- (cadr c) min-c)))
                             cells)))))))

  ;; Rotate 90 degrees clockwise: (r, c) -> (c, -r)
  (fset 'neovm--gol-rotate-cw
    (lambda (grid)
      (let ((cells nil))
        (maphash (lambda (pos _)
                   (setq cells (cons (list (cdr pos) (- (car pos))) cells)))
                 grid)
        (funcall 'neovm--gol-normalize
                 (funcall 'neovm--gol-make-grid cells)))))

  ;; Reflect horizontally: (r, c) -> (r, -c)
  (fset 'neovm--gol-reflect-h
    (lambda (grid)
      (let ((cells nil))
        (maphash (lambda (pos _)
                   (setq cells (cons (list (car pos) (- (cdr pos))) cells)))
                 grid)
        (funcall 'neovm--gol-normalize
                 (funcall 'neovm--gol-make-grid cells)))))

  ;; Reflect vertically: (r, c) -> (-r, c)
  (fset 'neovm--gol-reflect-v
    (lambda (grid)
      (let ((cells nil))
        (maphash (lambda (pos _)
                   (setq cells (cons (list (- (car pos)) (cdr pos)) cells)))
                 grid)
        (funcall 'neovm--gol-normalize
                 (funcall 'neovm--gol-make-grid cells)))))

  ;; Check if two patterns are the same (after normalization)
  (fset 'neovm--gol-same-pattern-p
    (lambda (g1 g2)
      (equal (funcall 'neovm--gol-to-sorted-list
                       (funcall 'neovm--gol-normalize g1))
             (funcall 'neovm--gol-to-sorted-list
                       (funcall 'neovm--gol-normalize g2)))))

  (unwind-protect
      (let ((glider (funcall 'neovm--gol-make-grid
                              '((0 1) (1 2) (2 0) (2 1) (2 2))))
            (block (funcall 'neovm--gol-make-grid
                             '((0 0) (0 1) (1 0) (1 1)))))
        (list
         ;; Glider rotated 4 times returns to original (up to normalization)
         (let ((g glider))
           (dotimes (_ 4) (setq g (funcall 'neovm--gol-rotate-cw g)))
           (funcall 'neovm--gol-same-pattern-p glider g))
         ;; Block is symmetric under rotation
         (funcall 'neovm--gol-same-pattern-p
                  block (funcall 'neovm--gol-rotate-cw block))
         ;; Block is symmetric under horizontal reflection
         (funcall 'neovm--gol-same-pattern-p
                  block (funcall 'neovm--gol-reflect-h block))
         ;; Glider is NOT symmetric under rotation
         (funcall 'neovm--gol-same-pattern-p
                  glider (funcall 'neovm--gol-rotate-cw glider))
         ;; Double reflection = identity
         (funcall 'neovm--gol-same-pattern-p
                  glider
                  (funcall 'neovm--gol-reflect-h
                           (funcall 'neovm--gol-reflect-h glider)))
         (funcall 'neovm--gol-same-pattern-p
                  glider
                  (funcall 'neovm--gol-reflect-v
                           (funcall 'neovm--gol-reflect-v glider)))
         ;; Normalized coordinates of glider and its rotations
         (funcall 'neovm--gol-to-sorted-list (funcall 'neovm--gol-normalize glider))
         (funcall 'neovm--gol-to-sorted-list
                  (funcall 'neovm--gol-normalize
                           (funcall 'neovm--gol-rotate-cw glider)))
         ;; Blinker vertical and horizontal reflection
         (let ((blinker-v (funcall 'neovm--gol-make-grid '((0 0) (1 0) (2 0))))
               (blinker-h (funcall 'neovm--gol-make-grid '((0 0) (0 1) (0 2)))))
           (list
            ;; Rotating vertical blinker gives horizontal
            (funcall 'neovm--gol-same-pattern-p
                     blinker-h
                     (funcall 'neovm--gol-rotate-cw blinker-v))
            ;; Reflecting vertical blinker keeps it the same
            (funcall 'neovm--gol-same-pattern-p
                     blinker-v
                     (funcall 'neovm--gol-reflect-h blinker-v))))))
    (fmakunbound 'neovm--gol-make-grid)
    (fmakunbound 'neovm--gol-to-sorted-list)
    (fmakunbound 'neovm--gol-normalize)
    (fmakunbound 'neovm--gol-rotate-cw)
    (fmakunbound 'neovm--gol-reflect-h)
    (fmakunbound 'neovm--gol-reflect-v)
    (fmakunbound 'neovm--gol-same-pattern-p)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t nil t t ((0 1) (1 2) (2 0) (2 1) (2 2)) ((0 0) (1 0) (1 2) (2 0) (2 1)) (t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 1D CA: space-time pattern analysis and entropy
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cellular_1d_spacetime_analysis() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--ca-step-1d
    (lambda (cells rule)
      (let* ((n (length cells))
             (next (make-vector n 0)))
        (dotimes (i n)
          (let* ((l (aref cells (% (+ i n -1) n)))
                 (c (aref cells i))
                 (r (aref cells (% (+ i 1) n)))
                 (nb (+ (* l 4) (* c 2) r)))
            (aset next i (if (= (logand rule (ash 1 nb)) 0) 0 1))))
        next)))

  (fset 'neovm--ca-population
    (lambda (cells)
      (let ((count 0))
        (dotimes (i (length cells)) (when (= (aref cells i) 1) (setq count (1+ count))))
        count)))

  ;; Count number of 0->1 and 1->0 transitions (activity measure)
  (fset 'neovm--ca-activity
    (lambda (prev curr)
      (let ((changes 0)
            (n (length prev)))
        (dotimes (i n)
          (unless (= (aref prev i) (aref curr i))
            (setq changes (1+ changes))))
        changes)))

  ;; Hamming distance between two states
  (fset 'neovm--ca-hamming
    (lambda (a b)
      (let ((dist 0)
            (n (length a)))
        (dotimes (i n)
          (unless (= (aref a i) (aref b i))
            (setq dist (1+ dist))))
        dist)))

  ;; Run two CAs with slightly different initial conditions and measure divergence
  (fset 'neovm--ca-sensitivity
    (lambda (init rule bit-to-flip steps)
      "Flip one bit and run both. Return list of hamming distances per step."
      (let* ((init2 (copy-sequence init))
             (_ (aset init2 bit-to-flip (- 1 (aref init2 bit-to-flip))))
             (cells1 (copy-sequence init))
             (cells2 (copy-sequence init2))
             (distances nil))
        (dotimes (_ steps)
          (setq cells1 (funcall 'neovm--ca-step-1d cells1 rule))
          (setq cells2 (funcall 'neovm--ca-step-1d cells2 rule))
          (setq distances (cons (funcall 'neovm--ca-hamming cells1 cells2) distances)))
        (nreverse distances))))

  (unwind-protect
      (let* ((width 21)
             (init (make-vector width 0)))
        (aset init (/ width 2) 1)
        (list
         ;; Activity per step for Rule 30 (chaotic — high activity)
         (let ((cells (copy-sequence init))
               (activities nil))
           (dotimes (_ 10)
             (let ((next (funcall 'neovm--ca-step-1d cells 30)))
               (setq activities (cons (funcall 'neovm--ca-activity cells next) activities))
               (setq cells next)))
           (nreverse activities))
         ;; Activity per step for Rule 0 (all die — one step of activity)
         (let ((cells (copy-sequence init))
               (activities nil))
           (dotimes (_ 5)
             (let ((next (funcall 'neovm--ca-step-1d cells 0)))
               (setq activities (cons (funcall 'neovm--ca-activity cells next) activities))
               (setq cells next)))
           (nreverse activities))
         ;; Sensitivity: Rule 30 (chaotic) should diverge
         (funcall 'neovm--ca-sensitivity init 30 (/ width 2) 10)
         ;; Sensitivity: Rule 0 (trivial) — no divergence after death
         (funcall 'neovm--ca-sensitivity init 0 (/ width 2) 5)
         ;; Compare population curves: Rule 30 vs Rule 90
         (let ((pop30 nil) (pop90 nil)
               (c30 (copy-sequence init))
               (c90 (copy-sequence init)))
           (dotimes (_ 8)
             (setq c30 (funcall 'neovm--ca-step-1d c30 30))
             (setq c90 (funcall 'neovm--ca-step-1d c90 90))
             (setq pop30 (cons (funcall 'neovm--ca-population c30) pop30))
             (setq pop90 (cons (funcall 'neovm--ca-population c90) pop90)))
           (list (nreverse pop30) (nreverse pop90)))))
    (fmakunbound 'neovm--ca-step-1d)
    (fmakunbound 'neovm--ca-population)
    (fmakunbound 'neovm--ca-activity)
    (fmakunbound 'neovm--ca-hamming)
    (fmakunbound 'neovm--ca-sensitivity)))"#;
    let expect = expect_test::expect![[
        r#""OK ((2 4 5 6 7 8 9 13 11 13) (1 0 0 0 0) (3 3 6 4 9 5 12 7 12 11) (0 0 0 0 0) ((3 3 6 4 9 5 12 7) (2 2 4 2 4 4 8 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
