//! Advanced oracle parity tests for parser combinator patterns in Elisp:
//! Monadic combinators (bind/return/fail), `many`/`many1` with backtracking,
//! `sepBy`/`endBy`, `chainl1`/`chainr1` for left/right recursive operators,
//! `try` for backtracking, label for error messages, full arithmetic expression
//! parser with precedence, and a JSON-subset parser.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Monadic parser combinators: return, fail, bind
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_parser_combinator_advanced_monadic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parser: (input pos) -> (value . new-pos) | nil
    // return: always succeeds with a given value, consuming no input
    // fail: always fails
    // bind: sequentially compose, passing result to next parser-producing function
    let form = r#"(progn
  ;; Monadic return: succeed with value, consume nothing
  (fset 'neovm--pca-return
    (lambda (val)
      (lambda (input pos) (cons val pos))))

  ;; Monadic fail: always fail
  (fset 'neovm--pca-fail
    (lambda () (lambda (input pos) nil)))

  ;; Monadic bind: run p, pass result to f which produces next parser
  (fset 'neovm--pca-bind
    (lambda (p f)
      (lambda (input pos)
        (let ((r (funcall p input pos)))
          (when r
            (funcall (funcall f (car r)) input (cdr r)))))))

  ;; Char parser: match a specific character
  (fset 'neovm--pca-char
    (lambda (expected)
      (lambda (input pos)
        (when (and (< pos (length input)) (= (aref input pos) expected))
          (cons (char-to-string expected) (1+ pos))))))

  ;; Run parser
  (fset 'neovm--pca-run
    (lambda (p input) (funcall p input 0)))

  (unwind-protect
      (list
        ;; return always succeeds
        (funcall 'neovm--pca-run (funcall 'neovm--pca-return 42) "anything")
        (funcall 'neovm--pca-run (funcall 'neovm--pca-return 'hello) "")
        ;; fail always fails
        (funcall 'neovm--pca-run (funcall 'neovm--pca-fail) "anything")
        ;; bind: parse 'a' then return its uppercase
        (funcall 'neovm--pca-run
          (funcall 'neovm--pca-bind
            (funcall 'neovm--pca-char ?a)
            (lambda (ch) (funcall 'neovm--pca-return (upcase ch))))
          "abc")
        ;; bind chain: parse 'a' then 'b', return concatenation
        (funcall 'neovm--pca-run
          (funcall 'neovm--pca-bind
            (funcall 'neovm--pca-char ?a)
            (lambda (a)
              (funcall 'neovm--pca-bind
                (funcall 'neovm--pca-char ?b)
                (lambda (b)
                  (funcall 'neovm--pca-return (concat a b))))))
          "abc")
        ;; bind with failure in second parser
        (funcall 'neovm--pca-run
          (funcall 'neovm--pca-bind
            (funcall 'neovm--pca-char ?a)
            (lambda (_) (funcall 'neovm--pca-char ?z)))
          "abc")
        ;; Monad law: return a >>= f  ==  f a
        (let* ((f (lambda (x) (funcall 'neovm--pca-return (concat x "!"))))
               (r1 (funcall 'neovm--pca-run
                     (funcall 'neovm--pca-bind (funcall 'neovm--pca-return "hi") f)
                     ""))
               (r2 (funcall 'neovm--pca-run (funcall f "hi") "")))
          (equal r1 r2)))
    (fmakunbound 'neovm--pca-return)
    (fmakunbound 'neovm--pca-fail)
    (fmakunbound 'neovm--pca-bind)
    (fmakunbound 'neovm--pca-char)
    (fmakunbound 'neovm--pca-run)))"#;
    let expect =
        expect_test::expect![[r#""OK ((42 . 0) (hello . 0) nil (\"A\" . 1) (\"ab\" . 2) nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// many1 with backtracking and try combinator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_parser_combinator_advanced_many1_try() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--pcb-char-pred
    (lambda (pred)
      (lambda (input pos)
        (when (and (< pos (length input)) (funcall pred (aref input pos)))
          (cons (char-to-string (aref input pos)) (1+ pos))))))

  (fset 'neovm--pcb-literal
    (lambda (expected)
      (lambda (input pos)
        (let ((end (+ pos (length expected))))
          (when (and (<= end (length input))
                     (string= (substring input pos end) expected))
            (cons expected end))))))

  (fset 'neovm--pcb-or
    (lambda (p1 p2)
      (lambda (input pos)
        (or (funcall p1 input pos) (funcall p2 input pos)))))

  ;; many1: one or more matches
  (fset 'neovm--pcb-many1
    (lambda (p)
      (lambda (input pos)
        (let ((first (funcall p input pos)))
          (when first
            (let ((results (list (car first)))
                  (cp (cdr first))
                  (done nil))
              (while (not done)
                (let ((r (funcall p input cp)))
                  (if r
                      (setq results (cons (car r) results) cp (cdr r))
                    (setq done t))))
              (cons (nreverse results) cp)))))))

  ;; many: zero or more matches
  (fset 'neovm--pcb-many
    (lambda (p)
      (lambda (input pos)
        (let ((results nil) (cp pos) (done nil))
          (while (not done)
            (let ((r (funcall p input cp)))
              (if r
                  (setq results (cons (car r) results) cp (cdr r))
                (setq done t))))
          (cons (nreverse results) cp)))))

  ;; try: attempt parser, restore position on failure (backtracking)
  (fset 'neovm--pcb-try
    (lambda (p)
      (lambda (input pos)
        (funcall p input pos))))

  ;; Labeled parser: on failure, provides error context (returns nil same as fail)
  (fset 'neovm--pcb-label
    (lambda (p name)
      (lambda (input pos)
        (let ((r (funcall p input pos)))
          (or r nil)))))

  (fset 'neovm--pcb-run (lambda (p input) (funcall p input 0)))

  (unwind-protect
      (let ((p-digit (funcall 'neovm--pcb-char-pred
                       (lambda (c) (and (>= c ?0) (<= c ?9)))))
            (p-alpha (funcall 'neovm--pcb-char-pred
                       (lambda (c) (or (and (>= c ?a) (<= c ?z))
                                       (and (>= c ?A) (<= c ?Z)))))))
        (list
          ;; many1 digits
          (funcall 'neovm--pcb-run (funcall 'neovm--pcb-many1 p-digit) "123abc")
          ;; many1 on no match: fail
          (funcall 'neovm--pcb-run (funcall 'neovm--pcb-many1 p-digit) "abc")
          ;; many digits (zero matches ok)
          (funcall 'neovm--pcb-run (funcall 'neovm--pcb-many p-digit) "abc")
          ;; try with backtracking: try "abc" or "ab"
          (funcall 'neovm--pcb-run
            (funcall 'neovm--pcb-or
              (funcall 'neovm--pcb-try (funcall 'neovm--pcb-literal "abc"))
              (funcall 'neovm--pcb-literal "ab"))
            "abd")
          ;; try succeeds on first alternative
          (funcall 'neovm--pcb-run
            (funcall 'neovm--pcb-or
              (funcall 'neovm--pcb-try (funcall 'neovm--pcb-literal "abc"))
              (funcall 'neovm--pcb-literal "ab"))
            "abcdef")
          ;; many1 of (alpha or digit)
          (funcall 'neovm--pcb-run
            (funcall 'neovm--pcb-many1
              (funcall 'neovm--pcb-or p-alpha p-digit))
            "hello42world!")
          ;; label wrapping
          (funcall 'neovm--pcb-run
            (funcall 'neovm--pcb-label p-digit "expected digit")
            "abc")))
    (fmakunbound 'neovm--pcb-char-pred)
    (fmakunbound 'neovm--pcb-literal)
    (fmakunbound 'neovm--pcb-or)
    (fmakunbound 'neovm--pcb-many1)
    (fmakunbound 'neovm--pcb-many)
    (fmakunbound 'neovm--pcb-try)
    (fmakunbound 'neovm--pcb-label)
    (fmakunbound 'neovm--pcb-run)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"1\" \"2\" \"3\") . 3) nil (nil . 0) (\"ab\" . 2) (\"abc\" . 3) ((\"h\" \"e\" \"l\" \"l\" \"o\" \"4\" \"2\" \"w\" \"o\" \"r\" \"l\" \"d\") . 12) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// sepBy and endBy combinators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_parser_combinator_advanced_sep_end_by() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--pcc-char-pred
    (lambda (pred)
      (lambda (input pos)
        (when (and (< pos (length input)) (funcall pred (aref input pos)))
          (cons (char-to-string (aref input pos)) (1+ pos))))))

  (fset 'neovm--pcc-literal
    (lambda (expected)
      (lambda (input pos)
        (let ((end (+ pos (length expected))))
          (when (and (<= end (length input))
                     (string= (substring input pos end) expected))
            (cons expected end))))))

  (fset 'neovm--pcc-many1
    (lambda (p)
      (lambda (input pos)
        (let ((first (funcall p input pos)))
          (when first
            (let ((results (list (car first))) (cp (cdr first)) (done nil))
              (while (not done)
                (let ((r (funcall p input cp)))
                  (if r (setq results (cons (car r) results) cp (cdr r))
                    (setq done t))))
              (cons (nreverse results) cp)))))))

  ;; sepBy: p separated by sep, returns list of p results
  (fset 'neovm--pcc-sepby
    (lambda (p sep)
      (lambda (input pos)
        (let ((first (funcall p input pos)))
          (if (not first)
              ;; Zero elements is ok for sepBy
              (cons nil pos)
            (let ((results (list (car first)))
                  (cp (cdr first))
                  (done nil))
              (while (not done)
                (let ((sep-r (funcall sep input cp)))
                  (if sep-r
                      (let ((next (funcall p input (cdr sep-r))))
                        (if next
                            (setq results (cons (car next) results)
                                  cp (cdr next))
                          (setq done t)))
                    (setq done t))))
              (cons (nreverse results) cp)))))))

  ;; sepBy1: at least one
  (fset 'neovm--pcc-sepby1
    (lambda (p sep)
      (lambda (input pos)
        (let ((first (funcall p input pos)))
          (when first
            (let ((results (list (car first)))
                  (cp (cdr first))
                  (done nil))
              (while (not done)
                (let ((sep-r (funcall sep input cp)))
                  (if sep-r
                      (let ((next (funcall p input (cdr sep-r))))
                        (if next
                            (setq results (cons (car next) results)
                                  cp (cdr next))
                          (setq done t)))
                    (setq done t))))
              (cons (nreverse results) cp)))))))

  ;; endBy: p followed by sep, repeated
  (fset 'neovm--pcc-endby
    (lambda (p sep)
      (lambda (input pos)
        (let ((results nil) (cp pos) (done nil))
          (while (not done)
            (let ((val-r (funcall p input cp)))
              (if val-r
                  (let ((sep-r (funcall sep input (cdr val-r))))
                    (if sep-r
                        (setq results (cons (car val-r) results)
                              cp (cdr sep-r))
                      (setq done t)))
                (setq done t))))
          (cons (nreverse results) cp)))))

  (fset 'neovm--pcc-run (lambda (p input) (funcall p input 0)))

  (unwind-protect
      (let* ((p-digit (funcall 'neovm--pcc-char-pred
                        (lambda (c) (and (>= c ?0) (<= c ?9)))))
             (p-word (funcall 'neovm--pcc-many1
                       (funcall 'neovm--pcc-char-pred
                         (lambda (c) (and (>= c ?a) (<= c ?z))))))
             ;; Word parser that joins chars
             (p-word-str (lambda (input pos)
                           (let ((r (funcall p-word input pos)))
                             (when r (cons (apply 'concat (car r)) (cdr r))))))
             (p-comma (funcall 'neovm--pcc-literal ","))
             (p-semi (funcall 'neovm--pcc-literal ";")))
        (list
          ;; sepBy: words separated by commas
          (funcall 'neovm--pcc-run
            (funcall 'neovm--pcc-sepby p-word-str p-comma)
            "hello,world,foo")
          ;; sepBy: single element
          (funcall 'neovm--pcc-run
            (funcall 'neovm--pcc-sepby p-word-str p-comma)
            "alone")
          ;; sepBy: empty input
          (funcall 'neovm--pcc-run
            (funcall 'neovm--pcc-sepby p-word-str p-comma)
            "")
          ;; sepBy1: fails on empty
          (funcall 'neovm--pcc-run
            (funcall 'neovm--pcc-sepby1 p-word-str p-comma)
            "")
          ;; endBy: words ended by semicolons
          (funcall 'neovm--pcc-run
            (funcall 'neovm--pcc-endby p-word-str p-semi)
            "hello;world;foo;")
          ;; endBy: last element without separator not consumed
          (funcall 'neovm--pcc-run
            (funcall 'neovm--pcc-endby p-word-str p-semi)
            "hello;world;foo")
          ;; endBy: empty input
          (funcall 'neovm--pcc-run
            (funcall 'neovm--pcc-endby p-word-str p-semi)
            "")))
    (fmakunbound 'neovm--pcc-char-pred)
    (fmakunbound 'neovm--pcc-literal)
    (fmakunbound 'neovm--pcc-many1)
    (fmakunbound 'neovm--pcc-sepby)
    (fmakunbound 'neovm--pcc-sepby1)
    (fmakunbound 'neovm--pcc-endby)
    (fmakunbound 'neovm--pcc-run)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"hello\" \"world\" \"foo\") . 15) ((\"alone\") . 5) (nil . 0) nil ((\"hello\" \"world\" \"foo\") . 16) ((\"hello\" \"world\") . 12) (nil . 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// chainl1: left-associative operator chaining
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_parser_combinator_advanced_chainl1() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--pcd-skip-ws
    (lambda (input pos)
      (while (and (< pos (length input)) (= (aref input pos) ?\s))
        (setq pos (1+ pos)))
      pos))

  ;; Parse a natural number
  (fset 'neovm--pcd-number
    (lambda (input pos)
      (setq pos (funcall 'neovm--pcd-skip-ws input pos))
      (let ((start pos))
        (while (and (< pos (length input))
                    (>= (aref input pos) ?0) (<= (aref input pos) ?9))
          (setq pos (1+ pos)))
        (when (> pos start)
          (cons (string-to-number (substring input start pos)) pos)))))

  ;; chainl1: parse one or more p separated by op, fold left
  ;; op returns a binary function
  (fset 'neovm--pcd-chainl1
    (lambda (p op)
      (lambda (input pos)
        (let ((left (funcall p input pos)))
          (when left
            (let ((val (car left)) (cp (cdr left)) (done nil))
              (while (not done)
                (let ((op-r (funcall op input cp)))
                  (if op-r
                      (let ((right (funcall p input (cdr op-r))))
                        (if right
                            (setq val (funcall (car op-r) val (car right))
                                  cp (cdr right))
                          (setq done t)))
                    (setq done t))))
              (cons val cp)))))))

  ;; chainr1: parse one or more p separated by op, fold right
  (fset 'neovm--pcd-chainr1
    (lambda (p op)
      (lambda (input pos)
        (let ((first (funcall p input pos)))
          (when first
            ;; Collect all values and operators
            (let ((vals (list (car first)))
                  (ops nil)
                  (cp (cdr first))
                  (done nil))
              (while (not done)
                (let ((op-r (funcall op input cp)))
                  (if op-r
                      (let ((next (funcall p input (cdr op-r))))
                        (if next
                            (setq vals (cons (car next) vals)
                                  ops (cons (car op-r) ops)
                                  cp (cdr next))
                          (setq done t)))
                    (setq done t))))
              ;; Fold right: vals and ops are in reverse order
              (let ((result (car vals))
                    (rest-vals (cdr vals))
                    (rest-ops ops))
                (while rest-vals
                  (setq result (funcall (car rest-ops) (car rest-vals) result)
                        rest-vals (cdr rest-vals)
                        rest-ops (cdr rest-ops)))
                (cons result cp))))))))

  ;; Operator parsers: return a function
  (fset 'neovm--pcd-addop
    (lambda (input pos)
      (setq pos (funcall 'neovm--pcd-skip-ws input pos))
      (when (< pos (length input))
        (cond
          ((= (aref input pos) ?+) (cons (lambda (a b) (+ a b)) (1+ pos)))
          ((= (aref input pos) ?-) (cons (lambda (a b) (- a b)) (1+ pos)))))))

  (fset 'neovm--pcd-mulop
    (lambda (input pos)
      (setq pos (funcall 'neovm--pcd-skip-ws input pos))
      (when (< pos (length input))
        (cond
          ((= (aref input pos) ?*) (cons (lambda (a b) (* a b)) (1+ pos)))
          ((= (aref input pos) ?/) (cons (lambda (a b) (/ a b)) (1+ pos)))))))

  ;; Power operator (right-associative)
  (fset 'neovm--pcd-powop
    (lambda (input pos)
      (setq pos (funcall 'neovm--pcd-skip-ws input pos))
      (when (and (< pos (length input)) (= (aref input pos) ?^))
        (cons (lambda (a b) (expt a b)) (1+ pos)))))

  (fset 'neovm--pcd-run (lambda (p input) (funcall p input 0)))

  (unwind-protect
      (list
        ;; chainl1 with addition: left-associative
        ;; 1 + 2 + 3 = (1+2)+3 = 6
        (funcall 'neovm--pcd-run
          (funcall 'neovm--pcd-chainl1 'neovm--pcd-number 'neovm--pcd-addop)
          "1 + 2 + 3")
        ;; chainl1 with subtraction: left-associative
        ;; 10 - 3 - 2 = (10-3)-2 = 5
        (funcall 'neovm--pcd-run
          (funcall 'neovm--pcd-chainl1 'neovm--pcd-number 'neovm--pcd-addop)
          "10 - 3 - 2")
        ;; chainl1 with multiplication
        ;; 2 * 3 * 4 = 24
        (funcall 'neovm--pcd-run
          (funcall 'neovm--pcd-chainl1 'neovm--pcd-number 'neovm--pcd-mulop)
          "2 * 3 * 4")
        ;; chainr1 with power: right-associative
        ;; 2 ^ 3 ^ 2 = 2^(3^2) = 2^9 = 512
        (funcall 'neovm--pcd-run
          (funcall 'neovm--pcd-chainr1 'neovm--pcd-number 'neovm--pcd-powop)
          "2 ^ 3 ^ 2")
        ;; Single element: no operator
        (funcall 'neovm--pcd-run
          (funcall 'neovm--pcd-chainl1 'neovm--pcd-number 'neovm--pcd-addop)
          "42")
        ;; Division left-assoc: 100 / 5 / 4 = (100/5)/4 = 5
        (funcall 'neovm--pcd-run
          (funcall 'neovm--pcd-chainl1 'neovm--pcd-number 'neovm--pcd-mulop)
          "100 / 5 / 4")
        ;; Mixed addition and subtraction
        ;; 10 + 5 - 3 + 1 = 13
        (funcall 'neovm--pcd-run
          (funcall 'neovm--pcd-chainl1 'neovm--pcd-number 'neovm--pcd-addop)
          "10 + 5 - 3 + 1"))
    (fmakunbound 'neovm--pcd-skip-ws)
    (fmakunbound 'neovm--pcd-number)
    (fmakunbound 'neovm--pcd-chainl1)
    (fmakunbound 'neovm--pcd-chainr1)
    (fmakunbound 'neovm--pcd-addop)
    (fmakunbound 'neovm--pcd-mulop)
    (fmakunbound 'neovm--pcd-powop)
    (fmakunbound 'neovm--pcd-run)))"#;
    let expect = expect_test::expect![[
        r#""OK ((6 . 9) (5 . 10) (24 . 9) (512 . 9) (42 . 2) (5 . 11) (13 . 14))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Full arithmetic expression parser with proper precedence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_parser_combinator_advanced_full_arith() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Uses chainl1 for proper operator precedence:
    //   expr = term chainl1 ('+' | '-')
    //   term = factor chainl1 ('*' | '/')
    //   factor = number | '(' expr ')'
    let form = r#"(progn
  (fset 'neovm--pce-ws
    (lambda (input pos)
      (while (and (< pos (length input)) (= (aref input pos) ?\s))
        (setq pos (1+ pos)))
      pos))

  (fset 'neovm--pce-number
    (lambda (input pos)
      (setq pos (funcall 'neovm--pce-ws input pos))
      (let ((start pos))
        (while (and (< pos (length input)) (>= (aref input pos) ?0) (<= (aref input pos) ?9))
          (setq pos (1+ pos)))
        (when (> pos start)
          (cons (string-to-number (substring input start pos)) pos)))))

  (defvar neovm--pce-expr nil)

  (fset 'neovm--pce-factor
    (lambda (input pos)
      (setq pos (funcall 'neovm--pce-ws input pos))
      (if (and (< pos (length input)) (= (aref input pos) ?\())
          (let ((inner (funcall neovm--pce-expr input (1+ pos))))
            (when inner
              (let ((cp (funcall 'neovm--pce-ws input (cdr inner))))
                (when (and (< cp (length input)) (= (aref input cp) ?\)))
                  (cons (car inner) (1+ cp))))))
        (funcall 'neovm--pce-number input pos))))

  ;; term = factor chainl1 ('*'|'/')
  (fset 'neovm--pce-term
    (lambda (input pos)
      (let ((left (funcall 'neovm--pce-factor input pos)))
        (when left
          (let ((val (car left)) (cp (cdr left)) (done nil))
            (while (not done)
              (setq cp (funcall 'neovm--pce-ws input cp))
              (if (and (< cp (length input)) (memq (aref input cp) '(?* ?/)))
                  (let ((op (aref input cp))
                        (right (funcall 'neovm--pce-factor input (1+ cp))))
                    (if right
                        (progn
                          (setq val (if (= op ?*) (* val (car right)) (/ val (car right))))
                          (setq cp (cdr right)))
                      (setq done t)))
                (setq done t)))
            (cons val cp))))))

  ;; expr = term chainl1 ('+'|'-')
  (setq neovm--pce-expr
    (lambda (input pos)
      (let ((left (funcall 'neovm--pce-term input pos)))
        (when left
          (let ((val (car left)) (cp (cdr left)) (done nil))
            (while (not done)
              (setq cp (funcall 'neovm--pce-ws input cp))
              (if (and (< cp (length input)) (memq (aref input cp) '(?+ ?-)))
                  (let ((op (aref input cp))
                        (right (funcall 'neovm--pce-term input (1+ cp))))
                    (if right
                        (progn
                          (setq val (if (= op ?+) (+ val (car right)) (- val (car right))))
                          (setq cp (cdr right)))
                      (setq done t)))
                (setq done t)))
            (cons val cp))))))

  (fset 'neovm--pce-eval
    (lambda (s) (let ((r (funcall neovm--pce-expr s 0))) (when r (car r)))))

  (unwind-protect
      (list
        (funcall 'neovm--pce-eval "1 + 2 * 3")          ;; 7
        (funcall 'neovm--pce-eval "(1 + 2) * 3")        ;; 9
        (funcall 'neovm--pce-eval "10 - 2 - 3")         ;; 5
        (funcall 'neovm--pce-eval "2 * (3 + 4) * 5")    ;; 70
        (funcall 'neovm--pce-eval "100 / 10 / 2")       ;; 5
        (funcall 'neovm--pce-eval "((((5))))")           ;; 5
        (funcall 'neovm--pce-eval "1 + 2 + 3 + 4 + 5")  ;; 15
        (funcall 'neovm--pce-eval "2 * 3 + 4 * 5 - 1")  ;; 25
        (funcall 'neovm--pce-eval "0")                   ;; 0
        (funcall 'neovm--pce-eval "(2 + 3) * (7 - 2)")) ;; 25
    (fmakunbound 'neovm--pce-ws)
    (fmakunbound 'neovm--pce-number)
    (fmakunbound 'neovm--pce-factor)
    (fmakunbound 'neovm--pce-term)
    (fmakunbound 'neovm--pce-eval)
    (makunbound 'neovm--pce-expr)))"#;
    let expect = expect_test::expect![[r#""OK (7 9 5 70 5 5 15 25 0 25)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// JSON-subset parser: numbers, strings, booleans, null, arrays, objects
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_parser_combinator_advanced_json_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--pcf-ws
    (lambda (input pos)
      (while (and (< pos (length input))
                  (memq (aref input pos) '(?\s ?\t ?\n ?\r)))
        (setq pos (1+ pos)))
      pos))

  ;; Parse JSON number (integers only for simplicity)
  (fset 'neovm--pcf-number
    (lambda (input pos)
      (setq pos (funcall 'neovm--pcf-ws input pos))
      (let ((start pos) (neg nil))
        (when (and (< pos (length input)) (= (aref input pos) ?-))
          (setq neg t pos (1+ pos)))
        (let ((cp pos))
          (while (and (< cp (length input)) (>= (aref input cp) ?0) (<= (aref input cp) ?9))
            (setq cp (1+ cp)))
          (when (> cp pos)
            (cons (string-to-number (substring input start cp)) cp))))))

  ;; Parse JSON string (no escape sequences for simplicity)
  (fset 'neovm--pcf-string
    (lambda (input pos)
      (setq pos (funcall 'neovm--pcf-ws input pos))
      (when (and (< pos (length input)) (= (aref input pos) ?\"))
        (setq pos (1+ pos))
        (let ((start pos))
          (while (and (< pos (length input)) (/= (aref input pos) ?\"))
            (setq pos (1+ pos)))
          (when (and (< pos (length input)) (= (aref input pos) ?\"))
            (cons (substring input start pos) (1+ pos)))))))

  ;; Parse JSON literal (true, false, null)
  (fset 'neovm--pcf-literal
    (lambda (expected val)
      (lambda (input pos)
        (setq pos (funcall 'neovm--pcf-ws input pos))
        (let ((end (+ pos (length expected))))
          (when (and (<= end (length input))
                     (string= (substring input pos end) expected))
            (cons val end))))))

  (defvar neovm--pcf-value nil)

  ;; Parse JSON array
  (fset 'neovm--pcf-array
    (lambda (input pos)
      (setq pos (funcall 'neovm--pcf-ws input pos))
      (when (and (< pos (length input)) (= (aref input pos) ?\[))
        (setq pos (1+ pos))
        (setq pos (funcall 'neovm--pcf-ws input pos))
        (if (and (< pos (length input)) (= (aref input pos) ?\]))
            ;; Empty array
            (cons (vector) (1+ pos))
          ;; Non-empty: first element
          (let ((first (funcall neovm--pcf-value input pos)))
            (when first
              (let ((items (list (car first)))
                    (cp (cdr first))
                    (done nil))
                (while (not done)
                  (setq cp (funcall 'neovm--pcf-ws input cp))
                  (if (and (< cp (length input)) (= (aref input cp) ?,))
                      (let ((next (funcall neovm--pcf-value input (1+ cp))))
                        (if next
                            (setq items (cons (car next) items)
                                  cp (cdr next))
                          (setq done t)))
                    (setq done t)))
                (setq cp (funcall 'neovm--pcf-ws input cp))
                (when (and (< cp (length input)) (= (aref input cp) ?\]))
                  (cons (vconcat (nreverse items)) (1+ cp))))))))))

  ;; Parse JSON object
  (fset 'neovm--pcf-object
    (lambda (input pos)
      (setq pos (funcall 'neovm--pcf-ws input pos))
      (when (and (< pos (length input)) (= (aref input pos) ?\{))
        (setq pos (1+ pos))
        (setq pos (funcall 'neovm--pcf-ws input pos))
        (if (and (< pos (length input)) (= (aref input pos) ?\}))
            ;; Empty object
            (cons nil (1+ pos))
          ;; Non-empty: key-value pairs
          (let ((pairs nil) (cp pos) (done nil) (first t))
            (while (not done)
              (when (not first)
                (setq cp (funcall 'neovm--pcf-ws input cp))
                (if (and (< cp (length input)) (= (aref input cp) ?,))
                    (setq cp (1+ cp))
                  (setq done t)))
              (unless done
                (let ((key (funcall 'neovm--pcf-string input cp)))
                  (if key
                      (progn
                        (setq cp (funcall 'neovm--pcf-ws input (cdr key)))
                        (if (and (< cp (length input)) (= (aref input cp) ?:))
                            (let ((val (funcall neovm--pcf-value input (1+ cp))))
                              (if val
                                  (setq pairs (cons (cons (car key) (car val)) pairs)
                                        cp (cdr val)
                                        first nil)
                                (setq done t)))
                          (setq done t)))
                    (setq done t)))))
            (setq cp (funcall 'neovm--pcf-ws input cp))
            (when (and (< cp (length input)) (= (aref input cp) ?\}))
              (cons (nreverse pairs) (1+ cp))))))))

  ;; Value: string | number | true | false | null | array | object
  (let ((p-true (funcall 'neovm--pcf-literal "true" t))
        (p-false (funcall 'neovm--pcf-literal "false" :json-false))
        (p-null (funcall 'neovm--pcf-literal "null" :json-null)))
    (setq neovm--pcf-value
      (lambda (input pos)
        (or (funcall 'neovm--pcf-string input pos)
            (funcall 'neovm--pcf-number input pos)
            (funcall p-true input pos)
            (funcall p-false input pos)
            (funcall p-null input pos)
            (funcall 'neovm--pcf-array input pos)
            (funcall 'neovm--pcf-object input pos)))))

  (fset 'neovm--pcf-parse
    (lambda (input)
      (let ((r (funcall neovm--pcf-value input 0)))
        (when r (car r)))))

  (unwind-protect
      (list
        ;; Numbers
        (funcall 'neovm--pcf-parse "42")
        (funcall 'neovm--pcf-parse "-7")
        ;; Strings
        (funcall 'neovm--pcf-parse "\"hello world\"")
        ;; Booleans and null
        (funcall 'neovm--pcf-parse "true")
        (funcall 'neovm--pcf-parse "false")
        (funcall 'neovm--pcf-parse "null")
        ;; Arrays
        (funcall 'neovm--pcf-parse "[1, 2, 3]")
        (funcall 'neovm--pcf-parse "[]")
        (funcall 'neovm--pcf-parse "[\"a\", 1, true, null]")
        ;; Nested arrays
        (funcall 'neovm--pcf-parse "[[1, 2], [3, 4]]")
        ;; Objects
        (funcall 'neovm--pcf-parse "{\"name\": \"Alice\", \"age\": 30}")
        (funcall 'neovm--pcf-parse "{}")
        ;; Nested: object with array value
        (funcall 'neovm--pcf-parse "{\"items\": [1, 2, 3], \"ok\": true}"))
    (fmakunbound 'neovm--pcf-ws)
    (fmakunbound 'neovm--pcf-number)
    (fmakunbound 'neovm--pcf-string)
    (fmakunbound 'neovm--pcf-literal)
    (fmakunbound 'neovm--pcf-array)
    (fmakunbound 'neovm--pcf-object)
    (fmakunbound 'neovm--pcf-parse)
    (makunbound 'neovm--pcf-value)))"#;
    let expect = expect_test::expect![[
        r#""OK (42 -7 \"hello world\" t :json-false :json-null [1 2 3] [] [\"a\" 1 t :json-null] [[1 2] [3 4]] ((\"name\" . \"Alice\") (\"age\" . 30)) nil ((\"items\" . [1 2 3]) (\"ok\" . t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: parser with error accumulation and position tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_parser_combinator_advanced_error_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parsers that track the furthest position reached for error reporting
    let form = r#"(progn
  ;; Parser result is (value pos furthest-pos) on success
  ;; or ('error pos expected-label) on failure
  (fset 'neovm--pcg-char
    (lambda (expected label)
      (lambda (input pos)
        (if (and (< pos (length input)) (= (aref input pos) expected))
            (list (char-to-string expected) (1+ pos) (1+ pos))
          (list 'error pos label)))))

  (fset 'neovm--pcg-or
    (lambda (p1 p2)
      (lambda (input pos)
        (let ((r1 (funcall p1 input pos)))
          (if (not (eq (car r1) 'error))
              r1
            (let ((r2 (funcall p2 input pos)))
              (if (not (eq (car r2) 'error))
                  r2
                ;; Return error with furthest position
                (if (>= (cadr r1) (cadr r2)) r1 r2))))))))

  (fset 'neovm--pcg-seq
    (lambda (p1 p2)
      (lambda (input pos)
        (let ((r1 (funcall p1 input pos)))
          (if (eq (car r1) 'error)
              r1
            (let ((r2 (funcall p2 input (cadr r1))))
              (if (eq (car r2) 'error)
                  r2
                (list (concat (car r1) (car r2)) (cadr r2)
                      (max (caddr r1) (caddr r2))))))))))

  (fset 'neovm--pcg-run
    (lambda (p input) (funcall p input 0)))

  (unwind-protect
      (let ((p-a (funcall 'neovm--pcg-char ?a "expected 'a'"))
            (p-b (funcall 'neovm--pcg-char ?b "expected 'b'"))
            (p-c (funcall 'neovm--pcg-char ?c "expected 'c'")))
        (list
          ;; Success: "ab"
          (funcall 'neovm--pcg-run
            (funcall 'neovm--pcg-seq p-a p-b) "abc")
          ;; Failure with error position
          (funcall 'neovm--pcg-run
            (funcall 'neovm--pcg-seq p-a p-c) "abc")
          ;; Or: first fails, second succeeds
          (funcall 'neovm--pcg-run
            (funcall 'neovm--pcg-or p-b p-a) "abc")
          ;; Or: both fail, report furthest
          (funcall 'neovm--pcg-run
            (funcall 'neovm--pcg-or
              (funcall 'neovm--pcg-seq p-a p-c)
              p-b)
            "axyz")
          ;; Complex: (a then b) or (a then c)
          (funcall 'neovm--pcg-run
            (funcall 'neovm--pcg-or
              (funcall 'neovm--pcg-seq p-a p-b)
              (funcall 'neovm--pcg-seq p-a p-c))
            "acx")))
    (fmakunbound 'neovm--pcg-char)
    (fmakunbound 'neovm--pcg-or)
    (fmakunbound 'neovm--pcg-seq)
    (fmakunbound 'neovm--pcg-run)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"ab\" 2 2) (error 1 \"expected 'c'\") (\"a\" 1 1) (error 1 \"expected 'c'\") (\"ac\" 2 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
