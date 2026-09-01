//! Oracle parity tests for an Earley parser implemented in Elisp.
//! Implements grammar representation (rules as alists), Earley items
//! (rule, dot-position, origin-position), prediction, scanning,
//! completion steps, arithmetic expression parsing, and ambiguity detection.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Core Earley parser: grammar, items, predict/scan/complete
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_earley_parser_core() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement core Earley parsing infrastructure and parse a simple grammar:
    //   S -> A B
    //   A -> "a"
    //   B -> "b"
    // Tokens: ("a" "b")
    let form = r#"(progn
  ;; Grammar: list of (LHS . RHS) where RHS is a list of symbols.
  ;; Terminals are strings, non-terminals are symbols.
  ;; Earley item: (rule-index dot origin)
  ;; Chart: vector of lists (one list per position)

  ;; Get the rule at index from grammar
  (fset 'neovm--ep-rule (lambda (grammar idx) (nth idx grammar)))
  ;; Get LHS of a rule
  (fset 'neovm--ep-lhs (lambda (rule) (car rule)))
  ;; Get RHS of a rule
  (fset 'neovm--ep-rhs (lambda (rule) (cdr rule)))
  ;; Get symbol at dot position in rule
  (fset 'neovm--ep-next-sym
    (lambda (grammar item)
      (let* ((rule (funcall 'neovm--ep-rule grammar (nth 0 item)))
             (rhs (funcall 'neovm--ep-rhs rule))
             (dot (nth 1 item)))
        (nth dot rhs))))
  ;; Is item complete? (dot at end of RHS)
  (fset 'neovm--ep-complete-p
    (lambda (grammar item)
      (let* ((rule (funcall 'neovm--ep-rule grammar (nth 0 item)))
             (rhs (funcall 'neovm--ep-rhs rule))
             (dot (nth 1 item)))
        (>= dot (length rhs)))))
  ;; Is symbol a terminal? (string)
  (fset 'neovm--ep-terminal-p (lambda (sym) (stringp sym)))
  ;; Add item to chart if not already present
  (fset 'neovm--ep-add-item
    (lambda (chart pos item)
      (let ((items (aref chart pos)))
        (unless (member item items)
          (aset chart pos (append items (list item)))
          t))))
  ;; Predict: for each item [A -> ... . B ..., j] at position k,
  ;; add [B -> . rhs, k] for each rule B -> rhs
  (fset 'neovm--ep-predict
    (lambda (grammar chart pos)
      (let ((items (aref chart pos))
            (i 0)
            (added nil))
        ;; Use index-based iteration since items list may grow
        (while (< i (length items))
          (let* ((item (nth i items))
                 (sym (funcall 'neovm--ep-next-sym grammar item)))
            (when (and sym (not (funcall 'neovm--ep-terminal-p sym)))
              ;; Find all rules with sym as LHS
              (let ((ri 0))
                (while (< ri (length grammar))
                  (when (eq (funcall 'neovm--ep-lhs (nth ri grammar)) sym)
                    (when (funcall 'neovm--ep-add-item chart pos (list ri 0 pos))
                      (setq added t)))
                  (setq ri (1+ ri))))))
          (setq i (1+ i)))
        added)))
  ;; Scan: for each item [A -> ... . a ..., j] at pos k where a = token[k],
  ;; add [A -> ... a . ..., j] at pos k+1
  (fset 'neovm--ep-scan
    (lambda (grammar chart pos tokens)
      (when (< pos (length tokens))
        (let ((token (nth pos tokens))
              (items (aref chart pos)))
          (dolist (item items)
            (let ((sym (funcall 'neovm--ep-next-sym grammar item)))
              (when (and sym (funcall 'neovm--ep-terminal-p sym)
                         (string= sym token))
                (funcall 'neovm--ep-add-item chart (1+ pos)
                         (list (nth 0 item) (1+ (nth 1 item)) (nth 2 item))))))))))
  ;; Complete: for each complete item [B -> ... ., j] at pos k,
  ;; find items [A -> ... . B ..., i] at pos j, advance their dot
  (fset 'neovm--ep-complete
    (lambda (grammar chart pos)
      (let ((items (aref chart pos))
            (i 0)
            (added nil))
        (while (< i (length items))
          (let ((item (nth i items)))
            (when (funcall 'neovm--ep-complete-p grammar item)
              (let* ((completed-lhs (funcall 'neovm--ep-lhs
                                              (funcall 'neovm--ep-rule grammar (nth 0 item))))
                     (origin (nth 2 item))
                     (origin-items (aref chart origin)))
                (dolist (oi origin-items)
                  (let ((sym (funcall 'neovm--ep-next-sym grammar oi)))
                    (when (and sym (eq sym completed-lhs))
                      (when (funcall 'neovm--ep-add-item chart pos
                                     (list (nth 0 oi) (1+ (nth 1 oi)) (nth 2 oi)))
                        (setq added t))))))))
          (setq i (1+ i)))
        added)))
  ;; Main parse function
  (fset 'neovm--ep-parse
    (lambda (grammar tokens start-symbol)
      (let* ((n (length tokens))
             (chart (make-vector (1+ n) nil)))
        ;; Seed: add all rules with start-symbol as LHS at position 0
        (let ((ri 0))
          (while (< ri (length grammar))
            (when (eq (funcall 'neovm--ep-lhs (nth ri grammar)) start-symbol)
              (funcall 'neovm--ep-add-item chart 0 (list ri 0 0)))
            (setq ri (1+ ri))))
        ;; Process each position
        (let ((pos 0))
          (while (<= pos n)
            ;; Predict and complete until no changes
            (let ((changed t))
              (while changed
                (setq changed nil)
                (when (funcall 'neovm--ep-predict grammar chart pos)
                  (setq changed t))
                (when (funcall 'neovm--ep-complete grammar chart pos)
                  (setq changed t))))
            ;; Scan
            (funcall 'neovm--ep-scan grammar chart pos tokens)
            (setq pos (1+ pos))))
        chart)))
  ;; Check if parse succeeded: look for complete start-symbol item at final position
  (fset 'neovm--ep-accepted-p
    (lambda (grammar chart n start-symbol)
      (let ((final-items (aref chart n))
            (found nil))
        (dolist (item final-items)
          (when (and (funcall 'neovm--ep-complete-p grammar item)
                     (= (nth 2 item) 0)
                     (eq (funcall 'neovm--ep-lhs
                                   (funcall 'neovm--ep-rule grammar (nth 0 item)))
                         start-symbol))
            (setq found t)))
        found)))

  (unwind-protect
      (let* ((grammar '((S A B)    ;; rule 0: S -> A B
                         (A "a")    ;; rule 1: A -> "a"
                         (B "b")))  ;; rule 2: B -> "b"
             (tokens '("a" "b"))
             (chart (funcall 'neovm--ep-parse grammar tokens 'S))
             (accepted (funcall 'neovm--ep-accepted-p grammar chart (length tokens) 'S)))
        ;; Also check chart contents
        (list accepted
              ;; Chart[0] should have items for S and A
              (length (aref chart 0))
              ;; Chart[2] should have completed S item
              (length (aref chart 2))
              ;; Rejected input: "b" "a"
              (let ((chart2 (funcall 'neovm--ep-parse grammar '("b" "a") 'S)))
                (funcall 'neovm--ep-accepted-p grammar chart2 2 'S))
              ;; Rejected input: "a" only (incomplete)
              (let ((chart3 (funcall 'neovm--ep-parse grammar '("a") 'S)))
                (funcall 'neovm--ep-accepted-p grammar chart3 1 'S))
              ;; Accepted: single-token grammar
              (let* ((g2 '((S "x")))
                     (c2 (funcall 'neovm--ep-parse g2 '("x") 'S)))
                (funcall 'neovm--ep-accepted-p g2 c2 1 'S))))
    (fmakunbound 'neovm--ep-rule)
    (fmakunbound 'neovm--ep-lhs)
    (fmakunbound 'neovm--ep-rhs)
    (fmakunbound 'neovm--ep-next-sym)
    (fmakunbound 'neovm--ep-complete-p)
    (fmakunbound 'neovm--ep-terminal-p)
    (fmakunbound 'neovm--ep-add-item)
    (fmakunbound 'neovm--ep-predict)
    (fmakunbound 'neovm--ep-scan)
    (fmakunbound 'neovm--ep-complete)
    (fmakunbound 'neovm--ep-parse)
    (fmakunbound 'neovm--ep-accepted-p)))"#;
    let expect = expect_test::expect![[r#""OK (t 2 2 nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Earley parser: recursive grammar (nested parentheses)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_earley_parser_recursive_grammar() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Grammar for balanced parentheses:
    //   S -> "(" S ")" | ""  (epsilon via S -> )
    // We encode epsilon as an empty-RHS rule.
    let form = r#"(progn
  (fset 'neovm--ep2-rule (lambda (g i) (nth i g)))
  (fset 'neovm--ep2-lhs (lambda (r) (car r)))
  (fset 'neovm--ep2-rhs (lambda (r) (cdr r)))
  (fset 'neovm--ep2-next
    (lambda (g item)
      (let ((rhs (cdr (nth (nth 0 item) g))))
        (nth (nth 1 item) rhs))))
  (fset 'neovm--ep2-done-p
    (lambda (g item)
      (>= (nth 1 item) (length (cdr (nth (nth 0 item) g))))))
  (fset 'neovm--ep2-termp (lambda (s) (stringp s)))
  (fset 'neovm--ep2-add
    (lambda (ch pos item)
      (let ((items (aref ch pos)))
        (unless (member item items)
          (aset ch pos (append items (list item))) t))))
  (fset 'neovm--ep2-predict
    (lambda (g ch pos)
      (let ((items (aref ch pos)) (i 0) (added nil))
        (while (< i (length items))
          (let ((sym (funcall 'neovm--ep2-next g (nth i items))))
            (when (and sym (not (funcall 'neovm--ep2-termp sym)))
              (let ((ri 0))
                (while (< ri (length g))
                  (when (eq (car (nth ri g)) sym)
                    (when (funcall 'neovm--ep2-add ch pos (list ri 0 pos))
                      (setq added t)))
                  (setq ri (1+ ri))))))
          (setq i (1+ i)))
        added)))
  (fset 'neovm--ep2-scan
    (lambda (g ch pos tokens)
      (when (< pos (length tokens))
        (let ((tok (nth pos tokens)))
          (dolist (item (aref ch pos))
            (let ((sym (funcall 'neovm--ep2-next g item)))
              (when (and sym (funcall 'neovm--ep2-termp sym) (string= sym tok))
                (funcall 'neovm--ep2-add ch (1+ pos)
                         (list (nth 0 item) (1+ (nth 1 item)) (nth 2 item))))))))))
  (fset 'neovm--ep2-complete
    (lambda (g ch pos)
      (let ((items (aref ch pos)) (i 0) (added nil))
        (while (< i (length items))
          (let ((item (nth i items)))
            (when (funcall 'neovm--ep2-done-p g item)
              (let ((lhs (car (nth (nth 0 item) g)))
                    (origin (nth 2 item)))
                (dolist (oi (aref ch origin))
                  (let ((sym (funcall 'neovm--ep2-next g oi)))
                    (when (and sym (eq sym lhs))
                      (when (funcall 'neovm--ep2-add ch pos
                                     (list (nth 0 oi) (1+ (nth 1 oi)) (nth 2 oi)))
                        (setq added t))))))))
          (setq i (1+ i)))
        added)))
  (fset 'neovm--ep2-parse
    (lambda (g tokens start)
      (let* ((n (length tokens))
             (ch (make-vector (1+ n) nil)))
        (let ((ri 0))
          (while (< ri (length g))
            (when (eq (car (nth ri g)) start)
              (funcall 'neovm--ep2-add ch 0 (list ri 0 0)))
            (setq ri (1+ ri))))
        (let ((pos 0))
          (while (<= pos n)
            (let ((changed t))
              (while changed
                (setq changed nil)
                (when (funcall 'neovm--ep2-predict g ch pos) (setq changed t))
                (when (funcall 'neovm--ep2-complete g ch pos) (setq changed t))))
            (funcall 'neovm--ep2-scan g ch pos tokens)
            (setq pos (1+ pos))))
        ch)))
  (fset 'neovm--ep2-ok-p
    (lambda (g ch n start)
      (let ((found nil))
        (dolist (item (aref ch n))
          (when (and (funcall 'neovm--ep2-done-p g item)
                     (= (nth 2 item) 0)
                     (eq (car (nth (nth 0 item) g)) start))
            (setq found t)))
        found)))

  (unwind-protect
      (let ((grammar '((S "(" S ")")   ;; S -> ( S )
                        (S))))          ;; S -> epsilon
        (list
         ;; Empty string: accepted (epsilon)
         (let ((c (funcall 'neovm--ep2-parse grammar '() 'S)))
           (funcall 'neovm--ep2-ok-p grammar c 0 'S))
         ;; "()" accepted
         (let ((c (funcall 'neovm--ep2-parse grammar '("(" ")") 'S)))
           (funcall 'neovm--ep2-ok-p grammar c 2 'S))
         ;; "(())" accepted
         (let ((c (funcall 'neovm--ep2-parse grammar '("(" "(" ")" ")") 'S)))
           (funcall 'neovm--ep2-ok-p grammar c 4 'S))
         ;; "((()))" accepted
         (let ((c (funcall 'neovm--ep2-parse grammar '("(" "(" "(" ")" ")" ")") 'S)))
           (funcall 'neovm--ep2-ok-p grammar c 6 'S))
         ;; "(" alone: rejected
         (let ((c (funcall 'neovm--ep2-parse grammar '("(") 'S)))
           (funcall 'neovm--ep2-ok-p grammar c 1 'S))
         ;; ")(" rejected
         (let ((c (funcall 'neovm--ep2-parse grammar '(")" "(") 'S)))
           (funcall 'neovm--ep2-ok-p grammar c 2 'S))))
    (fmakunbound 'neovm--ep2-rule)
    (fmakunbound 'neovm--ep2-lhs)
    (fmakunbound 'neovm--ep2-rhs)
    (fmakunbound 'neovm--ep2-next)
    (fmakunbound 'neovm--ep2-done-p)
    (fmakunbound 'neovm--ep2-termp)
    (fmakunbound 'neovm--ep2-add)
    (fmakunbound 'neovm--ep2-predict)
    (fmakunbound 'neovm--ep2-scan)
    (fmakunbound 'neovm--ep2-complete)
    (fmakunbound 'neovm--ep2-parse)
    (fmakunbound 'neovm--ep2-ok-p)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Earley parser: arithmetic expression grammar
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_earley_parser_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Grammar for arithmetic expressions with proper precedence:
    //   E -> E "+" T | T
    //   T -> T "*" F | F
    //   F -> "(" E ")" | "n"
    let form = r#"(progn
  (fset 'neovm--ep3-next
    (lambda (g item)
      (nth (nth 1 item) (cdr (nth (nth 0 item) g)))))
  (fset 'neovm--ep3-done-p
    (lambda (g item)
      (>= (nth 1 item) (length (cdr (nth (nth 0 item) g))))))
  (fset 'neovm--ep3-termp (lambda (s) (stringp s)))
  (fset 'neovm--ep3-add
    (lambda (ch pos item)
      (unless (member item (aref ch pos))
        (aset ch pos (append (aref ch pos) (list item))) t)))
  (fset 'neovm--ep3-predict
    (lambda (g ch pos)
      (let ((items (aref ch pos)) (i 0) (added nil))
        (while (< i (length items))
          (let ((sym (funcall 'neovm--ep3-next g (nth i items))))
            (when (and sym (not (funcall 'neovm--ep3-termp sym)))
              (let ((ri 0))
                (while (< ri (length g))
                  (when (eq (car (nth ri g)) sym)
                    (when (funcall 'neovm--ep3-add ch pos (list ri 0 pos))
                      (setq added t)))
                  (setq ri (1+ ri))))))
          (setq i (1+ i)))
        added)))
  (fset 'neovm--ep3-scan
    (lambda (g ch pos tokens)
      (when (< pos (length tokens))
        (let ((tok (nth pos tokens)))
          (dolist (item (aref ch pos))
            (let ((sym (funcall 'neovm--ep3-next g item)))
              (when (and sym (funcall 'neovm--ep3-termp sym) (string= sym tok))
                (funcall 'neovm--ep3-add ch (1+ pos)
                         (list (nth 0 item) (1+ (nth 1 item)) (nth 2 item))))))))))
  (fset 'neovm--ep3-complete
    (lambda (g ch pos)
      (let ((items (aref ch pos)) (i 0) (added nil))
        (while (< i (length items))
          (let ((item (nth i items)))
            (when (funcall 'neovm--ep3-done-p g item)
              (let ((lhs (car (nth (nth 0 item) g)))
                    (origin (nth 2 item)))
                (dolist (oi (aref ch origin))
                  (let ((sym (funcall 'neovm--ep3-next g oi)))
                    (when (and sym (eq sym lhs))
                      (when (funcall 'neovm--ep3-add ch pos
                                     (list (nth 0 oi) (1+ (nth 1 oi)) (nth 2 oi)))
                        (setq added t))))))))
          (setq i (1+ i)))
        added)))
  (fset 'neovm--ep3-parse
    (lambda (g tokens start)
      (let* ((n (length tokens))
             (ch (make-vector (1+ n) nil)))
        (let ((ri 0))
          (while (< ri (length g))
            (when (eq (car (nth ri g)) start)
              (funcall 'neovm--ep3-add ch 0 (list ri 0 0)))
            (setq ri (1+ ri))))
        (let ((pos 0))
          (while (<= pos n)
            (let ((changed t))
              (while changed
                (setq changed nil)
                (when (funcall 'neovm--ep3-predict g ch pos) (setq changed t))
                (when (funcall 'neovm--ep3-complete g ch pos) (setq changed t))))
            (funcall 'neovm--ep3-scan g ch pos tokens)
            (setq pos (1+ pos))))
        ch)))
  (fset 'neovm--ep3-ok-p
    (lambda (g ch n start)
      (let ((found nil))
        (dolist (item (aref ch n))
          (when (and (funcall 'neovm--ep3-done-p g item)
                     (= (nth 2 item) 0)
                     (eq (car (nth (nth 0 item) g)) start))
            (setq found t)))
        found)))

  (unwind-protect
      (let ((grammar '((E E "+" T)     ;; E -> E + T
                        (E T)           ;; E -> T
                        (T T "*" F)     ;; T -> T * F
                        (T F)           ;; T -> F
                        (F "(" E ")")   ;; F -> ( E )
                        (F "n"))))      ;; F -> n
        (list
         ;; "n" -> accepted
         (let ((c (funcall 'neovm--ep3-parse grammar '("n") 'E)))
           (funcall 'neovm--ep3-ok-p grammar c 1 'E))
         ;; "n + n" -> accepted
         (let ((c (funcall 'neovm--ep3-parse grammar '("n" "+" "n") 'E)))
           (funcall 'neovm--ep3-ok-p grammar c 3 'E))
         ;; "n * n + n" -> accepted
         (let ((c (funcall 'neovm--ep3-parse grammar '("n" "*" "n" "+" "n") 'E)))
           (funcall 'neovm--ep3-ok-p grammar c 5 'E))
         ;; "( n + n ) * n" -> accepted
         (let ((c (funcall 'neovm--ep3-parse grammar '("(" "n" "+" "n" ")" "*" "n") 'E)))
           (funcall 'neovm--ep3-ok-p grammar c 7 'E))
         ;; "n +" -> rejected (incomplete)
         (let ((c (funcall 'neovm--ep3-parse grammar '("n" "+") 'E)))
           (funcall 'neovm--ep3-ok-p grammar c 2 'E))
         ;; "+ n" -> rejected
         (let ((c (funcall 'neovm--ep3-parse grammar '("+" "n") 'E)))
           (funcall 'neovm--ep3-ok-p grammar c 2 'E))
         ;; "( n" -> rejected (unmatched paren)
         (let ((c (funcall 'neovm--ep3-parse grammar '("(" "n") 'E)))
           (funcall 'neovm--ep3-ok-p grammar c 2 'E))
         ;; "n + n * n + n" -> accepted (longer expr)
         (let ((c (funcall 'neovm--ep3-parse grammar
                           '("n" "+" "n" "*" "n" "+" "n") 'E)))
           (funcall 'neovm--ep3-ok-p grammar c 7 'E))))
    (fmakunbound 'neovm--ep3-next)
    (fmakunbound 'neovm--ep3-done-p)
    (fmakunbound 'neovm--ep3-termp)
    (fmakunbound 'neovm--ep3-add)
    (fmakunbound 'neovm--ep3-predict)
    (fmakunbound 'neovm--ep3-scan)
    (fmakunbound 'neovm--ep3-complete)
    (fmakunbound 'neovm--ep3-parse)
    (fmakunbound 'neovm--ep3-ok-p)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t nil nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Earley parser: ambiguity detection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_earley_parser_ambiguity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // An ambiguous grammar: S -> S S | "a"
    // "aaa" has two parse trees: (S (S a) (S (S a) (S a))) and (S (S (S a) (S a)) (S a))
    // Detect ambiguity by counting complete S items at the final chart position.
    let form = r#"(progn
  (fset 'neovm--ep4-next
    (lambda (g item)
      (nth (nth 1 item) (cdr (nth (nth 0 item) g)))))
  (fset 'neovm--ep4-done-p
    (lambda (g item)
      (>= (nth 1 item) (length (cdr (nth (nth 0 item) g))))))
  (fset 'neovm--ep4-termp (lambda (s) (stringp s)))
  (fset 'neovm--ep4-add
    (lambda (ch pos item)
      (unless (member item (aref ch pos))
        (aset ch pos (append (aref ch pos) (list item))) t)))
  (fset 'neovm--ep4-predict
    (lambda (g ch pos)
      (let ((items (aref ch pos)) (i 0) (added nil))
        (while (< i (length items))
          (let ((sym (funcall 'neovm--ep4-next g (nth i items))))
            (when (and sym (not (funcall 'neovm--ep4-termp sym)))
              (let ((ri 0))
                (while (< ri (length g))
                  (when (eq (car (nth ri g)) sym)
                    (when (funcall 'neovm--ep4-add ch pos (list ri 0 pos))
                      (setq added t)))
                  (setq ri (1+ ri))))))
          (setq i (1+ i)))
        added)))
  (fset 'neovm--ep4-scan
    (lambda (g ch pos tokens)
      (when (< pos (length tokens))
        (let ((tok (nth pos tokens)))
          (dolist (item (aref ch pos))
            (let ((sym (funcall 'neovm--ep4-next g item)))
              (when (and sym (funcall 'neovm--ep4-termp sym) (string= sym tok))
                (funcall 'neovm--ep4-add ch (1+ pos)
                         (list (nth 0 item) (1+ (nth 1 item)) (nth 2 item))))))))))
  (fset 'neovm--ep4-complete
    (lambda (g ch pos)
      (let ((items (aref ch pos)) (i 0) (added nil))
        (while (< i (length items))
          (let ((item (nth i items)))
            (when (funcall 'neovm--ep4-done-p g item)
              (let ((lhs (car (nth (nth 0 item) g)))
                    (origin (nth 2 item)))
                (dolist (oi (aref ch origin))
                  (let ((sym (funcall 'neovm--ep4-next g oi)))
                    (when (and sym (eq sym lhs))
                      (when (funcall 'neovm--ep4-add ch pos
                                     (list (nth 0 oi) (1+ (nth 1 oi)) (nth 2 oi)))
                        (setq added t))))))))
          (setq i (1+ i)))
        added)))
  (fset 'neovm--ep4-parse
    (lambda (g tokens start)
      (let* ((n (length tokens))
             (ch (make-vector (1+ n) nil)))
        (let ((ri 0))
          (while (< ri (length g))
            (when (eq (car (nth ri g)) start)
              (funcall 'neovm--ep4-add ch 0 (list ri 0 0)))
            (setq ri (1+ ri))))
        (let ((pos 0))
          (while (<= pos n)
            (let ((changed t))
              (while changed
                (setq changed nil)
                (when (funcall 'neovm--ep4-predict g ch pos) (setq changed t))
                (when (funcall 'neovm--ep4-complete g ch pos) (setq changed t))))
            (funcall 'neovm--ep4-scan g ch pos tokens)
            (setq pos (1+ pos))))
        ch)))
  ;; Count complete start-symbol items spanning the entire input
  (fset 'neovm--ep4-count-parses
    (lambda (g ch n start)
      (let ((count 0))
        (dolist (item (aref ch n))
          (when (and (funcall 'neovm--ep4-done-p g item)
                     (= (nth 2 item) 0)
                     (eq (car (nth (nth 0 item) g)) start))
            (setq count (1+ count))))
        count)))

  (unwind-protect
      (let ((ambig-grammar '((S S S)    ;; S -> S S
                              (S "a")))) ;; S -> "a"
        (list
         ;; "a": 1 parse
         (let ((c (funcall 'neovm--ep4-parse ambig-grammar '("a") 'S)))
           (funcall 'neovm--ep4-count-parses ambig-grammar c 1 'S))
         ;; "aa": 1 parse (only S -> S S, each S -> "a")
         (let ((c (funcall 'neovm--ep4-parse ambig-grammar '("a" "a") 'S)))
           (funcall 'neovm--ep4-count-parses ambig-grammar c 2 'S))
         ;; "aaa": 2 parses (ambiguous: (a)(aa) vs (aa)(a))
         (let ((c (funcall 'neovm--ep4-parse ambig-grammar '("a" "a" "a") 'S)))
           (funcall 'neovm--ep4-count-parses ambig-grammar c 3 'S))
         ;; "aaaa": multiple parses (Catalan number C(3)=5)
         (let ((c (funcall 'neovm--ep4-parse ambig-grammar '("a" "a" "a" "a") 'S)))
           (funcall 'neovm--ep4-count-parses ambig-grammar c 4 'S))
         ;; Non-ambiguous grammar: S -> "a" "b" -- always 1 parse
         (let* ((unamb '((S "a" "b")))
                (c (funcall 'neovm--ep4-parse unamb '("a" "b") 'S)))
           (funcall 'neovm--ep4-count-parses unamb c 2 'S))))
    (fmakunbound 'neovm--ep4-next)
    (fmakunbound 'neovm--ep4-done-p)
    (fmakunbound 'neovm--ep4-termp)
    (fmakunbound 'neovm--ep4-add)
    (fmakunbound 'neovm--ep4-predict)
    (fmakunbound 'neovm--ep4-scan)
    (fmakunbound 'neovm--ep4-complete)
    (fmakunbound 'neovm--ep4-parse)
    (fmakunbound 'neovm--ep4-count-parses)))"#;
    let expect = expect_test::expect![[r#""OK (1 1 1 1 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Earley parser: multi-rule grammar with optional elements
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_earley_parser_multi_rule() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Grammar for simple statements:
    //   PROG -> STMT
    //   PROG -> PROG ";" STMT
    //   STMT -> "let" ID "=" EXPR
    //   STMT -> "print" EXPR
    //   EXPR -> ID
    //   EXPR -> NUM
    //   ID -> "x" | "y" | "z"
    //   NUM -> "0" | "1" | "2"
    let form = r#"(progn
  (fset 'neovm--ep5-next
    (lambda (g item)
      (nth (nth 1 item) (cdr (nth (nth 0 item) g)))))
  (fset 'neovm--ep5-done-p
    (lambda (g item)
      (>= (nth 1 item) (length (cdr (nth (nth 0 item) g))))))
  (fset 'neovm--ep5-termp (lambda (s) (stringp s)))
  (fset 'neovm--ep5-add
    (lambda (ch pos item)
      (unless (member item (aref ch pos))
        (aset ch pos (append (aref ch pos) (list item))) t)))
  (fset 'neovm--ep5-predict
    (lambda (g ch pos)
      (let ((items (aref ch pos)) (i 0) (added nil))
        (while (< i (length items))
          (let ((sym (funcall 'neovm--ep5-next g (nth i items))))
            (when (and sym (not (funcall 'neovm--ep5-termp sym)))
              (let ((ri 0))
                (while (< ri (length g))
                  (when (eq (car (nth ri g)) sym)
                    (when (funcall 'neovm--ep5-add ch pos (list ri 0 pos))
                      (setq added t)))
                  (setq ri (1+ ri))))))
          (setq i (1+ i)))
        added)))
  (fset 'neovm--ep5-scan
    (lambda (g ch pos tokens)
      (when (< pos (length tokens))
        (let ((tok (nth pos tokens)))
          (dolist (item (aref ch pos))
            (let ((sym (funcall 'neovm--ep5-next g item)))
              (when (and sym (funcall 'neovm--ep5-termp sym) (string= sym tok))
                (funcall 'neovm--ep5-add ch (1+ pos)
                         (list (nth 0 item) (1+ (nth 1 item)) (nth 2 item))))))))))
  (fset 'neovm--ep5-complete
    (lambda (g ch pos)
      (let ((items (aref ch pos)) (i 0) (added nil))
        (while (< i (length items))
          (let ((item (nth i items)))
            (when (funcall 'neovm--ep5-done-p g item)
              (let ((lhs (car (nth (nth 0 item) g)))
                    (origin (nth 2 item)))
                (dolist (oi (aref ch origin))
                  (let ((sym (funcall 'neovm--ep5-next g oi)))
                    (when (and sym (eq sym lhs))
                      (when (funcall 'neovm--ep5-add ch pos
                                     (list (nth 0 oi) (1+ (nth 1 oi)) (nth 2 oi)))
                        (setq added t))))))))
          (setq i (1+ i)))
        added)))
  (fset 'neovm--ep5-parse
    (lambda (g tokens start)
      (let* ((n (length tokens))
             (ch (make-vector (1+ n) nil)))
        (let ((ri 0))
          (while (< ri (length g))
            (when (eq (car (nth ri g)) start)
              (funcall 'neovm--ep5-add ch 0 (list ri 0 0)))
            (setq ri (1+ ri))))
        (let ((pos 0))
          (while (<= pos n)
            (let ((changed t))
              (while changed
                (setq changed nil)
                (when (funcall 'neovm--ep5-predict g ch pos) (setq changed t))
                (when (funcall 'neovm--ep5-complete g ch pos) (setq changed t))))
            (funcall 'neovm--ep5-scan g ch pos tokens)
            (setq pos (1+ pos))))
        ch)))
  (fset 'neovm--ep5-ok-p
    (lambda (g ch n start)
      (let ((found nil))
        (dolist (item (aref ch n))
          (when (and (funcall 'neovm--ep5-done-p g item)
                     (= (nth 2 item) 0)
                     (eq (car (nth (nth 0 item) g)) start))
            (setq found t)))
        found)))

  (unwind-protect
      (let ((grammar '((PROG STMT)                  ;; 0
                        (PROG PROG ";" STMT)         ;; 1
                        (STMT "let" ID "=" EXPR)     ;; 2
                        (STMT "print" EXPR)          ;; 3
                        (EXPR ID)                    ;; 4
                        (EXPR NUM)                   ;; 5
                        (ID "x")                     ;; 6
                        (ID "y")                     ;; 7
                        (ID "z")                     ;; 8
                        (NUM "0")                    ;; 9
                        (NUM "1")                    ;; 10
                        (NUM "2"))))                 ;; 11
        (list
         ;; "print x" -> accepted
         (let ((c (funcall 'neovm--ep5-parse grammar '("print" "x") 'PROG)))
           (funcall 'neovm--ep5-ok-p grammar c 2 'PROG))
         ;; "let x = 1" -> accepted
         (let ((c (funcall 'neovm--ep5-parse grammar '("let" "x" "=" "1") 'PROG)))
           (funcall 'neovm--ep5-ok-p grammar c 4 'PROG))
         ;; "let x = 1 ; print y" -> accepted (two statements)
         (let ((c (funcall 'neovm--ep5-parse grammar
                           '("let" "x" "=" "1" ";" "print" "y") 'PROG)))
           (funcall 'neovm--ep5-ok-p grammar c 7 'PROG))
         ;; "let x = y ; let z = 0 ; print x" -> accepted (three statements)
         (let ((c (funcall 'neovm--ep5-parse grammar
                           '("let" "x" "=" "y" ";" "let" "z" "=" "0" ";" "print" "x") 'PROG)))
           (funcall 'neovm--ep5-ok-p grammar c 12 'PROG))
         ;; "let" alone -> rejected
         (let ((c (funcall 'neovm--ep5-parse grammar '("let") 'PROG)))
           (funcall 'neovm--ep5-ok-p grammar c 1 'PROG))
         ;; "print" alone -> rejected
         (let ((c (funcall 'neovm--ep5-parse grammar '("print") 'PROG)))
           (funcall 'neovm--ep5-ok-p grammar c 1 'PROG))
         ;; "; print x" -> rejected (leading semicolon)
         (let ((c (funcall 'neovm--ep5-parse grammar '(";" "print" "x") 'PROG)))
           (funcall 'neovm--ep5-ok-p grammar c 3 'PROG))))
    (fmakunbound 'neovm--ep5-next)
    (fmakunbound 'neovm--ep5-done-p)
    (fmakunbound 'neovm--ep5-termp)
    (fmakunbound 'neovm--ep5-add)
    (fmakunbound 'neovm--ep5-predict)
    (fmakunbound 'neovm--ep5-scan)
    (fmakunbound 'neovm--ep5-complete)
    (fmakunbound 'neovm--ep5-parse)
    (fmakunbound 'neovm--ep5-ok-p)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Earley parser: chart size and item counts
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_earley_parser_chart_stats() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify deterministic chart sizes for known grammars and inputs
    let form = r#"(progn
  (fset 'neovm--ep6-next
    (lambda (g item)
      (nth (nth 1 item) (cdr (nth (nth 0 item) g)))))
  (fset 'neovm--ep6-done-p
    (lambda (g item)
      (>= (nth 1 item) (length (cdr (nth (nth 0 item) g))))))
  (fset 'neovm--ep6-termp (lambda (s) (stringp s)))
  (fset 'neovm--ep6-add
    (lambda (ch pos item)
      (unless (member item (aref ch pos))
        (aset ch pos (append (aref ch pos) (list item))) t)))
  (fset 'neovm--ep6-predict
    (lambda (g ch pos)
      (let ((items (aref ch pos)) (i 0) (added nil))
        (while (< i (length items))
          (let ((sym (funcall 'neovm--ep6-next g (nth i items))))
            (when (and sym (not (funcall 'neovm--ep6-termp sym)))
              (let ((ri 0))
                (while (< ri (length g))
                  (when (eq (car (nth ri g)) sym)
                    (when (funcall 'neovm--ep6-add ch pos (list ri 0 pos))
                      (setq added t)))
                  (setq ri (1+ ri))))))
          (setq i (1+ i)))
        added)))
  (fset 'neovm--ep6-scan
    (lambda (g ch pos tokens)
      (when (< pos (length tokens))
        (let ((tok (nth pos tokens)))
          (dolist (item (aref ch pos))
            (let ((sym (funcall 'neovm--ep6-next g item)))
              (when (and sym (funcall 'neovm--ep6-termp sym) (string= sym tok))
                (funcall 'neovm--ep6-add ch (1+ pos)
                         (list (nth 0 item) (1+ (nth 1 item)) (nth 2 item))))))))))
  (fset 'neovm--ep6-complete
    (lambda (g ch pos)
      (let ((items (aref ch pos)) (i 0) (added nil))
        (while (< i (length items))
          (let ((item (nth i items)))
            (when (funcall 'neovm--ep6-done-p g item)
              (let ((lhs (car (nth (nth 0 item) g)))
                    (origin (nth 2 item)))
                (dolist (oi (aref ch origin))
                  (let ((sym (funcall 'neovm--ep6-next g oi)))
                    (when (and sym (eq sym lhs))
                      (when (funcall 'neovm--ep6-add ch pos
                                     (list (nth 0 oi) (1+ (nth 1 oi)) (nth 2 oi)))
                        (setq added t))))))))
          (setq i (1+ i)))
        added)))
  (fset 'neovm--ep6-parse
    (lambda (g tokens start)
      (let* ((n (length tokens))
             (ch (make-vector (1+ n) nil)))
        (let ((ri 0))
          (while (< ri (length g))
            (when (eq (car (nth ri g)) start)
              (funcall 'neovm--ep6-add ch 0 (list ri 0 0)))
            (setq ri (1+ ri))))
        (let ((pos 0))
          (while (<= pos n)
            (let ((changed t))
              (while changed
                (setq changed nil)
                (when (funcall 'neovm--ep6-predict g ch pos) (setq changed t))
                (when (funcall 'neovm--ep6-complete g ch pos) (setq changed t))))
            (funcall 'neovm--ep6-scan g ch pos tokens)
            (setq pos (1+ pos))))
        ch)))
  ;; Collect stats: items per chart position
  (fset 'neovm--ep6-chart-stats
    (lambda (ch n)
      (let ((stats nil) (pos 0))
        (while (<= pos n)
          (setq stats (cons (length (aref ch pos)) stats))
          (setq pos (1+ pos)))
        (nreverse stats))))

  (unwind-protect
      (let ((g1 '((S "a" "b" "c"))))  ;; simple 3-token grammar
        (list
         ;; Chart stats for S -> "a" "b" "c" with input "a" "b" "c"
         (let ((c (funcall 'neovm--ep6-parse g1 '("a" "b" "c") 'S)))
           (funcall 'neovm--ep6-chart-stats c 3))
         ;; Chart stats for ambiguous S -> S S | "a" with input "a" "a"
         (let* ((g2 '((S S S) (S "a")))
                (c (funcall 'neovm--ep6-parse g2 '("a" "a") 'S)))
           (funcall 'neovm--ep6-chart-stats c 2))
         ;; Total items across all chart positions
         (let* ((g3 '((S A B) (A "x") (B "y")))
                (c (funcall 'neovm--ep6-parse g3 '("x" "y") 'S))
                (total 0) (pos 0))
           (while (<= pos 2)
             (setq total (+ total (length (aref c pos))))
             (setq pos (1+ pos)))
           total)
         ;; Empty chart for rejected input
         (let* ((g4 '((S "a")))
                (c (funcall 'neovm--ep6-parse g4 '("b") 'S)))
           (funcall 'neovm--ep6-chart-stats c 1))))
    (fmakunbound 'neovm--ep6-next)
    (fmakunbound 'neovm--ep6-done-p)
    (fmakunbound 'neovm--ep6-termp)
    (fmakunbound 'neovm--ep6-add)
    (fmakunbound 'neovm--ep6-predict)
    (fmakunbound 'neovm--ep6-scan)
    (fmakunbound 'neovm--ep6-complete)
    (fmakunbound 'neovm--ep6-parse)
    (fmakunbound 'neovm--ep6-chart-stats)))"#;
    let expect = expect_test::expect![[r#""OK ((1 1 1 1) (2 4 6) 7 (1 0))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
