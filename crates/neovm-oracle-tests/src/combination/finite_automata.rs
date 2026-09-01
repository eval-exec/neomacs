//! Oracle parity tests for finite automata and state machine patterns in Elisp.
//!
//! Tests DFA for binary divisibility by 3, NFA with epsilon transitions,
//! state machine for quoted string parsing, simple regex-to-NFA compilation,
//! Moore machine for sequence detection, and Mealy machine for protocol
//! state transitions.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// DFA for binary number divisibility by 3
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fa_dfa_div_by_3() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // DFA states represent remainder mod 3 when reading binary left-to-right
    // State 0: remainder 0 (accepting), State 1: remainder 1, State 2: remainder 2
    // Transition: on bit b, new_state = (old_state * 2 + b) mod 3
    let form = r#"(progn
  ;; Build transition table as hash-table of (state . input) -> new-state
  (defvar neovm--fa-d3-trans (make-hash-table :test 'equal))
  ;; State 0: 0*2+0=0, 0*2+1=1
  (puthash '(0 . 0) 0 neovm--fa-d3-trans)
  (puthash '(0 . 1) 1 neovm--fa-d3-trans)
  ;; State 1: 1*2+0=2, 1*2+1=0
  (puthash '(1 . 0) 2 neovm--fa-d3-trans)
  (puthash '(1 . 1) 0 neovm--fa-d3-trans)
  ;; State 2: 2*2+0=1, 2*2+1=2
  (puthash '(2 . 0) 1 neovm--fa-d3-trans)
  (puthash '(2 . 1) 2 neovm--fa-d3-trans)

  (fset 'neovm--fa-d3-run
    (lambda (binary-str)
      "Run DFA on binary string. Return (divisible-p final-state trace)."
      (let ((state 0)
            (trace nil)
            (i 0)
            (len (length binary-str)))
        (while (< i len)
          (let* ((bit (- (aref binary-str i) ?0))
                 (new-state (gethash (cons state bit) neovm--fa-d3-trans)))
            (setq trace (cons (list state bit new-state) trace))
            (setq state new-state))
          (setq i (1+ i)))
        (list (= state 0) state (nreverse trace)))))

  (fset 'neovm--fa-d3-check
    (lambda (n)
      "Convert integer to binary and check divisibility by 3."
      (let ((binary-str
             (let ((result nil) (num (abs n)))
               (if (= num 0) "0"
                 (progn
                   (while (> num 0)
                     (setq result (cons (+ ?0 (% num 2)) result))
                     (setq num (/ num 2)))
                   (concat result))))))
        (list n binary-str
              (car (funcall 'neovm--fa-d3-run binary-str))
              (= (% (abs n) 3) 0)))))

  (unwind-protect
      (list
        ;; Direct binary string tests
        (funcall 'neovm--fa-d3-run "0")      ;; 0 -> divisible
        (funcall 'neovm--fa-d3-run "11")     ;; 3 -> divisible
        (funcall 'neovm--fa-d3-run "110")    ;; 6 -> divisible
        (funcall 'neovm--fa-d3-run "10")     ;; 2 -> not divisible
        (funcall 'neovm--fa-d3-run "111")    ;; 7 -> not divisible
        (funcall 'neovm--fa-d3-run "1001")   ;; 9 -> divisible
        ;; Integer conversion tests: verify DFA matches modular arithmetic
        (mapcar (lambda (n)
                  (let ((result (funcall 'neovm--fa-d3-check n)))
                    ;; Assert DFA result matches actual mod check
                    (list (car result) (nth 2 result) (nth 3 result)
                          (eq (nth 2 result) (nth 3 result)))))
                '(0 1 2 3 4 5 6 7 8 9 10 11 12 15 21 42 99 100)))
    (fmakunbound 'neovm--fa-d3-run)
    (fmakunbound 'neovm--fa-d3-check)
    (makunbound 'neovm--fa-d3-trans)))"#;
    let expect = expect_test::expect![[
        r#""OK ((t 0 ((0 0 0))) (t 0 ((0 1 1) (1 1 0))) (t 0 ((0 1 1) (1 1 0) (0 0 0))) (nil 2 ((0 1 1) (1 0 2))) (nil 1 ((0 1 1) (1 1 0) (0 1 1))) (t 0 ((0 1 1) (1 0 2) (2 0 1) (1 1 0))) ((0 t t t) (1 nil nil t) (2 nil nil t) (3 t t t) (4 nil nil t) (5 nil nil t) (6 t t t) (7 nil nil t) (8 nil nil t) (9 t t t) (10 nil nil t) (11 nil nil t) (12 t t t) (15 t t t) (21 t t t) (42 t t t) (99 t t t) (100 nil nil t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// NFA simulation with epsilon transitions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fa_nfa_epsilon() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // NFA that accepts strings matching (a|b)*abb
    // States: 0(start), 1, 2, 3(accept)
    // Transitions stored in hash-table: (state . char) -> list-of-states
    // Epsilon transitions: (state . epsilon) -> list-of-states
    let form = r#"(progn
  (defvar neovm--fa-nfa-trans (make-hash-table :test 'equal))
  (defvar neovm--fa-nfa-eps (make-hash-table :test 'equal))

  ;; NFA for (a|b)*abb
  ;; State 0: a->0,1  b->0
  ;; State 1: b->2
  ;; State 2: b->3
  ;; State 3: accepting
  (puthash '(0 . ?a) '(0 1) neovm--fa-nfa-trans)
  (puthash '(0 . ?b) '(0) neovm--fa-nfa-trans)
  (puthash '(1 . ?b) '(2) neovm--fa-nfa-trans)
  (puthash '(2 . ?b) '(3) neovm--fa-nfa-trans)

  (fset 'neovm--fa-nfa-epsilon-closure
    (lambda (states)
      "Compute epsilon closure of a set of states."
      (let ((result (copy-sequence states))
            (worklist (copy-sequence states)))
        (while worklist
          (let ((s (car worklist)))
            (setq worklist (cdr worklist))
            (let ((eps-targets (gethash (cons s 'epsilon) neovm--fa-nfa-eps)))
              (dolist (t2 eps-targets)
                (unless (memq t2 result)
                  (setq result (cons t2 result))
                  (setq worklist (cons t2 worklist)))))))
        result)))

  (fset 'neovm--fa-nfa-move
    (lambda (states ch)
      "Compute set of states reachable from STATES on input CH."
      (let ((result nil))
        (dolist (s states)
          (let ((targets (gethash (cons s ch) neovm--fa-nfa-trans)))
            (dolist (t2 targets)
              (unless (memq t2 result)
                (setq result (cons t2 result))))))
        result)))

  (fset 'neovm--fa-nfa-run
    (lambda (input accepting)
      "Simulate NFA. Return t if any final state is in ACCEPTING."
      (let ((current (funcall 'neovm--fa-nfa-epsilon-closure '(0)))
            (i 0)
            (len (length input)))
        (while (< i len)
          (let ((ch (aref input i)))
            (setq current
                  (funcall 'neovm--fa-nfa-epsilon-closure
                           (funcall 'neovm--fa-nfa-move current ch))))
          (setq i (1+ i)))
        (let ((accepted nil))
          (dolist (s current)
            (when (memq s accepting)
              (setq accepted t)))
          accepted))))

  (unwind-protect
      (list
        ;; Should accept: ends with "abb"
        (funcall 'neovm--fa-nfa-run "abb" '(3))
        (funcall 'neovm--fa-nfa-run "aabb" '(3))
        (funcall 'neovm--fa-nfa-run "babb" '(3))
        (funcall 'neovm--fa-nfa-run "aababb" '(3))
        (funcall 'neovm--fa-nfa-run "ababb" '(3))
        ;; Should reject
        (funcall 'neovm--fa-nfa-run "ab" '(3))
        (funcall 'neovm--fa-nfa-run "a" '(3))
        (funcall 'neovm--fa-nfa-run "b" '(3))
        (funcall 'neovm--fa-nfa-run "" '(3))
        (funcall 'neovm--fa-nfa-run "abba" '(3))
        (funcall 'neovm--fa-nfa-run "aab" '(3))
        ;; Edge cases
        (funcall 'neovm--fa-nfa-run "bbbbabb" '(3))
        (funcall 'neovm--fa-nfa-run "aaaaaabb" '(3)))
    (fmakunbound 'neovm--fa-nfa-epsilon-closure)
    (fmakunbound 'neovm--fa-nfa-move)
    (fmakunbound 'neovm--fa-nfa-run)
    (makunbound 'neovm--fa-nfa-trans)
    (makunbound 'neovm--fa-nfa-eps)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t nil nil nil nil nil nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// State machine for parsing quoted strings with escape sequences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fa_quoted_string_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--fa-parse-strings
    (lambda (input)
      "Extract all quoted strings from input, handling escapes."
      (let ((state 'outside)
            (strings nil)
            (current nil)
            (i 0)
            (len (length input))
            (errors nil))
        (while (< i len)
          (let ((ch (aref input i)))
            (cond
              ;; Outside any string
              ((eq state 'outside)
               (cond
                 ((= ch ?\")
                  (setq state 'in-double-quote)
                  (setq current nil))
                 ((= ch ?\')
                  (setq state 'in-single-quote)
                  (setq current nil))))
              ;; Inside double-quoted string
              ((eq state 'in-double-quote)
               (cond
                 ((= ch ?\\)
                  (setq state 'escape-double))
                 ((= ch ?\")
                  (setq strings (cons (concat (nreverse current)) strings))
                  (setq state 'outside))
                 (t (setq current (cons ch current)))))
              ;; Escape inside double-quoted string
              ((eq state 'escape-double)
               (cond
                 ((= ch ?n) (setq current (cons ?\n current)))
                 ((= ch ?t) (setq current (cons ?\t current)))
                 ((= ch ?\\) (setq current (cons ?\\ current)))
                 ((= ch ?\") (setq current (cons ?\" current)))
                 (t (setq current (cons ?\\ current))
                    (setq current (cons ch current))))
               (setq state 'in-double-quote))
              ;; Inside single-quoted string (no escape processing)
              ((eq state 'in-single-quote)
               (if (= ch ?\')
                   (progn
                     (setq strings (cons (concat (nreverse current)) strings))
                     (setq state 'outside))
                 (setq current (cons ch current))))))
          (setq i (1+ i)))
        ;; Check for unterminated strings
        (unless (eq state 'outside)
          (setq errors (cons "unterminated string" errors)))
        (list (nreverse strings) errors
              (eq state 'outside)))))

  (unwind-protect
      (list
        ;; Simple double-quoted
        (funcall 'neovm--fa-parse-strings "say \"hello\" and \"world\"")
        ;; With escapes
        (funcall 'neovm--fa-parse-strings "path \"C:\\\\Users\\\\test\"")
        ;; Newline escape
        (funcall 'neovm--fa-parse-strings "msg \"line1\\nline2\"")
        ;; Single-quoted (no escape processing)
        (funcall 'neovm--fa-parse-strings "it's 'raw\\nstring' here")
        ;; Mixed quotes
        (funcall 'neovm--fa-parse-strings "a \"double\" and 'single' mix")
        ;; Empty strings
        (funcall 'neovm--fa-parse-strings "empty \"\" and ''")
        ;; Escaped quote inside string
        (funcall 'neovm--fa-parse-strings "say \"he said \\\"hi\\\"\"")
        ;; No strings at all
        (funcall 'neovm--fa-parse-strings "no strings here")
        ;; Unterminated
        (funcall 'neovm--fa-parse-strings "broken \"string"))
    (fmakunbound 'neovm--fa-parse-strings)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"hello\" \"world\") nil t) ((\"C:\\\\Users\\\\test\") nil t) ((\"line1\\nline2\") nil t) ((\"s \") (\"unterminated string\") nil) ((\"double\" \"single\") nil t) ((\"\" \"\") nil t) ((\"he said \\\"hi\\\"\") nil t) (nil nil t) (nil (\"unterminated string\") nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Simple regex-to-NFA compilation (subset: literals, concat, alternation, *)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fa_regex_to_nfa() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compile simple regex patterns into NFA and match strings
    // Supported: literal chars, concatenation, | (alternation), * (Kleene star)
    // Uses Thompson's construction with explicit state numbering
    let form = r#"(progn
  (defvar neovm--fa-rx-next-state 0)

  (fset 'neovm--fa-rx-new-state
    (lambda ()
      (let ((s neovm--fa-rx-next-state))
        (setq neovm--fa-rx-next-state (1+ neovm--fa-rx-next-state))
        s)))

  ;; NFA fragment: (start end transitions)
  ;; transitions: list of (from char-or-nil to) where nil = epsilon
  (fset 'neovm--fa-rx-literal
    (lambda (ch)
      (let ((s (funcall 'neovm--fa-rx-new-state))
            (e (funcall 'neovm--fa-rx-new-state)))
        (list s e (list (list s ch e))))))

  (fset 'neovm--fa-rx-concat
    (lambda (nfa1 nfa2)
      (let ((trans (append (nth 2 nfa1) (nth 2 nfa2)
                           (list (list (nth 1 nfa1) nil (nth 0 nfa2))))))
        (list (nth 0 nfa1) (nth 1 nfa2) trans))))

  (fset 'neovm--fa-rx-alt
    (lambda (nfa1 nfa2)
      (let ((s (funcall 'neovm--fa-rx-new-state))
            (e (funcall 'neovm--fa-rx-new-state)))
        (list s e
              (append (nth 2 nfa1) (nth 2 nfa2)
                      (list (list s nil (nth 0 nfa1))
                            (list s nil (nth 0 nfa2))
                            (list (nth 1 nfa1) nil e)
                            (list (nth 1 nfa2) nil e)))))))

  (fset 'neovm--fa-rx-star
    (lambda (nfa1)
      (let ((s (funcall 'neovm--fa-rx-new-state))
            (e (funcall 'neovm--fa-rx-new-state)))
        (list s e
              (append (nth 2 nfa1)
                      (list (list s nil (nth 0 nfa1))
                            (list s nil e)
                            (list (nth 1 nfa1) nil (nth 0 nfa1))
                            (list (nth 1 nfa1) nil e)))))))

  ;; Epsilon closure on transition list
  (fset 'neovm--fa-rx-eps-closure
    (lambda (states transitions)
      (let ((result (copy-sequence states))
            (worklist (copy-sequence states)))
        (while worklist
          (let ((s (car worklist)))
            (setq worklist (cdr worklist))
            (dolist (tr transitions)
              (when (and (= (nth 0 tr) s) (null (nth 1 tr)))
                (let ((target (nth 2 tr)))
                  (unless (memq target result)
                    (setq result (cons target result))
                    (setq worklist (cons target worklist))))))))
        result)))

  ;; Move on character
  (fset 'neovm--fa-rx-move
    (lambda (states ch transitions)
      (let ((result nil))
        (dolist (s states)
          (dolist (tr transitions)
            (when (and (= (nth 0 tr) s) (eql (nth 1 tr) ch))
              (unless (memq (nth 2 tr) result)
                (setq result (cons (nth 2 tr) result))))))
        result)))

  ;; Match
  (fset 'neovm--fa-rx-match
    (lambda (nfa input)
      (let ((transitions (nth 2 nfa))
            (accept (nth 1 nfa))
            (current (funcall 'neovm--fa-rx-eps-closure (list (nth 0 nfa)) (nth 2 nfa)))
            (i 0)
            (len (length input)))
        (while (< i len)
          (setq current
                (funcall 'neovm--fa-rx-eps-closure
                         (funcall 'neovm--fa-rx-move current (aref input i) transitions)
                         transitions))
          (setq i (1+ i)))
        (if (memq accept current) t nil))))

  (unwind-protect
      (progn
        ;; Test 1: literal "a"
        (setq neovm--fa-rx-next-state 0)
        (let ((nfa-a (funcall 'neovm--fa-rx-literal ?a)))
          ;; Test 2: concat "ab"
          (setq neovm--fa-rx-next-state 0)
          (let ((nfa-ab (funcall 'neovm--fa-rx-concat
                                  (funcall 'neovm--fa-rx-literal ?a)
                                  (funcall 'neovm--fa-rx-literal ?b))))
            ;; Test 3: alt "a|b"
            (setq neovm--fa-rx-next-state 0)
            (let ((nfa-a-or-b (funcall 'neovm--fa-rx-alt
                                        (funcall 'neovm--fa-rx-literal ?a)
                                        (funcall 'neovm--fa-rx-literal ?b))))
              ;; Test 4: star "a*"
              (setq neovm--fa-rx-next-state 0)
              (let ((nfa-a-star (funcall 'neovm--fa-rx-star
                                         (funcall 'neovm--fa-rx-literal ?a))))
                ;; Test 5: complex "(a|b)*c"
                (setq neovm--fa-rx-next-state 0)
                (let ((nfa-complex
                       (funcall 'neovm--fa-rx-concat
                                (funcall 'neovm--fa-rx-star
                                         (funcall 'neovm--fa-rx-alt
                                                  (funcall 'neovm--fa-rx-literal ?a)
                                                  (funcall 'neovm--fa-rx-literal ?b)))
                                (funcall 'neovm--fa-rx-literal ?c))))
                  (list
                    ;; "a" matches "a" but not "b" or ""
                    (funcall 'neovm--fa-rx-match nfa-a "a")
                    (funcall 'neovm--fa-rx-match nfa-a "b")
                    (funcall 'neovm--fa-rx-match nfa-a "")
                    ;; "ab" matches "ab" but not "a" or "ba"
                    (funcall 'neovm--fa-rx-match nfa-ab "ab")
                    (funcall 'neovm--fa-rx-match nfa-ab "a")
                    (funcall 'neovm--fa-rx-match nfa-ab "ba")
                    ;; "a|b" matches "a" and "b" but not "c" or "ab"
                    (funcall 'neovm--fa-rx-match nfa-a-or-b "a")
                    (funcall 'neovm--fa-rx-match nfa-a-or-b "b")
                    (funcall 'neovm--fa-rx-match nfa-a-or-b "c")
                    (funcall 'neovm--fa-rx-match nfa-a-or-b "ab")
                    ;; "a*" matches "", "a", "aaa" but not "b"
                    (funcall 'neovm--fa-rx-match nfa-a-star "")
                    (funcall 'neovm--fa-rx-match nfa-a-star "a")
                    (funcall 'neovm--fa-rx-match nfa-a-star "aaa")
                    (funcall 'neovm--fa-rx-match nfa-a-star "b")
                    ;; "(a|b)*c" matches "c", "ac", "bc", "ababc", not "ab"
                    (funcall 'neovm--fa-rx-match nfa-complex "c")
                    (funcall 'neovm--fa-rx-match nfa-complex "ac")
                    (funcall 'neovm--fa-rx-match nfa-complex "bc")
                    (funcall 'neovm--fa-rx-match nfa-complex "ababc")
                    (funcall 'neovm--fa-rx-match nfa-complex "ab")
                    (funcall 'neovm--fa-rx-match nfa-complex ""))))))))
    (fmakunbound 'neovm--fa-rx-new-state)
    (fmakunbound 'neovm--fa-rx-literal)
    (fmakunbound 'neovm--fa-rx-concat)
    (fmakunbound 'neovm--fa-rx-alt)
    (fmakunbound 'neovm--fa-rx-star)
    (fmakunbound 'neovm--fa-rx-eps-closure)
    (fmakunbound 'neovm--fa-rx-move)
    (fmakunbound 'neovm--fa-rx-match)
    (makunbound 'neovm--fa-rx-next-state)))"#;
    let expect = expect_test::expect![[
        r#""OK (t nil nil t nil nil t t nil nil t t t nil t t t t nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Moore machine for sequence detection (detect "101" in bit stream)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fa_moore_sequence_detector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Moore machine: output depends only on current state
    // States: S0(start), S1(saw 1), S2(saw 10), S3(saw 101, output=1)
    // Overlapping detection: after detecting "101", stay ready for next
    let form = r#"(progn
  (defvar neovm--fa-moore-trans (make-hash-table :test 'equal))
  (defvar neovm--fa-moore-output (make-hash-table))

  ;; Transitions for "101" detector
  ;; S0 --0--> S0, S0 --1--> S1
  (puthash '(0 . 0) 0 neovm--fa-moore-trans)
  (puthash '(0 . 1) 1 neovm--fa-moore-trans)
  ;; S1 --0--> S2, S1 --1--> S1
  (puthash '(1 . 0) 2 neovm--fa-moore-trans)
  (puthash '(1 . 1) 1 neovm--fa-moore-trans)
  ;; S2 --0--> S0, S2 --1--> S3
  (puthash '(2 . 0) 0 neovm--fa-moore-trans)
  (puthash '(2 . 1) 3 neovm--fa-moore-trans)
  ;; S3 --0--> S2 (overlapping: "101" -> seen "1", then "0"), S3 --1--> S1
  (puthash '(3 . 0) 2 neovm--fa-moore-trans)
  (puthash '(3 . 1) 1 neovm--fa-moore-trans)

  ;; Output: 1 only in state 3
  (puthash 0 0 neovm--fa-moore-output)
  (puthash 1 0 neovm--fa-moore-output)
  (puthash 2 0 neovm--fa-moore-output)
  (puthash 3 1 neovm--fa-moore-output)

  (fset 'neovm--fa-moore-run
    (lambda (bit-string)
      "Run Moore machine on bit string. Return (outputs detection-count state-trace)."
      (let ((state 0)
            (outputs nil)
            (trace nil)
            (count 0)
            (i 0)
            (len (length bit-string)))
        (while (< i len)
          (let* ((bit (- (aref bit-string i) ?0))
                 (new-state (gethash (cons state bit) neovm--fa-moore-trans))
                 (out (gethash new-state neovm--fa-moore-output)))
            (setq trace (cons new-state trace))
            (setq outputs (cons out outputs))
            (when (= out 1) (setq count (1+ count)))
            (setq state new-state))
          (setq i (1+ i)))
        (list (nreverse outputs) count (nreverse trace)))))

  (unwind-protect
      (list
        ;; "101" -> detect at position 2
        (funcall 'neovm--fa-moore-run "101")
        ;; "10101" -> overlapping: detect at positions 2 and 4
        (funcall 'neovm--fa-moore-run "10101")
        ;; "1010101" -> 3 detections
        (funcall 'neovm--fa-moore-run "1010101")
        ;; "1100101" -> 1 detection
        (funcall 'neovm--fa-moore-run "1100101")
        ;; "0000" -> no detection
        (funcall 'neovm--fa-moore-run "0000")
        ;; "1111" -> no detection
        (funcall 'neovm--fa-moore-run "1111")
        ;; Empty input
        (funcall 'neovm--fa-moore-run "")
        ;; Long stream with multiple overlaps
        (let ((result (funcall 'neovm--fa-moore-run "110101011010")))
          (list (nth 1 result)  ;; just the count
                (length (nth 0 result)))))
    (fmakunbound 'neovm--fa-moore-run)
    (makunbound 'neovm--fa-moore-trans)
    (makunbound 'neovm--fa-moore-output)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 0 1) 1 (1 2 3)) ((0 0 1 0 1) 2 (1 2 3 2 3)) ((0 0 1 0 1 0 1) 3 (1 2 3 2 3 2 3)) ((0 0 0 0 0 0 1) 1 (1 1 2 0 1 2 3)) ((0 0 0 0) 0 (0 0 0 0)) ((0 0 0 0) 0 (1 1 1 1)) (nil 0 nil) (4 12))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Mealy machine for protocol state transitions with output
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fa_mealy_protocol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simple protocol: IDLE -> CONNECTING -> CONNECTED -> TRANSMITTING -> CONNECTED
    // Events: connect, connected, send, ack, disconnect, error, reset
    // Mealy: output (action) depends on state AND input
    let form = r#"(progn
  (defvar neovm--fa-mealy-trans (make-hash-table :test 'equal))
  (defvar neovm--fa-mealy-out (make-hash-table :test 'equal))

  ;; (state . event) -> new-state
  (puthash '(idle . connect) 'connecting neovm--fa-mealy-trans)
  (puthash '(idle . disconnect) 'idle neovm--fa-mealy-trans)
  (puthash '(connecting . connected) 'connected neovm--fa-mealy-trans)
  (puthash '(connecting . error) 'idle neovm--fa-mealy-trans)
  (puthash '(connecting . reset) 'idle neovm--fa-mealy-trans)
  (puthash '(connected . send) 'transmitting neovm--fa-mealy-trans)
  (puthash '(connected . disconnect) 'idle neovm--fa-mealy-trans)
  (puthash '(connected . error) 'idle neovm--fa-mealy-trans)
  (puthash '(transmitting . ack) 'connected neovm--fa-mealy-trans)
  (puthash '(transmitting . error) 'connected neovm--fa-mealy-trans)
  (puthash '(transmitting . reset) 'idle neovm--fa-mealy-trans)

  ;; (state . event) -> output action
  (puthash '(idle . connect) "SYN_SENT" neovm--fa-mealy-out)
  (puthash '(idle . disconnect) "ALREADY_IDLE" neovm--fa-mealy-out)
  (puthash '(connecting . connected) "ACK_RECEIVED" neovm--fa-mealy-out)
  (puthash '(connecting . error) "CONN_FAILED" neovm--fa-mealy-out)
  (puthash '(connecting . reset) "CONN_RESET" neovm--fa-mealy-out)
  (puthash '(connected . send) "DATA_QUEUED" neovm--fa-mealy-out)
  (puthash '(connected . disconnect) "FIN_SENT" neovm--fa-mealy-out)
  (puthash '(connected . error) "CONN_LOST" neovm--fa-mealy-out)
  (puthash '(transmitting . ack) "DATA_CONFIRMED" neovm--fa-mealy-out)
  (puthash '(transmitting . error) "RETRANSMIT" neovm--fa-mealy-out)
  (puthash '(transmitting . reset) "HARD_RESET" neovm--fa-mealy-out)

  (fset 'neovm--fa-mealy-run
    (lambda (events)
      "Run Mealy machine on event sequence. Return (final-state outputs log)."
      (let ((state 'idle)
            (outputs nil)
            (log nil))
        (dolist (event events)
          (let ((new-state (gethash (cons state event) neovm--fa-mealy-trans))
                (output (gethash (cons state event) neovm--fa-mealy-out)))
            (if new-state
                (progn
                  (setq log (cons (list state event output new-state) log))
                  (setq outputs (cons output outputs))
                  (setq state new-state))
              ;; Invalid transition
              (setq log (cons (list state event "INVALID" state) log))
              (setq outputs (cons "INVALID" outputs)))))
        (list state (nreverse outputs) (nreverse log)))))

  (unwind-protect
      (list
        ;; Happy path: connect -> connected -> send -> ack -> disconnect
        (funcall 'neovm--fa-mealy-run
                 '(connect connected send ack disconnect))
        ;; Error during connection
        (funcall 'neovm--fa-mealy-run
                 '(connect error connect connected))
        ;; Multiple sends with acks
        (funcall 'neovm--fa-mealy-run
                 '(connect connected send ack send ack send ack disconnect))
        ;; Transmission error then retry
        (funcall 'neovm--fa-mealy-run
                 '(connect connected send error send ack disconnect))
        ;; Invalid event in wrong state
        (funcall 'neovm--fa-mealy-run
                 '(send))
        ;; Hard reset during transmission
        (funcall 'neovm--fa-mealy-run
                 '(connect connected send reset connect connected))
        ;; Disconnect while already idle
        (funcall 'neovm--fa-mealy-run
                 '(disconnect disconnect)))
    (fmakunbound 'neovm--fa-mealy-run)
    (makunbound 'neovm--fa-mealy-trans)
    (makunbound 'neovm--fa-mealy-out)))"#;
    let expect = expect_test::expect![[
        r#""OK ((idle (\"SYN_SENT\" \"ACK_RECEIVED\" \"DATA_QUEUED\" \"DATA_CONFIRMED\" \"FIN_SENT\") ((idle connect \"SYN_SENT\" connecting) (connecting connected \"ACK_RECEIVED\" connected) (connected send \"DATA_QUEUED\" transmitting) (transmitting ack \"DATA_CONFIRMED\" connected) (connected disconnect \"FIN_SENT\" idle))) (connected (\"SYN_SENT\" \"CONN_FAILED\" \"SYN_SENT\" \"ACK_RECEIVED\") ((idle connect \"SYN_SENT\" connecting) (connecting error \"CONN_FAILED\" idle) (idle connect \"SYN_SENT\" connecting) (connecting connected \"ACK_RECEIVED\" connected))) (idle (\"SYN_SENT\" \"ACK_RECEIVED\" \"DATA_QUEUED\" \"DATA_CONFIRMED\" \"DATA_QUEUED\" \"DATA_CONFIRMED\" \"DATA_QUEUED\" \"DATA_CONFIRMED\" \"FIN_SENT\") ((idle connect \"SYN_SENT\" connecting) (connecting connected \"ACK_RECEIVED\" connected) (connected send \"DATA_QUEUED\" transmitting) (transmitting ack \"DATA_CONFIRMED\" connected) (connected send \"DATA_QUEUED\" transmitting) (transmitting ack \"DATA_CONFIRMED\" connected) (connected send \"DATA_QUEUED\" transmitting) (transmitting ack \"DATA_CONFIRMED\" connected) (connected disconnect \"FIN_SENT\" idle))) (idle (\"SYN_SENT\" \"ACK_RECEIVED\" \"DATA_QUEUED\" \"RETRANSMIT\" \"DATA_QUEUED\" \"DATA_CONFIRMED\" \"FIN_SENT\") ((idle connect \"SYN_SENT\" connecting) (connecting connected \"ACK_RECEIVED\" connected) (connected send \"DATA_QUEUED\" transmitting) (transmitting error \"RETRANSMIT\" connected) (connected send \"DATA_QUEUED\" transmitting) (transmitting ack \"DATA_CONFIRMED\" connected) (connected disconnect \"FIN_SENT\" idle))) (idle (\"INVALID\") ((idle send \"INVALID\" idle))) (connected (\"SYN_SENT\" \"ACK_RECEIVED\" \"DATA_QUEUED\" \"HARD_RESET\" \"SYN_SENT\" \"ACK_RECEIVED\") ((idle connect \"SYN_SENT\" connecting) (connecting connected \"ACK_RECEIVED\" connected) (connected send \"DATA_QUEUED\" transmitting) (transmitting reset \"HARD_RESET\" idle) (idle connect \"SYN_SENT\" connecting) (connecting connected \"ACK_RECEIVED\" connected))) (idle (\"ALREADY_IDLE\" \"ALREADY_IDLE\") ((idle disconnect \"ALREADY_IDLE\" idle) (idle disconnect \"ALREADY_IDLE\" idle))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Deterministic pushdown automaton (DPDA) for matching nested structures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fa_dpda_nested_structures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // DPDA that validates and computes nesting depth of XML-like tags
    // Input: sequence of 'open and 'close symbols with tag names
    // Validates proper nesting and returns max depth
    let form = r#"(progn
  (fset 'neovm--fa-dpda-validate
    (lambda (tokens)
      "Validate XML-like nesting. Tokens: ((open . tag) (close . tag) ...).
       Return (valid max-depth tag-counts)."
      (let ((stack nil)
            (depth 0)
            (max-depth 0)
            (valid t)
            (error-msg nil)
            (tag-counts (make-hash-table :test 'equal))
            (i 0))
        (dolist (tok tokens)
          (when valid
            (let ((kind (car tok))
                  (tag (cdr tok)))
              (cond
                ((eq kind 'open)
                 (setq stack (cons tag stack))
                 (setq depth (1+ depth))
                 (when (> depth max-depth) (setq max-depth depth))
                 (puthash tag (1+ (gethash tag tag-counts 0)) tag-counts))
                ((eq kind 'close)
                 (if (null stack)
                     (progn
                       (setq valid nil)
                       (setq error-msg (concat "unexpected close: " tag)))
                   (if (string= (car stack) tag)
                       (progn
                         (setq stack (cdr stack))
                         (setq depth (1- depth)))
                     (setq valid nil)
                     (setq error-msg
                           (concat "mismatch: expected " (car stack)
                                   " got " tag))))))))
          (setq i (1+ i)))
        (when (and valid stack)
          (setq valid nil)
          (setq error-msg (concat "unclosed: " (car stack))))
        (let ((counts nil))
          (maphash (lambda (k v) (setq counts (cons (cons k v) counts)))
                   tag-counts)
          (setq counts (sort counts (lambda (a b) (string< (car a) (car b)))))
          (if valid
              (list t max-depth counts)
            (list nil error-msg counts))))))

  (unwind-protect
      (list
        ;; Simple valid: <div></div>
        (funcall 'neovm--fa-dpda-validate
                 '((open . "div") (close . "div")))
        ;; Nested: <div><p><span></span></p></div>
        (funcall 'neovm--fa-dpda-validate
                 '((open . "div") (open . "p") (open . "span")
                   (close . "span") (close . "p") (close . "div")))
        ;; Siblings: <p></p><p></p>
        (funcall 'neovm--fa-dpda-validate
                 '((open . "p") (close . "p") (open . "p") (close . "p")))
        ;; Mismatch: <div></span>
        (funcall 'neovm--fa-dpda-validate
                 '((open . "div") (close . "span")))
        ;; Unclosed tag
        (funcall 'neovm--fa-dpda-validate
                 '((open . "div") (open . "p") (close . "p")))
        ;; Empty input
        (funcall 'neovm--fa-dpda-validate nil)
        ;; Deep nesting
        (funcall 'neovm--fa-dpda-validate
                 '((open . "a") (open . "b") (open . "c") (open . "d")
                   (close . "d") (close . "c") (close . "b") (close . "a")))
        ;; Complex with repeated tags
        (funcall 'neovm--fa-dpda-validate
                 '((open . "ul") (open . "li") (close . "li")
                   (open . "li") (open . "a") (close . "a") (close . "li")
                   (open . "li") (close . "li") (close . "ul"))))
    (fmakunbound 'neovm--fa-dpda-validate)))"#;
    let expect = expect_test::expect![[
        r#""OK ((t 1 ((\"div\" . 1))) (t 3 ((\"div\" . 1) (\"p\" . 1) (\"span\" . 1))) (t 1 ((\"p\" . 2))) (nil \"mismatch: expected div got span\" ((\"div\" . 1))) (nil \"unclosed: div\" ((\"div\" . 1) (\"p\" . 1))) (t 0 nil) (t 4 ((\"a\" . 1) (\"b\" . 1) (\"c\" . 1) (\"d\" . 1))) (t 3 ((\"a\" . 1) (\"li\" . 3) (\"ul\" . 1))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
