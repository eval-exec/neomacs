//! Oracle parity tests for consensus and voting algorithm patterns in Elisp.
//!
//! Tests Boyer-Moore majority vote, ranked choice / instant runoff voting,
//! Borda count scoring, plurality with elimination, approval voting,
//! and a complex multi-round election simulation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Boyer-Moore majority vote algorithm
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_consensus_majority_vote() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Boyer-Moore voting algorithm: find element appearing > n/2 times
  ;; Phase 1: find candidate, Phase 2: verify count
  (fset 'neovm--test-boyer-moore-candidate
    (lambda (votes)
      (let ((candidate nil)
            (count 0))
        (dolist (v votes)
          (cond
            ((= count 0)
             (setq candidate v)
             (setq count 1))
            ((equal v candidate)
             (setq count (1+ count)))
            (t
             (setq count (1- count)))))
        candidate)))

  (fset 'neovm--test-verify-majority
    (lambda (votes candidate)
      (let ((count 0)
            (total (length votes)))
        (dolist (v votes)
          (when (equal v candidate)
            (setq count (1+ count))))
        (if (> count (/ total 2))
            (list 'majority candidate count total)
          (list 'no-majority candidate count total)))))

  (fset 'neovm--test-majority-vote
    (lambda (votes)
      (if (null votes)
          '(empty)
        (let ((candidate (funcall 'neovm--test-boyer-moore-candidate votes)))
          (funcall 'neovm--test-verify-majority votes candidate)))))

  (unwind-protect
      (list
        ;; Clear majority
        (funcall 'neovm--test-majority-vote '(a a a b b))
        ;; No majority (3-way tie-ish)
        (funcall 'neovm--test-majority-vote '(a b c a b c a))
        ;; All same
        (funcall 'neovm--test-majority-vote '(x x x x x))
        ;; Single element
        (funcall 'neovm--test-majority-vote '(z))
        ;; Empty
        (funcall 'neovm--test-majority-vote nil)
        ;; Majority at exactly n/2+1
        (funcall 'neovm--test-majority-vote '(a b a b a))
        ;; Numeric votes
        (funcall 'neovm--test-majority-vote '(1 2 1 2 1 1 2 1 1))
        ;; String votes
        (funcall 'neovm--test-majority-vote '("yes" "no" "yes" "yes" "no" "yes")))
    (fmakunbound 'neovm--test-boyer-moore-candidate)
    (fmakunbound 'neovm--test-verify-majority)
    (fmakunbound 'neovm--test-majority-vote)))"#;
    let expect = expect_test::expect![[
        r#""OK ((majority a 3 5) (no-majority a 3 7) (majority x 5 5) (majority z 1 1) (empty) (majority a 3 5) (majority 1 6 9) (majority \"yes\" 4 6))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Ranked choice voting (instant runoff)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_consensus_ranked_choice() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Instant runoff: each ballot is a ranked list of candidates.
  ;; Repeatedly eliminate the candidate with fewest first-choice votes
  ;; until one has majority.

  (fset 'neovm--test-count-first-choices
    (lambda (ballots eliminated)
      ;; Count first non-eliminated choice on each ballot
      (let ((counts (make-hash-table :test 'eq)))
        (dolist (ballot ballots)
          (let ((choice nil)
                (remaining ballot))
            ;; Find first non-eliminated candidate
            (while (and remaining (not choice))
              (unless (memq (car remaining) eliminated)
                (setq choice (car remaining)))
              (setq remaining (cdr remaining)))
            (when choice
              (puthash choice (1+ (or (gethash choice counts) 0)) counts))))
        counts)))

  (fset 'neovm--test-find-loser
    (lambda (counts eliminated)
      ;; Find candidate with fewest votes (not eliminated)
      (let ((min-votes most-positive-fixnum)
            (loser nil))
        (maphash (lambda (candidate votes)
                   (unless (memq candidate eliminated)
                     (when (< votes min-votes)
                       (setq min-votes votes)
                       (setq loser candidate))))
                 counts)
        loser)))

  (fset 'neovm--test-instant-runoff
    (lambda (ballots candidates)
      (let ((eliminated nil)
            (rounds nil)
            (winner nil)
            (total-ballots (length ballots))
            (max-rounds 20)
            (round-num 0))
        (while (and (not winner) (< round-num max-rounds))
          (setq round-num (1+ round-num))
          (let* ((counts (funcall 'neovm--test-count-first-choices
                                  ballots eliminated))
                 (round-info nil))
            ;; Build round info
            (dolist (c candidates)
              (unless (memq c eliminated)
                (setq round-info
                      (cons (cons c (or (gethash c counts) 0))
                            round-info))))
            (setq round-info (sort round-info
                                   (lambda (a b) (> (cdr a) (cdr b)))))
            (setq rounds (cons (list :round round-num :counts round-info) rounds))
            ;; Check for majority
            (let ((top (cdar round-info)))
              (if (and top (> top (/ total-ballots 2)))
                  (setq winner (caar round-info))
                ;; Eliminate loser
                (let ((loser (funcall 'neovm--test-find-loser counts eliminated)))
                  (when loser
                    (setq eliminated (cons loser eliminated))))))))
        (list :winner winner
              :rounds (nreverse rounds)
              :eliminated eliminated))))

  (unwind-protect
      (let* ((candidates '(alice bob carol dave))
             ;; 9 ballots with ranked preferences
             (ballots '((alice bob carol dave)
                        (alice carol bob dave)
                        (bob alice carol dave)
                        (bob carol alice dave)
                        (carol dave alice bob)
                        (carol dave bob alice)
                        (dave carol bob alice)
                        (alice bob dave carol)
                        (dave carol alice bob))))
        (list
          (funcall 'neovm--test-instant-runoff ballots candidates)
          ;; Unanimous election
          (funcall 'neovm--test-instant-runoff
                   '((x) (x) (x)) '(x y))
          ;; Two-candidate race
          (funcall 'neovm--test-instant-runoff
                   '((a b) (b a) (a b) (a b) (b a)) '(a b))))
    (fmakunbound 'neovm--test-count-first-choices)
    (fmakunbound 'neovm--test-find-loser)
    (fmakunbound 'neovm--test-instant-runoff)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:winner carol :rounds ((:round 1 :counts ((alice . 3) (dave . 2) (carol . 2) (bob . 2))) (:round 2 :counts ((alice . 4) (carol . 3) (dave . 2))) (:round 3 :counts ((carol . 5) (alice . 4)))) :eliminated (dave bob)) (:winner x :rounds ((:round 1 :counts ((x . 3) (y . 0)))) :eliminated nil) (:winner a :rounds ((:round 1 :counts ((a . 3) (b . 2)))) :eliminated nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Borda count scoring
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_consensus_borda_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Borda count: for N candidates, first choice gets N-1 points,
  ;; second gets N-2, etc. Winner has highest total.

  (fset 'neovm--test-borda-count
    (lambda (ballots candidates)
      (let* ((n (length candidates))
             (scores (make-hash-table :test 'eq)))
        ;; Initialize all candidates to 0
        (dolist (c candidates)
          (puthash c 0 scores))
        ;; Score each ballot
        (dolist (ballot ballots)
          (let ((rank 0))
            (dolist (candidate ballot)
              (when (memq candidate candidates)
                (let ((points (- n 1 rank)))
                  (puthash candidate
                           (+ (gethash candidate scores) (max points 0))
                           scores))
                (setq rank (1+ rank))))))
        ;; Build sorted results
        (let ((results nil))
          (maphash (lambda (c s)
                     (setq results (cons (cons c s) results)))
                   scores)
          (setq results (sort results (lambda (a b)
                                        (if (= (cdr a) (cdr b))
                                            (string< (symbol-name (car a))
                                                     (symbol-name (car b)))
                                          (> (cdr a) (cdr b))))))
          (list :winner (caar results)
                :scores results
                :total-ballots (length ballots)
                :max-possible-score (* (length ballots) (1- n)))))))

  (unwind-protect
      (let ((candidates '(alpha beta gamma delta)))
        (list
          ;; Standard election
          (funcall 'neovm--test-borda-count
                   '((alpha beta gamma delta)
                     (beta alpha gamma delta)
                     (alpha gamma beta delta)
                     (gamma beta delta alpha)
                     (beta gamma alpha delta)
                     (alpha beta delta gamma)
                     (gamma alpha beta delta))
                   candidates)
          ;; Unanimous first choice
          (funcall 'neovm--test-borda-count
                   '((alpha beta gamma delta)
                     (alpha beta gamma delta)
                     (alpha beta gamma delta))
                   candidates)
          ;; Condorcet-loser can win Borda (compromise candidate)
          ;; All put "beta" second; beta never wins first-choice but
          ;; accumulates points
          (funcall 'neovm--test-borda-count
                   '((alpha beta gamma delta)
                     (gamma beta alpha delta)
                     (delta beta gamma alpha)
                     (alpha beta delta gamma)
                     (gamma beta delta alpha))
                   candidates)
          ;; Two candidates only
          (funcall 'neovm--test-borda-count
                   '((alpha beta) (beta alpha) (alpha beta))
                   '(alpha beta))))
    (fmakunbound 'neovm--test-borda-count)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:winner alpha :scores ((alpha . 14) (beta . 14) (gamma . 12) (delta . 2)) :total-ballots 7 :max-possible-score 21) (:winner alpha :scores ((alpha . 9) (beta . 6) (gamma . 3) (delta . 0)) :total-ballots 3 :max-possible-score 9) (:winner beta :scores ((beta . 10) (gamma . 8) (alpha . 7) (delta . 5)) :total-ballots 5 :max-possible-score 15) (:winner alpha :scores ((alpha . 2) (beta . 1)) :total-ballots 3 :max-possible-score 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Plurality with elimination
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_consensus_plurality_elimination() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Plurality: each voter picks ONE candidate. If no majority,
  ;; eliminate the candidate with fewest votes and re-vote.
  ;; Here we simulate with preference lists: each voter's top
  ;; non-eliminated choice counts.

  (fset 'neovm--test-tally-votes
    (lambda (ballots eliminated)
      (let ((counts (make-hash-table :test 'eq))
            (valid 0))
        (dolist (ballot ballots)
          ;; First non-eliminated candidate
          (let ((pick nil) (rest ballot))
            (while (and rest (not pick))
              (unless (memq (car rest) eliminated)
                (setq pick (car rest)))
              (setq rest (cdr rest)))
            (when pick
              (puthash pick (1+ (or (gethash pick counts) 0)) counts)
              (setq valid (1+ valid)))))
        (cons valid counts))))

  (fset 'neovm--test-plurality-elimination
    (lambda (ballots all-candidates)
      (let ((eliminated nil)
            (round-log nil)
            (done nil)
            (round-num 0))
        (while (and (not done) (< round-num 20))
          (setq round-num (1+ round-num))
          (let* ((tally (funcall 'neovm--test-tally-votes ballots eliminated))
                 (valid (car tally))
                 (counts (cdr tally))
                 (summary nil)
                 (min-votes most-positive-fixnum)
                 (min-cand nil)
                 (max-votes 0)
                 (max-cand nil))
            ;; Build summary and find min/max
            (dolist (c all-candidates)
              (unless (memq c eliminated)
                (let ((v (or (gethash c counts) 0)))
                  (setq summary (cons (cons c v) summary))
                  (when (< v min-votes)
                    (setq min-votes v)
                    (setq min-cand c))
                  (when (> v max-votes)
                    (setq max-votes v)
                    (setq max-cand c)))))
            (setq summary (sort summary (lambda (a b) (> (cdr a) (cdr b)))))
            (setq round-log (cons (list round-num summary) round-log))
            ;; Check for majority
            (if (> max-votes (/ valid 2))
                (setq done max-cand)
              ;; Eliminate lowest
              (when min-cand
                (setq eliminated (cons min-cand eliminated))
                ;; If only one left, they win
                (let ((remaining 0))
                  (dolist (c all-candidates)
                    (unless (memq c eliminated) (setq remaining (1+ remaining))))
                  (when (= remaining 1)
                    (dolist (c all-candidates)
                      (unless (memq c eliminated) (setq done c)))))))))
        (list :winner done
              :rounds (nreverse round-log)
              :eliminated (nreverse eliminated)))))

  (unwind-protect
      (let ((candidates '(ann ben cal dee)))
        (list
          ;; Competitive election
          (funcall 'neovm--test-plurality-elimination
                   '((ann ben cal dee)
                     (ann cal ben dee)
                     (ben ann cal dee)
                     (ben cal ann dee)
                     (cal dee ann ben)
                     (cal dee ben ann)
                     (dee cal ann ben)
                     (ann ben dee cal)
                     (ben ann dee cal)
                     (cal ann ben dee)
                     (dee ben cal ann))
                   candidates)
          ;; First-round majority
          (funcall 'neovm--test-plurality-elimination
                   '((ann) (ann) (ann) (ben) (cal))
                   '(ann ben cal))
          ;; All tied — eliminate alphabetically first with fewest
          (funcall 'neovm--test-plurality-elimination
                   '((ann ben) (ben ann))
                   '(ann ben))))
    (fmakunbound 'neovm--test-tally-votes)
    (fmakunbound 'neovm--test-plurality-elimination)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:winner ben :rounds ((1 ((cal . 3) (ben . 3) (ann . 3) (dee . 2))) (2 ((cal . 4) (ben . 4) (ann . 3))) (3 ((ben . 6) (cal . 5)))) :eliminated (dee ann)) (:winner ann :rounds ((1 ((ann . 3) (cal . 1) (ben . 1)))) :eliminated nil) (:winner ben :rounds ((1 ((ben . 1) (ann . 1)))) :eliminated (ann)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Approval voting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_consensus_approval_voting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Approval voting: each voter approves of zero or more candidates.
  ;; The candidate with the most approvals wins.

  (fset 'neovm--test-approval-vote
    (lambda (ballots candidates)
      (let ((approvals (make-hash-table :test 'eq))
            (voter-count (length ballots)))
        ;; Initialize
        (dolist (c candidates)
          (puthash c 0 approvals))
        ;; Count approvals
        (dolist (ballot ballots)
          (dolist (approved ballot)
            (when (memq approved candidates)
              (puthash approved (1+ (gethash approved approvals)) approvals))))
        ;; Build results
        (let ((results nil))
          (maphash (lambda (c v) (setq results (cons (cons c v) results)))
                   approvals)
          (setq results (sort results (lambda (a b)
                                        (if (= (cdr a) (cdr b))
                                            (string< (symbol-name (car a))
                                                     (symbol-name (car b)))
                                          (> (cdr a) (cdr b))))))
          ;; Compute approval percentages (as integer %)
          (let ((with-pct
                 (mapcar (lambda (r)
                           (list (car r) (cdr r)
                                 (if (> voter-count 0)
                                     (/ (* 100 (cdr r)) voter-count)
                                   0)))
                         results)))
            (list :winner (caar results)
                  :results with-pct
                  :voters voter-count
                  ;; How many candidates have majority approval?
                  :majority-approved
                  (let ((count 0))
                    (dolist (r results)
                      (when (> (cdr r) (/ voter-count 2))
                        (setq count (1+ count))))
                    count)))))))

  (unwind-protect
      (let ((candidates '(apple banana cherry date)))
        (list
          ;; Normal election: each voter approves some candidates
          (funcall 'neovm--test-approval-vote
                   '((apple banana)
                     (banana cherry)
                     (apple cherry date)
                     (banana date)
                     (apple banana cherry)
                     (cherry date)
                     (apple))
                   candidates)
          ;; Everyone approves everyone
          (funcall 'neovm--test-approval-vote
                   '((apple banana cherry date)
                     (apple banana cherry date)
                     (apple banana cherry date))
                   candidates)
          ;; Everyone approves exactly one (same as plurality)
          (funcall 'neovm--test-approval-vote
                   '((apple) (banana) (cherry) (apple) (banana))
                   candidates)
          ;; Empty ballots (no approvals)
          (funcall 'neovm--test-approval-vote
                   '(nil nil nil)
                   candidates)
          ;; Single voter approves all
          (funcall 'neovm--test-approval-vote
                   '((apple banana cherry date))
                   candidates)))
    (fmakunbound 'neovm--test-approval-vote)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:winner apple :results ((apple 4 57) (banana 4 57) (cherry 4 57) (date 3 42)) :voters 7 :majority-approved 3) (:winner apple :results ((apple 3 100) (banana 3 100) (cherry 3 100) (date 3 100)) :voters 3 :majority-approved 4) (:winner apple :results ((apple 2 40) (banana 2 40) (cherry 1 20) (date 0 0)) :voters 5 :majority-approved 0) (:winner apple :results ((apple 0 0) (banana 0 0) (cherry 0 0) (date 0 0)) :voters 3 :majority-approved 0) (:winner apple :results ((apple 1 100) (banana 1 100) (cherry 1 100) (date 1 100)) :voters 1 :majority-approved 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: election simulation with multiple voting methods compared
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_consensus_election_simulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Compare multiple voting methods on the same set of ballots
  ;; to show how different methods can produce different winners.

  ;; Plurality: just count first choices
  (fset 'neovm--test-sim-plurality
    (lambda (ballots candidates)
      (let ((counts (make-hash-table :test 'eq)))
        (dolist (c candidates) (puthash c 0 counts))
        (dolist (b ballots)
          (when (car b)
            (puthash (car b) (1+ (gethash (car b) counts)) counts)))
        (let ((best nil) (best-count -1))
          (maphash (lambda (c v)
                     (when (> v best-count)
                       (setq best c best-count v)))
                   counts)
          (cons best best-count)))))

  ;; Borda: N-1 for first, N-2 for second, etc.
  (fset 'neovm--test-sim-borda
    (lambda (ballots candidates)
      (let* ((n (length candidates))
             (scores (make-hash-table :test 'eq)))
        (dolist (c candidates) (puthash c 0 scores))
        (dolist (b ballots)
          (let ((rank 0))
            (dolist (c b)
              (when (memq c candidates)
                (puthash c (+ (gethash c scores) (max 0 (- n 1 rank))) scores)
                (setq rank (1+ rank))))))
        (let ((best nil) (best-score -1))
          (maphash (lambda (c s)
                     (when (> s best-score)
                       (setq best c best-score s)))
                   scores)
          (cons best best-score)))))

  ;; Condorcet: check if any candidate beats all others head-to-head
  (fset 'neovm--test-sim-condorcet
    (lambda (ballots candidates)
      (let ((pairwise (make-hash-table :test 'equal)))
        ;; For each pair, count who is preferred
        (dolist (b ballots)
          (let ((prefs b))
            (while prefs
              (let ((higher (car prefs))
                    (rest (cdr prefs)))
                (dolist (lower rest)
                  (when (and (memq higher candidates) (memq lower candidates))
                    (let ((key (list higher lower)))
                      (puthash key (1+ (or (gethash key pairwise) 0)) pairwise)))))
              (setq prefs (cdr prefs)))))
        ;; Find Condorcet winner
        (let ((total (length ballots))
              (winner nil))
          (dolist (c candidates)
            (let ((beats-all t))
              (dolist (other candidates)
                (unless (eq c other)
                  (let ((wins (or (gethash (list c other) pairwise) 0)))
                    (when (<= wins (/ total 2))
                      (setq beats-all nil)))))
              (when beats-all (setq winner c))))
          winner))))

  ;; Anti-plurality: each voter votes against their LAST choice
  ;; Candidate with fewest anti-votes wins
  (fset 'neovm--test-sim-anti-plurality
    (lambda (ballots candidates)
      (let ((anti-counts (make-hash-table :test 'eq)))
        (dolist (c candidates) (puthash c 0 anti-counts))
        (dolist (b ballots)
          (let ((last-choice (car (last b))))
            (when (memq last-choice candidates)
              (puthash last-choice (1+ (gethash last-choice anti-counts))
                       anti-counts))))
        (let ((best nil) (best-anti most-positive-fixnum))
          (maphash (lambda (c v)
                     (when (< v best-anti)
                       (setq best c best-anti v)))
                   anti-counts)
          (cons best best-anti)))))

  (unwind-protect
      (let* ((candidates '(w x y z))
             ;; Carefully constructed ballots where methods disagree:
             ;; w wins plurality, y wins Borda, x is Condorcet winner
             (ballots '((w x y z)  ; voter 1
                        (w x y z)  ; voter 2
                        (w x y z)  ; voter 3
                        (x y z w)  ; voter 4
                        (x y z w)  ; voter 5
                        (y x z w)  ; voter 6
                        (y x z w)  ; voter 7
                        (y z x w)  ; voter 8
                        (z y x w)  ; voter 9
                        ))
             (plur (funcall 'neovm--test-sim-plurality ballots candidates))
             (borda (funcall 'neovm--test-sim-borda ballots candidates))
             (condorcet (funcall 'neovm--test-sim-condorcet ballots candidates))
             (anti-plur (funcall 'neovm--test-sim-anti-plurality ballots candidates))
             ;; Check if all methods agree
             (all-agree (and (eq (car plur) (car borda))
                             (eq (car borda) condorcet)
                             (eq condorcet (car anti-plur))))
             ;; Count distinct winners
             (winners (let ((seen nil))
                        (dolist (w (list (car plur) (car borda)
                                         condorcet (car anti-plur)))
                          (unless (memq w seen)
                            (setq seen (cons w seen))))
                        seen)))
        (list
          (list :plurality plur)
          (list :borda borda)
          (list :condorcet condorcet)
          (list :anti-plurality anti-plur)
          :all-agree all-agree
          :distinct-winners (length winners)
          :winner-set (sort winners (lambda (a b)
                                      (string< (symbol-name a) (symbol-name b))))))
    (fmakunbound 'neovm--test-sim-plurality)
    (fmakunbound 'neovm--test-sim-borda)
    (fmakunbound 'neovm--test-sim-condorcet)
    (fmakunbound 'neovm--test-sim-anti-plurality)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:plurality (w . 3)) (:borda (x . 18)) (:condorcet x) (:anti-plurality (x . 0)) :all-agree nil :distinct-winners 2 :winner-set (w x))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
