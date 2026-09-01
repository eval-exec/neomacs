//! Complex oracle parity tests for state machine implementations in Elisp.
//!
//! Tests DFA-based tokenizer, balanced parentheses validator, protocol
//! parser, Mealy machine transliteration, PDA-like recursive descent
//! with explicit stack, and NFA-like regex pattern matching.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// DFA-based tokenizer: identifiers, numbers, strings, operators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sm_dfa_tokenizer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-tokenize
    (lambda (input)
      (let ((tokens nil)
            (i 0)
            (len (length input)))
        (while (< i len)
          (let ((ch (aref input i)))
            (cond
              ;; Skip whitespace
              ((or (= ch ?\s) (= ch ?\t) (= ch ?\n))
               (setq i (1+ i)))
              ;; Number: digits, optional dot
              ((and (>= ch ?0) (<= ch ?9))
               (let ((start i) (has-dot nil))
                 (while (and (< i len)
                             (let ((c (aref input i)))
                               (or (and (>= c ?0) (<= c ?9))
                                   (and (= c ?.) (not has-dot)
                                        (progn (setq has-dot t) t)))))
                   (setq i (1+ i)))
                 (setq tokens
                       (cons (list (if has-dot 'float 'int)
                                   (substring input start i))
                             tokens))))
              ;; Identifier: alpha or underscore, then alnum/underscore
              ((or (and (>= ch ?a) (<= ch ?z))
                   (and (>= ch ?A) (<= ch ?Z))
                   (= ch ?_))
               (let ((start i))
                 (while (and (< i len)
                             (let ((c (aref input i)))
                               (or (and (>= c ?a) (<= c ?z))
                                   (and (>= c ?A) (<= c ?Z))
                                   (and (>= c ?0) (<= c ?9))
                                   (= c ?_))))
                   (setq i (1+ i)))
                 (setq tokens
                       (cons (list 'ident (substring input start i))
                             tokens))))
              ;; String: double-quoted
              ((= ch ?\")
               (let ((start i))
                 (setq i (1+ i))
                 (while (and (< i len) (/= (aref input i) ?\"))
                   (setq i (1+ i)))
                 (when (< i len) (setq i (1+ i)))
                 (setq tokens
                       (cons (list 'string (substring input (1+ start) (1- i)))
                             tokens))))
              ;; Two-char operators
              ((and (< (1+ i) len)
                    (let ((two (substring input i (+ i 2))))
                      (member two '("==" "!=" "<=" ">="))))
               (setq tokens
                     (cons (list 'op (substring input i (+ i 2)))
                           tokens))
               (setq i (+ i 2)))
              ;; Single-char operators
              ((member ch '(?+ ?- ?* ?/ ?= ?< ?> ?( ?) ?\; ?,))
               (setq tokens
                     (cons (list 'op (char-to-string ch))
                           tokens))
               (setq i (1+ i)))
              ;; Unknown
              (t (setq tokens
                       (cons (list 'unknown (char-to-string ch))
                             tokens))
                 (setq i (1+ i))))))
        (nreverse tokens))))
  (unwind-protect
      (list
        (funcall 'neovm--test-tokenize "x = 42")
        (funcall 'neovm--test-tokenize "foo_bar + 3.14")
        (funcall 'neovm--test-tokenize "if (x >= 10) y = x * 2;")
        (funcall 'neovm--test-tokenize "\"hello\" != \"world\"")
        (funcall 'neovm--test-tokenize ""))
    (fmakunbound 'neovm--test-tokenize)))"#;
    let expect = expect_test::expect![[
        r#""OK (((ident \"x\") (op \"=\") (int \"42\")) ((ident \"foo_bar\") (op \"+\") (float \"3.14\")) ((ident \"if\") (op \"(\") (ident \"x\") (op \">=\") (int \"10\") (op \")\") (ident \"y\") (op \"=\") (ident \"x\") (op \"*\") (int \"2\") (op \";\")) ((string \"hello\") (op \"!=\") (string \"world\")) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// State machine for balanced parentheses validation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sm_balanced_parens() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Validates balanced parens/brackets/braces using explicit stack
    let form = r#"(progn
  (fset 'neovm--test-balanced
    (lambda (input)
      (let ((stack nil)
            (i 0)
            (len (length input))
            (valid t)
            (openers (make-hash-table))
            (closers (make-hash-table)))
        ;; Map closer -> opener
        (puthash ?\) ?\( closers)
        (puthash ?\] ?\[ closers)
        (puthash ?\} ?\{ closers)
        ;; Mark openers
        (puthash ?\( t openers)
        (puthash ?\[ t openers)
        (puthash ?\{ t openers)
        (while (and (< i len) valid)
          (let ((ch (aref input i)))
            (cond
              ((gethash ch openers)
               (setq stack (cons ch stack)))
              ((gethash ch closers)
               (if (and stack (= (car stack) (gethash ch closers)))
                   (setq stack (cdr stack))
                 (setq valid nil)))))
          (setq i (1+ i)))
        (and valid (null stack)))))
  (unwind-protect
      (list
        (funcall 'neovm--test-balanced "()")
        (funcall 'neovm--test-balanced "()[]{}")
        (funcall 'neovm--test-balanced "((([])))")
        (funcall 'neovm--test-balanced "{[()()]}")
        (funcall 'neovm--test-balanced "(]")
        (funcall 'neovm--test-balanced "(()")
        (funcall 'neovm--test-balanced ")(")
        (funcall 'neovm--test-balanced "a(b[c{d}e]f)g")
        (funcall 'neovm--test-balanced "")
        (funcall 'neovm--test-balanced "{[(])}"))
    (fmakunbound 'neovm--test-balanced)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t nil nil nil t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// State machine-based protocol parser (HTTP-like request line)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sm_protocol_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse "METHOD /path HTTP/x.y" into components using states
    let form = r#"(progn
  (fset 'neovm--test-parse-request
    (lambda (input)
      (let ((state 'method)
            (i 0)
            (len (length input))
            (method-start 0) (method-end nil)
            (path-start nil) (path-end nil)
            (version-start nil)
            (error nil))
        (while (and (< i len) (not error))
          (let ((ch (aref input i)))
            (cond
              ;; State: reading method
              ((eq state 'method)
               (if (= ch ?\s)
                   (progn
                     (setq method-end i)
                     (setq state 'pre-path))
                 (unless (and (>= ch ?A) (<= ch ?Z))
                   (setq error (format "bad method char at %d" i)))))
              ;; State: skip spaces before path
              ((eq state 'pre-path)
               (unless (= ch ?\s)
                 (if (= ch ?/)
                     (progn (setq path-start i) (setq state 'path))
                   (setq error (format "expected / at %d" i)))))
              ;; State: reading path
              ((eq state 'path)
               (when (= ch ?\s)
                 (setq path-end i)
                 (setq state 'pre-version)))
              ;; State: skip spaces before version
              ((eq state 'pre-version)
               (unless (= ch ?\s)
                 (setq version-start i)
                 (setq state 'version)))
              ;; State: reading version
              ((eq state 'version)
               nil)))  ;; accept all chars
          (setq i (1+ i)))
        (if error
            (list 'error error)
          (list 'ok
                (if method-end (substring input method-start method-end) nil)
                (if (and path-start path-end)
                    (substring input path-start path-end)
                  (if path-start (substring input path-start) nil))
                (if version-start (substring input version-start) nil))))))
  (unwind-protect
      (list
        (funcall 'neovm--test-parse-request "GET /index.html HTTP/1.1")
        (funcall 'neovm--test-parse-request "POST /api/data HTTP/2.0")
        (funcall 'neovm--test-parse-request "DELETE /items/42 HTTP/1.0")
        (funcall 'neovm--test-parse-request "GET / HTTP/1.1")
        (funcall 'neovm--test-parse-request "GET /path")
        (funcall 'neovm--test-parse-request "bad request"))
    (fmakunbound 'neovm--test-parse-request)))"#;
    let expect = expect_test::expect![[
        r#""OK ((ok \"GET\" \"/index.html\" \"HTTP/1.1\") (ok \"POST\" \"/api/data\" \"HTTP/2.0\") (ok \"DELETE\" \"/items/42\" \"HTTP/1.0\") (ok \"GET\" \"/\" \"HTTP/1.1\") (ok \"GET\" \"/path\" nil) (error \"bad method char at 0\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Mealy machine: output on transitions (transliteration)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sm_mealy_transliteration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Mealy machine that converts a simple phonetic input to transformed output
    // Rules: "ch" -> "X", "sh" -> "Y", "th" -> "Z", others pass through
    let form = r#"(progn
  (fset 'neovm--test-mealy
    (lambda (input)
      (let ((state 'normal)
            (output nil)
            (i 0)
            (len (length input)))
        (while (< i len)
          (let ((ch (aref input i)))
            (cond
              ;; Normal state: check for digraph starters
              ((eq state 'normal)
               (cond
                 ((= ch ?c) (setq state 'after-c))
                 ((= ch ?s) (setq state 'after-s))
                 ((= ch ?t) (setq state 'after-t))
                 (t (setq output (cons ch output)))))
              ;; After 'c': check for 'h'
              ((eq state 'after-c)
               (if (= ch ?h)
                   (progn (setq output (cons ?X output))
                          (setq state 'normal))
                 ;; Not a digraph: emit 'c' then reprocess current char
                 (setq output (cons ?c output))
                 (setq state 'normal)
                 (setq i (1- i))))  ;; re-process current char
              ;; After 's': check for 'h'
              ((eq state 'after-s)
               (if (= ch ?h)
                   (progn (setq output (cons ?Y output))
                          (setq state 'normal))
                 (setq output (cons ?s output))
                 (setq state 'normal)
                 (setq i (1- i))))
              ;; After 't': check for 'h'
              ((eq state 'after-t)
               (if (= ch ?h)
                   (progn (setq output (cons ?Z output))
                          (setq state 'normal))
                 (setq output (cons ?t output))
                 (setq state 'normal)
                 (setq i (1- i))))))
          (setq i (1+ i)))
        ;; Flush pending state
        (cond
          ((eq state 'after-c) (setq output (cons ?c output)))
          ((eq state 'after-s) (setq output (cons ?s output)))
          ((eq state 'after-t) (setq output (cons ?t output))))
        (concat (nreverse output)))))
  (unwind-protect
      (list
        (funcall 'neovm--test-mealy "church")
        (funcall 'neovm--test-mealy "shout")
        (funcall 'neovm--test-mealy "the thing")
        (funcall 'neovm--test-mealy "catch this shell")
        (funcall 'neovm--test-mealy "cats")
        (funcall 'neovm--test-mealy "")
        (funcall 'neovm--test-mealy "chest")
        (funcall 'neovm--test-mealy "sc"))
    (fmakunbound 'neovm--test-mealy)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"XurX\" \"Yout\" \"Ze Zing\" \"catX Zis Yell\" \"cats\" \"\" \"Xest\" \"sc\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// PDA-like recursive descent with explicit stack
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sm_pda_expression_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse and evaluate arithmetic expressions: num, +, *, parens
    // Grammar: E -> T (('+' T)*), T -> F (('*' F)*), F -> num | '(' E ')'
    // Uses explicit position tracking (poor man's recursive descent)
    let form = r#"(progn
  ;; Global position variable for the parser
  (defvar neovm--test-parse-pos 0)
  (defvar neovm--test-parse-input "")

  (fset 'neovm--test-peek
    (lambda ()
      (if (< neovm--test-parse-pos (length neovm--test-parse-input))
          (aref neovm--test-parse-input neovm--test-parse-pos)
        nil)))

  (fset 'neovm--test-advance
    (lambda ()
      (setq neovm--test-parse-pos (1+ neovm--test-parse-pos))))

  (fset 'neovm--test-skip-ws
    (lambda ()
      (while (and (funcall 'neovm--test-peek)
                  (= (funcall 'neovm--test-peek) ?\s))
        (funcall 'neovm--test-advance))))

  (fset 'neovm--test-parse-num
    (lambda ()
      (funcall 'neovm--test-skip-ws)
      (let ((start neovm--test-parse-pos))
        (while (and (funcall 'neovm--test-peek)
                    (let ((c (funcall 'neovm--test-peek)))
                      (and (>= c ?0) (<= c ?9))))
          (funcall 'neovm--test-advance))
        (if (> neovm--test-parse-pos start)
            (string-to-number
             (substring neovm--test-parse-input start neovm--test-parse-pos))
          nil))))

  ;; F -> num | '(' E ')'
  (fset 'neovm--test-parse-factor
    (lambda ()
      (funcall 'neovm--test-skip-ws)
      (let ((ch (funcall 'neovm--test-peek)))
        (if (and ch (= ch ?\())
            (progn
              (funcall 'neovm--test-advance)
              (let ((val (funcall 'neovm--test-parse-expr)))
                (funcall 'neovm--test-skip-ws)
                (funcall 'neovm--test-advance)  ;; skip ')'
                val))
          (funcall 'neovm--test-parse-num)))))

  ;; T -> F (('*' F)*)
  (fset 'neovm--test-parse-term
    (lambda ()
      (let ((val (funcall 'neovm--test-parse-factor)))
        (funcall 'neovm--test-skip-ws)
        (while (and (funcall 'neovm--test-peek)
                    (= (funcall 'neovm--test-peek) ?*))
          (funcall 'neovm--test-advance)
          (setq val (* val (funcall 'neovm--test-parse-factor)))
          (funcall 'neovm--test-skip-ws))
        val)))

  ;; E -> T (('+' T)*)
  (fset 'neovm--test-parse-expr
    (lambda ()
      (let ((val (funcall 'neovm--test-parse-term)))
        (funcall 'neovm--test-skip-ws)
        (while (and (funcall 'neovm--test-peek)
                    (= (funcall 'neovm--test-peek) ?+))
          (funcall 'neovm--test-advance)
          (setq val (+ val (funcall 'neovm--test-parse-term)))
          (funcall 'neovm--test-skip-ws))
        val)))

  (fset 'neovm--test-eval-expr
    (lambda (input)
      (setq neovm--test-parse-pos 0)
      (setq neovm--test-parse-input input)
      (funcall 'neovm--test-parse-expr)))

  (unwind-protect
      (list
        (funcall 'neovm--test-eval-expr "42")
        (funcall 'neovm--test-eval-expr "2+3")
        (funcall 'neovm--test-eval-expr "2*3+4")
        (funcall 'neovm--test-eval-expr "2+3*4")
        (funcall 'neovm--test-eval-expr "(2+3)*4")
        (funcall 'neovm--test-eval-expr "1+2+3+4+5")
        (funcall 'neovm--test-eval-expr "((10))")
        (funcall 'neovm--test-eval-expr "2 * (3 + 4) * 5"))
    (fmakunbound 'neovm--test-peek)
    (fmakunbound 'neovm--test-advance)
    (fmakunbound 'neovm--test-skip-ws)
    (fmakunbound 'neovm--test-parse-num)
    (fmakunbound 'neovm--test-parse-factor)
    (fmakunbound 'neovm--test-parse-term)
    (fmakunbound 'neovm--test-parse-expr)
    (fmakunbound 'neovm--test-eval-expr)
    (makunbound 'neovm--test-parse-pos)
    (makunbound 'neovm--test-parse-input)))"#;
    let expect = expect_test::expect![[r#""OK (42 5 10 14 20 15 10 70)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// NFA-like pattern matching (simplified glob patterns)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sm_glob_pattern_matcher() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Match strings against glob patterns: ? = any single char, * = any sequence
    // Uses backtracking approach (NFA-like)
    let form = r#"(progn
  (fset 'neovm--test-glob-match
    (lambda (pattern text)
      (let ((plen (length pattern))
            (tlen (length text))
            ;; Stack of (pi . ti) backtrack points for '*'
            (star-pi -1)
            (star-ti -1)
            (pi 0)
            (ti 0)
            (matched t))
        (while (and matched (< ti tlen))
          (cond
            ;; Pattern char matches or is '?'
            ((and (< pi plen)
                  (let ((pc (aref pattern pi)))
                    (or (= pc ??)
                        (= pc (aref text ti)))))
             (setq pi (1+ pi) ti (1+ ti)))
            ;; '*' in pattern: record backtrack point
            ((and (< pi plen) (= (aref pattern pi) ?*))
             (setq star-pi pi star-ti ti)
             (setq pi (1+ pi)))
            ;; Mismatch but we have a '*' to backtrack to
            ((>= star-pi 0)
             (setq star-ti (1+ star-ti))
             (setq ti star-ti)
             (setq pi (1+ star-pi)))
            ;; No match
            (t (setq matched nil))))
        ;; Consume trailing '*' in pattern
        (while (and matched (< pi plen) (= (aref pattern pi) ?*))
          (setq pi (1+ pi)))
        (and matched (= pi plen)))))
  (unwind-protect
      (list
        ;; Exact match
        (funcall 'neovm--test-glob-match "hello" "hello")
        (funcall 'neovm--test-glob-match "hello" "world")
        ;; ? matches single char
        (funcall 'neovm--test-glob-match "h?llo" "hello")
        (funcall 'neovm--test-glob-match "h?llo" "hallo")
        (funcall 'neovm--test-glob-match "h?llo" "hllo")
        ;; * matches any sequence
        (funcall 'neovm--test-glob-match "h*o" "hello")
        (funcall 'neovm--test-glob-match "h*o" "ho")
        (funcall 'neovm--test-glob-match "*" "anything")
        (funcall 'neovm--test-glob-match "*" "")
        ;; Complex patterns
        (funcall 'neovm--test-glob-match "*.txt" "file.txt")
        (funcall 'neovm--test-glob-match "*.txt" "file.csv")
        (funcall 'neovm--test-glob-match "src/*/*.rs" "src/foo/bar.rs")
        (funcall 'neovm--test-glob-match "a*b*c" "aXbYc")
        (funcall 'neovm--test-glob-match "a*b*c" "abc")
        (funcall 'neovm--test-glob-match "a*b*c" "ac"))
    (fmakunbound 'neovm--test-glob-match)))"#;
    let expect = expect_test::expect![[r#""OK (t nil t t nil t t t t t nil t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// State machine for CSV line parsing with quoting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sm_csv_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse CSV lines handling quoted fields with escaped quotes
    let form = r#"(progn
  (fset 'neovm--test-parse-csv
    (lambda (line)
      (let ((fields nil)
            (current nil)
            (state 'field-start)
            (i 0)
            (len (length line)))
        (while (< i len)
          (let ((ch (aref line i)))
            (cond
              ;; Start of field
              ((eq state 'field-start)
               (cond
                 ((= ch ?,)
                  (setq fields (cons (concat (nreverse current)) fields))
                  (setq current nil))
                 ((= ch ?\")
                  (setq state 'in-quoted))
                 (t
                  (setq current (cons ch current))
                  (setq state 'in-unquoted))))
              ;; Inside unquoted field
              ((eq state 'in-unquoted)
               (if (= ch ?,)
                   (progn
                     (setq fields (cons (concat (nreverse current)) fields))
                     (setq current nil)
                     (setq state 'field-start))
                 (setq current (cons ch current))))
              ;; Inside quoted field
              ((eq state 'in-quoted)
               (if (= ch ?\")
                   (setq state 'quote-or-end)
                 (setq current (cons ch current))))
              ;; After quote inside quoted field: doubled quote or end
              ((eq state 'quote-or-end)
               (cond
                 ((= ch ?\")
                  ;; Doubled quote -> literal quote
                  (setq current (cons ?\" current))
                  (setq state 'in-quoted))
                 ((= ch ?,)
                  (setq fields (cons (concat (nreverse current)) fields))
                  (setq current nil)
                  (setq state 'field-start))
                 (t
                  ;; After closing quote, non-comma (shouldn't happen in valid CSV)
                  (setq current (cons ch current))
                  (setq state 'in-unquoted))))))
          (setq i (1+ i)))
        ;; Emit last field
        (setq fields (cons (concat (nreverse current)) fields))
        (nreverse fields))))
  (unwind-protect
      (list
        (funcall 'neovm--test-parse-csv "a,b,c")
        (funcall 'neovm--test-parse-csv "hello,world")
        (funcall 'neovm--test-parse-csv "\"quoted\",plain")
        (funcall 'neovm--test-parse-csv "\"has,comma\",ok")
        (funcall 'neovm--test-parse-csv "\"has\"\"quote\",end")
        (funcall 'neovm--test-parse-csv ",,,")
        (funcall 'neovm--test-parse-csv "single")
        (funcall 'neovm--test-parse-csv ""))
    (fmakunbound 'neovm--test-parse-csv)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"a\" \"b\" \"c\") (\"hello\" \"world\") (\"quoted\" \"plain\") (\"has,comma\" \"ok\") (\"has\\\"quote\" \"end\") (\"\" \"\" \"\" \"\") (\"single\") (\"\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-state counter: traffic light controller
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sm_traffic_light_controller() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a traffic light with timed transitions and event logging
    let form = "(let ((state 'red)
                      (timer 0)
                      (log nil)
                      ;; Durations: red=5, green=4, yellow=2
                      (durations (make-hash-table)))
                  (puthash 'red 5 durations)
                  (puthash 'green 4 durations)
                  (puthash 'yellow 2 durations)
                  ;; Transitions
                  (let ((next-state (make-hash-table)))
                    (puthash 'red 'green next-state)
                    (puthash 'green 'yellow next-state)
                    (puthash 'yellow 'red next-state)
                    ;; Simulate 20 ticks
                    (dotimes (tick 20)
                      (setq timer (1+ timer))
                      (when (>= timer (gethash state durations))
                        (let ((old-state state))
                          (setq state (gethash state next-state))
                          (setq timer 0)
                          (setq log (cons (list tick old-state '-> state) log)))))
                    ;; Final state and transition log
                    (list state timer (length (nreverse log))
                          ;; Count how many times each state was entered
                          (let ((counts (make-hash-table)))
                            (dolist (entry (nreverse log))
                              (let ((to (nth 3 entry)))
                                (puthash to (1+ (gethash to counts 0)) counts)))
                            (let ((result nil))
                              (maphash (lambda (k v)
                                         (setq result (cons (cons k v) result)))
                                       counts)
                              (sort result (lambda (a b)
                                             (string-lessp (symbol-name (car a))
                                                           (symbol-name (car b))))))))))";
    let expect = expect_test::expect![[r#""OK (yellow 0 5 ((yellow . 1)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
