//! Oracle parity tests for parser combinator patterns in Elisp.
//!
//! Implements basic parsers (literal, char-class, any-char), combinators
//! (sequence, alternative, many, optional, map), recursive parsers
//! (balanced parentheses, nested expressions), arithmetic expression
//! parsing, and S-expression parsing -- all using a combinator framework.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Basic parsers: literal string, character class, any-char
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_parser_comb_basic_parsers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parser = function that takes (input pos) and returns (value . new-pos) or nil on failure.
    // Implement literal, char-class, and any-char parsers.
    let form = r#"(progn
  ;; literal: matches exact string at position
  (fset 'neovm--pc-literal
    (lambda (expected)
      "Return a parser that matches EXPECTED string literally."
      (lambda (input pos)
        (let ((end (+ pos (length expected))))
          (when (and (<= end (length input))
                     (string= (substring input pos end) expected))
            (cons expected end))))))

  ;; char-class: matches any single char satisfying predicate
  (fset 'neovm--pc-char-class
    (lambda (pred-fn)
      "Return a parser that matches one char satisfying PRED-FN."
      (lambda (input pos)
        (when (< pos (length input))
          (let ((ch (aref input pos)))
            (when (funcall pred-fn ch)
              (cons (char-to-string ch) (1+ pos))))))))

  ;; any-char: matches any single character
  (fset 'neovm--pc-any-char
    (lambda ()
      "Return a parser that matches any single character."
      (lambda (input pos)
        (when (< pos (length input))
          (cons (char-to-string (aref input pos)) (1+ pos))))))

  ;; run parser
  (fset 'neovm--pc-run
    (lambda (parser input)
      "Run PARSER on INPUT from position 0."
      (funcall parser input 0)))

  (unwind-protect
      (let ((p-hello (funcall 'neovm--pc-literal "hello"))
            (p-digit (funcall 'neovm--pc-char-class
                       (lambda (ch) (and (>= ch ?0) (<= ch ?9)))))
            (p-alpha (funcall 'neovm--pc-char-class
                       (lambda (ch) (or (and (>= ch ?a) (<= ch ?z))
                                        (and (>= ch ?A) (<= ch ?Z))))))
            (p-any (funcall 'neovm--pc-any-char)))
        (list
          ;; Literal parser
          (funcall 'neovm--pc-run p-hello "hello world")
          (funcall 'neovm--pc-run p-hello "hell world")
          (funcall 'neovm--pc-run p-hello "HELLO world")
          ;; Digit parser
          (funcall 'neovm--pc-run p-digit "3abc")
          (funcall 'neovm--pc-run p-digit "abc3")
          ;; Alpha parser
          (funcall 'neovm--pc-run p-alpha "x42")
          (funcall 'neovm--pc-run p-alpha "42x")
          ;; Any-char
          (funcall 'neovm--pc-run p-any "abc")
          (funcall 'neovm--pc-run p-any "")
          ;; Literal at specific positions
          (funcall p-hello "say hello" 4)
          ;; Chained manual: digit then alpha
          (let* ((r1 (funcall p-digit "7z" 0))
                 (r2 (when r1 (funcall p-alpha "7z" (cdr r1)))))
            (list r1 r2))))
    (fmakunbound 'neovm--pc-literal)
    (fmakunbound 'neovm--pc-char-class)
    (fmakunbound 'neovm--pc-any-char)
    (fmakunbound 'neovm--pc-run)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"hello\" . 5) nil nil (\"3\" . 1) nil (\"x\" . 1) nil (\"a\" . 1) nil (\"hello\" . 9) ((\"7\" . 1) (\"z\" . 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combinators: sequence (and-then), alternative (or-else), many, optional
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_parser_comb_combinators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build combinators that compose parsers into more complex parsers.
    let form = r#"(progn
  ;; Primitive parsers
  (fset 'neovm--pc2-literal
    (lambda (expected)
      (lambda (input pos)
        (let ((end (+ pos (length expected))))
          (when (and (<= end (length input))
                     (string= (substring input pos end) expected))
            (cons expected end))))))

  (fset 'neovm--pc2-char-class
    (lambda (pred-fn)
      (lambda (input pos)
        (when (< pos (length input))
          (let ((ch (aref input pos)))
            (when (funcall pred-fn ch)
              (cons (char-to-string ch) (1+ pos))))))))

  ;; and-then: run p1, if success run p2, return pair of results
  (fset 'neovm--pc2-and-then
    (lambda (p1 p2)
      (lambda (input pos)
        (let ((r1 (funcall p1 input pos)))
          (when r1
            (let ((r2 (funcall p2 input (cdr r1))))
              (when r2
                (cons (list (car r1) (car r2)) (cdr r2)))))))))

  ;; or-else: try p1, if fail try p2
  (fset 'neovm--pc2-or-else
    (lambda (p1 p2)
      (lambda (input pos)
        (or (funcall p1 input pos)
            (funcall p2 input pos)))))

  ;; many: apply parser zero or more times, collect results
  (fset 'neovm--pc2-many
    (lambda (p)
      (lambda (input pos)
        (let ((results nil)
              (current-pos pos)
              (done nil))
          (while (not done)
            (let ((r (funcall p input current-pos)))
              (if r
                  (progn
                    (setq results (cons (car r) results))
                    (setq current-pos (cdr r)))
                (setq done t))))
          (cons (nreverse results) current-pos)))))

  ;; optional: try parser, return nil-value if fails
  (fset 'neovm--pc2-optional
    (lambda (p)
      (lambda (input pos)
        (or (funcall p input pos)
            (cons nil pos)))))

  ;; run
  (fset 'neovm--pc2-run
    (lambda (parser input)
      (funcall parser input 0)))

  (unwind-protect
      (let ((p-a (funcall 'neovm--pc2-literal "a"))
            (p-b (funcall 'neovm--pc2-literal "b"))
            (p-digit (funcall 'neovm--pc2-char-class
                       (lambda (ch) (and (>= ch ?0) (<= ch ?9))))))
        (let ((p-ab (funcall 'neovm--pc2-and-then p-a p-b))
              (p-a-or-b (funcall 'neovm--pc2-or-else p-a p-b))
              (p-many-a (funcall 'neovm--pc2-many p-a))
              (p-many-digit (funcall 'neovm--pc2-many p-digit))
              (p-opt-b (funcall 'neovm--pc2-optional p-b)))
          (list
            ;; and-then
            (funcall 'neovm--pc2-run p-ab "abc")
            (funcall 'neovm--pc2-run p-ab "axc")
            (funcall 'neovm--pc2-run p-ab "bac")
            ;; or-else
            (funcall 'neovm--pc2-run p-a-or-b "abc")
            (funcall 'neovm--pc2-run p-a-or-b "bac")
            (funcall 'neovm--pc2-run p-a-or-b "cab")
            ;; many
            (funcall 'neovm--pc2-run p-many-a "aaab")
            (funcall 'neovm--pc2-run p-many-a "bbb")
            (funcall 'neovm--pc2-run p-many-a "")
            ;; many digits
            (funcall 'neovm--pc2-run p-many-digit "12345abc")
            (funcall 'neovm--pc2-run p-many-digit "abc")
            ;; optional
            (funcall 'neovm--pc2-run p-opt-b "bx")
            (funcall 'neovm--pc2-run p-opt-b "ax")
            ;; Composed: many (a or b)
            (let ((p-many-ab (funcall 'neovm--pc2-many p-a-or-b)))
              (funcall 'neovm--pc2-run p-many-ab "aababbc")))))
    (fmakunbound 'neovm--pc2-literal)
    (fmakunbound 'neovm--pc2-char-class)
    (fmakunbound 'neovm--pc2-and-then)
    (fmakunbound 'neovm--pc2-or-else)
    (fmakunbound 'neovm--pc2-many)
    (fmakunbound 'neovm--pc2-optional)
    (fmakunbound 'neovm--pc2-run)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"a\" \"b\") . 2) nil nil (\"a\" . 1) (\"b\" . 1) nil ((\"a\" \"a\" \"a\") . 3) (nil . 0) (nil . 0) ((\"1\" \"2\" \"3\" \"4\" \"5\") . 5) (nil . 0) (\"b\" . 1) (nil . 0) ((\"a\" \"a\" \"b\" \"a\" \"b\" \"b\") . 6))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Map combinator (transform parsed value)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_parser_comb_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // map: transform the result of a successful parse without changing position.
    // Compose with other combinators to build number/identifier parsers.
    let form = r#"(progn
  (fset 'neovm--pc3-char-class
    (lambda (pred-fn)
      (lambda (input pos)
        (when (< pos (length input))
          (let ((ch (aref input pos)))
            (when (funcall pred-fn ch)
              (cons (char-to-string ch) (1+ pos))))))))

  (fset 'neovm--pc3-many1
    (lambda (p)
      "One or more: like many but requires at least one match."
      (lambda (input pos)
        (let ((first (funcall p input pos)))
          (when first
            (let ((results (list (car first)))
                  (current-pos (cdr first))
                  (done nil))
              (while (not done)
                (let ((r (funcall p input current-pos)))
                  (if r
                      (progn
                        (setq results (cons (car r) results))
                        (setq current-pos (cdr r)))
                    (setq done t))))
              (cons (nreverse results) current-pos)))))))

  (fset 'neovm--pc3-map
    (lambda (p transform-fn)
      "Apply TRANSFORM-FN to the result of parser P."
      (lambda (input pos)
        (let ((r (funcall p input pos)))
          (when r
            (cons (funcall transform-fn (car r)) (cdr r)))))))

  (fset 'neovm--pc3-and-then
    (lambda (p1 p2)
      (lambda (input pos)
        (let ((r1 (funcall p1 input pos)))
          (when r1
            (let ((r2 (funcall p2 input (cdr r1))))
              (when r2
                (cons (list (car r1) (car r2)) (cdr r2)))))))))

  (fset 'neovm--pc3-or-else
    (lambda (p1 p2)
      (lambda (input pos)
        (or (funcall p1 input pos)
            (funcall p2 input pos)))))

  (fset 'neovm--pc3-run
    (lambda (parser input)
      (funcall parser input 0)))

  (unwind-protect
      (let* ((p-digit (funcall 'neovm--pc3-char-class
                        (lambda (ch) (and (>= ch ?0) (<= ch ?9)))))
             (p-alpha (funcall 'neovm--pc3-char-class
                        (lambda (ch) (or (and (>= ch ?a) (<= ch ?z))
                                         (and (>= ch ?A) (<= ch ?Z))))))
             ;; Number parser: many1 digits -> join -> string-to-number
             (p-number (funcall 'neovm--pc3-map
                         (funcall 'neovm--pc3-many1 p-digit)
                         (lambda (chars) (string-to-number (apply 'concat chars)))))
             ;; Identifier parser: alpha then many (alpha or digit) -> join to string
             (p-alnum (funcall 'neovm--pc3-or-else p-alpha p-digit))
             (p-ident (funcall 'neovm--pc3-map
                        (funcall 'neovm--pc3-and-then
                          p-alpha
                          (funcall 'neovm--pc3-map
                            (lambda (input pos)
                              (let ((results nil) (cp pos) (done nil))
                                (while (not done)
                                  (let ((r (funcall p-alnum input cp)))
                                    (if r (progn (setq results (cons (car r) results)
                                                       cp (cdr r)))
                                      (setq done t))))
                                (cons (nreverse results) cp)))
                            (lambda (chars) (apply 'concat chars))))
                        (lambda (parts)
                          (concat (car parts) (cadr parts)))))
             ;; Doubled number: parse number, multiply by 2
             (p-doubled (funcall 'neovm--pc3-map p-number
                          (lambda (n) (* n 2)))))
        (list
          ;; Number parser
          (funcall 'neovm--pc3-run p-number "42abc")
          (funcall 'neovm--pc3-run p-number "0")
          (funcall 'neovm--pc3-run p-number "abc")
          (funcall 'neovm--pc3-run p-number "12345xyz")
          ;; Identifier parser
          (funcall 'neovm--pc3-run p-ident "hello123 world")
          (funcall 'neovm--pc3-run p-ident "x")
          (funcall 'neovm--pc3-run p-ident "123abc")
          ;; Doubled number
          (funcall 'neovm--pc3-run p-doubled "21rest")
          (funcall 'neovm--pc3-run p-doubled "100end")
          ;; Chained map: number -> double -> add 1
          (let ((p-double-plus-1
                 (funcall 'neovm--pc3-map p-doubled (lambda (n) (1+ n)))))
            (funcall 'neovm--pc3-run p-double-plus-1 "10xyz"))))
    (fmakunbound 'neovm--pc3-char-class)
    (fmakunbound 'neovm--pc3-many1)
    (fmakunbound 'neovm--pc3-map)
    (fmakunbound 'neovm--pc3-and-then)
    (fmakunbound 'neovm--pc3-or-else)
    (fmakunbound 'neovm--pc3-run)))"#;
    let expect = expect_test::expect![[
        r#""OK ((42 . 2) (0 . 1) nil (12345 . 5) (\"hello123\" . 8) (\"x\" . 1) nil (42 . 2) (200 . 3) (21 . 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Recursive parsers: balanced parentheses, nested expressions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_parser_comb_recursive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use a lazy/deferred parser approach to handle recursion:
    // p-expr references itself via a symbol that is resolved at parse time.
    let form = r#"(progn
  (fset 'neovm--pc4-literal
    (lambda (expected)
      (lambda (input pos)
        (let ((end (+ pos (length expected))))
          (when (and (<= end (length input))
                     (string= (substring input pos end) expected))
            (cons expected end))))))

  (fset 'neovm--pc4-char-class
    (lambda (pred-fn)
      (lambda (input pos)
        (when (< pos (length input))
          (let ((ch (aref input pos)))
            (when (funcall pred-fn ch)
              (cons (char-to-string ch) (1+ pos))))))))

  (fset 'neovm--pc4-many
    (lambda (p)
      (lambda (input pos)
        (let ((results nil) (cp pos) (done nil))
          (while (not done)
            (let ((r (funcall p input cp)))
              (if r (progn (setq results (cons (car r) results)
                                 cp (cdr r)))
                (setq done t))))
          (cons (nreverse results) cp)))))

  (fset 'neovm--pc4-and-then
    (lambda (p1 p2)
      (lambda (input pos)
        (let ((r1 (funcall p1 input pos)))
          (when r1
            (let ((r2 (funcall p2 input (cdr r1))))
              (when r2
                (cons (list (car r1) (car r2)) (cdr r2)))))))))

  (fset 'neovm--pc4-map
    (lambda (p transform)
      (lambda (input pos)
        (let ((r (funcall p input pos)))
          (when r (cons (funcall transform (car r)) (cdr r)))))))

  ;; Lazy parser: resolves symbol to parser at parse time (for recursion)
  (fset 'neovm--pc4-lazy
    (lambda (parser-sym)
      (lambda (input pos)
        (funcall (symbol-value parser-sym) input pos))))

  ;; Balanced parentheses parser:
  ;; paren-expr = "(" inner ")" where inner = (paren-expr | non-paren-char)*
  ;; Returns nesting depth
  (fset 'neovm--pc4-build-paren-parser
    (lambda ()
      (let* ((p-open (funcall 'neovm--pc4-literal "("))
             (p-close (funcall 'neovm--pc4-literal ")"))
             (p-other (funcall 'neovm--pc4-char-class
                        (lambda (ch) (and (/= ch ?\() (/= ch ?\))))))
             ;; inner = many(paren-expr | other)
             ;; paren-expr = "(" inner ")" -> compute depth
             (p-inner nil)
             (p-paren nil))
        ;; Define paren-expr using defvar + lazy
        (defvar neovm--pc4-paren-parser nil)
        ;; paren-expr: "(" inner ")" -> depth = 1 + max(inner depths)
        (setq neovm--pc4-paren-parser
              (lambda (input pos)
                (let ((r-open (funcall p-open input pos)))
                  (when r-open
                    (let ((items nil)
                          (cp (cdr r-open))
                          (done nil))
                      ;; Parse inner items (recursive paren or other char)
                      (while (not done)
                        (let ((r-sub (funcall neovm--pc4-paren-parser input cp)))
                          (if r-sub
                              (progn
                                (setq items (cons (car r-sub) items)
                                      cp (cdr r-sub)))
                            (let ((r-ch (funcall p-other input cp)))
                              (if r-ch
                                  (setq items (cons 0 items)
                                        cp (cdr r-ch))
                                (setq done t))))))
                      ;; Consume closing paren
                      (let ((r-close (funcall p-close input cp)))
                        (when r-close
                          (let ((max-depth 0))
                            (dolist (d items)
                              (when (> d max-depth) (setq max-depth d)))
                            (cons (1+ max-depth) (cdr r-close))))))))))
        neovm--pc4-paren-parser)))

  (unwind-protect
      (let ((p-paren (funcall 'neovm--pc4-build-paren-parser)))
        (list
          ;; Simple: "()" -> depth 1
          (funcall p-paren "()" 0)
          ;; Nested: "(())" -> depth 2
          (funcall p-paren "(())" 0)
          ;; Deep: "((()))" -> depth 3
          (funcall p-paren "((()))" 0)
          ;; With content: "(a(b)c)" -> depth 2
          (funcall p-paren "(a(b)c)" 0)
          ;; Siblings inside: "(()())" -> depth 2
          (funcall p-paren "(()())" 0)
          ;; Complex: "((a)(b(c)))" -> depth 3
          (funcall p-paren "((a)(b(c)))" 0)
          ;; Not a paren expr: "abc"
          (funcall p-paren "abc" 0)
          ;; Unbalanced: "(()" -> only inner () matches at pos 1
          (funcall p-paren "(()x" 0)
          ;; Empty content: "()" at offset
          (funcall p-paren "xx()yy" 2)))
    (fmakunbound 'neovm--pc4-literal)
    (fmakunbound 'neovm--pc4-char-class)
    (fmakunbound 'neovm--pc4-many)
    (fmakunbound 'neovm--pc4-and-then)
    (fmakunbound 'neovm--pc4-map)
    (fmakunbound 'neovm--pc4-lazy)
    (fmakunbound 'neovm--pc4-build-paren-parser)
    (makunbound 'neovm--pc4-paren-parser)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 . 2) (2 . 4) (3 . 6) (2 . 7) (2 . 6) (3 . 11) nil nil (1 . 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: parse arithmetic expressions using combinators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_parser_comb_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full arithmetic expression parser using combinator style:
    //   expr = term (('+' | '-') term)*
    //   term = factor (('*' | '/') factor)*
    //   factor = number | '(' expr ')'
    // Returns evaluated integer result.
    let form = r#"(progn
  ;; Parser primitives
  (fset 'neovm--pc5-char-pred
    (lambda (pred-fn)
      (lambda (input pos)
        (when (< pos (length input))
          (let ((ch (aref input pos)))
            (when (funcall pred-fn ch)
              (cons ch (1+ pos))))))))

  (fset 'neovm--pc5-skip-ws
    (lambda (input pos)
      (let ((cp pos))
        (while (and (< cp (length input))
                    (memq (aref input cp) '(?\s ?\t)))
          (setq cp (1+ cp)))
        (cons nil cp))))

  ;; Parse a natural number (one or more digits)
  (fset 'neovm--pc5-number
    (lambda (input pos)
      (let ((r (funcall 'neovm--pc5-skip-ws input pos)))
        (setq pos (cdr r)))
      (let ((start pos) (cp pos))
        (while (and (< cp (length input))
                    (>= (aref input cp) ?0)
                    (<= (aref input cp) ?9))
          (setq cp (1+ cp)))
        (when (> cp start)
          (cons (string-to-number (substring input start cp)) cp)))))

  ;; Forward declarations for mutual recursion
  (defvar neovm--pc5-expr-fn nil)

  ;; factor = number | '(' expr ')'
  (fset 'neovm--pc5-factor
    (lambda (input pos)
      (let ((r (funcall 'neovm--pc5-skip-ws input pos)))
        (setq pos (cdr r)))
      (if (and (< pos (length input)) (= (aref input pos) ?\())
          ;; Parenthesized expression
          (let* ((inner (funcall neovm--pc5-expr-fn input (1+ pos))))
            (when inner
              (let ((r2 (funcall 'neovm--pc5-skip-ws input (cdr inner))))
                (setq pos (cdr r2)))
              (when (and (< pos (length input)) (= (aref input pos) ?\)))
                (cons (car inner) (1+ pos)))))
        ;; Number
        (funcall 'neovm--pc5-number input pos))))

  ;; term = factor (('*' | '/') factor)*
  (fset 'neovm--pc5-term
    (lambda (input pos)
      (let ((left (funcall 'neovm--pc5-factor input pos)))
        (when left
          (let ((val (car left)) (cp (cdr left)) (done nil))
            (while (not done)
              (let ((r (funcall 'neovm--pc5-skip-ws input cp)))
                (setq cp (cdr r)))
              (if (and (< cp (length input))
                       (memq (aref input cp) '(?* ?/)))
                  (let ((op (aref input cp))
                        (right (funcall 'neovm--pc5-factor input (1+ cp))))
                    (if right
                        (progn
                          (if (= op ?*)
                              (setq val (* val (car right)))
                            (setq val (/ val (car right))))
                          (setq cp (cdr right)))
                      (setq done t)))
                (setq done t)))
            (cons val cp))))))

  ;; expr = term (('+' | '-') term)*
  (setq neovm--pc5-expr-fn
    (lambda (input pos)
      (let ((left (funcall 'neovm--pc5-term input pos)))
        (when left
          (let ((val (car left)) (cp (cdr left)) (done nil))
            (while (not done)
              (let ((r (funcall 'neovm--pc5-skip-ws input cp)))
                (setq cp (cdr r)))
              (if (and (< cp (length input))
                       (memq (aref input cp) '(?+ ?-)))
                  (let ((op (aref input cp))
                        (right (funcall 'neovm--pc5-term input (1+ cp))))
                    (if right
                        (progn
                          (if (= op ?+)
                              (setq val (+ val (car right)))
                            (setq val (- val (car right))))
                          (setq cp (cdr right)))
                      (setq done t)))
                (setq done t)))
            (cons val cp))))))

  (fset 'neovm--pc5-eval
    (lambda (input)
      (let ((result (funcall neovm--pc5-expr-fn input 0)))
        (when result (car result)))))

  (unwind-protect
      (list
        (funcall 'neovm--pc5-eval "2 + 3")
        (funcall 'neovm--pc5-eval "2 + 3 * 4")
        (funcall 'neovm--pc5-eval "(2 + 3) * 4")
        (funcall 'neovm--pc5-eval "10 - 2 * 3 + 1")
        (funcall 'neovm--pc5-eval "100 / 5 / 4")
        (funcall 'neovm--pc5-eval "((2 + 3) * (4 - 1))")
        (funcall 'neovm--pc5-eval "1 + 2 + 3 + 4 + 5")
        (funcall 'neovm--pc5-eval "2 * 3 + 4 * 5")
        (funcall 'neovm--pc5-eval "42")
        (funcall 'neovm--pc5-eval "(((7)))"))
    (fmakunbound 'neovm--pc5-char-pred)
    (fmakunbound 'neovm--pc5-skip-ws)
    (fmakunbound 'neovm--pc5-number)
    (fmakunbound 'neovm--pc5-factor)
    (fmakunbound 'neovm--pc5-term)
    (fmakunbound 'neovm--pc5-eval)
    (makunbound 'neovm--pc5-expr-fn)))"#;
    let expect = expect_test::expect![[r#""OK (5 14 20 5 5 15 15 26 42 7)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: parse a simple S-expression using combinators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_parser_comb_sexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // S-expression parser built from combinators:
    //   sexp = atom | list
    //   atom = number | symbol
    //   list = '(' sexp* ')'
    // Returns nested Lisp structure.
    let form = r#"(progn
  (fset 'neovm--pc6-skip-ws
    (lambda (input pos)
      (while (and (< pos (length input))
                  (memq (aref input pos) '(?\s ?\t ?\n)))
        (setq pos (1+ pos)))
      pos))

  ;; Parse a number
  (fset 'neovm--pc6-number
    (lambda (input pos)
      (setq pos (funcall 'neovm--pc6-skip-ws input pos))
      (let ((start pos) (neg nil))
        ;; Optional minus
        (when (and (< pos (length input)) (= (aref input pos) ?-))
          (setq neg t pos (1+ pos)))
        (let ((cp pos))
          (while (and (< cp (length input))
                      (>= (aref input cp) ?0) (<= (aref input cp) ?9))
            (setq cp (1+ cp)))
          (when (> cp pos)
            (cons (string-to-number (substring input start cp)) cp))))))

  ;; Parse a symbol (alphanumeric + some special chars)
  (fset 'neovm--pc6-symbol
    (lambda (input pos)
      (setq pos (funcall 'neovm--pc6-skip-ws input pos))
      (let ((start pos))
        (while (and (< pos (length input))
                    (let ((ch (aref input pos)))
                      (or (and (>= ch ?a) (<= ch ?z))
                          (and (>= ch ?A) (<= ch ?Z))
                          (and (>= ch ?0) (<= ch ?9))
                          (= ch ?-) (= ch ?_) (= ch ?+)
                          (= ch ?*) (= ch ?/) (= ch ??)
                          (= ch ?!))))
          (setq pos (1+ pos)))
        (when (> pos start)
          (cons (intern (substring input start pos)) pos)))))

  ;; Forward declaration for sexp parser
  (defvar neovm--pc6-sexp-fn nil)

  ;; Parse a list: '(' sexp* ')'
  (fset 'neovm--pc6-list
    (lambda (input pos)
      (setq pos (funcall 'neovm--pc6-skip-ws input pos))
      (when (and (< pos (length input)) (= (aref input pos) ?\())
        (setq pos (1+ pos))
        (let ((items nil) (done nil))
          (while (not done)
            (setq pos (funcall 'neovm--pc6-skip-ws input pos))
            (if (and (< pos (length input)) (= (aref input pos) ?\)))
                (setq done t)
              (let ((r (funcall neovm--pc6-sexp-fn input pos)))
                (if r
                    (progn
                      (setq items (cons (car r) items)
                            pos (cdr r)))
                  (setq done t)))))
          (setq pos (funcall 'neovm--pc6-skip-ws input pos))
          (when (and (< pos (length input)) (= (aref input pos) ?\)))
            (cons (nreverse items) (1+ pos)))))))

  ;; sexp = list | number | symbol
  (setq neovm--pc6-sexp-fn
    (lambda (input pos)
      (setq pos (funcall 'neovm--pc6-skip-ws input pos))
      (or (funcall 'neovm--pc6-list input pos)
          (funcall 'neovm--pc6-number input pos)
          (funcall 'neovm--pc6-symbol input pos))))

  (fset 'neovm--pc6-parse
    (lambda (input)
      (let ((r (funcall neovm--pc6-sexp-fn input 0)))
        (when r (car r)))))

  (unwind-protect
      (list
        ;; Atoms
        (funcall 'neovm--pc6-parse "42")
        (funcall 'neovm--pc6-parse "-7")
        (funcall 'neovm--pc6-parse "hello")
        ;; Simple lists
        (funcall 'neovm--pc6-parse "()")
        (funcall 'neovm--pc6-parse "(1 2 3)")
        (funcall 'neovm--pc6-parse "(a b c)")
        ;; Nested lists
        (funcall 'neovm--pc6-parse "(+ 1 (* 2 3))")
        (funcall 'neovm--pc6-parse "(defun square (x) (* x x))")
        (funcall 'neovm--pc6-parse "((a b) (c d) (e f))")
        ;; Deep nesting
        (funcall 'neovm--pc6-parse "(((nested)))")
        ;; Mixed content
        (funcall 'neovm--pc6-parse "(if (> x 0) (+ x 1) (- x 1))")
        ;; Whitespace handling
        (funcall 'neovm--pc6-parse "  ( a   b   c )  "))
    (fmakunbound 'neovm--pc6-skip-ws)
    (fmakunbound 'neovm--pc6-number)
    (fmakunbound 'neovm--pc6-symbol)
    (fmakunbound 'neovm--pc6-list)
    (fmakunbound 'neovm--pc6-parse)
    (makunbound 'neovm--pc6-sexp-fn)))"#;
    let expect = expect_test::expect![[
        r#""OK (42 -7 hello nil (1 2 3) (a b c) (+ 1 (* 2 3)) (defun square (x) (* x x)) ((a b) (c d) (e f)) (((nested))) nil (a b c))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: combinator-based CSV parser with quoting support
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_parser_comb_csv() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // CSV parser using combinator-like approach:
    //   row = field (',' field)*
    //   field = quoted-field | unquoted-field
    //   quoted-field = '"' (char | '""')* '"'
    //   unquoted-field = [^,\n]*
    let form = r#"(progn
  ;; Parse unquoted field: everything up to , or newline or end
  (fset 'neovm--pc7-unquoted
    (lambda (input pos)
      (let ((start pos))
        (while (and (< pos (length input))
                    (/= (aref input pos) ?,)
                    (/= (aref input pos) ?\n))
          (setq pos (1+ pos)))
        (cons (substring input start pos) pos))))

  ;; Parse quoted field: "..." with "" as escaped quote
  (fset 'neovm--pc7-quoted
    (lambda (input pos)
      (when (and (< pos (length input)) (= (aref input pos) ?\"))
        (setq pos (1+ pos))
        (let ((chars nil) (done nil))
          (while (not done)
            (cond
              ((>= pos (length input))
               (setq done t))
              ((= (aref input pos) ?\")
               (if (and (< (1+ pos) (length input))
                        (= (aref input (1+ pos)) ?\"))
                   ;; Escaped quote
                   (progn
                     (setq chars (cons ?\" chars))
                     (setq pos (+ pos 2)))
                 ;; End of quoted field
                 (setq pos (1+ pos))
                 (setq done t)))
              (t
               (setq chars (cons (aref input pos) chars))
               (setq pos (1+ pos)))))
          (cons (concat (nreverse chars)) pos)))))

  ;; Parse one field (quoted or unquoted)
  (fset 'neovm--pc7-field
    (lambda (input pos)
      (or (funcall 'neovm--pc7-quoted input pos)
          (funcall 'neovm--pc7-unquoted input pos))))

  ;; Parse one row: field (',' field)*
  (fset 'neovm--pc7-row
    (lambda (input pos)
      (let ((first (funcall 'neovm--pc7-field input pos)))
        (when first
          (let ((fields (list (car first)))
                (cp (cdr first))
                (done nil))
            (while (not done)
              (if (and (< cp (length input)) (= (aref input cp) ?,))
                  (let ((next (funcall 'neovm--pc7-field input (1+ cp))))
                    (when next
                      (setq fields (cons (car next) fields)
                            cp (cdr next))))
                (setq done t)))
            (cons (nreverse fields) cp))))))

  ;; Parse full CSV: row ('\n' row)*
  (fset 'neovm--pc7-csv
    (lambda (input)
      (let ((rows nil) (pos 0) (done nil))
        (while (not done)
          (let ((r (funcall 'neovm--pc7-row input pos)))
            (if (and r (> (cdr r) pos))
                (progn
                  (setq rows (cons (car r) rows)
                        pos (cdr r))
                  ;; Skip newline
                  (when (and (< pos (length input))
                             (= (aref input pos) ?\n))
                    (setq pos (1+ pos))))
              (setq done t))))
        (nreverse rows))))

  (unwind-protect
      (list
        ;; Simple CSV
        (funcall 'neovm--pc7-csv "a,b,c\n1,2,3")
        ;; Quoted fields
        (funcall 'neovm--pc7-csv "name,desc\n\"Alice\",\"A \"\"great\"\" person\"")
        ;; Mixed quoted and unquoted
        (funcall 'neovm--pc7-csv "id,\"full name\",age\n1,\"John Doe\",30")
        ;; Empty fields
        (funcall 'neovm--pc7-csv "a,,c\n,b,")
        ;; Single row
        (funcall 'neovm--pc7-csv "hello,world")
        ;; Single column
        (funcall 'neovm--pc7-csv "one\ntwo\nthree")
        ;; Quoted field with commas
        (funcall 'neovm--pc7-csv "city,\"lat,lon\"\nNYC,\"40.7,-74.0\""))
    (fmakunbound 'neovm--pc7-unquoted)
    (fmakunbound 'neovm--pc7-quoted)
    (fmakunbound 'neovm--pc7-field)
    (fmakunbound 'neovm--pc7-row)
    (fmakunbound 'neovm--pc7-csv)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"a\" \"b\" \"c\") (\"1\" \"2\" \"3\")) ((\"name\" \"desc\") (\"Alice\" \"A \\\"great\\\" person\")) ((\"id\" \"full name\" \"age\") (\"1\" \"John Doe\" \"30\")) ((\"a\" \"\" \"c\") (\"\" \"b\" \"\")) ((\"hello\" \"world\")) ((\"one\") (\"two\") (\"three\")) ((\"city\" \"lat,lon\") (\"NYC\" \"40.7,-74.0\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
