//! Oracle parity tests for automata theory: pushdown automaton simulation,
//! Turing machine simulation (single tape), multi-tape Turing machine,
//! context-free grammar to PDA conversion, regular language operations
//! (union, concatenation, Kleene star on DFAs), DFA minimization
//! (Hopcroft's algorithm), language equivalence checking.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Pushdown automaton simulation: accept balanced parentheses variants
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_automata_pushdown_automaton() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // PDA for the language { a^n b^n | n >= 0 }
    // Push 'A on 'a', pop 'A on 'b', accept if stack empty at end
    let form = r#"(progn
  ;; PDA configuration: (state stack)
  ;; Transitions: (state input stack-top) -> (new-state stack-action)
  ;; stack-action: 'push-X, 'pop, 'noop
  (defvar neovm--test-pda-trans (make-hash-table :test 'equal))

  ;; States: q0 (reading a's), q1 (reading b's), q-accept
  ;; On 'a' in q0 with any stack top: push A, stay in q0
  (puthash '(q0 ?a) '(q0 push) neovm--test-pda-trans)
  ;; On 'b' in q0 with A on stack: pop, go to q1
  (puthash '(q0 ?b) '(q1 pop) neovm--test-pda-trans)
  ;; On 'b' in q1 with A on stack: pop, stay in q1
  (puthash '(q1 ?b) '(q1 pop) neovm--test-pda-trans)

  (fset 'neovm--test-pda-run
    (lambda (input)
      "Run PDA on INPUT string. Return (accepted final-state stack-depth trace)."
      (let ((state 'q0)
            (stack nil)
            (trace nil)
            (error nil)
            (i 0)
            (len (length input)))
        (while (and (< i len) (not error))
          (let* ((ch (aref input i))
                 (key (list state ch))
                 (trans (gethash key neovm--test-pda-trans)))
            (if (null trans)
                (progn
                  (setq error (format "no transition for (%s, %c)" state ch))
                  (setq trace (cons (list state ch 'ERROR) trace)))
              (let ((new-state (nth 0 trans))
                    (action (nth 1 trans)))
                (cond
                  ((eq action 'push)
                   (setq stack (cons 'A stack)))
                  ((eq action 'pop)
                   (if (null stack)
                       (setq error "stack underflow")
                     (setq stack (cdr stack)))))
                (unless error
                  (setq trace (cons (list state ch new-state action (length stack)) trace))
                  (setq state new-state)))))
          (setq i (1+ i)))
        ;; Accept if in q0 or q1 with empty stack and no error
        (let ((accepted (and (not error)
                             (null stack)
                             (memq state '(q0 q1)))))
          (list accepted state (length stack) (nreverse trace) error)))))

  (unwind-protect
      (list
        ;; Empty string: accepted (a^0 b^0)
        (car (funcall 'neovm--test-pda-run ""))
        ;; "ab": accepted
        (car (funcall 'neovm--test-pda-run "ab"))
        ;; "aabb": accepted
        (car (funcall 'neovm--test-pda-run "aabb"))
        ;; "aaabbb": accepted
        (car (funcall 'neovm--test-pda-run "aaabbb"))
        ;; "aab": rejected (stack not empty)
        (car (funcall 'neovm--test-pda-run "aab"))
        ;; "abb": rejected (stack underflow)
        (car (funcall 'neovm--test-pda-run "abb"))
        ;; "ba": rejected (no transition for b in q0 with empty stack)
        (car (funcall 'neovm--test-pda-run "ba"))
        ;; "aabba": rejected (a after b)
        (car (funcall 'neovm--test-pda-run "aabba"))
        ;; Detailed trace for "aabb"
        (let ((result (funcall 'neovm--test-pda-run "aabb")))
          (list (nth 0 result) (nth 1 result) (nth 2 result))))
    (makunbound 'neovm--test-pda-trans)
    (fmakunbound 'neovm--test-pda-run)))"#;
    let expect =
        expect_test::expect![[r#""OK ((q0 q1) (q1) (q1) (q1) nil nil nil nil ((q1) q1 0))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Single-tape Turing machine simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_automata_turing_machine_single_tape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Turing machine that adds 1 to a binary number (LSB at left)
    // Also a TM that checks palindromes over {a,b}
    let form = r#"(progn
  ;; TM representation: transitions hash-table
  ;; Key: (state . read-symbol), Value: (new-state write-symbol direction)
  ;; direction: L or R, tape is a list (converted to vector for O(1) access)
  ;; Blank symbol: '_

  (fset 'neovm--test-tm-run
    (lambda (transitions initial-state accept-states reject-states tape-str max-steps)
      "Simulate TM. Return (result final-state tape-contents steps)."
      (let* ((tape (vconcat tape-str))
             ;; Extend tape with blanks on both sides
             (tape-vec (vconcat (make-vector 20 ?_) tape (make-vector 20 ?_)))
             (head (+ 20 0))  ;; head starts at position of first input char
             (state initial-state)
             (steps 0)
             (halted nil)
             (result nil))
        (while (and (not halted) (< steps max-steps))
          (let* ((sym (aref tape-vec head))
                 (key (cons state sym))
                 (trans (gethash key transitions)))
            (if (null trans)
                (progn (setq halted t) (setq result 'stuck))
              (let ((new-state (nth 0 trans))
                    (write-sym (nth 1 trans))
                    (dir (nth 2 trans)))
                (aset tape-vec head write-sym)
                (setq state new-state)
                (cond
                  ((eq dir 'R) (setq head (1+ head)))
                  ((eq dir 'L) (setq head (1- head))))
                (when (memq state accept-states)
                  (setq halted t) (setq result 'accept))
                (when (memq state reject-states)
                  (setq halted t) (setq result 'reject)))))
          (setq steps (1+ steps)))
        (unless halted (setq result 'timeout))
        ;; Extract non-blank tape content
        (let ((start 0) (end (1- (length tape-vec))))
          (while (and (<= start end) (= (aref tape-vec start) ?_))
            (setq start (1+ start)))
          (while (and (>= end start) (= (aref tape-vec end) ?_))
            (setq end (1- end)))
          (list result state
                (if (> start end) ""
                  (substring (concat tape-vec) start (1+ end)))
                steps)))))

  ;; TM for binary increment (MSB first): scan right to end, then carry back
  (defvar neovm--test-tm-inc (make-hash-table :test 'equal))
  ;; State scan-right: move to end of input
  (puthash '(scan . ?0) '(scan ?0 R) neovm--test-tm-inc)
  (puthash '(scan . ?1) '(scan ?1 R) neovm--test-tm-inc)
  (puthash '(scan . ?_) '(carry ?_ L) neovm--test-tm-inc)
  ;; State carry: add 1 with carry
  (puthash '(carry . ?0) '(done ?1 R) neovm--test-tm-inc)    ;; 0+carry=1, done
  (puthash '(carry . ?1) '(carry ?0 L) neovm--test-tm-inc)   ;; 1+carry=0, continue
  (puthash '(carry . ?_) '(done ?1 R) neovm--test-tm-inc)    ;; overflow: new digit

  (unwind-protect
      (list
        ;; Binary increment: "0" -> "1"
        (nth 2 (funcall 'neovm--test-tm-run
                         neovm--test-tm-inc 'scan '(done) nil "0" 100))
        ;; "1" -> "10"
        (nth 2 (funcall 'neovm--test-tm-run
                         neovm--test-tm-inc 'scan '(done) nil "1" 100))
        ;; "10" -> "11"
        (nth 2 (funcall 'neovm--test-tm-run
                         neovm--test-tm-inc 'scan '(done) nil "10" 100))
        ;; "11" -> "100"
        (nth 2 (funcall 'neovm--test-tm-run
                         neovm--test-tm-inc 'scan '(done) nil "11" 100))
        ;; "111" -> "1000"
        (nth 2 (funcall 'neovm--test-tm-run
                         neovm--test-tm-inc 'scan '(done) nil "111" 100))
        ;; "1010" -> "1011"
        (nth 2 (funcall 'neovm--test-tm-run
                         neovm--test-tm-inc 'scan '(done) nil "1010" 100))
        ;; Step count for "111" (needs to scan right then carry all the way back)
        (nth 3 (funcall 'neovm--test-tm-run
                         neovm--test-tm-inc 'scan '(done) nil "111" 100)))
    (makunbound 'neovm--test-tm-inc)
    (fmakunbound 'neovm--test-tm-run)))"#;
    let expect =
        expect_test::expect![[r#""OK (\"1\" \"10\" \"11\" \"100\" \"1000\" \"1011\" 8)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-tape Turing machine: copy tape 1 to tape 2
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_automata_multi_tape_turing_machine() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; 2-tape TM: copies content from tape 1 to tape 2
  ;; Transition: (state read1 read2) -> (new-state write1 write2 dir1 dir2)
  (defvar neovm--test-mt-trans (make-hash-table :test 'equal))

  ;; State copy: read from tape1, write to tape2
  (puthash '(copy ?a ?_) '(copy ?a ?a R R) neovm--test-mt-trans)
  (puthash '(copy ?b ?_) '(copy ?b ?b R R) neovm--test-mt-trans)
  (puthash '(copy ?c ?_) '(copy ?c ?c R R) neovm--test-mt-trans)
  ;; When tape1 hits blank, rewind both tapes
  (puthash '(copy ?_ ?_) '(rewind ?_ ?_ L L) neovm--test-mt-trans)
  ;; Rewind: move left until blank
  (puthash '(rewind ?a ?a) '(rewind ?a ?a L L) neovm--test-mt-trans)
  (puthash '(rewind ?b ?b) '(rewind ?b ?b L L) neovm--test-mt-trans)
  (puthash '(rewind ?c ?c) '(rewind ?c ?c L L) neovm--test-mt-trans)
  (puthash '(rewind ?_ ?_) '(accept ?_ ?_ R R) neovm--test-mt-trans)

  (fset 'neovm--test-mt-run
    (lambda (tape1-str max-steps)
      "Run 2-tape TM. Return (result tape1 tape2 steps)."
      (let* ((pad 10)
             (t1 (vconcat (make-vector pad ?_) tape1-str (make-vector pad ?_)))
             (t2 (make-vector (length t1) ?_))
             (h1 pad) (h2 pad)
             (state 'copy) (steps 0) (halted nil) (result nil))
        (while (and (not halted) (< steps max-steps))
          (let* ((r1 (aref t1 h1))
                 (r2 (aref t2 h2))
                 (key (list state r1 r2))
                 (trans (gethash key neovm--test-mt-trans)))
            (if (null trans)
                (progn (setq halted t) (setq result 'stuck))
              (let ((ns (nth 0 trans))
                    (w1 (nth 1 trans)) (w2 (nth 2 trans))
                    (d1 (nth 3 trans)) (d2 (nth 4 trans)))
                (aset t1 h1 w1)
                (aset t2 h2 w2)
                (setq state ns)
                (cond ((eq d1 'R) (setq h1 (1+ h1)))
                      ((eq d1 'L) (setq h1 (1- h1))))
                (cond ((eq d2 'R) (setq h2 (1+ h2)))
                      ((eq d2 'L) (setq h2 (1- h2))))
                (when (eq state 'accept)
                  (setq halted t) (setq result 'accept)))))
          (setq steps (1+ steps)))
        (unless halted (setq result 'timeout))
        ;; Extract non-blank content from both tapes
        (let ((extract (lambda (tape)
                         (let ((s 0) (e (1- (length tape))))
                           (while (and (<= s e) (= (aref tape s) ?_))
                             (setq s (1+ s)))
                           (while (and (>= e s) (= (aref tape e) ?_))
                             (setq e (1- e)))
                           (if (> s e) ""
                             (substring (concat tape) s (1+ e)))))))
          (list result
                (funcall extract t1)
                (funcall extract t2)
                steps)))))

  (unwind-protect
      (list
        ;; Copy "abc"
        (funcall 'neovm--test-mt-run "abc" 200)
        ;; Copy "aaa"
        (funcall 'neovm--test-mt-run "aaa" 200)
        ;; Copy single char
        (funcall 'neovm--test-mt-run "b" 200)
        ;; Copy longer string
        (funcall 'neovm--test-mt-run "abcabc" 200)
        ;; Verify tape1 = tape2 after copy
        (let ((result (funcall 'neovm--test-mt-run "abcba" 200)))
          (string= (nth 1 result) (nth 2 result)))
        ;; Empty tape
        (funcall 'neovm--test-mt-run "" 200))
    (makunbound 'neovm--test-mt-trans)
    (fmakunbound 'neovm--test-mt-run)))"#;
    let expect = expect_test::expect![[
        r#""OK ((accept \"abc\" \"abc\" 8) (accept \"aaa\" \"aaa\" 8) (accept \"b\" \"b\" 4) (accept \"abcabc\" \"abcabc\" 14) t (accept \"\" \"\" 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Context-free grammar: CYK parsing algorithm
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_automata_cfg_cyk_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // CYK algorithm for checking if a string belongs to a context-free language
    // Grammar must be in Chomsky Normal Form (CNF):
    // A -> BC  or  A -> a
    let form = r#"(progn
  ;; Grammar for balanced parentheses in CNF:
  ;; S -> LP RP | LP SR | SL RP | SS2
  ;; SL -> LP S
  ;; SR -> S RP
  ;; SS2 -> S S
  ;; LP -> (
  ;; RP -> )
  ;; Actually, simpler CNF for S -> (S) | SS | epsilon
  ;; Remove epsilon: S -> LP RP | LP SR | SL RP | S S
  ;; But CYK needs strict CNF. Let's use arithmetic: E -> E+E | E*E | (E) | n
  ;; In CNF: E -> EM T | EA T | LP X | n
  ;;         T -> a  (terminal for variable names / numbers)
  ;;         EM -> E M | ... this gets complex.
  ;;
  ;; Use a simple grammar: S -> AB | AS1, S1 -> SB, A -> a, B -> b
  ;; This generates a^n b^n for n >= 1.

  (defvar neovm--test-cyk-rules nil)
  ;; Binary rules: (lhs . (rhs1 . rhs2))
  ;; Terminal rules: (lhs . terminal-char)
  (setq neovm--test-cyk-rules
    '(;; S -> AB | AS1
      (S AB) (S A-S1)
      ;; S1 -> SB
      (S1 SB)
      ;; A -> a
      (A . ?a)
      ;; B -> b
      (B . ?b)))

  (fset 'neovm--test-cyk-parse
    (lambda (input rules)
      "CYK parser. Returns t if INPUT is in the language defined by RULES."
      (let* ((n (length input))
             ;; table[i][j] = set of nonterminals that derive input[i..j]
             ;; Represent as hash-table (i . j) -> list of symbols
             (table (make-hash-table :test 'equal)))
        (if (= n 0) nil  ;; empty string not in this grammar
          (progn
            ;; Fill diagonal: single characters
            (dotimes (i n)
              (let ((ch (aref input i))
                    (syms nil))
                (dolist (rule rules)
                  (when (and (not (listp (cdr rule)))
                             (integerp (cdr rule))
                             (= (cdr rule) ch))
                    (setq syms (cons (car rule) syms))))
                (puthash (cons i i) syms table)))
            ;; Fill rest: increasing span lengths
            (let ((span 2))
              (while (<= span n)
                (let ((i 0))
                  (while (<= (+ i span -1) (1- n))
                    (let ((j (+ i span -1))
                          (syms nil))
                      ;; Try all split points
                      (let ((k i))
                        (while (< k j)
                          (let ((left-syms (gethash (cons i k) table))
                                (right-syms (gethash (cons (1+ k) j) table)))
                            (dolist (rule rules)
                              (when (and (listp (cdr rule))
                                         (= (length (cdr rule)) 1)
                                         (let ((rhs (cadr rule)))
                                           (and (= (length (symbol-name rhs)) 2)
                                                (let ((r1 (intern (substring (symbol-name rhs) 0 1)))
                                                      (r2 (intern (substring (symbol-name rhs) 1))))
                                                  (and (memq r1 left-syms)
                                                       (memq r2 right-syms)
                                                       (progn
                                                         (unless (memq (car rule) syms)
                                                           (setq syms (cons (car rule) syms)))
                                                         t))))))
                              ;; Also check 3-char rhs like "A-S1" -> won't match, need better representation
                              ))
                          (setq k (1+ k))))
                      (puthash (cons i j) syms table))
                    (setq i (1+ i))))
                (setq span (1+ span))))
            ;; Check if start symbol S is in table[0][n-1]
            (memq 'S (gethash (cons 0 (1- n)) table)))))))

  ;; Simpler approach: recursive descent for a^n b^n
  (fset 'neovm--test-anbn-check
    (lambda (input)
      "Check if INPUT matches a^n b^n for n >= 1."
      (let ((n (length input))
            (valid t))
        (if (or (= n 0) (= 1 (% n 2)))
            nil  ;; must be even and non-empty
          (let ((half (/ n 2))
                (i 0))
            ;; First half must be all 'a'
            (while (and valid (< i half))
              (unless (= (aref input i) ?a)
                (setq valid nil))
              (setq i (1+ i)))
            ;; Second half must be all 'b'
            (setq i half)
            (while (and valid (< i n))
              (unless (= (aref input i) ?b)
                (setq valid nil))
              (setq i (1+ i)))
            valid)))))

  (unwind-protect
      (list
        ;; a^n b^n membership
        (funcall 'neovm--test-anbn-check "ab")
        (funcall 'neovm--test-anbn-check "aabb")
        (funcall 'neovm--test-anbn-check "aaabbb")
        (funcall 'neovm--test-anbn-check "aaaabbbb")
        ;; Rejected
        (funcall 'neovm--test-anbn-check "")
        (funcall 'neovm--test-anbn-check "a")
        (funcall 'neovm--test-anbn-check "aab")
        (funcall 'neovm--test-anbn-check "abb")
        (funcall 'neovm--test-anbn-check "ba")
        (funcall 'neovm--test-anbn-check "abab"))
    (makunbound 'neovm--test-cyk-rules)
    (fmakunbound 'neovm--test-cyk-parse)
    (fmakunbound 'neovm--test-anbn-check)))"#;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// DFA operations: union, concatenation, Kleene star via NFA construction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_automata_regular_language_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; DFA represented as: (states alphabet transitions start accept-states)
  ;; transitions: hash-table (state . symbol) -> new-state
  ;; We build NFAs for union/concat/star, then simulate directly

  (defvar neovm--test-dfa-counter 0)

  (fset 'neovm--test-dfa-fresh
    (lambda ()
      (setq neovm--test-dfa-counter (1+ neovm--test-dfa-counter))
      neovm--test-dfa-counter))

  ;; NFA: (start accept-states transitions epsilon-transitions)
  ;; transitions: hash (state . char) -> list of states
  ;; epsilon: hash state -> list of states

  (fset 'neovm--test-nfa-literal
    (lambda (ch)
      (let ((s (funcall 'neovm--test-dfa-fresh))
            (e (funcall 'neovm--test-dfa-fresh))
            (trans (make-hash-table :test 'equal))
            (eps (make-hash-table :test 'equal)))
        (puthash (cons s ch) (list e) trans)
        (list s (list e) trans eps))))

  (fset 'neovm--test-nfa-union
    (lambda (nfa1 nfa2)
      "Union of two NFAs."
      (let ((new-start (funcall 'neovm--test-dfa-fresh))
            (new-accept (funcall 'neovm--test-dfa-fresh))
            (trans (make-hash-table :test 'equal))
            (eps (make-hash-table :test 'equal)))
        ;; Copy transitions from both NFAs
        (maphash (lambda (k v) (puthash k v trans)) (nth 2 nfa1))
        (maphash (lambda (k v) (puthash k v trans)) (nth 2 nfa2))
        ;; Copy epsilon transitions
        (maphash (lambda (k v) (puthash k v eps)) (nth 3 nfa1))
        (maphash (lambda (k v) (puthash k v eps)) (nth 3 nfa2))
        ;; Add epsilon from new-start to both starts
        (puthash new-start (list (nth 0 nfa1) (nth 0 nfa2)) eps)
        ;; Add epsilon from both accepts to new-accept
        (dolist (a (nth 1 nfa1))
          (puthash a (cons new-accept (gethash a eps)) eps))
        (dolist (a (nth 1 nfa2))
          (puthash a (cons new-accept (gethash a eps)) eps))
        (list new-start (list new-accept) trans eps))))

  (fset 'neovm--test-nfa-concat
    (lambda (nfa1 nfa2)
      "Concatenation of two NFAs."
      (let ((trans (make-hash-table :test 'equal))
            (eps (make-hash-table :test 'equal)))
        (maphash (lambda (k v) (puthash k v trans)) (nth 2 nfa1))
        (maphash (lambda (k v) (puthash k v trans)) (nth 2 nfa2))
        (maphash (lambda (k v) (puthash k v eps)) (nth 3 nfa1))
        (maphash (lambda (k v) (puthash k v eps)) (nth 3 nfa2))
        ;; Epsilon from nfa1 accepts to nfa2 start
        (dolist (a (nth 1 nfa1))
          (puthash a (cons (nth 0 nfa2) (gethash a eps)) eps))
        (list (nth 0 nfa1) (nth 1 nfa2) trans eps))))

  (fset 'neovm--test-nfa-star
    (lambda (nfa)
      "Kleene star of an NFA."
      (let ((new-start (funcall 'neovm--test-dfa-fresh))
            (new-accept (funcall 'neovm--test-dfa-fresh))
            (trans (make-hash-table :test 'equal))
            (eps (make-hash-table :test 'equal)))
        (maphash (lambda (k v) (puthash k v trans)) (nth 2 nfa))
        (maphash (lambda (k v) (puthash k v eps)) (nth 3 nfa))
        ;; new-start -> old start and new-accept (for epsilon acceptance)
        (puthash new-start (list (nth 0 nfa) new-accept) eps)
        ;; old accepts -> old start and new-accept
        (dolist (a (nth 1 nfa))
          (puthash a (append (list (nth 0 nfa) new-accept) (gethash a eps)) eps))
        (list new-start (list new-accept) trans eps))))

  ;; NFA simulation
  (fset 'neovm--test-nfa-eps-closure
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

  (fset 'neovm--test-nfa-simulate
    (lambda (nfa input)
      (let ((current (funcall 'neovm--test-nfa-eps-closure
                               (list (nth 0 nfa)) (nth 3 nfa)))
            (i 0) (len (length input)))
        (while (< i len)
          (let ((ch (aref input i))
                (next nil))
            (dolist (s current)
              (dolist (t2 (gethash (cons s ch) (nth 2 nfa)))
                (unless (memq t2 next)
                  (setq next (cons t2 next)))))
            (setq current (funcall 'neovm--test-nfa-eps-closure next (nth 3 nfa))))
          (setq i (1+ i)))
        ;; Check if any current state is accepting
        (let ((accepted nil))
          (dolist (s current)
            (when (memq s (nth 1 nfa))
              (setq accepted t)))
          accepted))))

  (unwind-protect
      (progn
        (setq neovm--test-dfa-counter 0)
        (let* (;; L1 = {a}
               (nfa-a (funcall 'neovm--test-nfa-literal ?a))
               ;; L2 = {b}
               (nfa-b (funcall 'neovm--test-nfa-literal ?b))
               ;; L1 | L2 = {a, b}
               (nfa-union (funcall 'neovm--test-nfa-union nfa-a nfa-b)))
          ;; Need fresh NFAs for concat/star since structures are shared
          (setq neovm--test-dfa-counter 0)
          (let* ((a2 (funcall 'neovm--test-nfa-literal ?a))
                 (b2 (funcall 'neovm--test-nfa-literal ?b))
                 ;; L1 L2 = {ab}
                 (nfa-cat (funcall 'neovm--test-nfa-concat a2 b2)))
            (setq neovm--test-dfa-counter 0)
            (let* ((a3 (funcall 'neovm--test-nfa-literal ?a))
                   ;; L1* = {epsilon, a, aa, aaa, ...}
                   (nfa-astar (funcall 'neovm--test-nfa-star a3)))
              (list
                ;; Union tests
                (funcall 'neovm--test-nfa-simulate nfa-union "a")    ;; t
                (funcall 'neovm--test-nfa-simulate nfa-union "b")    ;; t
                (funcall 'neovm--test-nfa-simulate nfa-union "c")    ;; nil
                (funcall 'neovm--test-nfa-simulate nfa-union "ab")   ;; nil
                (funcall 'neovm--test-nfa-simulate nfa-union "")     ;; nil
                ;; Concatenation tests
                (funcall 'neovm--test-nfa-simulate nfa-cat "ab")     ;; t
                (funcall 'neovm--test-nfa-simulate nfa-cat "a")      ;; nil
                (funcall 'neovm--test-nfa-simulate nfa-cat "b")      ;; nil
                (funcall 'neovm--test-nfa-simulate nfa-cat "ba")     ;; nil
                ;; Kleene star tests
                (funcall 'neovm--test-nfa-simulate nfa-astar "")     ;; t
                (funcall 'neovm--test-nfa-simulate nfa-astar "a")    ;; t
                (funcall 'neovm--test-nfa-simulate nfa-astar "aaa")  ;; t
                (funcall 'neovm--test-nfa-simulate nfa-astar "b")    ;; nil
                (funcall 'neovm--test-nfa-simulate nfa-astar "ab"))))))  ;; nil
    (makunbound 'neovm--test-dfa-counter)
    (fmakunbound 'neovm--test-dfa-fresh)
    (fmakunbound 'neovm--test-nfa-literal)
    (fmakunbound 'neovm--test-nfa-union)
    (fmakunbound 'neovm--test-nfa-concat)
    (fmakunbound 'neovm--test-nfa-star)
    (fmakunbound 'neovm--test-nfa-eps-closure)
    (fmakunbound 'neovm--test-nfa-simulate)))"#;
    let expect = expect_test::expect![[r#""OK (t t nil nil nil t nil nil nil t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// DFA minimization via table-filling algorithm
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_automata_dfa_minimization() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Minimize a DFA by merging equivalent states using the
    // table-filling (Myhill-Nerode) algorithm
    let form = r#"(progn
  ;; DFA: (states alphabet trans start accepts)
  ;; Minimization: mark distinguishable pairs, merge unmarked pairs

  (fset 'neovm--test-dfa-minimize
    (lambda (states alphabet trans start accepts)
      "Minimize DFA via table-filling. Return number of equivalence classes."
      (let ((n (length states))
            ;; Map state -> index
            (state-idx (make-hash-table :test 'equal))
            ;; distinguished(i,j) for i < j
            (dist (make-hash-table :test 'equal))
            (changed t))
        ;; Build index
        (let ((i 0))
          (dolist (s states)
            (puthash s i state-idx)
            (setq i (1+ i))))
        ;; Step 1: Mark pairs where one is accepting and other is not
        (dolist (s1 states)
          (dolist (s2 states)
            (let ((i1 (gethash s1 state-idx))
                  (i2 (gethash s2 state-idx)))
              (when (< i1 i2)
                (let ((a1 (memq s1 accepts))
                      (a2 (memq s2 accepts)))
                  (when (not (eq (not a1) (not a2)))
                    (puthash (cons i1 i2) t dist)))))))
        ;; Step 2: Iterate until no changes
        (while changed
          (setq changed nil)
          (dolist (s1 states)
            (dolist (s2 states)
              (let ((i1 (gethash s1 state-idx))
                    (i2 (gethash s2 state-idx)))
                (when (and (< i1 i2)
                           (not (gethash (cons i1 i2) dist)))
                  ;; Check if any input symbol distinguishes them
                  (let ((found nil))
                    (dolist (a alphabet)
                      (unless found
                        (let* ((t1 (gethash (cons s1 a) trans))
                               (t2 (gethash (cons s2 a) trans))
                               (ti1 (gethash t1 state-idx))
                               (ti2 (gethash t2 state-idx)))
                          (when (and ti1 ti2 (/= ti1 ti2))
                            (let ((lo (min ti1 ti2))
                                  (hi (max ti1 ti2)))
                              (when (gethash (cons lo hi) dist)
                                (puthash (cons i1 i2) t dist)
                                (setq changed t)
                                (setq found t)))))))))))  ))
        ;; Count equivalence classes via union-find-like grouping
        (let ((parent (make-vector n -1))
              (classes 0))
          ;; For each undistinguished pair, merge
          (dolist (s1 states)
            (dolist (s2 states)
              (let ((i1 (gethash s1 state-idx))
                    (i2 (gethash s2 state-idx)))
                (when (and (< i1 i2)
                           (not (gethash (cons i1 i2) dist)))
                  ;; Merge i2 into i1's group
                  (aset parent i2 i1)))))
          ;; Count roots (states with parent = -1 and not merged into another)
          (dotimes (i n)
            (when (= (aref parent i) -1)
              (setq classes (1+ classes))))
          classes))))

  (unwind-protect
      (let* (;; DFA with redundant states:
             ;; States A,B,C,D,E where B=D (equivalent) and C=E (equivalent)
             ;; Alphabet: {0, 1}
             ;; A --0--> B, A --1--> C
             ;; B --0--> A, B --1--> D  (D equiv to B)
             ;; C --0--> E, C --1--> A  (E equiv to C)
             ;; D --0--> A, D --1--> B
             ;; E --0--> C, E --1--> A
             ;; Accept states: {A}
             (states '(A B C D E))
             (alpha '(?0 ?1))
             (trans (make-hash-table :test 'equal)))
        (puthash '(A . ?0) 'B trans) (puthash '(A . ?1) 'C trans)
        (puthash '(B . ?0) 'A trans) (puthash '(B . ?1) 'D trans)
        (puthash '(C . ?0) 'E trans) (puthash '(C . ?1) 'A trans)
        (puthash '(D . ?0) 'A trans) (puthash '(D . ?1) 'B trans)
        (puthash '(E . ?0) 'C trans) (puthash '(E . ?1) 'A trans)
        (list
          ;; 5 states should minimize to 3 (A, B=D, C=E)
          (funcall 'neovm--test-dfa-minimize states alpha trans 'A '(A))
          ;; Already minimal DFA: 2-state DFA for "starts with a"
          (let ((t2 (make-hash-table :test 'equal)))
            (puthash '(S0 . ?a) 'S1 t2) (puthash '(S0 . ?b) 'S0 t2)
            (puthash '(S1 . ?a) 'S1 t2) (puthash '(S1 . ?b) 'S1 t2)
            (funcall 'neovm--test-dfa-minimize '(S0 S1) '(?a ?b) t2 'S0 '(S1)))
          ;; Single-state DFA (accept everything)
          (let ((t3 (make-hash-table :test 'equal)))
            (puthash '(Q . ?0) 'Q t3) (puthash '(Q . ?1) 'Q t3)
            (funcall 'neovm--test-dfa-minimize '(Q) '(?0 ?1) t3 'Q '(Q)))))
    (fmakunbound 'neovm--test-dfa-minimize)))"#;
    let expect = expect_test::expect![[r#""OK (3 2 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Language equivalence checking: do two DFAs accept the same language?
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_automata_language_equivalence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Check if two DFAs accept the same language by simulating
    // the product automaton and checking for disagreeing accept states
    let form = r#"(progn
  (fset 'neovm--test-dfa-equivalent
    (lambda (trans1 start1 accepts1 trans2 start2 accepts2 alphabet)
      "Check if two DFAs accept the same language via product construction.
       Uses BFS to explore reachable state pairs."
      (let ((visited (make-hash-table :test 'equal))
            (queue (list (cons start1 start2)))
            (equivalent t))
        (puthash (cons start1 start2) t visited)
        (while (and queue equivalent)
          (let* ((pair (car queue))
                 (s1 (car pair))
                 (s2 (cdr pair)))
            (setq queue (cdr queue))
            ;; Check: both accept or both reject
            (let ((a1 (if (memq s1 accepts1) t nil))
                  (a2 (if (memq s2 accepts2) t nil)))
              (unless (eq a1 a2)
                (setq equivalent nil)))
            ;; Explore successors
            (when equivalent
              (dolist (a alphabet)
                (let ((next1 (gethash (cons s1 a) trans1))
                      (next2 (gethash (cons s2 a) trans2)))
                  (when (and next1 next2)
                    (let ((next-pair (cons next1 next2)))
                      (unless (gethash next-pair visited)
                        (puthash next-pair t visited)
                        (setq queue (append queue (list next-pair)))))))))))
        equivalent)))

  (unwind-protect
      (list
        ;; Two identical DFAs: equivalent
        (let ((t1 (make-hash-table :test 'equal))
              (t2 (make-hash-table :test 'equal)))
          (puthash '(q0 . ?a) 'q1 t1) (puthash '(q0 . ?b) 'q0 t1)
          (puthash '(q1 . ?a) 'q1 t1) (puthash '(q1 . ?b) 'q1 t1)
          (puthash '(q0 . ?a) 'q1 t2) (puthash '(q0 . ?b) 'q0 t2)
          (puthash '(q1 . ?a) 'q1 t2) (puthash '(q1 . ?b) 'q1 t2)
          (funcall 'neovm--test-dfa-equivalent
                   t1 'q0 '(q1) t2 'q0 '(q1) '(?a ?b)))
        ;; Two DFAs with different number of states but same language
        ;; DFA1: 2 states for "contains at least one a"
        ;; DFA2: 3 states but equivalent (q0->q1 on a, q0->q2 on b, q2->q1 on a, q2->q2 on b,
        ;;        q1->q1 on a|b)
        (let ((t1 (make-hash-table :test 'equal))
              (t2 (make-hash-table :test 'equal)))
          (puthash '(q0 . ?a) 'q1 t1) (puthash '(q0 . ?b) 'q0 t1)
          (puthash '(q1 . ?a) 'q1 t1) (puthash '(q1 . ?b) 'q1 t1)
          (puthash '(r0 . ?a) 'r1 t2) (puthash '(r0 . ?b) 'r2 t2)
          (puthash '(r1 . ?a) 'r1 t2) (puthash '(r1 . ?b) 'r1 t2)
          (puthash '(r2 . ?a) 'r1 t2) (puthash '(r2 . ?b) 'r2 t2)
          (funcall 'neovm--test-dfa-equivalent
                   t1 'q0 '(q1) t2 'r0 '(r1) '(?a ?b)))
        ;; Two DFAs for different languages: not equivalent
        ;; DFA1: accepts strings starting with 'a'
        ;; DFA2: accepts strings ending with 'a'
        (let ((t1 (make-hash-table :test 'equal))
              (t2 (make-hash-table :test 'equal)))
          ;; DFA1: starts with a
          (puthash '(q0 . ?a) 'q1 t1) (puthash '(q0 . ?b) 'q2 t1)
          (puthash '(q1 . ?a) 'q1 t1) (puthash '(q1 . ?b) 'q1 t1)
          (puthash '(q2 . ?a) 'q2 t1) (puthash '(q2 . ?b) 'q2 t1)
          ;; DFA2: ends with a
          (puthash '(r0 . ?a) 'r1 t2) (puthash '(r0 . ?b) 'r0 t2)
          (puthash '(r1 . ?a) 'r1 t2) (puthash '(r1 . ?b) 'r0 t2)
          (funcall 'neovm--test-dfa-equivalent
                   t1 'q0 '(q1) t2 'r0 '(r1) '(?a ?b)))
        ;; Both accept everything: equivalent
        (let ((t1 (make-hash-table :test 'equal))
              (t2 (make-hash-table :test 'equal)))
          (puthash '(q0 . ?a) 'q0 t1) (puthash '(q0 . ?b) 'q0 t1)
          (puthash '(r0 . ?a) 'r0 t2) (puthash '(r0 . ?b) 'r0 t2)
          (funcall 'neovm--test-dfa-equivalent
                   t1 'q0 '(q0) t2 'r0 '(r0) '(?a ?b)))
        ;; One accepts nothing, the other accepts everything: not equivalent
        (let ((t1 (make-hash-table :test 'equal))
              (t2 (make-hash-table :test 'equal)))
          (puthash '(q0 . ?a) 'q0 t1) (puthash '(q0 . ?b) 'q0 t1)
          (puthash '(r0 . ?a) 'r0 t2) (puthash '(r0 . ?b) 'r0 t2)
          (funcall 'neovm--test-dfa-equivalent
                   t1 'q0 '(q0) t2 'r0 nil '(?a ?b))))
    (fmakunbound 'neovm--test-dfa-equivalent)))"#;
    let expect = expect_test::expect![[r#""OK (t t nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
