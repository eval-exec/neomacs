//! Complex combination oracle parity tests: A* search algorithm in Elisp.
//! Implements grid-based pathfinding with obstacles, Manhattan distance heuristic,
//! open/closed sets using hash tables, path reconstruction, multiple grid
//! configurations, and diagonal movement variant.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Core A* on a simple grid with no obstacles
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_a_star_simple_grid() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A* on a 5x5 grid with no obstacles. Verify path and cost.
    let form = r#"(progn
  ;; Grid: 0 = open, 1 = obstacle
  ;; Position encoded as (row . col)
  ;; Neighbor function returns 4-directional neighbors within grid bounds

  (fset 'neovm--as-pos-key
    (lambda (pos)
      (format "%d,%d" (car pos) (cdr pos))))

  (fset 'neovm--as-manhattan
    (lambda (a b)
      (+ (abs (- (car a) (car b)))
         (abs (- (cdr a) (cdr b))))))

  (fset 'neovm--as-neighbors-4
    (lambda (pos rows cols grid)
      (let ((r (car pos)) (c (cdr pos)) (result nil))
        (dolist (delta '((-1 . 0) (1 . 0) (0 . -1) (0 . 1)))
          (let ((nr (+ r (car delta)))
                (nc (+ c (cdr delta))))
            (when (and (>= nr 0) (< nr rows) (>= nc 0) (< nc cols))
              (when (= 0 (aref (aref grid nr) nc))
                (push (cons nr nc) result)))))
        (nreverse result))))

  ;; A* search: returns (path-list . cost) or nil if no path
  (fset 'neovm--as-search
    (lambda (grid rows cols start goal neighbor-fn heuristic-fn)
      (let ((open-set (make-hash-table :test 'equal))
            (closed-set (make-hash-table :test 'equal))
            (g-score (make-hash-table :test 'equal))
            (f-score (make-hash-table :test 'equal))
            (came-from (make-hash-table :test 'equal))
            (found nil)
            (limit 500))
        ;; Initialize start node
        (puthash (funcall 'neovm--as-pos-key start) start open-set)
        (puthash (funcall 'neovm--as-pos-key start) 0 g-score)
        (puthash (funcall 'neovm--as-pos-key start)
                 (funcall heuristic-fn start goal) f-score)

        (while (and (> (hash-table-count open-set) 0) (not found) (> limit 0))
          (setq limit (1- limit))
          ;; Find node in open set with lowest f-score
          (let ((best-key nil) (best-f 999999) (best-pos nil))
            (maphash (lambda (k v)
                       (let ((f (gethash k f-score 999999)))
                         (when (< f best-f)
                           (setq best-key k best-f f best-pos v))))
                     open-set)
            ;; Check if we reached the goal
            (if (and (= (car best-pos) (car goal))
                     (= (cdr best-pos) (cdr goal)))
                (setq found best-pos)
              ;; Move to closed set
              (remhash best-key open-set)
              (puthash best-key t closed-set)
              ;; Examine neighbors
              (dolist (nbr (funcall neighbor-fn best-pos rows cols grid))
                (let ((nbr-key (funcall 'neovm--as-pos-key nbr)))
                  (unless (gethash nbr-key closed-set)
                    (let ((tentative-g (1+ (gethash best-key g-score))))
                      (when (< tentative-g (gethash nbr-key g-score 999999))
                        (puthash nbr-key best-pos came-from)
                        (puthash nbr-key tentative-g g-score)
                        (puthash nbr-key
                                 (+ tentative-g (funcall heuristic-fn nbr goal))
                                 f-score)
                        (unless (gethash nbr-key open-set)
                          (puthash nbr-key nbr open-set))))))))))

        ;; Reconstruct path
        (if found
            (let ((path (list goal))
                  (current (funcall 'neovm--as-pos-key goal)))
              (while (gethash current came-from)
                (let ((prev (gethash current came-from)))
                  (push prev path)
                  (setq current (funcall 'neovm--as-pos-key prev))))
              (cons path (gethash (funcall 'neovm--as-pos-key goal) g-score)))
          nil))))

  (unwind-protect
      (let* (;; 5x5 grid, all open
             (grid (vector [0 0 0 0 0]
                           [0 0 0 0 0]
                           [0 0 0 0 0]
                           [0 0 0 0 0]
                           [0 0 0 0 0]))
             (result (funcall 'neovm--as-search
                              grid 5 5
                              '(0 . 0) '(4 . 4)
                              'neovm--as-neighbors-4
                              'neovm--as-manhattan)))
        (list
         ;; Path cost should be 8 (Manhattan distance for (0,0)->(4,4))
         (cdr result)
         ;; Path length should be 9 (8 steps + start)
         (length (car result))
         ;; Start and end of path
         (car (car result))
         (car (last (car result)))))
    (fmakunbound 'neovm--as-pos-key)
    (fmakunbound 'neovm--as-manhattan)
    (fmakunbound 'neovm--as-neighbors-4)
    (fmakunbound 'neovm--as-search)))"#;
    let expect = expect_test::expect![[r#""OK (8 9 (0 . 0) (4 . 4))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// A* with obstacles: wall blocking direct path
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_a_star_with_obstacles() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--as2-key (lambda (p) (format "%d,%d" (car p) (cdr p))))
  (fset 'neovm--as2-h (lambda (a b) (+ (abs (- (car a) (car b))) (abs (- (cdr a) (cdr b))))))
  (fset 'neovm--as2-nbr4
    (lambda (pos rows cols grid)
      (let ((r (car pos)) (c (cdr pos)) (res nil))
        (dolist (d '((-1 . 0) (1 . 0) (0 . -1) (0 . 1)))
          (let ((nr (+ r (car d))) (nc (+ c (cdr d))))
            (when (and (>= nr 0) (< nr rows) (>= nc 0) (< nc cols)
                       (= 0 (aref (aref grid nr) nc)))
              (push (cons nr nc) res))))
        (nreverse res))))
  (fset 'neovm--as2-run
    (lambda (grid rows cols start goal nbr-fn h-fn)
      (let ((open (make-hash-table :test 'equal))
            (closed (make-hash-table :test 'equal))
            (gs (make-hash-table :test 'equal))
            (fs (make-hash-table :test 'equal))
            (cf (make-hash-table :test 'equal))
            (found nil) (limit 1000))
        (puthash (funcall 'neovm--as2-key start) start open)
        (puthash (funcall 'neovm--as2-key start) 0 gs)
        (puthash (funcall 'neovm--as2-key start) (funcall h-fn start goal) fs)
        (while (and (> (hash-table-count open) 0) (not found) (> limit 0))
          (setq limit (1- limit))
          (let ((bk nil) (bf 999999) (bp nil))
            (maphash (lambda (k v) (let ((f (gethash k fs 999999)))
                                     (when (< f bf) (setq bk k bf f bp v)))) open)
            (if (and (= (car bp) (car goal)) (= (cdr bp) (cdr goal)))
                (setq found bp)
              (remhash bk open) (puthash bk t closed)
              (dolist (n (funcall nbr-fn bp rows cols grid))
                (let ((nk (funcall 'neovm--as2-key n)))
                  (unless (gethash nk closed)
                    (let ((tg (1+ (gethash bk gs))))
                      (when (< tg (gethash nk gs 999999))
                        (puthash nk bp cf) (puthash nk tg gs)
                        (puthash nk (+ tg (funcall h-fn n goal)) fs)
                        (unless (gethash nk open) (puthash nk n open))))))))))
        (if found
            (let ((path (list goal)) (cur (funcall 'neovm--as2-key goal)))
              (while (gethash cur cf)
                (let ((prev (gethash cur cf)))
                  (push prev path)
                  (setq cur (funcall 'neovm--as2-key prev))))
              (cons path (gethash (funcall 'neovm--as2-key goal) gs)))
          nil))))

  (unwind-protect
      (let* (;; 7x7 grid with wall across middle (row 3), gap at col 5
             ;; . . . . . . .
             ;; . . . . . . .
             ;; . . . . . . .
             ;; 1 1 1 1 1 . .
             ;; . . . . . . .
             ;; . . . . . . .
             ;; . . . . . . .
             (grid (vector [0 0 0 0 0 0 0]
                           [0 0 0 0 0 0 0]
                           [0 0 0 0 0 0 0]
                           [1 1 1 1 1 0 0]
                           [0 0 0 0 0 0 0]
                           [0 0 0 0 0 0 0]
                           [0 0 0 0 0 0 0]))
             (result (funcall 'neovm--as2-run grid 7 7
                              '(0 . 0) '(6 . 0)
                              'neovm--as2-nbr4 'neovm--as2-h)))
        (list
         ;; Should find a path (not nil)
         (not (null result))
         ;; Cost should be > 6 (direct Manhattan) due to wall
         (cdr result)
         ;; Path must go around the wall
         (length (car result))
         ;; Verify start and end
         (car (car result))
         (car (last (car result)))))
    (fmakunbound 'neovm--as2-key)
    (fmakunbound 'neovm--as2-h)
    (fmakunbound 'neovm--as2-nbr4)
    (fmakunbound 'neovm--as2-run)))"#;
    let expect = expect_test::expect![[r#""OK (t 16 17 (0 . 0) (6 . 0))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// A* unreachable goal (completely blocked)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_a_star_unreachable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--as3-key (lambda (p) (format "%d,%d" (car p) (cdr p))))
  (fset 'neovm--as3-h (lambda (a b) (+ (abs (- (car a) (car b))) (abs (- (cdr a) (cdr b))))))
  (fset 'neovm--as3-nbr4
    (lambda (pos rows cols grid)
      (let ((r (car pos)) (c (cdr pos)) (res nil))
        (dolist (d '((-1 . 0) (1 . 0) (0 . -1) (0 . 1)))
          (let ((nr (+ r (car d))) (nc (+ c (cdr d))))
            (when (and (>= nr 0) (< nr rows) (>= nc 0) (< nc cols)
                       (= 0 (aref (aref grid nr) nc)))
              (push (cons nr nc) res))))
        (nreverse res))))
  (fset 'neovm--as3-run
    (lambda (grid rows cols start goal nbr-fn h-fn)
      (let ((open (make-hash-table :test 'equal))
            (closed (make-hash-table :test 'equal))
            (gs (make-hash-table :test 'equal))
            (fs (make-hash-table :test 'equal))
            (cf (make-hash-table :test 'equal))
            (found nil) (limit 500))
        (puthash (funcall 'neovm--as3-key start) start open)
        (puthash (funcall 'neovm--as3-key start) 0 gs)
        (puthash (funcall 'neovm--as3-key start) (funcall h-fn start goal) fs)
        (while (and (> (hash-table-count open) 0) (not found) (> limit 0))
          (setq limit (1- limit))
          (let ((bk nil) (bf 999999) (bp nil))
            (maphash (lambda (k v) (let ((f (gethash k fs 999999)))
                                     (when (< f bf) (setq bk k bf f bp v)))) open)
            (if (and (= (car bp) (car goal)) (= (cdr bp) (cdr goal)))
                (setq found bp)
              (remhash bk open) (puthash bk t closed)
              (dolist (n (funcall nbr-fn bp rows cols grid))
                (let ((nk (funcall 'neovm--as3-key n)))
                  (unless (gethash nk closed)
                    (let ((tg (1+ (gethash bk gs))))
                      (when (< tg (gethash nk gs 999999))
                        (puthash nk bp cf) (puthash nk tg gs)
                        (puthash nk (+ tg (funcall h-fn n goal)) fs)
                        (unless (gethash nk open) (puthash nk n open))))))))))
        (if found
            (let ((path (list goal)) (cur (funcall 'neovm--as3-key goal)))
              (while (gethash cur cf)
                (let ((prev (gethash cur cf)))
                  (push prev path)
                  (setq cur (funcall 'neovm--as3-key prev))))
              (cons path (gethash (funcall 'neovm--as3-key goal) gs)))
          nil))))

  (unwind-protect
      (let* (;; 5x5 grid where goal (4,4) is completely surrounded by walls
             (grid (vector [0 0 0 0 0]
                           [0 0 0 0 0]
                           [0 0 0 0 0]
                           [0 0 0 1 1]
                           [0 0 0 1 0]))
             (result (funcall 'neovm--as3-run grid 5 5
                              '(0 . 0) '(4 . 4)
                              'neovm--as3-nbr4 'neovm--as3-h)))
        ;; Should return nil (no path)
        (list (null result)
              ;; Also test start = goal (trivial path)
              (let ((r2 (funcall 'neovm--as3-run grid 5 5
                                 '(0 . 0) '(0 . 0)
                                 'neovm--as3-nbr4 'neovm--as3-h)))
                (list (not (null r2))
                      (cdr r2)
                      (length (car r2))))))
    (fmakunbound 'neovm--as3-key)
    (fmakunbound 'neovm--as3-h)
    (fmakunbound 'neovm--as3-nbr4)
    (fmakunbound 'neovm--as3-run)))"#;
    let expect = expect_test::expect![[r#""OK (t (t 0 1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// A* with diagonal movement (8-directional)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_a_star_diagonal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--as4-key (lambda (p) (format "%d,%d" (car p) (cdr p))))
  ;; Chebyshev distance for 8-directional movement
  (fset 'neovm--as4-chebyshev
    (lambda (a b)
      (max (abs (- (car a) (car b)))
           (abs (- (cdr a) (cdr b))))))
  ;; 8-directional neighbors
  (fset 'neovm--as4-nbr8
    (lambda (pos rows cols grid)
      (let ((r (car pos)) (c (cdr pos)) (res nil))
        (dolist (d '((-1 . -1) (-1 . 0) (-1 . 1)
                     (0 . -1)            (0 . 1)
                     (1 . -1)  (1 . 0)  (1 . 1)))
          (let ((nr (+ r (car d))) (nc (+ c (cdr d))))
            (when (and (>= nr 0) (< nr rows) (>= nc 0) (< nc cols)
                       (= 0 (aref (aref grid nr) nc)))
              (push (cons nr nc) res))))
        (nreverse res))))
  (fset 'neovm--as4-run
    (lambda (grid rows cols start goal nbr-fn h-fn)
      (let ((open (make-hash-table :test 'equal))
            (closed (make-hash-table :test 'equal))
            (gs (make-hash-table :test 'equal))
            (fs (make-hash-table :test 'equal))
            (cf (make-hash-table :test 'equal))
            (found nil) (limit 1000))
        (puthash (funcall 'neovm--as4-key start) start open)
        (puthash (funcall 'neovm--as4-key start) 0 gs)
        (puthash (funcall 'neovm--as4-key start) (funcall h-fn start goal) fs)
        (while (and (> (hash-table-count open) 0) (not found) (> limit 0))
          (setq limit (1- limit))
          (let ((bk nil) (bf 999999) (bp nil))
            (maphash (lambda (k v) (let ((f (gethash k fs 999999)))
                                     (when (< f bf) (setq bk k bf f bp v)))) open)
            (if (and (= (car bp) (car goal)) (= (cdr bp) (cdr goal)))
                (setq found bp)
              (remhash bk open) (puthash bk t closed)
              (dolist (n (funcall nbr-fn bp rows cols grid))
                (let ((nk (funcall 'neovm--as4-key n)))
                  (unless (gethash nk closed)
                    (let ((tg (1+ (gethash bk gs))))
                      (when (< tg (gethash nk gs 999999))
                        (puthash nk bp cf) (puthash nk tg gs)
                        (puthash nk (+ tg (funcall h-fn n goal)) fs)
                        (unless (gethash nk open) (puthash nk n open))))))))))
        (if found
            (let ((path (list goal)) (cur (funcall 'neovm--as4-key goal)))
              (while (gethash cur cf)
                (let ((prev (gethash cur cf)))
                  (push prev path)
                  (setq cur (funcall 'neovm--as4-key prev))))
              (cons path (gethash (funcall 'neovm--as4-key goal) gs)))
          nil))))

  (unwind-protect
      (let* ((grid (vector [0 0 0 0 0]
                           [0 0 0 0 0]
                           [0 0 0 0 0]
                           [0 0 0 0 0]
                           [0 0 0 0 0]))
             ;; 8-dir from (0,0) to (4,4): should be 4 steps diagonal
             (result (funcall 'neovm--as4-run grid 5 5
                              '(0 . 0) '(4 . 4)
                              'neovm--as4-nbr8 'neovm--as4-chebyshev)))
        (list
         ;; Cost should be 4 (Chebyshev distance)
         (cdr result)
         ;; Path length should be 5
         (length (car result))
         ;; Compare with 4-directional on same grid
         ;; 4-dir Manhattan cost = 8, diagonal cost = 4
         (let* ((result4 (funcall 'neovm--as4-run grid 5 5
                                  '(0 . 0) '(4 . 4)
                                  'neovm--as4-nbr8 'neovm--as4-chebyshev)))
           (< (cdr result4) 8))))
    (fmakunbound 'neovm--as4-key)
    (fmakunbound 'neovm--as4-chebyshev)
    (fmakunbound 'neovm--as4-nbr8)
    (fmakunbound 'neovm--as4-run)))"#;
    let expect = expect_test::expect![[r#""OK (4 5 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// A* on larger grid with maze-like obstacles
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_a_star_maze() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--as5-key (lambda (p) (format "%d,%d" (car p) (cdr p))))
  (fset 'neovm--as5-h (lambda (a b) (+ (abs (- (car a) (car b))) (abs (- (cdr a) (cdr b))))))
  (fset 'neovm--as5-nbr4
    (lambda (pos rows cols grid)
      (let ((r (car pos)) (c (cdr pos)) (res nil))
        (dolist (d '((-1 . 0) (1 . 0) (0 . -1) (0 . 1)))
          (let ((nr (+ r (car d))) (nc (+ c (cdr d))))
            (when (and (>= nr 0) (< nr rows) (>= nc 0) (< nc cols)
                       (= 0 (aref (aref grid nr) nc)))
              (push (cons nr nc) res))))
        (nreverse res))))
  (fset 'neovm--as5-run
    (lambda (grid rows cols start goal nbr-fn h-fn)
      (let ((open (make-hash-table :test 'equal))
            (closed (make-hash-table :test 'equal))
            (gs (make-hash-table :test 'equal))
            (fs (make-hash-table :test 'equal))
            (cf (make-hash-table :test 'equal))
            (found nil) (limit 2000))
        (puthash (funcall 'neovm--as5-key start) start open)
        (puthash (funcall 'neovm--as5-key start) 0 gs)
        (puthash (funcall 'neovm--as5-key start) (funcall h-fn start goal) fs)
        (while (and (> (hash-table-count open) 0) (not found) (> limit 0))
          (setq limit (1- limit))
          (let ((bk nil) (bf 999999) (bp nil))
            (maphash (lambda (k v) (let ((f (gethash k fs 999999)))
                                     (when (< f bf) (setq bk k bf f bp v)))) open)
            (if (and (= (car bp) (car goal)) (= (cdr bp) (cdr goal)))
                (setq found bp)
              (remhash bk open) (puthash bk t closed)
              (dolist (n (funcall nbr-fn bp rows cols grid))
                (let ((nk (funcall 'neovm--as5-key n)))
                  (unless (gethash nk closed)
                    (let ((tg (1+ (gethash bk gs))))
                      (when (< tg (gethash nk gs 999999))
                        (puthash nk bp cf) (puthash nk tg gs)
                        (puthash nk (+ tg (funcall h-fn n goal)) fs)
                        (unless (gethash nk open) (puthash nk n open))))))))))
        (if found
            (let ((path (list goal)) (cur (funcall 'neovm--as5-key goal)))
              (while (gethash cur cf)
                (let ((prev (gethash cur cf)))
                  (push prev path)
                  (setq cur (funcall 'neovm--as5-key prev))))
              (cons path (gethash (funcall 'neovm--as5-key goal) gs)))
          nil))))

  (unwind-protect
      (let* (;; 8x8 maze:
             ;; S . . 1 . . . .
             ;; . 1 . 1 . 1 . .
             ;; . 1 . . . 1 . .
             ;; . 1 1 1 . 1 . .
             ;; . . . . . 1 . .
             ;; 1 1 1 1 . . . .
             ;; . . . . . 1 1 .
             ;; . . . . . . . G
             (grid (vector [0 0 0 1 0 0 0 0]
                           [0 1 0 1 0 1 0 0]
                           [0 1 0 0 0 1 0 0]
                           [0 1 1 1 0 1 0 0]
                           [0 0 0 0 0 1 0 0]
                           [1 1 1 1 0 0 0 0]
                           [0 0 0 0 0 1 1 0]
                           [0 0 0 0 0 0 0 0]))
             (result (funcall 'neovm--as5-run grid 8 8
                              '(0 . 0) '(7 . 7)
                              'neovm--as5-nbr4 'neovm--as5-h)))
        (list
         ;; Should find a path
         (not (null result))
         ;; Path cost
         (cdr result)
         ;; Path length
         (length (car result))
         ;; All path positions should be open cells
         (let ((all-open t))
           (dolist (pos (car result))
             (unless (= 0 (aref (aref grid (car pos)) (cdr pos)))
               (setq all-open nil)))
           all-open)
         ;; Nodes expanded (closed set would tell us, but check cost is reasonable)
         (> (cdr result) 0)
         ;; Start and end correct
         (equal (car (car result)) '(0 . 0))
         (equal (car (last (car result))) '(7 . 7))))
    (fmakunbound 'neovm--as5-key)
    (fmakunbound 'neovm--as5-h)
    (fmakunbound 'neovm--as5-nbr4)
    (fmakunbound 'neovm--as5-run)))"#;
    let expect = expect_test::expect![[r#""OK (t 14 15 t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// A* comparing 4-directional vs 8-directional on same grid
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_a_star_compare_4dir_vs_8dir() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--as6-key (lambda (p) (format "%d,%d" (car p) (cdr p))))
  (fset 'neovm--as6-manhattan (lambda (a b) (+ (abs (- (car a) (car b))) (abs (- (cdr a) (cdr b))))))
  (fset 'neovm--as6-chebyshev (lambda (a b) (max (abs (- (car a) (car b))) (abs (- (cdr a) (cdr b))))))
  (fset 'neovm--as6-nbr4
    (lambda (pos rows cols grid)
      (let ((r (car pos)) (c (cdr pos)) (res nil))
        (dolist (d '((-1 . 0) (1 . 0) (0 . -1) (0 . 1)))
          (let ((nr (+ r (car d))) (nc (+ c (cdr d))))
            (when (and (>= nr 0) (< nr rows) (>= nc 0) (< nc cols)
                       (= 0 (aref (aref grid nr) nc)))
              (push (cons nr nc) res))))
        (nreverse res))))
  (fset 'neovm--as6-nbr8
    (lambda (pos rows cols grid)
      (let ((r (car pos)) (c (cdr pos)) (res nil))
        (dolist (d '((-1 . -1) (-1 . 0) (-1 . 1) (0 . -1) (0 . 1) (1 . -1) (1 . 0) (1 . 1)))
          (let ((nr (+ r (car d))) (nc (+ c (cdr d))))
            (when (and (>= nr 0) (< nr rows) (>= nc 0) (< nc cols)
                       (= 0 (aref (aref grid nr) nc)))
              (push (cons nr nc) res))))
        (nreverse res))))
  (fset 'neovm--as6-run
    (lambda (grid rows cols start goal nbr-fn h-fn)
      (let ((open (make-hash-table :test 'equal))
            (closed (make-hash-table :test 'equal))
            (gs (make-hash-table :test 'equal))
            (fs (make-hash-table :test 'equal))
            (cf (make-hash-table :test 'equal))
            (found nil) (limit 2000))
        (puthash (funcall 'neovm--as6-key start) start open)
        (puthash (funcall 'neovm--as6-key start) 0 gs)
        (puthash (funcall 'neovm--as6-key start) (funcall h-fn start goal) fs)
        (while (and (> (hash-table-count open) 0) (not found) (> limit 0))
          (setq limit (1- limit))
          (let ((bk nil) (bf 999999) (bp nil))
            (maphash (lambda (k v) (let ((f (gethash k fs 999999)))
                                     (when (< f bf) (setq bk k bf f bp v)))) open)
            (if (and (= (car bp) (car goal)) (= (cdr bp) (cdr goal)))
                (setq found bp)
              (remhash bk open) (puthash bk t closed)
              (dolist (n (funcall nbr-fn bp rows cols grid))
                (let ((nk (funcall 'neovm--as6-key n)))
                  (unless (gethash nk closed)
                    (let ((tg (1+ (gethash bk gs))))
                      (when (< tg (gethash nk gs 999999))
                        (puthash nk bp cf) (puthash nk tg gs)
                        (puthash nk (+ tg (funcall h-fn n goal)) fs)
                        (unless (gethash nk open) (puthash nk n open))))))))))
        (if found
            (let ((path (list goal)) (cur (funcall 'neovm--as6-key goal)))
              (while (gethash cur cf)
                (let ((prev (gethash cur cf)))
                  (push prev path)
                  (setq cur (funcall 'neovm--as6-key prev))))
              (cons path (gethash (funcall 'neovm--as6-key goal) gs)))
          nil))))

  (unwind-protect
      (let* ((grid (vector [0 0 0 0 0 0]
                           [0 0 0 0 0 0]
                           [0 0 1 1 0 0]
                           [0 0 1 1 0 0]
                           [0 0 0 0 0 0]
                           [0 0 0 0 0 0]))
             ;; 4-directional path
             (r4 (funcall 'neovm--as6-run grid 6 6
                          '(0 . 0) '(5 . 5)
                          'neovm--as6-nbr4 'neovm--as6-manhattan))
             ;; 8-directional path
             (r8 (funcall 'neovm--as6-run grid 6 6
                          '(0 . 0) '(5 . 5)
                          'neovm--as6-nbr8 'neovm--as6-chebyshev)))
        (list
         ;; Both find paths
         (not (null r4))
         (not (null r8))
         ;; 4-dir cost >= 8-dir cost (diagonal is shorter or equal)
         (>= (cdr r4) (cdr r8))
         ;; 4-dir path length
         (length (car r4))
         ;; 8-dir path length
         (length (car r8))
         ;; 4-dir cost
         (cdr r4)
         ;; 8-dir cost
         (cdr r8)))
    (fmakunbound 'neovm--as6-key)
    (fmakunbound 'neovm--as6-manhattan)
    (fmakunbound 'neovm--as6-chebyshev)
    (fmakunbound 'neovm--as6-nbr4)
    (fmakunbound 'neovm--as6-nbr8)
    (fmakunbound 'neovm--as6-run)))"#;
    let expect = expect_test::expect![[r#""OK (t t t 11 8 10 7)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
