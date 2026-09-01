//! Advanced oracle parity tests for a recursive descent JSON parser in Elisp:
//! tokenizer with lookahead, JSON objects, arrays, strings (with escapes),
//! numbers, booleans, null, nested structures, whitespace handling,
//! and error recovery/reporting.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// JSON tokenizer: numbers, strings (with escapes), booleans, null, punctuation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rd_adv_json_tokenizer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (fset 'neovm--json-is-ws
    (lambda (ch) (memq ch '(?\s ?\t ?\n ?\r))))

  (fset 'neovm--json-is-digit
    (lambda (ch) (and (>= ch ?0) (<= ch ?9))))

  (fset 'neovm--json-tokenize
    (lambda (input)
      (let ((pos 0) (tokens nil) (len (length input)))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ;; Whitespace
              ((funcall 'neovm--json-is-ws ch)
               (setq pos (1+ pos)))
              ;; String
              ((= ch ?\")
               (setq pos (1+ pos))
               (let ((start pos) (chars nil) (escaped nil))
                 (while (and (< pos len)
                             (or escaped (not (= (aref input pos) ?\"))))
                   (if escaped
                       (let ((ech (aref input pos)))
                         (setq chars (cons (cond
                                            ((= ech ?n) ?\n)
                                            ((= ech ?t) ?\t)
                                            ((= ech ?\\) ?\\)
                                            ((= ech ?\") ?\")
                                            ((= ech ?/) ?/)
                                            (t ech))
                                           chars))
                         (setq escaped nil))
                     (if (= (aref input pos) ?\\)
                         (setq escaped t)
                       (setq chars (cons (aref input pos) chars))))
                   (setq pos (1+ pos)))
                 (when (< pos len) (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'string (apply 'string (nreverse chars)))
                                    tokens))))
              ;; Number (integer or decimal, with optional leading minus)
              ((or (funcall 'neovm--json-is-digit ch)
                   (and (= ch ?-) (< (1+ pos) len)
                        (funcall 'neovm--json-is-digit (aref input (1+ pos)))))
               (let ((start pos))
                 (when (= ch ?-) (setq pos (1+ pos)))
                 (while (and (< pos len) (funcall 'neovm--json-is-digit (aref input pos)))
                   (setq pos (1+ pos)))
                 (when (and (< pos len) (= (aref input pos) ?.))
                   (setq pos (1+ pos))
                   (while (and (< pos len) (funcall 'neovm--json-is-digit (aref input pos)))
                     (setq pos (1+ pos))))
                 (setq tokens (cons (cons 'number (string-to-number (substring input start pos)))
                                    tokens))))
              ;; true
              ((and (<= (+ pos 4) len) (string= (substring input pos (+ pos 4)) "true"))
               (setq tokens (cons (cons 'boolean t) tokens))
               (setq pos (+ pos 4)))
              ;; false
              ((and (<= (+ pos 5) len) (string= (substring input pos (+ pos 5)) "false"))
               (setq tokens (cons (cons 'boolean nil) tokens))
               (setq pos (+ pos 5)))
              ;; null
              ((and (<= (+ pos 4) len) (string= (substring input pos (+ pos 4)) "null"))
               (setq tokens (cons '(null . nil) tokens))
               (setq pos (+ pos 4)))
              ;; Punctuation
              ((= ch ?\{) (setq tokens (cons '(lbrace . "{") tokens)) (setq pos (1+ pos)))
              ((= ch ?\}) (setq tokens (cons '(rbrace . "}") tokens)) (setq pos (1+ pos)))
              ((= ch ?\[) (setq tokens (cons '(lbracket . "[") tokens)) (setq pos (1+ pos)))
              ((= ch ?\]) (setq tokens (cons '(rbracket . "]") tokens)) (setq pos (1+ pos)))
              ((= ch ?,) (setq tokens (cons '(comma . ",") tokens)) (setq pos (1+ pos)))
              ((= ch ?:) (setq tokens (cons '(colon . ":") tokens)) (setq pos (1+ pos)))
              ;; Unknown
              (t (setq tokens (cons (cons 'error (char-to-string ch)) tokens))
                 (setq pos (1+ pos))))))
        (nreverse tokens))))

  (unwind-protect
      (list
        ;; Simple values
        (funcall 'neovm--json-tokenize "42")
        (funcall 'neovm--json-tokenize "-3.14")
        (funcall 'neovm--json-tokenize "\"hello\"")
        (funcall 'neovm--json-tokenize "true")
        (funcall 'neovm--json-tokenize "false")
        (funcall 'neovm--json-tokenize "null")
        ;; String with escapes
        (funcall 'neovm--json-tokenize "\"hello\\nworld\"")
        (funcall 'neovm--json-tokenize "\"a\\tb\\\\c\"")
        (funcall 'neovm--json-tokenize "\"quote: \\\"hi\\\"\"")
        ;; Object
        (funcall 'neovm--json-tokenize "{\"key\": \"value\"}")
        ;; Array
        (funcall 'neovm--json-tokenize "[1, 2, 3]")
        ;; Complex nested
        (funcall 'neovm--json-tokenize "{\"a\": [1, true, null]}")
        ;; Whitespace handling
        (funcall 'neovm--json-tokenize "  { \"x\" :  42  } ")
        ;; Empty containers
        (funcall 'neovm--json-tokenize "{}")
        (funcall 'neovm--json-tokenize "[]"))
    (fmakunbound 'neovm--json-is-ws)
    (fmakunbound 'neovm--json-is-digit)
    (fmakunbound 'neovm--json-tokenize)))
"#;
    let expect = expect_test::expect![[
        r#""OK (((number . 42)) ((number . -3.14)) ((string . \"hello\")) ((boolean . t)) ((boolean)) ((null)) ((string . \"hello\\nworld\")) ((string . \"a\tb\\\\c\")) ((string . \"quote: \\\"hi\\\"\")) ((lbrace . \"{\") (string . \"key\") (colon . \":\") (string . \"value\") (rbrace . \"}\")) ((lbracket . \"[\") (number . 1) (comma . \",\") (number . 2) (comma . \",\") (number . 3) (rbracket . \"]\")) ((lbrace . \"{\") (string . \"a\") (colon . \":\") (lbracket . \"[\") (number . 1) (comma . \",\") (boolean . t) (comma . \",\") (null) (rbracket . \"]\") (rbrace . \"}\")) ((lbrace . \"{\") (string . \"x\") (colon . \":\") (number . 42) (rbrace . \"}\")) ((lbrace . \"{\") (rbrace . \"}\")) ((lbracket . \"[\") (rbracket . \"]\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Full JSON parser: objects, arrays, all value types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rd_adv_json_parser_full() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; Tokenizer (compact)
  (fset 'neovm--jp-tokenize
    (lambda (input)
      (let ((pos 0) (tokens nil) (len (length input)))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ((memq ch '(?\s ?\t ?\n ?\r)) (setq pos (1+ pos)))
              ((= ch ?\")
               (setq pos (1+ pos))
               (let ((chars nil) (escaped nil))
                 (while (and (< pos len) (or escaped (not (= (aref input pos) ?\"))))
                   (if escaped
                       (progn (setq chars (cons (let ((e (aref input pos)))
                                                  (cond ((= e ?n) ?\n) ((= e ?t) ?\t)
                                                        ((= e ?\\) ?\\) ((= e ?\") ?\") (t e)))
                                                chars))
                              (setq escaped nil))
                     (if (= (aref input pos) ?\\) (setq escaped t)
                       (setq chars (cons (aref input pos) chars))))
                   (setq pos (1+ pos)))
                 (when (< pos len) (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'string (apply 'string (nreverse chars))) tokens))))
              ((or (and (>= ch ?0) (<= ch ?9))
                   (and (= ch ?-) (< (1+ pos) len) (>= (aref input (1+ pos)) ?0) (<= (aref input (1+ pos)) ?9)))
               (let ((start pos))
                 (when (= ch ?-) (setq pos (1+ pos)))
                 (while (and (< pos len) (>= (aref input pos) ?0) (<= (aref input pos) ?9)) (setq pos (1+ pos)))
                 (when (and (< pos len) (= (aref input pos) ?.))
                   (setq pos (1+ pos))
                   (while (and (< pos len) (>= (aref input pos) ?0) (<= (aref input pos) ?9)) (setq pos (1+ pos))))
                 (setq tokens (cons (cons 'number (string-to-number (substring input start pos))) tokens))))
              ((and (<= (+ pos 4) len) (string= (substring input pos (+ pos 4)) "true"))
               (setq tokens (cons '(boolean . t) tokens)) (setq pos (+ pos 4)))
              ((and (<= (+ pos 5) len) (string= (substring input pos (+ pos 5)) "false"))
               (setq tokens (cons '(boolean . nil) tokens)) (setq pos (+ pos 5)))
              ((and (<= (+ pos 4) len) (string= (substring input pos (+ pos 4)) "null"))
               (setq tokens (cons '(null . nil) tokens)) (setq pos (+ pos 4)))
              ((= ch ?\{) (setq tokens (cons '(lbrace . "{") tokens)) (setq pos (1+ pos)))
              ((= ch ?\}) (setq tokens (cons '(rbrace . "}") tokens)) (setq pos (1+ pos)))
              ((= ch ?\[) (setq tokens (cons '(lbracket . "[") tokens)) (setq pos (1+ pos)))
              ((= ch ?\]) (setq tokens (cons '(rbracket . "]") tokens)) (setq pos (1+ pos)))
              ((= ch ?,) (setq tokens (cons '(comma . ",") tokens)) (setq pos (1+ pos)))
              ((= ch ?:) (setq tokens (cons '(colon . ":") tokens)) (setq pos (1+ pos)))
              (t (setq pos (1+ pos))))))
        (nreverse tokens))))

  ;; Parser state
  (defvar neovm--jp-tokens nil)
  (fset 'neovm--jp-peek (lambda () (car neovm--jp-tokens)))
  (fset 'neovm--jp-advance
    (lambda () (let ((t1 (car neovm--jp-tokens)))
                 (setq neovm--jp-tokens (cdr neovm--jp-tokens)) t1)))
  (fset 'neovm--jp-expect
    (lambda (type)
      (let ((t1 (car neovm--jp-tokens)))
        (if (and t1 (eq (car t1) type))
            (funcall 'neovm--jp-advance)
          nil))))

  ;; value = string | number | boolean | null | object | array
  (fset 'neovm--jp-parse-value
    (lambda ()
      (let ((tok (funcall 'neovm--jp-peek)))
        (cond
          ((null tok) '(error "unexpected end"))
          ((eq (car tok) 'string) (funcall 'neovm--jp-advance) (list 'string (cdr tok)))
          ((eq (car tok) 'number) (funcall 'neovm--jp-advance) (list 'number (cdr tok)))
          ((eq (car tok) 'boolean) (funcall 'neovm--jp-advance) (list 'boolean (cdr tok)))
          ((eq (car tok) 'null) (funcall 'neovm--jp-advance) '(null))
          ((eq (car tok) 'lbrace) (funcall 'neovm--jp-parse-object))
          ((eq (car tok) 'lbracket) (funcall 'neovm--jp-parse-array))
          (t (list 'error (format "unexpected %S" tok)))))))

  ;; object = '{' (string ':' value (',' string ':' value)*)? '}'
  (fset 'neovm--jp-parse-object
    (lambda ()
      (funcall 'neovm--jp-advance)  ;; consume '{'
      (let ((pairs nil))
        (unless (and (funcall 'neovm--jp-peek) (eq (car (funcall 'neovm--jp-peek)) 'rbrace))
          (let ((done nil))
            (while (not done)
              (let ((key-tok (funcall 'neovm--jp-expect 'string)))
                (if (null key-tok)
                    (progn (setq done t) (setq pairs (cons '(error "expected string key") pairs)))
                  (funcall 'neovm--jp-expect 'colon)
                  (let ((val (funcall 'neovm--jp-parse-value)))
                    (setq pairs (cons (list (cdr key-tok) val) pairs))
                    (if (funcall 'neovm--jp-expect 'comma)
                        nil  ;; continue
                      (setq done t))))))))
        (funcall 'neovm--jp-expect 'rbrace)
        (list 'object (nreverse pairs)))))

  ;; array = '[' (value (',' value)*)? ']'
  (fset 'neovm--jp-parse-array
    (lambda ()
      (funcall 'neovm--jp-advance)  ;; consume '['
      (let ((items nil))
        (unless (and (funcall 'neovm--jp-peek) (eq (car (funcall 'neovm--jp-peek)) 'rbracket))
          (let ((done nil))
            (while (not done)
              (setq items (cons (funcall 'neovm--jp-parse-value) items))
              (if (funcall 'neovm--jp-expect 'comma)
                  nil
                (setq done t)))))
        (funcall 'neovm--jp-expect 'rbracket)
        (list 'array (nreverse items)))))

  (fset 'neovm--jp-parse
    (lambda (input)
      (setq neovm--jp-tokens (funcall 'neovm--jp-tokenize input))
      (let ((result (funcall 'neovm--jp-parse-value)))
        (if neovm--jp-tokens
            (list 'partial result 'remaining neovm--jp-tokens)
          result))))

  (unwind-protect
      (list
        ;; Primitive values
        (funcall 'neovm--jp-parse "42")
        (funcall 'neovm--jp-parse "-3.14")
        (funcall 'neovm--jp-parse "\"hello\"")
        (funcall 'neovm--jp-parse "true")
        (funcall 'neovm--jp-parse "false")
        (funcall 'neovm--jp-parse "null")
        ;; Simple object
        (funcall 'neovm--jp-parse "{\"name\": \"Alice\", \"age\": 30}")
        ;; Simple array
        (funcall 'neovm--jp-parse "[1, 2, 3]")
        ;; Empty containers
        (funcall 'neovm--jp-parse "{}")
        (funcall 'neovm--jp-parse "[]")
        ;; Nested object in array
        (funcall 'neovm--jp-parse "[{\"a\": 1}, {\"b\": 2}]")
        ;; Array in object
        (funcall 'neovm--jp-parse "{\"items\": [10, 20, 30]}")
        ;; String with escapes
        (funcall 'neovm--jp-parse "{\"msg\": \"hello\\nworld\"}"))
    (fmakunbound 'neovm--jp-tokenize)
    (fmakunbound 'neovm--jp-peek)
    (fmakunbound 'neovm--jp-advance)
    (fmakunbound 'neovm--jp-expect)
    (fmakunbound 'neovm--jp-parse-value)
    (fmakunbound 'neovm--jp-parse-object)
    (fmakunbound 'neovm--jp-parse-array)
    (fmakunbound 'neovm--jp-parse)
    (makunbound 'neovm--jp-tokens)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((number 42) (number -3.14) (string \"hello\") (boolean t) (boolean nil) (null) (object ((\"name\" (string \"Alice\")) (\"age\" (number 30)))) (array ((number 1) (number 2) (number 3))) (object nil) (array nil) (array ((object ((\"a\" (number 1)))) (object ((\"b\" (number 2)))))) (object ((\"items\" (array ((number 10) (number 20) (number 30)))))) (object ((\"msg\" (string \"hello\\nworld\")))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Deeply nested JSON structures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rd_adv_json_deep_nesting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; Reuse tokenizer/parser from above (compact inline)
  (fset 'neovm--jpn-tokenize
    (lambda (input)
      (let ((pos 0) (tokens nil) (len (length input)))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ((memq ch '(?\s ?\t ?\n ?\r)) (setq pos (1+ pos)))
              ((= ch ?\")
               (setq pos (1+ pos))
               (let ((chars nil) (escaped nil))
                 (while (and (< pos len) (or escaped (not (= (aref input pos) ?\"))))
                   (if escaped
                       (progn (setq chars (cons (let ((e (aref input pos)))
                                                  (cond ((= e ?n) ?\n) ((= e ?\\) ?\\)
                                                        ((= e ?\") ?\") (t e))) chars))
                              (setq escaped nil))
                     (if (= (aref input pos) ?\\) (setq escaped t)
                       (setq chars (cons (aref input pos) chars))))
                   (setq pos (1+ pos)))
                 (when (< pos len) (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'string (apply 'string (nreverse chars))) tokens))))
              ((or (and (>= ch ?0) (<= ch ?9))
                   (and (= ch ?-) (< (1+ pos) len) (>= (aref input (1+ pos)) ?0) (<= (aref input (1+ pos)) ?9)))
               (let ((start pos))
                 (when (= ch ?-) (setq pos (1+ pos)))
                 (while (and (< pos len) (>= (aref input pos) ?0) (<= (aref input pos) ?9)) (setq pos (1+ pos)))
                 (when (and (< pos len) (= (aref input pos) ?.))
                   (setq pos (1+ pos))
                   (while (and (< pos len) (>= (aref input pos) ?0) (<= (aref input pos) ?9)) (setq pos (1+ pos))))
                 (setq tokens (cons (cons 'number (string-to-number (substring input start pos))) tokens))))
              ((and (<= (+ pos 4) len) (string= (substring input pos (+ pos 4)) "true"))
               (setq tokens (cons '(boolean . t) tokens)) (setq pos (+ pos 4)))
              ((and (<= (+ pos 5) len) (string= (substring input pos (+ pos 5)) "false"))
               (setq tokens (cons '(boolean . nil) tokens)) (setq pos (+ pos 5)))
              ((and (<= (+ pos 4) len) (string= (substring input pos (+ pos 4)) "null"))
               (setq tokens (cons '(null . nil) tokens)) (setq pos (+ pos 4)))
              ((= ch ?\{) (setq tokens (cons '(lbrace) tokens)) (setq pos (1+ pos)))
              ((= ch ?\}) (setq tokens (cons '(rbrace) tokens)) (setq pos (1+ pos)))
              ((= ch ?\[) (setq tokens (cons '(lbracket) tokens)) (setq pos (1+ pos)))
              ((= ch ?\]) (setq tokens (cons '(rbracket) tokens)) (setq pos (1+ pos)))
              ((= ch ?,) (setq tokens (cons '(comma) tokens)) (setq pos (1+ pos)))
              ((= ch ?:) (setq tokens (cons '(colon) tokens)) (setq pos (1+ pos)))
              (t (setq pos (1+ pos))))))
        (nreverse tokens))))

  (defvar neovm--jpn-tokens nil)
  (fset 'neovm--jpn-peek (lambda () (car neovm--jpn-tokens)))
  (fset 'neovm--jpn-advance
    (lambda () (let ((t1 (car neovm--jpn-tokens)))
                 (setq neovm--jpn-tokens (cdr neovm--jpn-tokens)) t1)))
  (fset 'neovm--jpn-expect
    (lambda (type) (if (and (car neovm--jpn-tokens) (eq (car (car neovm--jpn-tokens)) type))
                       (funcall 'neovm--jpn-advance) nil)))

  (fset 'neovm--jpn-parse-value
    (lambda ()
      (let ((tok (funcall 'neovm--jpn-peek)))
        (cond
          ((null tok) '(error "eof"))
          ((eq (car tok) 'string) (funcall 'neovm--jpn-advance) (list 'string (cdr tok)))
          ((eq (car tok) 'number) (funcall 'neovm--jpn-advance) (list 'number (cdr tok)))
          ((eq (car tok) 'boolean) (funcall 'neovm--jpn-advance) (list 'boolean (cdr tok)))
          ((eq (car tok) 'null) (funcall 'neovm--jpn-advance) '(null))
          ((eq (car tok) 'lbrace) (funcall 'neovm--jpn-parse-object))
          ((eq (car tok) 'lbracket) (funcall 'neovm--jpn-parse-array))
          (t (funcall 'neovm--jpn-advance) (list 'error "bad token"))))))

  (fset 'neovm--jpn-parse-object
    (lambda ()
      (funcall 'neovm--jpn-advance)
      (let ((pairs nil))
        (unless (and (funcall 'neovm--jpn-peek) (eq (car (funcall 'neovm--jpn-peek)) 'rbrace))
          (let ((done nil))
            (while (not done)
              (let ((k (funcall 'neovm--jpn-expect 'string)))
                (if k (progn (funcall 'neovm--jpn-expect 'colon)
                             (setq pairs (cons (list (cdr k) (funcall 'neovm--jpn-parse-value)) pairs))
                             (unless (funcall 'neovm--jpn-expect 'comma) (setq done t)))
                  (setq done t))))))
        (funcall 'neovm--jpn-expect 'rbrace)
        (list 'object (nreverse pairs)))))

  (fset 'neovm--jpn-parse-array
    (lambda ()
      (funcall 'neovm--jpn-advance)
      (let ((items nil))
        (unless (and (funcall 'neovm--jpn-peek) (eq (car (funcall 'neovm--jpn-peek)) 'rbracket))
          (let ((done nil))
            (while (not done)
              (setq items (cons (funcall 'neovm--jpn-parse-value) items))
              (unless (funcall 'neovm--jpn-expect 'comma) (setq done t)))))
        (funcall 'neovm--jpn-expect 'rbracket)
        (list 'array (nreverse items)))))

  (fset 'neovm--jpn-parse
    (lambda (input)
      (setq neovm--jpn-tokens (funcall 'neovm--jpn-tokenize input))
      (funcall 'neovm--jpn-parse-value)))

  (unwind-protect
      (list
        ;; Nested arrays
        (funcall 'neovm--jpn-parse "[[1, 2], [3, [4, 5]]]")
        ;; Nested objects
        (funcall 'neovm--jpn-parse "{\"a\": {\"b\": {\"c\": 42}}}")
        ;; Mixed deep nesting
        (funcall 'neovm--jpn-parse "{\"data\": [{\"id\": 1, \"tags\": [\"a\", \"b\"]}, {\"id\": 2, \"tags\": []}]}")
        ;; Object with all value types
        (funcall 'neovm--jpn-parse "{\"str\": \"hello\", \"num\": 42, \"float\": 3.14, \"bool_t\": true, \"bool_f\": false, \"nul\": null, \"arr\": [1], \"obj\": {}}")
        ;; Array of mixed types
        (funcall 'neovm--jpn-parse "[\"str\", 42, true, false, null, [], {}]")
        ;; Nested empty containers
        (funcall 'neovm--jpn-parse "{\"a\": {}, \"b\": [], \"c\": {\"d\": []}}")
        ;; Negative numbers in various positions
        (funcall 'neovm--jpn-parse "[-1, -2.5, {\"x\": -100}]"))
    (fmakunbound 'neovm--jpn-tokenize)
    (fmakunbound 'neovm--jpn-peek)
    (fmakunbound 'neovm--jpn-advance)
    (fmakunbound 'neovm--jpn-expect)
    (fmakunbound 'neovm--jpn-parse-value)
    (fmakunbound 'neovm--jpn-parse-object)
    (fmakunbound 'neovm--jpn-parse-array)
    (fmakunbound 'neovm--jpn-parse)
    (makunbound 'neovm--jpn-tokens)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((array ((array ((number 1) (number 2))) (array ((number 3) (array ((number 4) (number 5))))))) (object ((\"a\" (object ((\"b\" (object ((\"c\" (number 42)))))))))) (object ((\"data\" (array ((object ((\"id\" (number 1)) (\"tags\" (array ((string \"a\") (string \"b\")))))) (object ((\"id\" (number 2)) (\"tags\" (array nil))))))))) (object ((\"str\" (string \"hello\")) (\"num\" (number 42)) (\"float\" (number 3.14)) (\"bool_t\" (boolean t)) (\"bool_f\" (boolean nil)) (\"nul\" (null)) (\"arr\" (array ((number 1)))) (\"obj\" (object nil)))) (array ((string \"str\") (number 42) (boolean t) (boolean nil) (null) (array nil) (object nil))) (object ((\"a\" (object nil)) (\"b\" (array nil)) (\"c\" (object ((\"d\" (array nil))))))) (array ((number -1) (number -2.5) (object ((\"x\" (number -100)))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// JSON AST accessor/query functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rd_adv_json_ast_queries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse JSON then query the AST: get by key, get by index,
    // walk paths, count nodes, extract leaf values.
    let form = r#"
(progn
  ;; Compact tokenizer/parser (same structure)
  (fset 'neovm--jpq-tokenize
    (lambda (input)
      (let ((pos 0) (tokens nil) (len (length input)))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ((memq ch '(?\s ?\t ?\n ?\r)) (setq pos (1+ pos)))
              ((= ch ?\")
               (setq pos (1+ pos))
               (let ((chars nil) (esc nil))
                 (while (and (< pos len) (or esc (not (= (aref input pos) ?\"))))
                   (if esc (progn (setq chars (cons (aref input pos) chars) esc nil))
                     (if (= (aref input pos) ?\\) (setq esc t) (setq chars (cons (aref input pos) chars))))
                   (setq pos (1+ pos)))
                 (when (< pos len) (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'string (apply 'string (nreverse chars))) tokens))))
              ((or (and (>= ch ?0) (<= ch ?9)) (and (= ch ?-) (< (1+ pos) len) (>= (aref input (1+ pos)) ?0)))
               (let ((start pos))
                 (when (= ch ?-) (setq pos (1+ pos)))
                 (while (and (< pos len) (>= (aref input pos) ?0) (<= (aref input pos) ?9)) (setq pos (1+ pos)))
                 (when (and (< pos len) (= (aref input pos) ?.))
                   (setq pos (1+ pos))
                   (while (and (< pos len) (>= (aref input pos) ?0) (<= (aref input pos) ?9)) (setq pos (1+ pos))))
                 (setq tokens (cons (cons 'number (string-to-number (substring input start pos))) tokens))))
              ((and (<= (+ pos 4) len) (string= (substring input pos (+ pos 4)) "true"))
               (setq tokens (cons '(boolean . t) tokens)) (setq pos (+ pos 4)))
              ((and (<= (+ pos 5) len) (string= (substring input pos (+ pos 5)) "false"))
               (setq tokens (cons '(boolean . nil) tokens)) (setq pos (+ pos 5)))
              ((and (<= (+ pos 4) len) (string= (substring input pos (+ pos 4)) "null"))
               (setq tokens (cons '(null) tokens)) (setq pos (+ pos 4)))
              ((= ch ?\{) (setq tokens (cons '(lbrace) tokens)) (setq pos (1+ pos)))
              ((= ch ?\}) (setq tokens (cons '(rbrace) tokens)) (setq pos (1+ pos)))
              ((= ch ?\[) (setq tokens (cons '(lbracket) tokens)) (setq pos (1+ pos)))
              ((= ch ?\]) (setq tokens (cons '(rbracket) tokens)) (setq pos (1+ pos)))
              ((= ch ?,) (setq tokens (cons '(comma) tokens)) (setq pos (1+ pos)))
              ((= ch ?:) (setq tokens (cons '(colon) tokens)) (setq pos (1+ pos)))
              (t (setq pos (1+ pos))))))
        (nreverse tokens))))

  (defvar neovm--jpq-tokens nil)
  (fset 'neovm--jpq-peek (lambda () (car neovm--jpq-tokens)))
  (fset 'neovm--jpq-advance (lambda () (let ((t1 (car neovm--jpq-tokens))) (setq neovm--jpq-tokens (cdr neovm--jpq-tokens)) t1)))
  (fset 'neovm--jpq-expect (lambda (tp) (if (and (car neovm--jpq-tokens) (eq (car (car neovm--jpq-tokens)) tp)) (funcall 'neovm--jpq-advance) nil)))

  (fset 'neovm--jpq-parse-value
    (lambda ()
      (let ((tok (funcall 'neovm--jpq-peek)))
        (cond
          ((null tok) '(null))
          ((eq (car tok) 'string) (funcall 'neovm--jpq-advance) (list 'string (cdr tok)))
          ((eq (car tok) 'number) (funcall 'neovm--jpq-advance) (list 'number (cdr tok)))
          ((eq (car tok) 'boolean) (funcall 'neovm--jpq-advance) (list 'boolean (cdr tok)))
          ((eq (car tok) 'null) (funcall 'neovm--jpq-advance) '(null))
          ((eq (car tok) 'lbrace) (funcall 'neovm--jpq-parse-object))
          ((eq (car tok) 'lbracket) (funcall 'neovm--jpq-parse-array))
          (t (funcall 'neovm--jpq-advance) '(null))))))

  (fset 'neovm--jpq-parse-object
    (lambda ()
      (funcall 'neovm--jpq-advance)
      (let ((pairs nil))
        (unless (and (funcall 'neovm--jpq-peek) (eq (car (funcall 'neovm--jpq-peek)) 'rbrace))
          (let ((done nil))
            (while (not done)
              (let ((k (funcall 'neovm--jpq-expect 'string)))
                (if k (progn (funcall 'neovm--jpq-expect 'colon)
                             (setq pairs (cons (list (cdr k) (funcall 'neovm--jpq-parse-value)) pairs))
                             (unless (funcall 'neovm--jpq-expect 'comma) (setq done t)))
                  (setq done t))))))
        (funcall 'neovm--jpq-expect 'rbrace)
        (list 'object (nreverse pairs)))))

  (fset 'neovm--jpq-parse-array
    (lambda ()
      (funcall 'neovm--jpq-advance)
      (let ((items nil))
        (unless (and (funcall 'neovm--jpq-peek) (eq (car (funcall 'neovm--jpq-peek)) 'rbracket))
          (let ((done nil))
            (while (not done)
              (setq items (cons (funcall 'neovm--jpq-parse-value) items))
              (unless (funcall 'neovm--jpq-expect 'comma) (setq done t)))))
        (funcall 'neovm--jpq-expect 'rbracket)
        (list 'array (nreverse items)))))

  (fset 'neovm--jpq-parse
    (lambda (input)
      (setq neovm--jpq-tokens (funcall 'neovm--jpq-tokenize input))
      (funcall 'neovm--jpq-parse-value)))

  ;; Query helpers
  (fset 'neovm--jpq-get-key
    (lambda (ast key)
      "Get value by key from JSON object AST."
      (if (eq (car ast) 'object)
          (let ((found nil))
            (dolist (pair (cadr ast))
              (when (string= (car pair) key)
                (setq found (cadr pair))))
            found)
        nil)))

  (fset 'neovm--jpq-get-index
    (lambda (ast idx)
      "Get value by index from JSON array AST."
      (if (eq (car ast) 'array)
          (nth idx (cadr ast))
        nil)))

  ;; Count all nodes in AST
  (fset 'neovm--jpq-count-nodes
    (lambda (ast)
      (cond
       ((memq (car ast) '(string number boolean null)) 1)
       ((eq (car ast) 'object)
        (let ((count 1))
          (dolist (pair (cadr ast))
            (setq count (+ count 1 (funcall 'neovm--jpq-count-nodes (cadr pair)))))
          count))
       ((eq (car ast) 'array)
        (let ((count 1))
          (dolist (item (cadr ast))
            (setq count (+ count (funcall 'neovm--jpq-count-nodes item))))
          count))
       (t 1))))

  ;; Collect all leaf values
  (fset 'neovm--jpq-collect-leaves
    (lambda (ast)
      (cond
       ((memq (car ast) '(string number boolean null))
        (list (cadr ast)))
       ((eq (car ast) 'object)
        (let ((leaves nil))
          (dolist (pair (cadr ast))
            (setq leaves (append leaves (funcall 'neovm--jpq-collect-leaves (cadr pair)))))
          leaves))
       ((eq (car ast) 'array)
        (let ((leaves nil))
          (dolist (item (cadr ast))
            (setq leaves (append leaves (funcall 'neovm--jpq-collect-leaves item))))
          leaves))
       (t nil))))

  (unwind-protect
      (let ((ast (funcall 'neovm--jpq-parse
                   "{\"name\": \"Alice\", \"age\": 30, \"scores\": [95, 87, 92], \"address\": {\"city\": \"NYC\", \"zip\": 10001}}")))
        (list
          ;; Get by key
          (funcall 'neovm--jpq-get-key ast "name")
          (funcall 'neovm--jpq-get-key ast "age")
          (funcall 'neovm--jpq-get-key ast "scores")
          ;; Nested access
          (funcall 'neovm--jpq-get-key (funcall 'neovm--jpq-get-key ast "address") "city")
          (funcall 'neovm--jpq-get-key (funcall 'neovm--jpq-get-key ast "address") "zip")
          ;; Array index
          (funcall 'neovm--jpq-get-index (funcall 'neovm--jpq-get-key ast "scores") 0)
          (funcall 'neovm--jpq-get-index (funcall 'neovm--jpq-get-key ast "scores") 2)
          ;; Non-existent key
          (funcall 'neovm--jpq-get-key ast "nonexistent")
          ;; Node count
          (funcall 'neovm--jpq-count-nodes ast)
          ;; Collect all leaf values
          (funcall 'neovm--jpq-collect-leaves ast)))
    (fmakunbound 'neovm--jpq-tokenize)
    (fmakunbound 'neovm--jpq-peek)
    (fmakunbound 'neovm--jpq-advance)
    (fmakunbound 'neovm--jpq-expect)
    (fmakunbound 'neovm--jpq-parse-value)
    (fmakunbound 'neovm--jpq-parse-object)
    (fmakunbound 'neovm--jpq-parse-array)
    (fmakunbound 'neovm--jpq-parse)
    (fmakunbound 'neovm--jpq-get-key)
    (fmakunbound 'neovm--jpq-get-index)
    (fmakunbound 'neovm--jpq-count-nodes)
    (fmakunbound 'neovm--jpq-collect-leaves)
    (makunbound 'neovm--jpq-tokens)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((string \"Alice\") (number 30) (array ((number 95) (number 87) (number 92))) (string \"NYC\") (number 10001) (number 95) (number 92) nil 16 (\"Alice\" 30 95 87 92 \"NYC\" 10001))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// JSON error recovery and reporting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rd_adv_json_error_recovery() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parser that reports errors but attempts to continue
    let form = r#"
(progn
  ;; Compact tokenizer
  (fset 'neovm--jpe-tokenize
    (lambda (input)
      (let ((pos 0) (tokens nil) (len (length input)))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ((memq ch '(?\s ?\t ?\n ?\r)) (setq pos (1+ pos)))
              ((= ch ?\")
               (setq pos (1+ pos))
               (let ((chars nil) (esc nil))
                 (while (and (< pos len) (or esc (not (= (aref input pos) ?\"))))
                   (if esc (progn (setq chars (cons (aref input pos) chars) esc nil))
                     (if (= (aref input pos) ?\\) (setq esc t) (setq chars (cons (aref input pos) chars))))
                   (setq pos (1+ pos)))
                 (when (< pos len) (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'string (apply 'string (nreverse chars))) tokens))))
              ((or (and (>= ch ?0) (<= ch ?9)) (and (= ch ?-) (< (1+ pos) len) (>= (aref input (1+ pos)) ?0)))
               (let ((start pos))
                 (when (= ch ?-) (setq pos (1+ pos)))
                 (while (and (< pos len) (>= (aref input pos) ?0) (<= (aref input pos) ?9)) (setq pos (1+ pos)))
                 (when (and (< pos len) (= (aref input pos) ?.))
                   (setq pos (1+ pos))
                   (while (and (< pos len) (>= (aref input pos) ?0) (<= (aref input pos) ?9)) (setq pos (1+ pos))))
                 (setq tokens (cons (cons 'number (string-to-number (substring input start pos))) tokens))))
              ((and (<= (+ pos 4) len) (string= (substring input pos (+ pos 4)) "true"))
               (setq tokens (cons '(boolean . t) tokens)) (setq pos (+ pos 4)))
              ((and (<= (+ pos 5) len) (string= (substring input pos (+ pos 5)) "false"))
               (setq tokens (cons '(boolean . nil) tokens)) (setq pos (+ pos 5)))
              ((and (<= (+ pos 4) len) (string= (substring input pos (+ pos 4)) "null"))
               (setq tokens (cons '(null) tokens)) (setq pos (+ pos 4)))
              ((= ch ?\{) (setq tokens (cons '(lbrace) tokens)) (setq pos (1+ pos)))
              ((= ch ?\}) (setq tokens (cons '(rbrace) tokens)) (setq pos (1+ pos)))
              ((= ch ?\[) (setq tokens (cons '(lbracket) tokens)) (setq pos (1+ pos)))
              ((= ch ?\]) (setq tokens (cons '(rbracket) tokens)) (setq pos (1+ pos)))
              ((= ch ?,) (setq tokens (cons '(comma) tokens)) (setq pos (1+ pos)))
              ((= ch ?:) (setq tokens (cons '(colon) tokens)) (setq pos (1+ pos)))
              (t (setq tokens (cons (cons 'error (char-to-string ch)) tokens)) (setq pos (1+ pos))))))
        (nreverse tokens))))

  (defvar neovm--jpe-tokens nil)
  (defvar neovm--jpe-errors nil)

  (fset 'neovm--jpe-peek (lambda () (car neovm--jpe-tokens)))
  (fset 'neovm--jpe-advance (lambda () (let ((t1 (car neovm--jpe-tokens))) (setq neovm--jpe-tokens (cdr neovm--jpe-tokens)) t1)))
  (fset 'neovm--jpe-expect
    (lambda (tp)
      (if (and (car neovm--jpe-tokens) (eq (car (car neovm--jpe-tokens)) tp))
          (funcall 'neovm--jpe-advance)
        (setq neovm--jpe-errors (cons (format "expected %s got %S" tp (car neovm--jpe-tokens)) neovm--jpe-errors))
        nil)))

  ;; Skip to recovery point (comma, rbrace, rbracket, or end)
  (fset 'neovm--jpe-skip-to-recovery
    (lambda ()
      (while (and neovm--jpe-tokens
                  (not (memq (car (car neovm--jpe-tokens)) '(comma rbrace rbracket))))
        (funcall 'neovm--jpe-advance))))

  (fset 'neovm--jpe-parse-value
    (lambda ()
      (let ((tok (funcall 'neovm--jpe-peek)))
        (cond
          ((null tok) (setq neovm--jpe-errors (cons "unexpected end" neovm--jpe-errors)) '(null))
          ((eq (car tok) 'string) (funcall 'neovm--jpe-advance) (list 'string (cdr tok)))
          ((eq (car tok) 'number) (funcall 'neovm--jpe-advance) (list 'number (cdr tok)))
          ((eq (car tok) 'boolean) (funcall 'neovm--jpe-advance) (list 'boolean (cdr tok)))
          ((eq (car tok) 'null) (funcall 'neovm--jpe-advance) '(null))
          ((eq (car tok) 'lbrace) (funcall 'neovm--jpe-parse-object))
          ((eq (car tok) 'lbracket) (funcall 'neovm--jpe-parse-array))
          (t (setq neovm--jpe-errors (cons (format "unexpected token %S" tok) neovm--jpe-errors))
             (funcall 'neovm--jpe-skip-to-recovery)
             '(error "recovered"))))))

  (fset 'neovm--jpe-parse-object
    (lambda ()
      (funcall 'neovm--jpe-advance)
      (let ((pairs nil))
        (unless (and (funcall 'neovm--jpe-peek) (eq (car (funcall 'neovm--jpe-peek)) 'rbrace))
          (let ((done nil))
            (while (not done)
              (let ((k (funcall 'neovm--jpe-expect 'string)))
                (if k
                    (progn (funcall 'neovm--jpe-expect 'colon)
                           (setq pairs (cons (list (cdr k) (funcall 'neovm--jpe-parse-value)) pairs))
                           (unless (and (funcall 'neovm--jpe-peek) (eq (car (funcall 'neovm--jpe-peek)) 'comma))
                             (setq done t))
                           (when (and (not done) (funcall 'neovm--jpe-peek) (eq (car (funcall 'neovm--jpe-peek)) 'comma))
                             (funcall 'neovm--jpe-advance)))
                  (funcall 'neovm--jpe-skip-to-recovery)
                  (setq done t))))))
        (funcall 'neovm--jpe-expect 'rbrace)
        (list 'object (nreverse pairs)))))

  (fset 'neovm--jpe-parse-array
    (lambda ()
      (funcall 'neovm--jpe-advance)
      (let ((items nil))
        (unless (and (funcall 'neovm--jpe-peek) (eq (car (funcall 'neovm--jpe-peek)) 'rbracket))
          (let ((done nil))
            (while (not done)
              (setq items (cons (funcall 'neovm--jpe-parse-value) items))
              (if (and (funcall 'neovm--jpe-peek) (eq (car (funcall 'neovm--jpe-peek)) 'comma))
                  (funcall 'neovm--jpe-advance)
                (setq done t)))))
        (funcall 'neovm--jpe-expect 'rbracket)
        (list 'array (nreverse items)))))

  (fset 'neovm--jpe-parse
    (lambda (input)
      (setq neovm--jpe-tokens (funcall 'neovm--jpe-tokenize input))
      (setq neovm--jpe-errors nil)
      (let ((ast (funcall 'neovm--jpe-parse-value)))
        (list 'result ast 'errors (nreverse neovm--jpe-errors)))))

  (unwind-protect
      (list
        ;; Valid JSON
        (funcall 'neovm--jpe-parse "{\"a\": 1, \"b\": 2}")
        ;; Missing closing brace
        (funcall 'neovm--jpe-parse "{\"a\": 1")
        ;; Missing colon
        (funcall 'neovm--jpe-parse "{\"a\" 1}")
        ;; Trailing comma in array
        (funcall 'neovm--jpe-parse "[1, 2, 3,]")
        ;; Empty input
        (funcall 'neovm--jpe-parse "")
        ;; Valid nested
        (funcall 'neovm--jpe-parse "{\"x\": [1, 2], \"y\": {\"z\": true}}"))
    (fmakunbound 'neovm--jpe-tokenize)
    (fmakunbound 'neovm--jpe-peek)
    (fmakunbound 'neovm--jpe-advance)
    (fmakunbound 'neovm--jpe-expect)
    (fmakunbound 'neovm--jpe-skip-to-recovery)
    (fmakunbound 'neovm--jpe-parse-value)
    (fmakunbound 'neovm--jpe-parse-object)
    (fmakunbound 'neovm--jpe-parse-array)
    (fmakunbound 'neovm--jpe-parse)
    (makunbound 'neovm--jpe-tokens)
    (makunbound 'neovm--jpe-errors)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((result (object ((\"a\" (number 1)) (\"b\" (number 2)))) errors nil) (result (object ((\"a\" (number 1)))) errors (\"expected rbrace got nil\")) (result (object ((\"a\" (number 1)))) errors (\"expected colon got (number . 1)\")) (result (array ((number 1) (number 2) (number 3) (error \"recovered\"))) errors (\"unexpected token (rbracket)\")) (result (null) errors (\"unexpected end\")) (result (object ((\"x\" (array ((number 1) (number 2)))) (\"y\" (object ((\"z\" (boolean t))))))) errors nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// JSON serializer (AST -> string round-trip)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rd_adv_json_serialize_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse JSON, serialize AST back to string, verify round-trip by re-parsing.
    let form = r#"
(progn
  ;; Compact tokenizer/parser (same as before)
  (fset 'neovm--jps-tokenize
    (lambda (input)
      (let ((pos 0) (tokens nil) (len (length input)))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ((memq ch '(?\s ?\t ?\n ?\r)) (setq pos (1+ pos)))
              ((= ch ?\")
               (setq pos (1+ pos))
               (let ((chars nil) (esc nil))
                 (while (and (< pos len) (or esc (not (= (aref input pos) ?\"))))
                   (if esc (progn (setq chars (cons (aref input pos) chars) esc nil))
                     (if (= (aref input pos) ?\\) (setq esc t) (setq chars (cons (aref input pos) chars))))
                   (setq pos (1+ pos)))
                 (when (< pos len) (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'string (apply 'string (nreverse chars))) tokens))))
              ((or (and (>= ch ?0) (<= ch ?9)) (and (= ch ?-) (< (1+ pos) len) (>= (aref input (1+ pos)) ?0)))
               (let ((start pos))
                 (when (= ch ?-) (setq pos (1+ pos)))
                 (while (and (< pos len) (>= (aref input pos) ?0) (<= (aref input pos) ?9)) (setq pos (1+ pos)))
                 (when (and (< pos len) (= (aref input pos) ?.))
                   (setq pos (1+ pos))
                   (while (and (< pos len) (>= (aref input pos) ?0) (<= (aref input pos) ?9)) (setq pos (1+ pos))))
                 (setq tokens (cons (cons 'number (string-to-number (substring input start pos))) tokens))))
              ((and (<= (+ pos 4) len) (string= (substring input pos (+ pos 4)) "true"))
               (setq tokens (cons '(boolean . t) tokens)) (setq pos (+ pos 4)))
              ((and (<= (+ pos 5) len) (string= (substring input pos (+ pos 5)) "false"))
               (setq tokens (cons '(boolean . nil) tokens)) (setq pos (+ pos 5)))
              ((and (<= (+ pos 4) len) (string= (substring input pos (+ pos 4)) "null"))
               (setq tokens (cons '(null) tokens)) (setq pos (+ pos 4)))
              ((= ch ?\{) (setq tokens (cons '(lbrace) tokens)) (setq pos (1+ pos)))
              ((= ch ?\}) (setq tokens (cons '(rbrace) tokens)) (setq pos (1+ pos)))
              ((= ch ?\[) (setq tokens (cons '(lbracket) tokens)) (setq pos (1+ pos)))
              ((= ch ?\]) (setq tokens (cons '(rbracket) tokens)) (setq pos (1+ pos)))
              ((= ch ?,) (setq tokens (cons '(comma) tokens)) (setq pos (1+ pos)))
              ((= ch ?:) (setq tokens (cons '(colon) tokens)) (setq pos (1+ pos)))
              (t (setq pos (1+ pos))))))
        (nreverse tokens))))

  (defvar neovm--jps-tokens nil)
  (fset 'neovm--jps-peek (lambda () (car neovm--jps-tokens)))
  (fset 'neovm--jps-advance (lambda () (let ((t1 (car neovm--jps-tokens))) (setq neovm--jps-tokens (cdr neovm--jps-tokens)) t1)))
  (fset 'neovm--jps-expect (lambda (tp) (if (and (car neovm--jps-tokens) (eq (car (car neovm--jps-tokens)) tp)) (funcall 'neovm--jps-advance) nil)))

  (fset 'neovm--jps-parse-value
    (lambda ()
      (let ((tok (funcall 'neovm--jps-peek)))
        (cond
          ((null tok) '(null))
          ((eq (car tok) 'string) (funcall 'neovm--jps-advance) (list 'string (cdr tok)))
          ((eq (car tok) 'number) (funcall 'neovm--jps-advance) (list 'number (cdr tok)))
          ((eq (car tok) 'boolean) (funcall 'neovm--jps-advance) (list 'boolean (cdr tok)))
          ((eq (car tok) 'null) (funcall 'neovm--jps-advance) '(null))
          ((eq (car tok) 'lbrace) (funcall 'neovm--jps-parse-object))
          ((eq (car tok) 'lbracket) (funcall 'neovm--jps-parse-array))
          (t (funcall 'neovm--jps-advance) '(null))))))

  (fset 'neovm--jps-parse-object
    (lambda ()
      (funcall 'neovm--jps-advance)
      (let ((pairs nil))
        (unless (and (funcall 'neovm--jps-peek) (eq (car (funcall 'neovm--jps-peek)) 'rbrace))
          (let ((done nil))
            (while (not done)
              (let ((k (funcall 'neovm--jps-expect 'string)))
                (if k (progn (funcall 'neovm--jps-expect 'colon)
                             (setq pairs (cons (list (cdr k) (funcall 'neovm--jps-parse-value)) pairs))
                             (unless (funcall 'neovm--jps-expect 'comma) (setq done t)))
                  (setq done t))))))
        (funcall 'neovm--jps-expect 'rbrace)
        (list 'object (nreverse pairs)))))

  (fset 'neovm--jps-parse-array
    (lambda ()
      (funcall 'neovm--jps-advance)
      (let ((items nil))
        (unless (and (funcall 'neovm--jps-peek) (eq (car (funcall 'neovm--jps-peek)) 'rbracket))
          (let ((done nil))
            (while (not done)
              (setq items (cons (funcall 'neovm--jps-parse-value) items))
              (unless (funcall 'neovm--jps-expect 'comma) (setq done t)))))
        (funcall 'neovm--jps-expect 'rbracket)
        (list 'array (nreverse items)))))

  (fset 'neovm--jps-parse
    (lambda (input)
      (setq neovm--jps-tokens (funcall 'neovm--jps-tokenize input))
      (funcall 'neovm--jps-parse-value)))

  ;; Serializer: AST -> JSON string
  (fset 'neovm--jps-serialize
    (lambda (ast)
      (cond
       ((eq (car ast) 'string)
        (concat "\"" (cadr ast) "\""))
       ((eq (car ast) 'number)
        (number-to-string (cadr ast)))
       ((eq (car ast) 'boolean)
        (if (cadr ast) "true" "false"))
       ((eq (car ast) 'null) "null")
       ((eq (car ast) 'object)
        (concat "{"
                (mapconcat (lambda (pair)
                             (concat "\"" (car pair) "\":"
                                     (funcall 'neovm--jps-serialize (cadr pair))))
                           (cadr ast)
                           ",")
                "}"))
       ((eq (car ast) 'array)
        (concat "["
                (mapconcat (lambda (item)
                             (funcall 'neovm--jps-serialize item))
                           (cadr ast)
                           ",")
                "]"))
       (t "null"))))

  (unwind-protect
      (let ((inputs '("42"
                       "\"hello\""
                       "true"
                       "false"
                       "null"
                       "[1,2,3]"
                       "{\"a\":1,\"b\":2}"
                       "{\"x\":[1,true,null],\"y\":{\"z\":\"hi\"}}"
                       "[]"
                       "{}")))
        (mapcar (lambda (input)
                  (let* ((ast1 (funcall 'neovm--jps-parse input))
                         (serialized (funcall 'neovm--jps-serialize ast1))
                         (ast2 (funcall 'neovm--jps-parse serialized)))
                    (list 'input input
                          'serialized serialized
                          'round-trip-equal (equal ast1 ast2))))
                inputs))
    (fmakunbound 'neovm--jps-tokenize)
    (fmakunbound 'neovm--jps-peek)
    (fmakunbound 'neovm--jps-advance)
    (fmakunbound 'neovm--jps-expect)
    (fmakunbound 'neovm--jps-parse-value)
    (fmakunbound 'neovm--jps-parse-object)
    (fmakunbound 'neovm--jps-parse-array)
    (fmakunbound 'neovm--jps-parse)
    (fmakunbound 'neovm--jps-serialize)
    (makunbound 'neovm--jps-tokens)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((input \"42\" serialized \"42\" round-trip-equal t) (input \"\\\"hello\\\"\" serialized \"\\\"hello\\\"\" round-trip-equal t) (input \"true\" serialized \"true\" round-trip-equal t) (input \"false\" serialized \"false\" round-trip-equal t) (input \"null\" serialized \"null\" round-trip-equal t) (input \"[1,2,3]\" serialized \"[1,2,3]\" round-trip-equal t) (input \"{\\\"a\\\":1,\\\"b\\\":2}\" serialized \"{\\\"a\\\":1,\\\"b\\\":2}\" round-trip-equal t) (input \"{\\\"x\\\":[1,true,null],\\\"y\\\":{\\\"z\\\":\\\"hi\\\"}}\" serialized \"{\\\"x\\\":[1,true,null],\\\"y\\\":{\\\"z\\\":\\\"hi\\\"}}\" round-trip-equal t) (input \"[]\" serialized \"[]\" round-trip-equal t) (input \"{}\" serialized \"{}\" round-trip-equal t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
