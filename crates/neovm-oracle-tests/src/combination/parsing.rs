//! Complex oracle parity tests for parsing combinations in Elisp:
//! recursive descent expression parser, S-expression parser,
//! INI/config file parser, JSON-like parser, tokenizer+parser pipeline,
//! and template string parser with variable interpolation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Recursive descent arithmetic expression parser
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_parsing_recursive_descent_expr() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse and evaluate arithmetic expressions with +, -, *, /, parens
    // Grammar: expr = term ((+|-) term)*
    //          term = factor ((*|/) factor)*
    //          factor = NUMBER | '(' expr ')'
    let form = r#"(progn
  ;; Tokenizer
  (fset 'neovm--test-tokenize-expr
    (lambda (input)
      (let ((tokens nil) (i 0) (len (length input)))
        (while (< i len)
          (let ((ch (aref input i)))
            (cond
              ((or (= ch ?\s) (= ch ?\t))
               (setq i (1+ i)))
              ((and (>= ch ?0) (<= ch ?9))
               (let ((start i))
                 (while (and (< i len) (>= (aref input i) ?0) (<= (aref input i) ?9))
                   (setq i (1+ i)))
                 (setq tokens (cons (cons 'num (string-to-number (substring input start i)))
                                    tokens))))
              ((= ch ?\()
               (setq tokens (cons '(lparen) tokens) i (1+ i)))
              ((= ch ?\))
               (setq tokens (cons '(rparen) tokens) i (1+ i)))
              ((or (= ch ?+) (= ch ?-) (= ch ?*) (= ch ?/))
               (setq tokens (cons (cons 'op (char-to-string ch)) tokens) i (1+ i)))
              (t (setq i (1+ i))))))
        (nreverse tokens))))

  ;; Parser state: (tokens . result)
  ;; Each parse function consumes tokens and returns (remaining-tokens . parsed-value)

  (fset 'neovm--test-parse-factor
    (lambda (tokens)
      (cond
        ((eq (caar tokens) 'num)
         (cons (cdr tokens) (cdar tokens)))
        ((eq (caar tokens) 'lparen)
         (let* ((inner (funcall 'neovm--test-parse-expr (cdr tokens)))
                (rest (car inner))
                (val (cdr inner)))
           ;; consume rparen
           (cons (cdr rest) val)))
        (t (cons tokens 0)))))

  (fset 'neovm--test-parse-term
    (lambda (tokens)
      (let* ((left-result (funcall 'neovm--test-parse-factor tokens))
             (toks (car left-result))
             (val (cdr left-result)))
        (while (and toks (eq (caar toks) 'op)
                    (or (string= (cdar toks) "*") (string= (cdar toks) "/")))
          (let* ((op (cdar toks))
                 (right-result (funcall 'neovm--test-parse-factor (cdr toks)))
                 (right-val (cdr right-result)))
            (setq toks (car right-result))
            (if (string= op "*")
                (setq val (* val right-val))
              (setq val (/ val right-val)))))
        (cons toks val))))

  (fset 'neovm--test-parse-expr
    (lambda (tokens)
      (let* ((left-result (funcall 'neovm--test-parse-term tokens))
             (toks (car left-result))
             (val (cdr left-result)))
        (while (and toks (eq (caar toks) 'op)
                    (or (string= (cdar toks) "+") (string= (cdar toks) "-")))
          (let* ((op (cdar toks))
                 (right-result (funcall 'neovm--test-parse-term (cdr toks)))
                 (right-val (cdr right-result)))
            (setq toks (car right-result))
            (if (string= op "+")
                (setq val (+ val right-val))
              (setq val (- val right-val)))))
        (cons toks val))))

  (fset 'neovm--test-eval-expr
    (lambda (input)
      (let* ((tokens (funcall 'neovm--test-tokenize-expr input))
             (result (funcall 'neovm--test-parse-expr tokens)))
        (cdr result))))

  (unwind-protect
      (list
        (funcall 'neovm--test-eval-expr "3 + 4")
        (funcall 'neovm--test-eval-expr "3 + 4 * 2")
        (funcall 'neovm--test-eval-expr "(3 + 4) * 2")
        (funcall 'neovm--test-eval-expr "10 - 2 * 3 + 1")
        (funcall 'neovm--test-eval-expr "100 / 5 / 4")
        (funcall 'neovm--test-eval-expr "((2 + 3) * (4 - 1))"))
    (fmakunbound 'neovm--test-tokenize-expr)
    (fmakunbound 'neovm--test-parse-factor)
    (fmakunbound 'neovm--test-parse-term)
    (fmakunbound 'neovm--test-parse-expr)
    (fmakunbound 'neovm--test-eval-expr)))"#;
    let expect = expect_test::expect![[r#""OK (7 11 14 5 5 15)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// S-expression parser (from string to nested lists)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_parsing_sexp_from_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Hand-written S-expression parser operating on string characters
    // Supports: atoms (symbols/numbers), lists, nested lists, dotted pairs
    let form = r#"(progn
  (fset 'neovm--test-parse-sexp
    (lambda (input)
      (let ((pos 0) (len (length input)))
        ;; Skip whitespace
        (fset 'neovm--test-skip-ws
          (lambda ()
            (while (and (< pos len)
                        (memq (aref input pos) '(?\s ?\t ?\n)))
              (setq pos (1+ pos)))))

        ;; Read an atom (number or symbol name)
        (fset 'neovm--test-read-atom
          (lambda ()
            (let ((start pos))
              (while (and (< pos len)
                          (not (memq (aref input pos)
                                     '(?\s ?\t ?\n ?\( ?\) ?.))))
                (setq pos (1+ pos)))
              (let ((token (substring input start pos)))
                (if (string-match "\\`-?[0-9]+\\'" token)
                    (string-to-number token)
                  (intern token))))))

        ;; Read one S-expression
        (fset 'neovm--test-read-one
          (lambda ()
            (funcall 'neovm--test-skip-ws)
            (when (< pos len)
              (let ((ch (aref input pos)))
                (cond
                  ((= ch ?\()
                   (setq pos (1+ pos))
                   (funcall 'neovm--test-skip-ws)
                   (let ((items nil))
                     (while (and (< pos len) (/= (aref input pos) ?\)))
                       (setq items (cons (funcall 'neovm--test-read-one) items))
                       (funcall 'neovm--test-skip-ws)
                       ;; Check for dotted pair
                       (when (and (< pos len) (= (aref input pos) ?.)
                                  (< (1+ pos) len)
                                  (memq (aref input (1+ pos)) '(?\s ?\t)))
                         (setq pos (1+ pos))
                         (let ((cdr-val (funcall 'neovm--test-read-one)))
                           (funcall 'neovm--test-skip-ws)
                           ;; Build dotted list
                           (let ((result cdr-val))
                             (dolist (item items)
                               (setq result (cons item result)))
                             ;; consume closing paren
                             (when (and (< pos len) (= (aref input pos) ?\)))
                               (setq pos (1+ pos)))
                             (throw 'neovm--test-sexp-result result)))))
                     ;; Regular list
                     (when (and (< pos len) (= (aref input pos) ?\)))
                       (setq pos (1+ pos)))
                     (nreverse items)))
                  (t (funcall 'neovm--test-read-atom)))))))

        (fset 'neovm--test-parse-top
          (lambda (s)
            (setq pos 0 len (length s) input s)
            (catch 'neovm--test-sexp-result
              (funcall 'neovm--test-read-one))))

        (let ((results
               (list
                 (funcall 'neovm--test-parse-top "42")
                 (funcall 'neovm--test-parse-top "hello")
                 (funcall 'neovm--test-parse-top "(1 2 3)")
                 (funcall 'neovm--test-parse-top "(a (b c) d)")
                 (funcall 'neovm--test-parse-top "(x . y)")
                 (funcall 'neovm--test-parse-top "(1 2 . 3)")
                 (funcall 'neovm--test-parse-top "(+ (* 2 3) (- 5 1))"))))
          (fmakunbound 'neovm--test-skip-ws)
          (fmakunbound 'neovm--test-read-atom)
          (fmakunbound 'neovm--test-read-one)
          (fmakunbound 'neovm--test-parse-sexp)
          (fmakunbound 'neovm--test-parse-top)
          results)))))"#;
    let expect = expect_test::expect![[
        r#""OK (closure (t) (input) (let ((pos 0) (len (length input))) (fset 'neovm--test-skip-ws (lambda nil (while (and (< pos len) (memq (aref input pos) '(32 9 10))) (setq pos (1+ pos))))) (fset 'neovm--test-read-atom (lambda nil (let ((start pos)) (while (and (< pos len) (not (memq (aref input pos) '(32 9 10 40 41 46)))) (setq pos (1+ pos))) (let ((token (substring input start pos))) (if (string-match \"\\\\`-?[0-9]+\\\\'\" token) (string-to-number token) (intern token)))))) (fset 'neovm--test-read-one (lambda nil (funcall 'neovm--test-skip-ws) (when (< pos len) (let ((ch (aref input pos))) (cond ((= ch 40) (setq pos (1+ pos)) (funcall 'neovm--test-skip-ws) (let ((items nil)) (while (and (< pos len) (/= (aref input pos) 41)) (setq items (cons (funcall 'neovm--test-read-one) items)) (funcall 'neovm--test-skip-ws) (when (and (< pos len) (= (aref input pos) 46) (< (1+ pos) len) (memq (aref input (1+ pos)) '(32 9))) (setq pos (1+ pos)) (let ((cdr-val (funcall 'neovm--test-read-one))) (funcall 'neovm--test-skip-ws) (let ((result cdr-val)) (dolist (item items) (setq result (cons item result))) (when (and (< pos len) (= (aref input pos) 41)) (setq pos (1+ pos))) (throw 'neovm--test-sexp-result result))))) (when (and (< pos len) (= (aref input pos) 41)) (setq pos (1+ pos))) (nreverse items))) (t (funcall 'neovm--test-read-atom))))))) (fset 'neovm--test-parse-top (lambda (s) (setq pos 0 len (length s) input s) (catch 'neovm--test-sexp-result (funcall 'neovm--test-read-one)))) (let ((results (list (funcall 'neovm--test-parse-top \"42\") (funcall 'neovm--test-parse-top \"hello\") (funcall 'neovm--test-parse-top \"(1 2 3)\") (funcall 'neovm--test-parse-top \"(a (b c) d)\") (funcall 'neovm--test-parse-top \"(x . y)\") (funcall 'neovm--test-parse-top \"(1 2 . 3)\") (funcall 'neovm--test-parse-top \"(+ (* 2 3) (- 5 1))\")))) (fmakunbound 'neovm--test-skip-ws) (fmakunbound 'neovm--test-read-atom) (fmakunbound 'neovm--test-read-one) (fmakunbound 'neovm--test-parse-sexp) (fmakunbound 'neovm--test-parse-top) results)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// INI/config file parser with sections
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_parsing_ini_config() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse INI format: [section]\nkey=value\n...
    // Returns alist of (section . ((key . value) ...))
    let form = r#"(progn
  (fset 'neovm--test-parse-ini
    (lambda (input)
      (let ((sections nil)
            (current-section nil)
            (current-pairs nil)
            (lines nil)
            (i 0)
            (len (length input))
            (line-start 0))
        ;; Split into lines
        (while (<= i len)
          (when (or (= i len) (= (aref input i) ?\n))
            (let ((line (substring input line-start i)))
              (setq lines (cons line lines)
                    line-start (1+ i))))
          (setq i (1+ i)))
        (setq lines (nreverse lines))
        ;; Process lines
        (while lines
          (let ((line (car lines)))
            (cond
              ;; Empty or comment line
              ((or (= (length line) 0)
                   (and (> (length line) 0) (= (aref line 0) ?#)))
               nil)
              ;; Section header [name]
              ((and (> (length line) 2)
                    (= (aref line 0) ?\[)
                    (= (aref line (1- (length line))) ?\]))
               ;; Save previous section
               (when current-section
                 (setq sections
                       (cons (cons current-section (nreverse current-pairs))
                             sections)))
               (setq current-section
                     (substring line 1 (1- (length line)))
                     current-pairs nil))
              ;; Key=value pair
              (t
               (let ((eq-pos (string-match "=" line)))
                 (when eq-pos
                   (let ((key (substring line 0 eq-pos))
                         (val (substring line (1+ eq-pos))))
                     (setq current-pairs
                           (cons (cons key val) current-pairs))))))))
          (setq lines (cdr lines)))
        ;; Save last section
        (when current-section
          (setq sections
                (cons (cons current-section (nreverse current-pairs))
                      sections)))
        (nreverse sections))))

  (unwind-protect
      (let ((config (funcall 'neovm--test-parse-ini
                      "[database]\nhost=localhost\nport=5432\nname=mydb\n\n[server]\nhost=0.0.0.0\nport=8080\nworkers=4\n\n[logging]\nlevel=info\nfile=/var/log/app.log\n")))
        (list
          ;; Number of sections
          (length config)
          ;; Database section
          (cdr (assoc "host" (cdr (assoc "database" config))))
          (cdr (assoc "port" (cdr (assoc "database" config))))
          ;; Server section
          (cdr (assoc "port" (cdr (assoc "server" config))))
          (cdr (assoc "workers" (cdr (assoc "server" config))))
          ;; Logging section
          (cdr (assoc "level" (cdr (assoc "logging" config))))
          ;; All section names
          (mapcar 'car config)))
    (fmakunbound 'neovm--test-parse-ini)))"#;
    let expect = expect_test::expect![[
        r#""OK (3 \"localhost\" \"5432\" \"8080\" \"4\" \"info\" (\"database\" \"server\" \"logging\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// JSON-like structure parser (simplified)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_parsing_json_like() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse simplified JSON: objects -> alists, arrays -> lists,
    // strings, numbers, true/false/null
    let form = r#"(progn
  (defvar neovm--test-json-pos 0)
  (defvar neovm--test-json-input "")

  (fset 'neovm--test-json-peek
    (lambda ()
      (when (< neovm--test-json-pos (length neovm--test-json-input))
        (aref neovm--test-json-input neovm--test-json-pos))))

  (fset 'neovm--test-json-advance
    (lambda ()
      (setq neovm--test-json-pos (1+ neovm--test-json-pos))))

  (fset 'neovm--test-json-skip-ws
    (lambda ()
      (while (and (< neovm--test-json-pos (length neovm--test-json-input))
                  (memq (funcall 'neovm--test-json-peek) '(?\s ?\t ?\n ?\r)))
        (funcall 'neovm--test-json-advance))))

  (fset 'neovm--test-json-parse-string
    (lambda ()
      (funcall 'neovm--test-json-advance) ;; skip opening quote
      (let ((start neovm--test-json-pos))
        (while (and (< neovm--test-json-pos (length neovm--test-json-input))
                    (/= (aref neovm--test-json-input neovm--test-json-pos) ?\"))
          (setq neovm--test-json-pos (1+ neovm--test-json-pos)))
        (let ((result (substring neovm--test-json-input start neovm--test-json-pos)))
          (funcall 'neovm--test-json-advance) ;; skip closing quote
          result))))

  (fset 'neovm--test-json-parse-number
    (lambda ()
      (let ((start neovm--test-json-pos))
        (when (and (funcall 'neovm--test-json-peek) (= (funcall 'neovm--test-json-peek) ?-))
          (funcall 'neovm--test-json-advance))
        (while (and (< neovm--test-json-pos (length neovm--test-json-input))
                    (>= (aref neovm--test-json-input neovm--test-json-pos) ?0)
                    (<= (aref neovm--test-json-input neovm--test-json-pos) ?9))
          (setq neovm--test-json-pos (1+ neovm--test-json-pos)))
        (string-to-number (substring neovm--test-json-input start neovm--test-json-pos)))))

  (fset 'neovm--test-json-parse-value
    (lambda ()
      (funcall 'neovm--test-json-skip-ws)
      (let ((ch (funcall 'neovm--test-json-peek)))
        (cond
          ((= ch ?\")
           (funcall 'neovm--test-json-parse-string))
          ((or (and (>= ch ?0) (<= ch ?9)) (= ch ?-))
           (funcall 'neovm--test-json-parse-number))
          ((= ch ?\{)
           (funcall 'neovm--test-json-parse-object))
          ((= ch ?\[)
           (funcall 'neovm--test-json-parse-array))
          ((= ch ?t)
           (setq neovm--test-json-pos (+ neovm--test-json-pos 4)) t)
          ((= ch ?f)
           (setq neovm--test-json-pos (+ neovm--test-json-pos 5)) nil)
          ((= ch ?n)
           (setq neovm--test-json-pos (+ neovm--test-json-pos 4)) nil)
          (t nil)))))

  (fset 'neovm--test-json-parse-array
    (lambda ()
      (funcall 'neovm--test-json-advance) ;; skip [
      (funcall 'neovm--test-json-skip-ws)
      (if (and (funcall 'neovm--test-json-peek)
               (= (funcall 'neovm--test-json-peek) ?\]))
          (progn (funcall 'neovm--test-json-advance) nil)
        (let ((items nil))
          (setq items (cons (funcall 'neovm--test-json-parse-value) items))
          (funcall 'neovm--test-json-skip-ws)
          (while (and (funcall 'neovm--test-json-peek)
                      (= (funcall 'neovm--test-json-peek) ?,))
            (funcall 'neovm--test-json-advance)
            (setq items (cons (funcall 'neovm--test-json-parse-value) items))
            (funcall 'neovm--test-json-skip-ws))
          (funcall 'neovm--test-json-advance) ;; skip ]
          (nreverse items)))))

  (fset 'neovm--test-json-parse-object
    (lambda ()
      (funcall 'neovm--test-json-advance) ;; skip {
      (funcall 'neovm--test-json-skip-ws)
      (if (and (funcall 'neovm--test-json-peek)
               (= (funcall 'neovm--test-json-peek) ?\}))
          (progn (funcall 'neovm--test-json-advance) nil)
        (let ((pairs nil))
          (funcall 'neovm--test-json-skip-ws)
          (let ((key (funcall 'neovm--test-json-parse-string)))
            (funcall 'neovm--test-json-skip-ws)
            (funcall 'neovm--test-json-advance) ;; skip :
            (let ((val (funcall 'neovm--test-json-parse-value)))
              (setq pairs (cons (cons key val) pairs))))
          (funcall 'neovm--test-json-skip-ws)
          (while (and (funcall 'neovm--test-json-peek)
                      (= (funcall 'neovm--test-json-peek) ?,))
            (funcall 'neovm--test-json-advance)
            (funcall 'neovm--test-json-skip-ws)
            (let ((key (funcall 'neovm--test-json-parse-string)))
              (funcall 'neovm--test-json-skip-ws)
              (funcall 'neovm--test-json-advance) ;; skip :
              (let ((val (funcall 'neovm--test-json-parse-value)))
                (setq pairs (cons (cons key val) pairs))))
            (funcall 'neovm--test-json-skip-ws))
          (funcall 'neovm--test-json-advance) ;; skip }
          (nreverse pairs)))))

  (fset 'neovm--test-json-parse
    (lambda (input)
      (setq neovm--test-json-pos 0
            neovm--test-json-input input)
      (funcall 'neovm--test-json-parse-value)))

  (unwind-protect
      (list
        ;; Simple object
        (funcall 'neovm--test-json-parse "{\"name\": \"alice\", \"age\": 30}")
        ;; Array
        (funcall 'neovm--test-json-parse "[1, 2, 3]")
        ;; Nested
        (funcall 'neovm--test-json-parse "{\"users\": [{\"id\": 1}, {\"id\": 2}]}")
        ;; Booleans and null
        (funcall 'neovm--test-json-parse "{\"active\": true, \"deleted\": false, \"note\": null}")
        ;; Empty structures
        (funcall 'neovm--test-json-parse "{}")
        (funcall 'neovm--test-json-parse "[]"))
    (fmakunbound 'neovm--test-json-peek)
    (fmakunbound 'neovm--test-json-advance)
    (fmakunbound 'neovm--test-json-skip-ws)
    (fmakunbound 'neovm--test-json-parse-string)
    (fmakunbound 'neovm--test-json-parse-number)
    (fmakunbound 'neovm--test-json-parse-value)
    (fmakunbound 'neovm--test-json-parse-array)
    (fmakunbound 'neovm--test-json-parse-object)
    (fmakunbound 'neovm--test-json-parse)
    (makunbound 'neovm--test-json-pos)
    (makunbound 'neovm--test-json-input)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"name\" . \"alice\") (\"age\" . 30)) (1 2 3) ((\"users\" ((\"id\" . 1)) ((\"id\" . 2)))) ((\"active\" . t) (\"deleted\") (\"note\")) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Tokenizer + parser pipeline (two-phase)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_parsing_tokenizer_parser_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Phase 1: tokenize a mini-language (let/set/print statements)
    // Phase 2: parse tokens into AST
    // Phase 3: interpret AST
    let form = r#"(progn
  ;; Tokenizer: input string -> list of (type . value) tokens
  (fset 'neovm--test-lang-tokenize
    (lambda (input)
      (let ((tokens nil) (i 0) (len (length input)))
        (while (< i len)
          (let ((ch (aref input i)))
            (cond
              ;; Whitespace
              ((memq ch '(?\s ?\t ?\n))
               (setq i (1+ i)))
              ;; Semicolon (statement separator)
              ((= ch ?\;)
               (setq tokens (cons '(semi) tokens) i (1+ i)))
              ;; Equals sign
              ((= ch ?=)
               (setq tokens (cons '(eq) tokens) i (1+ i)))
              ;; Plus/minus
              ((= ch ?+)
               (setq tokens (cons '(plus) tokens) i (1+ i)))
              ((= ch ?-)
               (setq tokens (cons '(minus) tokens) i (1+ i)))
              ((= ch ?*)
               (setq tokens (cons '(star) tokens) i (1+ i)))
              ;; Numbers
              ((and (>= ch ?0) (<= ch ?9))
               (let ((start i))
                 (while (and (< i len)
                             (>= (aref input i) ?0) (<= (aref input i) ?9))
                   (setq i (1+ i)))
                 (setq tokens (cons (cons 'num (string-to-number
                                                 (substring input start i)))
                                    tokens))))
              ;; Identifiers / keywords
              ((or (and (>= ch ?a) (<= ch ?z))
                   (and (>= ch ?A) (<= ch ?Z)))
               (let ((start i))
                 (while (and (< i len)
                             (let ((c (aref input i)))
                               (or (and (>= c ?a) (<= c ?z))
                                   (and (>= c ?A) (<= c ?Z))
                                   (and (>= c ?0) (<= c ?9)))))
                   (setq i (1+ i)))
                 (let ((word (substring input start i)))
                   (setq tokens (cons (cond
                                        ((string= word "let") '(kw-let))
                                        ((string= word "print") '(kw-print))
                                        (t (cons 'ident word)))
                                      tokens)))))
              (t (setq i (1+ i))))))
        (nreverse tokens))))

  ;; Parser: tokens -> list of statements
  ;; Statement = (let name expr) | (print expr)
  ;; Expr = term ((+|-) term)*
  ;; Term = factor ((*) factor)*
  ;; Factor = num | ident
  (fset 'neovm--test-lang-parse
    (lambda (tokens)
      (let ((stmts nil) (toks tokens))
        ;; Parse factor
        (fset 'neovm--test-lp-factor
          (lambda ()
            (let ((tok (car toks)))
              (cond
                ((eq (car tok) 'num)
                 (setq toks (cdr toks))
                 (list 'lit (cdr tok)))
                ((eq (car tok) 'ident)
                 (setq toks (cdr toks))
                 (list 'var (cdr tok)))
                (t (list 'lit 0))))))
        ;; Parse term
        (fset 'neovm--test-lp-term
          (lambda ()
            (let ((left (funcall 'neovm--test-lp-factor)))
              (while (and toks (eq (caar toks) 'star))
                (setq toks (cdr toks))
                (let ((right (funcall 'neovm--test-lp-factor)))
                  (setq left (list '* left right))))
              left)))
        ;; Parse expr
        (fset 'neovm--test-lp-expr
          (lambda ()
            (let ((left (funcall 'neovm--test-lp-term)))
              (while (and toks (memq (caar toks) '(plus minus)))
                (let ((op (if (eq (caar toks) 'plus) '+ '-)))
                  (setq toks (cdr toks))
                  (let ((right (funcall 'neovm--test-lp-term)))
                    (setq left (list op left right)))))
              left)))
        ;; Parse statements
        (while toks
          (cond
            ((eq (caar toks) 'kw-let)
             (setq toks (cdr toks))
             (let ((name (cdar toks)))
               (setq toks (cdr toks)) ;; ident
               (setq toks (cdr toks)) ;; =
               (let ((expr (funcall 'neovm--test-lp-expr)))
                 (setq stmts (cons (list 'let-stmt name expr) stmts)))))
            ((eq (caar toks) 'kw-print)
             (setq toks (cdr toks))
             (let ((expr (funcall 'neovm--test-lp-expr)))
               (setq stmts (cons (list 'print-stmt expr) stmts))))
            (t (setq toks (cdr toks))))
          ;; Skip optional semicolons
          (while (and toks (eq (caar toks) 'semi))
            (setq toks (cdr toks))))
        (let ((result (nreverse stmts)))
          (fmakunbound 'neovm--test-lp-factor)
          (fmakunbound 'neovm--test-lp-term)
          (fmakunbound 'neovm--test-lp-expr)
          result))))

  ;; Interpreter: execute AST, return output list
  (fset 'neovm--test-lang-eval-expr
    (lambda (expr env)
      (cond
        ((eq (car expr) 'lit) (cadr expr))
        ((eq (car expr) 'var)
         (let ((binding (assoc (cadr expr) env)))
           (if binding (cdr binding) 0)))
        ((eq (car expr) '+)
         (+ (funcall 'neovm--test-lang-eval-expr (cadr expr) env)
            (funcall 'neovm--test-lang-eval-expr (caddr expr) env)))
        ((eq (car expr) '-)
         (- (funcall 'neovm--test-lang-eval-expr (cadr expr) env)
            (funcall 'neovm--test-lang-eval-expr (caddr expr) env)))
        ((eq (car expr) '*)
         (* (funcall 'neovm--test-lang-eval-expr (cadr expr) env)
            (funcall 'neovm--test-lang-eval-expr (caddr expr) env)))
        (t 0))))

  (fset 'neovm--test-lang-run
    (lambda (program)
      (let* ((tokens (funcall 'neovm--test-lang-tokenize program))
             (ast (funcall 'neovm--test-lang-parse tokens))
             (env nil)
             (output nil))
        (dolist (stmt ast)
          (cond
            ((eq (car stmt) 'let-stmt)
             (let ((name (cadr stmt))
                   (val (funcall 'neovm--test-lang-eval-expr (caddr stmt) env)))
               (setq env (cons (cons name val) env))))
            ((eq (car stmt) 'print-stmt)
             (setq output
                   (cons (funcall 'neovm--test-lang-eval-expr (cadr stmt) env)
                         output)))))
        (nreverse output))))

  (unwind-protect
      (list
        (funcall 'neovm--test-lang-run "let x = 10; let y = 20; print x + y")
        (funcall 'neovm--test-lang-run "let a = 3; let b = a * 4; print b + 1; print a")
        (funcall 'neovm--test-lang-run "let x = 5; let y = x * 2 + 3; print y - x"))
    (fmakunbound 'neovm--test-lang-tokenize)
    (fmakunbound 'neovm--test-lang-parse)
    (fmakunbound 'neovm--test-lang-eval-expr)
    (fmakunbound 'neovm--test-lang-run)))"#;
    let expect = expect_test::expect![[r#""OK ((30) (13 3) (8))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Template string parser with variable interpolation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_parsing_template_interpolation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse "Hello ${name}, you have ${count} items" into segments
    // Then interpolate with an environment alist
    let form = r#"(progn
  ;; Parse template into segments: (literal "text") or (var "name")
  (fset 'neovm--test-tpl-parse
    (lambda (template)
      (let ((segments nil)
            (i 0)
            (len (length template))
            (text-start 0))
        (while (< i len)
          (if (and (= (aref template i) ?$)
                   (< (1+ i) len)
                   (= (aref template (1+ i)) ?\{))
              (progn
                ;; Emit preceding literal if any
                (when (> i text-start)
                  (setq segments (cons (list 'literal (substring template text-start i))
                                       segments)))
                ;; Find closing brace
                (let ((var-start (+ i 2))
                      (j (+ i 2)))
                  (while (and (< j len) (/= (aref template j) ?\}))
                    (setq j (1+ j)))
                  (setq segments (cons (list 'var (substring template var-start j))
                                       segments))
                  (setq i (1+ j)
                        text-start (1+ j))))
            (setq i (1+ i))))
        ;; Trailing literal
        (when (> i text-start)
          (setq segments (cons (list 'literal (substring template text-start i))
                               segments)))
        (nreverse segments))))

  ;; Interpolate: given parsed segments and env alist, produce string
  (fset 'neovm--test-tpl-render
    (lambda (segments env)
      (let ((parts nil))
        (dolist (seg segments)
          (cond
            ((eq (car seg) 'literal)
             (setq parts (cons (cadr seg) parts)))
            ((eq (car seg) 'var)
             (let ((binding (assoc (cadr seg) env)))
               (setq parts (cons (if binding
                                     (format "%s" (cdr binding))
                                   (concat "${" (cadr seg) "}"))
                                 parts))))))
        (apply 'concat (nreverse parts)))))

  ;; Full pipeline
  (fset 'neovm--test-tpl-expand
    (lambda (template env)
      (funcall 'neovm--test-tpl-render
               (funcall 'neovm--test-tpl-parse template)
               env)))

  (unwind-protect
      (list
        ;; Basic interpolation
        (funcall 'neovm--test-tpl-expand
                 "Hello ${name}, welcome!"
                 '(("name" . "Alice")))
        ;; Multiple variables
        (funcall 'neovm--test-tpl-expand
                 "${greeting} ${name}, you have ${count} new messages."
                 '(("greeting" . "Hi") ("name" . "Bob") ("count" . 5)))
        ;; Missing variable: kept as-is
        (funcall 'neovm--test-tpl-expand
                 "User: ${user}, Role: ${role}"
                 '(("user" . "carol")))
        ;; No variables
        (funcall 'neovm--test-tpl-expand
                 "no interpolation here"
                 nil)
        ;; Adjacent variables
        (funcall 'neovm--test-tpl-expand
                 "${a}${b}${c}"
                 '(("a" . "X") ("b" . "Y") ("c" . "Z")))
        ;; Parsed segments structure
        (funcall 'neovm--test-tpl-parse "before ${x} middle ${y} after"))
    (fmakunbound 'neovm--test-tpl-parse)
    (fmakunbound 'neovm--test-tpl-render)
    (fmakunbound 'neovm--test-tpl-expand)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello Alice, welcome!\" \"Hi Bob, you have 5 new messages.\" \"User: carol, Role: ${role}\" \"no interpolation here\" \"XYZ\" ((literal \"before \") (var \"x\") (literal \" middle \") (var \"y\") (literal \" after\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Balanced delimiter validator with error reporting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_parsing_balanced_delimiters() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Validate balanced (), [], {} with position tracking for errors
    let form = r#"(progn
  (fset 'neovm--test-check-balanced
    (lambda (input)
      (let ((stack nil)
            (i 0)
            (len (length input))
            (error-info nil))
        (catch 'neovm--test-balance-err
          (while (< i len)
            (let ((ch (aref input i)))
              (cond
                ((memq ch '(?\( ?\[ ?\{))
                 (setq stack (cons (cons ch i) stack)))
                ((memq ch '(?\) ?\] ?\}))
                 (let ((expected (cond
                                   ((= ch ?\)) ?\()
                                   ((= ch ?\]) ?\[)
                                   ((= ch ?\}) ?\{))))
                   (if (null stack)
                       (throw 'neovm--test-balance-err
                              (list 'unmatched-close
                                    (char-to-string ch) i))
                     (if (/= (caar stack) expected)
                         (throw 'neovm--test-balance-err
                                (list 'mismatch
                                      (char-to-string (caar stack))
                                      (cdar stack)
                                      (char-to-string ch)
                                      i))
                       (setq stack (cdr stack))))))))
            (setq i (1+ i)))
          (if stack
              (list 'unclosed
                    (char-to-string (caar stack))
                    (cdar stack))
            'balanced)))))

  (unwind-protect
      (list
        (funcall 'neovm--test-check-balanced "(())")
        (funcall 'neovm--test-check-balanced "([{}])")
        (funcall 'neovm--test-check-balanced "(()")
        (funcall 'neovm--test-check-balanced "([)]")
        (funcall 'neovm--test-check-balanced ")")
        (funcall 'neovm--test-check-balanced "{[()]}([])")
        (funcall 'neovm--test-check-balanced "hello (world) [foo] {bar}")
        (funcall 'neovm--test-check-balanced "((({"))
    (fmakunbound 'neovm--test-check-balanced)))"#;
    let expect = expect_test::expect![[
        r#""OK (balanced balanced (unclosed \"(\" 0) (mismatch \"[\" 1 \")\" 2) (unmatched-close \")\" 0) balanced balanced (unclosed \"{\" 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
