//! Oracle parity tests for a PEG (Parsing Expression Grammar) parser
//! implemented in pure Elisp. PEG operators: sequence, ordered choice,
//! star, plus, optional, not-predicate, literal matching, character class,
//! and rule definitions via alist. Complex tests parse arithmetic expressions
//! and JSON-like structures.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// PEG core: literal matching, sequence, ordered choice
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_peg_core_operators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // PEG parser: a parse result is (value . remaining-input) on success, or nil on failure.
    // Grammar is an alist of (rule-name . peg-expression).
    // PEG expressions:
    //   (lit STR)            — match literal string
    //   (char-class PRED)    — match one char satisfying predicate name
    //   (seq E1 E2 ...)      — sequence: all must match in order
    //   (choice E1 E2 ...)   — ordered choice: first match wins
    //   (star E)             — zero or more
    //   (plus E)             — one or more
    //   (opt E)              — optional (zero or one)
    //   (not-pred E)         — not-predicate: succeed if E fails (consume nothing)
    //   (rule NAME)          — reference to named rule
    let form = r#"(progn
  ;; Core PEG interpreter
  (fset 'neovm--peg-parse
    (lambda (expr input grammar)
      "Parse INPUT with PEG EXPR using GRAMMAR. Returns (value . rest) or nil."
      (cond
       ;; Literal
       ((eq (car expr) 'lit)
        (let ((s (cadr expr)))
          (if (and (>= (length input) (length s))
                   (string= (substring input 0 (length s)) s))
              (cons s (substring input (length s)))
            nil)))

       ;; Character class (predicate symbol)
       ((eq (car expr) 'char-class)
        (let ((pred (cadr expr)))
          (if (and (> (length input) 0)
                   (funcall pred (aref input 0)))
              (cons (substring input 0 1) (substring input 1))
            nil)))

       ;; Sequence
       ((eq (car expr) 'seq)
        (let ((exprs (cdr expr))
              (rest input)
              (vals nil)
              (ok t))
          (while (and ok exprs)
            (let ((r (funcall 'neovm--peg-parse (car exprs) rest grammar)))
              (if r
                  (progn
                    (setq vals (cons (car r) vals))
                    (setq rest (cdr r)))
                (setq ok nil)))
            (setq exprs (cdr exprs)))
          (if ok
              (cons (nreverse vals) rest)
            nil)))

       ;; Ordered choice
       ((eq (car expr) 'choice)
        (let ((exprs (cdr expr))
              (result nil))
          (while (and (not result) exprs)
            (setq result (funcall 'neovm--peg-parse (car exprs) input grammar))
            (setq exprs (cdr exprs)))
          result))

       ;; Star (zero or more)
       ((eq (car expr) 'star)
        (let ((sub (cadr expr))
              (rest input)
              (vals nil)
              (cont t))
          (while cont
            (let ((r (funcall 'neovm--peg-parse sub rest grammar)))
              (if r
                  (progn
                    (setq vals (cons (car r) vals))
                    (setq rest (cdr r)))
                (setq cont nil))))
          (cons (nreverse vals) rest)))

       ;; Plus (one or more)
       ((eq (car expr) 'plus)
        (let ((sub (cadr expr)))
          (let ((first (funcall 'neovm--peg-parse sub input grammar)))
            (if first
                (let ((star-result (funcall 'neovm--peg-parse
                                            (list 'star sub) (cdr first) grammar)))
                  (cons (cons (car first) (car star-result))
                        (cdr star-result)))
              nil))))

       ;; Optional (zero or one)
       ((eq (car expr) 'opt)
        (let ((sub (cadr expr)))
          (let ((r (funcall 'neovm--peg-parse sub input grammar)))
            (if r
                (cons (list (car r)) (cdr r))
              (cons nil input)))))

       ;; Not-predicate (succeed if sub fails, consume nothing)
       ((eq (car expr) 'not-pred)
        (let ((sub (cadr expr)))
          (let ((r (funcall 'neovm--peg-parse sub input grammar)))
            (if r nil (cons t input)))))

       ;; Rule reference
       ((eq (car expr) 'rule)
        (let ((name (cadr expr)))
          (let ((rule-expr (cdr (assq name grammar))))
            (if rule-expr
                (funcall 'neovm--peg-parse rule-expr input grammar)
              (error "Unknown rule: %s" name)))))

       (t (error "Unknown PEG expr: %s" (car expr))))))

  (unwind-protect
      (let ((grammar nil))
        (list
         ;; Test literal matching
         (funcall 'neovm--peg-parse '(lit "hello") "hello world" grammar)
         (funcall 'neovm--peg-parse '(lit "hello") "goodbye" grammar)
         ;; Test sequence
         (funcall 'neovm--peg-parse '(seq (lit "a") (lit "b") (lit "c"))
                  "abcdef" grammar)
         ;; Sequence failure
         (funcall 'neovm--peg-parse '(seq (lit "a") (lit "x")) "abcdef" grammar)
         ;; Test ordered choice
         (funcall 'neovm--peg-parse '(choice (lit "x") (lit "a") (lit "b"))
                  "abcdef" grammar)
         ;; First choice wins
         (funcall 'neovm--peg-parse '(choice (lit "ab") (lit "a"))
                  "abcdef" grammar)))
    (fmakunbound 'neovm--peg-parse)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"hello\" . \" world\") nil ((\"a\" \"b\" \"c\") . \"def\") nil (\"a\" . \"bcdef\") (\"ab\" . \"cdef\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// PEG repetition operators: star, plus, optional
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_peg_repetition_operators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--peg2-parse
    (lambda (expr input grammar)
      (cond
       ((eq (car expr) 'lit)
        (let ((s (cadr expr)))
          (if (and (>= (length input) (length s))
                   (string= (substring input 0 (length s)) s))
              (cons s (substring input (length s)))
            nil)))
       ((eq (car expr) 'char-class)
        (let ((pred (cadr expr)))
          (if (and (> (length input) 0) (funcall pred (aref input 0)))
              (cons (substring input 0 1) (substring input 1))
            nil)))
       ((eq (car expr) 'seq)
        (let ((exprs (cdr expr)) (rest input) (vals nil) (ok t))
          (while (and ok exprs)
            (let ((r (funcall 'neovm--peg2-parse (car exprs) rest grammar)))
              (if r (progn (setq vals (cons (car r) vals)) (setq rest (cdr r)))
                (setq ok nil)))
            (setq exprs (cdr exprs)))
          (if ok (cons (nreverse vals) rest) nil)))
       ((eq (car expr) 'choice)
        (let ((exprs (cdr expr)) (result nil))
          (while (and (not result) exprs)
            (setq result (funcall 'neovm--peg2-parse (car exprs) input grammar))
            (setq exprs (cdr exprs)))
          result))
       ((eq (car expr) 'star)
        (let ((sub (cadr expr)) (rest input) (vals nil) (cont t))
          (while cont
            (let ((r (funcall 'neovm--peg2-parse sub rest grammar)))
              (if r (progn (setq vals (cons (car r) vals)) (setq rest (cdr r)))
                (setq cont nil))))
          (cons (nreverse vals) rest)))
       ((eq (car expr) 'plus)
        (let ((sub (cadr expr)))
          (let ((first (funcall 'neovm--peg2-parse sub input grammar)))
            (if first
                (let ((sr (funcall 'neovm--peg2-parse (list 'star sub) (cdr first) grammar)))
                  (cons (cons (car first) (car sr)) (cdr sr)))
              nil))))
       ((eq (car expr) 'opt)
        (let ((sub (cadr expr)))
          (let ((r (funcall 'neovm--peg2-parse sub input grammar)))
            (if r (cons (list (car r)) (cdr r))
              (cons nil input)))))
       ((eq (car expr) 'not-pred)
        (let ((r (funcall 'neovm--peg2-parse (cadr expr) input grammar)))
          (if r nil (cons t input))))
       ((eq (car expr) 'rule)
        (let ((rule-expr (cdr (assq (cadr expr) grammar))))
          (if rule-expr (funcall 'neovm--peg2-parse rule-expr input grammar)
            nil)))
       (t nil))))

  (unwind-protect
      (let ((g nil))
        (list
         ;; star: zero matches
         (funcall 'neovm--peg2-parse '(star (lit "x")) "yyy" g)
         ;; star: multiple matches
         (funcall 'neovm--peg2-parse '(star (lit "ab")) "ababab-rest" g)
         ;; plus: requires at least one
         (funcall 'neovm--peg2-parse '(plus (lit "x")) "yyy" g)
         (funcall 'neovm--peg2-parse '(plus (lit "x")) "xxxy" g)
         ;; optional: present
         (funcall 'neovm--peg2-parse '(opt (lit "prefix-")) "prefix-data" g)
         ;; optional: absent
         (funcall 'neovm--peg2-parse '(opt (lit "prefix-")) "data" g)
         ;; not-predicate: succeeds when sub fails
         (funcall 'neovm--peg2-parse '(not-pred (lit "bad")) "good" g)
         ;; not-predicate: fails when sub succeeds
         (funcall 'neovm--peg2-parse '(not-pred (lit "ba")) "bad" g)))
    (fmakunbound 'neovm--peg2-parse)))"#;
    let expect = expect_test::expect![[
        r#""OK ((nil . \"yyy\") ((\"ab\" \"ab\" \"ab\") . \"-rest\") nil ((\"x\" \"x\" \"x\") . \"y\") ((\"prefix-\") . \"data\") (nil . \"data\") (t . \"good\") nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// PEG with named rules: simple grammar
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_peg_named_rules() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--peg3-parse
    (lambda (expr input grammar)
      (cond
       ((eq (car expr) 'lit)
        (let ((s (cadr expr)))
          (if (and (>= (length input) (length s))
                   (string= (substring input 0 (length s)) s))
              (cons s (substring input (length s))) nil)))
       ((eq (car expr) 'char-class)
        (let ((pred (cadr expr)))
          (if (and (> (length input) 0) (funcall pred (aref input 0)))
              (cons (substring input 0 1) (substring input 1)) nil)))
       ((eq (car expr) 'seq)
        (let ((exprs (cdr expr)) (rest input) (vals nil) (ok t))
          (while (and ok exprs)
            (let ((r (funcall 'neovm--peg3-parse (car exprs) rest grammar)))
              (if r (progn (setq vals (cons (car r) vals)) (setq rest (cdr r)))
                (setq ok nil)))
            (setq exprs (cdr exprs)))
          (if ok (cons (nreverse vals) rest) nil)))
       ((eq (car expr) 'choice)
        (let ((exprs (cdr expr)) (result nil))
          (while (and (not result) exprs)
            (setq result (funcall 'neovm--peg3-parse (car exprs) input grammar))
            (setq exprs (cdr exprs)))
          result))
       ((eq (car expr) 'star)
        (let ((sub (cadr expr)) (rest input) (vals nil) (cont t))
          (while cont
            (let ((r (funcall 'neovm--peg3-parse sub rest grammar)))
              (if r (progn (setq vals (cons (car r) vals)) (setq rest (cdr r)))
                (setq cont nil))))
          (cons (nreverse vals) rest)))
       ((eq (car expr) 'plus)
        (let ((sub (cadr expr)))
          (let ((first (funcall 'neovm--peg3-parse sub input grammar)))
            (if first
                (let ((sr (funcall 'neovm--peg3-parse (list 'star sub) (cdr first) grammar)))
                  (cons (cons (car first) (car sr)) (cdr sr)))
              nil))))
       ((eq (car expr) 'opt)
        (let ((r (funcall 'neovm--peg3-parse (cadr expr) input grammar)))
          (if r (cons (list (car r)) (cdr r)) (cons nil input))))
       ((eq (car expr) 'not-pred)
        (let ((r (funcall 'neovm--peg3-parse (cadr expr) input grammar)))
          (if r nil (cons t input))))
       ((eq (car expr) 'rule)
        (let ((rule-expr (cdr (assq (cadr expr) grammar))))
          (if rule-expr (funcall 'neovm--peg3-parse rule-expr input grammar) nil)))
       (t nil))))

  ;; Grammar for: identifier = letter (letter | digit)*
  ;; letter = a-z, digit = 0-9
  (fset 'neovm--peg3-is-letter
    (lambda (c) (and (>= c ?a) (<= c ?z))))
  (fset 'neovm--peg3-is-digit
    (lambda (c) (and (>= c ?0) (<= c ?9))))

  (unwind-protect
      (let ((grammar (list
                      (cons 'identifier
                            '(seq (char-class neovm--peg3-is-letter)
                                  (star (choice (char-class neovm--peg3-is-letter)
                                                (char-class neovm--peg3-is-digit)))))
                      (cons 'number
                            '(plus (char-class neovm--peg3-is-digit))))))
        (list
         ;; Parse identifiers
         (funcall 'neovm--peg3-parse '(rule identifier) "hello123 rest" grammar)
         (funcall 'neovm--peg3-parse '(rule identifier) "x99y end" grammar)
         ;; Identifier must start with letter
         (funcall 'neovm--peg3-parse '(rule identifier) "123abc" grammar)
         ;; Parse numbers
         (funcall 'neovm--peg3-parse '(rule number) "42abc" grammar)
         (funcall 'neovm--peg3-parse '(rule number) "007rest" grammar)
         ;; Number must start with digit
         (funcall 'neovm--peg3-parse '(rule number) "abc" grammar)
         ;; Choice between identifier and number
         (funcall 'neovm--peg3-parse '(choice (rule number) (rule identifier))
                  "hello" grammar)
         (funcall 'neovm--peg3-parse '(choice (rule number) (rule identifier))
                  "42" grammar)))
    (fmakunbound 'neovm--peg3-parse)
    (fmakunbound 'neovm--peg3-is-letter)
    (fmakunbound 'neovm--peg3-is-digit)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"h\" (\"e\" \"l\" \"l\" \"o\" \"1\" \"2\" \"3\")) . \" rest\") ((\"x\" (\"9\" \"9\" \"y\")) . \" end\") nil ((\"4\" \"2\") . \"abc\") ((\"0\" \"0\" \"7\") . \"rest\") nil ((\"h\" (\"e\" \"l\" \"l\" \"o\")) . \"\") ((\"4\" \"2\") . \"\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: parse arithmetic expressions with PEG
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_peg_arithmetic_expressions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // PEG grammar for simple arithmetic: expr = term (('+' | '-') term)*
    // term = factor (('*' | '/') factor)*
    // factor = number | '(' expr ')'
    // number = digit+
    let form = r#"(progn
  (fset 'neovm--peg4-parse
    (lambda (expr input grammar)
      (cond
       ((eq (car expr) 'lit)
        (let ((s (cadr expr)))
          (if (and (>= (length input) (length s))
                   (string= (substring input 0 (length s)) s))
              (cons s (substring input (length s))) nil)))
       ((eq (car expr) 'char-class)
        (let ((pred (cadr expr)))
          (if (and (> (length input) 0) (funcall pred (aref input 0)))
              (cons (substring input 0 1) (substring input 1)) nil)))
       ((eq (car expr) 'seq)
        (let ((exprs (cdr expr)) (rest input) (vals nil) (ok t))
          (while (and ok exprs)
            (let ((r (funcall 'neovm--peg4-parse (car exprs) rest grammar)))
              (if r (progn (setq vals (cons (car r) vals)) (setq rest (cdr r)))
                (setq ok nil)))
            (setq exprs (cdr exprs)))
          (if ok (cons (nreverse vals) rest) nil)))
       ((eq (car expr) 'choice)
        (let ((exprs (cdr expr)) (result nil))
          (while (and (not result) exprs)
            (setq result (funcall 'neovm--peg4-parse (car exprs) input grammar))
            (setq exprs (cdr exprs)))
          result))
       ((eq (car expr) 'star)
        (let ((sub (cadr expr)) (rest input) (vals nil) (cont t))
          (while cont
            (let ((r (funcall 'neovm--peg4-parse sub rest grammar)))
              (if r (progn (setq vals (cons (car r) vals)) (setq rest (cdr r)))
                (setq cont nil))))
          (cons (nreverse vals) rest)))
       ((eq (car expr) 'plus)
        (let ((sub (cadr expr)))
          (let ((first (funcall 'neovm--peg4-parse sub input grammar)))
            (if first
                (let ((sr (funcall 'neovm--peg4-parse (list 'star sub) (cdr first) grammar)))
                  (cons (cons (car first) (car sr)) (cdr sr)))
              nil))))
       ((eq (car expr) 'opt)
        (let ((r (funcall 'neovm--peg4-parse (cadr expr) input grammar)))
          (if r (cons (list (car r)) (cdr r)) (cons nil input))))
       ((eq (car expr) 'not-pred)
        (let ((r (funcall 'neovm--peg4-parse (cadr expr) input grammar)))
          (if r nil (cons t input))))
       ((eq (car expr) 'rule)
        (let ((rule-expr (cdr (assq (cadr expr) grammar))))
          (if rule-expr (funcall 'neovm--peg4-parse rule-expr input grammar) nil)))
       (t nil))))

  (fset 'neovm--peg4-is-digit (lambda (c) (and (>= c ?0) (<= c ?9))))

  (unwind-protect
      (let ((grammar
             (list
              ;; expr = term (('+' | '-') term)*
              (cons 'expr '(seq (rule term)
                                (star (seq (choice (lit "+") (lit "-"))
                                           (rule term)))))
              ;; term = factor (('*' | '/') factor)*
              (cons 'term '(seq (rule factor)
                                (star (seq (choice (lit "*") (lit "/"))
                                           (rule factor)))))
              ;; factor = number | '(' expr ')'
              (cons 'factor '(choice (rule number)
                                     (seq (lit "(") (rule expr) (lit ")"))))
              ;; number = digit+
              (cons 'number '(plus (char-class neovm--peg4-is-digit))))))
        (list
         ;; Simple number
         (not (null (funcall 'neovm--peg4-parse '(rule expr) "42" grammar)))
         ;; Addition
         (car (funcall 'neovm--peg4-parse '(rule expr) "1+2" grammar))
         ;; Multiplication with addition
         (car (funcall 'neovm--peg4-parse '(rule expr) "3*4+5" grammar))
         ;; Parenthesized
         (not (null (funcall 'neovm--peg4-parse '(rule expr) "(1+2)*3" grammar)))
         ;; Remaining input after parse
         (cdr (funcall 'neovm--peg4-parse '(rule expr) "10+20rest" grammar))
         ;; Nested parens
         (not (null (funcall 'neovm--peg4-parse '(rule expr) "((1+2))" grammar)))
         ;; Invalid: starts with operator
         (funcall 'neovm--peg4-parse '(rule expr) "+3" grammar)))
    (fmakunbound 'neovm--peg4-parse)
    (fmakunbound 'neovm--peg4-is-digit)))"#;
    let expect = expect_test::expect![[
        r#""OK (t (((\"1\") nil) ((\"+\" ((\"2\") nil)))) (((\"3\") ((\"*\" (\"4\")))) ((\"+\" ((\"5\") nil)))) t \"rest\" t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: parse simple key-value structures (JSON-like)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_peg_json_like_structures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Grammar for simple key:value pairs separated by commas, enclosed in braces
    // {key1:val1,key2:val2}
    // Keys and values are alphanumeric strings
    let form = r#"(progn
  (fset 'neovm--peg5-parse
    (lambda (expr input grammar)
      (cond
       ((eq (car expr) 'lit)
        (let ((s (cadr expr)))
          (if (and (>= (length input) (length s))
                   (string= (substring input 0 (length s)) s))
              (cons s (substring input (length s))) nil)))
       ((eq (car expr) 'char-class)
        (let ((pred (cadr expr)))
          (if (and (> (length input) 0) (funcall pred (aref input 0)))
              (cons (substring input 0 1) (substring input 1)) nil)))
       ((eq (car expr) 'seq)
        (let ((exprs (cdr expr)) (rest input) (vals nil) (ok t))
          (while (and ok exprs)
            (let ((r (funcall 'neovm--peg5-parse (car exprs) rest grammar)))
              (if r (progn (setq vals (cons (car r) vals)) (setq rest (cdr r)))
                (setq ok nil)))
            (setq exprs (cdr exprs)))
          (if ok (cons (nreverse vals) rest) nil)))
       ((eq (car expr) 'choice)
        (let ((exprs (cdr expr)) (result nil))
          (while (and (not result) exprs)
            (setq result (funcall 'neovm--peg5-parse (car exprs) input grammar))
            (setq exprs (cdr exprs)))
          result))
       ((eq (car expr) 'star)
        (let ((sub (cadr expr)) (rest input) (vals nil) (cont t))
          (while cont
            (let ((r (funcall 'neovm--peg5-parse sub rest grammar)))
              (if r (progn (setq vals (cons (car r) vals)) (setq rest (cdr r)))
                (setq cont nil))))
          (cons (nreverse vals) rest)))
       ((eq (car expr) 'plus)
        (let ((sub (cadr expr)))
          (let ((first (funcall 'neovm--peg5-parse sub input grammar)))
            (if first
                (let ((sr (funcall 'neovm--peg5-parse (list 'star sub) (cdr first) grammar)))
                  (cons (cons (car first) (car sr)) (cdr sr)))
              nil))))
       ((eq (car expr) 'opt)
        (let ((r (funcall 'neovm--peg5-parse (cadr expr) input grammar)))
          (if r (cons (list (car r)) (cdr r)) (cons nil input))))
       ((eq (car expr) 'not-pred)
        (let ((r (funcall 'neovm--peg5-parse (cadr expr) input grammar)))
          (if r nil (cons t input))))
       ((eq (car expr) 'rule)
        (let ((rule-expr (cdr (assq (cadr expr) grammar))))
          (if rule-expr (funcall 'neovm--peg5-parse rule-expr input grammar) nil)))
       (t nil))))

  (fset 'neovm--peg5-is-alnum
    (lambda (c) (or (and (>= c ?a) (<= c ?z))
                    (and (>= c ?A) (<= c ?Z))
                    (and (>= c ?0) (<= c ?9)))))

  (unwind-protect
      (let ((grammar
             (list
              ;; object = '{' pair (',' pair)* '}'
              (cons 'object '(seq (lit "{")
                                  (rule pair)
                                  (star (seq (lit ",") (rule pair)))
                                  (lit "}")))
              ;; pair = word ':' word
              (cons 'pair '(seq (rule word) (lit ":") (rule word)))
              ;; word = alnum+
              (cons 'word '(plus (char-class neovm--peg5-is-alnum))))))
        (list
         ;; Single pair
         (not (null (funcall 'neovm--peg5-parse '(rule object)
                             "{name:alice}" grammar)))
         ;; Two pairs
         (car (funcall 'neovm--peg5-parse '(rule object)
                       "{name:alice,age:30}" grammar))
         ;; Three pairs
         (not (null (funcall 'neovm--peg5-parse '(rule object)
                             "{a:1,b:2,c:3}" grammar)))
         ;; Remaining input
         (cdr (funcall 'neovm--peg5-parse '(rule object)
                       "{x:y}extra" grammar))
         ;; Empty object fails (no pairs)
         (funcall 'neovm--peg5-parse '(rule object) "{}" grammar)
         ;; Missing closing brace fails
         (funcall 'neovm--peg5-parse '(rule object) "{a:b" grammar)
         ;; Just a word
         (car (funcall 'neovm--peg5-parse '(rule word) "hello123" grammar))))
    (fmakunbound 'neovm--peg5-parse)
    (fmakunbound 'neovm--peg5-is-alnum)))"#;
    let expect = expect_test::expect![[
        r#""OK (t (\"{\" ((\"n\" \"a\" \"m\" \"e\") \":\" (\"a\" \"l\" \"i\" \"c\" \"e\")) ((\",\" ((\"a\" \"g\" \"e\") \":\" (\"3\" \"0\")))) \"}\") t \"extra\" nil nil (\"h\" \"e\" \"l\" \"l\" \"o\" \"1\" \"2\" \"3\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// PEG not-predicate and character classes combined
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_peg_not_predicate_and_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use not-predicate to match "any character except X" pattern
    // Grammar: match a C-style line comment (// until end-of-line)
    let form = r#"(progn
  (fset 'neovm--peg6-parse
    (lambda (expr input grammar)
      (cond
       ((eq (car expr) 'lit)
        (let ((s (cadr expr)))
          (if (and (>= (length input) (length s))
                   (string= (substring input 0 (length s)) s))
              (cons s (substring input (length s))) nil)))
       ((eq (car expr) 'char-class)
        (let ((pred (cadr expr)))
          (if (and (> (length input) 0) (funcall pred (aref input 0)))
              (cons (substring input 0 1) (substring input 1)) nil)))
       ((eq (car expr) 'any-char)
        (if (> (length input) 0)
            (cons (substring input 0 1) (substring input 1)) nil))
       ((eq (car expr) 'seq)
        (let ((exprs (cdr expr)) (rest input) (vals nil) (ok t))
          (while (and ok exprs)
            (let ((r (funcall 'neovm--peg6-parse (car exprs) rest grammar)))
              (if r (progn (setq vals (cons (car r) vals)) (setq rest (cdr r)))
                (setq ok nil)))
            (setq exprs (cdr exprs)))
          (if ok (cons (nreverse vals) rest) nil)))
       ((eq (car expr) 'choice)
        (let ((exprs (cdr expr)) (result nil))
          (while (and (not result) exprs)
            (setq result (funcall 'neovm--peg6-parse (car exprs) input grammar))
            (setq exprs (cdr exprs)))
          result))
       ((eq (car expr) 'star)
        (let ((sub (cadr expr)) (rest input) (vals nil) (cont t))
          (while cont
            (let ((r (funcall 'neovm--peg6-parse sub rest grammar)))
              (if r (progn (setq vals (cons (car r) vals)) (setq rest (cdr r)))
                (setq cont nil))))
          (cons (nreverse vals) rest)))
       ((eq (car expr) 'plus)
        (let ((sub (cadr expr)))
          (let ((first (funcall 'neovm--peg6-parse sub input grammar)))
            (if first
                (let ((sr (funcall 'neovm--peg6-parse (list 'star sub) (cdr first) grammar)))
                  (cons (cons (car first) (car sr)) (cdr sr)))
              nil))))
       ((eq (car expr) 'opt)
        (let ((r (funcall 'neovm--peg6-parse (cadr expr) input grammar)))
          (if r (cons (list (car r)) (cdr r)) (cons nil input))))
       ((eq (car expr) 'not-pred)
        (let ((r (funcall 'neovm--peg6-parse (cadr expr) input grammar)))
          (if r nil (cons t input))))
       ((eq (car expr) 'rule)
        (let ((rule-expr (cdr (assq (cadr expr) grammar))))
          (if rule-expr (funcall 'neovm--peg6-parse rule-expr input grammar) nil)))
       (t nil))))

  (fset 'neovm--peg6-is-not-newline
    (lambda (c) (not (= c ?\n))))

  (unwind-protect
      (let ((grammar
             (list
              ;; comment = '//' not-newline* newline?
              (cons 'comment '(seq (lit "//")
                                   (star (char-class neovm--peg6-is-not-newline))))
              ;; word = not-predicate on space, then any non-space char+
              (cons 'nonspace-char
                    '(seq (not-pred (lit " ")) (char-class neovm--peg6-is-not-newline))))))
        (list
         ;; Parse a line comment
         (funcall 'neovm--peg6-parse '(rule comment)
                  "// this is a comment\nnext line" grammar)
         ;; Empty comment
         (funcall 'neovm--peg6-parse '(rule comment) "//\ncode" grammar)
         ;; Not a comment
         (funcall 'neovm--peg6-parse '(rule comment) "not a comment" grammar)
         ;; Non-space characters using not-predicate
         (funcall 'neovm--peg6-parse '(plus (rule nonspace-char))
                  "hello world" grammar)
         ;; not-pred on matching input returns nil
         (funcall 'neovm--peg6-parse '(not-pred (lit "//")) "// comment" grammar)
         ;; not-pred on non-matching input returns t and consumes nothing
         (funcall 'neovm--peg6-parse '(not-pred (lit "//")) "normal" grammar)))
    (fmakunbound 'neovm--peg6-parse)
    (fmakunbound 'neovm--peg6-is-not-newline)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"//\" (\" \" \"t\" \"h\" \"i\" \"s\" \" \" \"i\" \"s\" \" \" \"a\" \" \" \"c\" \"o\" \"m\" \"m\" \"e\" \"n\" \"t\")) . \"\\nnext line\") ((\"//\" nil) . \"\\ncode\") nil (((t \"h\") (t \"e\") (t \"l\") (t \"l\") (t \"o\")) . \" world\") nil (t . \"normal\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
