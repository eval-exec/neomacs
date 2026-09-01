//! Oracle parity tests for model checking patterns implemented in Elisp.
//!
//! Implements: Kripke structure representation, CTL formula evaluation
//! (EX, AX, EF, AF, EG, AG, EU, AU operators), state space exploration,
//! reachability analysis, safety property checking, liveness property
//! checking, and counterexample generation.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// Returns the Elisp preamble defining the model checker infrastructure.
/// A Kripke structure is represented as:
///   - states: list of state symbols
///   - transitions: alist mapping state -> list of successor states
///   - labels: alist mapping state -> list of atomic propositions
fn model_checker_preamble() -> &'static str {
    r#"
  ;; ================================================================
  ;; Kripke Structure & CTL Model Checker
  ;; ================================================================

  ;; Create a Kripke structure: (states transitions labels)
  (fset 'neovm--test-mc-make-kripke
    (lambda (states transitions labels)
      (list states transitions labels)))

  (fset 'neovm--test-mc-states (lambda (k) (nth 0 k)))
  (fset 'neovm--test-mc-transitions (lambda (k) (nth 1 k)))
  (fset 'neovm--test-mc-labels (lambda (k) (nth 2 k)))

  ;; Get successors of a state
  (fset 'neovm--test-mc-successors
    (lambda (k state)
      (cdr (assq state (funcall 'neovm--test-mc-transitions k)))))

  ;; Check if a state has an atomic proposition
  (fset 'neovm--test-mc-has-prop
    (lambda (k state prop)
      (memq prop (cdr (assq state (funcall 'neovm--test-mc-labels k))))))

  ;; Get all states satisfying an atomic proposition
  (fset 'neovm--test-mc-sat-atom
    (lambda (k prop)
      (let ((result nil))
        (dolist (s (funcall 'neovm--test-mc-states k))
          (when (funcall 'neovm--test-mc-has-prop k s prop)
            (setq result (cons s result))))
        (nreverse result))))

  ;; ================================================================
  ;; CTL Operators
  ;; ================================================================

  ;; EX phi: exists a successor satisfying phi
  ;; Returns list of states where EX phi holds
  (fset 'neovm--test-mc-EX
    (lambda (k phi-states)
      (let ((result nil))
        (dolist (s (funcall 'neovm--test-mc-states k))
          (let ((succs (funcall 'neovm--test-mc-successors k s))
                (found nil))
            (dolist (succ succs)
              (when (memq succ phi-states)
                (setq found t)))
            (when found
              (setq result (cons s result)))))
        (nreverse result))))

  ;; AX phi: all successors satisfy phi
  (fset 'neovm--test-mc-AX
    (lambda (k phi-states)
      (let ((result nil))
        (dolist (s (funcall 'neovm--test-mc-states k))
          (let ((succs (funcall 'neovm--test-mc-successors k s))
                (all-sat t))
            (if (null succs)
                ;; Deadlock state: AX vacuously true
                (setq all-sat t)
              (dolist (succ succs)
                (unless (memq succ phi-states)
                  (setq all-sat nil))))
            (when all-sat
              (setq result (cons s result)))))
        (nreverse result))))

  ;; EF phi: exists a path where phi eventually holds (reachability)
  ;; Fixed-point: EF phi = phi ∪ EX(EF phi)
  (fset 'neovm--test-mc-EF
    (lambda (k phi-states)
      (let ((current phi-states)
            (changed t))
        (while changed
          (setq changed nil)
          (let ((new-states (funcall 'neovm--test-mc-EX k current)))
            (dolist (s new-states)
              (unless (memq s current)
                (setq current (cons s current))
                (setq changed t)))))
        ;; Sort for deterministic output
        (sort current (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))

  ;; AF phi: on all paths, phi eventually holds
  ;; Fixed-point: AF phi = phi ∪ AX(AF phi)
  (fset 'neovm--test-mc-AF
    (lambda (k phi-states)
      (let ((current phi-states)
            (changed t))
        (while changed
          (setq changed nil)
          (let ((new-states (funcall 'neovm--test-mc-AX k current)))
            (dolist (s new-states)
              (unless (memq s current)
                (setq current (cons s current))
                (setq changed t)))))
        (sort current (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))

  ;; EG phi: exists a path where phi always holds
  ;; Fixed-point: EG phi = phi ∩ EX(EG phi)
  (fset 'neovm--test-mc-EG
    (lambda (k phi-states)
      (let ((current (copy-sequence phi-states))
            (changed t))
        (while changed
          (setq changed nil)
          (let ((new-current nil))
            (dolist (s current)
              (let ((succs (funcall 'neovm--test-mc-successors k s))
                    (has-good-succ nil))
                ;; Check if any successor is still in current set
                (dolist (succ succs)
                  (when (memq succ current)
                    (setq has-good-succ t)))
                (if has-good-succ
                    (setq new-current (cons s new-current))
                  (setq changed t))))
            (setq current (nreverse new-current))))
        (sort current (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))

  ;; AG phi: on all paths, phi always holds
  ;; AG phi = phi ∩ AX(AG phi)
  (fset 'neovm--test-mc-AG
    (lambda (k phi-states)
      (let ((current (copy-sequence phi-states))
            (changed t))
        (while changed
          (setq changed nil)
          (let ((new-current nil))
            (dolist (s current)
              (let ((succs (funcall 'neovm--test-mc-successors k s))
                    (all-good t))
                (dolist (succ succs)
                  (unless (memq succ current)
                    (setq all-good nil)))
                (if all-good
                    (setq new-current (cons s new-current))
                  (setq changed t))))
            (setq current (nreverse new-current))))
        (sort current (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))

  ;; EU(phi, psi): exists path where phi holds until psi holds
  ;; Fixed-point: EU(phi,psi) = psi ∪ (phi ∩ EX(EU(phi,psi)))
  (fset 'neovm--test-mc-EU
    (lambda (k phi-states psi-states)
      (let ((current (copy-sequence psi-states))
            (changed t))
        (while changed
          (setq changed nil)
          (let ((pre (funcall 'neovm--test-mc-EX k current)))
            (dolist (s pre)
              (when (and (memq s phi-states)
                         (not (memq s current)))
                (setq current (cons s current))
                (setq changed t)))))
        (sort current (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))

  ;; BFS reachability from initial states
  (fset 'neovm--test-mc-reachable
    (lambda (k initial-states)
      (let ((visited nil)
            (queue (copy-sequence initial-states)))
        (while queue
          (let ((s (car queue)))
            (setq queue (cdr queue))
            (unless (memq s visited)
              (setq visited (cons s visited))
              (dolist (succ (funcall 'neovm--test-mc-successors k s))
                (unless (memq succ visited)
                  (setq queue (append queue (list succ))))))))
        (sort visited (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))

  ;; Counterexample: find a path from initial to a bad state (BFS)
  (fset 'neovm--test-mc-find-path
    (lambda (k initial target)
      (let ((visited nil)
            (queue (list (list initial)))
            (found nil))
        (while (and queue (not found))
          (let ((path (car queue)))
            (setq queue (cdr queue))
            (let ((current (car (last path))))
              (if (eq current target)
                  (setq found path)
                (unless (memq current visited)
                  (setq visited (cons current visited))
                  (dolist (succ (funcall 'neovm--test-mc-successors k current))
                    (unless (memq succ visited)
                      (setq queue (append queue (list (append path (list succ))))))))))))
        found)))
"#
}

fn model_checker_cleanup() -> &'static str {
    r#"
    (fmakunbound 'neovm--test-mc-make-kripke)
    (fmakunbound 'neovm--test-mc-states)
    (fmakunbound 'neovm--test-mc-transitions)
    (fmakunbound 'neovm--test-mc-labels)
    (fmakunbound 'neovm--test-mc-successors)
    (fmakunbound 'neovm--test-mc-has-prop)
    (fmakunbound 'neovm--test-mc-sat-atom)
    (fmakunbound 'neovm--test-mc-EX)
    (fmakunbound 'neovm--test-mc-AX)
    (fmakunbound 'neovm--test-mc-EF)
    (fmakunbound 'neovm--test-mc-AF)
    (fmakunbound 'neovm--test-mc-EG)
    (fmakunbound 'neovm--test-mc-AG)
    (fmakunbound 'neovm--test-mc-EU)
    (fmakunbound 'neovm--test-mc-reachable)
    (fmakunbound 'neovm--test-mc-find-path)
"#
}

// ---------------------------------------------------------------------------
// Test 1: Kripke structure construction and basic queries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mc_kripke_construction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (let ((k (funcall 'neovm--test-mc-make-kripke
                 '(s0 s1 s2 s3)
                 '((s0 s1 s2) (s1 s3) (s2 s3) (s3 s0))
                 '((s0 init safe) (s1 processing) (s2 processing safe) (s3 done)))))
        (list
         ;; Basic structure queries
         (funcall 'neovm--test-mc-states k)
         (funcall 'neovm--test-mc-successors k 's0)
         (funcall 'neovm--test-mc-successors k 's3)
         ;; Atomic proposition checks
         (if (funcall 'neovm--test-mc-has-prop k 's0 'init) t nil)
         (if (funcall 'neovm--test-mc-has-prop k 's1 'init) t nil)
         (if (funcall 'neovm--test-mc-has-prop k 's2 'safe) t nil)
         ;; States satisfying atomic propositions
         (funcall 'neovm--test-mc-sat-atom k 'processing)
         (funcall 'neovm--test-mc-sat-atom k 'safe)
         (funcall 'neovm--test-mc-sat-atom k 'done)
         (funcall 'neovm--test-mc-sat-atom k 'nonexistent)))
    {cleanup}))"#,
        preamble = model_checker_preamble(),
        cleanup = model_checker_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 2: EX and AX operators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mc_ex_ax_operators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (let ((k (funcall 'neovm--test-mc-make-kripke
                 '(s0 s1 s2 s3)
                 '((s0 s1 s2) (s1 s3) (s2 s3) (s3 s0))
                 '((s0 a) (s1 b) (s2 b c) (s3 a d)))))
        (let ((sat-a (funcall 'neovm--test-mc-sat-atom k 'a))
              (sat-b (funcall 'neovm--test-mc-sat-atom k 'b))
              (sat-c (funcall 'neovm--test-mc-sat-atom k 'c))
              (sat-d (funcall 'neovm--test-mc-sat-atom k 'd)))
          (list
           ;; EX a: states with a successor satisfying 'a'
           (funcall 'neovm--test-mc-EX k sat-a)
           ;; EX b: states with a successor satisfying 'b'
           (funcall 'neovm--test-mc-EX k sat-b)
           ;; EX d: states with a successor satisfying 'd'
           (funcall 'neovm--test-mc-EX k sat-d)
           ;; AX a: states where ALL successors satisfy 'a'
           (funcall 'neovm--test-mc-AX k sat-a)
           ;; AX b: states where ALL successors satisfy 'b'
           (funcall 'neovm--test-mc-AX k sat-b)
           ;; AX with all states (trivially true)
           (funcall 'neovm--test-mc-AX k '(s0 s1 s2 s3)))))
    {cleanup}))"#,
        preamble = model_checker_preamble(),
        cleanup = model_checker_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 3: EF (reachability) and AF (inevitable)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mc_ef_af_operators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Linear chain: s0 -> s1 -> s2 -> s3 (s3 is terminal/self-loop)
    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (let ((k (funcall 'neovm--test-mc-make-kripke
                 '(s0 s1 s2 s3)
                 '((s0 s1) (s1 s2) (s2 s3) (s3 s3))
                 '((s0 start) (s1 mid) (s2 mid) (s3 end)))))
        (list
         ;; EF end: can s3 be reached? All states should satisfy
         (funcall 'neovm--test-mc-EF k (funcall 'neovm--test-mc-sat-atom k 'end))
         ;; EF start: only s0 satisfies 'start', and no one reaches s0 again
         ;; (s3->s3 loop, no path back to s0)
         (funcall 'neovm--test-mc-EF k (funcall 'neovm--test-mc-sat-atom k 'start))
         ;; AF end: is end inevitable? Yes, all paths lead to s3
         (funcall 'neovm--test-mc-AF k (funcall 'neovm--test-mc-sat-atom k 'end))
         ;; AF start: is start inevitable? No, once past s0 we never return
         (funcall 'neovm--test-mc-AF k (funcall 'neovm--test-mc-sat-atom k 'start))))
    {cleanup}))"#,
        preamble = model_checker_preamble(),
        cleanup = model_checker_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 4: EG and AG operators (persistence)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mc_eg_ag_operators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Diamond with cycle: s0 -> s1, s0 -> s2, s1 -> s3, s2 -> s3, s3 -> s3
    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (let ((k (funcall 'neovm--test-mc-make-kripke
                 '(s0 s1 s2 s3)
                 '((s0 s1 s2) (s1 s3) (s2 s3) (s3 s3))
                 '((s0 safe) (s1 safe processing) (s2 dangerous) (s3 safe done)))))
        (let ((sat-safe (funcall 'neovm--test-mc-sat-atom k 'safe)))
          (list
           ;; EG safe: exists a path where safe always holds
           ;; Path s0->s1->s3->s3... all safe => s0, s1, s3 satisfy EG safe
           (funcall 'neovm--test-mc-EG k sat-safe)
           ;; AG safe: on ALL paths, safe always holds
           ;; s0 -> s2 which is dangerous, so s0 fails AG safe
           ;; s3 -> s3 (safe loop) => s3 satisfies AG safe
           ;; s1 -> s3 (safe) => s1 satisfies AG safe
           (funcall 'neovm--test-mc-AG k sat-safe)
           ;; EG processing: only s1 has processing, and s1->s3 (no processing)
           ;; So no state can have EG processing (no infinite processing path)
           (funcall 'neovm--test-mc-EG k
                    (funcall 'neovm--test-mc-sat-atom k 'processing))
           ;; AG done: only s3 has done and loops to itself
           (funcall 'neovm--test-mc-AG k
                    (funcall 'neovm--test-mc-sat-atom k 'done)))))
    {cleanup}))"#,
        preamble = model_checker_preamble(),
        cleanup = model_checker_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 5: EU (exists until) operator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mc_eu_operator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Traffic light: green -> yellow -> red -> green (cycle)
    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (let ((k (funcall 'neovm--test-mc-make-kripke
                 '(green yellow red)
                 '((green yellow) (yellow red) (red green))
                 '((green go safe) (yellow caution safe) (red stop)))))
        (let ((sat-safe (funcall 'neovm--test-mc-sat-atom k 'safe))
              (sat-stop (funcall 'neovm--test-mc-sat-atom k 'stop))
              (sat-go (funcall 'neovm--test-mc-sat-atom k 'go))
              (sat-caution (funcall 'neovm--test-mc-sat-atom k 'caution)))
          (list
           ;; EU(safe, stop): safe holds until stop
           ;; green(safe) -> yellow(safe) -> red(stop): green & yellow satisfy
           ;; red satisfies because psi(stop) holds immediately
           (funcall 'neovm--test-mc-EU k sat-safe sat-stop)
           ;; EU(go, stop): go holds until stop
           ;; Only green has 'go', and green->yellow (no go), so only red satisfies (psi)
           ;; Actually: green->yellow(no go), so green can't maintain go until stop
           ;; Only red satisfies (psi=stop holds immediately)
           (funcall 'neovm--test-mc-EU k sat-go sat-stop)
           ;; EU(caution, go): caution until go
           ;; yellow(caution)->red(no caution, no go), fails
           ;; Only green satisfies (psi=go immediately)
           (funcall 'neovm--test-mc-EU k sat-caution sat-go)
           ;; EU(true, stop): true until stop = EF stop
           (funcall 'neovm--test-mc-EU k '(green yellow red) sat-stop))))
    {cleanup}))"#,
        preamble = model_checker_preamble(),
        cleanup = model_checker_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 6: Reachability and safety checking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mc_reachability_and_safety() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // System with safe and unsafe states
    // s0 (init) -> s1 (safe) -> s2 (safe) -> s3 (unsafe!)
    //                                    \-> s4 (safe, terminal)
    let form = format!(
        r#"(progn
  {preamble}

  ;; Safety check: verify no reachable state has 'error' property
  (fset 'neovm--test-mc-check-safety
    (lambda (k initial-states bad-prop)
      (let ((reachable (funcall 'neovm--test-mc-reachable k initial-states))
            (bad-states (funcall 'neovm--test-mc-sat-atom k bad-prop))
            (violations nil))
        (dolist (s reachable)
          (when (memq s bad-states)
            (setq violations (cons s violations))))
        (if violations
            (list 'unsafe (nreverse violations))
          (list 'safe)))))

  (unwind-protect
      (let ((k1 (funcall 'neovm--test-mc-make-kripke
                  '(s0 s1 s2 s3 s4)
                  '((s0 s1) (s1 s2) (s2 s3 s4) (s3 s3) (s4 s4))
                  '((s0 init) (s1 safe) (s2 safe) (s3 error) (s4 safe done))))
            ;; k2: safe system with no error states reachable
            (k2 (funcall 'neovm--test-mc-make-kripke
                  '(a b c d)
                  '((a b) (b c) (c d) (d d))
                  '((a init) (b ok) (c ok) (d ok done)))))
        (list
         ;; Reachability from s0: should reach all states
         (funcall 'neovm--test-mc-reachable k1 '(s0))
         ;; Reachability from s2: should reach s2, s3, s4
         (funcall 'neovm--test-mc-reachable k1 '(s2))
         ;; Safety check on k1: should find s3 as violation
         (funcall 'neovm--test-mc-check-safety k1 '(s0) 'error)
         ;; Safety check starting from s4 only: safe (s3 not reachable)
         (funcall 'neovm--test-mc-check-safety k1 '(s4) 'error)
         ;; Safety check on k2: no error states at all
         (funcall 'neovm--test-mc-check-safety k2 '(a) 'error)
         ;; Reachability on k2
         (funcall 'neovm--test-mc-reachable k2 '(a))))
    (fmakunbound 'neovm--test-mc-check-safety)
    {cleanup}))"#,
        preamble = model_checker_preamble(),
        cleanup = model_checker_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 7: Liveness checking and counterexample generation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mc_liveness_and_counterexample() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}

  ;; Liveness: every reachable state can eventually reach 'done'
  (fset 'neovm--test-mc-check-liveness
    (lambda (k initial-states goal-prop)
      (let* ((reachable (funcall 'neovm--test-mc-reachable k initial-states))
             (goal-states (funcall 'neovm--test-mc-sat-atom k goal-prop))
             (can-reach-goal (funcall 'neovm--test-mc-EF k goal-states))
             (stuck nil))
        (dolist (s reachable)
          (unless (memq s can-reach-goal)
            (setq stuck (cons s stuck))))
        (if stuck
            (list 'not-live (sort (nreverse stuck)
                                  (lambda (a b) (string< (symbol-name a) (symbol-name b)))))
          (list 'live)))))

  (unwind-protect
      (let* (;; k1: system where not all states can reach 'done'
             ;; s0 -> s1 -> s2 (loop), s0 -> s3 -> s4 (done)
             (k1 (funcall 'neovm--test-mc-make-kripke
                   '(s0 s1 s2 s3 s4)
                   '((s0 s1 s3) (s1 s2) (s2 s1) (s3 s4) (s4 s4))
                   '((s0 init) (s1 work) (s2 work) (s3 finishing) (s4 done))))
             ;; k2: all paths eventually reach done
             (k2 (funcall 'neovm--test-mc-make-kripke
                   '(a b c)
                   '((a b) (b c) (c c))
                   '((a start) (b mid) (c done)))))
        (list
         ;; Liveness check on k1: s1 and s2 are stuck in a loop
         (funcall 'neovm--test-mc-check-liveness k1 '(s0) 'done)
         ;; Liveness check on k2: all states can reach done
         (funcall 'neovm--test-mc-check-liveness k2 '(a) 'done)
         ;; Counterexample: find path from s0 to s1 (exists)
         (funcall 'neovm--test-mc-find-path k1 's0 's1)
         ;; Counterexample: find path from s0 to s4
         (funcall 'neovm--test-mc-find-path k1 's0 's4)
         ;; Counterexample: find path from s1 to s4 — impossible (stuck in loop)
         (funcall 'neovm--test-mc-find-path k1 's1 's4)
         ;; Path in linear system
         (funcall 'neovm--test-mc-find-path k2 'a 'c)))
    (fmakunbound 'neovm--test-mc-check-liveness)
    {cleanup}))"#,
        preamble = model_checker_preamble(),
        cleanup = model_checker_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 8: Mutual exclusion protocol verification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mc_mutual_exclusion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Model a simple two-process mutual exclusion protocol
    // States encode (process1_state, process2_state) where each can be:
    // idle, trying, critical
    // Safety: never both in critical section
    let form = format!(
        r#"(progn
  {preamble}

  (unwind-protect
      (let* (;; Encode states as symbols: p1state-p2state
             ;; Only model a subset to keep it tractable
             (k (funcall 'neovm--test-mc-make-kripke
                  '(ii it ti ic ci tc ct)
                  ;; ii=idle-idle, it=idle-trying, etc.
                  '((ii it ti)        ;; either process starts trying
                    (it ic ti)        ;; p2 enters critical (if p1 idle) or p1 starts trying
                    (ti tc it)        ;; p1 enters critical (if p2 idle) or p2 starts trying
                    (ic it)           ;; p2 leaves critical -> idle
                    (ci ti)           ;; p1 leaves critical -> idle
                    (tc ti)           ;; p1 leaves critical, p2 still trying
                    (ct it))          ;; p2 leaves critical, p1 still trying
                  '((ii idle)
                    (it p2trying)
                    (ti p1trying)
                    (ic p2critical)
                    (ci p1critical)
                    (tc p1critical p2trying)
                    (ct p2critical p1trying)))))
        ;; Note: no state has both p1critical and p2critical — that's the safety invariant
        (list
         ;; Reachable states from initial idle-idle
         (funcall 'neovm--test-mc-reachable k '(ii))
         ;; States where p1 is in critical section
         (funcall 'neovm--test-mc-sat-atom k 'p1critical)
         ;; States where p2 is in critical section
         (funcall 'neovm--test-mc-sat-atom k 'p2critical)
         ;; EF p1critical: from which states can p1 reach critical?
         (funcall 'neovm--test-mc-EF k (funcall 'neovm--test-mc-sat-atom k 'p1critical))
         ;; AG(not both critical): safety property
         ;; Since no state has both, all states satisfy AG(not-both)
         ;; Check: intersection of p1critical and p2critical states
         (let ((p1c (funcall 'neovm--test-mc-sat-atom k 'p1critical))
               (p2c (funcall 'neovm--test-mc-sat-atom k 'p2critical))
               (both nil))
           (dolist (s p1c)
             (when (memq s p2c)
               (setq both (cons s both))))
           (if both (list 'UNSAFE both) (list 'SAFE 'mutual-exclusion-holds)))))
    {cleanup}))"#,
        preamble = model_checker_preamble(),
        cleanup = model_checker_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 9: Complex CTL formula composition
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mc_complex_ctl_formulas() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compose CTL operators: AG(EF done), EF(AG safe), etc.
    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (let ((k (funcall 'neovm--test-mc-make-kripke
                 '(s0 s1 s2 s3 s4)
                 '((s0 s1 s2) (s1 s3) (s2 s4) (s3 s0) (s4 s4))
                 '((s0 alive) (s1 alive working) (s2 alive) (s3 alive resting)
                   (s4 done)))))
        (let ((sat-done (funcall 'neovm--test-mc-sat-atom k 'done))
              (sat-alive (funcall 'neovm--test-mc-sat-atom k 'alive))
              (sat-working (funcall 'neovm--test-mc-sat-atom k 'working)))
          (list
           ;; EF done: which states can reach 'done'?
           (funcall 'neovm--test-mc-EF k sat-done)
           ;; AG alive: states where alive always holds on all paths
           ;; s4 is NOT alive, so any state that can reach s4 via all paths fails
           (funcall 'neovm--test-mc-AG k sat-alive)
           ;; EG alive: states where there exists an infinite path staying alive
           ;; s0->s1->s3->s0... is an alive cycle
           (funcall 'neovm--test-mc-EG k sat-alive)
           ;; AG(EF done): from every reachable state, done is always reachable
           ;; First compute EF done, then AG of that
           (let ((ef-done (funcall 'neovm--test-mc-EF k sat-done)))
             (funcall 'neovm--test-mc-AG k ef-done))
           ;; EX working: states with a successor that's working
           (funcall 'neovm--test-mc-EX k sat-working)
           ;; AX alive: states where all successors are alive
           (funcall 'neovm--test-mc-AX k sat-alive))))
    {cleanup}))"#,
        preamble = model_checker_preamble(),
        cleanup = model_checker_cleanup()
    );
    assert_oracle_parity(&form);
}
