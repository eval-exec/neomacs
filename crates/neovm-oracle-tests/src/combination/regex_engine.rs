//! Oracle parity tests for a simple regex engine implemented in Elisp.
//!
//! Builds an NFA-based regex engine supporting: literal characters,
//! `.` (any char), `*` (zero or more), `+` (one or more), `?` (optional),
//! `|` (alternation), `()` grouping, `^` (start anchor), `$` (end anchor).
//! Tests parsing regex into AST, compiling AST to NFA (Thompson's construction),
//! and matching various patterns against test strings.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Regex parser: tokenize and parse regex string into AST
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regex_engine_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // AST node types:
    //   (lit CHAR)        - literal character
    //   (dot)             - any character
    //   (anchor-start)    - ^
    //   (anchor-end)      - $
    //   (cat A B)         - concatenation
    //   (alt A B)         - alternation
    //   (star A)          - zero or more
    //   (plus A)          - one or more
    //   (opt A)           - optional
    let form = r#"(progn
  ;; Tokenizer: returns list of tokens
  ;; Tokens: (lit . CHAR), dot, star, plus, qmark, pipe, lparen, rparen, caret, dollar
  (fset 'neovm--re-tokenize
    (lambda (pattern)
      (let ((tokens nil) (i 0) (len (length pattern)))
        (while (< i len)
          (let ((ch (aref pattern i)))
            (cond
              ((= ch ?\\)
               (setq i (1+ i))
               (when (< i len)
                 (setq tokens (cons (cons 'lit (aref pattern i)) tokens))))
              ((= ch ?.) (setq tokens (cons 'dot tokens)))
              ((= ch ?*) (setq tokens (cons 'star tokens)))
              ((= ch ?+) (setq tokens (cons 'plus tokens)))
              ((= ch ??) (setq tokens (cons 'qmark tokens)))
              ((= ch ?|) (setq tokens (cons 'pipe tokens)))
              ((= ch ?\() (setq tokens (cons 'lparen tokens)))
              ((= ch ?\)) (setq tokens (cons 'rparen tokens)))
              ((= ch ?^) (setq tokens (cons 'caret tokens)))
              ((= ch ?$) (setq tokens (cons 'dollar tokens)))
              (t (setq tokens (cons (cons 'lit ch) tokens)))))
          (setq i (1+ i)))
        (nreverse tokens))))

  ;; Recursive descent parser
  ;; Grammar:
  ;;   expr     -> sequence ('|' sequence)*
  ;;   sequence -> postfix+
  ;;   postfix  -> atom ('*' | '+' | '?')?
  ;;   atom     -> '(' expr ')' | '.' | '^' | '$' | literal

  (defvar neovm--re-tokens nil)

  (fset 'neovm--re-peek
    (lambda ()
      (car neovm--re-tokens)))

  (fset 'neovm--re-advance
    (lambda ()
      (let ((tok (car neovm--re-tokens)))
        (setq neovm--re-tokens (cdr neovm--re-tokens))
        tok)))

  (fset 'neovm--re-parse-atom
    (lambda ()
      (let ((tok (funcall 'neovm--re-peek)))
        (cond
          ((null tok) nil)
          ((eq tok 'lparen)
           (funcall 'neovm--re-advance) ;; consume (
           (let ((inner (funcall 'neovm--re-parse-expr)))
             (funcall 'neovm--re-advance) ;; consume )
             inner))
          ((eq tok 'dot)
           (funcall 'neovm--re-advance)
           '(dot))
          ((eq tok 'caret)
           (funcall 'neovm--re-advance)
           '(anchor-start))
          ((eq tok 'dollar)
           (funcall 'neovm--re-advance)
           '(anchor-end))
          ((and (consp tok) (eq (car tok) 'lit))
           (funcall 'neovm--re-advance)
           (list 'lit (cdr tok)))
          (t nil)))))

  (fset 'neovm--re-parse-postfix
    (lambda ()
      (let ((atom (funcall 'neovm--re-parse-atom)))
        (when atom
          (let ((next (funcall 'neovm--re-peek)))
            (cond
              ((eq next 'star)
               (funcall 'neovm--re-advance)
               (list 'star atom))
              ((eq next 'plus)
               (funcall 'neovm--re-advance)
               (list 'plus atom))
              ((eq next 'qmark)
               (funcall 'neovm--re-advance)
               (list 'opt atom))
              (t atom)))))))

  (fset 'neovm--re-parse-sequence
    (lambda ()
      (let ((first (funcall 'neovm--re-parse-postfix)))
        (if (null first) nil
          (let ((rest (funcall 'neovm--re-parse-postfix)))
            (if (null rest) first
              ;; Build left-associative cat chain
              (let ((result (list 'cat first rest)))
                (let ((next (funcall 'neovm--re-parse-postfix)))
                  (while next
                    (setq result (list 'cat result next))
                    (setq next (funcall 'neovm--re-parse-postfix))))
                result)))))))

  (fset 'neovm--re-parse-expr
    (lambda ()
      (let ((left (funcall 'neovm--re-parse-sequence)))
        (if (eq (funcall 'neovm--re-peek) 'pipe)
            (progn
              (funcall 'neovm--re-advance) ;; consume |
              (let ((right (funcall 'neovm--re-parse-expr)))
                (list 'alt left right)))
          left))))

  (fset 'neovm--re-parse
    (lambda (pattern)
      (setq neovm--re-tokens (funcall 'neovm--re-tokenize pattern))
      (funcall 'neovm--re-parse-expr)))

  (unwind-protect
      (list
        ;; Simple literal
        (funcall 'neovm--re-parse "abc")
        ;; Dot
        (funcall 'neovm--re-parse "a.c")
        ;; Star
        (funcall 'neovm--re-parse "a*")
        ;; Plus
        (funcall 'neovm--re-parse "a+")
        ;; Optional
        (funcall 'neovm--re-parse "ab?c")
        ;; Alternation
        (funcall 'neovm--re-parse "a|b")
        ;; Grouping with star
        (funcall 'neovm--re-parse "(ab)*")
        ;; Complex: (a|b)*c+
        (funcall 'neovm--re-parse "(a|b)*c+")
        ;; Anchors
        (funcall 'neovm--re-parse "^abc$")
        ;; Escaped special char
        (funcall 'neovm--re-parse "a\\.b"))
    (fmakunbound 'neovm--re-tokenize)
    (fmakunbound 'neovm--re-peek)
    (fmakunbound 'neovm--re-advance)
    (fmakunbound 'neovm--re-parse-atom)
    (fmakunbound 'neovm--re-parse-postfix)
    (fmakunbound 'neovm--re-parse-sequence)
    (fmakunbound 'neovm--re-parse-expr)
    (fmakunbound 'neovm--re-parse)
    (makunbound 'neovm--re-tokens)))"#;
    let expect = expect_test::expect![[
        r#""OK ((cat (cat (lit 97) (lit 98)) (lit 99)) (cat (cat (lit 97) (dot)) (lit 99)) (star (lit 97)) (plus (lit 97)) (cat (cat (lit 97) (opt (lit 98))) (lit 99)) (alt (lit 97) (lit 98)) (star (cat (lit 97) (lit 98))) (cat (star (alt (lit 97) (lit 98))) (plus (lit 99))) (cat (cat (cat (cat (anchor-start) (lit 97)) (lit 98)) (lit 99)) (anchor-end)) (cat (cat (lit 97) (lit 46)) (lit 98)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// NFA compiler: Thompson's construction from AST
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regex_engine_nfa_compiler() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // NFA state: integer. Transitions stored in hash table.
    // Fragment: (start accept transitions) where transitions is a list
    // of (from-state match-spec to-state), match-spec is char, 'any, or 'epsilon
    let form = r#"(progn
  (defvar neovm--re-state-counter 0)

  (fset 'neovm--re-new-state
    (lambda ()
      (let ((s neovm--re-state-counter))
        (setq neovm--re-state-counter (1+ neovm--re-state-counter))
        s)))

  ;; Tokenizer (same as parser test)
  (fset 'neovm--re-tokenize
    (lambda (pattern)
      (let ((tokens nil) (i 0) (len (length pattern)))
        (while (< i len)
          (let ((ch (aref pattern i)))
            (cond
              ((= ch ?\\) (setq i (1+ i))
               (when (< i len) (setq tokens (cons (cons 'lit (aref pattern i)) tokens))))
              ((= ch ?.) (setq tokens (cons 'dot tokens)))
              ((= ch ?*) (setq tokens (cons 'star tokens)))
              ((= ch ?+) (setq tokens (cons 'plus tokens)))
              ((= ch ??) (setq tokens (cons 'qmark tokens)))
              ((= ch ?|) (setq tokens (cons 'pipe tokens)))
              ((= ch ?\() (setq tokens (cons 'lparen tokens)))
              ((= ch ?\)) (setq tokens (cons 'rparen tokens)))
              ((= ch ?^) (setq tokens (cons 'caret tokens)))
              ((= ch ?$) (setq tokens (cons 'dollar tokens)))
              (t (setq tokens (cons (cons 'lit ch) tokens)))))
          (setq i (1+ i)))
        (nreverse tokens))))

  ;; Parser (same)
  (defvar neovm--re-tokens nil)
  (fset 'neovm--re-peek (lambda () (car neovm--re-tokens)))
  (fset 'neovm--re-advance
    (lambda () (let ((t2 (car neovm--re-tokens)))
                 (setq neovm--re-tokens (cdr neovm--re-tokens)) t2)))
  (fset 'neovm--re-parse-atom
    (lambda ()
      (let ((tok (funcall 'neovm--re-peek)))
        (cond ((null tok) nil)
              ((eq tok 'lparen) (funcall 'neovm--re-advance)
               (let ((inner (funcall 'neovm--re-parse-expr)))
                 (funcall 'neovm--re-advance) inner))
              ((eq tok 'dot) (funcall 'neovm--re-advance) '(dot))
              ((eq tok 'caret) (funcall 'neovm--re-advance) '(anchor-start))
              ((eq tok 'dollar) (funcall 'neovm--re-advance) '(anchor-end))
              ((and (consp tok) (eq (car tok) 'lit))
               (funcall 'neovm--re-advance) (list 'lit (cdr tok)))
              (t nil)))))
  (fset 'neovm--re-parse-postfix
    (lambda ()
      (let ((atom (funcall 'neovm--re-parse-atom)))
        (when atom
          (let ((next (funcall 'neovm--re-peek)))
            (cond ((eq next 'star) (funcall 'neovm--re-advance) (list 'star atom))
                  ((eq next 'plus) (funcall 'neovm--re-advance) (list 'plus atom))
                  ((eq next 'qmark) (funcall 'neovm--re-advance) (list 'opt atom))
                  (t atom)))))))
  (fset 'neovm--re-parse-sequence
    (lambda ()
      (let ((first (funcall 'neovm--re-parse-postfix)))
        (if (null first) nil
          (let ((rest (funcall 'neovm--re-parse-postfix)))
            (if (null rest) first
              (let ((result (list 'cat first rest)))
                (let ((next (funcall 'neovm--re-parse-postfix)))
                  (while next
                    (setq result (list 'cat result next))
                    (setq next (funcall 'neovm--re-parse-postfix))))
                result)))))))
  (fset 'neovm--re-parse-expr
    (lambda ()
      (let ((left (funcall 'neovm--re-parse-sequence)))
        (if (eq (funcall 'neovm--re-peek) 'pipe)
            (progn (funcall 'neovm--re-advance)
                   (list 'alt left (funcall 'neovm--re-parse-expr)))
          left))))
  (fset 'neovm--re-parse
    (lambda (pattern)
      (setq neovm--re-tokens (funcall 'neovm--re-tokenize pattern))
      (funcall 'neovm--re-parse-expr)))

  ;; Compiler: AST -> NFA fragment (start accept trans-list)
  (fset 'neovm--re-compile-ast
    (lambda (ast)
      (let ((kind (car ast)))
        (cond
          ;; Literal char
          ((eq kind 'lit)
           (let ((s (funcall 'neovm--re-new-state))
                 (e (funcall 'neovm--re-new-state)))
             (list s e (list (list s (nth 1 ast) e)))))

          ;; Dot (any char)
          ((eq kind 'dot)
           (let ((s (funcall 'neovm--re-new-state))
                 (e (funcall 'neovm--re-new-state)))
             (list s e (list (list s 'any e)))))

          ;; Concatenation
          ((eq kind 'cat)
           (let ((f1 (funcall 'neovm--re-compile-ast (nth 1 ast)))
                 (f2 (funcall 'neovm--re-compile-ast (nth 2 ast))))
             (list (nth 0 f1) (nth 1 f2)
                   (append (nth 2 f1) (nth 2 f2)
                           (list (list (nth 1 f1) 'epsilon (nth 0 f2)))))))

          ;; Alternation
          ((eq kind 'alt)
           (let ((f1 (funcall 'neovm--re-compile-ast (nth 1 ast)))
                 (f2 (funcall 'neovm--re-compile-ast (nth 2 ast)))
                 (s (funcall 'neovm--re-new-state))
                 (e (funcall 'neovm--re-new-state)))
             (list s e
                   (append (nth 2 f1) (nth 2 f2)
                           (list (list s 'epsilon (nth 0 f1))
                                 (list s 'epsilon (nth 0 f2))
                                 (list (nth 1 f1) 'epsilon e)
                                 (list (nth 1 f2) 'epsilon e))))))

          ;; Star (zero or more)
          ((eq kind 'star)
           (let ((f1 (funcall 'neovm--re-compile-ast (nth 1 ast)))
                 (s (funcall 'neovm--re-new-state))
                 (e (funcall 'neovm--re-new-state)))
             (list s e
                   (append (nth 2 f1)
                           (list (list s 'epsilon (nth 0 f1))
                                 (list s 'epsilon e)
                                 (list (nth 1 f1) 'epsilon (nth 0 f1))
                                 (list (nth 1 f1) 'epsilon e))))))

          ;; Plus (one or more) = A then A*
          ((eq kind 'plus)
           (let ((f1 (funcall 'neovm--re-compile-ast (nth 1 ast)))
                 (s (funcall 'neovm--re-new-state))
                 (e (funcall 'neovm--re-new-state)))
             (list s e
                   (append (nth 2 f1)
                           (list (list s 'epsilon (nth 0 f1))
                                 (list (nth 1 f1) 'epsilon (nth 0 f1))
                                 (list (nth 1 f1) 'epsilon e))))))

          ;; Optional (zero or one)
          ((eq kind 'opt)
           (let ((f1 (funcall 'neovm--re-compile-ast (nth 1 ast)))
                 (s (funcall 'neovm--re-new-state))
                 (e (funcall 'neovm--re-new-state)))
             (list s e
                   (append (nth 2 f1)
                           (list (list s 'epsilon (nth 0 f1))
                                 (list s 'epsilon e)
                                 (list (nth 1 f1) 'epsilon e))))))

          ;; Anchors: treated as epsilon (matching handled in runner)
          ((eq kind 'anchor-start)
           (let ((s (funcall 'neovm--re-new-state))
                 (e (funcall 'neovm--re-new-state)))
             (list s e (list (list s 'anchor-start e)))))

          ((eq kind 'anchor-end)
           (let ((s (funcall 'neovm--re-new-state))
                 (e (funcall 'neovm--re-new-state)))
             (list s e (list (list s 'anchor-end e)))))

          (t (let ((s (funcall 'neovm--re-new-state)))
               (list s s nil)))))))

  ;; Compile pattern string to NFA
  (fset 'neovm--re-compile
    (lambda (pattern)
      (setq neovm--re-state-counter 0)
      (let ((ast (funcall 'neovm--re-parse pattern)))
        (if ast (funcall 'neovm--re-compile-ast ast)
          (let ((s (funcall 'neovm--re-new-state)))
            (list s s nil))))))

  (unwind-protect
      (list
        ;; Compile "a" -> 2 states, 1 transition
        (let ((nfa (funcall 'neovm--re-compile "a")))
          (list (nth 0 nfa) (nth 1 nfa) (length (nth 2 nfa))))
        ;; Compile "ab" -> states + epsilon link
        (let ((nfa (funcall 'neovm--re-compile "ab")))
          (list (length (nth 2 nfa))))
        ;; Compile "a|b" -> 2 branches with epsilon links
        (let ((nfa (funcall 'neovm--re-compile "a|b")))
          (list (length (nth 2 nfa))))
        ;; Compile "a*" -> star construct
        (let ((nfa (funcall 'neovm--re-compile "a*")))
          (list (length (nth 2 nfa))))
        ;; Compile "a+" -> plus construct
        (let ((nfa (funcall 'neovm--re-compile "a+")))
          (list (length (nth 2 nfa))))
        ;; Compile "a?" -> opt construct
        (let ((nfa (funcall 'neovm--re-compile "a?")))
          (list (length (nth 2 nfa))))
        ;; Complex: (ab|cd)*ef+
        (let ((nfa (funcall 'neovm--re-compile "(ab|cd)*ef+")))
          (list (> (length (nth 2 nfa)) 10))))
    (fmakunbound 'neovm--re-new-state)
    (fmakunbound 'neovm--re-tokenize)
    (fmakunbound 'neovm--re-peek)
    (fmakunbound 'neovm--re-advance)
    (fmakunbound 'neovm--re-parse-atom)
    (fmakunbound 'neovm--re-parse-postfix)
    (fmakunbound 'neovm--re-parse-sequence)
    (fmakunbound 'neovm--re-parse-expr)
    (fmakunbound 'neovm--re-parse)
    (fmakunbound 'neovm--re-compile-ast)
    (fmakunbound 'neovm--re-compile)
    (makunbound 'neovm--re-state-counter)
    (makunbound 'neovm--re-tokens)))"#;
    let expect = expect_test::expect![[r#""OK ((0 1 1) (3) (6) (5) (4) (4) (t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// NFA simulator: run NFA on input string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regex_engine_nfa_simulator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--re-state-counter 0)
  (defvar neovm--re-tokens nil)

  (fset 'neovm--re-new-state
    (lambda () (let ((s neovm--re-state-counter))
                 (setq neovm--re-state-counter (1+ neovm--re-state-counter)) s)))
  (fset 'neovm--re-tokenize
    (lambda (pattern)
      (let ((tokens nil) (i 0) (len (length pattern)))
        (while (< i len)
          (let ((ch (aref pattern i)))
            (cond ((= ch ?\\) (setq i (1+ i))
                   (when (< i len) (setq tokens (cons (cons 'lit (aref pattern i)) tokens))))
                  ((= ch ?.) (setq tokens (cons 'dot tokens)))
                  ((= ch ?*) (setq tokens (cons 'star tokens)))
                  ((= ch ?+) (setq tokens (cons 'plus tokens)))
                  ((= ch ??) (setq tokens (cons 'qmark tokens)))
                  ((= ch ?|) (setq tokens (cons 'pipe tokens)))
                  ((= ch ?\() (setq tokens (cons 'lparen tokens)))
                  ((= ch ?\)) (setq tokens (cons 'rparen tokens)))
                  ((= ch ?^) (setq tokens (cons 'caret tokens)))
                  ((= ch ?$) (setq tokens (cons 'dollar tokens)))
                  (t (setq tokens (cons (cons 'lit ch) tokens)))))
          (setq i (1+ i)))
        (nreverse tokens))))
  (fset 'neovm--re-peek (lambda () (car neovm--re-tokens)))
  (fset 'neovm--re-advance
    (lambda () (let ((t2 (car neovm--re-tokens)))
                 (setq neovm--re-tokens (cdr neovm--re-tokens)) t2)))
  (fset 'neovm--re-parse-atom
    (lambda ()
      (let ((tok (funcall 'neovm--re-peek)))
        (cond ((null tok) nil)
              ((eq tok 'lparen) (funcall 'neovm--re-advance)
               (let ((inner (funcall 'neovm--re-parse-expr)))
                 (funcall 'neovm--re-advance) inner))
              ((eq tok 'dot) (funcall 'neovm--re-advance) '(dot))
              ((eq tok 'caret) (funcall 'neovm--re-advance) '(anchor-start))
              ((eq tok 'dollar) (funcall 'neovm--re-advance) '(anchor-end))
              ((and (consp tok) (eq (car tok) 'lit))
               (funcall 'neovm--re-advance) (list 'lit (cdr tok)))
              (t nil)))))
  (fset 'neovm--re-parse-postfix
    (lambda ()
      (let ((atom (funcall 'neovm--re-parse-atom)))
        (when atom
          (let ((next (funcall 'neovm--re-peek)))
            (cond ((eq next 'star) (funcall 'neovm--re-advance) (list 'star atom))
                  ((eq next 'plus) (funcall 'neovm--re-advance) (list 'plus atom))
                  ((eq next 'qmark) (funcall 'neovm--re-advance) (list 'opt atom))
                  (t atom)))))))
  (fset 'neovm--re-parse-sequence
    (lambda ()
      (let ((first (funcall 'neovm--re-parse-postfix)))
        (if (null first) nil
          (let ((rest (funcall 'neovm--re-parse-postfix)))
            (if (null rest) first
              (let ((result (list 'cat first rest)))
                (let ((next (funcall 'neovm--re-parse-postfix)))
                  (while next (setq result (list 'cat result next))
                         (setq next (funcall 'neovm--re-parse-postfix))))
                result)))))))
  (fset 'neovm--re-parse-expr
    (lambda ()
      (let ((left (funcall 'neovm--re-parse-sequence)))
        (if (eq (funcall 'neovm--re-peek) 'pipe)
            (progn (funcall 'neovm--re-advance)
                   (list 'alt left (funcall 'neovm--re-parse-expr)))
          left))))
  (fset 'neovm--re-parse
    (lambda (pattern) (setq neovm--re-tokens (funcall 'neovm--re-tokenize pattern))
      (funcall 'neovm--re-parse-expr)))

  ;; Compiler
  (fset 'neovm--re-compile-ast
    (lambda (ast)
      (let ((kind (car ast)))
        (cond
          ((eq kind 'lit)
           (let ((s (funcall 'neovm--re-new-state)) (e (funcall 'neovm--re-new-state)))
             (list s e (list (list s (nth 1 ast) e)))))
          ((eq kind 'dot)
           (let ((s (funcall 'neovm--re-new-state)) (e (funcall 'neovm--re-new-state)))
             (list s e (list (list s 'any e)))))
          ((eq kind 'cat)
           (let ((f1 (funcall 'neovm--re-compile-ast (nth 1 ast)))
                 (f2 (funcall 'neovm--re-compile-ast (nth 2 ast))))
             (list (nth 0 f1) (nth 1 f2)
                   (append (nth 2 f1) (nth 2 f2)
                           (list (list (nth 1 f1) 'epsilon (nth 0 f2)))))))
          ((eq kind 'alt)
           (let ((f1 (funcall 'neovm--re-compile-ast (nth 1 ast)))
                 (f2 (funcall 'neovm--re-compile-ast (nth 2 ast)))
                 (s (funcall 'neovm--re-new-state)) (e (funcall 'neovm--re-new-state)))
             (list s e (append (nth 2 f1) (nth 2 f2)
                               (list (list s 'epsilon (nth 0 f1))
                                     (list s 'epsilon (nth 0 f2))
                                     (list (nth 1 f1) 'epsilon e)
                                     (list (nth 1 f2) 'epsilon e))))))
          ((eq kind 'star)
           (let ((f1 (funcall 'neovm--re-compile-ast (nth 1 ast)))
                 (s (funcall 'neovm--re-new-state)) (e (funcall 'neovm--re-new-state)))
             (list s e (append (nth 2 f1)
                               (list (list s 'epsilon (nth 0 f1))
                                     (list s 'epsilon e)
                                     (list (nth 1 f1) 'epsilon (nth 0 f1))
                                     (list (nth 1 f1) 'epsilon e))))))
          ((eq kind 'plus)
           (let ((f1 (funcall 'neovm--re-compile-ast (nth 1 ast)))
                 (s (funcall 'neovm--re-new-state)) (e (funcall 'neovm--re-new-state)))
             (list s e (append (nth 2 f1)
                               (list (list s 'epsilon (nth 0 f1))
                                     (list (nth 1 f1) 'epsilon (nth 0 f1))
                                     (list (nth 1 f1) 'epsilon e))))))
          ((eq kind 'opt)
           (let ((f1 (funcall 'neovm--re-compile-ast (nth 1 ast)))
                 (s (funcall 'neovm--re-new-state)) (e (funcall 'neovm--re-new-state)))
             (list s e (append (nth 2 f1)
                               (list (list s 'epsilon (nth 0 f1))
                                     (list s 'epsilon e)
                                     (list (nth 1 f1) 'epsilon e))))))
          (t (let ((s (funcall 'neovm--re-new-state))) (list s s nil)))))))

  (fset 'neovm--re-compile
    (lambda (pattern) (setq neovm--re-state-counter 0)
      (let ((ast (funcall 'neovm--re-parse pattern)))
        (if ast (funcall 'neovm--re-compile-ast ast)
          (let ((s (funcall 'neovm--re-new-state))) (list s s nil))))))

  ;; NFA simulator
  (fset 'neovm--re-eps-closure
    (lambda (states trans)
      (let ((result (copy-sequence states))
            (worklist (copy-sequence states)))
        (while worklist
          (let ((s (car worklist)))
            (setq worklist (cdr worklist))
            (dolist (tr trans)
              (when (and (= (nth 0 tr) s) (eq (nth 1 tr) 'epsilon))
                (let ((target (nth 2 tr)))
                  (unless (memq target result)
                    (setq result (cons target result))
                    (setq worklist (cons target worklist))))))))
        result)))

  (fset 'neovm--re-move
    (lambda (states ch trans)
      (let ((result nil))
        (dolist (s states)
          (dolist (tr trans)
            (when (and (= (nth 0 tr) s)
                       (or (eql (nth 1 tr) ch)
                           (eq (nth 1 tr) 'any)))
              (unless (memq (nth 2 tr) result)
                (setq result (cons (nth 2 tr) result))))))
        result)))

  ;; Full match: entire string must match
  (fset 'neovm--re-match-full
    (lambda (pattern input)
      (let* ((nfa (funcall 'neovm--re-compile pattern))
             (trans (nth 2 nfa))
             (accept (nth 1 nfa))
             (current (funcall 'neovm--re-eps-closure (list (nth 0 nfa)) trans))
             (i 0) (len (length input)))
        (while (< i len)
          (setq current
                (funcall 'neovm--re-eps-closure
                         (funcall 'neovm--re-move current (aref input i) trans)
                         trans))
          (setq i (1+ i)))
        (if (memq accept current) t nil))))

  (unwind-protect
      (list
        ;; Literal matching
        (funcall 'neovm--re-match-full "abc" "abc")
        (funcall 'neovm--re-match-full "abc" "ab")
        (funcall 'neovm--re-match-full "abc" "abcd")

        ;; Dot
        (funcall 'neovm--re-match-full "a.c" "abc")
        (funcall 'neovm--re-match-full "a.c" "axc")
        (funcall 'neovm--re-match-full "a.c" "ac")
        (funcall 'neovm--re-match-full "..." "xyz")

        ;; Star
        (funcall 'neovm--re-match-full "a*" "")
        (funcall 'neovm--re-match-full "a*" "a")
        (funcall 'neovm--re-match-full "a*" "aaa")
        (funcall 'neovm--re-match-full "a*" "b")
        (funcall 'neovm--re-match-full "a*b" "b")
        (funcall 'neovm--re-match-full "a*b" "aaab")

        ;; Plus
        (funcall 'neovm--re-match-full "a+" "")
        (funcall 'neovm--re-match-full "a+" "a")
        (funcall 'neovm--re-match-full "a+" "aaa")
        (funcall 'neovm--re-match-full "a+b" "b")
        (funcall 'neovm--re-match-full "a+b" "ab")
        (funcall 'neovm--re-match-full "a+b" "aaab")

        ;; Optional
        (funcall 'neovm--re-match-full "ab?c" "ac")
        (funcall 'neovm--re-match-full "ab?c" "abc")
        (funcall 'neovm--re-match-full "ab?c" "abbc")

        ;; Alternation
        (funcall 'neovm--re-match-full "a|b" "a")
        (funcall 'neovm--re-match-full "a|b" "b")
        (funcall 'neovm--re-match-full "a|b" "c")
        (funcall 'neovm--re-match-full "cat|dog" "cat")
        (funcall 'neovm--re-match-full "cat|dog" "dog")
        (funcall 'neovm--re-match-full "cat|dog" "car")

        ;; Grouping
        (funcall 'neovm--re-match-full "(ab)*" "")
        (funcall 'neovm--re-match-full "(ab)*" "ab")
        (funcall 'neovm--re-match-full "(ab)*" "abab")
        (funcall 'neovm--re-match-full "(ab)*" "aba")

        ;; Complex patterns
        (funcall 'neovm--re-match-full "(a|b)*c" "c")
        (funcall 'neovm--re-match-full "(a|b)*c" "abc")
        (funcall 'neovm--re-match-full "(a|b)*c" "bababc")
        (funcall 'neovm--re-match-full "(a|b)*c" "abx"))
    (fmakunbound 'neovm--re-new-state)
    (fmakunbound 'neovm--re-tokenize)
    (fmakunbound 'neovm--re-peek)
    (fmakunbound 'neovm--re-advance)
    (fmakunbound 'neovm--re-parse-atom)
    (fmakunbound 'neovm--re-parse-postfix)
    (fmakunbound 'neovm--re-parse-sequence)
    (fmakunbound 'neovm--re-parse-expr)
    (fmakunbound 'neovm--re-parse)
    (fmakunbound 'neovm--re-compile-ast)
    (fmakunbound 'neovm--re-compile)
    (fmakunbound 'neovm--re-eps-closure)
    (fmakunbound 'neovm--re-move)
    (fmakunbound 'neovm--re-match-full)
    (makunbound 'neovm--re-state-counter)
    (makunbound 'neovm--re-tokens)))"#;
    let expect = expect_test::expect![[
        r#""OK (t nil nil t t nil t t t t nil t t nil t t nil t t t t nil t t nil t t nil t t t nil t t t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Pattern search: find first match within a string (not just full-match)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regex_engine_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--re-state-counter 0)
  (defvar neovm--re-tokens nil)

  ;; All the infrastructure (tokenizer, parser, compiler, simulator)
  (fset 'neovm--re-new-state
    (lambda () (let ((s neovm--re-state-counter))
                 (setq neovm--re-state-counter (1+ neovm--re-state-counter)) s)))
  (fset 'neovm--re-tokenize
    (lambda (pattern)
      (let ((tokens nil) (i 0) (len (length pattern)))
        (while (< i len)
          (let ((ch (aref pattern i)))
            (cond ((= ch ?\\) (setq i (1+ i))
                   (when (< i len) (setq tokens (cons (cons 'lit (aref pattern i)) tokens))))
                  ((= ch ?.) (setq tokens (cons 'dot tokens)))
                  ((= ch ?*) (setq tokens (cons 'star tokens)))
                  ((= ch ?+) (setq tokens (cons 'plus tokens)))
                  ((= ch ??) (setq tokens (cons 'qmark tokens)))
                  ((= ch ?|) (setq tokens (cons 'pipe tokens)))
                  ((= ch ?\() (setq tokens (cons 'lparen tokens)))
                  ((= ch ?\)) (setq tokens (cons 'rparen tokens)))
                  ((= ch ?^) (setq tokens (cons 'caret tokens)))
                  ((= ch ?$) (setq tokens (cons 'dollar tokens)))
                  (t (setq tokens (cons (cons 'lit ch) tokens)))))
          (setq i (1+ i)))
        (nreverse tokens))))
  (fset 'neovm--re-peek (lambda () (car neovm--re-tokens)))
  (fset 'neovm--re-advance
    (lambda () (let ((t2 (car neovm--re-tokens)))
                 (setq neovm--re-tokens (cdr neovm--re-tokens)) t2)))
  (fset 'neovm--re-parse-atom
    (lambda ()
      (let ((tok (funcall 'neovm--re-peek)))
        (cond ((null tok) nil)
              ((eq tok 'lparen) (funcall 'neovm--re-advance)
               (let ((inner (funcall 'neovm--re-parse-expr)))
                 (funcall 'neovm--re-advance) inner))
              ((eq tok 'dot) (funcall 'neovm--re-advance) '(dot))
              ((and (consp tok) (eq (car tok) 'lit))
               (funcall 'neovm--re-advance) (list 'lit (cdr tok)))
              (t nil)))))
  (fset 'neovm--re-parse-postfix
    (lambda ()
      (let ((atom (funcall 'neovm--re-parse-atom)))
        (when atom
          (let ((next (funcall 'neovm--re-peek)))
            (cond ((eq next 'star) (funcall 'neovm--re-advance) (list 'star atom))
                  ((eq next 'plus) (funcall 'neovm--re-advance) (list 'plus atom))
                  ((eq next 'qmark) (funcall 'neovm--re-advance) (list 'opt atom))
                  (t atom)))))))
  (fset 'neovm--re-parse-sequence
    (lambda ()
      (let ((first (funcall 'neovm--re-parse-postfix)))
        (if (null first) nil
          (let ((rest (funcall 'neovm--re-parse-postfix)))
            (if (null rest) first
              (let ((result (list 'cat first rest)))
                (let ((next (funcall 'neovm--re-parse-postfix)))
                  (while next (setq result (list 'cat result next))
                         (setq next (funcall 'neovm--re-parse-postfix))))
                result)))))))
  (fset 'neovm--re-parse-expr
    (lambda ()
      (let ((left (funcall 'neovm--re-parse-sequence)))
        (if (eq (funcall 'neovm--re-peek) 'pipe)
            (progn (funcall 'neovm--re-advance)
                   (list 'alt left (funcall 'neovm--re-parse-expr)))
          left))))
  (fset 'neovm--re-parse
    (lambda (pattern) (setq neovm--re-tokens (funcall 'neovm--re-tokenize pattern))
      (funcall 'neovm--re-parse-expr)))
  (fset 'neovm--re-compile-ast
    (lambda (ast)
      (let ((kind (car ast)))
        (cond
          ((eq kind 'lit)
           (let ((s (funcall 'neovm--re-new-state)) (e (funcall 'neovm--re-new-state)))
             (list s e (list (list s (nth 1 ast) e)))))
          ((eq kind 'dot)
           (let ((s (funcall 'neovm--re-new-state)) (e (funcall 'neovm--re-new-state)))
             (list s e (list (list s 'any e)))))
          ((eq kind 'cat)
           (let ((f1 (funcall 'neovm--re-compile-ast (nth 1 ast)))
                 (f2 (funcall 'neovm--re-compile-ast (nth 2 ast))))
             (list (nth 0 f1) (nth 1 f2)
                   (append (nth 2 f1) (nth 2 f2)
                           (list (list (nth 1 f1) 'epsilon (nth 0 f2)))))))
          ((eq kind 'alt)
           (let ((f1 (funcall 'neovm--re-compile-ast (nth 1 ast)))
                 (f2 (funcall 'neovm--re-compile-ast (nth 2 ast)))
                 (s (funcall 'neovm--re-new-state)) (e (funcall 'neovm--re-new-state)))
             (list s e (append (nth 2 f1) (nth 2 f2)
                               (list (list s 'epsilon (nth 0 f1))
                                     (list s 'epsilon (nth 0 f2))
                                     (list (nth 1 f1) 'epsilon e)
                                     (list (nth 1 f2) 'epsilon e))))))
          ((eq kind 'star)
           (let ((f1 (funcall 'neovm--re-compile-ast (nth 1 ast)))
                 (s (funcall 'neovm--re-new-state)) (e (funcall 'neovm--re-new-state)))
             (list s e (append (nth 2 f1)
                               (list (list s 'epsilon (nth 0 f1)) (list s 'epsilon e)
                                     (list (nth 1 f1) 'epsilon (nth 0 f1))
                                     (list (nth 1 f1) 'epsilon e))))))
          ((eq kind 'plus)
           (let ((f1 (funcall 'neovm--re-compile-ast (nth 1 ast)))
                 (s (funcall 'neovm--re-new-state)) (e (funcall 'neovm--re-new-state)))
             (list s e (append (nth 2 f1)
                               (list (list s 'epsilon (nth 0 f1))
                                     (list (nth 1 f1) 'epsilon (nth 0 f1))
                                     (list (nth 1 f1) 'epsilon e))))))
          ((eq kind 'opt)
           (let ((f1 (funcall 'neovm--re-compile-ast (nth 1 ast)))
                 (s (funcall 'neovm--re-new-state)) (e (funcall 'neovm--re-new-state)))
             (list s e (append (nth 2 f1)
                               (list (list s 'epsilon (nth 0 f1)) (list s 'epsilon e)
                                     (list (nth 1 f1) 'epsilon e))))))
          (t (let ((s (funcall 'neovm--re-new-state))) (list s s nil)))))))
  (fset 'neovm--re-compile
    (lambda (pattern) (setq neovm--re-state-counter 0)
      (let ((ast (funcall 'neovm--re-parse pattern)))
        (if ast (funcall 'neovm--re-compile-ast ast)
          (let ((s (funcall 'neovm--re-new-state))) (list s s nil))))))
  (fset 'neovm--re-eps-closure
    (lambda (states trans)
      (let ((result (copy-sequence states)) (worklist (copy-sequence states)))
        (while worklist
          (let ((s (car worklist)))
            (setq worklist (cdr worklist))
            (dolist (tr trans)
              (when (and (= (nth 0 tr) s) (eq (nth 1 tr) 'epsilon))
                (let ((target (nth 2 tr)))
                  (unless (memq target result)
                    (setq result (cons target result))
                    (setq worklist (cons target worklist))))))))
        result)))
  (fset 'neovm--re-move
    (lambda (states ch trans)
      (let ((result nil))
        (dolist (s states)
          (dolist (tr trans)
            (when (and (= (nth 0 tr) s)
                       (or (eql (nth 1 tr) ch) (eq (nth 1 tr) 'any)))
              (unless (memq (nth 2 tr) result)
                (setq result (cons (nth 2 tr) result))))))
        result)))

  ;; Search: try matching at every position, return (start . end) or nil
  (fset 'neovm--re-search
    (lambda (pattern input)
      "Find first occurrence of PATTERN in INPUT. Return (start . end) or nil."
      (let* ((nfa (funcall 'neovm--re-compile pattern))
             (trans (nth 2 nfa))
             (accept (nth 1 nfa))
             (len (length input))
             (found nil)
             (start-pos 0))
        (while (and (not found) (<= start-pos len))
          (let ((current (funcall 'neovm--re-eps-closure (list (nth 0 nfa)) trans))
                (pos start-pos))
            ;; Check if empty match at start
            (when (memq accept current)
              (setq found (cons start-pos start-pos)))
            ;; Try consuming characters
            (while (and (not found) (< pos len) current)
              (setq current
                    (funcall 'neovm--re-eps-closure
                             (funcall 'neovm--re-move current (aref input pos) trans)
                             trans))
              (setq pos (1+ pos))
              (when (memq accept current)
                (setq found (cons start-pos pos)))))
          (setq start-pos (1+ start-pos)))
        found)))

  ;; Find all non-overlapping matches
  (fset 'neovm--re-find-all
    (lambda (pattern input)
      "Find all non-overlapping matches. Return list of matched substrings."
      (let ((results nil) (pos 0) (len (length input)))
        (while (<= pos len)
          (let* ((nfa (funcall 'neovm--re-compile pattern))
                 (trans (nth 2 nfa))
                 (accept (nth 1 nfa))
                 (current (funcall 'neovm--re-eps-closure (list (nth 0 nfa)) trans))
                 (match-end nil)
                 (i pos))
            ;; Check empty match
            (when (memq accept current)
              (setq match-end pos))
            ;; Try consuming
            (while (and (< i len) current)
              (setq current
                    (funcall 'neovm--re-eps-closure
                             (funcall 'neovm--re-move current (aref input i) trans)
                             trans))
              (setq i (1+ i))
              (when (memq accept current)
                (setq match-end i)))
            (if (and match-end (> match-end pos))
                (progn
                  (setq results (cons (substring input pos match-end) results))
                  (setq pos match-end))
              (setq pos (1+ pos)))))
        (nreverse results))))

  (unwind-protect
      (list
        ;; Search for literal in string
        (funcall 'neovm--re-search "bc" "abcde")
        ;; Search not found
        (funcall 'neovm--re-search "xyz" "abcde")
        ;; Search with dot
        (funcall 'neovm--re-search "a.c" "xabcx")
        ;; Search with star
        (funcall 'neovm--re-search "ab*c" "xacx")
        (funcall 'neovm--re-search "ab*c" "xabbcx")
        ;; Search with plus
        (funcall 'neovm--re-search "a+" "bbaaa")
        ;; Search with alternation
        (funcall 'neovm--re-search "cat|dog" "I have a dog")
        (funcall 'neovm--re-search "cat|dog" "I have a cat")
        ;; Find all: digits (simulated as specific chars)
        (funcall 'neovm--re-find-all "ab" "ababxab")
        ;; Find all with plus
        (funcall 'neovm--re-find-all "a+" "baaabaaab")
        ;; Find all with alternation
        (funcall 'neovm--re-find-all "cat|dog" "catdogcatbird")
        ;; Complex search
        (funcall 'neovm--re-search "(ab)+" "xxababxx"))
    (fmakunbound 'neovm--re-new-state)
    (fmakunbound 'neovm--re-tokenize)
    (fmakunbound 'neovm--re-peek)
    (fmakunbound 'neovm--re-advance)
    (fmakunbound 'neovm--re-parse-atom)
    (fmakunbound 'neovm--re-parse-postfix)
    (fmakunbound 'neovm--re-parse-sequence)
    (fmakunbound 'neovm--re-parse-expr)
    (fmakunbound 'neovm--re-parse)
    (fmakunbound 'neovm--re-compile-ast)
    (fmakunbound 'neovm--re-compile)
    (fmakunbound 'neovm--re-eps-closure)
    (fmakunbound 'neovm--re-move)
    (fmakunbound 'neovm--re-search)
    (fmakunbound 'neovm--re-find-all)
    (makunbound 'neovm--re-state-counter)
    (makunbound 'neovm--re-tokens)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 . 3) nil (1 . 4) (1 . 3) (1 . 5) (2 . 3) (9 . 12) (9 . 12) (\"ab\" \"ab\" \"ab\") (\"aaa\" \"aaa\") (\"cat\" \"dog\" \"cat\") (2 . 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Edge cases and complex patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regex_engine_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--re-state-counter 0)
  (defvar neovm--re-tokens nil)

  ;; Minimal infrastructure for full match
  (fset 'neovm--re-new-state
    (lambda () (let ((s neovm--re-state-counter))
                 (setq neovm--re-state-counter (1+ neovm--re-state-counter)) s)))
  (fset 'neovm--re-tokenize
    (lambda (pattern)
      (let ((tokens nil) (i 0) (len (length pattern)))
        (while (< i len)
          (let ((ch (aref pattern i)))
            (cond ((= ch ?\\) (setq i (1+ i))
                   (when (< i len) (setq tokens (cons (cons 'lit (aref pattern i)) tokens))))
                  ((= ch ?.) (setq tokens (cons 'dot tokens)))
                  ((= ch ?*) (setq tokens (cons 'star tokens)))
                  ((= ch ?+) (setq tokens (cons 'plus tokens)))
                  ((= ch ??) (setq tokens (cons 'qmark tokens)))
                  ((= ch ?|) (setq tokens (cons 'pipe tokens)))
                  ((= ch ?\() (setq tokens (cons 'lparen tokens)))
                  ((= ch ?\)) (setq tokens (cons 'rparen tokens)))
                  (t (setq tokens (cons (cons 'lit ch) tokens)))))
          (setq i (1+ i)))
        (nreverse tokens))))
  (fset 'neovm--re-peek (lambda () (car neovm--re-tokens)))
  (fset 'neovm--re-advance
    (lambda () (let ((t2 (car neovm--re-tokens)))
                 (setq neovm--re-tokens (cdr neovm--re-tokens)) t2)))
  (fset 'neovm--re-parse-atom
    (lambda ()
      (let ((tok (funcall 'neovm--re-peek)))
        (cond ((null tok) nil)
              ((eq tok 'lparen) (funcall 'neovm--re-advance)
               (let ((inner (funcall 'neovm--re-parse-expr)))
                 (funcall 'neovm--re-advance) inner))
              ((eq tok 'dot) (funcall 'neovm--re-advance) '(dot))
              ((and (consp tok) (eq (car tok) 'lit))
               (funcall 'neovm--re-advance) (list 'lit (cdr tok)))
              (t nil)))))
  (fset 'neovm--re-parse-postfix
    (lambda ()
      (let ((atom (funcall 'neovm--re-parse-atom)))
        (when atom
          (let ((next (funcall 'neovm--re-peek)))
            (cond ((eq next 'star) (funcall 'neovm--re-advance) (list 'star atom))
                  ((eq next 'plus) (funcall 'neovm--re-advance) (list 'plus atom))
                  ((eq next 'qmark) (funcall 'neovm--re-advance) (list 'opt atom))
                  (t atom)))))))
  (fset 'neovm--re-parse-sequence
    (lambda ()
      (let ((first (funcall 'neovm--re-parse-postfix)))
        (if (null first) nil
          (let ((rest (funcall 'neovm--re-parse-postfix)))
            (if (null rest) first
              (let ((result (list 'cat first rest)))
                (let ((next (funcall 'neovm--re-parse-postfix)))
                  (while next (setq result (list 'cat result next))
                         (setq next (funcall 'neovm--re-parse-postfix))))
                result)))))))
  (fset 'neovm--re-parse-expr
    (lambda ()
      (let ((left (funcall 'neovm--re-parse-sequence)))
        (if (eq (funcall 'neovm--re-peek) 'pipe)
            (progn (funcall 'neovm--re-advance)
                   (list 'alt left (funcall 'neovm--re-parse-expr)))
          left))))
  (fset 'neovm--re-parse
    (lambda (pattern) (setq neovm--re-tokens (funcall 'neovm--re-tokenize pattern))
      (funcall 'neovm--re-parse-expr)))
  (fset 'neovm--re-compile-ast
    (lambda (ast)
      (let ((kind (car ast)))
        (cond
          ((eq kind 'lit)
           (let ((s (funcall 'neovm--re-new-state)) (e (funcall 'neovm--re-new-state)))
             (list s e (list (list s (nth 1 ast) e)))))
          ((eq kind 'dot)
           (let ((s (funcall 'neovm--re-new-state)) (e (funcall 'neovm--re-new-state)))
             (list s e (list (list s 'any e)))))
          ((eq kind 'cat)
           (let ((f1 (funcall 'neovm--re-compile-ast (nth 1 ast)))
                 (f2 (funcall 'neovm--re-compile-ast (nth 2 ast))))
             (list (nth 0 f1) (nth 1 f2)
                   (append (nth 2 f1) (nth 2 f2)
                           (list (list (nth 1 f1) 'epsilon (nth 0 f2)))))))
          ((eq kind 'alt)
           (let ((f1 (funcall 'neovm--re-compile-ast (nth 1 ast)))
                 (f2 (funcall 'neovm--re-compile-ast (nth 2 ast)))
                 (s (funcall 'neovm--re-new-state)) (e (funcall 'neovm--re-new-state)))
             (list s e (append (nth 2 f1) (nth 2 f2)
                               (list (list s 'epsilon (nth 0 f1))
                                     (list s 'epsilon (nth 0 f2))
                                     (list (nth 1 f1) 'epsilon e)
                                     (list (nth 1 f2) 'epsilon e))))))
          ((eq kind 'star)
           (let ((f1 (funcall 'neovm--re-compile-ast (nth 1 ast)))
                 (s (funcall 'neovm--re-new-state)) (e (funcall 'neovm--re-new-state)))
             (list s e (append (nth 2 f1)
                               (list (list s 'epsilon (nth 0 f1)) (list s 'epsilon e)
                                     (list (nth 1 f1) 'epsilon (nth 0 f1))
                                     (list (nth 1 f1) 'epsilon e))))))
          ((eq kind 'plus)
           (let ((f1 (funcall 'neovm--re-compile-ast (nth 1 ast)))
                 (s (funcall 'neovm--re-new-state)) (e (funcall 'neovm--re-new-state)))
             (list s e (append (nth 2 f1)
                               (list (list s 'epsilon (nth 0 f1))
                                     (list (nth 1 f1) 'epsilon (nth 0 f1))
                                     (list (nth 1 f1) 'epsilon e))))))
          ((eq kind 'opt)
           (let ((f1 (funcall 'neovm--re-compile-ast (nth 1 ast)))
                 (s (funcall 'neovm--re-new-state)) (e (funcall 'neovm--re-new-state)))
             (list s e (append (nth 2 f1)
                               (list (list s 'epsilon (nth 0 f1)) (list s 'epsilon e)
                                     (list (nth 1 f1) 'epsilon e))))))
          (t (let ((s (funcall 'neovm--re-new-state))) (list s s nil)))))))
  (fset 'neovm--re-compile
    (lambda (pattern) (setq neovm--re-state-counter 0)
      (let ((ast (funcall 'neovm--re-parse pattern)))
        (if ast (funcall 'neovm--re-compile-ast ast)
          (let ((s (funcall 'neovm--re-new-state))) (list s s nil))))))
  (fset 'neovm--re-eps-closure
    (lambda (states trans)
      (let ((result (copy-sequence states)) (worklist (copy-sequence states)))
        (while worklist
          (let ((s (car worklist)))
            (setq worklist (cdr worklist))
            (dolist (tr trans)
              (when (and (= (nth 0 tr) s) (eq (nth 1 tr) 'epsilon))
                (let ((target (nth 2 tr)))
                  (unless (memq target result)
                    (setq result (cons target result))
                    (setq worklist (cons target worklist))))))))
        result)))
  (fset 'neovm--re-move
    (lambda (states ch trans)
      (let ((result nil))
        (dolist (s states)
          (dolist (tr trans)
            (when (and (= (nth 0 tr) s)
                       (or (eql (nth 1 tr) ch) (eq (nth 1 tr) 'any)))
              (unless (memq (nth 2 tr) result)
                (setq result (cons (nth 2 tr) result))))))
        result)))
  (fset 'neovm--re-match-full
    (lambda (pattern input)
      (let* ((nfa (funcall 'neovm--re-compile pattern))
             (trans (nth 2 nfa)) (accept (nth 1 nfa))
             (current (funcall 'neovm--re-eps-closure (list (nth 0 nfa)) trans))
             (i 0) (len (length input)))
        (while (< i len)
          (setq current
                (funcall 'neovm--re-eps-closure
                         (funcall 'neovm--re-move current (aref input i) trans) trans))
          (setq i (1+ i)))
        (if (memq accept current) t nil))))

  (unwind-protect
      (list
        ;; Empty pattern matches empty string
        ;; Single char star variations
        (funcall 'neovm--re-match-full "x*" "")
        (funcall 'neovm--re-match-full "x*" "x")
        (funcall 'neovm--re-match-full "x*" "xxxxxx")

        ;; Nested groups
        (funcall 'neovm--re-match-full "((a))" "a")
        (funcall 'neovm--re-match-full "((ab)*c)*" "")
        (funcall 'neovm--re-match-full "((ab)*c)*" "c")
        (funcall 'neovm--re-match-full "((ab)*c)*" "abcc")
        (funcall 'neovm--re-match-full "((ab)*c)*" "ababcababc")

        ;; Multiple alternations
        (funcall 'neovm--re-match-full "a|b|c" "a")
        (funcall 'neovm--re-match-full "a|b|c" "b")
        (funcall 'neovm--re-match-full "a|b|c" "c")
        (funcall 'neovm--re-match-full "a|b|c" "d")

        ;; Star of alternation
        (funcall 'neovm--re-match-full "(a|b|c)*" "")
        (funcall 'neovm--re-match-full "(a|b|c)*" "abcabc")
        (funcall 'neovm--re-match-full "(a|b|c)*" "abcxabc")

        ;; Optional in sequence
        (funcall 'neovm--re-match-full "a?b?c?" "")
        (funcall 'neovm--re-match-full "a?b?c?" "a")
        (funcall 'neovm--re-match-full "a?b?c?" "abc")
        (funcall 'neovm--re-match-full "a?b?c?" "ac")
        (funcall 'neovm--re-match-full "a?b?c?" "bc")

        ;; Dot star (match anything)
        (funcall 'neovm--re-match-full ".*" "")
        (funcall 'neovm--re-match-full ".*" "anything goes here")
        (funcall 'neovm--re-match-full "a.*z" "az")
        (funcall 'neovm--re-match-full "a.*z" "abcdefghijklmnopqrstuvwxyz")
        (funcall 'neovm--re-match-full "a.*z" "a")

        ;; Escaped special characters
        (funcall 'neovm--re-match-full "a\\.b" "a.b")
        (funcall 'neovm--re-match-full "a\\.b" "axb")
        (funcall 'neovm--re-match-full "a\\*b" "a*b")

        ;; Complex real-world-ish pattern: simple email-like
        ;; [a-z]+ simplified as a+
        (funcall 'neovm--re-match-full "a+.a+" "a@a")
        (funcall 'neovm--re-match-full "a+.a+" "aaa@aaa"))
    (fmakunbound 'neovm--re-new-state)
    (fmakunbound 'neovm--re-tokenize)
    (fmakunbound 'neovm--re-peek)
    (fmakunbound 'neovm--re-advance)
    (fmakunbound 'neovm--re-parse-atom)
    (fmakunbound 'neovm--re-parse-postfix)
    (fmakunbound 'neovm--re-parse-sequence)
    (fmakunbound 'neovm--re-parse-expr)
    (fmakunbound 'neovm--re-parse)
    (fmakunbound 'neovm--re-compile-ast)
    (fmakunbound 'neovm--re-compile)
    (fmakunbound 'neovm--re-eps-closure)
    (fmakunbound 'neovm--re-move)
    (fmakunbound 'neovm--re-match-full)
    (makunbound 'neovm--re-state-counter)
    (makunbound 'neovm--re-tokens)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t t t t t t t nil t t nil t t t t t t t t t nil t nil t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Regex engine: practical matching use cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regex_engine_practical() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test the engine on practical-ish patterns, verify consistency
    let form = r#"(progn
  (defvar neovm--re-state-counter 0)
  (defvar neovm--re-tokens nil)

  ;; Full infrastructure (compact)
  (fset 'neovm--re-new-state
    (lambda () (let ((s neovm--re-state-counter))
                 (setq neovm--re-state-counter (1+ neovm--re-state-counter)) s)))
  (fset 'neovm--re-tokenize
    (lambda (pattern)
      (let ((tokens nil) (i 0) (len (length pattern)))
        (while (< i len)
          (let ((ch (aref pattern i)))
            (cond ((= ch ?\\) (setq i (1+ i))
                   (when (< i len) (setq tokens (cons (cons 'lit (aref pattern i)) tokens))))
                  ((= ch ?.) (setq tokens (cons 'dot tokens)))
                  ((= ch ?*) (setq tokens (cons 'star tokens)))
                  ((= ch ?+) (setq tokens (cons 'plus tokens)))
                  ((= ch ??) (setq tokens (cons 'qmark tokens)))
                  ((= ch ?|) (setq tokens (cons 'pipe tokens)))
                  ((= ch ?\() (setq tokens (cons 'lparen tokens)))
                  ((= ch ?\)) (setq tokens (cons 'rparen tokens)))
                  (t (setq tokens (cons (cons 'lit ch) tokens)))))
          (setq i (1+ i)))
        (nreverse tokens))))
  (fset 'neovm--re-peek (lambda () (car neovm--re-tokens)))
  (fset 'neovm--re-advance
    (lambda () (let ((t2 (car neovm--re-tokens)))
                 (setq neovm--re-tokens (cdr neovm--re-tokens)) t2)))
  (fset 'neovm--re-parse-atom
    (lambda ()
      (let ((tok (funcall 'neovm--re-peek)))
        (cond ((null tok) nil)
              ((eq tok 'lparen) (funcall 'neovm--re-advance)
               (let ((inner (funcall 'neovm--re-parse-expr)))
                 (funcall 'neovm--re-advance) inner))
              ((eq tok 'dot) (funcall 'neovm--re-advance) '(dot))
              ((and (consp tok) (eq (car tok) 'lit))
               (funcall 'neovm--re-advance) (list 'lit (cdr tok)))
              (t nil)))))
  (fset 'neovm--re-parse-postfix
    (lambda ()
      (let ((atom (funcall 'neovm--re-parse-atom)))
        (when atom
          (let ((next (funcall 'neovm--re-peek)))
            (cond ((eq next 'star) (funcall 'neovm--re-advance) (list 'star atom))
                  ((eq next 'plus) (funcall 'neovm--re-advance) (list 'plus atom))
                  ((eq next 'qmark) (funcall 'neovm--re-advance) (list 'opt atom))
                  (t atom)))))))
  (fset 'neovm--re-parse-sequence
    (lambda ()
      (let ((first (funcall 'neovm--re-parse-postfix)))
        (if (null first) nil
          (let ((rest (funcall 'neovm--re-parse-postfix)))
            (if (null rest) first
              (let ((result (list 'cat first rest)))
                (let ((next (funcall 'neovm--re-parse-postfix)))
                  (while next (setq result (list 'cat result next))
                         (setq next (funcall 'neovm--re-parse-postfix))))
                result)))))))
  (fset 'neovm--re-parse-expr
    (lambda ()
      (let ((left (funcall 'neovm--re-parse-sequence)))
        (if (eq (funcall 'neovm--re-peek) 'pipe)
            (progn (funcall 'neovm--re-advance)
                   (list 'alt left (funcall 'neovm--re-parse-expr)))
          left))))
  (fset 'neovm--re-parse
    (lambda (pattern) (setq neovm--re-tokens (funcall 'neovm--re-tokenize pattern))
      (funcall 'neovm--re-parse-expr)))
  (fset 'neovm--re-compile-ast
    (lambda (ast)
      (let ((kind (car ast)))
        (cond
          ((eq kind 'lit)
           (let ((s (funcall 'neovm--re-new-state)) (e (funcall 'neovm--re-new-state)))
             (list s e (list (list s (nth 1 ast) e)))))
          ((eq kind 'dot)
           (let ((s (funcall 'neovm--re-new-state)) (e (funcall 'neovm--re-new-state)))
             (list s e (list (list s 'any e)))))
          ((eq kind 'cat)
           (let ((f1 (funcall 'neovm--re-compile-ast (nth 1 ast)))
                 (f2 (funcall 'neovm--re-compile-ast (nth 2 ast))))
             (list (nth 0 f1) (nth 1 f2)
                   (append (nth 2 f1) (nth 2 f2)
                           (list (list (nth 1 f1) 'epsilon (nth 0 f2)))))))
          ((eq kind 'alt)
           (let ((f1 (funcall 'neovm--re-compile-ast (nth 1 ast)))
                 (f2 (funcall 'neovm--re-compile-ast (nth 2 ast)))
                 (s (funcall 'neovm--re-new-state)) (e (funcall 'neovm--re-new-state)))
             (list s e (append (nth 2 f1) (nth 2 f2)
                               (list (list s 'epsilon (nth 0 f1))
                                     (list s 'epsilon (nth 0 f2))
                                     (list (nth 1 f1) 'epsilon e)
                                     (list (nth 1 f2) 'epsilon e))))))
          ((eq kind 'star)
           (let ((f1 (funcall 'neovm--re-compile-ast (nth 1 ast)))
                 (s (funcall 'neovm--re-new-state)) (e (funcall 'neovm--re-new-state)))
             (list s e (append (nth 2 f1)
                               (list (list s 'epsilon (nth 0 f1)) (list s 'epsilon e)
                                     (list (nth 1 f1) 'epsilon (nth 0 f1))
                                     (list (nth 1 f1) 'epsilon e))))))
          ((eq kind 'plus)
           (let ((f1 (funcall 'neovm--re-compile-ast (nth 1 ast)))
                 (s (funcall 'neovm--re-new-state)) (e (funcall 'neovm--re-new-state)))
             (list s e (append (nth 2 f1)
                               (list (list s 'epsilon (nth 0 f1))
                                     (list (nth 1 f1) 'epsilon (nth 0 f1))
                                     (list (nth 1 f1) 'epsilon e))))))
          ((eq kind 'opt)
           (let ((f1 (funcall 'neovm--re-compile-ast (nth 1 ast)))
                 (s (funcall 'neovm--re-new-state)) (e (funcall 'neovm--re-new-state)))
             (list s e (append (nth 2 f1)
                               (list (list s 'epsilon (nth 0 f1)) (list s 'epsilon e)
                                     (list (nth 1 f1) 'epsilon e))))))
          (t (let ((s (funcall 'neovm--re-new-state))) (list s s nil)))))))
  (fset 'neovm--re-compile
    (lambda (pattern) (setq neovm--re-state-counter 0)
      (let ((ast (funcall 'neovm--re-parse pattern)))
        (if ast (funcall 'neovm--re-compile-ast ast)
          (let ((s (funcall 'neovm--re-new-state))) (list s s nil))))))
  (fset 'neovm--re-eps-closure
    (lambda (states trans)
      (let ((result (copy-sequence states)) (worklist (copy-sequence states)))
        (while worklist
          (let ((s (car worklist)))
            (setq worklist (cdr worklist))
            (dolist (tr trans)
              (when (and (= (nth 0 tr) s) (eq (nth 1 tr) 'epsilon))
                (let ((target (nth 2 tr)))
                  (unless (memq target result)
                    (setq result (cons target result))
                    (setq worklist (cons target worklist))))))))
        result)))
  (fset 'neovm--re-move
    (lambda (states ch trans)
      (let ((result nil))
        (dolist (s states)
          (dolist (tr trans)
            (when (and (= (nth 0 tr) s)
                       (or (eql (nth 1 tr) ch) (eq (nth 1 tr) 'any)))
              (unless (memq (nth 2 tr) result)
                (setq result (cons (nth 2 tr) result))))))
        result)))
  (fset 'neovm--re-match-full
    (lambda (pattern input)
      (let* ((nfa (funcall 'neovm--re-compile pattern))
             (trans (nth 2 nfa)) (accept (nth 1 nfa))
             (current (funcall 'neovm--re-eps-closure (list (nth 0 nfa)) trans))
             (i 0) (len (length input)))
        (while (< i len)
          (setq current
                (funcall 'neovm--re-eps-closure
                         (funcall 'neovm--re-move current (aref input i) trans) trans))
          (setq i (1+ i)))
        (if (memq accept current) t nil))))

  ;; Batch test helper: test pattern against multiple inputs
  (fset 'neovm--re-batch-test
    (lambda (pattern inputs)
      (mapcar (lambda (input)
                (cons input (funcall 'neovm--re-match-full pattern input)))
              inputs)))

  (unwind-protect
      (list
        ;; Pattern: "colou?r" (American/British spelling)
        (funcall 'neovm--re-batch-test "colou?r"
                 '("color" "colour" "colur" "colouur"))

        ;; Pattern: "ab(cd|ef)gh" (fixed alternatives in context)
        (funcall 'neovm--re-batch-test "ab(cd|ef)gh"
                 '("abcdgh" "abefgh" "abgh" "abcdefgh"))

        ;; Pattern: "go*gle" (variable o's)
        (funcall 'neovm--re-batch-test "go*gle"
                 '("ggle" "gogle" "google" "gooogle" "goooogle"))

        ;; Pattern: "(ha)+" (laughter)
        (funcall 'neovm--re-batch-test "(ha)+"
                 '("ha" "haha" "hahaha" "h" "hah" ""))

        ;; Pattern: "(..)+" (pairs of any chars)
        (funcall 'neovm--re-batch-test "(..)*"
                 '("" "ab" "abcd" "abcde" "x"))

        ;; Verify associativity: "a|b|c|d" matches all single chars
        (funcall 'neovm--re-batch-test "a|b|c|d"
                 '("a" "b" "c" "d" "e" "ab"))

        ;; Complex: "(a+b)*c?(de|fg)*"
        (funcall 'neovm--re-batch-test "(a+b)*c?(de|fg)*"
                 '("" "c" "de" "fg" "defg"
                   "abcde" "aabab" "aabcfgde" "abx")))
    (fmakunbound 'neovm--re-new-state)
    (fmakunbound 'neovm--re-tokenize)
    (fmakunbound 'neovm--re-peek)
    (fmakunbound 'neovm--re-advance)
    (fmakunbound 'neovm--re-parse-atom)
    (fmakunbound 'neovm--re-parse-postfix)
    (fmakunbound 'neovm--re-parse-sequence)
    (fmakunbound 'neovm--re-parse-expr)
    (fmakunbound 'neovm--re-parse)
    (fmakunbound 'neovm--re-compile-ast)
    (fmakunbound 'neovm--re-compile)
    (fmakunbound 'neovm--re-eps-closure)
    (fmakunbound 'neovm--re-move)
    (fmakunbound 'neovm--re-match-full)
    (fmakunbound 'neovm--re-batch-test)
    (makunbound 'neovm--re-state-counter)
    (makunbound 'neovm--re-tokens)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"color\" . t) (\"colour\" . t) (\"colur\") (\"colouur\")) ((\"abcdgh\" . t) (\"abefgh\" . t) (\"abgh\") (\"abcdefgh\")) ((\"ggle\" . t) (\"gogle\" . t) (\"google\" . t) (\"gooogle\" . t) (\"goooogle\" . t)) ((\"ha\" . t) (\"haha\" . t) (\"hahaha\" . t) (\"h\") (\"hah\") (\"\")) ((\"\" . t) (\"ab\" . t) (\"abcd\" . t) (\"abcde\") (\"x\")) ((\"a\" . t) (\"b\" . t) (\"c\" . t) (\"d\" . t) (\"e\") (\"ab\")) ((\"\" . t) (\"c\" . t) (\"de\" . t) (\"fg\" . t) (\"defg\" . t) (\"abcde\" . t) (\"aabab\" . t) (\"aabcfgde\" . t) (\"abx\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
