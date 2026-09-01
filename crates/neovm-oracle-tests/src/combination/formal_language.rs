//! Oracle parity tests for formal language theory in Elisp:
//! DFA construction and simulation, NFA to DFA subset construction,
//! regular expression to NFA (Thompson's construction), context-free
//! grammar parsing (CYK algorithm), Chomsky normal form conversion,
//! language recognition and classification.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// DFA construction and simulation: binary string parity checker
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_formal_lang_dfa_parity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // DFA that accepts binary strings with even number of 1s
    let form = r#"(progn
  (fset 'neovm--fl-dfa-make
    (lambda (states alphabet transitions start accepting)
      "Create a DFA as a plist.
       transitions: hash-table of (state . symbol) -> state."
      (list 'states states 'alphabet alphabet
            'transitions transitions
            'start start 'accepting accepting)))

  (fset 'neovm--fl-dfa-run
    (lambda (dfa input)
      "Run DFA on input string. Returns (accepted final-state trace)."
      (let ((trans (plist-get dfa 'transitions))
            (state (plist-get dfa 'start))
            (accept-set (plist-get dfa 'accepting))
            (trace nil)
            (i 0)
            (len (length input)))
        (while (< i len)
          (let* ((sym (aref input i))
                 (new-state (gethash (cons state sym) trans)))
            (if new-state
                (progn
                  (setq trace (cons (list state sym new-state) trace))
                  (setq state new-state))
              ;; Dead state on invalid transition
              (setq trace (cons (list state sym 'dead) trace))
              (setq state 'dead)))
          (setq i (1+ i)))
        (list (and (not (eq state 'dead))
                   (memq state accept-set)
                   t)
              state
              (nreverse trace)))))

  (unwind-protect
      (let ((trans (make-hash-table :test 'equal)))
        ;; Even-parity DFA: states {even, odd}, alphabet {?0, ?1}
        ;; even --0--> even, even --1--> odd
        ;; odd --0--> odd, odd --1--> even
        (puthash '(even . ?0) 'even trans)
        (puthash '(even . ?1) 'odd trans)
        (puthash '(odd . ?0) 'odd trans)
        (puthash '(odd . ?1) 'even trans)
        (let ((dfa (funcall 'neovm--fl-dfa-make
                            '(even odd) '(?0 ?1)
                            trans 'even '(even))))
          (list
            ;; Empty string: 0 ones (even) -> accept
            (car (funcall 'neovm--fl-dfa-run dfa ""))
            ;; "0": 0 ones -> accept
            (car (funcall 'neovm--fl-dfa-run dfa "0"))
            ;; "1": 1 one (odd) -> reject
            (car (funcall 'neovm--fl-dfa-run dfa "1"))
            ;; "11": 2 ones -> accept
            (car (funcall 'neovm--fl-dfa-run dfa "11"))
            ;; "101": 2 ones -> accept
            (car (funcall 'neovm--fl-dfa-run dfa "101"))
            ;; "111": 3 ones -> reject
            (car (funcall 'neovm--fl-dfa-run dfa "111"))
            ;; "1001001": 3 ones -> reject
            (car (funcall 'neovm--fl-dfa-run dfa "1001001"))
            ;; "11001100": 4 ones -> accept
            (car (funcall 'neovm--fl-dfa-run dfa "11001100"))
            ;; Full trace for "1010"
            (funcall 'neovm--fl-dfa-run dfa "1010"))))
    (fmakunbound 'neovm--fl-dfa-make)
    (fmakunbound 'neovm--fl-dfa-run)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t nil t t nil nil t (t even ((even 49 odd) (odd 48 odd) (odd 49 even) (even 48 even))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// NFA to DFA subset construction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_formal_lang_nfa_to_dfa() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; NFA transitions: hash-table of (state . symbol) -> list-of-states
  ;; Epsilon transitions: (state . eps) -> list-of-states

  (fset 'neovm--fl-eps-closure
    (lambda (states eps-trans)
      "Compute epsilon closure of a set of states."
      (let ((result (copy-sequence states))
            (worklist (copy-sequence states)))
        (while worklist
          (let* ((s (car worklist))
                 (targets (gethash (cons s 'eps) eps-trans)))
            (setq worklist (cdr worklist))
            (dolist (t2 targets)
              (unless (memq t2 result)
                (setq result (cons t2 result))
                (setq worklist (cons t2 worklist))))))
        (sort result (lambda (a b) (< a b))))))

  (fset 'neovm--fl-nfa-move
    (lambda (states sym nfa-trans)
      "States reachable from STATES on symbol SYM."
      (let ((result nil))
        (dolist (s states)
          (dolist (t2 (gethash (cons s sym) nfa-trans))
            (unless (memq t2 result)
              (setq result (cons t2 result)))))
        (sort result (lambda (a b) (< a b))))))

  (fset 'neovm--fl-subset-construct
    (lambda (nfa-trans eps-trans alphabet nfa-start nfa-accept)
      "Convert NFA to DFA via subset construction.
       Returns (dfa-transitions dfa-start dfa-accepting state-map)."
      (let* ((start-set (funcall 'neovm--fl-eps-closure (list nfa-start) eps-trans))
             (dfa-trans (make-hash-table :test 'equal))
             (worklist (list start-set))
             (visited nil)
             (accepting nil)
             (state-map nil)
             (next-id 0))
        ;; Assign ID to start set
        (setq state-map (list (cons start-set next-id)))
        (setq next-id (1+ next-id))
        (while worklist
          (let ((current (car worklist)))
            (setq worklist (cdr worklist))
            (unless (member current visited)
              (setq visited (cons current visited))
              (let ((cur-id (cdr (assoc current state-map))))
                ;; Check if accepting
                (dolist (s current)
                  (when (memq s nfa-accept)
                    (unless (memq cur-id accepting)
                      (setq accepting (cons cur-id accepting)))))
                ;; Compute transitions for each symbol
                (dolist (sym alphabet)
                  (let* ((moved (funcall 'neovm--fl-nfa-move current sym nfa-trans))
                         (closed (funcall 'neovm--fl-eps-closure moved eps-trans)))
                    (when closed
                      ;; Get or create ID for target set
                      (unless (assoc closed state-map)
                        (setq state-map (cons (cons closed next-id) state-map))
                        (setq next-id (1+ next-id)))
                      (let ((target-id (cdr (assoc closed state-map))))
                        (puthash (cons cur-id sym) target-id dfa-trans)
                        (unless (member closed visited)
                          (unless (member closed worklist)
                            (setq worklist (cons closed worklist)))))))))))
        (list dfa-trans
              (cdr (assoc start-set state-map))
              (sort accepting '<)
              next-id))))

  (fset 'neovm--fl-run-dfa-table
    (lambda (dfa-trans start accepting input)
      "Run DFA from subset construction on input."
      (let ((state start) (i 0) (len (length input)) (valid t))
        (while (and valid (< i len))
          (let ((next (gethash (cons state (aref input i)) dfa-trans)))
            (if next
                (setq state next)
              (setq valid nil)))
          (setq i (1+ i)))
        (and valid (memq state accepting) t))))

  (unwind-protect
      (let ((nfa-trans (make-hash-table :test 'equal))
            (eps-trans (make-hash-table :test 'equal)))
        ;; NFA for a*b|ab*: accepts "b", "ab", "aab", "abb", "abbb", etc.
        ;; States: 0(start), 1, 2, 3, 4, 5(accept-path1), 6(accept-path2)
        ;; Path 1 (a*b): 0 -eps-> 1, 1 -a-> 1, 1 -b-> 5
        ;; Path 2 (ab*): 0 -eps-> 2, 2 -a-> 3, 3 -b-> 3, 3 -eps-> 6
        (puthash '(0 . eps) '(1 2) eps-trans)
        (puthash '(1 . ?a) '(1) nfa-trans)
        (puthash '(1 . ?b) '(5) nfa-trans)
        (puthash '(2 . ?a) '(3) nfa-trans)
        (puthash '(3 . ?b) '(3) nfa-trans)
        (puthash '(3 . eps) '(6) eps-trans)
        ;; Construct DFA
        (let* ((result (funcall 'neovm--fl-subset-construct
                                nfa-trans eps-trans '(?a ?b) 0 '(5 6)))
               (dfa-trans (nth 0 result))
               (dfa-start (nth 1 result))
               (dfa-accept (nth 2 result))
               (num-states (nth 3 result)))
          (list
            ;; Number of DFA states
            num-states
            ;; Test strings
            (funcall 'neovm--fl-run-dfa-table dfa-trans dfa-start dfa-accept "b")
            (funcall 'neovm--fl-run-dfa-table dfa-trans dfa-start dfa-accept "ab")
            (funcall 'neovm--fl-run-dfa-table dfa-trans dfa-start dfa-accept "aab")
            (funcall 'neovm--fl-run-dfa-table dfa-trans dfa-start dfa-accept "abb")
            (funcall 'neovm--fl-run-dfa-table dfa-trans dfa-start dfa-accept "abbb")
            (funcall 'neovm--fl-run-dfa-table dfa-trans dfa-start dfa-accept "a")
            (funcall 'neovm--fl-run-dfa-table dfa-trans dfa-start dfa-accept "")
            (funcall 'neovm--fl-run-dfa-table dfa-trans dfa-start dfa-accept "ba")
            (funcall 'neovm--fl-run-dfa-table dfa-trans dfa-start dfa-accept "aabb"))))
    (fmakunbound 'neovm--fl-eps-closure)
    (fmakunbound 'neovm--fl-nfa-move)
    (fmakunbound 'neovm--fl-subset-construct)
    (fmakunbound 'neovm--fl-run-dfa-table)))"#;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Thompson's construction: regex to NFA
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_formal_lang_thompson_construction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Thompson's construction for a simplified regex AST:
  ;; (lit c)        - literal character
  ;; (cat r1 r2)    - concatenation
  ;; (alt r1 r2)    - alternation
  ;; (star r)       - Kleene star
  ;; (plus r)       - one or more (r . r*)
  ;; (opt r)        - optional (epsilon | r)
  ;; Returns: (start accept transitions) where transitions is list of (from sym to)

  (defvar neovm--fl-tc-next 0)

  (fset 'neovm--fl-tc-new-state
    (lambda ()
      (let ((s neovm--fl-tc-next))
        (setq neovm--fl-tc-next (1+ neovm--fl-tc-next))
        s)))

  (fset 'neovm--fl-tc-compile
    (lambda (regex)
      "Compile regex AST to NFA fragment (start accept transitions)."
      (let ((tag (car regex)))
        (cond
          ((eq tag 'lit)
           (let ((s (funcall 'neovm--fl-tc-new-state))
                 (e (funcall 'neovm--fl-tc-new-state)))
             (list s e (list (list s (nth 1 regex) e)))))
          ((eq tag 'cat)
           (let ((n1 (funcall 'neovm--fl-tc-compile (nth 1 regex)))
                 (n2 (funcall 'neovm--fl-tc-compile (nth 2 regex))))
             (list (nth 0 n1) (nth 1 n2)
                   (append (nth 2 n1) (nth 2 n2)
                           (list (list (nth 1 n1) nil (nth 0 n2)))))))
          ((eq tag 'alt)
           (let ((n1 (funcall 'neovm--fl-tc-compile (nth 1 regex)))
                 (n2 (funcall 'neovm--fl-tc-compile (nth 2 regex)))
                 (s (funcall 'neovm--fl-tc-new-state))
                 (e (funcall 'neovm--fl-tc-new-state)))
             (list s e
                   (append (nth 2 n1) (nth 2 n2)
                           (list (list s nil (nth 0 n1))
                                 (list s nil (nth 0 n2))
                                 (list (nth 1 n1) nil e)
                                 (list (nth 1 n2) nil e))))))
          ((eq tag 'star)
           (let ((n1 (funcall 'neovm--fl-tc-compile (nth 1 regex)))
                 (s (funcall 'neovm--fl-tc-new-state))
                 (e (funcall 'neovm--fl-tc-new-state)))
             (list s e
                   (append (nth 2 n1)
                           (list (list s nil (nth 0 n1))
                                 (list s nil e)
                                 (list (nth 1 n1) nil (nth 0 n1))
                                 (list (nth 1 n1) nil e))))))
          ((eq tag 'plus)
           ;; r+ = r . r*
           (funcall 'neovm--fl-tc-compile
                    (list 'cat (nth 1 regex) (list 'star (nth 1 regex)))))
          ((eq tag 'opt)
           ;; r? = epsilon | r
           (let ((n1 (funcall 'neovm--fl-tc-compile (nth 1 regex)))
                 (s (funcall 'neovm--fl-tc-new-state))
                 (e (funcall 'neovm--fl-tc-new-state)))
             (list s e
                   (append (nth 2 n1)
                           (list (list s nil (nth 0 n1))
                                 (list s nil e)
                                 (list (nth 1 n1) nil e))))))))))

  ;; NFA runner
  (fset 'neovm--fl-tc-eps-close
    (lambda (states transitions)
      (let ((result (copy-sequence states))
            (worklist (copy-sequence states)))
        (while worklist
          (let ((s (car worklist)))
            (setq worklist (cdr worklist))
            (dolist (tr transitions)
              (when (and (= (nth 0 tr) s) (null (nth 1 tr)))
                (unless (memq (nth 2 tr) result)
                  (setq result (cons (nth 2 tr) result))
                  (setq worklist (cons (nth 2 tr) worklist)))))))
        result)))

  (fset 'neovm--fl-tc-match
    (lambda (nfa input)
      (let* ((transitions (nth 2 nfa))
             (accept (nth 1 nfa))
             (current (funcall 'neovm--fl-tc-eps-close (list (nth 0 nfa)) transitions))
             (i 0) (len (length input)))
        (while (< i len)
          (let ((ch (aref input i)) (next nil))
            (dolist (s current)
              (dolist (tr transitions)
                (when (and (= (nth 0 tr) s) (eql (nth 1 tr) ch))
                  (unless (memq (nth 2 tr) next)
                    (setq next (cons (nth 2 tr) next))))))
            (setq current (funcall 'neovm--fl-tc-eps-close next transitions)))
          (setq i (1+ i)))
        (if (memq accept current) t nil))))

  (unwind-protect
      (progn
        ;; Test: (a|b)+c?
        (setq neovm--fl-tc-next 0)
        (let ((nfa (funcall 'neovm--fl-tc-compile
                            '(cat (plus (alt (lit ?a) (lit ?b)))
                                  (opt (lit ?c))))))
          (list
            ;; Count transitions (structural check)
            (length (nth 2 nfa))
            ;; Match tests
            (funcall 'neovm--fl-tc-match nfa "a")
            (funcall 'neovm--fl-tc-match nfa "b")
            (funcall 'neovm--fl-tc-match nfa "ac")
            (funcall 'neovm--fl-tc-match nfa "bc")
            (funcall 'neovm--fl-tc-match nfa "abc")
            (funcall 'neovm--fl-tc-match nfa "abac")
            (funcall 'neovm--fl-tc-match nfa "bbbbc")
            (funcall 'neovm--fl-tc-match nfa "")
            (funcall 'neovm--fl-tc-match nfa "c")
            (funcall 'neovm--fl-tc-match nfa "acc")
            (funcall 'neovm--fl-tc-match nfa "aabb"))))
    (fmakunbound 'neovm--fl-tc-new-state)
    (fmakunbound 'neovm--fl-tc-compile)
    (fmakunbound 'neovm--fl-tc-eps-close)
    (fmakunbound 'neovm--fl-tc-match)
    (makunbound 'neovm--fl-tc-next)))"#;
    let expect = expect_test::expect![[r#""OK (22 t t t t t t t nil nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// CYK algorithm for context-free grammar parsing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_formal_lang_cyk_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; CYK algorithm for a grammar in Chomsky Normal Form (CNF)
  ;; Grammar rules as list of (lhs . rhs) where rhs is either:
  ;;   (A B) - two non-terminals
  ;;   (terminal) - single terminal
  ;; Table: hash-table of (i j) -> list of non-terminals

  (fset 'neovm--fl-cyk-parse
    (lambda (grammar start-symbol input)
      "CYK parsing. Returns (accepted table-size derivation-exists)."
      (let* ((n (length input))
             (table (make-hash-table :test 'equal)))
        ;; Base case: length-1 substrings
        (let ((i 0))
          (while (< i n)
            (let ((sym (aref input i))
                  (producers nil))
              (dolist (rule grammar)
                (let ((lhs (car rule))
                      (rhs (cdr rule)))
                  (when (and (= (length rhs) 1)
                             (eql (car rhs) sym))
                    (unless (memq lhs producers)
                      (setq producers (cons lhs producers))))))
              (puthash (cons i i) producers table))
            (setq i (1+ i))))
        ;; Fill table for increasing span lengths
        (let ((span 2))
          (while (<= span n)
            (let ((i 0))
              (while (<= (+ i span) n)
                (let ((j (+ i span -1))
                      (producers nil))
                  ;; Try all split points
                  (let ((k i))
                    (while (< k j)
                      (let ((left-set (gethash (cons i k) table))
                            (right-set (gethash (cons (1+ k) j) table)))
                        (when (and left-set right-set)
                          (dolist (rule grammar)
                            (let ((lhs (car rule))
                                  (rhs (cdr rule)))
                              (when (and (= (length rhs) 2)
                                         (memq (car rhs) left-set)
                                         (memq (nth 1 rhs) right-set))
                                (unless (memq lhs producers)
                                  (setq producers (cons lhs producers))))))))
                      (setq k (1+ k))))
                  (puthash (cons i j) producers table))
                (setq i (1+ i))))
            (setq span (1+ span))))
        ;; Check if start symbol derives the full string
        (let ((final-set (gethash (cons 0 (1- n)) table)))
          (list (and final-set (memq start-symbol final-set) t)
                n
                final-set)))))

  (unwind-protect
      (let ((grammar
             ;; Grammar for balanced parens in CNF:
             ;; S -> AB | SS | AC
             ;; A -> (    [terminal '(' ]
             ;; B -> )    [terminal ')' ]
             ;; C -> SB   [S followed by ')']
             '((S AB) (S SS) (S AC)
               (A ?\() (B ?\))
               (C S B))))
        (list
          ;; "()" -> S -> AB -> (terminal)(terminal) = accepted
          (car (funcall 'neovm--fl-cyk-parse grammar 'S "()"))
          ;; "(())" -> accepted
          (car (funcall 'neovm--fl-cyk-parse grammar 'S "(())"))
          ;; "()()" -> accepted
          (car (funcall 'neovm--fl-cyk-parse grammar 'S "()()"))
          ;; "((()))" -> accepted
          (car (funcall 'neovm--fl-cyk-parse grammar 'S "((()))"))
          ;; "(()())" -> accepted
          (car (funcall 'neovm--fl-cyk-parse grammar 'S "(()())"))
          ;; "(" -> rejected
          (car (funcall 'neovm--fl-cyk-parse grammar 'S "("))
          ;; ")(" -> rejected
          (car (funcall 'neovm--fl-cyk-parse grammar 'S ")("))
          ;; "(()" -> rejected
          (car (funcall 'neovm--fl-cyk-parse grammar 'S "(()"))
          ;; "" -> empty, special case
          (funcall 'neovm--fl-cyk-parse grammar 'S "")
          ;; Full result for "(())"
          (funcall 'neovm--fl-cyk-parse grammar 'S "(())")))
    (fmakunbound 'neovm--fl-cyk-parse)))"#;
    let expect =
        expect_test::expect![[r#""OK (nil nil nil nil nil nil nil nil (nil 0 nil) (nil 4 nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Chomsky Normal Form conversion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_formal_lang_cnf_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Convert a simple grammar to CNF
  ;; Input rules: (lhs . (sym1 sym2 ...)) where syms are terminals (chars) or nonterminals (symbols)
  ;; CNF: each rule is either A -> BC (two nonterminals) or A -> a (one terminal)

  (fset 'neovm--fl-is-terminal
    (lambda (sym)
      (integerp sym)))

  (fset 'neovm--fl-cnf-convert
    (lambda (rules terminals)
      "Convert grammar rules to CNF form.
       Returns list of CNF rules as (lhs . rhs-list)."
      (let ((cnf-rules nil)
            (term-map (make-hash-table :test 'equal))
            (next-nt-id 0))
        ;; Step 1: For each terminal in a non-unit rule, create a new NT
        (dolist (t2 terminals)
          (let ((nt-name (intern (format "T_%d" next-nt-id))))
            (puthash t2 nt-name term-map)
            (setq cnf-rules (cons (cons nt-name (list t2)) cnf-rules))
            (setq next-nt-id (1+ next-nt-id))))
        ;; Step 2: Process each rule
        (dolist (rule rules)
          (let ((lhs (car rule))
                (rhs (cdr rule)))
            (cond
              ;; Already unit: A -> a
              ((and (= (length rhs) 1) (funcall 'neovm--fl-is-terminal (car rhs)))
               (setq cnf-rules (cons (cons lhs rhs) cnf-rules)))
              ;; Binary: A -> B C (replace terminals with their NT proxies)
              ((= (length rhs) 2)
               (let ((r1 (if (funcall 'neovm--fl-is-terminal (car rhs))
                             (gethash (car rhs) term-map)
                           (car rhs)))
                     (r2 (if (funcall 'neovm--fl-is-terminal (nth 1 rhs))
                             (gethash (nth 1 rhs) term-map)
                           (nth 1 rhs))))
                 (setq cnf-rules (cons (cons lhs (list r1 r2)) cnf-rules))))
              ;; Longer: A -> B C D ... -> chain of binary rules
              ((> (length rhs) 2)
               (let* ((replaced (mapcar (lambda (s)
                                          (if (funcall 'neovm--fl-is-terminal s)
                                              (gethash s term-map)
                                            s))
                                        rhs))
                      (current-lhs lhs)
                      (remaining replaced))
                 (while (> (length remaining) 2)
                   (let ((new-nt (intern (format "X_%d" next-nt-id))))
                     (setq next-nt-id (1+ next-nt-id))
                     (setq cnf-rules
                           (cons (cons current-lhs (list (car remaining) new-nt))
                                 cnf-rules))
                     (setq current-lhs new-nt)
                     (setq remaining (cdr remaining))))
                 (setq cnf-rules
                       (cons (cons current-lhs remaining) cnf-rules)))))))
        (nreverse cnf-rules))))

  (unwind-protect
      (let* ((rules '((S . (A B))
                      (S . (S S))
                      (A . (?\())
                      (B . (?\)))))
             (cnf (funcall 'neovm--fl-cnf-convert rules '(?\( ?\)))))
        (list
          ;; Number of CNF rules
          (length cnf)
          ;; All rules are either binary NT or single terminal
          (let ((all-valid t))
            (dolist (rule cnf)
              (let ((rhs (cdr rule)))
                (unless (or (and (= (length rhs) 1) (funcall 'neovm--fl-is-terminal (car rhs)))
                            (and (= (length rhs) 2)
                                 (not (funcall 'neovm--fl-is-terminal (car rhs)))
                                 (not (funcall 'neovm--fl-is-terminal (nth 1 rhs)))))
                  (setq all-valid nil))))
            all-valid)
          ;; The rules themselves
          cnf
          ;; Test with a longer rule
          (let ((long-rules '((S . (A B C))
                              (A . (?a))
                              (B . (?b))
                              (C . (?c))))
                (long-cnf (funcall 'neovm--fl-cnf-convert long-rules '(?a ?b ?c))))
            (list (length long-cnf) long-cnf))))
    (fmakunbound 'neovm--fl-is-terminal)
    (fmakunbound 'neovm--fl-cnf-convert)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable long-rules)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Language recognition: classify strings by Chomsky hierarchy level
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_formal_lang_recognition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Recognizers for different formal language types

  ;; Regular: a*b* (DFA-recognizable)
  (fset 'neovm--fl-recog-a-star-b-star
    (lambda (input)
      "Recognize a*b* via DFA simulation."
      (let ((state 'in-a) (i 0) (len (length input)) (valid t))
        (while (and valid (< i len))
          (let ((ch (aref input i)))
            (cond
              ((and (eq state 'in-a) (= ch ?a)) nil)  ;; stay
              ((and (eq state 'in-a) (= ch ?b)) (setq state 'in-b))
              ((and (eq state 'in-b) (= ch ?b)) nil)  ;; stay
              (t (setq valid nil))))
          (setq i (1+ i)))
        valid)))

  ;; Context-free: a^n b^n (PDA-recognizable, not regular)
  (fset 'neovm--fl-recog-an-bn
    (lambda (input)
      "Recognize a^n b^n via counter."
      (let ((n (length input)) (i 0) (count 0) (state 'in-a) (valid t))
        (while (and valid (< i n))
          (let ((ch (aref input i)))
            (cond
              ((and (eq state 'in-a) (= ch ?a)) (setq count (1+ count)))
              ((and (eq state 'in-a) (= ch ?b))
               (setq state 'in-b)
               (setq count (1- count)))
              ((and (eq state 'in-b) (= ch ?b))
               (setq count (1- count)))
              (t (setq valid nil)))
            (when (< count 0) (setq valid nil)))
          (setq i (1+ i)))
        (and valid (= count 0)))))

  ;; Context-sensitive: a^n b^n c^n (not context-free)
  (fset 'neovm--fl-recog-an-bn-cn
    (lambda (input)
      "Recognize a^n b^n c^n."
      (let ((n (length input))
            (i 0) (a-count 0) (b-count 0) (c-count 0)
            (state 'in-a) (valid t))
        (while (and valid (< i n))
          (let ((ch (aref input i)))
            (cond
              ((and (eq state 'in-a) (= ch ?a)) (setq a-count (1+ a-count)))
              ((and (eq state 'in-a) (= ch ?b))
               (setq state 'in-b) (setq b-count 1))
              ((and (eq state 'in-b) (= ch ?b)) (setq b-count (1+ b-count)))
              ((and (eq state 'in-b) (= ch ?c))
               (setq state 'in-c) (setq c-count 1))
              ((and (eq state 'in-c) (= ch ?c)) (setq c-count (1+ c-count)))
              (t (setq valid nil))))
          (setq i (1+ i)))
        (and valid (> a-count 0)
             (= a-count b-count) (= b-count c-count)))))

  ;; Palindrome recognizer (context-free)
  (fset 'neovm--fl-recog-palindrome
    (lambda (input)
      "Recognize palindromes over {a,b}."
      (let ((n (length input)) (valid t) (i 0))
        (while (and valid (< i (/ n 2)))
          (unless (= (aref input i) (aref input (- n 1 i)))
            (setq valid nil))
          (setq i (1+ i)))
        valid)))

  (unwind-protect
      (list
        ;; a*b* tests
        (mapcar (lambda (s) (funcall 'neovm--fl-recog-a-star-b-star s))
                '("" "a" "b" "aabb" "aaabbb" "ab" "ba" "aba" "aab" "bba"))
        ;; a^n b^n tests
        (mapcar (lambda (s) (funcall 'neovm--fl-recog-an-bn s))
                '("" "ab" "aabb" "aaabbb" "a" "b" "aab" "abb" "ba" "abab"))
        ;; a^n b^n c^n tests
        (mapcar (lambda (s) (funcall 'neovm--fl-recog-an-bn-cn s))
                '("abc" "aabbcc" "aaabbbccc" "" "ab" "aabbc" "abcc" "abcabc"))
        ;; Palindrome tests
        (mapcar (lambda (s) (funcall 'neovm--fl-recog-palindrome s))
                '("" "a" "aa" "ab" "aba" "abb" "abba" "abab" "aabaa" "abcba"))
        ;; Classification: which hierarchy level?
        (let ((classify (lambda (input)
                          (list
                            (funcall 'neovm--fl-recog-a-star-b-star input)
                            (funcall 'neovm--fl-recog-an-bn input)
                            (funcall 'neovm--fl-recog-an-bn-cn input)
                            (funcall 'neovm--fl-recog-palindrome input)))))
          (mapcar classify '("" "aabb" "aabbcc" "aba" "ab" "abc"))))
    (fmakunbound 'neovm--fl-recog-a-star-b-star)
    (fmakunbound 'neovm--fl-recog-an-bn)
    (fmakunbound 'neovm--fl-recog-an-bn-cn)
    (fmakunbound 'neovm--fl-recog-palindrome)))"#;
    let expect = expect_test::expect![[
        r#""OK ((t t t t t t nil nil t nil) (t t t t nil nil nil nil nil nil) (t t t nil nil nil nil nil) (t t t nil t nil t nil t t) ((t t nil t) (t t nil nil) (nil nil t nil) (nil nil nil t) (t t nil nil) (nil nil t nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Grammar analysis: FIRST and FOLLOW sets
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_formal_lang_first_follow_sets() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Compute FIRST sets for a grammar
  ;; Grammar: list of (lhs . (rhs-symbols...))
  ;; Terminals are characters, nonterminals are symbols

  (fset 'neovm--fl-is-term
    (lambda (sym) (integerp sym)))

  (fset 'neovm--fl-compute-first
    (lambda (grammar nonterminals)
      "Compute FIRST sets. Returns hash-table of NT -> list of terminals."
      (let ((first-sets (make-hash-table))
            (changed t))
        ;; Initialize empty
        (dolist (nt nonterminals)
          (puthash nt nil first-sets))
        ;; Fixed-point iteration
        (while changed
          (setq changed nil)
          (dolist (rule grammar)
            (let ((lhs (car rule))
                  (rhs (cdr rule))
                  (old-set (gethash lhs first-sets)))
              (if (null rhs)
                  ;; Epsilon production: add nil marker
                  (unless (memq 'epsilon old-set)
                    (puthash lhs (cons 'epsilon old-set) first-sets)
                    (setq changed t))
                (let ((first-sym (car rhs)))
                  (if (funcall 'neovm--fl-is-term first-sym)
                      ;; Terminal: add to FIRST
                      (unless (memq first-sym old-set)
                        (puthash lhs (cons first-sym old-set) first-sets)
                        (setq changed t))
                    ;; Nonterminal: add its FIRST set (minus epsilon)
                    (dolist (sym (gethash first-sym first-sets))
                      (unless (or (eq sym 'epsilon) (memq sym old-set))
                        (puthash lhs (cons sym (gethash lhs first-sets)) first-sets)
                        (setq old-set (gethash lhs first-sets))
                        (setq changed t)))))))))
        first-sets)))

  (fset 'neovm--fl-first-to-alist
    (lambda (first-sets nonterminals)
      "Convert FIRST hash-table to sorted alist for stable comparison."
      (let ((result nil))
        (dolist (nt nonterminals)
          (let ((firsts (sort (copy-sequence (gethash nt first-sets))
                              (lambda (a b)
                                (cond
                                  ((eq a 'epsilon) t)
                                  ((eq b 'epsilon) nil)
                                  (t (< a b)))))))
            (setq result (cons (cons nt firsts) result))))
        (nreverse result))))

  (unwind-protect
      (let* (;; Grammar for simple arithmetic expressions:
             ;; E -> T E'
             ;; E' -> + T E' | epsilon
             ;; T -> F T'
             ;; T' -> * F T' | epsilon
             ;; F -> ( E ) | id
             ;; Using symbols: E, Ep (E'), T, Tp (T'), F
             ;; Terminals: ?+ ?* ?\( ?\) ?i (for 'id')
             (grammar '((E . (T Ep))
                        (Ep . (?+ T Ep))
                        (Ep)               ;; epsilon production
                        (T . (F Tp))
                        (Tp . (?* F Tp))
                        (Tp)               ;; epsilon production
                        (F . (?\( E ?\)))
                        (F . (?i))))
             (nts '(E Ep T Tp F))
             (firsts (funcall 'neovm--fl-compute-first grammar nts)))
        (list
          ;; FIRST sets as alist
          (funcall 'neovm--fl-first-to-alist firsts nts)
          ;; Verify specific expectations:
          ;; FIRST(E) should contain ( and id
          (let ((e-first (gethash 'E firsts)))
            (list (memq ?\( e-first) (memq ?i e-first)))
          ;; FIRST(Ep) should contain + and epsilon
          (let ((ep-first (gethash 'Ep firsts)))
            (list (memq ?+ ep-first) (memq 'epsilon ep-first)))
          ;; FIRST(F) should contain ( and id
          (let ((f-first (gethash 'F firsts)))
            (list (memq ?\( f-first) (memq ?i f-first)))))
    (fmakunbound 'neovm--fl-is-term)
    (fmakunbound 'neovm--fl-compute-first)
    (fmakunbound 'neovm--fl-first-to-alist)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable lhs)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
