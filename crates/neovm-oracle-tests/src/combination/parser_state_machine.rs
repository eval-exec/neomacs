//! Oracle parity tests for parsers using state machines in Elisp.
//!
//! Implements DFA-based tokenizers, state transition tables, a lexer
//! that produces token streams, a CSV parser with quoted fields, and
//! an HTML tag parser supporting opening, closing, self-closing tags,
//! and attributes.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// DFA for tokenizing identifiers, numbers, and operators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_parser_sm_dfa_tokenizer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a DFA with explicit state transitions:
    // States: start, in-ident, in-number, in-operator, done
    let form = r#"(progn
  (fset 'neovm--psm-char-class
    (lambda (ch)
      "Classify a character into a token class."
      (cond
        ((and (>= ch ?a) (<= ch ?z)) 'alpha)
        ((and (>= ch ?A) (<= ch ?Z)) 'alpha)
        ((= ch ?_) 'alpha)
        ((and (>= ch ?0) (<= ch ?9)) 'digit)
        ((memq ch '(?+ ?- ?* ?/ ?= ?< ?> ?!)) 'operator)
        ((or (= ch ?\s) (= ch ?\t) (= ch ?\n)) 'whitespace)
        ((memq ch '(?\( ?\) ?\{ ?\} ?\[ ?\] ?\; ?,)) 'punct)
        (t 'other))))

  (fset 'neovm--psm-dfa-tokenize
    (lambda (input)
      "Tokenize INPUT using a DFA with explicit state transitions."
      (let ((tokens nil)
            (state 'start)
            (current "")
            (token-type nil)
            (i 0)
            (len (length input)))
        (while (<= i len)
          (let* ((ch (if (< i len) (aref input i) ?\s))
                 (class (funcall 'neovm--psm-char-class ch))
                 (at-end (= i len)))
            (cond
              ;; START state
              ((eq state 'start)
               (cond
                 ((eq class 'alpha)
                  (setq state 'in-ident current (char-to-string ch) token-type 'IDENT))
                 ((eq class 'digit)
                  (setq state 'in-number current (char-to-string ch) token-type 'NUMBER))
                 ((eq class 'operator)
                  (setq tokens (cons (list 'OP (char-to-string ch)) tokens)))
                 ((eq class 'punct)
                  (setq tokens (cons (list 'PUNCT (char-to-string ch)) tokens)))
                 ((eq class 'whitespace) nil)  ;; skip
                 (t nil)))

              ;; IN-IDENT state
              ((eq state 'in-ident)
               (if (or (eq class 'alpha) (eq class 'digit))
                   (setq current (concat current (char-to-string ch)))
                 ;; Emit identifier token
                 (setq tokens (cons (list token-type current) tokens))
                 (setq state 'start current "" token-type nil)
                 ;; Reprocess current char
                 (setq i (1- i))))

              ;; IN-NUMBER state
              ((eq state 'in-number)
               (if (eq class 'digit)
                   (setq current (concat current (char-to-string ch)))
                 ;; Check for dot (float)
                 (if (and (< i len) (= ch ?.))
                     (setq current (concat current ".") token-type 'FLOAT)
                   ;; Emit number
                   (setq tokens (cons (list token-type current) tokens))
                   (setq state 'start current "" token-type nil)
                   (setq i (1- i)))))))
          (setq i (1+ i)))
        ;; Emit any remaining token
        (when (> (length current) 0)
          (setq tokens (cons (list token-type current) tokens)))
        (nreverse tokens))))

  (unwind-protect
      (list
        (funcall 'neovm--psm-dfa-tokenize "x = 42 + y")
        (funcall 'neovm--psm-dfa-tokenize "foo123 bar456")
        (funcall 'neovm--psm-dfa-tokenize "a+b*c-d/e")
        (funcall 'neovm--psm-dfa-tokenize "")
        (funcall 'neovm--psm-dfa-tokenize "   ")
        (funcall 'neovm--psm-dfa-tokenize "hello")
        (funcall 'neovm--psm-dfa-tokenize "12345")
        (funcall 'neovm--psm-dfa-tokenize "(x + y) * z"))
    (fmakunbound 'neovm--psm-char-class)
    (fmakunbound 'neovm--psm-dfa-tokenize)))"#;
    let expect = expect_test::expect![[
        r#""OK (((IDENT \"x\") (OP \"=\") (NUMBER \"42\") (OP \"+\") (IDENT \"y\")) ((IDENT \"foo123\") (IDENT \"bar456\")) ((IDENT \"a\") (OP \"+\") (IDENT \"b\") (OP \"*\") (IDENT \"c\") (OP \"-\") (IDENT \"d\") (OP \"/\") (IDENT \"e\")) nil nil ((IDENT \"hello\")) ((NUMBER \"12345\")) ((PUNCT \"(\") (IDENT \"x\") (OP \"+\") (IDENT \"y\") (PUNCT \")\") (OP \"*\") (IDENT \"z\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// State transition table representation and execution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_parser_sm_transition_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Represent a DFA as a transition table (hash of hash) and execute
    // it to match patterns: integer literals, identifiers, and operators.
    let form = r#"(progn
  (fset 'neovm--psm-build-table
    (lambda ()
      "Build a DFA transition table for tokenizing.
States: start, ident, number, accept-ident, accept-number, accept-op.
Inputs: alpha, digit, op, ws, other."
      (let ((table (make-hash-table :test 'eq)))
        ;; From start
        (let ((start-trans (make-hash-table :test 'eq)))
          (puthash 'alpha 'ident start-trans)
          (puthash 'digit 'number start-trans)
          (puthash 'op 'accept-op start-trans)
          (puthash 'ws 'start start-trans)
          (puthash 'other 'error start-trans)
          (puthash 'start start-trans table))
        ;; From ident
        (let ((ident-trans (make-hash-table :test 'eq)))
          (puthash 'alpha 'ident ident-trans)
          (puthash 'digit 'ident ident-trans)
          (puthash 'op 'accept-ident ident-trans)
          (puthash 'ws 'accept-ident ident-trans)
          (puthash 'other 'accept-ident ident-trans)
          (puthash 'ident ident-trans table))
        ;; From number
        (let ((number-trans (make-hash-table :test 'eq)))
          (puthash 'alpha 'accept-number number-trans)
          (puthash 'digit 'number number-trans)
          (puthash 'op 'accept-number number-trans)
          (puthash 'ws 'accept-number number-trans)
          (puthash 'other 'accept-number number-trans)
          (puthash 'number number-trans table))
        table)))

  (fset 'neovm--psm-classify
    (lambda (ch)
      (cond
        ((and (>= ch ?a) (<= ch ?z)) 'alpha)
        ((and (>= ch ?A) (<= ch ?Z)) 'alpha)
        ((= ch ?_) 'alpha)
        ((and (>= ch ?0) (<= ch ?9)) 'digit)
        ((memq ch '(?+ ?- ?* ?/ ?=)) 'op)
        ((or (= ch ?\s) (= ch ?\t)) 'ws)
        (t 'other))))

  (fset 'neovm--psm-run-dfa
    (lambda (input)
      "Run DFA on input, collecting tokens."
      (let ((table (funcall 'neovm--psm-build-table))
            (tokens nil)
            (state 'start)
            (current "")
            (i 0)
            (len (length input)))
        (while (< i len)
          (let* ((ch (aref input i))
                 (class (funcall 'neovm--psm-classify ch))
                 (state-trans (gethash state table))
                 (next-state (when state-trans (gethash class state-trans 'error))))
            (cond
              ;; Accept states: emit token, don't consume char
              ((eq next-state 'accept-ident)
               (setq tokens (cons (list 'IDENT current) tokens))
               (setq state 'start current ""))
              ((eq next-state 'accept-number)
               (setq tokens (cons (list 'NUMBER current) tokens))
               (setq state 'start current ""))
              ((eq next-state 'accept-op)
               (when (> (length current) 0)
                 (setq tokens (cons (list 'IDENT current) tokens))
                 (setq current ""))
               (setq tokens (cons (list 'OP (char-to-string ch)) tokens))
               (setq state 'start)
               (setq i (1+ i)))
              ;; Accumulating states
              ((memq next-state '(ident number))
               (setq current (concat current (char-to-string ch)))
               (setq state next-state)
               (setq i (1+ i)))
              ;; start: skip whitespace
              ((eq next-state 'start)
               (setq i (1+ i)))
              ;; error
              (t (setq i (1+ i))))))
        (when (> (length current) 0)
          (let ((tok-type (if (eq state 'number) 'NUMBER 'IDENT)))
            (setq tokens (cons (list tok-type current) tokens))))
        (nreverse tokens))))

  (unwind-protect
      (list
        (funcall 'neovm--psm-run-dfa "abc + 123")
        (funcall 'neovm--psm-run-dfa "x=y")
        (funcall 'neovm--psm-run-dfa "hello_world 42")
        (funcall 'neovm--psm-run-dfa "")
        (funcall 'neovm--psm-run-dfa "   abc   ")
        (funcall 'neovm--psm-run-dfa "a+b+c")
        (funcall 'neovm--psm-run-dfa "999"))
    (fmakunbound 'neovm--psm-build-table)
    (fmakunbound 'neovm--psm-classify)
    (fmakunbound 'neovm--psm-run-dfa)))"#;
    let expect = expect_test::expect![[
        r#""OK (((IDENT \"abc\") (OP \"+\") (NUMBER \"123\")) ((IDENT \"x\") (OP \"=\") (IDENT \"y\")) ((IDENT \"hello_world\") (NUMBER \"42\")) nil ((IDENT \"abc\")) ((IDENT \"a\") (OP \"+\") (IDENT \"b\") (OP \"+\") (IDENT \"c\")) ((NUMBER \"999\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Lexer producing structured token stream with positions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_parser_sm_structured_lexer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A lexer that tracks position (offset, length) per token and
    // categorizes into keywords, identifiers, numbers, strings, and ops.
    let form = r#"(progn
  (defvar neovm--psm-kw-list '("if" "else" "while" "for" "return" "let" "var"))

  (fset 'neovm--psm-lex
    (lambda (input)
      "Produce tokens with (TYPE VALUE OFFSET LENGTH)."
      (let ((tokens nil)
            (i 0)
            (len (length input)))
        (while (< i len)
          (let ((ch (aref input i)))
            (cond
              ;; Whitespace: skip
              ((or (= ch ?\s) (= ch ?\t) (= ch ?\n))
               (setq i (1+ i)))
              ;; String literal
              ((= ch ?\")
               (let ((start i)
                     (chars nil))
                 (setq i (1+ i))
                 (while (and (< i len) (/= (aref input i) ?\"))
                   (if (and (= (aref input i) ?\\) (< (1+ i) len))
                       (progn
                         (setq i (1+ i))
                         (setq chars (cons (aref input i) chars))
                         (setq i (1+ i)))
                     (setq chars (cons (aref input i) chars))
                     (setq i (1+ i))))
                 (when (< i len) (setq i (1+ i)))  ;; skip closing quote
                 (setq tokens (cons (list 'STRING (concat (nreverse chars))
                                          start (- i start))
                                    tokens))))
              ;; Number
              ((and (>= ch ?0) (<= ch ?9))
               (let ((start i))
                 (while (and (< i len)
                             (let ((c (aref input i)))
                               (or (and (>= c ?0) (<= c ?9))
                                   (= c ?.))))
                   (setq i (1+ i)))
                 (let ((text (substring input start i)))
                   (setq tokens (cons (list (if (string-match-p "\\." text) 'FLOAT 'INT)
                                            text start (- i start))
                                      tokens)))))
              ;; Identifier / keyword
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
                 (let ((text (substring input start i)))
                   (setq tokens (cons (list (if (member text neovm--psm-kw-list) 'KEYWORD 'IDENT)
                                            text start (- i start))
                                      tokens)))))
              ;; Two-char operators
              ((and (< (1+ i) len)
                    (member (substring input i (+ i 2)) '("==" "!=" "<=" ">=")))
               (let ((op (substring input i (+ i 2))))
                 (setq tokens (cons (list 'OP op i 2) tokens))
                 (setq i (+ i 2))))
              ;; Single-char operators and punctuation
              (t
               (setq tokens (cons (list 'PUNCT (char-to-string ch) i 1) tokens))
               (setq i (1+ i))))))
        (nreverse tokens))))

  (unwind-protect
      (list
        (funcall 'neovm--psm-lex "let x = 42;")
        (funcall 'neovm--psm-lex "if x >= 10 { return \"ok\" }")
        (funcall 'neovm--psm-lex "while i != 0 { i = i - 1 }")
        (funcall 'neovm--psm-lex "3.14 + 2.71")
        (funcall 'neovm--psm-lex "")
        ;; Verify offsets reconstruct original
        (let* ((input "let x = 42")
               (toks (funcall 'neovm--psm-lex input))
               (reconstructed
                 (mapcar (lambda (tok)
                           (substring input (nth 2 tok) (+ (nth 2 tok) (nth 3 tok))))
                         toks)))
          (list toks reconstructed)))
    (fmakunbound 'neovm--psm-lex)
    (makunbound 'neovm--psm-kw-list)))"#;
    let expect = expect_test::expect![[
        r#""OK (((KEYWORD \"let\" 0 3) (IDENT \"x\" 4 1) (PUNCT \"=\" 6 1) (INT \"42\" 8 2) (PUNCT \";\" 10 1)) ((KEYWORD \"if\" 0 2) (IDENT \"x\" 3 1) (OP \">=\" 5 2) (INT \"10\" 8 2) (PUNCT \"{\" 11 1) (KEYWORD \"return\" 13 6) (STRING \"ok\" 20 4) (PUNCT \"}\" 25 1)) ((KEYWORD \"while\" 0 5) (IDENT \"i\" 6 1) (OP \"!=\" 8 2) (INT \"0\" 11 1) (PUNCT \"{\" 13 1) (IDENT \"i\" 15 1) (PUNCT \"=\" 17 1) (IDENT \"i\" 19 1) (PUNCT \"-\" 21 1) (INT \"1\" 23 1) (PUNCT \"}\" 25 1)) ((FLOAT \"3.14\" 0 4) (PUNCT \"+\" 5 1) (FLOAT \"2.71\" 7 4)) nil (((KEYWORD \"let\" 0 3) (IDENT \"x\" 4 1) (PUNCT \"=\" 6 1) (INT \"42\" 8 2)) (\"let\" \"x\" \"=\" \"42\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// CSV parser with quoted fields
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_parser_sm_csv_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse CSV with proper handling of:
    // - Quoted fields with embedded commas
    // - Escaped quotes ("" inside quoted fields)
    // - Newlines inside quoted fields (not applicable for single-line)
    // - Empty fields
    // - Trailing commas
    let form = r#"(progn
  (fset 'neovm--psm-csv-parse-row
    (lambda (line)
      "Parse a single CSV row into a list of field strings."
      (let ((fields nil)
            (current "")
            (state 'field-start)  ;; field-start, unquoted, quoted, quote-seen
            (i 0)
            (len (length line)))
        (while (<= i len)
          (let ((ch (if (< i len) (aref line i) nil)))
            (cond
              ;; FIELD-START: beginning of a new field
              ((eq state 'field-start)
               (cond
                 ((null ch)  ;; end of input
                  (setq fields (cons current fields)))
                 ((= ch ?\")
                  (setq state 'quoted current ""))
                 ((= ch ?,)
                  (setq fields (cons current fields) current ""))
                 (t
                  (setq state 'unquoted current (char-to-string ch)))))

              ;; UNQUOTED: inside an unquoted field
              ((eq state 'unquoted)
               (cond
                 ((or (null ch) (= ch ?,))
                  (setq fields (cons current fields) current "")
                  (setq state 'field-start))
                 (t
                  (setq current (concat current (char-to-string ch))))))

              ;; QUOTED: inside a quoted field
              ((eq state 'quoted)
               (cond
                 ((null ch)
                  ;; Unterminated quote: emit what we have
                  (setq fields (cons current fields)))
                 ((= ch ?\")
                  (setq state 'quote-seen))
                 (t
                  (setq current (concat current (char-to-string ch))))))

              ;; QUOTE-SEEN: just saw a quote inside a quoted field
              ((eq state 'quote-seen)
               (cond
                 ((and ch (= ch ?\"))
                  ;; Escaped quote: ""
                  (setq current (concat current "\"") state 'quoted))
                 ((or (null ch) (= ch ?,))
                  (setq fields (cons current fields) current "")
                  (setq state 'field-start))
                 (t
                  ;; Unexpected char after quote
                  (setq current (concat current (char-to-string ch))
                        state 'unquoted))))))
          (setq i (1+ i)))
        (nreverse fields))))

  (fset 'neovm--psm-csv-parse
    (lambda (text)
      "Parse multi-line CSV text into list of rows."
      (let ((lines (split-string text "\n"))
            (rows nil))
        (dolist (line lines)
          (when (> (length line) 0)
            (setq rows (cons (funcall 'neovm--psm-csv-parse-row line) rows))))
        (nreverse rows))))

  (unwind-protect
      (list
        ;; Simple fields
        (funcall 'neovm--psm-csv-parse-row "a,b,c")
        ;; Quoted field with comma
        (funcall 'neovm--psm-csv-parse-row "a,\"b,c\",d")
        ;; Escaped quotes
        (funcall 'neovm--psm-csv-parse-row "a,\"he said \"\"hello\"\"\",b")
        ;; Empty fields
        (funcall 'neovm--psm-csv-parse-row ",,a,,b,,")
        ;; All quoted
        (funcall 'neovm--psm-csv-parse-row "\"x\",\"y\",\"z\"")
        ;; Mixed quoted and unquoted
        (funcall 'neovm--psm-csv-parse-row "hello,\"world, earth\",42")
        ;; Single field
        (funcall 'neovm--psm-csv-parse-row "solo")
        ;; Empty string
        (funcall 'neovm--psm-csv-parse-row "")
        ;; Multi-line CSV
        (funcall 'neovm--psm-csv-parse
                 "name,age,city\nAlice,30,\"New York\"\nBob,25,London\nCharlie,35,\"San Francisco\"")
        ;; Verify field counts are consistent
        (let* ((csv "a,b,c\n1,2,3\nx,y,z")
               (rows (funcall 'neovm--psm-csv-parse csv))
               (counts (mapcar 'length rows)))
          (list rows counts (apply '= counts))))
    (fmakunbound 'neovm--psm-csv-parse-row)
    (fmakunbound 'neovm--psm-csv-parse)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"a\" \"b\" \"c\") (\"a\" \"b,c\" \"d\") (\"a\" \"he said \\\"hello\\\"\" \"b\") (\"\" \"\" \"a\" \"\" \"b\" \"\" \"\") (\"x\" \"y\" \"z\") (\"hello\" \"world, earth\" \"42\") (\"solo\") (\"\") ((\"name\" \"age\" \"city\") (\"Alice\" \"30\" \"New York\") (\"Bob\" \"25\" \"London\") (\"Charlie\" \"35\" \"San Francisco\")) (((\"a\" \"b\" \"c\") (\"1\" \"2\" \"3\") (\"x\" \"y\" \"z\")) (3 3 3) t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// HTML tag parser: opening, closing, self-closing, attributes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_parser_sm_html_tag_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse HTML tags extracting:
    // - Tag type (opening, closing, self-closing)
    // - Tag name
    // - Attributes as alist
    let form = r#"(progn
  (fset 'neovm--psm-parse-html-tag
    (lambda (tag-str)
      "Parse a single HTML tag string like <div class=\"foo\"> into structured form."
      (let ((i 0)
            (len (length tag-str))
            (tag-type nil)
            (tag-name "")
            (attrs nil))
        ;; Skip <
        (when (and (< i len) (= (aref tag-str i) ?<))
          (setq i (1+ i)))
        ;; Check for closing tag
        (when (and (< i len) (= (aref tag-str i) ?/))
          (setq tag-type 'closing i (1+ i)))
        ;; Skip whitespace
        (while (and (< i len) (= (aref tag-str i) ?\s))
          (setq i (1+ i)))
        ;; Read tag name
        (while (and (< i len)
                    (let ((ch (aref tag-str i)))
                      (and (/= ch ?\s) (/= ch ?>) (/= ch ?/))))
          (setq tag-name (concat tag-name (char-to-string (aref tag-str i))))
          (setq i (1+ i)))
        ;; Parse attributes
        (while (< i len)
          (let ((ch (aref tag-str i)))
            (cond
              ;; Self-closing />
              ((and (= ch ?/) (< (1+ i) len) (= (aref tag-str (1+ i)) ?>))
               (setq tag-type 'self-closing i len))
              ;; End of tag
              ((= ch ?>)
               (setq i len))
              ;; Whitespace: skip
              ((= ch ?\s)
               (setq i (1+ i)))
              ;; Attribute name
              (t
               (let ((attr-name "")
                     (attr-val nil))
                 ;; Read attribute name
                 (while (and (< i len)
                             (let ((c (aref tag-str i)))
                               (and (/= c ?=) (/= c ?\s) (/= c ?>) (/= c ?/))))
                   (setq attr-name (concat attr-name (char-to-string (aref tag-str i))))
                   (setq i (1+ i)))
                 ;; Check for = value
                 (when (and (< i len) (= (aref tag-str i) ?=))
                   (setq i (1+ i))
                   (if (and (< i len) (= (aref tag-str i) ?\"))
                       ;; Quoted value
                       (progn
                         (setq i (1+ i) attr-val "")
                         (while (and (< i len) (/= (aref tag-str i) ?\"))
                           (setq attr-val (concat attr-val (char-to-string (aref tag-str i))))
                           (setq i (1+ i)))
                         (when (< i len) (setq i (1+ i))))
                     ;; Unquoted value
                     (setq attr-val "")
                     (while (and (< i len)
                                 (let ((c (aref tag-str i)))
                                   (and (/= c ?\s) (/= c ?>) (/= c ?/))))
                       (setq attr-val (concat attr-val (char-to-string (aref tag-str i))))
                       (setq i (1+ i)))))
                 (when (> (length attr-name) 0)
                   (setq attrs (cons (cons attr-name (or attr-val t)) attrs))))))))
        (unless tag-type
          (setq tag-type 'opening))
        (list tag-type (downcase tag-name) (nreverse attrs)))))

  (unwind-protect
      (list
        ;; Simple opening tag
        (funcall 'neovm--psm-parse-html-tag "<div>")
        ;; Closing tag
        (funcall 'neovm--psm-parse-html-tag "</div>")
        ;; Self-closing tag
        (funcall 'neovm--psm-parse-html-tag "<br/>")
        (funcall 'neovm--psm-parse-html-tag "<img src=\"pic.jpg\"/>")
        ;; Tag with attributes
        (funcall 'neovm--psm-parse-html-tag "<a href=\"http://example.com\" target=\"_blank\">")
        ;; Tag with class and id
        (funcall 'neovm--psm-parse-html-tag "<div id=\"main\" class=\"container\">")
        ;; Boolean attribute
        (funcall 'neovm--psm-parse-html-tag "<input disabled>")
        ;; Mixed attributes
        (funcall 'neovm--psm-parse-html-tag "<input type=\"text\" required>")
        ;; Uppercase tag name (should normalize to lowercase)
        (funcall 'neovm--psm-parse-html-tag "<DIV CLASS=\"foo\">")
        ;; Self-closing with space
        (funcall 'neovm--psm-parse-html-tag "<hr />")
        ;; Multiple attributes
        (funcall 'neovm--psm-parse-html-tag "<meta name=\"viewport\" content=\"width=device-width\">"))
    (fmakunbound 'neovm--psm-parse-html-tag)))"#;
    let expect = expect_test::expect![[
        r#""OK ((opening \"div\" nil) (closing \"div\" nil) (self-closing \"br\" nil) (self-closing \"img\" ((\"src\" . \"pic.jpg\"))) (opening \"a\" ((\"href\" . \"http://example.com\") (\"target\" . \"_blank\"))) (opening \"div\" ((\"id\" . \"main\") (\"class\" . \"container\"))) (opening \"input\" ((\"disabled\" . t))) (opening \"input\" ((\"type\" . \"text\") (\"required\" . t))) (opening \"div\" ((\"CLASS\" . \"foo\"))) (self-closing \"hr\" nil) (opening \"meta\" ((\"name\" . \"viewport\") (\"content\" . \"width=device-width\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// HTML tag stream parser: extract all tags from HTML text
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_parser_sm_html_stream() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extract all HTML tags from a document, producing a structured
    // list of tags with their types, names, and attributes.
    let form = r#"(progn
  (fset 'neovm--psm2-parse-tag
    (lambda (tag-str)
      "Simplified HTML tag parser."
      (let ((i 0) (len (length tag-str))
            (tag-type 'opening) (tag-name "") (attrs nil))
        (when (and (< i len) (= (aref tag-str i) ?<)) (setq i (1+ i)))
        (when (and (< i len) (= (aref tag-str i) ?/))
          (setq tag-type 'closing i (1+ i)))
        (while (and (< i len) (= (aref tag-str i) ?\s)) (setq i (1+ i)))
        (while (and (< i len)
                    (let ((ch (aref tag-str i)))
                      (and (/= ch ?\s) (/= ch ?>) (/= ch ?/))))
          (setq tag-name (concat tag-name (downcase (char-to-string (aref tag-str i)))))
          (setq i (1+ i)))
        ;; Simplified attribute parsing
        (while (< i len)
          (let ((ch (aref tag-str i)))
            (cond
              ((and (= ch ?/) (< (1+ i) len) (= (aref tag-str (1+ i)) ?>))
               (setq tag-type 'self-closing i len))
              ((= ch ?>) (setq i len))
              ((= ch ?\s) (setq i (1+ i)))
              (t
               (let ((attr-name ""))
                 (while (and (< i len) (let ((c (aref tag-str i)))
                                         (and (/= c ?=) (/= c ?\s) (/= c ?>) (/= c ?/))))
                   (setq attr-name (concat attr-name (char-to-string (aref tag-str i))))
                   (setq i (1+ i)))
                 (if (and (< i len) (= (aref tag-str i) ?=))
                     (progn
                       (setq i (1+ i))
                       (if (and (< i len) (= (aref tag-str i) ?\"))
                           (let ((val ""))
                             (setq i (1+ i))
                             (while (and (< i len) (/= (aref tag-str i) ?\"))
                               (setq val (concat val (char-to-string (aref tag-str i))))
                               (setq i (1+ i)))
                             (when (< i len) (setq i (1+ i)))
                             (setq attrs (cons (cons attr-name val) attrs)))
                         (let ((val ""))
                           (while (and (< i len)
                                       (let ((c (aref tag-str i)))
                                         (and (/= c ?\s) (/= c ?>) (/= c ?/))))
                             (setq val (concat val (char-to-string (aref tag-str i))))
                             (setq i (1+ i)))
                           (setq attrs (cons (cons attr-name val) attrs)))))
                   (when (> (length attr-name) 0)
                     (setq attrs (cons (cons attr-name t) attrs)))))))))
        (list tag-type tag-name (nreverse attrs)))))

  (fset 'neovm--psm2-extract-tags
    (lambda (html)
      "Extract all HTML tags from an HTML string."
      (let ((tags nil)
            (i 0)
            (len (length html)))
        (while (< i len)
          (if (= (aref html i) ?<)
              (let ((start i))
                (while (and (< i len) (/= (aref html i) ?>))
                  (setq i (1+ i)))
                (when (< i len) (setq i (1+ i)))
                (let ((tag-text (substring html start i)))
                  (setq tags (cons (funcall 'neovm--psm2-parse-tag tag-text) tags))))
            (setq i (1+ i))))
        (nreverse tags))))

  (fset 'neovm--psm2-validate-nesting
    (lambda (tags)
      "Check if tags are properly nested. Returns t or the first mismatch."
      (let ((stack nil)
            (error nil))
        (dolist (tag tags)
          (unless error
            (let ((type (car tag))
                  (name (cadr tag)))
              (cond
                ((eq type 'opening)
                 (setq stack (cons name stack)))
                ((eq type 'closing)
                 (if (and stack (string= (car stack) name))
                     (setq stack (cdr stack))
                   (setq error (list 'mismatch name (car stack)))))
                ;; self-closing: no stack change
                ))))
        (cond
          (error error)
          (stack (list 'unclosed stack))
          (t t)))))

  (unwind-protect
      (let ((html1 "<div><p>Hello</p></div>")
            (html2 "<div class=\"main\"><h1>Title</h1><p>Text</p><br/></div>")
            (html3 "<div><p>Unclosed"))
        (list
          ;; Extract tags from simple HTML
          (funcall 'neovm--psm2-extract-tags html1)
          ;; Extract tags with attributes
          (funcall 'neovm--psm2-extract-tags html2)
          ;; Count of each tag type
          (let ((tags (funcall 'neovm--psm2-extract-tags html2))
                (open 0) (close 0) (self 0))
            (dolist (tag tags)
              (cond ((eq (car tag) 'opening) (setq open (1+ open)))
                    ((eq (car tag) 'closing) (setq close (1+ close)))
                    ((eq (car tag) 'self-closing) (setq self (1+ self)))))
            (list open close self))
          ;; Validate proper nesting
          (funcall 'neovm--psm2-validate-nesting
                   (funcall 'neovm--psm2-extract-tags html1))
          (funcall 'neovm--psm2-validate-nesting
                   (funcall 'neovm--psm2-extract-tags html2))
          ;; Unclosed tags
          (funcall 'neovm--psm2-validate-nesting
                   (funcall 'neovm--psm2-extract-tags html3))
          ;; Extract just tag names
          (mapcar 'cadr (funcall 'neovm--psm2-extract-tags html2))))
    (fmakunbound 'neovm--psm2-parse-tag)
    (fmakunbound 'neovm--psm2-extract-tags)
    (fmakunbound 'neovm--psm2-validate-nesting)))"#;
    let expect = expect_test::expect![[
        r#""OK (((opening \"div\" nil) (opening \"p\" nil) (closing \"p\" nil) (closing \"div\" nil)) ((opening \"div\" ((\"class\" . \"main\"))) (opening \"h1\" nil) (closing \"h1\" nil) (opening \"p\" nil) (closing \"p\" nil) (self-closing \"br\" nil) (closing \"div\" nil)) (3 3 1) t t (unclosed (\"p\" \"div\")) (\"div\" \"h1\" \"h1\" \"p\" \"p\" \"br\" \"div\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
