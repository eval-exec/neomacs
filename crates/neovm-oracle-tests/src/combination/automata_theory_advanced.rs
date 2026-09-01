//! Advanced automata theory oracle parity tests.
//!
//! Implements DFA (states, alphabet, delta, start, accept), NFA with epsilon
//! transitions, subset construction (NFA->DFA), DFA minimization (Hopcroft's),
//! Thompson's construction (regex->NFA), DFA complement/intersection/union,
//! language emptiness, string acceptance, DFA equivalence, pumping lemma
//! test structure, and Myhill-Nerode equivalence classes.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// DFA representation, acceptance, complement
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_automata_adv_dfa_basic_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; DFA: transitions hash (state . symbol) -> state
  ;; dfa-accept?: run DFA on input string, return t/nil

  (fset 'neovm--adv-dfa-run
    (lambda (trans start accepts input)
      "Run DFA on INPUT. Return (accepted? final-state)."
      (let ((state start) (i 0) (len (length input)) (stuck nil))
        (while (and (< i len) (not stuck))
          (let ((next (gethash (cons state (aref input i)) trans)))
            (if next
                (setq state next)
              (setq stuck t)))
          (setq i (1+ i)))
        (if stuck
            (list nil nil)
          (list (if (memq state accepts) t nil) state)))))

  ;; DFA complement: same DFA but flip accept/non-accept
  (fset 'neovm--adv-dfa-complement-accepts
    (lambda (all-states accepts)
      "Return complement accept set."
      (let ((result nil))
        (dolist (s all-states)
          (unless (memq s accepts)
            (setq result (cons s result))))
        result)))

  (unwind-protect
      (let ((trans (make-hash-table :test 'equal)))
        ;; DFA for "strings containing 'ab'" over {a,b}
        ;; States: q0 (no progress), q1 (saw 'a'), q2 (saw 'ab', accept)
        (puthash '(q0 . ?a) 'q1 trans) (puthash '(q0 . ?b) 'q0 trans)
        (puthash '(q1 . ?a) 'q1 trans) (puthash '(q1 . ?b) 'q2 trans)
        (puthash '(q2 . ?a) 'q2 trans) (puthash '(q2 . ?b) 'q2 trans)
        (let ((states '(q0 q1 q2))
              (accepts '(q2)))
          (list
            ;; Accept tests
            (car (funcall 'neovm--adv-dfa-run trans 'q0 accepts "ab"))
            (car (funcall 'neovm--adv-dfa-run trans 'q0 accepts "aab"))
            (car (funcall 'neovm--adv-dfa-run trans 'q0 accepts "bab"))
            (car (funcall 'neovm--adv-dfa-run trans 'q0 accepts "abab"))
            ;; Reject tests
            (car (funcall 'neovm--adv-dfa-run trans 'q0 accepts ""))
            (car (funcall 'neovm--adv-dfa-run trans 'q0 accepts "a"))
            (car (funcall 'neovm--adv-dfa-run trans 'q0 accepts "b"))
            (car (funcall 'neovm--adv-dfa-run trans 'q0 accepts "ba"))
            (car (funcall 'neovm--adv-dfa-run trans 'q0 accepts "bba"))
            ;; Complement: accepts strings NOT containing 'ab'
            (let ((comp-accepts (funcall 'neovm--adv-dfa-complement-accepts
                                         states accepts)))
              (list
                (car (funcall 'neovm--adv-dfa-run trans 'q0 comp-accepts ""))
                (car (funcall 'neovm--adv-dfa-run trans 'q0 comp-accepts "a"))
                (car (funcall 'neovm--adv-dfa-run trans 'q0 comp-accepts "ba"))
                (car (funcall 'neovm--adv-dfa-run trans 'q0 comp-accepts "ab"))
                (car (funcall 'neovm--adv-dfa-run trans 'q0 comp-accepts "bba")))))))
    (fmakunbound 'neovm--adv-dfa-run)
    (fmakunbound 'neovm--adv-dfa-complement-accepts)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t nil nil nil nil nil (t t t nil t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// NFA with epsilon transitions and epsilon-closure
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_automata_adv_nfa_epsilon_transitions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; NFA: trans hash (state . char) -> list-of-states
  ;; eps hash state -> list-of-states (epsilon transitions)

  (fset 'neovm--adv-eps-closure
    (lambda (states eps)
      "Compute epsilon closure of a set of states."
      (let ((result (copy-sequence states))
            (worklist (copy-sequence states)))
        (while worklist
          (let ((s (car worklist)))
            (setq worklist (cdr worklist))
            (dolist (t2 (gethash s eps))
              (unless (memq t2 result)
                (setq result (cons t2 result))
                (setq worklist (cons t2 worklist))))))
        result)))

  (fset 'neovm--adv-nfa-step
    (lambda (current-states ch trans eps)
      "Advance NFA one step on character CH."
      (let ((next nil))
        (dolist (s current-states)
          (dolist (t2 (gethash (cons s ch) trans))
            (unless (memq t2 next)
              (setq next (cons t2 next)))))
        (funcall 'neovm--adv-eps-closure next eps))))

  (fset 'neovm--adv-nfa-accepts
    (lambda (trans eps start accepts input)
      "Run NFA on INPUT, return t if accepted."
      (let ((current (funcall 'neovm--adv-eps-closure (list start) eps))
            (i 0) (len (length input)))
        (while (< i len)
          (setq current (funcall 'neovm--adv-nfa-step
                                  current (aref input i) trans eps))
          (setq i (1+ i)))
        (let ((found nil))
          (dolist (s current)
            (when (memq s accepts) (setq found t)))
          found))))

  (unwind-protect
      ;; NFA for (a|b)*abb (standard textbook NFA)
      ;; States: 0(start), 1, 2, 3(accept)
      (let ((trans (make-hash-table :test 'equal))
            (eps (make-hash-table :test 'equal)))
        ;; State 0: a->0, b->0 (loop on any), a->1 (start matching)
        (puthash '(0 . ?a) '(0 1) trans)
        (puthash '(0 . ?b) '(0) trans)
        ;; State 1: b->2
        (puthash '(1 . ?b) '(2) trans)
        ;; State 2: b->3
        (puthash '(2 . ?b) '(3) trans)
        (list
          ;; Accept: ends with "abb"
          (funcall 'neovm--adv-nfa-accepts trans eps 0 '(3) "abb")
          (funcall 'neovm--adv-nfa-accepts trans eps 0 '(3) "aabb")
          (funcall 'neovm--adv-nfa-accepts trans eps 0 '(3) "babb")
          (funcall 'neovm--adv-nfa-accepts trans eps 0 '(3) "aababb")
          ;; Reject: doesn't end with "abb"
          (funcall 'neovm--adv-nfa-accepts trans eps 0 '(3) "")
          (funcall 'neovm--adv-nfa-accepts trans eps 0 '(3) "ab")
          (funcall 'neovm--adv-nfa-accepts trans eps 0 '(3) "abba")
          (funcall 'neovm--adv-nfa-accepts trans eps 0 '(3) "bba")
          ;; Epsilon transition test: NFA with epsilon
          ;; NFA for a*|b: start --eps--> q1 --a--> q1 (accept), start --eps--> q2 --b--> q3 (accept)
          (let ((t2 (make-hash-table :test 'equal))
                (e2 (make-hash-table :test 'equal)))
            (puthash 's0 '(q1 q2) e2)  ;; epsilon from s0 to q1 and q2
            (puthash '(q1 . ?a) '(q1) t2)  ;; q1 loops on 'a'
            (puthash '(q2 . ?b) '(q3) t2)  ;; q2 -> q3 on 'b'
            (list
              (funcall 'neovm--adv-nfa-accepts t2 e2 's0 '(q1 q3) "")    ;; epsilon to q1 (accept)
              (funcall 'neovm--adv-nfa-accepts t2 e2 's0 '(q1 q3) "a")   ;; a via q1
              (funcall 'neovm--adv-nfa-accepts t2 e2 's0 '(q1 q3) "aaa") ;; a* via q1
              (funcall 'neovm--adv-nfa-accepts t2 e2 's0 '(q1 q3) "b")   ;; b via q2->q3
              (funcall 'neovm--adv-nfa-accepts t2 e2 's0 '(q1 q3) "ab")  ;; reject
              (funcall 'neovm--adv-nfa-accepts t2 e2 's0 '(q1 q3) "bb")  ;; reject
              ))))
    (fmakunbound 'neovm--adv-eps-closure)
    (fmakunbound 'neovm--adv-nfa-step)
    (fmakunbound 'neovm--adv-nfa-accepts)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t nil nil nil nil (t t t t nil nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Subset construction: NFA -> DFA
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_automata_adv_subset_construction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Epsilon closure (reused)
  (fset 'neovm--adv-sc-eps-closure
    (lambda (states eps)
      (let ((result (copy-sequence states))
            (work (copy-sequence states)))
        (while work
          (let ((s (car work)))
            (setq work (cdr work))
            (dolist (t2 (gethash s eps))
              (unless (memq t2 result)
                (setq result (cons t2 result))
                (setq work (cons t2 work))))))
        (sort result (lambda (a b) (< a b))))))  ;; canonical order

  ;; Subset construction
  (fset 'neovm--adv-nfa-to-dfa
    (lambda (nfa-trans nfa-eps start nfa-accepts alphabet)
      "Convert NFA to DFA via subset construction.
       Returns (dfa-trans dfa-start dfa-accepts dfa-state-count)."
      (let* ((dfa-trans (make-hash-table :test 'equal))
             (start-set (funcall 'neovm--adv-sc-eps-closure (list start) nfa-eps))
             (state-map (make-hash-table :test 'equal))
             (state-counter 0)
             (worklist (list start-set))
             (dfa-accepts nil))
        ;; Assign DFA state ID to start set
        (puthash (prin1-to-string start-set) state-counter state-map)
        (setq state-counter (1+ state-counter))
        (while worklist
          (let* ((current (car worklist))
                 (cur-id (gethash (prin1-to-string current) state-map)))
            (setq worklist (cdr worklist))
            ;; Check if this DFA state is accepting
            (let ((is-accept nil))
              (dolist (s current)
                (when (memq s nfa-accepts) (setq is-accept t)))
              (when is-accept
                (setq dfa-accepts (cons cur-id dfa-accepts))))
            ;; For each alphabet symbol, compute successor
            (dolist (ch alphabet)
              (let ((next nil))
                (dolist (s current)
                  (dolist (t2 (gethash (cons s ch) nfa-trans))
                    (unless (memq t2 next)
                      (setq next (cons t2 next)))))
                (setq next (funcall 'neovm--adv-sc-eps-closure next nfa-eps))
                (when next
                  (let ((next-key (prin1-to-string next)))
                    (unless (gethash next-key state-map)
                      (puthash next-key state-counter state-map)
                      (setq state-counter (1+ state-counter))
                      (setq worklist (cons next worklist)))
                    (puthash (cons cur-id ch)
                             (gethash next-key state-map)
                             dfa-trans)))))))
        (list dfa-trans 0 dfa-accepts state-counter))))

  ;; DFA simulation helper
  (fset 'neovm--adv-sc-dfa-run
    (lambda (trans start accepts input)
      (let ((state start) (i 0) (len (length input)) (stuck nil))
        (while (and (< i len) (not stuck))
          (let ((next (gethash (cons state (aref input i)) trans)))
            (if next (setq state next) (setq stuck t)))
          (setq i (1+ i)))
        (and (not stuck) (memq state accepts) t))))

  (unwind-protect
      ;; Build NFA for (a|b)*abb, convert to DFA, test same strings
      (let ((nfa-trans (make-hash-table :test 'equal))
            (nfa-eps (make-hash-table :test 'equal)))
        (puthash '(0 . ?a) '(0 1) nfa-trans)
        (puthash '(0 . ?b) '(0) nfa-trans)
        (puthash '(1 . ?b) '(2) nfa-trans)
        (puthash '(2 . ?b) '(3) nfa-trans)
        (let* ((dfa-result (funcall 'neovm--adv-nfa-to-dfa
                                     nfa-trans nfa-eps 0 '(3) '(?a ?b)))
               (dfa-trans (nth 0 dfa-result))
               (dfa-start (nth 1 dfa-result))
               (dfa-accepts (nth 2 dfa-result))
               (dfa-count (nth 3 dfa-result)))
          (list
            ;; Number of DFA states (should be 4 for this NFA)
            dfa-count
            ;; DFA acceptance matches NFA
            (funcall 'neovm--adv-sc-dfa-run dfa-trans dfa-start dfa-accepts "abb")
            (funcall 'neovm--adv-sc-dfa-run dfa-trans dfa-start dfa-accepts "aabb")
            (funcall 'neovm--adv-sc-dfa-run dfa-trans dfa-start dfa-accepts "babb")
            (funcall 'neovm--adv-sc-dfa-run dfa-trans dfa-start dfa-accepts "")
            (funcall 'neovm--adv-sc-dfa-run dfa-trans dfa-start dfa-accepts "ab")
            (funcall 'neovm--adv-sc-dfa-run dfa-trans dfa-start dfa-accepts "abba"))))
    (fmakunbound 'neovm--adv-sc-eps-closure)
    (fmakunbound 'neovm--adv-nfa-to-dfa)
    (fmakunbound 'neovm--adv-sc-dfa-run)))"#;
    let expect = expect_test::expect![[r#""OK (4 t t t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// DFA minimization: Hopcroft-style partition refinement
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_automata_adv_dfa_minimization_hopcroft() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--adv-hopcroft-minimize
    (lambda (states alphabet trans accepts)
      "Hopcroft-style DFA minimization via iterative partition refinement.
       Returns number of equivalence classes (minimized state count)."
      (let* ((non-accepts (let ((na nil))
                            (dolist (s states) (unless (memq s accepts) (setq na (cons s na))))
                            na))
             ;; Initial partition: {accepts, non-accepts}
             (partition (if non-accepts (list accepts non-accepts) (list accepts)))
             (changed t))
        ;; Refine until stable
        (while changed
          (setq changed nil)
          (let ((new-partition nil))
            (dolist (group partition)
              (if (<= (length group) 1)
                  (setq new-partition (cons group new-partition))
                ;; Try to split this group
                (let ((split-found nil))
                  (dolist (ch alphabet)
                    (unless split-found
                      ;; For each state in group, find which partition block its successor lands in
                      (let ((groups-map (make-hash-table :test 'equal)))
                        (dolist (s group)
                          (let* ((succ (gethash (cons s ch) trans))
                                 (block-idx
                                  (if succ
                                      (let ((idx 0) (found -1))
                                        (dolist (p partition)
                                          (when (memq succ p) (setq found idx))
                                          (setq idx (1+ idx)))
                                        found)
                                    -2)))  ;; dead state
                            (puthash block-idx
                                     (cons s (gethash block-idx groups-map))
                                     groups-map)))
                        ;; If more than one group, split
                        (when (> (hash-table-count groups-map) 1)
                          (setq split-found t)
                          (setq changed t)
                          (maphash (lambda (_k v) (setq new-partition (cons v new-partition)))
                                   groups-map)))))
                  (unless split-found
                    (setq new-partition (cons group new-partition))))))
            (setq partition new-partition)))
        ;; Return count of partitions
        (length partition))))

  (unwind-protect
      (list
        ;; DFA with 5 states, 2 equivalent pairs: should minimize to 3
        ;; A=accept, B=non-accept, C=non-accept, D=non-accept equiv B, E=non-accept equiv C
        (let ((t1 (make-hash-table :test 'equal)))
          (puthash '(A . ?0) 'B t1) (puthash '(A . ?1) 'C t1)
          (puthash '(B . ?0) 'A t1) (puthash '(B . ?1) 'D t1)
          (puthash '(C . ?0) 'E t1) (puthash '(C . ?1) 'A t1)
          (puthash '(D . ?0) 'A t1) (puthash '(D . ?1) 'B t1)
          (puthash '(E . ?0) 'C t1) (puthash '(E . ?1) 'A t1)
          (funcall 'neovm--adv-hopcroft-minimize
                   '(A B C D E) '(?0 ?1) t1 '(A)))
        ;; Already minimal 2-state DFA
        (let ((t2 (make-hash-table :test 'equal)))
          (puthash '(q0 . ?a) 'q1 t2) (puthash '(q0 . ?b) 'q0 t2)
          (puthash '(q1 . ?a) 'q1 t2) (puthash '(q1 . ?b) 'q1 t2)
          (funcall 'neovm--adv-hopcroft-minimize
                   '(q0 q1) '(?a ?b) t2 '(q1)))
        ;; Single state DFA (universal acceptor)
        (let ((t3 (make-hash-table :test 'equal)))
          (puthash '(q . ?0) 'q t3) (puthash '(q . ?1) 'q t3)
          (funcall 'neovm--adv-hopcroft-minimize '(q) '(?0 ?1) t3 '(q)))
        ;; 4-state DFA with 2 equivalent pairs -> 2 states
        (let ((t4 (make-hash-table :test 'equal)))
          (puthash '(s0 . ?a) 's1 t4) (puthash '(s0 . ?b) 's2 t4)
          (puthash '(s1 . ?a) 's3 t4) (puthash '(s1 . ?b) 's0 t4)
          (puthash '(s2 . ?a) 's3 t4) (puthash '(s2 . ?b) 's0 t4)
          (puthash '(s3 . ?a) 's1 t4) (puthash '(s3 . ?b) 's2 t4)
          (funcall 'neovm--adv-hopcroft-minimize
                   '(s0 s1 s2 s3) '(?a ?b) t4 '(s0 s3))))
    (fmakunbound 'neovm--adv-hopcroft-minimize)))"#;
    let expect = expect_test::expect![[r#""OK (3 2 1 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Thompson's construction: regex -> NFA
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_automata_adv_thompson_construction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Simple regex AST: (lit c) | (cat r1 r2) | (alt r1 r2) | (star r)
  ;; Thompson's: each construct produces NFA with single start and single accept

  (defvar neovm--adv-tc-counter 0)
  (fset 'neovm--adv-tc-fresh
    (lambda () (setq neovm--adv-tc-counter (1+ neovm--adv-tc-counter))
      neovm--adv-tc-counter))

  ;; Returns: (start accept trans eps)
  (fset 'neovm--adv-tc-build
    (lambda (regex trans eps)
      "Build NFA fragment for REGEX. Mutates TRANS and EPS."
      (cond
       ;; Literal: s --c--> a
       ((eq (car regex) 'lit)
        (let ((s (funcall 'neovm--adv-tc-fresh))
              (a (funcall 'neovm--adv-tc-fresh))
              (ch (cadr regex)))
          (puthash (cons s ch) (cons a (gethash (cons s ch) trans)) trans)
          (list s a)))

       ;; Concatenation: build r1, build r2, eps from r1.accept to r2.start
       ((eq (car regex) 'cat)
        (let* ((n1 (funcall 'neovm--adv-tc-build (cadr regex) trans eps))
               (n2 (funcall 'neovm--adv-tc-build (caddr regex) trans eps)))
          (puthash (cadr n1)
                   (cons (car n2) (gethash (cadr n1) eps))
                   eps)
          (list (car n1) (cadr n2))))

       ;; Alternation: new start eps to both, both accepts eps to new accept
       ((eq (car regex) 'alt)
        (let* ((s (funcall 'neovm--adv-tc-fresh))
               (a (funcall 'neovm--adv-tc-fresh))
               (n1 (funcall 'neovm--adv-tc-build (cadr regex) trans eps))
               (n2 (funcall 'neovm--adv-tc-build (caddr regex) trans eps)))
          (puthash s (list (car n1) (car n2)) eps)
          (puthash (cadr n1) (cons a (gethash (cadr n1) eps)) eps)
          (puthash (cadr n2) (cons a (gethash (cadr n2) eps)) eps)
          (list s a)))

       ;; Kleene star: new start/accept, eps to inner start and to accept
       ((eq (car regex) 'star)
        (let* ((s (funcall 'neovm--adv-tc-fresh))
               (a (funcall 'neovm--adv-tc-fresh))
               (n (funcall 'neovm--adv-tc-build (cadr regex) trans eps)))
          (puthash s (list (car n) a) eps)
          (puthash (cadr n) (list (car n) a) eps)
          (list s a))))))

  ;; NFA simulation
  (fset 'neovm--adv-tc-eps-closure
    (lambda (states eps)
      (let ((result (copy-sequence states))
            (work (copy-sequence states)))
        (while work
          (let ((s (car work)))
            (setq work (cdr work))
            (dolist (t2 (gethash s eps))
              (unless (memq t2 result)
                (setq result (cons t2 result))
                (setq work (cons t2 work))))))
        result)))

  (fset 'neovm--adv-tc-simulate
    (lambda (trans eps start accept input)
      (let ((current (funcall 'neovm--adv-tc-eps-closure (list start) eps))
            (i 0) (len (length input)))
        (while (< i len)
          (let ((next nil))
            (dolist (s current)
              (dolist (t2 (gethash (cons s (aref input i)) trans))
                (unless (memq t2 next)
                  (setq next (cons t2 next)))))
            (setq current (funcall 'neovm--adv-tc-eps-closure next eps)))
          (setq i (1+ i)))
        (memq accept current))))

  (unwind-protect
      (progn
        (setq neovm--adv-tc-counter 0)
        ;; Build NFA for regex: (a|b)*abb
        ;; AST: (cat (star (alt (lit ?a) (lit ?b))) (cat (lit ?a) (cat (lit ?b) (lit ?b))))
        (let ((trans (make-hash-table :test 'equal))
              (eps (make-hash-table :test 'equal)))
          (let* ((regex '(cat (star (alt (lit ?a) (lit ?b)))
                              (cat (lit ?a) (cat (lit ?b) (lit ?b)))))
                 (nfa (funcall 'neovm--adv-tc-build regex trans eps))
                 (start (car nfa))
                 (accept (cadr nfa)))
            (list
              ;; Accept tests
              (not (null (funcall 'neovm--adv-tc-simulate trans eps start accept "abb")))
              (not (null (funcall 'neovm--adv-tc-simulate trans eps start accept "aabb")))
              (not (null (funcall 'neovm--adv-tc-simulate trans eps start accept "babb")))
              (not (null (funcall 'neovm--adv-tc-simulate trans eps start accept "abababb")))
              ;; Reject tests
              (not (null (funcall 'neovm--adv-tc-simulate trans eps start accept "")))
              (not (null (funcall 'neovm--adv-tc-simulate trans eps start accept "ab")))
              (not (null (funcall 'neovm--adv-tc-simulate trans eps start accept "abba")))
              ;; Test regex: a*
              (setq neovm--adv-tc-counter 100)
              (let ((t2 (make-hash-table :test 'equal))
                    (e2 (make-hash-table :test 'equal)))
                (let* ((n2 (funcall 'neovm--adv-tc-build '(star (lit ?a)) t2 e2)))
                  (list
                    (not (null (funcall 'neovm--adv-tc-simulate t2 e2 (car n2) (cadr n2) "")))
                    (not (null (funcall 'neovm--adv-tc-simulate t2 e2 (car n2) (cadr n2) "a")))
                    (not (null (funcall 'neovm--adv-tc-simulate t2 e2 (car n2) (cadr n2) "aaa")))
                    (not (null (funcall 'neovm--adv-tc-simulate t2 e2 (car n2) (cadr n2) "b"))))))))))
    (makunbound 'neovm--adv-tc-counter)
    (fmakunbound 'neovm--adv-tc-fresh)
    (fmakunbound 'neovm--adv-tc-build)
    (fmakunbound 'neovm--adv-tc-eps-closure)
    (fmakunbound 'neovm--adv-tc-simulate)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t nil nil nil 100 (t t t nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// DFA intersection and union via product construction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_automata_adv_dfa_intersection_union() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Product construction: given two DFAs, build product DFA
  ;; Intersection: accept iff both accept
  ;; Union: accept iff either accepts

  (fset 'neovm--adv-product-run
    (lambda (t1 s1 acc1 t2 s2 acc2 alphabet input mode)
      "Simulate product automaton on INPUT.
       MODE is 'intersect or 'union."
      (let ((state1 s1) (state2 s2)
            (i 0) (len (length input)) (stuck nil))
        (while (and (< i len) (not stuck))
          (let ((ch (aref input i)))
            (let ((n1 (gethash (cons state1 ch) t1))
                  (n2 (gethash (cons state2 ch) t2)))
              (if (and n1 n2)
                  (progn (setq state1 n1) (setq state2 n2))
                (setq stuck t))))
          (setq i (1+ i)))
        (if stuck nil
          (let ((a1 (memq state1 acc1))
                (a2 (memq state2 acc2)))
            (if (eq mode 'intersect)
                (and a1 a2 t)
              (or a1 a2)))))))

  (unwind-protect
      ;; DFA1: strings with even number of a's
      ;; DFA2: strings ending with b
      (let ((t1 (make-hash-table :test 'equal))
            (t2 (make-hash-table :test 'equal)))
        ;; DFA1: even/odd a-count. States: even(accept), odd
        (puthash '(even . ?a) 'odd t1) (puthash '(even . ?b) 'even t1)
        (puthash '(odd . ?a) 'even t1) (puthash '(odd . ?b) 'odd t1)
        ;; DFA2: ends with b. States: q0(start), q1(accept, saw b)
        (puthash '(q0 . ?a) 'q0 t2) (puthash '(q0 . ?b) 'q1 t2)
        (puthash '(q1 . ?a) 'q0 t2) (puthash '(q1 . ?b) 'q1 t2)
        (list
          ;; Intersection: even a's AND ends with b
          (funcall 'neovm--adv-product-run t1 'even '(even) t2 'q0 '(q1) '(?a ?b) "b" 'intersect)      ;; t (0 a's=even, ends b)
          (funcall 'neovm--adv-product-run t1 'even '(even) t2 'q0 '(q1) '(?a ?b) "aab" 'intersect)    ;; t (2 a's=even, ends b)
          (funcall 'neovm--adv-product-run t1 'even '(even) t2 'q0 '(q1) '(?a ?b) "ab" 'intersect)     ;; nil (1 a=odd)
          (funcall 'neovm--adv-product-run t1 'even '(even) t2 'q0 '(q1) '(?a ?b) "aa" 'intersect)     ;; nil (even but ends a)
          (funcall 'neovm--adv-product-run t1 'even '(even) t2 'q0 '(q1) '(?a ?b) "" 'intersect)       ;; nil (even but no end b)
          ;; Union: even a's OR ends with b
          (funcall 'neovm--adv-product-run t1 'even '(even) t2 'q0 '(q1) '(?a ?b) "" 'union)           ;; t (even, 0 a's)
          (funcall 'neovm--adv-product-run t1 'even '(even) t2 'q0 '(q1) '(?a ?b) "ab" 'union)         ;; t (ends b)
          (funcall 'neovm--adv-product-run t1 'even '(even) t2 'q0 '(q1) '(?a ?b) "aa" 'union)         ;; t (even a's)
          (funcall 'neovm--adv-product-run t1 'even '(even) t2 'q0 '(q1) '(?a ?b) "a" 'union)          ;; nil (odd, ends a)
          ))
    (fmakunbound 'neovm--adv-product-run)))"#;
    let expect = expect_test::expect![[r#""OK (t t nil nil nil (even) (q1) (even) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Language emptiness check: BFS reachability to accept state
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_automata_adv_language_emptiness() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--adv-language-empty
    (lambda (trans start accepts alphabet)
      "Check if the language of a DFA is empty (no accepting state reachable).
       Returns t if empty, nil if non-empty."
      (let ((visited (make-hash-table))
            (queue (list start))
            (found nil))
        (puthash start t visited)
        (while (and queue (not found))
          (let ((s (car queue)))
            (setq queue (cdr queue))
            (when (memq s accepts)
              (setq found t))
            (dolist (ch alphabet)
              (let ((next (gethash (cons s ch) trans)))
                (when (and next (not (gethash next visited)))
                  (puthash next t visited)
                  (setq queue (append queue (list next))))))))
        (not found))))

  (unwind-protect
      (list
        ;; Non-empty: DFA accepting "a"
        (let ((t1 (make-hash-table :test 'equal)))
          (puthash '(q0 . ?a) 'q1 t1) (puthash '(q0 . ?b) 'q0 t1)
          (puthash '(q1 . ?a) 'q1 t1) (puthash '(q1 . ?b) 'q1 t1)
          (funcall 'neovm--adv-language-empty t1 'q0 '(q1) '(?a ?b)))
        ;; Empty: no transitions lead to accept state
        (let ((t2 (make-hash-table :test 'equal)))
          (puthash '(q0 . ?a) 'q0 t2) (puthash '(q0 . ?b) 'q0 t2)
          (funcall 'neovm--adv-language-empty t2 'q0 '(q1) '(?a ?b)))
        ;; Non-empty: start is accepting
        (let ((t3 (make-hash-table :test 'equal)))
          (puthash '(q0 . ?a) 'q0 t3)
          (funcall 'neovm--adv-language-empty t3 'q0 '(q0) '(?a)))
        ;; Empty: accept state exists but unreachable
        (let ((t4 (make-hash-table :test 'equal)))
          (puthash '(q0 . ?a) 'q1 t4) (puthash '(q1 . ?a) 'q0 t4)
          (puthash '(q2 . ?a) 'q2 t4)  ;; q2 unreachable
          (funcall 'neovm--adv-language-empty t4 'q0 '(q2) '(?a))))
    (fmakunbound 'neovm--adv-language-empty)))"#;
    let expect = expect_test::expect![[r#""OK (nil t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// DFA equivalence check via symmetric difference
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_automata_adv_dfa_equivalence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Two DFAs are equivalent iff L(A1) symmetric-diff L(A2) is empty
  ;; We check via product construction: find reachable (s1,s2) where
  ;; exactly one is accepting

  (fset 'neovm--adv-dfa-equiv
    (lambda (t1 s1 acc1 t2 s2 acc2 alphabet)
      "Check DFA equivalence via BFS on product automaton.
       Returns t if equivalent."
      (let ((visited (make-hash-table :test 'equal))
            (queue (list (cons s1 s2)))
            (equiv t))
        (puthash (cons s1 s2) t visited)
        (while (and queue equiv)
          (let* ((pair (car queue))
                 (p1 (car pair)) (p2 (cdr pair)))
            (setq queue (cdr queue))
            ;; Check disagreement
            (let ((a1 (if (memq p1 acc1) t nil))
                  (a2 (if (memq p2 acc2) t nil)))
              (unless (eq a1 a2)
                (setq equiv nil)))
            (when equiv
              (dolist (ch alphabet)
                (let ((n1 (gethash (cons p1 ch) t1))
                      (n2 (gethash (cons p2 ch) t2)))
                  (when (and n1 n2)
                    (let ((np (cons n1 n2)))
                      (unless (gethash np visited)
                        (puthash np t visited)
                        (setq queue (append queue (list np)))))))))))
        equiv)))

  (unwind-protect
      (list
        ;; Equivalent: same DFA
        (let ((ta (make-hash-table :test 'equal))
              (tb (make-hash-table :test 'equal)))
          (puthash '(q0 . ?a) 'q1 ta) (puthash '(q0 . ?b) 'q0 ta)
          (puthash '(q1 . ?a) 'q1 ta) (puthash '(q1 . ?b) 'q1 ta)
          (puthash '(r0 . ?a) 'r1 tb) (puthash '(r0 . ?b) 'r0 tb)
          (puthash '(r1 . ?a) 'r1 tb) (puthash '(r1 . ?b) 'r1 tb)
          (funcall 'neovm--adv-dfa-equiv ta 'q0 '(q1) tb 'r0 '(r1) '(?a ?b)))
        ;; Not equivalent: one accepts "a*", other accepts "a+"
        (let ((ta (make-hash-table :test 'equal))
              (tb (make-hash-table :test 'equal)))
          ;; DFA for a*: q0(accept) --a--> q0
          (puthash '(q0 . ?a) 'q0 ta)
          ;; DFA for a+: r0(non-accept) --a--> r1(accept) --a--> r1
          (puthash '(r0 . ?a) 'r1 tb)
          (puthash '(r1 . ?a) 'r1 tb)
          (funcall 'neovm--adv-dfa-equiv ta 'q0 '(q0) tb 'r0 '(r1) '(?a)))
        ;; Equivalent: both accept everything
        (let ((ta (make-hash-table :test 'equal))
              (tb (make-hash-table :test 'equal)))
          (puthash '(q . ?a) 'q ta) (puthash '(q . ?b) 'q ta)
          (puthash '(r . ?a) 'r tb) (puthash '(r . ?b) 'r tb)
          (funcall 'neovm--adv-dfa-equiv ta 'q '(q) tb 'r '(r) '(?a ?b))))
    (fmakunbound 'neovm--adv-dfa-equiv)))"#;
    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Pumping lemma test structure
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_automata_adv_pumping_lemma() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Pumping lemma test: for a regular language with pumping length p,
  ;; any string s with |s| >= p can be split into xyz where:
  ;; 1. |xy| <= p
  ;; 2. |y| > 0
  ;; 3. xy^i z is in the language for all i >= 0
  ;; We verify this property for a known regular language.

  (fset 'neovm--adv-pump-test
    (lambda (accepts-fn p s)
      "Test pumping lemma on string S with pumping length P.
       Try all valid splits xyz. Returns t if some split satisfies pumping."
      (if (< (length s) p)
          'too-short
        (let ((found nil) (x-len 0))
          ;; Try all splits: x = s[0..x-len), y = s[x-len..x-len+y-len), z = rest
          (while (and (not found) (<= x-len p))
            (let ((y-len 1))
              (while (and (not found) (<= (+ x-len y-len) p) (<= (+ x-len y-len) (length s)))
                (let* ((x (substring s 0 x-len))
                       (y (substring s x-len (+ x-len y-len)))
                       (z (substring s (+ x-len y-len)))
                       ;; Test pumping: xy^0z, xy^1z, xy^2z, xy^3z
                       (all-ok t))
                  (dolist (i '(0 1 2 3 4))
                    (let ((pumped (concat x
                                          (let ((r "") (j 0))
                                            (while (< j i)
                                              (setq r (concat r y))
                                              (setq j (1+ j)))
                                            r)
                                          z)))
                      (unless (funcall accepts-fn pumped)
                        (setq all-ok nil))))
                  (when all-ok (setq found (list x y z))))
                (setq y-len (1+ y-len))))
            (setq x-len (1+ x-len)))
          (if found (cons t found) nil)))))

  ;; Language: strings over {a,b} containing "ab"
  (fset 'neovm--adv-pump-contains-ab
    (lambda (s)
      (let ((found nil) (i 0) (len (1- (length s))))
        (while (and (not found) (< i len))
          (when (and (= (aref s i) ?a) (= (aref s (1+ i)) ?b))
            (setq found t))
          (setq i (1+ i)))
        found)))

  (unwind-protect
      (list
        ;; Test with p=3 (the DFA has 3 states)
        ;; Strings of length >= 3 that contain "ab" should be pumpable
        (funcall 'neovm--adv-pump-test 'neovm--adv-pump-contains-ab 3 "aab")
        (funcall 'neovm--adv-pump-test 'neovm--adv-pump-contains-ab 3 "abb")
        (funcall 'neovm--adv-pump-test 'neovm--adv-pump-contains-ab 3 "bab")
        (funcall 'neovm--adv-pump-test 'neovm--adv-pump-contains-ab 3 "abab")
        ;; String too short
        (funcall 'neovm--adv-pump-test 'neovm--adv-pump-contains-ab 3 "ab"))
    (fmakunbound 'neovm--adv-pump-test)
    (fmakunbound 'neovm--adv-pump-contains-ab)))"#;
    let expect = expect_test::expect![[
        r#""OK ((t \"\" \"a\" \"ab\") (t \"a\" \"b\" \"b\") (t \"\" \"b\" \"ab\") (t \"\" \"a\" \"bab\") too-short)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Myhill-Nerode equivalence classes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_automata_adv_myhill_nerode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Myhill-Nerode: two strings x, y are equivalent (x ~ y) if for all z,
  ;; xz in L <=> yz in L.
  ;; We approximate by testing suffixes up to a given length.

  (fset 'neovm--adv-mn-equiv
    (lambda (accepts-fn x y suffixes)
      "Check if X and Y are Myhill-Nerode equivalent wrt SUFFIXES."
      (let ((equiv t))
        (dolist (z suffixes)
          (let ((xz-in (funcall accepts-fn (concat x z)))
                (yz-in (funcall accepts-fn (concat y z))))
            (unless (eq (not (not xz-in)) (not (not yz-in)))
              (setq equiv nil))))
        equiv)))

  (fset 'neovm--adv-mn-classes
    (lambda (accepts-fn strings suffixes)
      "Partition STRINGS into Myhill-Nerode equivalence classes."
      (let ((classes nil))
        (dolist (s strings)
          (let ((found nil))
            (dolist (cls classes)
              (unless found
                (when (funcall 'neovm--adv-mn-equiv
                               accepts-fn s (car cls) suffixes)
                  (setcdr (last cls) (list s))
                  (setq found t))))
            (unless found
              (setq classes (cons (list s) classes)))))
        classes)))

  ;; DFA for "even number of a's" over {a, b}
  (fset 'neovm--adv-mn-even-a
    (lambda (s)
      (let ((count 0) (i 0) (len (length s)))
        (while (< i len)
          (when (= (aref s i) ?a) (setq count (1+ count)))
          (setq i (1+ i)))
        (= 0 (% count 2)))))

  (unwind-protect
      (let ((strings '("" "a" "b" "aa" "ab" "ba" "bb" "aaa" "aab" "aba" "abb"))
            (suffixes '("" "a" "b" "aa" "ab" "ba" "bb")))
        (let ((classes (funcall 'neovm--adv-mn-classes
                                'neovm--adv-mn-even-a strings suffixes)))
          (list
            ;; Number of equivalence classes (should be 2: even-a, odd-a)
            (length classes)
            ;; Sort classes for deterministic output
            (mapcar (lambda (cls)
                      (sort cls 'string<))
                    (sort classes
                          (lambda (a b) (string< (car a) (car b)))))
            ;; Specific equivalence checks
            (funcall 'neovm--adv-mn-equiv 'neovm--adv-mn-even-a "" "aa" suffixes)   ;; t (both even)
            (funcall 'neovm--adv-mn-equiv 'neovm--adv-mn-even-a "" "a" suffixes)    ;; nil (even vs odd)
            (funcall 'neovm--adv-mn-equiv 'neovm--adv-mn-even-a "a" "aaa" suffixes) ;; t (both odd)
            (funcall 'neovm--adv-mn-equiv 'neovm--adv-mn-even-a "b" "bb" suffixes)  ;; t (both even)
            )))
    (fmakunbound 'neovm--adv-mn-equiv)
    (fmakunbound 'neovm--adv-mn-classes)
    (fmakunbound 'neovm--adv-mn-even-a)))"#;
    let expect = expect_test::expect![[
        r#""OK (2 ((\"\" \"aa\" \"aab\" \"aba\" \"b\" \"bb\") (\"a\" \"aaa\" \"ab\" \"abb\" \"ba\")) t nil t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
