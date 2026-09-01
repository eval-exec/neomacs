//! Advanced oracle parity tests for Petri net modeling:
//! weighted arcs, firing rules with sufficient token checks, transition firing,
//! enabled transitions enumeration, firing sequences, reachability analysis
//! (BFS over markings), deadlock detection, coverability checks,
//! P/T-invariant verification (conservation laws), and conflict detection
//! (shared input places between transitions).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Infrastructure: shared Petri net helpers used across tests
// The functions are defined inline to avoid cross-test state issues.
// ---------------------------------------------------------------------------

/// Helper: returns the standard Petri net function definitions as a string prefix.
fn pn_defs() -> &'static str {
    r#"
  (fset 'neovm--pna-enabled-p
    (lambda (marking transition)
      (let ((inputs (nth 1 transition)) (ok t))
        (dolist (arc inputs ok)
          (when (< (or (cdr (assq (car arc) marking)) 0) (cdr arc))
            (setq ok nil))))))

  (fset 'neovm--pna-fire
    (lambda (marking transition)
      (let ((new-m (copy-sequence marking)))
        (dolist (arc (nth 1 transition))
          (let ((entry (assq (car arc) new-m)))
            (if entry (setcdr entry (- (cdr entry) (cdr arc)))
              (push (cons (car arc) (- (cdr arc))) new-m))))
        (dolist (arc (nth 2 transition))
          (let ((entry (assq (car arc) new-m)))
            (if entry (setcdr entry (+ (cdr entry) (cdr arc)))
              (push (cons (car arc) (cdr arc)) new-m))))
        new-m)))

  (fset 'neovm--pna-tokens
    (lambda (marking place) (or (cdr (assq place marking)) 0)))

  (fset 'neovm--pna-enabled-list
    (lambda (marking transitions)
      (let ((result nil))
        (dolist (tr transitions)
          (when (funcall 'neovm--pna-enabled-p marking tr)
            (push tr result)))
        (nreverse result))))

  (fset 'neovm--pna-deadlocked-p
    (lambda (marking transitions)
      (null (funcall 'neovm--pna-enabled-list marking transitions))))
"#
}

fn pn_cleanup() -> &'static str {
    r#"
    (fmakunbound 'neovm--pna-enabled-p)
    (fmakunbound 'neovm--pna-fire)
    (fmakunbound 'neovm--pna-tokens)
    (fmakunbound 'neovm--pna-enabled-list)
    (fmakunbound 'neovm--pna-deadlocked-p)
"#
}

// ---------------------------------------------------------------------------
// Firing sequences: deterministic trace of multiple firings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_petri_net_adv_firing_sequence_deterministic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {defs}
  (unwind-protect
      (let* ((marking '((p1 . 5) (p2 . 0) (p3 . 0) (p4 . 0)))
             (t1 '(t1 ((p1 . 1)) ((p2 . 1))))
             (t2 '(t2 ((p2 . 2)) ((p3 . 1))))
             (t3 '(t3 ((p3 . 1) (p1 . 1)) ((p4 . 2))))
             (transitions (list t1 t2 t3))
             (trace nil)
             (state marking))
        ;; Fire t1 four times
        (dotimes (_ 4)
          (setq state (funcall 'neovm--pna-fire state t1))
          (push (list :t1 (funcall 'neovm--pna-tokens state 'p1)
                      (funcall 'neovm--pna-tokens state 'p2)) trace))
        ;; Fire t2 twice (needs 2 tokens from p2 each time)
        (dotimes (_ 2)
          (setq state (funcall 'neovm--pna-fire state t2))
          (push (list :t2 (funcall 'neovm--pna-tokens state 'p2)
                      (funcall 'neovm--pna-tokens state 'p3)) trace))
        ;; Fire t3 once (needs p3=1 and p1=1)
        (setq state (funcall 'neovm--pna-fire state t3))
        (push (list :t3 (funcall 'neovm--pna-tokens state 'p3)
                    (funcall 'neovm--pna-tokens state 'p4)
                    (funcall 'neovm--pna-tokens state 'p1)) trace)
        (list :trace (nreverse trace)
              :final-tokens (list (funcall 'neovm--pna-tokens state 'p1)
                                  (funcall 'neovm--pna-tokens state 'p2)
                                  (funcall 'neovm--pna-tokens state 'p3)
                                  (funcall 'neovm--pna-tokens state 'p4))
              :enabled (mapcar #'car (funcall 'neovm--pna-enabled-list state transitions))))
    {cleanup}))"#,
        defs = pn_defs(),
        cleanup = pn_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Reachability analysis: BFS over marking space
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_petri_net_adv_reachability_bfs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {defs}
  ;; Marking equality: sort alist by car before comparing
  (fset 'neovm--pna-marking-equal
    (lambda (m1 m2)
      (let ((s1 (sort (copy-sequence m1) (lambda (a b) (string< (symbol-name (car a)) (symbol-name (car b))))))
            (s2 (sort (copy-sequence m2) (lambda (a b) (string< (symbol-name (car a)) (symbol-name (car b)))))))
        (equal s1 s2))))

  ;; BFS reachability: explore all markings up to a bound
  (fset 'neovm--pna-reachability-bfs
    (lambda (initial transitions max-states)
      (let ((visited (list initial))
            (queue (list initial))
            (count 0))
        (while (and queue (< count max-states))
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (setq count (1+ count))
            (dolist (tr transitions)
              (when (funcall 'neovm--pna-enabled-p current tr)
                (let ((next (funcall 'neovm--pna-fire current tr))
                      (found nil))
                  (dolist (v visited)
                    (when (funcall 'neovm--pna-marking-equal v next)
                      (setq found t)))
                  (unless found
                    (push next visited)
                    (setq queue (append queue (list next)))))))))
        (list :states-explored count
              :unique-markings (length visited)))))

  (unwind-protect
      (let ((results nil))
        ;; Simple cyclic: p1 <-> p2 (only 2 reachable markings)
        (let* ((m '((p1 . 1) (p2 . 0)))
               (t1 '(t1 ((p1 . 1)) ((p2 . 1))))
               (t2 '(t2 ((p2 . 1)) ((p1 . 1)))))
          (push (list :cyclic (funcall 'neovm--pna-reachability-bfs m (list t1 t2) 50))
                results))

        ;; Linear: p1 -> p2 -> p3 (3 markings)
        (let* ((m '((p1 . 1) (p2 . 0) (p3 . 0)))
               (t1 '(t1 ((p1 . 1)) ((p2 . 1))))
               (t2 '(t2 ((p2 . 1)) ((p3 . 1)))))
          (push (list :linear (funcall 'neovm--pna-reachability-bfs m (list t1 t2) 50))
                results))

        ;; Fork-join: start -> (left, right) -> end
        (let* ((m '((start . 1) (left . 0) (right . 0) (end . 0)))
               (fork '(fork ((start . 1)) ((left . 1) (right . 1))))
               (join '(join ((left . 1) (right . 1)) ((end . 1)))))
          (push (list :fork-join (funcall 'neovm--pna-reachability-bfs m (list fork join) 50))
                results))

        ;; Choice: p1 -> p2 OR p1 -> p3 (diverging)
        (let* ((m '((p1 . 1) (p2 . 0) (p3 . 0)))
               (t1 '(t1 ((p1 . 1)) ((p2 . 1))))
               (t2 '(t2 ((p1 . 1)) ((p3 . 1)))))
          (push (list :choice (funcall 'neovm--pna-reachability-bfs m (list t1 t2) 50))
                results))

        (nreverse results))
    (fmakunbound 'neovm--pna-marking-equal)
    (fmakunbound 'neovm--pna-reachability-bfs)
    {cleanup}))"#,
        defs = pn_defs(),
        cleanup = pn_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Deadlock detection across various net topologies
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_petri_net_adv_deadlock_detection_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {defs}
  ;; Run simulation and check for deadlock at each step
  (fset 'neovm--pna-detect-deadlock-trace
    (lambda (marking transitions max-steps)
      (let ((state marking) (steps 0) (deadlocked nil) (trace nil))
        (while (and (< steps max-steps) (not deadlocked))
          (let ((enabled (funcall 'neovm--pna-enabled-list state transitions)))
            (if (null enabled)
                (setq deadlocked t)
              (progn
                (setq state (funcall 'neovm--pna-fire state (car enabled)))
                (push (car (car enabled)) trace)
                (setq steps (1+ steps))))))
        (list :deadlocked deadlocked :steps steps :trace (nreverse trace)))))

  (unwind-protect
      (let ((results nil))
        ;; Absorbing net: all tokens consumed, guaranteed deadlock
        (let* ((m '((p1 . 3)))
               (t1 '(t1 ((p1 . 1)) ())))
          (push (list :absorbing (funcall 'neovm--pna-detect-deadlock-trace m (list t1) 20))
                results))

        ;; Cyclic net: never deadlocks
        (let* ((m '((p1 . 1) (p2 . 0)))
               (t1 '(t1 ((p1 . 1)) ((p2 . 1))))
               (t2 '(t2 ((p2 . 1)) ((p1 . 1)))))
          (push (list :cyclic (funcall 'neovm--pna-detect-deadlock-trace m (list t1 t2) 10))
                results))

        ;; Draining pipeline: source -> buffer -> sink (deadlocks when source empty)
        (let* ((m '((source . 2) (buffer . 0) (sink . 0)))
               (produce '(produce ((source . 1)) ((buffer . 1))))
               (consume '(consume ((buffer . 1)) ((sink . 1)))))
          (push (list :pipeline (funcall 'neovm--pna-detect-deadlock-trace
                                          m (list produce consume) 20))
                results))

        ;; Mutual exclusion: never deadlocks with proper protocol
        (let* ((m '((idle1 . 1) (mutex . 1) (crit1 . 0)))
               (enter '(enter ((idle1 . 1) (mutex . 1)) ((crit1 . 1))))
               (leave '(leave ((crit1 . 1)) ((idle1 . 1) (mutex . 1)))))
          (push (list :mutex (funcall 'neovm--pna-detect-deadlock-trace
                                       m (list enter leave) 10))
                results))

        ;; Starvation net: two consumers, limited tokens
        (let* ((m '((pool . 1) (a-waiting . 1) (b-waiting . 1) (a-done . 0) (b-done . 0)))
               (a-takes '(a-takes ((a-waiting . 1) (pool . 1)) ((a-done . 1))))
               (b-takes '(b-takes ((b-waiting . 1) (pool . 1)) ((b-done . 1)))))
          (push (list :starvation (funcall 'neovm--pna-detect-deadlock-trace
                                            m (list a-takes b-takes) 10))
                results))

        (nreverse results))
    (fmakunbound 'neovm--pna-detect-deadlock-trace)
    {cleanup}))"#,
        defs = pn_defs(),
        cleanup = pn_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// P-invariant (place invariant) verification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_petri_net_adv_p_invariant_verification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  (require 'cl-lib)
  {defs}
  ;; Compute weighted sum for a place invariant
  (fset 'neovm--pna-invariant-sum
    (lambda (marking weights)
      (let ((sum 0))
        (dolist (w weights sum)
          (setq sum (+ sum (* (cdr w) (funcall 'neovm--pna-tokens marking (car w)))))))))

  ;; Verify invariant holds after a sequence of firings
  (fset 'neovm--pna-verify-invariant
    (lambda (marking transitions firing-seq weights)
      (let ((state marking)
            (initial-sum (funcall 'neovm--pna-invariant-sum marking weights))
            (all-ok t)
            (sums (list)))
        (dolist (tr-name firing-seq)
          (let ((tr (car (cl-remove-if-not
                           (lambda (t) (eq (car t) tr-name))
                           transitions))))
            (when tr
              (setq state (funcall 'neovm--pna-fire state tr))
              (let ((s (funcall 'neovm--pna-invariant-sum state weights)))
                (push s sums)
                (unless (= s initial-sum)
                  (setq all-ok nil))))))
        (list :initial-sum initial-sum
              :sums (nreverse sums)
              :invariant-holds all-ok))))

  (unwind-protect
      (let ((results nil))
        ;; Pipeline conservation: tokens(p1) + tokens(p2) + tokens(p3) = constant
        (let* ((m '((p1 . 5) (p2 . 0) (p3 . 0)))
               (t1 '(t1 ((p1 . 1)) ((p2 . 1))))
               (t2 '(t2 ((p2 . 1)) ((p3 . 1))))
               (transitions (list t1 t2))
               (weights '((p1 . 1) (p2 . 1) (p3 . 1))))
          (push (list :pipeline-conservation
                      (funcall 'neovm--pna-verify-invariant
                               m transitions '(t1 t1 t1 t2 t2 t1 t2) weights))
                results))

        ;; Mutex invariant: mutex + crit1 + crit2 = 1
        (let* ((m '((idle1 . 1) (idle2 . 1) (mutex . 1) (crit1 . 0) (crit2 . 0)))
               (e1 '(e1 ((idle1 . 1) (mutex . 1)) ((crit1 . 1))))
               (l1 '(l1 ((crit1 . 1)) ((idle1 . 1) (mutex . 1))))
               (e2 '(e2 ((idle2 . 1) (mutex . 1)) ((crit2 . 1))))
               (l2 '(l2 ((crit2 . 1)) ((idle2 . 1) (mutex . 1))))
               (transitions (list e1 l1 e2 l2))
               (weights '((mutex . 1) (crit1 . 1) (crit2 . 1))))
          (push (list :mutex-invariant
                      (funcall 'neovm--pna-verify-invariant
                               m transitions '(e1 l1 e2 l2 e1 l1) weights))
                results))

        ;; Weighted invariant: 2*p1 + p2 = constant (when t: p1 -> 2*p2)
        (let* ((m '((p1 . 3) (p2 . 0)))
               (t1 '(t1 ((p1 . 1)) ((p2 . 2))))
               (t2 '(t2 ((p2 . 2)) ((p1 . 1))))
               (transitions (list t1 t2))
               (weights '((p1 . 2) (p2 . 1))))
          (push (list :weighted-invariant
                      (funcall 'neovm--pna-verify-invariant
                               m transitions '(t1 t1 t2 t1 t2 t2) weights))
                results))

        (nreverse results))
    (fmakunbound 'neovm--pna-invariant-sum)
    (fmakunbound 'neovm--pna-verify-invariant)
    {cleanup}))"#,
        defs = pn_defs(),
        cleanup = pn_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Conflict detection: transitions sharing input places
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_petri_net_adv_conflict_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {defs}
  ;; Detect conflicts: two transitions are in conflict if they share an input place
  ;; and both are enabled but cannot both fire (insufficient tokens)
  (fset 'neovm--pna-input-places
    (lambda (transition)
      (mapcar #'car (nth 1 transition))))

  (fset 'neovm--pna-shared-inputs
    (lambda (t1 t2)
      (let ((places1 (funcall 'neovm--pna-input-places t1))
            (shared nil))
        (dolist (p places1)
          (when (memq p (funcall 'neovm--pna-input-places t2))
            (push p shared)))
        (nreverse shared))))

  (fset 'neovm--pna-in-conflict-p
    (lambda (marking t1 t2)
      "Return t if t1 and t2 are both enabled but cannot both fire."
      (and (funcall 'neovm--pna-enabled-p marking t1)
           (funcall 'neovm--pna-enabled-p marking t2)
           (let ((shared (funcall 'neovm--pna-shared-inputs t1 t2))
                 (conflict nil))
             ;; Check if shared places have enough tokens for both
             (dolist (p shared)
               (let ((available (funcall 'neovm--pna-tokens marking p))
                     (needed1 (or (cdr (assq p (nth 1 t1))) 0))
                     (needed2 (or (cdr (assq p (nth 1 t2))) 0)))
                 (when (< available (+ needed1 needed2))
                   (setq conflict t))))
             conflict))))

  ;; Find all conflicting pairs among enabled transitions
  (fset 'neovm--pna-find-conflicts
    (lambda (marking transitions)
      (let ((conflicts nil)
            (enabled (funcall 'neovm--pna-enabled-list marking transitions)))
        (let ((i 0))
          (dolist (t1 enabled)
            (let ((j 0))
              (dolist (t2 enabled)
                (when (and (> j i)
                           (funcall 'neovm--pna-in-conflict-p marking t1 t2))
                  (push (list (car t1) (car t2)
                              (funcall 'neovm--pna-shared-inputs t1 t2))
                        conflicts))
                (setq j (1+ j))))
            (setq i (1+ i))))
        (nreverse conflicts))))

  (unwind-protect
      (let ((results nil))
        ;; No conflict: independent transitions
        (let* ((m '((p1 . 1) (p2 . 1) (p3 . 0) (p4 . 0)))
               (t1 '(t1 ((p1 . 1)) ((p3 . 1))))
               (t2 '(t2 ((p2 . 1)) ((p4 . 1)))))
          (push (list :independent
                      :conflicts (funcall 'neovm--pna-find-conflicts m (list t1 t2))
                      :both-enabled (and (funcall 'neovm--pna-enabled-p m t1)
                                         (funcall 'neovm--pna-enabled-p m t2)))
                results))

        ;; Conflict: two transitions need same place, insufficient tokens
        (let* ((m '((resource . 1) (req-a . 1) (req-b . 1) (done-a . 0) (done-b . 0)))
               (use-a '(use-a ((resource . 1) (req-a . 1)) ((done-a . 1))))
               (use-b '(use-b ((resource . 1) (req-b . 1)) ((done-b . 1)))))
          (push (list :resource-conflict
                      :conflicts (funcall 'neovm--pna-find-conflicts m (list use-a use-b))
                      :shared (funcall 'neovm--pna-shared-inputs use-a use-b))
                results))

        ;; No conflict when enough tokens for both
        (let* ((m '((resource . 2) (req-a . 1) (req-b . 1) (done-a . 0) (done-b . 0)))
               (use-a '(use-a ((resource . 1) (req-a . 1)) ((done-a . 1))))
               (use-b '(use-b ((resource . 1) (req-b . 1)) ((done-b . 1)))))
          (push (list :no-conflict-enough-tokens
                      :conflicts (funcall 'neovm--pna-find-conflicts m (list use-a use-b)))
                results))

        ;; Three-way conflict
        (let* ((m '((pool . 1) (a . 1) (b . 1) (c . 1)))
               (ta '(ta ((pool . 1) (a . 1)) ((a . 1))))
               (tb '(tb ((pool . 1) (b . 1)) ((b . 1))))
               (tc '(tc ((pool . 1) (c . 1)) ((c . 1)))))
          (push (list :three-way
                      :conflicts (funcall 'neovm--pna-find-conflicts m (list ta tb tc)))
                results))

        (nreverse results))
    (fmakunbound 'neovm--pna-input-places)
    (fmakunbound 'neovm--pna-shared-inputs)
    (fmakunbound 'neovm--pna-in-conflict-p)
    (fmakunbound 'neovm--pna-find-conflicts)
    {cleanup}))"#,
        defs = pn_defs(),
        cleanup = pn_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Coverability check: can a target marking be covered?
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_petri_net_adv_coverability_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {defs}
  ;; Check if marking m1 covers m2 (every place in m2 has >= tokens in m1)
  (fset 'neovm--pna-covers-p
    (lambda (m1 m2)
      (let ((ok t))
        (dolist (entry m2 ok)
          (when (< (funcall 'neovm--pna-tokens m1 (car entry)) (cdr entry))
            (setq ok nil))))))

  ;; BFS coverability: can we reach a marking that covers the target?
  (fset 'neovm--pna-coverable-p
    (lambda (initial transitions target max-states)
      (let ((queue (list initial))
            (visited 0)
            (found nil))
        (while (and queue (not found) (< visited max-states))
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (setq visited (1+ visited))
            (when (funcall 'neovm--pna-covers-p current target)
              (setq found t))
            (unless found
              (dolist (tr transitions)
                (when (funcall 'neovm--pna-enabled-p current tr)
                  (setq queue (append queue
                                       (list (funcall 'neovm--pna-fire current tr)))))))))
        (list :coverable found :explored visited))))

  (unwind-protect
      (let ((results nil))
        ;; Can reach: p3 >= 2 via pipeline
        (let* ((m '((p1 . 3) (p2 . 0) (p3 . 0)))
               (t1 '(t1 ((p1 . 1)) ((p2 . 1))))
               (t2 '(t2 ((p2 . 1)) ((p3 . 1))))
               (target '((p3 . 2))))
          (push (list :reachable-p3>=2
                      (funcall 'neovm--pna-coverable-p m (list t1 t2) target 100))
                results))

        ;; Cannot reach: p3 >= 5 (only 3 tokens total)
        (let* ((m '((p1 . 3) (p2 . 0) (p3 . 0)))
               (t1 '(t1 ((p1 . 1)) ((p2 . 1))))
               (t2 '(t2 ((p2 . 1)) ((p3 . 1))))
               (target '((p3 . 5))))
          (push (list :unreachable-p3>=5
                      (funcall 'neovm--pna-coverable-p m (list t1 t2) target 100))
                results))

        ;; Token multiplication: t: p1 -> 2*p2, so p2 can exceed initial tokens
        (let* ((m '((p1 . 3) (p2 . 0)))
               (t1 '(t1 ((p1 . 1)) ((p2 . 2))))
               (target '((p2 . 5))))
          (push (list :multiplication-reachable
                      (funcall 'neovm--pna-coverable-p m (list t1) target 50))
                results))

        ;; Trivially coverable: initial marking already covers
        (let* ((m '((p1 . 5) (p2 . 3)))
               (t1 '(t1 ((p1 . 1)) ((p2 . 1))))
               (target '((p1 . 2) (p2 . 1))))
          (push (list :initial-covers
                      (funcall 'neovm--pna-coverable-p m (list t1) target 10))
                results))

        (nreverse results))
    (fmakunbound 'neovm--pna-covers-p)
    (fmakunbound 'neovm--pna-coverable-p)
    {cleanup}))"#,
        defs = pn_defs(),
        cleanup = pn_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Bounded net analysis: check if all reachable markings stay within bounds
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_petri_net_adv_boundedness_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {defs}
  ;; Check if a net is k-bounded: no place ever exceeds k tokens
  (fset 'neovm--pna-check-bounded
    (lambda (initial transitions max-states bound)
      (let ((queue (list initial))
            (visited 0)
            (max-tokens 0)
            (exceeded nil)
            (place-maxes nil))
        (while (and queue (not exceeded) (< visited max-states))
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (setq visited (1+ visited))
            ;; Check all places in current marking
            (dolist (entry current)
              (let ((tokens (cdr entry))
                    (existing (assq (car entry) place-maxes)))
                (if existing
                    (when (> tokens (cdr existing))
                      (setcdr existing tokens))
                  (push (cons (car entry) tokens) place-maxes))
                (when (> tokens max-tokens)
                  (setq max-tokens tokens))
                (when (> tokens bound)
                  (setq exceeded t))))
            (unless exceeded
              (dolist (tr transitions)
                (when (funcall 'neovm--pna-enabled-p current tr)
                  (setq queue (append queue
                                       (list (funcall 'neovm--pna-fire current tr)))))))))
        (list :bounded (not exceeded)
              :max-tokens max-tokens
              :explored visited
              :place-maxes place-maxes))))

  (unwind-protect
      (let ((results nil))
        ;; Conservative net: token count constant, always 1-bounded
        (let* ((m '((p1 . 1) (p2 . 0)))
               (t1 '(t1 ((p1 . 1)) ((p2 . 1))))
               (t2 '(t2 ((p2 . 1)) ((p1 . 1)))))
          (push (list :conservative-1-bounded
                      (funcall 'neovm--pna-check-bounded m (list t1 t2) 20 1))
                results))

        ;; Pipeline with bounded buffer (3 tokens total)
        (let* ((m '((src . 3) (buf . 0) (sink . 0)))
               (t1 '(t1 ((src . 1)) ((buf . 1))))
               (t2 '(t2 ((buf . 1)) ((sink . 1)))))
          (push (list :pipeline-3-bounded
                      (funcall 'neovm--pna-check-bounded m (list t1 t2) 50 3))
                results))

        ;; Token doubler: NOT bounded (p2 grows without limit)
        ;; 1 token in p1, transition makes 2 tokens in p2, then p2->p1
        (let* ((m '((p1 . 1) (p2 . 0)))
               (t1 '(t1 ((p1 . 1)) ((p2 . 2))))
               (t2 '(t2 ((p2 . 1)) ((p1 . 1)))))
          (push (list :doubler-unbounded
                      (funcall 'neovm--pna-check-bounded m (list t1 t2) 30 5))
                results))

        (nreverse results))
    (fmakunbound 'neovm--pna-check-bounded)
    {cleanup}))"#,
        defs = pn_defs(),
        cleanup = pn_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Multi-token weighted transitions: chemical reaction network
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_petri_net_adv_chemical_reaction_network() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {defs}
  (unwind-protect
      (let ((results nil))
        ;; Reaction 1: 2H2 + O2 -> 2H2O
        ;; Reaction 2: C + O2 -> CO2
        ;; Reaction 3: 2CO2 + energy -> 2CO + O2 (reverse, needs energy)
        (let* ((marking '((H2 . 8) (O2 . 5) (H2O . 0) (C . 3) (CO2 . 0) (CO . 0) (energy . 2)))
               (r1 '(r1 ((H2 . 2) (O2 . 1)) ((H2O . 2))))
               (r2 '(r2 ((C . 1) (O2 . 1)) ((CO2 . 1))))
               (r3 '(r3 ((CO2 . 2) (energy . 1)) ((CO . 2) (O2 . 1))))
               (transitions (list r1 r2 r3))
               (state marking))

          ;; Run r1 until no more H2+O2
          (let ((r1-fires 0))
            (while (funcall 'neovm--pna-enabled-p state r1)
              (setq state (funcall 'neovm--pna-fire state r1))
              (setq r1-fires (1+ r1-fires)))
            (push (list :after-r1
                        :fires r1-fires
                        :H2 (funcall 'neovm--pna-tokens state 'H2)
                        :O2 (funcall 'neovm--pna-tokens state 'O2)
                        :H2O (funcall 'neovm--pna-tokens state 'H2O))
                  results))

          ;; Run r2 to burn carbon
          (let ((r2-fires 0))
            (while (funcall 'neovm--pna-enabled-p state r2)
              (setq state (funcall 'neovm--pna-fire state r2))
              (setq r2-fires (1+ r2-fires)))
            (push (list :after-r2
                        :fires r2-fires
                        :C (funcall 'neovm--pna-tokens state 'C)
                        :O2 (funcall 'neovm--pna-tokens state 'O2)
                        :CO2 (funcall 'neovm--pna-tokens state 'CO2))
                  results))

          ;; Try reverse reaction r3
          (let ((r3-fires 0))
            (while (funcall 'neovm--pna-enabled-p state r3)
              (setq state (funcall 'neovm--pna-fire state r3))
              (setq r3-fires (1+ r3-fires)))
            (push (list :after-r3
                        :fires r3-fires
                        :CO2 (funcall 'neovm--pna-tokens state 'CO2)
                        :CO (funcall 'neovm--pna-tokens state 'CO)
                        :O2 (funcall 'neovm--pna-tokens state 'O2)
                        :energy (funcall 'neovm--pna-tokens state 'energy))
                  results))

          ;; Mass conservation: total H atoms constant, total O atoms constant
          ;; H: 2*H2 + 2*H2O (initially 16)
          ;; O: 2*O2 + H2O + 2*CO2 + CO (initially 10)
          (let ((h-total (+ (* 2 (funcall 'neovm--pna-tokens state 'H2))
                            (* 2 (funcall 'neovm--pna-tokens state 'H2O))))
                (o-total (+ (* 2 (funcall 'neovm--pna-tokens state 'O2))
                            (funcall 'neovm--pna-tokens state 'H2O)
                            (* 2 (funcall 'neovm--pna-tokens state 'CO2))
                            (funcall 'neovm--pna-tokens state 'CO))))
            (push (list :conservation :H-total h-total :O-total o-total) results)))

        (nreverse results))
    {cleanup}))"#,
        defs = pn_defs(),
        cleanup = pn_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Liveness check: can every transition fire at least once from some reachable state?
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_petri_net_adv_liveness_analysis() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {defs}
  ;; Check which transitions can fire at least once from initial marking
  ;; by exploring reachable markings
  (fset 'neovm--pna-liveness-check
    (lambda (initial transitions max-states)
      (let ((queue (list initial))
            (visited 0)
            (fired-set nil))  ;; list of transition names that were enabled somewhere
        (while (and queue (< visited max-states))
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (setq visited (1+ visited))
            (dolist (tr transitions)
              (when (funcall 'neovm--pna-enabled-p current tr)
                (unless (memq (car tr) fired-set)
                  (push (car tr) fired-set))
                (setq queue (append queue
                                     (list (funcall 'neovm--pna-fire current tr))))))))
        (list :explored visited
              :live-transitions (sort (copy-sequence fired-set)
                                      (lambda (a b) (string< (symbol-name a) (symbol-name b))))
              :dead-transitions (let ((dead nil))
                                  (dolist (tr transitions)
                                    (unless (memq (car tr) fired-set)
                                      (push (car tr) dead)))
                                  (nreverse dead))))))

  (unwind-protect
      (let ((results nil))
        ;; All transitions live
        (let* ((m '((p1 . 1) (p2 . 0)))
               (t1 '(t1 ((p1 . 1)) ((p2 . 1))))
               (t2 '(t2 ((p2 . 1)) ((p1 . 1)))))
          (push (list :all-live
                      (funcall 'neovm--pna-liveness-check m (list t1 t2) 20))
                results))

        ;; One transition dead: never enough tokens
        (let* ((m '((p1 . 1) (p2 . 0)))
               (t1 '(t1 ((p1 . 1)) ((p2 . 1))))
               (t2 '(t2 ((p2 . 1)) ((p1 . 1))))
               (t3 '(t3 ((p1 . 3)) ((p2 . 1)))))  ;; needs 3 tokens in p1, only 1 circulates
          (push (list :one-dead
                      (funcall 'neovm--pna-liveness-check m (list t1 t2 t3) 20))
                results))

        ;; Pipeline: all transitions fire at least once
        (let* ((m '((src . 3) (buf . 0) (sink . 0)))
               (t1 '(t1 ((src . 1)) ((buf . 1))))
               (t2 '(t2 ((buf . 1)) ((sink . 1)))))
          (push (list :pipeline-live
                      (funcall 'neovm--pna-liveness-check m (list t1 t2) 30))
                results))

        (nreverse results))
    (fmakunbound 'neovm--pna-liveness-check)
    {cleanup}))"#,
        defs = pn_defs(),
        cleanup = pn_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Concurrent firing: interleaved execution of multiple enabled transitions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_petri_net_adv_concurrent_interleaving() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {defs}
  ;; Simulate with round-robin among enabled transitions
  (fset 'neovm--pna-round-robin-sim
    (lambda (marking transitions max-steps)
      (let ((state marking)
            (steps 0)
            (fired-counts nil)
            (trace nil))
        ;; Initialize counters
        (dolist (tr transitions)
          (push (cons (car tr) 0) fired-counts))
        (while (and (< steps max-steps)
                    (not (funcall 'neovm--pna-deadlocked-p state transitions)))
          ;; Pick the enabled transition with lowest fire count (round-robin fairness)
          (let ((best nil) (best-count most-positive-fixnum))
            (dolist (tr transitions)
              (when (funcall 'neovm--pna-enabled-p state tr)
                (let ((cnt (cdr (assq (car tr) fired-counts))))
                  (when (< cnt best-count)
                    (setq best tr)
                    (setq best-count cnt)))))
            (when best
              (setq state (funcall 'neovm--pna-fire state best))
              (let ((entry (assq (car best) fired-counts)))
                (setcdr entry (1+ (cdr entry))))
              (push (car best) trace)
              (setq steps (1+ steps)))))
        (list :steps steps
              :fired-counts fired-counts
              :trace (nreverse trace)
              :deadlocked (funcall 'neovm--pna-deadlocked-p state transitions)))))

  (unwind-protect
      (let ((results nil))
        ;; Two independent workers sharing a pool
        (let* ((m '((pool . 4) (w1-idle . 1) (w2-idle . 1) (done1 . 0) (done2 . 0)))
               (w1-work '(w1-work ((pool . 1) (w1-idle . 1)) ((done1 . 1) (w1-idle . 1))))
               (w2-work '(w2-work ((pool . 1) (w2-idle . 1)) ((done2 . 1) (w2-idle . 1)))))
          (push (list :fair-workers
                      (funcall 'neovm--pna-round-robin-sim m (list w1-work w2-work) 20))
                results))

        ;; Producer-consumer with fair scheduling
        (let* ((m '((raw . 3) (processed . 0) (shipped . 0)))
               (process '(process ((raw . 1)) ((processed . 1))))
               (ship '(ship ((processed . 1)) ((shipped . 1)))))
          (push (list :producer-consumer
                      (funcall 'neovm--pna-round-robin-sim m (list process ship) 20))
                results))

        (nreverse results))
    (fmakunbound 'neovm--pna-round-robin-sim)
    {cleanup}))"#,
        defs = pn_defs(),
        cleanup = pn_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// State space statistics: count enabled, firing options, branching factor
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_petri_net_adv_state_space_statistics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {defs}
  ;; Compute statistics about the state space
  (fset 'neovm--pna-state-stats
    (lambda (marking transitions)
      (let* ((enabled (funcall 'neovm--pna-enabled-list marking transitions))
             (num-enabled (length enabled))
             (successors nil))
        ;; Compute all successor markings
        (dolist (tr enabled)
          (push (funcall 'neovm--pna-fire marking tr) successors))
        ;; Compute total tokens in marking
        (let ((total-tokens 0))
          (dolist (entry marking)
            (setq total-tokens (+ total-tokens (max 0 (cdr entry)))))
          (list :num-enabled num-enabled
                :total-tokens total-tokens
                :transition-names (mapcar #'car enabled))))))

  (unwind-protect
      (let ((results nil))
        ;; Complex net with varying branching factor
        (let* ((m '((p1 . 3) (p2 . 2) (p3 . 0) (p4 . 0) (p5 . 0)))
               (t1 '(t1 ((p1 . 1)) ((p3 . 1))))
               (t2 '(t2 ((p1 . 1) (p2 . 1)) ((p4 . 1))))
               (t3 '(t3 ((p2 . 2)) ((p5 . 1))))
               (t4 '(t4 ((p3 . 1)) ((p1 . 1))))
               (transitions (list t1 t2 t3 t4))
               (state m))
          ;; Stats at initial state
          (push (list :initial (funcall 'neovm--pna-state-stats state transitions)) results)
          ;; Fire t1, check stats
          (setq state (funcall 'neovm--pna-fire state t1))
          (push (list :after-t1 (funcall 'neovm--pna-state-stats state transitions)) results)
          ;; Fire t2, check stats
          (setq state (funcall 'neovm--pna-fire state t2))
          (push (list :after-t2 (funcall 'neovm--pna-state-stats state transitions)) results)
          ;; Fire t3, check stats
          (setq state (funcall 'neovm--pna-fire (copy-sequence m) t3))
          (push (list :after-t3-from-init (funcall 'neovm--pna-state-stats state transitions)) results))

        (nreverse results))
    (fmakunbound 'neovm--pna-state-stats)
    {cleanup}))"#,
        defs = pn_defs(),
        cleanup = pn_cleanup()
    );
    assert_oracle_parity(&form);
}
