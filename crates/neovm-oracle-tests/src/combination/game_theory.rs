//! Oracle parity tests for game theory concepts implemented in Elisp.
//!
//! Tests payoff matrix representation, Nash equilibrium finder (2-player),
//! dominant strategy detection, mixed strategy computation, iterated
//! prisoner's dilemma with strategies (tit-for-tat, always-cooperate,
//! always-defect, random), tournament simulation, and minimax for
//! zero-sum games.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Payoff matrix representation and lookup
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_game_theory_payoff_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; A game is: (strategies-p1 strategies-p2 payoff-matrix)
  ;; payoff-matrix: vector of vectors, each cell is (p1-payoff . p2-payoff)
  ;; Row = p1's strategy, Col = p2's strategy

  (fset 'neovm--gt-make-game
    (lambda (s1 s2 matrix) (list s1 s2 matrix)))
  (fset 'neovm--gt-strategies1 (lambda (g) (nth 0 g)))
  (fset 'neovm--gt-strategies2 (lambda (g) (nth 1 g)))
  (fset 'neovm--gt-matrix (lambda (g) (nth 2 g)))
  (fset 'neovm--gt-payoff
    (lambda (g i j) (aref (aref (nth 2 g) i) j)))
  (fset 'neovm--gt-p1-payoff (lambda (cell) (car cell)))
  (fset 'neovm--gt-p2-payoff (lambda (cell) (cdr cell)))

  (unwind-protect
      (let ((pd (funcall 'neovm--gt-make-game
                         '(cooperate defect) '(cooperate defect)
                         ;; Classic prisoner's dilemma payoffs:
                         ;;              P2:Coop  P2:Def
                         ;; P1:Coop      (3.3)   (0.5)
                         ;; P1:Def       (5.0)   (1.1)
                         (vector (vector '(3 . 3) '(0 . 5))
                                 (vector '(5 . 0) '(1 . 1))))))
        (list
         ;; Strategy names
         (funcall 'neovm--gt-strategies1 pd)
         (funcall 'neovm--gt-strategies2 pd)
         ;; Look up specific payoffs
         (funcall 'neovm--gt-payoff pd 0 0)  ;; both cooperate: (3.3)
         (funcall 'neovm--gt-payoff pd 0 1)  ;; p1 coop, p2 defect: (0.5)
         (funcall 'neovm--gt-payoff pd 1 0)  ;; p1 defect, p2 coop: (5.0)
         (funcall 'neovm--gt-payoff pd 1 1)  ;; both defect: (1.1)
         ;; Extract p1 and p2 payoffs separately
         (funcall 'neovm--gt-p1-payoff (funcall 'neovm--gt-payoff pd 1 0))
         (funcall 'neovm--gt-p2-payoff (funcall 'neovm--gt-payoff pd 1 0))
         ;; Row sums for p1 (total payoff across p2's strategies)
         (let ((row0-sum 0) (row1-sum 0))
           (dotimes (j 2)
             (setq row0-sum (+ row0-sum (funcall 'neovm--gt-p1-payoff
                                                 (funcall 'neovm--gt-payoff pd 0 j))))
             (setq row1-sum (+ row1-sum (funcall 'neovm--gt-p1-payoff
                                                 (funcall 'neovm--gt-payoff pd 1 j)))))
           (list row0-sum row1-sum))))
    (fmakunbound 'neovm--gt-make-game)
    (fmakunbound 'neovm--gt-strategies1)
    (fmakunbound 'neovm--gt-strategies2)
    (fmakunbound 'neovm--gt-matrix)
    (fmakunbound 'neovm--gt-payoff)
    (fmakunbound 'neovm--gt-p1-payoff)
    (fmakunbound 'neovm--gt-p2-payoff)))"#;
    let expect = expect_test::expect![[
        r#""OK ((cooperate defect) (cooperate defect) (3 . 3) (0 . 5) (5 . 0) (1 . 1) 5 0 (3 6))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dominant strategy detection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_game_theory_dominant_strategy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; A strategy i strictly dominates strategy j for player 1 if
  ;; p1-payoff(i, k) > p1-payoff(j, k) for ALL k (p2's strategies).
  ;; Similarly for player 2 (compare columns).

  (fset 'neovm--gt-p1-dominates
    (lambda (matrix i j ncols)
      (let ((dom t))
        (dotimes (k ncols)
          (unless (> (car (aref (aref matrix i) k))
                     (car (aref (aref matrix j) k)))
            (setq dom nil)))
        dom)))

  (fset 'neovm--gt-p2-dominates
    (lambda (matrix i j nrows)
      (let ((dom t))
        (dotimes (k nrows)
          (unless (> (cdr (aref (aref matrix k) i))
                     (cdr (aref (aref matrix k) j)))
            (setq dom nil)))
        dom)))

  ;; Find dominant strategy for p1 (one that dominates all others)
  (fset 'neovm--gt-find-dominant-p1
    (lambda (matrix nrows ncols)
      (let ((result nil))
        (dotimes (i nrows)
          (let ((dominates-all t))
            (dotimes (j nrows)
              (when (and (/= i j)
                         (not (funcall 'neovm--gt-p1-dominates matrix i j ncols)))
                (setq dominates-all nil)))
            (when dominates-all (setq result i))))
        result)))

  (fset 'neovm--gt-find-dominant-p2
    (lambda (matrix nrows ncols)
      (let ((result nil))
        (dotimes (i ncols)
          (let ((dominates-all t))
            (dotimes (j ncols)
              (when (and (/= i j)
                         (not (funcall 'neovm--gt-p2-dominates matrix i j nrows)))
                (setq dominates-all nil)))
            (when dominates-all (setq result i))))
        result)))

  (unwind-protect
      (list
       ;; Prisoner's dilemma: defect dominates cooperate for both players
       (let ((pd (vector (vector '(3 . 3) '(0 . 5))
                         (vector '(5 . 0) '(1 . 1)))))
         (list (funcall 'neovm--gt-find-dominant-p1 pd 2 2)   ;; 1 (defect)
               (funcall 'neovm--gt-find-dominant-p2 pd 2 2))) ;; 1 (defect)
       ;; Game where no dominant strategy exists (matching pennies)
       ;; Heads/Tails: p1 wants match, p2 wants mismatch
       (let ((mp (vector (vector '(1 . -1) '(-1 . 1))
                         (vector '(-1 . 1) '(1 . -1)))))
         (list (funcall 'neovm--gt-find-dominant-p1 mp 2 2)   ;; nil
               (funcall 'neovm--gt-find-dominant-p2 mp 2 2))) ;; nil
       ;; 3x3 game with dominant strategy for p1 but not p2
       (let ((g3 (vector (vector '(4 . 2) '(3 . 3) '(5 . 1))
                         (vector '(2 . 4) '(1 . 2) '(3 . 3))
                         (vector '(1 . 1) '(0 . 5) '(2 . 4)))))
         (list (funcall 'neovm--gt-find-dominant-p1 g3 3 3)
               (funcall 'neovm--gt-find-dominant-p2 g3 3 3)))
       ;; Does i=0 dominate i=1 in PD?
       (funcall 'neovm--gt-p1-dominates
                (vector (vector '(3 . 3) '(0 . 5))
                        (vector '(5 . 0) '(1 . 1)))
                0 1 2))
    (fmakunbound 'neovm--gt-p1-dominates)
    (fmakunbound 'neovm--gt-p2-dominates)
    (fmakunbound 'neovm--gt-find-dominant-p1)
    (fmakunbound 'neovm--gt-find-dominant-p2)))"#;
    let expect = expect_test::expect![[r#""OK ((1 1) (nil nil) (0 nil) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nash equilibrium finder for 2-player games (pure strategy)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_game_theory_nash_equilibrium() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Pure-strategy Nash equilibrium: strategy pair (i, j) where
  ;; i is p1's best response to j, and j is p2's best response to i.

  ;; Best response for p1 given p2 plays column j
  (fset 'neovm--gt-best-response-p1
    (lambda (matrix nrows j)
      (let ((best-i 0) (best-val (car (aref (aref matrix 0) j))))
        (dotimes (i nrows)
          (let ((val (car (aref (aref matrix i) j))))
            (when (> val best-val)
              (setq best-val val)
              (setq best-i i))))
        best-i)))

  ;; Best response for p2 given p1 plays row i
  (fset 'neovm--gt-best-response-p2
    (lambda (matrix ncols i)
      (let ((best-j 0) (best-val (cdr (aref (aref matrix i) 0))))
        (dotimes (j ncols)
          (let ((val (cdr (aref (aref matrix i) j))))
            (when (> val best-val)
              (setq best-val val)
              (setq best-j j))))
        best-j)))

  ;; Find all pure-strategy Nash equilibria
  (fset 'neovm--gt-find-nash
    (lambda (matrix nrows ncols)
      (let ((equilibria nil))
        (dotimes (i nrows)
          (dotimes (j ncols)
            (when (and (= i (funcall 'neovm--gt-best-response-p1 matrix nrows j))
                       (= j (funcall 'neovm--gt-best-response-p2 matrix ncols i)))
              (push (list i j (aref (aref matrix i) j)) equilibria))))
        (nreverse equilibria))))

  (unwind-protect
      (list
       ;; PD: one NE at (defect, defect) = (1,1)
       (funcall 'neovm--gt-find-nash
                (vector (vector '(3 . 3) '(0 . 5))
                        (vector '(5 . 0) '(1 . 1)))
                2 2)
       ;; Battle of the Sexes: two NE
       ;; Opera/Football:
       ;;              P2:Opera P2:Football
       ;; P1:Opera     (3.2)    (0.0)
       ;; P1:Football  (0.0)    (2.3)
       (funcall 'neovm--gt-find-nash
                (vector (vector '(3 . 2) '(0 . 0))
                        (vector '(0 . 0) '(2 . 3)))
                2 2)
       ;; Matching pennies: no pure-strategy NE
       (funcall 'neovm--gt-find-nash
                (vector (vector '(1 . -1) '(-1 . 1))
                        (vector '(-1 . 1) '(1 . -1)))
                2 2)
       ;; Coordination game: two NE on diagonal
       (funcall 'neovm--gt-find-nash
                (vector (vector '(2 . 2) '(0 . 0))
                        (vector '(0 . 0) '(1 . 1)))
                2 2)
       ;; Best responses
       (funcall 'neovm--gt-best-response-p1
                (vector (vector '(3 . 3) '(0 . 5))
                        (vector '(5 . 0) '(1 . 1)))
                2 0))
    (fmakunbound 'neovm--gt-best-response-p1)
    (fmakunbound 'neovm--gt-best-response-p2)
    (fmakunbound 'neovm--gt-find-nash)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 1 (1 . 1))) ((0 0 (3 . 2)) (1 1 (2 . 3))) nil ((0 0 (2 . 2)) (1 1 (1 . 1))) 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Iterated prisoner's dilemma with strategies
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_game_theory_iterated_pd() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Strategies for iterated PD:
  ;; 'always-c, 'always-d, 'tit-for-tat, 'grudger
  ;; Each strategy: (lambda (my-history opp-history) -> 'C or 'D)

  (fset 'neovm--ipd-always-c (lambda (my opp) 'C))
  (fset 'neovm--ipd-always-d (lambda (my opp) 'D))
  (fset 'neovm--ipd-tit-for-tat
    (lambda (my opp)
      (if (null opp) 'C (car opp))))   ;; cooperate first, then copy opponent
  (fset 'neovm--ipd-grudger
    (lambda (my opp)
      (if (memq 'D opp) 'D 'C)))       ;; cooperate until opponent defects once

  ;; Play one round: returns ((p1-payoff . p2-payoff) p1-move p2-move)
  (fset 'neovm--ipd-round-payoff
    (lambda (m1 m2)
      (cond
       ((and (eq m1 'C) (eq m2 'C)) '(3 . 3))
       ((and (eq m1 'C) (eq m2 'D)) '(0 . 5))
       ((and (eq m1 'D) (eq m2 'C)) '(5 . 0))
       (t '(1 . 1)))))

  ;; Play N rounds between two strategies
  (fset 'neovm--ipd-play
    (lambda (strat1 strat2 rounds)
      (let ((h1 nil) (h2 nil)
            (score1 0) (score2 0)
            (moves nil))
        (dotimes (_ rounds)
          (let* ((m1 (funcall strat1 h1 h2))
                 (m2 (funcall strat2 h2 h1))
                 (payoff (funcall 'neovm--ipd-round-payoff m1 m2)))
            (setq score1 (+ score1 (car payoff)))
            (setq score2 (+ score2 (cdr payoff)))
            (push m1 h1)
            (push m2 h2)
            (push (list m1 m2) moves)))
        (list score1 score2 (nreverse moves)))))

  (unwind-protect
      (list
       ;; Always-C vs Always-C: mutual cooperation
       (let ((r (funcall 'neovm--ipd-play 'neovm--ipd-always-c 'neovm--ipd-always-c 5)))
         (list (nth 0 r) (nth 1 r)))
       ;; Always-D vs Always-D: mutual defection
       (let ((r (funcall 'neovm--ipd-play 'neovm--ipd-always-d 'neovm--ipd-always-d 5)))
         (list (nth 0 r) (nth 1 r)))
       ;; Always-C vs Always-D: exploited
       (let ((r (funcall 'neovm--ipd-play 'neovm--ipd-always-c 'neovm--ipd-always-d 5)))
         (list (nth 0 r) (nth 1 r)))
       ;; Tit-for-tat vs Always-C: both cooperate forever
       (let ((r (funcall 'neovm--ipd-play 'neovm--ipd-tit-for-tat 'neovm--ipd-always-c 5)))
         (list (nth 0 r) (nth 1 r)))
       ;; Tit-for-tat vs Always-D: TFT cooperates first, then retaliates
       (let ((r (funcall 'neovm--ipd-play 'neovm--ipd-tit-for-tat 'neovm--ipd-always-d 5)))
         (list (nth 0 r) (nth 1 r) (nth 2 r)))
       ;; Tit-for-tat vs Tit-for-tat: eternal cooperation
       (let ((r (funcall 'neovm--ipd-play 'neovm--ipd-tit-for-tat 'neovm--ipd-tit-for-tat 5)))
         (list (nth 0 r) (nth 1 r)))
       ;; Grudger vs TFT: cooperate forever (TFT never defects first)
       (let ((r (funcall 'neovm--ipd-play 'neovm--ipd-grudger 'neovm--ipd-tit-for-tat 5)))
         (list (nth 0 r) (nth 1 r))))
    (fmakunbound 'neovm--ipd-always-c)
    (fmakunbound 'neovm--ipd-always-d)
    (fmakunbound 'neovm--ipd-tit-for-tat)
    (fmakunbound 'neovm--ipd-grudger)
    (fmakunbound 'neovm--ipd-round-payoff)
    (fmakunbound 'neovm--ipd-play)))"#;
    let expect = expect_test::expect![[
        r#""OK ((15 15) (5 5) (0 25) (15 15) (4 9 ((C D) (D D) (D D) (D D) (D D))) (15 15) (15 15))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Tournament simulation: round-robin between strategies
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_game_theory_tournament() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Tournament: each strategy plays every other strategy for N rounds.
  ;; Total scores determine ranking.

  (fset 'neovm--tour-always-c (lambda (my opp) 'C))
  (fset 'neovm--tour-always-d (lambda (my opp) 'D))
  (fset 'neovm--tour-tft (lambda (my opp) (if (null opp) 'C (car opp))))
  (fset 'neovm--tour-grudger (lambda (my opp) (if (memq 'D opp) 'D 'C)))

  (fset 'neovm--tour-payoff
    (lambda (m1 m2)
      (cond ((and (eq m1 'C) (eq m2 'C)) '(3 . 3))
            ((and (eq m1 'C) (eq m2 'D)) '(0 . 5))
            ((and (eq m1 'D) (eq m2 'C)) '(5 . 0))
            (t '(1 . 1)))))

  (fset 'neovm--tour-play
    (lambda (s1 s2 rounds)
      (let ((h1 nil) (h2 nil) (sc1 0) (sc2 0))
        (dotimes (_ rounds)
          (let* ((m1 (funcall s1 h1 h2))
                 (m2 (funcall s2 h2 h1))
                 (p (funcall 'neovm--tour-payoff m1 m2)))
            (setq sc1 (+ sc1 (car p)) sc2 (+ sc2 (cdr p)))
            (push m1 h1) (push m2 h2)))
        (cons sc1 sc2))))

  ;; Run tournament: returns sorted alist of (strategy-name . total-score)
  (fset 'neovm--tour-run
    (lambda (strategies rounds)
      (let ((scores (make-hash-table :test 'eq)))
        ;; Initialize scores
        (dolist (s strategies)
          (puthash (car s) 0 scores))
        ;; Round-robin
        (let ((strats strategies))
          (while strats
            (let ((rest (cdr strats))
                  (s1 (car strats)))
              (dolist (s2 rest)
                (let ((result (funcall 'neovm--tour-play (cdr s1) (cdr s2) rounds)))
                  (puthash (car s1) (+ (gethash (car s1) scores) (car result)) scores)
                  (puthash (car s2) (+ (gethash (car s2) scores) (cdr result)) scores))))
            (setq strats (cdr strats))))
        ;; Collect and sort by score descending
        (let ((result nil))
          (maphash (lambda (k v) (push (cons k v) result)) scores)
          (sort result (lambda (a b) (> (cdr a) (cdr b))))))))

  (unwind-protect
      (let ((strategies (list (cons 'always-c 'neovm--tour-always-c)
                              (cons 'always-d 'neovm--tour-always-d)
                              (cons 'tit-for-tat 'neovm--tour-tft)
                              (cons 'grudger 'neovm--tour-grudger))))
        (let ((ranking (funcall 'neovm--tour-run strategies 10)))
          (list
           ;; Full ranking
           ranking
           ;; Winner (first in sorted list)
           (caar ranking)
           ;; Scores only
           (mapcar #'cdr ranking)
           ;; TFT vs Always-D specific matchup
           (funcall 'neovm--tour-play 'neovm--tour-tft 'neovm--tour-always-d 10)
           ;; Number of participants
           (length ranking))))
    (fmakunbound 'neovm--tour-always-c)
    (fmakunbound 'neovm--tour-always-d)
    (fmakunbound 'neovm--tour-tft)
    (fmakunbound 'neovm--tour-grudger)
    (fmakunbound 'neovm--tour-payoff)
    (fmakunbound 'neovm--tour-play)
    (fmakunbound 'neovm--tour-run)))"#;
    let expect = expect_test::expect![[
        r#""OK (((always-d . 78) (grudger . 69) (tit-for-tat . 69) (always-c . 60)) always-d (78 69 69 60) (9 . 14) 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Minimax for zero-sum games
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_game_theory_zero_sum_minimax() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Zero-sum game: p2's payoff = -(p1's payoff).
  ;; Matrix stores only p1's payoffs.
  ;; Minimax theorem: max_i min_j a[i][j] = min_j max_i a[i][j] at saddle point.

  ;; maximin for p1: max over rows of (min of that row)
  (fset 'neovm--zs-maximin
    (lambda (matrix nrows ncols)
      (let ((best most-negative-fixnum)
            (best-row -1))
        (dotimes (i nrows)
          (let ((row-min most-positive-fixnum))
            (dotimes (j ncols)
              (let ((v (aref (aref matrix i) j)))
                (when (< v row-min) (setq row-min v))))
            (when (> row-min best)
              (setq best row-min)
              (setq best-row i))))
        (list best-row best))))

  ;; minimax for p2: min over cols of (max of that col)
  (fset 'neovm--zs-minimax
    (lambda (matrix nrows ncols)
      (let ((best most-positive-fixnum)
            (best-col -1))
        (dotimes (j ncols)
          (let ((col-max most-negative-fixnum))
            (dotimes (i nrows)
              (let ((v (aref (aref matrix i) j)))
                (when (> v col-max) (setq col-max v))))
            (when (< col-max best)
              (setq best col-max)
              (setq best-col j))))
        (list best-col best))))

  ;; Find saddle point: where maximin = minimax
  (fset 'neovm--zs-saddle-point
    (lambda (matrix nrows ncols)
      (let* ((mm (funcall 'neovm--zs-maximin matrix nrows ncols))
             (mn (funcall 'neovm--zs-minimax matrix nrows ncols)))
        (if (= (nth 1 mm) (nth 1 mn))
            (list 'saddle (nth 0 mm) (nth 0 mn) (nth 1 mm))
          (list 'no-saddle (nth 1 mm) (nth 1 mn))))))

  (unwind-protect
      (list
       ;; Game with saddle point:
       ;; 3  2  4
       ;; 1  4  6
       ;; 5  3  2
       ;; Row mins: 2, 1, 2; maximin = 2 (row 0 or 2)
       ;; Col maxs: 5, 4, 6; minimax = 4 (col 1)
       ;; No saddle point (2 != 4)
       (funcall 'neovm--zs-saddle-point
                (vector (vector 3 2 4) (vector 1 4 6) (vector 5 3 2))
                3 3)
       ;; Game with saddle point:
       ;; 1  2  3
       ;; 4  5  6
       ;; 7  8  9
       ;; Row mins: 1, 4, 7; maximin = 7 (row 2)
       ;; Col maxs: 7, 8, 9; minimax = 7 (col 0)
       ;; Saddle point at (2, 0) = 7
       (funcall 'neovm--zs-saddle-point
                (vector (vector 1 2 3) (vector 4 5 6) (vector 7 8 9))
                3 3)
       ;; 2x2 zero-sum matching pennies: no saddle point
       (funcall 'neovm--zs-saddle-point
                (vector (vector 1 -1) (vector -1 1))
                2 2)
       ;; Maximin and minimax separately
       (funcall 'neovm--zs-maximin
                (vector (vector 3 -1) (vector -2 4))
                2 2)
       (funcall 'neovm--zs-minimax
                (vector (vector 3 -1) (vector -2 4))
                2 2)
       ;; Simple 2x2 with saddle: row0 dominates
       (funcall 'neovm--zs-saddle-point
                (vector (vector 5 3) (vector 2 1))
                2 2))
    (fmakunbound 'neovm--zs-maximin)
    (fmakunbound 'neovm--zs-minimax)
    (fmakunbound 'neovm--zs-saddle-point)))"#;
    let expect = expect_test::expect![[
        r#""OK ((no-saddle 2 4) (saddle 2 0 7) (no-saddle -1 1) (0 -1) (0 3) (saddle 0 1 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Mixed strategy computation for 2x2 games
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_game_theory_mixed_strategy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; For a 2x2 zero-sum game with no saddle point, compute the mixed strategy.
  ;; For p1: probability of playing row 0 = (d - c) / (a - b - c + d)
  ;; where matrix is: a b
  ;;                   c d
  ;; Expected value = (ad - bc) / (a - b - c + d)

  (fset 'neovm--ms-compute
    (lambda (a b c d)
      (let ((denom (+ a (- b) (- c) d)))
        (if (= denom 0)
            'degenerate
          ;; Use integer arithmetic * 1000 to avoid float imprecision
          (let* ((p1-prob-num (* (- d c) 1000))
                 (p1-prob (/ p1-prob-num denom))
                 (p2-prob-num (* (- d b) 1000))
                 (p2-prob (/ p2-prob-num denom))
                 (ev-num (* (- (* a d) (* b c)) 1000))
                 (ev (/ ev-num denom)))
            (list p1-prob p2-prob ev))))))

  ;; Verify mixed strategy: expected payoff should be the same
  ;; regardless of opponent's pure strategy choice.
  (fset 'neovm--ms-verify
    (lambda (a b c d p1-prob)
      ;; p1 plays row0 with prob p1-prob/1000, row1 with (1000-p1-prob)/1000
      ;; Against col0: EV = p1*a + (1-p1)*c
      ;; Against col1: EV = p1*b + (1-p1)*d
      (let* ((ev-col0 (/ (+ (* p1-prob a) (* (- 1000 p1-prob) c)) 1000))
             (ev-col1 (/ (+ (* p1-prob b) (* (- 1000 p1-prob) d)) 1000)))
        (list ev-col0 ev-col1 (= ev-col0 ev-col1)))))

  (unwind-protect
      (list
       ;; Matching pennies: a=1, b=-1, c=-1, d=1
       ;; p1 = (1-(-1))/(1-(-1)-(-1)+1) = 2/4 = 500/1000
       (funcall 'neovm--ms-compute 1 -1 -1 1)
       ;; Verify matching pennies
       (let ((result (funcall 'neovm--ms-compute 1 -1 -1 1)))
         (funcall 'neovm--ms-verify 1 -1 -1 1 (nth 0 result)))
       ;; Asymmetric game: a=3, b=-1, c=-2, d=4
       (funcall 'neovm--ms-compute 3 -1 -2 4)
       ;; Game: a=2, b=0, c=0, d=2
       (funcall 'neovm--ms-compute 2 0 0 2)
       ;; Degenerate case: a=1, b=1, c=1, d=1 (denom = 0)
       (funcall 'neovm--ms-compute 1 1 1 1)
       ;; Game: a=4, b=1, c=2, d=3
       (funcall 'neovm--ms-compute 4 1 2 3))
    (fmakunbound 'neovm--ms-compute)
    (fmakunbound 'neovm--ms-verify)))"#;
    let expect = expect_test::expect![[
        r#""OK ((500 500 0) (0 0 t) (600 500 1000) (500 500 1000) degenerate (250 500 2500))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
