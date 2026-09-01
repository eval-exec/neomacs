//! Oracle parity tests for various automaton implementations in Elisp.
//!
//! Tests DFA simulation with trace, NFA-to-DFA conversion (subset construction),
//! pushdown automaton for balanced parentheses with nesting depth,
//! cellular automaton (Rule 30 and Rule 110), and Mealy/Moore machine
//! implementations.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// DFA simulation: recognizing binary multiples of 5
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_auto_dfa_multiples_of_5() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // DFA with 5 states representing remainder mod 5 when reading binary MSB-first.
    // Transition: new_state = (old_state * 2 + bit) mod 5
    // State 0 is accepting (remainder = 0).
    let form = r#"(progn
  (defvar neovm--auto-dfa5-trans (make-hash-table :test 'equal))

  ;; Build transition table for all 5 states x {0,1}
  (let ((s 0))
    (while (< s 5)
      (puthash (cons s 0) (% (* s 2) 5) neovm--auto-dfa5-trans)
      (puthash (cons s 1) (% (1+ (* s 2)) 5) neovm--auto-dfa5-trans)
      (setq s (1+ s))))

  (fset 'neovm--auto-dfa5-run
    (lambda (binary-str)
      "Run DFA. Return (accepted final-state trace)."
      (let ((state 0)
            (trace nil)
            (i 0)
            (len (length binary-str)))
        (while (< i len)
          (let* ((bit (- (aref binary-str i) ?0))
                 (new (gethash (cons state bit) neovm--auto-dfa5-trans)))
            (setq trace (cons (list state bit new) trace))
            (setq state new))
          (setq i (1+ i)))
        (list (= state 0) state (nreverse trace)))))

  (fset 'neovm--auto-int-to-bin
    (lambda (n)
      "Convert non-negative integer to binary string."
      (if (= n 0) "0"
        (let ((result nil)
              (num n))
          (while (> num 0)
            (setq result (cons (+ ?0 (% num 2)) result))
            (setq num (/ num 2)))
          (concat result)))))

  (unwind-protect
      (list
        ;; Direct binary string tests
        (car (funcall 'neovm--auto-dfa5-run "0"))       ;; 0 div by 5: t
        (car (funcall 'neovm--auto-dfa5-run "101"))     ;; 5: t
        (car (funcall 'neovm--auto-dfa5-run "1010"))    ;; 10: t
        (car (funcall 'neovm--auto-dfa5-run "1111"))    ;; 15: t
        (car (funcall 'neovm--auto-dfa5-run "10100"))   ;; 20: t
        (car (funcall 'neovm--auto-dfa5-run "11"))      ;; 3: nil
        (car (funcall 'neovm--auto-dfa5-run "111"))     ;; 7: nil
        (car (funcall 'neovm--auto-dfa5-run "1001"))    ;; 9: nil
        ;; Verify against modular arithmetic for 0..30
        (let ((all-match t))
          (let ((n 0))
            (while (<= n 30)
              (let* ((bin (funcall 'neovm--auto-int-to-bin n))
                     (dfa-result (car (funcall 'neovm--auto-dfa5-run bin)))
                     (mod-result (= (% n 5) 0)))
                (unless (eq dfa-result mod-result)
                  (setq all-match nil)))
              (setq n (1+ n))))
          all-match)
        ;; Full trace for "1010" (decimal 10)
        (nth 2 (funcall 'neovm--auto-dfa5-run "1010")))
    (fmakunbound 'neovm--auto-dfa5-run)
    (fmakunbound 'neovm--auto-int-to-bin)
    (makunbound 'neovm--auto-dfa5-trans)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t nil nil nil t ((0 1 1) (1 0 2) (2 1 0) (0 0 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// NFA to DFA conversion via subset construction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_auto_nfa_to_dfa_subset_construction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // NFA for "a(b|c)*d": start->a->loop(b|c)->d->accept
    // Convert to DFA using subset construction, then run both and compare.
    let form = r#"(progn
  ;; NFA representation: hash-table (state . char-or-nil) -> list-of-states
  ;; nil key = epsilon transition
  (defvar neovm--auto-nfa (make-hash-table :test 'equal))

  ;; NFA states: 0=start, 1=after-a, 2=loop, 3=after-d(accept)
  ;; 0 --a--> 1
  ;; 1 --eps--> 2
  ;; 2 --b--> 2, 2 --c--> 2
  ;; 2 --d--> 3
  (puthash '(0 . ?a) '(1) neovm--auto-nfa)
  (puthash '(1 . nil) '(2) neovm--auto-nfa)
  (puthash '(2 . ?b) '(2) neovm--auto-nfa)
  (puthash '(2 . ?c) '(2) neovm--auto-nfa)
  (puthash '(2 . ?d) '(3) neovm--auto-nfa)

  (fset 'neovm--auto-eps-closure
    (lambda (states nfa)
      (let ((result (copy-sequence states))
            (work (copy-sequence states)))
        (while work
          (let ((s (car work)))
            (setq work (cdr work))
            (let ((targets (gethash (cons s nil) nfa)))
              (dolist (t2 targets)
                (unless (memq t2 result)
                  (setq result (cons t2 result))
                  (setq work (cons t2 work)))))))
        (sort result #'<))))

  (fset 'neovm--auto-nfa-move
    (lambda (states ch nfa)
      (let ((result nil))
        (dolist (s states)
          (dolist (t2 (gethash (cons s ch) nfa))
            (unless (memq t2 result)
              (setq result (cons t2 result)))))
        result)))

  (fset 'neovm--auto-subset-construct
    (lambda (nfa alphabet accept-states)
      "Convert NFA to DFA. Returns (dfa-trans dfa-start dfa-accepts state-map)."
      (let* ((start (funcall 'neovm--auto-eps-closure '(0) nfa))
             (dfa-trans (make-hash-table :test 'equal))
             (state-map (make-hash-table :test 'equal))
             (next-id 0)
             (worklist (list start))
             (dfa-accepts nil))
        ;; Register start state
        (puthash (prin1-to-string start) next-id state-map)
        (setq next-id (1+ next-id))
        (while worklist
          (let ((current (car worklist)))
            (setq worklist (cdr worklist))
            (let ((current-id (gethash (prin1-to-string current) state-map)))
              ;; Check if this DFA state is accepting
              (let ((is-accept nil))
                (dolist (a accept-states)
                  (when (memq a current)
                    (setq is-accept t)))
                (when is-accept
                  (setq dfa-accepts (cons current-id dfa-accepts))))
              ;; For each input symbol
              (dolist (ch alphabet)
                (let* ((moved (funcall 'neovm--auto-nfa-move current ch nfa))
                       (closed (funcall 'neovm--auto-eps-closure moved nfa)))
                  (when closed
                    (let ((key (prin1-to-string closed)))
                      (unless (gethash key state-map)
                        (puthash key next-id state-map)
                        (setq next-id (1+ next-id))
                        (setq worklist (cons closed worklist)))
                      (puthash (cons current-id ch)
                               (gethash key state-map)
                               dfa-trans))))))))
        (list dfa-trans
              (gethash (prin1-to-string start) state-map)
              dfa-accepts
              next-id))))

  (fset 'neovm--auto-run-dfa
    (lambda (dfa-trans start accepts input)
      (let ((state start)
            (i 0)
            (len (length input))
            (valid t))
        (while (and (< i len) valid)
          (let ((next (gethash (cons state (aref input i)) dfa-trans)))
            (if next
                (setq state next)
              (setq valid nil)))
          (setq i (1+ i)))
        (and valid (memq state accepts)))))

  (fset 'neovm--auto-run-nfa
    (lambda (nfa accept-states input)
      (let ((current (funcall 'neovm--auto-eps-closure '(0) nfa))
            (i 0)
            (len (length input)))
        (while (< i len)
          (setq current
                (funcall 'neovm--auto-eps-closure
                         (funcall 'neovm--auto-nfa-move current (aref input i) nfa)
                         nfa))
          (setq i (1+ i)))
        (let ((accepted nil))
          (dolist (s current)
            (when (memq s accept-states)
              (setq accepted t)))
          accepted))))

  (unwind-protect
      (let* ((result (funcall 'neovm--auto-subset-construct
                               neovm--auto-nfa '(?a ?b ?c ?d) '(3)))
             (dfa-trans (nth 0 result))
             (dfa-start (nth 1 result))
             (dfa-accepts (nth 2 result))
             (num-dfa-states (nth 3 result)))
        (let ((test-inputs '("ad" "abd" "acd" "abcd" "abcbcd" "abbbbbd"
                              "acccccd" "abcbcbcbcd"
                              "" "a" "d" "ab" "abc" "bd" "aad" "add" "abcda")))
          (list
            ;; Number of DFA states
            num-dfa-states
            ;; Run all inputs through both NFA and DFA, verify same result
            (mapcar (lambda (input)
                      (let ((nfa-r (funcall 'neovm--auto-run-nfa
                                            neovm--auto-nfa '(3) input))
                            (dfa-r (funcall 'neovm--auto-run-dfa
                                            dfa-trans dfa-start dfa-accepts input)))
                        (list input
                              (if nfa-r t nil)
                              (if dfa-r t nil)
                              (eq (not (not nfa-r)) (not (not dfa-r))))))
                    test-inputs))))
    (fmakunbound 'neovm--auto-eps-closure)
    (fmakunbound 'neovm--auto-nfa-move)
    (fmakunbound 'neovm--auto-subset-construct)
    (fmakunbound 'neovm--auto-run-dfa)
    (fmakunbound 'neovm--auto-run-nfa)
    (makunbound 'neovm--auto-nfa)))"#;
    let expect = expect_test::expect![[
        r#""OK (4 ((\"ad\" t t t) (\"abd\" t t t) (\"acd\" t t t) (\"abcd\" t t t) (\"abcbcd\" t t t) (\"abbbbbd\" t t t) (\"acccccd\" t t t) (\"abcbcbcbcd\" t t t) (\"\" nil nil t) (\"a\" nil nil t) (\"d\" nil nil t) (\"ab\" nil nil t) (\"abc\" nil nil t) (\"bd\" nil nil t) (\"aad\" nil nil t) (\"add\" nil nil t) (\"abcda\" nil nil t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Pushdown automaton for balanced parentheses with depth tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_auto_pda_balanced_parens() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // PDA that validates multiple bracket types: (), [], {}.
    // Also computes max nesting depth and bracket counts.
    let form = r#"(progn
  (fset 'neovm--auto-pda-check
    (lambda (input)
      "Validate balanced brackets. Return (valid max-depth counts error-info)."
      (let ((stack nil)
            (depth 0)
            (max-depth 0)
            (counts (list (cons ?\( 0) (cons ?\[ 0) (cons ?\{ 0)))
            (i 0)
            (len (length input))
            (valid t)
            (error-info nil)
            (matching (list (cons ?\) ?\()
                            (cons ?\] ?\[)
                            (cons ?\} ?\{))))
        (while (and (< i len) valid)
          (let ((ch (aref input i)))
            (cond
              ;; Opening bracket
              ((memq ch '(?\( ?\[ ?\{))
               (setq stack (cons ch stack))
               (setq depth (1+ depth))
               (when (> depth max-depth) (setq max-depth depth))
               (let ((pair (assq ch counts)))
                 (when pair (setcdr pair (1+ (cdr pair))))))
              ;; Closing bracket
              ((memq ch '(?\) ?\] ?\}))
               (let ((expected (cdr (assq ch matching))))
                 (cond
                   ((null stack)
                    (setq valid nil)
                    (setq error-info (format "unexpected '%c' at pos %d" ch i)))
                   ((not (= (car stack) expected))
                    (setq valid nil)
                    (setq error-info
                          (format "mismatch at pos %d: expected '%c' got '%c'"
                                  i (car stack) ch)))
                   (t
                    (setq stack (cdr stack))
                    (setq depth (1- depth))))))))
          (setq i (1+ i)))
        (when (and valid stack)
          (setq valid nil)
          (setq error-info (format "unclosed '%c' (%d remaining)"
                                   (car stack) (length stack))))
        (list valid max-depth counts error-info))))

  (unwind-protect
      (list
        ;; Valid cases
        (funcall 'neovm--auto-pda-check "()")
        (funcall 'neovm--auto-pda-check "[]")
        (funcall 'neovm--auto-pda-check "{}")
        (funcall 'neovm--auto-pda-check "([]{})")
        (funcall 'neovm--auto-pda-check "((([])))")
        (funcall 'neovm--auto-pda-check "{[()()]}")
        (funcall 'neovm--auto-pda-check "()[]{}()[]")
        (funcall 'neovm--auto-pda-check "")
        ;; With non-bracket chars (ignored)
        (funcall 'neovm--auto-pda-check "(a + b) * [c - d]")
        (funcall 'neovm--auto-pda-check "f(x, g(y, z))")
        ;; Invalid cases
        (funcall 'neovm--auto-pda-check "(")
        (funcall 'neovm--auto-pda-check ")")
        (funcall 'neovm--auto-pda-check "(]")
        (funcall 'neovm--auto-pda-check "([)]")
        (funcall 'neovm--auto-pda-check "((())")
        ;; Deep nesting
        (funcall 'neovm--auto-pda-check "(((((((()))))))"))
    (fmakunbound 'neovm--auto-pda-check)))"#;
    let expect = expect_test::expect![[
        r#""OK ((t 1 ((40 . 1) (91 . 0) (123 . 0)) nil) (t 1 ((40 . 0) (91 . 1) (123 . 0)) nil) (t 1 ((40 . 0) (91 . 0) (123 . 1)) nil) (t 2 ((40 . 1) (91 . 1) (123 . 1)) nil) (t 4 ((40 . 3) (91 . 1) (123 . 0)) nil) (t 3 ((40 . 2) (91 . 1) (123 . 1)) nil) (t 1 ((40 . 2) (91 . 2) (123 . 1)) nil) (t 0 ((40 . 0) (91 . 0) (123 . 0)) nil) (t 1 ((40 . 1) (91 . 1) (123 . 0)) nil) (t 2 ((40 . 2) (91 . 0) (123 . 0)) nil) (nil 1 ((40 . 1) (91 . 0) (123 . 0)) \"unclosed '(' (1 remaining)\") (nil 0 ((40 . 0) (91 . 0) (123 . 0)) \"unexpected ')' at pos 0\") (nil 1 ((40 . 1) (91 . 0) (123 . 0)) \"mismatch at pos 1: expected '(' got ']'\") (nil 2 ((40 . 1) (91 . 1) (123 . 0)) \"mismatch at pos 2: expected '[' got ')'\") (nil 3 ((40 . 3) (91 . 0) (123 . 0)) \"unclosed '(' (1 remaining)\") (nil 8 ((40 . 8) (91 . 0) (123 . 0)) \"unclosed '(' (1 remaining)\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Elementary cellular automaton: Rule 30
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_auto_cellular_rule30() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Rule 30: one of Wolfram's elementary CAs.
    // Rule number encodes the 8 possible 3-cell neighborhoods:
    // neighborhood -> new state via binary digits of rule number.
    let form = r#"(progn
  (fset 'neovm--auto-ca-step
    (lambda (cells rule)
      "Apply one step of elementary CA with given rule number.
       CELLS is a vector of 0/1. Wrapping boundary."
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

  (fset 'neovm--auto-ca-run
    (lambda (initial rule steps)
      "Run CA for STEPS steps. Return list of all states including initial."
      (let ((history (list (append initial nil)))
            (cells (copy-sequence initial)))
        (dotimes (_ steps)
          (setq cells (funcall 'neovm--auto-ca-step cells rule))
          (setq history (cons (append cells nil) history)))
        (nreverse history))))

  (fset 'neovm--auto-ca-density
    (lambda (cells)
      "Return fraction of cells that are 1, as (alive . total)."
      (let ((alive 0))
        (dotimes (i (length cells))
          (when (= (aref cells i) 1)
            (setq alive (1+ alive))))
        (cons alive (length cells)))))

  (unwind-protect
      (let* ((width 15)
             (init (make-vector width 0)))
        ;; Single cell in center
        (aset init (/ width 2) 1)
        ;; Run Rule 30 for 7 steps
        (let ((r30-history (funcall 'neovm--auto-ca-run init 30 7)))
          ;; Run Rule 90 (Sierpinski triangle) for comparison
          (let ((r90-history (funcall 'neovm--auto-ca-run init 90 7)))
            (list
              ;; Rule 30 history (list of lists)
              r30-history
              ;; Rule 90 history
              r90-history
              ;; Density of final states
              (funcall 'neovm--auto-ca-density
                       (vconcat (car (last r30-history))))
              (funcall 'neovm--auto-ca-density
                       (vconcat (car (last r90-history))))
              ;; Rule 30 is not symmetric (unlike Rule 90)
              ;; Check: is row 7 a palindrome?
              (let ((row (car (last r30-history))))
                (equal row (reverse row)))
              (let ((row (car (last r90-history))))
                (equal row (reverse row)))))))
    (fmakunbound 'neovm--auto-ca-step)
    (fmakunbound 'neovm--auto-ca-run)
    (fmakunbound 'neovm--auto-ca-density)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 0 0 0 0 0 0 1 0 0 0 0 0 0 0) (0 0 0 0 0 0 1 1 1 0 0 0 0 0 0) (0 0 0 0 0 1 1 0 0 1 0 0 0 0 0) (0 0 0 0 1 1 0 1 1 1 1 0 0 0 0) (0 0 0 1 1 0 0 1 0 0 0 1 0 0 0) (0 0 1 1 0 1 1 1 1 0 1 1 1 0 0) (0 1 1 0 0 1 0 0 0 0 1 0 0 1 0) (1 1 0 1 1 1 1 0 0 1 1 1 1 1 1)) ((0 0 0 0 0 0 0 1 0 0 0 0 0 0 0) (0 0 0 0 0 0 1 0 1 0 0 0 0 0 0) (0 0 0 0 0 1 0 0 0 1 0 0 0 0 0) (0 0 0 0 1 0 1 0 1 0 1 0 0 0 0) (0 0 0 1 0 0 0 0 0 0 0 1 0 0 0) (0 0 1 0 1 0 0 0 0 0 1 0 1 0 0) (0 1 0 0 0 1 0 0 0 1 0 0 0 1 0) (1 0 1 0 1 0 1 0 1 0 1 0 1 0 1)) (12 . 15) (8 . 15) nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Elementary cellular automaton: Rule 110 (Turing-complete)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_auto_cellular_rule110() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Rule 110 is known to be Turing-complete.
    // Run from a specific initial condition and verify deterministic evolution.
    let form = r#"(progn
  (fset 'neovm--auto-r110-step
    (lambda (cells)
      "Apply Rule 110 to cells vector."
      (let* ((n (length cells))
             (next (make-vector n 0)))
        (dotimes (i n)
          (let* ((l (aref cells (% (+ i n -1) n)))
                 (c (aref cells i))
                 (r (aref cells (% (+ i 1) n)))
                 (nb (+ (* l 4) (* c 2) r))
                 ;; Rule 110 = 01101110 in binary
                 (new (if (= (logand 110 (ash 1 nb)) 0) 0 1)))
            (aset next i new)))
        next)))

  (fset 'neovm--auto-r110-to-string
    (lambda (cells)
      (let ((chars nil))
        (dotimes (i (length cells))
          (setq chars (cons (if (= (aref cells i) 1) ?# ?.) chars)))
        (concat (nreverse chars)))))

  (fset 'neovm--auto-r110-population
    (lambda (cells)
      (let ((count 0))
        (dotimes (i (length cells))
          (when (= (aref cells i) 1)
            (setq count (1+ count))))
        count)))

  (unwind-protect
      (let* ((width 21)
             (init (make-vector width 0)))
        ;; Start with rightmost cell set (common Rule 110 setup)
        (aset init (1- width) 1)
        ;; Run for 12 steps
        (let ((cells (copy-sequence init))
              (history nil)
              (populations nil))
          (setq history (cons (funcall 'neovm--auto-r110-to-string cells) history))
          (setq populations (cons (funcall 'neovm--auto-r110-population cells) populations))
          (dotimes (_ 12)
            (setq cells (funcall 'neovm--auto-r110-step cells))
            (setq history (cons (funcall 'neovm--auto-r110-to-string cells) history))
            (setq populations (cons (funcall 'neovm--auto-r110-population cells) populations)))
          (list
            ;; All 13 generations as strings
            (nreverse history)
            ;; Population counts per generation
            (nreverse populations)
            ;; Final generation
            (funcall 'neovm--auto-r110-to-string cells)
            ;; Is final state different from initial?
            (not (equal init cells)))))
    (fmakunbound 'neovm--auto-r110-step)
    (fmakunbound 'neovm--auto-r110-to-string)
    (fmakunbound 'neovm--auto-r110-population)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"....................#\" \"...................##\" \"..................###\" \".................##.#\" \"................#####\" \"...............##...#\" \"..............###..##\" \".............##.#.###\" \"............#######.#\" \"...........##.....###\" \"..........###....##.#\" \".........##.#...#####\" \"........#####..##...#\") (1 2 3 3 5 3 5 6 8 5 6 8 8) \"........#####..##...#\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Mealy machine: serial parity checker with error detection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_auto_mealy_parity_checker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Mealy machine outputs parity bit after each input bit.
    // States: even-parity and odd-parity.
    // Also implements a packet framer: START + data + parity + END.
    let form = r#"(progn
  (fset 'neovm--auto-mealy-parity
    (lambda (bits)
      "Mealy machine: output running parity (XOR) after each bit.
       State: 0=even, 1=odd. Output on transition."
      (let ((state 0)
            (outputs nil)
            (i 0)
            (len (length bits)))
        (while (< i len)
          (let* ((bit (- (aref bits i) ?0))
                 (new-state (logxor state bit)))
            (setq outputs (cons new-state outputs))
            (setq state new-state))
          (setq i (1+ i)))
        (list (nreverse outputs) state))))

  (fset 'neovm--auto-mealy-frame
    (lambda (data-bits)
      "Frame data bits with start marker, even parity bit, and end marker.
       Format: S <data> P E where S=start, P=parity, E=end."
      (let* ((parity-result (funcall 'neovm--auto-mealy-parity data-bits))
             (final-parity (nth 1 parity-result))
             (parity-bit (if (= final-parity 0) "0" "1")))
        (concat "S" data-bits parity-bit "E"))))

  (fset 'neovm--auto-mealy-verify-frame
    (lambda (frame)
      "Verify a framed packet. Return (valid data parity-ok)."
      (if (and (> (length frame) 3)
               (= (aref frame 0) ?S)
               (= (aref frame (1- (length frame))) ?E))
          (let* ((payload (substring frame 1 (1- (length frame))))
                 (data (substring payload 0 (1- (length payload))))
                 (sent-parity (- (aref payload (1- (length payload))) ?0))
                 (check (funcall 'neovm--auto-mealy-parity data))
                 (computed-parity (nth 1 check)))
            (list t data (= sent-parity computed-parity)))
        (list nil nil nil))))

  (unwind-protect
      (list
        ;; Basic parity computation
        (funcall 'neovm--auto-mealy-parity "0")
        (funcall 'neovm--auto-mealy-parity "1")
        (funcall 'neovm--auto-mealy-parity "11")
        (funcall 'neovm--auto-mealy-parity "110")
        (funcall 'neovm--auto-mealy-parity "1010")
        (funcall 'neovm--auto-mealy-parity "11111")
        (funcall 'neovm--auto-mealy-parity "10101010")
        ;; Framing
        (funcall 'neovm--auto-mealy-frame "1010")
        (funcall 'neovm--auto-mealy-frame "1111")
        (funcall 'neovm--auto-mealy-frame "0000")
        ;; Verify valid frames
        (funcall 'neovm--auto-mealy-verify-frame
                 (funcall 'neovm--auto-mealy-frame "1010"))
        (funcall 'neovm--auto-mealy-verify-frame
                 (funcall 'neovm--auto-mealy-frame "11001"))
        ;; Verify corrupted frame (flip a data bit)
        (let ((frame (funcall 'neovm--auto-mealy-frame "1010")))
          ;; Corrupt: flip bit at index 1 (first data bit)
          (let ((corrupted (concat frame)))
            (aset corrupted 1 (if (= (aref corrupted 1) ?0) ?1 ?0))
            (funcall 'neovm--auto-mealy-verify-frame corrupted)))
        ;; Invalid frame format
        (funcall 'neovm--auto-mealy-verify-frame "XYZW"))
    (fmakunbound 'neovm--auto-mealy-parity)
    (fmakunbound 'neovm--auto-mealy-frame)
    (fmakunbound 'neovm--auto-mealy-verify-frame)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0) 0) ((1) 1) ((1 0) 0) ((1 0 0) 0) ((1 1 0 0) 0) ((1 0 1 0 1) 1) ((1 1 0 0 1 1 0 0) 0) \"S10100E\" \"S11110E\" \"S00000E\" (t \"1010\" t) (t \"11001\" t) (t \"0010\" nil) (nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Moore machine: traffic light controller with timing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_auto_moore_traffic_light() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Moore machine for traffic light: output depends only on state.
    // States: green, yellow, red, left-arrow.
    // Inputs: tick (timer expired), emergency, clear-emergency.
    let form = r#"(progn
  (defvar neovm--auto-tl-trans (make-hash-table :test 'equal))
  (defvar neovm--auto-tl-output (make-hash-table :test 'eq))
  (defvar neovm--auto-tl-duration (make-hash-table :test 'eq))

  ;; Normal cycle: green -> yellow -> red -> left-arrow -> green
  (puthash '(green . tick) 'yellow neovm--auto-tl-trans)
  (puthash '(yellow . tick) 'red neovm--auto-tl-trans)
  (puthash '(red . tick) 'left-arrow neovm--auto-tl-trans)
  (puthash '(left-arrow . tick) 'green neovm--auto-tl-trans)
  ;; Emergency: any state -> flashing-red
  (puthash '(green . emergency) 'flashing-red neovm--auto-tl-trans)
  (puthash '(yellow . emergency) 'flashing-red neovm--auto-tl-trans)
  (puthash '(red . emergency) 'flashing-red neovm--auto-tl-trans)
  (puthash '(left-arrow . emergency) 'flashing-red neovm--auto-tl-trans)
  (puthash '(flashing-red . emergency) 'flashing-red neovm--auto-tl-trans)
  ;; Clear emergency -> red (safe restart)
  (puthash '(flashing-red . clear) 'red neovm--auto-tl-trans)
  ;; Tick in flashing-red stays (wait for clear)
  (puthash '(flashing-red . tick) 'flashing-red neovm--auto-tl-trans)

  ;; Outputs (Moore: output = f(state))
  (puthash 'green "GO" neovm--auto-tl-output)
  (puthash 'yellow "CAUTION" neovm--auto-tl-output)
  (puthash 'red "STOP" neovm--auto-tl-output)
  (puthash 'left-arrow "LEFT-TURN" neovm--auto-tl-output)
  (puthash 'flashing-red "EMERGENCY-STOP" neovm--auto-tl-output)

  ;; Durations (ticks per state)
  (puthash 'green 5 neovm--auto-tl-duration)
  (puthash 'yellow 2 neovm--auto-tl-duration)
  (puthash 'red 5 neovm--auto-tl-duration)
  (puthash 'left-arrow 3 neovm--auto-tl-duration)
  (puthash 'flashing-red 1 neovm--auto-tl-duration)

  (fset 'neovm--auto-tl-run
    (lambda (events)
      "Run traffic light through event sequence. Return (final-state outputs log)."
      (let ((state 'green)
            (outputs nil)
            (log nil))
        (dolist (event events)
          (let ((new-state (gethash (cons state event) neovm--auto-tl-trans state)))
            (let ((output (gethash new-state neovm--auto-tl-output "UNKNOWN")))
              (setq log (cons (list state event new-state output) log))
              (setq outputs (cons output outputs))
              (setq state new-state))))
        (list state (nreverse outputs) (nreverse log)))))

  (unwind-protect
      (list
        ;; Normal cycle
        (funcall 'neovm--auto-tl-run
                 '(tick tick tick tick))
        ;; Emergency during green
        (funcall 'neovm--auto-tl-run
                 '(emergency tick tick clear tick))
        ;; Full normal cycle back to green
        (let ((result (funcall 'neovm--auto-tl-run
                               '(tick tick tick tick))))
          (list (car result)
                (nth 1 result)))
        ;; Emergency during yellow, clear, then resume
        (let ((result (funcall 'neovm--auto-tl-run
                               '(tick emergency clear tick tick tick))))
          (list (car result)
                (nth 1 result)))
        ;; Multiple emergencies
        (let ((result (funcall 'neovm--auto-tl-run
                               '(emergency emergency emergency clear tick))))
          (car result))
        ;; Duration table
        (list (gethash 'green neovm--auto-tl-duration)
              (gethash 'yellow neovm--auto-tl-duration)
              (gethash 'red neovm--auto-tl-duration)))
    (fmakunbound 'neovm--auto-tl-run)
    (makunbound 'neovm--auto-tl-trans)
    (makunbound 'neovm--auto-tl-output)
    (makunbound 'neovm--auto-tl-duration)))"#;
    let expect = expect_test::expect![[
        r#""OK ((green (\"CAUTION\" \"STOP\" \"LEFT-TURN\" \"GO\") ((green tick yellow \"CAUTION\") (yellow tick red \"STOP\") (red tick left-arrow \"LEFT-TURN\") (left-arrow tick green \"GO\"))) (left-arrow (\"EMERGENCY-STOP\" \"EMERGENCY-STOP\" \"EMERGENCY-STOP\" \"STOP\" \"LEFT-TURN\") ((green emergency flashing-red \"EMERGENCY-STOP\") (flashing-red tick flashing-red \"EMERGENCY-STOP\") (flashing-red tick flashing-red \"EMERGENCY-STOP\") (flashing-red clear red \"STOP\") (red tick left-arrow \"LEFT-TURN\"))) (green (\"CAUTION\" \"STOP\" \"LEFT-TURN\" \"GO\")) (yellow (\"CAUTION\" \"EMERGENCY-STOP\" \"STOP\" \"LEFT-TURN\" \"GO\" \"CAUTION\")) left-arrow (5 2 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
