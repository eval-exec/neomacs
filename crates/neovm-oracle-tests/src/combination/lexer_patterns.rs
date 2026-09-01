//! Oracle parity tests for lexer/tokenizer patterns in Elisp.
//!
//! Builds a lexer using buffer operations and regexp to tokenize a
//! mini-language with: identifiers, numbers (int and float), strings,
//! operators (+, -, *, /, =, ==, !=), parentheses, keywords
//! (if, else, while, return). Tracks line/column. Tests with
//! multi-line input.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Core lexer: tokenize identifiers, numbers, operators, parens
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexer_basic_tokens() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Lexer produces list of (TYPE VALUE LINE COL) tokens
  (fset 'neovm--lex-keywords '("if" "else" "while" "return" "fn" "let" "true" "false"))

  (fset 'neovm--lex-tokenize
    (lambda (input)
      (with-temp-buffer
        (insert input)
        (goto-char (point-min))
        (let ((tokens nil)
              (line 1)
              (col 1)
              (line-start 1))
          (while (< (point) (point-max))
            (let ((ch (char-after)))
              (cond
               ;; Whitespace: track line/col
               ((= ch ?\n)
                (forward-char 1)
                (setq line (1+ line)
                      line-start (point)
                      col 1))
               ((or (= ch ?\s) (= ch ?\t))
                (forward-char 1)
                (setq col (- (point) line-start -1)))
               ;; Numbers: integer or float
               ((or (and (>= ch ?0) (<= ch ?9))
                    (and (= ch ?.) (< (1+ (point)) (point-max))
                         (let ((next (char-after (1+ (point)))))
                           (and (>= next ?0) (<= next ?9)))))
                (let ((start (point))
                      (start-col col))
                  (while (and (< (point) (point-max))
                              (let ((c (char-after)))
                                (and (>= c ?0) (<= c ?9))))
                    (forward-char 1))
                  ;; Check for float
                  (when (and (< (point) (point-max))
                             (= (char-after) ?.)
                             (< (1+ (point)) (point-max))
                             (let ((c (char-after (1+ (point)))))
                               (and (>= c ?0) (<= c ?9))))
                    (forward-char 1)
                    (while (and (< (point) (point-max))
                                (let ((c (char-after)))
                                  (and (>= c ?0) (<= c ?9))))
                      (forward-char 1)))
                  (let ((text (buffer-substring-no-properties start (point))))
                    (if (string-match-p "\\." text)
                        (setq tokens (cons (list 'FLOAT (string-to-number text) line start-col) tokens))
                      (setq tokens (cons (list 'INT (string-to-number text) line start-col) tokens))))
                  (setq col (- (point) line-start -1))))
               ;; Identifiers and keywords
               ((or (and (>= ch ?a) (<= ch ?z))
                    (and (>= ch ?A) (<= ch ?Z))
                    (= ch ?_))
                (let ((start (point))
                      (start-col col))
                  (while (and (< (point) (point-max))
                              (let ((c (char-after)))
                                (or (and (>= c ?a) (<= c ?z))
                                    (and (>= c ?A) (<= c ?Z))
                                    (and (>= c ?0) (<= c ?9))
                                    (= c ?_))))
                    (forward-char 1))
                  (let ((text (buffer-substring-no-properties start (point))))
                    (if (member text neovm--lex-keywords)
                        (setq tokens (cons (list 'KEYWORD text line start-col) tokens))
                      (setq tokens (cons (list 'IDENT text line start-col) tokens))))
                  (setq col (- (point) line-start -1))))
               ;; Two-char operators: ==, !=
               ((and (or (= ch ?=) (= ch ?!))
                     (< (1+ (point)) (point-max))
                     (= (char-after (1+ (point))) ?=))
                (let ((start-col col)
                      (op (buffer-substring-no-properties (point) (+ (point) 2))))
                  (forward-char 2)
                  (setq tokens (cons (list 'OP op line start-col) tokens))
                  (setq col (- (point) line-start -1))))
               ;; Single-char operators: + - * / =
               ((memq ch '(?+ ?- ?* ?/ ?=))
                (let ((start-col col))
                  (setq tokens (cons (list 'OP (char-to-string ch) line start-col) tokens))
                  (forward-char 1)
                  (setq col (- (point) line-start -1))))
               ;; Parentheses and braces
               ((memq ch '(?\( ?\) ?\{ ?\} ?\[ ?\]))
                (let ((start-col col)
                      (typ (cond ((= ch ?\() 'LPAREN)
                                 ((= ch ?\)) 'RPAREN)
                                 ((= ch ?\{) 'LBRACE)
                                 ((= ch ?\}) 'RBRACE)
                                 ((= ch ?\[) 'LBRACKET)
                                 ((= ch ?\]) 'RBRACKET))))
                  (setq tokens (cons (list typ (char-to-string ch) line start-col) tokens))
                  (forward-char 1)
                  (setq col (- (point) line-start -1))))
               ;; Semicolons (statement separator)
               ((= ch ?\;)
                (let ((start-col col))
                  (setq tokens (cons (list 'SEMI ";" line start-col) tokens))
                  (forward-char 1)
                  (setq col (- (point) line-start -1))))
               ;; Commas
               ((= ch ?,)
                (let ((start-col col))
                  (setq tokens (cons (list 'COMMA "," line start-col) tokens))
                  (forward-char 1)
                  (setq col (- (point) line-start -1))))
               ;; Unknown character: skip
               (t
                (forward-char 1)
                (setq col (- (point) line-start -1))))))
          (nreverse tokens)))))

  (unwind-protect
      (funcall 'neovm--lex-tokenize "x = 42 + y")
    (fmakunbound 'neovm--lex-tokenize)
    (makunbound 'neovm--lex-keywords)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable neovm--lex-keywords)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Lexer with string literals (double-quoted, escape sequences)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexer_string_literals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--lex2-keywords '("if" "else" "while" "return"))

  (fset 'neovm--lex2-tokenize
    (lambda (input)
      (with-temp-buffer
        (insert input)
        (goto-char (point-min))
        (let ((tokens nil)
              (line 1)
              (col 1)
              (line-start 1))
          (while (< (point) (point-max))
            (let ((ch (char-after)))
              (cond
               ;; Whitespace
               ((= ch ?\n)
                (forward-char 1)
                (setq line (1+ line) line-start (point) col 1))
               ((or (= ch ?\s) (= ch ?\t))
                (forward-char 1)
                (setq col (- (point) line-start -1)))
               ;; String literal
               ((= ch ?\")
                (let ((start-col col)
                      (chars nil)
                      (done nil))
                  (forward-char 1)
                  (while (and (< (point) (point-max)) (not done))
                    (let ((c (char-after)))
                      (cond
                       ((= c ?\\)
                        (forward-char 1)
                        (when (< (point) (point-max))
                          (let ((esc (char-after)))
                            (cond
                             ((= esc ?n) (setq chars (cons ?\n chars)))
                             ((= esc ?t) (setq chars (cons ?\t chars)))
                             ((= esc ?\\) (setq chars (cons ?\\ chars)))
                             ((= esc ?\") (setq chars (cons ?\" chars)))
                             (t (setq chars (cons esc chars)))))
                          (forward-char 1)))
                       ((= c ?\")
                        (forward-char 1)
                        (setq done t))
                       (t
                        (setq chars (cons c chars))
                        (forward-char 1)))))
                  (setq tokens (cons (list 'STRING (concat (nreverse chars)) line start-col) tokens))
                  (setq col (- (point) line-start -1))))
               ;; Numbers
               ((and (>= ch ?0) (<= ch ?9))
                (let ((start (point)) (start-col col))
                  (while (and (< (point) (point-max))
                              (let ((c (char-after)))
                                (and (>= c ?0) (<= c ?9))))
                    (forward-char 1))
                  (when (and (< (point) (point-max))
                             (= (char-after) ?.)
                             (< (1+ (point)) (point-max))
                             (let ((c (char-after (1+ (point)))))
                               (and (>= c ?0) (<= c ?9))))
                    (forward-char 1)
                    (while (and (< (point) (point-max))
                                (let ((c (char-after)))
                                  (and (>= c ?0) (<= c ?9))))
                      (forward-char 1)))
                  (let ((text (buffer-substring-no-properties start (point))))
                    (if (string-match-p "\\." text)
                        (setq tokens (cons (list 'FLOAT (string-to-number text) line start-col) tokens))
                      (setq tokens (cons (list 'INT (string-to-number text) line start-col) tokens))))
                  (setq col (- (point) line-start -1))))
               ;; Identifiers/keywords
               ((or (and (>= ch ?a) (<= ch ?z))
                    (and (>= ch ?A) (<= ch ?Z))
                    (= ch ?_))
                (let ((start (point)) (start-col col))
                  (while (and (< (point) (point-max))
                              (let ((c (char-after)))
                                (or (and (>= c ?a) (<= c ?z))
                                    (and (>= c ?A) (<= c ?Z))
                                    (and (>= c ?0) (<= c ?9))
                                    (= c ?_))))
                    (forward-char 1))
                  (let ((text (buffer-substring-no-properties start (point))))
                    (if (member text neovm--lex2-keywords)
                        (setq tokens (cons (list 'KEYWORD text line start-col) tokens))
                      (setq tokens (cons (list 'IDENT text line start-col) tokens))))
                  (setq col (- (point) line-start -1))))
               ;; Operators
               ((and (or (= ch ?=) (= ch ?!))
                     (< (1+ (point)) (point-max))
                     (= (char-after (1+ (point))) ?=))
                (let ((start-col col)
                      (op (buffer-substring-no-properties (point) (+ (point) 2))))
                  (forward-char 2)
                  (setq tokens (cons (list 'OP op line start-col) tokens))
                  (setq col (- (point) line-start -1))))
               ((memq ch '(?+ ?- ?* ?/ ?=))
                (let ((start-col col))
                  (setq tokens (cons (list 'OP (char-to-string ch) line start-col) tokens))
                  (forward-char 1)
                  (setq col (- (point) line-start -1))))
               ;; Parens
               ((memq ch '(?\( ?\)))
                (let ((start-col col)
                      (typ (if (= ch ?\() 'LPAREN 'RPAREN)))
                  (setq tokens (cons (list typ (char-to-string ch) line start-col) tokens))
                  (forward-char 1)
                  (setq col (- (point) line-start -1))))
               ;; Semicolons
               ((= ch ?\;)
                (let ((start-col col))
                  (setq tokens (cons (list 'SEMI ";" line start-col) tokens))
                  (forward-char 1)
                  (setq col (- (point) line-start -1))))
               ;; Skip unknown
               (t (forward-char 1)
                  (setq col (- (point) line-start -1))))))
          (nreverse tokens)))))

  (unwind-protect
      (list
       ;; String with escapes
       (funcall 'neovm--lex2-tokenize "x = \"hello\\nworld\"")
       ;; String with embedded quotes
       (funcall 'neovm--lex2-tokenize "msg = \"say \\\"hi\\\"\"")
       ;; Empty string
       (funcall 'neovm--lex2-tokenize "s = \"\"")
       ;; String next to other tokens
       (funcall 'neovm--lex2-tokenize "print(\"result\", 42)"))
    (fmakunbound 'neovm--lex2-tokenize)
    (makunbound 'neovm--lex2-keywords)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable neovm--lex2-keywords)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-line input with line/column tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexer_multiline_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--lex3-keywords '("if" "else" "while" "return" "fn" "let"))

  (fset 'neovm--lex3-tokenize
    (lambda (input)
      (with-temp-buffer
        (insert input)
        (goto-char (point-min))
        (let ((tokens nil)
              (line 1)
              (col 1)
              (line-start 1))
          (while (< (point) (point-max))
            (let ((ch (char-after)))
              (cond
               ((= ch ?\n)
                (forward-char 1)
                (setq line (1+ line) line-start (point) col 1))
               ((or (= ch ?\s) (= ch ?\t))
                (forward-char 1)
                (setq col (- (point) line-start -1)))
               ((and (>= ch ?0) (<= ch ?9))
                (let ((start (point)) (start-col col))
                  (while (and (< (point) (point-max))
                              (let ((c (char-after)))
                                (and (>= c ?0) (<= c ?9))))
                    (forward-char 1))
                  (let ((text (buffer-substring-no-properties start (point))))
                    (setq tokens (cons (list 'INT (string-to-number text) line start-col) tokens)))
                  (setq col (- (point) line-start -1))))
               ((or (and (>= ch ?a) (<= ch ?z))
                    (and (>= ch ?A) (<= ch ?Z))
                    (= ch ?_))
                (let ((start (point)) (start-col col))
                  (while (and (< (point) (point-max))
                              (let ((c (char-after)))
                                (or (and (>= c ?a) (<= c ?z))
                                    (and (>= c ?A) (<= c ?Z))
                                    (and (>= c ?0) (<= c ?9))
                                    (= c ?_))))
                    (forward-char 1))
                  (let ((text (buffer-substring-no-properties start (point))))
                    (if (member text neovm--lex3-keywords)
                        (setq tokens (cons (list 'KEYWORD text line start-col) tokens))
                      (setq tokens (cons (list 'IDENT text line start-col) tokens))))
                  (setq col (- (point) line-start -1))))
               ((and (or (= ch ?=) (= ch ?!))
                     (< (1+ (point)) (point-max))
                     (= (char-after (1+ (point))) ?=))
                (let ((start-col col)
                      (op (buffer-substring-no-properties (point) (+ (point) 2))))
                  (forward-char 2)
                  (setq tokens (cons (list 'OP op line start-col) tokens))
                  (setq col (- (point) line-start -1))))
               ((memq ch '(?+ ?- ?* ?/ ?= ?< ?>))
                (let ((start-col col))
                  (setq tokens (cons (list 'OP (char-to-string ch) line start-col) tokens))
                  (forward-char 1)
                  (setq col (- (point) line-start -1))))
               ((memq ch '(?\( ?\) ?\{ ?\}))
                (let ((start-col col)
                      (typ (cond ((= ch ?\() 'LPAREN) ((= ch ?\)) 'RPAREN)
                                 ((= ch ?\{) 'LBRACE) ((= ch ?\}) 'RBRACE))))
                  (setq tokens (cons (list typ (char-to-string ch) line start-col) tokens))
                  (forward-char 1)
                  (setq col (- (point) line-start -1))))
               ((= ch ?\;)
                (let ((start-col col))
                  (setq tokens (cons (list 'SEMI ";" line start-col) tokens))
                  (forward-char 1)
                  (setq col (- (point) line-start -1))))
               (t (forward-char 1)
                  (setq col (- (point) line-start -1))))))
          (nreverse tokens)))))

  (unwind-protect
      (let ((code "fn factorial(n) {\n  if n == 0 {\n    return 1\n  } else {\n    return n * factorial(n - 1)\n  }\n}"))
        (let ((tokens (funcall 'neovm--lex3-tokenize code)))
          (list
           ;; Total token count
           (length tokens)
           ;; First token: fn keyword at line 1, col 1
           (car tokens)
           ;; Check that line numbers advance
           (mapcar (lambda (tok) (nth 2 tok)) tokens)
           ;; Extract just token types
           (mapcar #'car tokens)
           ;; Last token
           (car (last tokens)))))
    (fmakunbound 'neovm--lex3-tokenize)
    (makunbound 'neovm--lex3-keywords)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable neovm--lex3-keywords)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Lexer with comments (// single-line and /* multi-line */)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexer_with_comments() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--lex4-keywords '("if" "else" "while" "return" "let"))

  (fset 'neovm--lex4-tokenize
    (lambda (input)
      (with-temp-buffer
        (insert input)
        (goto-char (point-min))
        (let ((tokens nil)
              (line 1)
              (col 1)
              (line-start 1))
          (while (< (point) (point-max))
            (let ((ch (char-after)))
              (cond
               ((= ch ?\n)
                (forward-char 1)
                (setq line (1+ line) line-start (point) col 1))
               ((or (= ch ?\s) (= ch ?\t))
                (forward-char 1)
                (setq col (- (point) line-start -1)))
               ;; Single-line comment: // to end of line
               ((and (= ch ?/)
                     (< (1+ (point)) (point-max))
                     (= (char-after (1+ (point))) ?/))
                (forward-char 2)
                (while (and (< (point) (point-max))
                            (/= (char-after) ?\n))
                  (forward-char 1))
                (setq col (- (point) line-start -1)))
               ;; Multi-line comment: /* ... */
               ((and (= ch ?/)
                     (< (1+ (point)) (point-max))
                     (= (char-after (1+ (point))) ?*))
                (forward-char 2)
                (let ((done nil))
                  (while (and (< (point) (point-max)) (not done))
                    (let ((c (char-after)))
                      (cond
                       ((= c ?\n)
                        (setq line (1+ line))
                        (forward-char 1)
                        (setq line-start (point)))
                       ((and (= c ?*)
                             (< (1+ (point)) (point-max))
                             (= (char-after (1+ (point))) ?/))
                        (forward-char 2)
                        (setq done t))
                       (t (forward-char 1))))))
                (setq col (- (point) line-start -1)))
               ;; Numbers
               ((and (>= ch ?0) (<= ch ?9))
                (let ((start (point)) (start-col col))
                  (while (and (< (point) (point-max))
                              (let ((c (char-after)))
                                (and (>= c ?0) (<= c ?9))))
                    (forward-char 1))
                  (let ((text (buffer-substring-no-properties start (point))))
                    (setq tokens (cons (list 'INT (string-to-number text) line start-col) tokens)))
                  (setq col (- (point) line-start -1))))
               ;; Identifiers/keywords
               ((or (and (>= ch ?a) (<= ch ?z))
                    (and (>= ch ?A) (<= ch ?Z))
                    (= ch ?_))
                (let ((start (point)) (start-col col))
                  (while (and (< (point) (point-max))
                              (let ((c (char-after)))
                                (or (and (>= c ?a) (<= c ?z))
                                    (and (>= c ?A) (<= c ?Z))
                                    (and (>= c ?0) (<= c ?9))
                                    (= c ?_))))
                    (forward-char 1))
                  (let ((text (buffer-substring-no-properties start (point))))
                    (if (member text neovm--lex4-keywords)
                        (setq tokens (cons (list 'KEYWORD text line start-col) tokens))
                      (setq tokens (cons (list 'IDENT text line start-col) tokens))))
                  (setq col (- (point) line-start -1))))
               ;; Operators
               ((and (or (= ch ?=) (= ch ?!))
                     (< (1+ (point)) (point-max))
                     (= (char-after (1+ (point))) ?=))
                (let ((start-col col)
                      (op (buffer-substring-no-properties (point) (+ (point) 2))))
                  (forward-char 2)
                  (setq tokens (cons (list 'OP op line start-col) tokens))
                  (setq col (- (point) line-start -1))))
               ((memq ch '(?+ ?- ?* ?/))
                (let ((start-col col))
                  (setq tokens (cons (list 'OP (char-to-string ch) line start-col) tokens))
                  (forward-char 1)
                  (setq col (- (point) line-start -1))))
               ((= ch ?=)
                (let ((start-col col))
                  (setq tokens (cons (list 'OP "=" line start-col) tokens))
                  (forward-char 1)
                  (setq col (- (point) line-start -1))))
               ;; Parens/braces
               ((memq ch '(?\( ?\) ?\{ ?\}))
                (let ((start-col col)
                      (typ (cond ((= ch ?\() 'LPAREN) ((= ch ?\)) 'RPAREN)
                                 ((= ch ?\{) 'LBRACE) ((= ch ?\}) 'RBRACE))))
                  (setq tokens (cons (list typ (char-to-string ch) line start-col) tokens))
                  (forward-char 1)
                  (setq col (- (point) line-start -1))))
               ((= ch ?\;)
                (let ((start-col col))
                  (setq tokens (cons (list 'SEMI ";" line start-col) tokens))
                  (forward-char 1)
                  (setq col (- (point) line-start -1))))
               (t (forward-char 1)
                  (setq col (- (point) line-start -1))))))
          (nreverse tokens)))))

  (unwind-protect
      (let ((code "// this is a comment\nlet x = 42; // inline comment\n/* multi\n   line\n   comment */\nlet y = x + 1;"))
        (let ((tokens (funcall 'neovm--lex4-tokenize code)))
          (list
           ;; Comments should be stripped
           (length tokens)
           ;; All tokens
           tokens
           ;; Verify no comment content in tokens
           (not (seq-find (lambda (tok) (string-match-p "comment" (format "%s" (nth 1 tok)))) tokens))
           ;; Line numbers: first token on line 2 (after comment)
           (nth 2 (car tokens))
           ;; Last token line
           (nth 2 (car (last tokens))))))
    (fmakunbound 'neovm--lex4-tokenize)
    (makunbound 'neovm--lex4-keywords)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable neovm--lex4-keywords)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Token stream analysis: count types, find patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexer_token_analysis() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--lex5-keywords '("if" "else" "while" "return" "let" "fn"))

  (fset 'neovm--lex5-tokenize
    (lambda (input)
      (with-temp-buffer
        (insert input)
        (goto-char (point-min))
        (let ((tokens nil)
              (line 1)
              (col 1)
              (line-start 1))
          (while (< (point) (point-max))
            (let ((ch (char-after)))
              (cond
               ((= ch ?\n)
                (forward-char 1)
                (setq line (1+ line) line-start (point) col 1))
               ((or (= ch ?\s) (= ch ?\t))
                (forward-char 1)
                (setq col (- (point) line-start -1)))
               ((and (>= ch ?0) (<= ch ?9))
                (let ((start (point)) (start-col col))
                  (while (and (< (point) (point-max))
                              (let ((c (char-after)))
                                (or (and (>= c ?0) (<= c ?9))
                                    (= c ?.))))
                    (forward-char 1))
                  (let ((text (buffer-substring-no-properties start (point))))
                    (if (string-match-p "\\." text)
                        (setq tokens (cons (list 'FLOAT text line start-col) tokens))
                      (setq tokens (cons (list 'INT text line start-col) tokens))))
                  (setq col (- (point) line-start -1))))
               ((or (and (>= ch ?a) (<= ch ?z))
                    (and (>= ch ?A) (<= ch ?Z))
                    (= ch ?_))
                (let ((start (point)) (start-col col))
                  (while (and (< (point) (point-max))
                              (let ((c (char-after)))
                                (or (and (>= c ?a) (<= c ?z))
                                    (and (>= c ?A) (<= c ?Z))
                                    (and (>= c ?0) (<= c ?9))
                                    (= c ?_))))
                    (forward-char 1))
                  (let ((text (buffer-substring-no-properties start (point))))
                    (if (member text neovm--lex5-keywords)
                        (setq tokens (cons (list 'KEYWORD text line start-col) tokens))
                      (setq tokens (cons (list 'IDENT text line start-col) tokens))))
                  (setq col (- (point) line-start -1))))
               ((memq ch '(?+ ?- ?* ?/ ?= ?< ?>))
                (let ((start-col col))
                  (setq tokens (cons (list 'OP (char-to-string ch) line start-col) tokens))
                  (forward-char 1)
                  (setq col (- (point) line-start -1))))
               ((memq ch '(?\( ?\) ?\{ ?\} ?\; ?,))
                (let ((start-col col)
                      (typ (cond ((= ch ?\() 'LPAREN) ((= ch ?\)) 'RPAREN)
                                 ((= ch ?\{) 'LBRACE) ((= ch ?\}) 'RBRACE)
                                 ((= ch ?\;) 'SEMI) ((= ch ?,) 'COMMA))))
                  (setq tokens (cons (list typ (char-to-string ch) line start-col) tokens))
                  (forward-char 1)
                  (setq col (- (point) line-start -1))))
               (t (forward-char 1)
                  (setq col (- (point) line-start -1))))))
          (nreverse tokens)))))

  ;; Analysis functions
  (fset 'neovm--lex5-count-types
    (lambda (tokens)
      (let ((counts nil))
        (dolist (tok tokens)
          (let ((typ (car tok))
                (existing (assq (car tok) counts)))
            (if existing
                (setcdr existing (1+ (cdr existing)))
              (setq counts (cons (cons typ 1) counts)))))
        (sort counts (lambda (a b) (string< (symbol-name (car a))
                                            (symbol-name (car b))))))))

  (fset 'neovm--lex5-find-identifiers
    (lambda (tokens)
      (let ((ids nil))
        (dolist (tok tokens)
          (when (eq (car tok) 'IDENT)
            (unless (member (nth 1 tok) ids)
              (setq ids (cons (nth 1 tok) ids)))))
        (sort ids 'string<))))

  (unwind-protect
      (let* ((code "fn max(a, b) {\n  if a > b {\n    return a\n  } else {\n    return b\n  }\n}\nlet result = max(10, 20);")
             (tokens (funcall 'neovm--lex5-tokenize code)))
        (list
         ;; Total tokens
         (length tokens)
         ;; Type counts
         (funcall 'neovm--lex5-count-types tokens)
         ;; Unique identifiers
         (funcall 'neovm--lex5-find-identifiers tokens)
         ;; All keyword values
         (mapcar (lambda (tok) (nth 1 tok))
                 (seq-filter (lambda (tok) (eq (car tok) 'KEYWORD)) tokens))
         ;; Lines used
         (let ((max-line 0))
           (dolist (tok tokens)
             (when (> (nth 2 tok) max-line)
               (setq max-line (nth 2 tok))))
           max-line)))
    (fmakunbound 'neovm--lex5-tokenize)
    (fmakunbound 'neovm--lex5-count-types)
    (fmakunbound 'neovm--lex5-find-identifiers)
    (makunbound 'neovm--lex5-keywords)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable neovm--lex5-keywords)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Lexer with regexp-based token matching
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexer_regexp_based() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Regexp-based lexer: try patterns in order at current point
  (fset 'neovm--lex6-token-specs
    (lambda ()
      ;; Each spec is (TYPE REGEXP), tried in order
      (list
       '(WHITESPACE "[ \t]+")
       '(NEWLINE "\n")
       '(FLOAT "[0-9]+\\.[0-9]+")
       '(INT "[0-9]+")
       '(KEYWORD "\\<\\(if\\|else\\|while\\|return\\|let\\|fn\\)\\>")
       '(IDENT "[a-zA-Z_][a-zA-Z0-9_]*")
       '(OP_EQ "==")
       '(OP_NE "!=")
       '(OP_LE "<=")
       '(OP_GE ">=")
       '(OP "[+\\-*/=<>]")
       '(LPAREN "(")
       '(RPAREN ")")
       '(LBRACE "{")
       '(RBRACE "}")
       '(SEMI ";")
       '(COMMA ","))))

  (fset 'neovm--lex6-tokenize
    (lambda (input)
      (with-temp-buffer
        (insert input)
        (goto-char (point-min))
        (let ((tokens nil)
              (specs (funcall 'neovm--lex6-token-specs))
              (line 1)
              (col 1)
              (line-start 1))
          (while (< (point) (point-max))
            (let ((matched nil)
                  (remaining specs))
              (while (and remaining (not matched))
                (let* ((spec (car remaining))
                       (typ (car spec))
                       (regexp (cadr spec)))
                  (when (looking-at regexp)
                    (let ((text (match-string 0))
                          (start-col col))
                      (goto-char (match-end 0))
                      (cond
                       ((eq typ 'NEWLINE)
                        (setq line (1+ line)
                              line-start (point)
                              col 1))
                       ((eq typ 'WHITESPACE)
                        (setq col (- (point) line-start -1)))
                       (t
                        (setq tokens (cons (list typ text line start-col) tokens))
                        (setq col (- (point) line-start -1))))
                      (setq matched t))))
                (setq remaining (cdr remaining)))
              ;; Skip unrecognized character
              (unless matched
                (forward-char 1)
                (setq col (- (point) line-start -1)))))
          (nreverse tokens)))))

  (unwind-protect
      (let ((code "let x = 3.14;\nif x > 2 {\n  return x * 2\n}"))
        (let ((tokens (funcall 'neovm--lex6-tokenize code)))
          (list
           (length tokens)
           tokens
           ;; Verify FLOAT token has correct value
           (nth 1 (seq-find (lambda (tok) (eq (car tok) 'FLOAT)) tokens))
           ;; Check multi-line
           (nth 2 (car (last tokens))))))
    (fmakunbound 'neovm--lex6-token-specs)
    (fmakunbound 'neovm--lex6-tokenize)))"#;
    let expect = expect_test::expect![[
        r#""OK (14 ((KEYWORD \"let\" 1 1) (IDENT \"x\" 1 5) (OP \"=\" 1 7) (FLOAT \"3.14\" 1 9) (SEMI \";\" 1 13) (KEYWORD \"if\" 2 1) (IDENT \"x\" 2 4) (OP \">\" 2 6) (INT \"2\" 2 8) (LBRACE \"{\" 2 10) (KEYWORD \"return\" 3 3) (IDENT \"x\" 3 10) (INT \"2\" 3 14) (RBRACE \"}\" 4 1)) \"3.14\" 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Full lexer pipeline: tokenize + classify + statistics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexer_full_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--lex7-keywords '("if" "else" "while" "return" "let" "fn" "true" "false"))

  (fset 'neovm--lex7-tokenize
    (lambda (input)
      (with-temp-buffer
        (insert input)
        (goto-char (point-min))
        (let ((tokens nil)
              (line 1)
              (col 1)
              (line-start 1))
          (while (< (point) (point-max))
            (let ((ch (char-after)))
              (cond
               ((= ch ?\n)
                (forward-char 1)
                (setq line (1+ line) line-start (point) col 1))
               ((or (= ch ?\s) (= ch ?\t))
                (forward-char 1)
                (setq col (- (point) line-start -1)))
               ((and (>= ch ?0) (<= ch ?9))
                (let ((start (point)) (start-col col))
                  (while (and (< (point) (point-max))
                              (let ((c (char-after)))
                                (or (and (>= c ?0) (<= c ?9))
                                    (= c ?.))))
                    (forward-char 1))
                  (let ((text (buffer-substring-no-properties start (point))))
                    (setq tokens (cons (list (if (string-match-p "\\." text) 'FLOAT 'INT)
                                             text line start-col) tokens)))
                  (setq col (- (point) line-start -1))))
               ((or (and (>= ch ?a) (<= ch ?z))
                    (and (>= ch ?A) (<= ch ?Z))
                    (= ch ?_))
                (let ((start (point)) (start-col col))
                  (while (and (< (point) (point-max))
                              (let ((c (char-after)))
                                (or (and (>= c ?a) (<= c ?z))
                                    (and (>= c ?A) (<= c ?Z))
                                    (and (>= c ?0) (<= c ?9))
                                    (= c ?_))))
                    (forward-char 1))
                  (let ((text (buffer-substring-no-properties start (point))))
                    (setq tokens (cons (list (if (member text neovm--lex7-keywords) 'KEYWORD 'IDENT)
                                             text line start-col) tokens)))
                  (setq col (- (point) line-start -1))))
               ((and (memq ch '(?= ?! ?< ?>))
                     (< (1+ (point)) (point-max))
                     (= (char-after (1+ (point))) ?=))
                (let ((start-col col)
                      (op (buffer-substring-no-properties (point) (+ (point) 2))))
                  (forward-char 2)
                  (setq tokens (cons (list 'OP op line start-col) tokens))
                  (setq col (- (point) line-start -1))))
               ((memq ch '(?+ ?- ?* ?/ ?= ?< ?>))
                (let ((start-col col))
                  (setq tokens (cons (list 'OP (char-to-string ch) line start-col) tokens))
                  (forward-char 1)
                  (setq col (- (point) line-start -1))))
               ((memq ch '(?\( ?\) ?\{ ?\} ?\; ?,))
                (let ((start-col col)
                      (typ (cond ((= ch ?\() 'LPAREN) ((= ch ?\)) 'RPAREN)
                                 ((= ch ?\{) 'LBRACE) ((= ch ?\}) 'RBRACE)
                                 ((= ch ?\;) 'SEMI) ((= ch ?,) 'COMMA))))
                  (setq tokens (cons (list typ (char-to-string ch) line start-col) tokens))
                  (forward-char 1)
                  (setq col (- (point) line-start -1))))
               (t (forward-char 1)
                  (setq col (- (point) line-start -1))))))
          (nreverse tokens)))))

  ;; Compute statistics on token stream
  (fset 'neovm--lex7-stats
    (lambda (tokens)
      (let ((type-counts (make-hash-table))
            (ident-freq (make-hash-table :test 'equal))
            (total 0)
            (max-line 0))
        (dolist (tok tokens)
          (setq total (1+ total))
          (puthash (car tok)
                   (1+ (gethash (car tok) type-counts 0))
                   type-counts)
          (when (eq (car tok) 'IDENT)
            (puthash (nth 1 tok)
                     (1+ (gethash (nth 1 tok) ident-freq 0))
                     ident-freq))
          (when (> (nth 2 tok) max-line)
            (setq max-line (nth 2 tok))))
        ;; Build sorted type counts
        (let ((tc nil))
          (maphash (lambda (k v) (setq tc (cons (cons k v) tc))) type-counts)
          (setq tc (sort tc (lambda (a b) (> (cdr a) (cdr b))))))
        ;; Build sorted ident frequencies
        (let ((idc nil))
          (maphash (lambda (k v) (setq idc (cons (cons k v) idc))) ident-freq)
          (setq idc (sort idc (lambda (a b) (> (cdr a) (cdr b))))))
        (let ((tc nil) (idc nil))
          (maphash (lambda (k v) (setq tc (cons (cons k v) tc))) type-counts)
          (maphash (lambda (k v) (setq idc (cons (cons k v) idc))) ident-freq)
          (list
           (cons 'total total)
           (cons 'lines max-line)
           (cons 'types (sort tc (lambda (a b) (> (cdr a) (cdr b)))))
           (cons 'idents (sort idc (lambda (a b) (> (cdr a) (cdr b))))))))))

  (unwind-protect
      (let* ((code "fn fib(n) {\n  if n <= 1 {\n    return n\n  }\n  let a = 0;\n  let b = 1;\n  let i = 2;\n  while i <= n {\n    let temp = a + b;\n    a = b;\n    b = temp;\n    i = i + 1;\n  }\n  return b;\n}")
             (tokens (funcall 'neovm--lex7-tokenize code))
             (stats (funcall 'neovm--lex7-stats tokens)))
        (list
         ;; Token count
         (cdr (assq 'total stats))
         ;; Lines
         (cdr (assq 'lines stats))
         ;; Type distribution
         (cdr (assq 'types stats))
         ;; Most used identifiers
         (cdr (assq 'idents stats))
         ;; Verify balanced parens/braces
         (let ((paren-depth 0) (brace-depth 0))
           (dolist (tok tokens)
             (cond ((eq (car tok) 'LPAREN) (setq paren-depth (1+ paren-depth)))
                   ((eq (car tok) 'RPAREN) (setq paren-depth (1- paren-depth)))
                   ((eq (car tok) 'LBRACE) (setq brace-depth (1+ brace-depth)))
                   ((eq (car tok) 'RBRACE) (setq brace-depth (1- brace-depth)))))
           (list (= paren-depth 0) (= brace-depth 0)))))
    (fmakunbound 'neovm--lex7-tokenize)
    (fmakunbound 'neovm--lex7-stats)
    (makunbound 'neovm--lex7-keywords)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable neovm--lex7-keywords)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
