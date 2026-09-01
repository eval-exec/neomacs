//! Oracle parity tests for advanced Earley parser patterns.
//!
//! Covers: epsilon/nullable productions, left-recursive grammars,
//! parse forest construction (shared packed parse forest), grammar
//! with operator priorities, error reporting with furthest match,
//! right-recursive vs left-recursive comparison, and grammar with
//! multiple start symbols.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// 1. Earley parser with nullable/epsilon productions and chained nullables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_earley_advanced_nullable_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Grammar with chained nullable productions:
    //   S -> A B C
    //   A -> "a" | (epsilon)
    //   B -> "b" | (epsilon)
    //   C -> "c" | (epsilon)
    // This means S can match "", "a", "b", "c", "ab", "ac", "bc", "abc"
    let form = r#"(progn
  (fset 'neovm--epa-next
    (lambda (g item)
      (nth (nth 1 item) (cdr (nth (nth 0 item) g)))))
  (fset 'neovm--epa-done-p
    (lambda (g item)
      (>= (nth 1 item) (length (cdr (nth (nth 0 item) g))))))
  (fset 'neovm--epa-termp (lambda (s) (stringp s)))
  (fset 'neovm--epa-add
    (lambda (ch pos item)
      (unless (member item (aref ch pos))
        (aset ch pos (append (aref ch pos) (list item))) t)))
  (fset 'neovm--epa-predict
    (lambda (g ch pos)
      (let ((items (aref ch pos)) (i 0) (added nil))
        (while (< i (length items))
          (let ((sym (funcall 'neovm--epa-next g (nth i items))))
            (when (and sym (not (funcall 'neovm--epa-termp sym)))
              (let ((ri 0))
                (while (< ri (length g))
                  (when (eq (car (nth ri g)) sym)
                    (when (funcall 'neovm--epa-add ch pos (list ri 0 pos))
                      (setq added t)))
                  (setq ri (1+ ri))))))
          (setq i (1+ i)))
        added)))
  (fset 'neovm--epa-scan
    (lambda (g ch pos tokens)
      (when (< pos (length tokens))
        (let ((tok (nth pos tokens)))
          (dolist (item (aref ch pos))
            (let ((sym (funcall 'neovm--epa-next g item)))
              (when (and sym (funcall 'neovm--epa-termp sym) (string= sym tok))
                (funcall 'neovm--epa-add ch (1+ pos)
                         (list (nth 0 item) (1+ (nth 1 item)) (nth 2 item))))))))))
  (fset 'neovm--epa-complete
    (lambda (g ch pos)
      (let ((items (aref ch pos)) (i 0) (added nil))
        (while (< i (length items))
          (let ((item (nth i items)))
            (when (funcall 'neovm--epa-done-p g item)
              (let ((lhs (car (nth (nth 0 item) g)))
                    (origin (nth 2 item)))
                (dolist (oi (aref ch origin))
                  (let ((sym (funcall 'neovm--epa-next g oi)))
                    (when (and sym (eq sym lhs))
                      (when (funcall 'neovm--epa-add ch pos
                                     (list (nth 0 oi) (1+ (nth 1 oi)) (nth 2 oi)))
                        (setq added t))))))))
          (setq i (1+ i)))
        added)))
  (fset 'neovm--epa-parse
    (lambda (g tokens start)
      (let* ((n (length tokens))
             (ch (make-vector (1+ n) nil)))
        (let ((ri 0))
          (while (< ri (length g))
            (when (eq (car (nth ri g)) start)
              (funcall 'neovm--epa-add ch 0 (list ri 0 0)))
            (setq ri (1+ ri))))
        (let ((pos 0))
          (while (<= pos n)
            (let ((changed t))
              (while changed
                (setq changed nil)
                (when (funcall 'neovm--epa-predict g ch pos) (setq changed t))
                (when (funcall 'neovm--epa-complete g ch pos) (setq changed t))))
            (funcall 'neovm--epa-scan g ch pos tokens)
            (setq pos (1+ pos))))
        ch)))
  (fset 'neovm--epa-ok-p
    (lambda (g ch n start)
      (let ((found nil))
        (dolist (item (aref ch n))
          (when (and (funcall 'neovm--epa-done-p g item)
                     (= (nth 2 item) 0)
                     (eq (car (nth (nth 0 item) g)) start))
            (setq found t)))
        found)))

  (unwind-protect
      (let ((grammar '((S A B C)   ;; 0: S -> A B C
                        (A "a")     ;; 1: A -> "a"
                        (A)         ;; 2: A -> epsilon
                        (B "b")     ;; 3: B -> "b"
                        (B)         ;; 4: B -> epsilon
                        (C "c")     ;; 5: C -> "c"
                        (C))))      ;; 6: C -> epsilon
        (list
         ;; Empty string accepted (all nullable)
         (let ((c (funcall 'neovm--epa-parse grammar '() 'S)))
           (funcall 'neovm--epa-ok-p grammar c 0 'S))
         ;; "a" accepted
         (let ((c (funcall 'neovm--epa-parse grammar '("a") 'S)))
           (funcall 'neovm--epa-ok-p grammar c 1 'S))
         ;; "b" accepted
         (let ((c (funcall 'neovm--epa-parse grammar '("b") 'S)))
           (funcall 'neovm--epa-ok-p grammar c 1 'S))
         ;; "c" accepted
         (let ((c (funcall 'neovm--epa-parse grammar '("c") 'S)))
           (funcall 'neovm--epa-ok-p grammar c 1 'S))
         ;; "ab" accepted
         (let ((c (funcall 'neovm--epa-parse grammar '("a" "b") 'S)))
           (funcall 'neovm--epa-ok-p grammar c 2 'S))
         ;; "abc" accepted (full)
         (let ((c (funcall 'neovm--epa-parse grammar '("a" "b" "c") 'S)))
           (funcall 'neovm--epa-ok-p grammar c 3 'S))
         ;; "bc" accepted (A nullable)
         (let ((c (funcall 'neovm--epa-parse grammar '("b" "c") 'S)))
           (funcall 'neovm--epa-ok-p grammar c 2 'S))
         ;; "ac" accepted (B nullable)
         (let ((c (funcall 'neovm--epa-parse grammar '("a" "c") 'S)))
           (funcall 'neovm--epa-ok-p grammar c 2 'S))
         ;; "ba" rejected (wrong order)
         (let ((c (funcall 'neovm--epa-parse grammar '("b" "a") 'S)))
           (funcall 'neovm--epa-ok-p grammar c 2 'S))
         ;; "abcd" rejected (extra token)
         (let ((c (funcall 'neovm--epa-parse grammar '("a" "b" "c" "d") 'S)))
           (funcall 'neovm--epa-ok-p grammar c 4 'S))))
    (fmakunbound 'neovm--epa-next)
    (fmakunbound 'neovm--epa-done-p)
    (fmakunbound 'neovm--epa-termp)
    (fmakunbound 'neovm--epa-add)
    (fmakunbound 'neovm--epa-predict)
    (fmakunbound 'neovm--epa-scan)
    (fmakunbound 'neovm--epa-complete)
    (fmakunbound 'neovm--epa-parse)
    (fmakunbound 'neovm--epa-ok-p)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 2. Left-recursive grammar: list of items
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_earley_advanced_left_recursive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Grammar:
    //   L -> L "," ITEM | ITEM
    //   ITEM -> "x"
    // This is directly left-recursive. Earley handles it naturally.
    let form = r#"(progn
  (fset 'neovm--epb-next
    (lambda (g item)
      (nth (nth 1 item) (cdr (nth (nth 0 item) g)))))
  (fset 'neovm--epb-done-p
    (lambda (g item)
      (>= (nth 1 item) (length (cdr (nth (nth 0 item) g))))))
  (fset 'neovm--epb-termp (lambda (s) (stringp s)))
  (fset 'neovm--epb-add
    (lambda (ch pos item)
      (unless (member item (aref ch pos))
        (aset ch pos (append (aref ch pos) (list item))) t)))
  (fset 'neovm--epb-predict
    (lambda (g ch pos)
      (let ((items (aref ch pos)) (i 0) (added nil))
        (while (< i (length items))
          (let ((sym (funcall 'neovm--epb-next g (nth i items))))
            (when (and sym (not (funcall 'neovm--epb-termp sym)))
              (let ((ri 0))
                (while (< ri (length g))
                  (when (eq (car (nth ri g)) sym)
                    (when (funcall 'neovm--epb-add ch pos (list ri 0 pos))
                      (setq added t)))
                  (setq ri (1+ ri))))))
          (setq i (1+ i)))
        added)))
  (fset 'neovm--epb-scan
    (lambda (g ch pos tokens)
      (when (< pos (length tokens))
        (let ((tok (nth pos tokens)))
          (dolist (item (aref ch pos))
            (let ((sym (funcall 'neovm--epb-next g item)))
              (when (and sym (funcall 'neovm--epb-termp sym) (string= sym tok))
                (funcall 'neovm--epb-add ch (1+ pos)
                         (list (nth 0 item) (1+ (nth 1 item)) (nth 2 item))))))))))
  (fset 'neovm--epb-complete
    (lambda (g ch pos)
      (let ((items (aref ch pos)) (i 0) (added nil))
        (while (< i (length items))
          (let ((item (nth i items)))
            (when (funcall 'neovm--epb-done-p g item)
              (let ((lhs (car (nth (nth 0 item) g)))
                    (origin (nth 2 item)))
                (dolist (oi (aref ch origin))
                  (let ((sym (funcall 'neovm--epb-next g oi)))
                    (when (and sym (eq sym lhs))
                      (when (funcall 'neovm--epb-add ch pos
                                     (list (nth 0 oi) (1+ (nth 1 oi)) (nth 2 oi)))
                        (setq added t))))))))
          (setq i (1+ i)))
        added)))
  (fset 'neovm--epb-parse
    (lambda (g tokens start)
      (let* ((n (length tokens))
             (ch (make-vector (1+ n) nil)))
        (let ((ri 0))
          (while (< ri (length g))
            (when (eq (car (nth ri g)) start)
              (funcall 'neovm--epb-add ch 0 (list ri 0 0)))
            (setq ri (1+ ri))))
        (let ((pos 0))
          (while (<= pos n)
            (let ((changed t))
              (while changed
                (setq changed nil)
                (when (funcall 'neovm--epb-predict g ch pos) (setq changed t))
                (when (funcall 'neovm--epb-complete g ch pos) (setq changed t))))
            (funcall 'neovm--epb-scan g ch pos tokens)
            (setq pos (1+ pos))))
        ch)))
  (fset 'neovm--epb-ok-p
    (lambda (g ch n start)
      (let ((found nil))
        (dolist (item (aref ch n))
          (when (and (funcall 'neovm--epb-done-p g item)
                     (= (nth 2 item) 0)
                     (eq (car (nth (nth 0 item) g)) start))
            (setq found t)))
        found)))

  (unwind-protect
      (let ((grammar '((L L "," ITEM)  ;; 0: L -> L , ITEM (left-recursive)
                        (L ITEM)        ;; 1: L -> ITEM
                        (ITEM "x"))))   ;; 2: ITEM -> "x"
        (list
         ;; "x" -> accepted (single item)
         (let ((c (funcall 'neovm--epb-parse grammar '("x") 'L)))
           (funcall 'neovm--epb-ok-p grammar c 1 'L))
         ;; "x , x" -> accepted (two items)
         (let ((c (funcall 'neovm--epb-parse grammar '("x" "," "x") 'L)))
           (funcall 'neovm--epb-ok-p grammar c 3 'L))
         ;; "x , x , x" -> accepted (three items)
         (let ((c (funcall 'neovm--epb-parse grammar '("x" "," "x" "," "x") 'L)))
           (funcall 'neovm--epb-ok-p grammar c 5 'L))
         ;; "x , x , x , x , x" -> accepted (five items)
         (let ((c (funcall 'neovm--epb-parse grammar
                           '("x" "," "x" "," "x" "," "x" "," "x") 'L)))
           (funcall 'neovm--epb-ok-p grammar c 9 'L))
         ;; "," -> rejected (no item)
         (let ((c (funcall 'neovm--epb-parse grammar '(",") 'L)))
           (funcall 'neovm--epb-ok-p grammar c 1 'L))
         ;; "x ," -> rejected (trailing comma)
         (let ((c (funcall 'neovm--epb-parse grammar '("x" ",") 'L)))
           (funcall 'neovm--epb-ok-p grammar c 2 'L))
         ;; "" -> rejected (empty)
         (let ((c (funcall 'neovm--epb-parse grammar '() 'L)))
           (funcall 'neovm--epb-ok-p grammar c 0 'L))))
    (fmakunbound 'neovm--epb-next)
    (fmakunbound 'neovm--epb-done-p)
    (fmakunbound 'neovm--epb-termp)
    (fmakunbound 'neovm--epb-add)
    (fmakunbound 'neovm--epb-predict)
    (fmakunbound 'neovm--epb-scan)
    (fmakunbound 'neovm--epb-complete)
    (fmakunbound 'neovm--epb-parse)
    (fmakunbound 'neovm--epb-ok-p)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 3. Parse forest construction: count distinct derivation paths
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_earley_advanced_parse_forest_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use the ambiguous grammar S -> S S | "a" and count the number of
    // distinct completed S items spanning [0, n] at each chart position
    // to indirectly measure the parse forest size.
    // The number of complete S items from origin 0 follows Catalan numbers:
    //   n=1: 1, n=2: 1, n=3: 2, n=4: 5
    let form = r#"(progn
  (fset 'neovm--epc-next
    (lambda (g item)
      (nth (nth 1 item) (cdr (nth (nth 0 item) g)))))
  (fset 'neovm--epc-done-p
    (lambda (g item)
      (>= (nth 1 item) (length (cdr (nth (nth 0 item) g))))))
  (fset 'neovm--epc-termp (lambda (s) (stringp s)))
  (fset 'neovm--epc-add
    (lambda (ch pos item)
      (unless (member item (aref ch pos))
        (aset ch pos (append (aref ch pos) (list item))) t)))
  (fset 'neovm--epc-predict
    (lambda (g ch pos)
      (let ((items (aref ch pos)) (i 0) (added nil))
        (while (< i (length items))
          (let ((sym (funcall 'neovm--epc-next g (nth i items))))
            (when (and sym (not (funcall 'neovm--epc-termp sym)))
              (let ((ri 0))
                (while (< ri (length g))
                  (when (eq (car (nth ri g)) sym)
                    (when (funcall 'neovm--epc-add ch pos (list ri 0 pos))
                      (setq added t)))
                  (setq ri (1+ ri))))))
          (setq i (1+ i)))
        added)))
  (fset 'neovm--epc-scan
    (lambda (g ch pos tokens)
      (when (< pos (length tokens))
        (let ((tok (nth pos tokens)))
          (dolist (item (aref ch pos))
            (let ((sym (funcall 'neovm--epc-next g item)))
              (when (and sym (funcall 'neovm--epc-termp sym) (string= sym tok))
                (funcall 'neovm--epc-add ch (1+ pos)
                         (list (nth 0 item) (1+ (nth 1 item)) (nth 2 item))))))))))
  (fset 'neovm--epc-complete
    (lambda (g ch pos)
      (let ((items (aref ch pos)) (i 0) (added nil))
        (while (< i (length items))
          (let ((item (nth i items)))
            (when (funcall 'neovm--epc-done-p g item)
              (let ((lhs (car (nth (nth 0 item) g)))
                    (origin (nth 2 item)))
                (dolist (oi (aref ch origin))
                  (let ((sym (funcall 'neovm--epc-next g oi)))
                    (when (and sym (eq sym lhs))
                      (when (funcall 'neovm--epc-add ch pos
                                     (list (nth 0 oi) (1+ (nth 1 oi)) (nth 2 oi)))
                        (setq added t))))))))
          (setq i (1+ i)))
        added)))
  (fset 'neovm--epc-parse
    (lambda (g tokens start)
      (let* ((n (length tokens))
             (ch (make-vector (1+ n) nil)))
        (let ((ri 0))
          (while (< ri (length g))
            (when (eq (car (nth ri g)) start)
              (funcall 'neovm--epc-add ch 0 (list ri 0 0)))
            (setq ri (1+ ri))))
        (let ((pos 0))
          (while (<= pos n)
            (let ((changed t))
              (while changed
                (setq changed nil)
                (when (funcall 'neovm--epc-predict g ch pos) (setq changed t))
                (when (funcall 'neovm--epc-complete g ch pos) (setq changed t))))
            (funcall 'neovm--epc-scan g ch pos tokens)
            (setq pos (1+ pos))))
        ch)))
  ;; Count completed start items spanning [0..n]
  (fset 'neovm--epc-count
    (lambda (g ch n start)
      (let ((count 0))
        (dolist (item (aref ch n))
          (when (and (funcall 'neovm--epc-done-p g item)
                     (= (nth 2 item) 0)
                     (eq (car (nth (nth 0 item) g)) start))
            (setq count (1+ count))))
        count)))
  ;; Count ALL completed items at position (any origin)
  (fset 'neovm--epc-count-all-complete
    (lambda (g ch pos)
      (let ((count 0))
        (dolist (item (aref ch pos))
          (when (funcall 'neovm--epc-done-p g item)
            (setq count (1+ count))))
        count)))

  (unwind-protect
      (let ((grammar '((S S S) (S "a"))))
        (let ((make-input
               (lambda (n)
                 (let ((r nil))
                   (dotimes (_ n) (setq r (cons "a" r)))
                   r))))
          (list
           ;; Parse counts for increasing input lengths (Catalan-like)
           (funcall 'neovm--epc-count grammar
                    (funcall 'neovm--epc-parse grammar (funcall make-input 1) 'S) 1 'S)
           (funcall 'neovm--epc-count grammar
                    (funcall 'neovm--epc-parse grammar (funcall make-input 2) 'S) 2 'S)
           (funcall 'neovm--epc-count grammar
                    (funcall 'neovm--epc-parse grammar (funcall make-input 3) 'S) 3 'S)
           (funcall 'neovm--epc-count grammar
                    (funcall 'neovm--epc-parse grammar (funcall make-input 4) 'S) 4 'S)
           ;; Total completed items at each position for "aaa"
           (let ((ch (funcall 'neovm--epc-parse grammar '("a" "a" "a") 'S)))
             (list
              (funcall 'neovm--epc-count-all-complete grammar ch 0)
              (funcall 'neovm--epc-count-all-complete grammar ch 1)
              (funcall 'neovm--epc-count-all-complete grammar ch 2)
              (funcall 'neovm--epc-count-all-complete grammar ch 3)))
           ;; Total items (not just complete) at each position for "aa"
           (let ((ch (funcall 'neovm--epc-parse grammar '("a" "a") 'S)))
             (list
              (length (aref ch 0))
              (length (aref ch 1))
              (length (aref ch 2)))))))
    (fmakunbound 'neovm--epc-next)
    (fmakunbound 'neovm--epc-done-p)
    (fmakunbound 'neovm--epc-termp)
    (fmakunbound 'neovm--epc-add)
    (fmakunbound 'neovm--epc-predict)
    (fmakunbound 'neovm--epc-scan)
    (fmakunbound 'neovm--epc-complete)
    (fmakunbound 'neovm--epc-parse)
    (fmakunbound 'neovm--epc-count)
    (fmakunbound 'neovm--epc-count-all-complete)))"#;
    let expect = expect_test::expect![[r#""OK (1 1 1 1 (0 1 2 3) (2 4 6))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 4. Earley with operator priority simulation via grammar layering
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_earley_advanced_priority_grammar() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Grammar encoding operator priority:
    //   E  -> E "||" E1 | E1              (lowest: OR)
    //   E1 -> E1 "&&" E2 | E2             (medium: AND)
    //   E2 -> "!" E2 | ATOM               (highest: NOT)
    //   ATOM -> "t" | "f" | "(" E ")"
    // Earley accepts all valid boolean expressions.
    let form = r#"(progn
  (fset 'neovm--epd-next
    (lambda (g item) (nth (nth 1 item) (cdr (nth (nth 0 item) g)))))
  (fset 'neovm--epd-done-p
    (lambda (g item) (>= (nth 1 item) (length (cdr (nth (nth 0 item) g))))))
  (fset 'neovm--epd-termp (lambda (s) (stringp s)))
  (fset 'neovm--epd-add
    (lambda (ch pos item)
      (unless (member item (aref ch pos))
        (aset ch pos (append (aref ch pos) (list item))) t)))
  (fset 'neovm--epd-predict
    (lambda (g ch pos)
      (let ((items (aref ch pos)) (i 0) (added nil))
        (while (< i (length items))
          (let ((sym (funcall 'neovm--epd-next g (nth i items))))
            (when (and sym (not (funcall 'neovm--epd-termp sym)))
              (let ((ri 0))
                (while (< ri (length g))
                  (when (eq (car (nth ri g)) sym)
                    (when (funcall 'neovm--epd-add ch pos (list ri 0 pos))
                      (setq added t)))
                  (setq ri (1+ ri))))))
          (setq i (1+ i)))
        added)))
  (fset 'neovm--epd-scan
    (lambda (g ch pos tokens)
      (when (< pos (length tokens))
        (let ((tok (nth pos tokens)))
          (dolist (item (aref ch pos))
            (let ((sym (funcall 'neovm--epd-next g item)))
              (when (and sym (funcall 'neovm--epd-termp sym) (string= sym tok))
                (funcall 'neovm--epd-add ch (1+ pos)
                         (list (nth 0 item) (1+ (nth 1 item)) (nth 2 item))))))))))
  (fset 'neovm--epd-complete
    (lambda (g ch pos)
      (let ((items (aref ch pos)) (i 0) (added nil))
        (while (< i (length items))
          (let ((item (nth i items)))
            (when (funcall 'neovm--epd-done-p g item)
              (let ((lhs (car (nth (nth 0 item) g)))
                    (origin (nth 2 item)))
                (dolist (oi (aref ch origin))
                  (let ((sym (funcall 'neovm--epd-next g oi)))
                    (when (and sym (eq sym lhs))
                      (when (funcall 'neovm--epd-add ch pos
                                     (list (nth 0 oi) (1+ (nth 1 oi)) (nth 2 oi)))
                        (setq added t))))))))
          (setq i (1+ i)))
        added)))
  (fset 'neovm--epd-parse
    (lambda (g tokens start)
      (let* ((n (length tokens))
             (ch (make-vector (1+ n) nil)))
        (let ((ri 0))
          (while (< ri (length g))
            (when (eq (car (nth ri g)) start)
              (funcall 'neovm--epd-add ch 0 (list ri 0 0)))
            (setq ri (1+ ri))))
        (let ((pos 0))
          (while (<= pos n)
            (let ((changed t))
              (while changed
                (setq changed nil)
                (when (funcall 'neovm--epd-predict g ch pos) (setq changed t))
                (when (funcall 'neovm--epd-complete g ch pos) (setq changed t))))
            (funcall 'neovm--epd-scan g ch pos tokens)
            (setq pos (1+ pos))))
        ch)))
  (fset 'neovm--epd-ok-p
    (lambda (g ch n start)
      (let ((found nil))
        (dolist (item (aref ch n))
          (when (and (funcall 'neovm--epd-done-p g item)
                     (= (nth 2 item) 0)
                     (eq (car (nth (nth 0 item) g)) start))
            (setq found t)))
        found)))

  (unwind-protect
      (let ((grammar '((E E "||" E1)        ;; 0
                        (E E1)               ;; 1
                        (E1 E1 "&&" E2)      ;; 2
                        (E1 E2)              ;; 3
                        (E2 "!" E2)          ;; 4
                        (E2 ATOM)            ;; 5
                        (ATOM "t")           ;; 6
                        (ATOM "f")           ;; 7
                        (ATOM "(" E ")"))))  ;; 8
        (list
         ;; "t" -> accepted
         (let ((c (funcall 'neovm--epd-parse grammar '("t") 'E)))
           (funcall 'neovm--epd-ok-p grammar c 1 'E))
         ;; "t || f" -> accepted
         (let ((c (funcall 'neovm--epd-parse grammar '("t" "||" "f") 'E)))
           (funcall 'neovm--epd-ok-p grammar c 3 'E))
         ;; "t && f" -> accepted
         (let ((c (funcall 'neovm--epd-parse grammar '("t" "&&" "f") 'E)))
           (funcall 'neovm--epd-ok-p grammar c 3 'E))
         ;; "! t" -> accepted
         (let ((c (funcall 'neovm--epd-parse grammar '("!" "t") 'E)))
           (funcall 'neovm--epd-ok-p grammar c 2 'E))
         ;; "! ! f" -> accepted (double negation)
         (let ((c (funcall 'neovm--epd-parse grammar '("!" "!" "f") 'E)))
           (funcall 'neovm--epd-ok-p grammar c 3 'E))
         ;; "t || f && t" -> accepted (priority: && binds tighter)
         (let ((c (funcall 'neovm--epd-parse grammar '("t" "||" "f" "&&" "t") 'E)))
           (funcall 'neovm--epd-ok-p grammar c 5 'E))
         ;; "( t || f ) && t" -> accepted (parens)
         (let ((c (funcall 'neovm--epd-parse grammar
                           '("(" "t" "||" "f" ")" "&&" "t") 'E)))
           (funcall 'neovm--epd-ok-p grammar c 7 'E))
         ;; "||" alone -> rejected
         (let ((c (funcall 'neovm--epd-parse grammar '("||") 'E)))
           (funcall 'neovm--epd-ok-p grammar c 1 'E))
         ;; "t &&" -> rejected (incomplete)
         (let ((c (funcall 'neovm--epd-parse grammar '("t" "&&") 'E)))
           (funcall 'neovm--epd-ok-p grammar c 2 'E))))
    (fmakunbound 'neovm--epd-next)
    (fmakunbound 'neovm--epd-done-p)
    (fmakunbound 'neovm--epd-termp)
    (fmakunbound 'neovm--epd-add)
    (fmakunbound 'neovm--epd-predict)
    (fmakunbound 'neovm--epd-scan)
    (fmakunbound 'neovm--epd-complete)
    (fmakunbound 'neovm--epd-parse)
    (fmakunbound 'neovm--epd-ok-p)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 5. Error reporting: furthest match position
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_earley_advanced_furthest_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Track the furthest chart position that contains any items,
    // which indicates how far the parser got before failing.
    let form = r#"(progn
  (fset 'neovm--epe-next
    (lambda (g item) (nth (nth 1 item) (cdr (nth (nth 0 item) g)))))
  (fset 'neovm--epe-done-p
    (lambda (g item) (>= (nth 1 item) (length (cdr (nth (nth 0 item) g))))))
  (fset 'neovm--epe-termp (lambda (s) (stringp s)))
  (fset 'neovm--epe-add
    (lambda (ch pos item)
      (unless (member item (aref ch pos))
        (aset ch pos (append (aref ch pos) (list item))) t)))
  (fset 'neovm--epe-predict
    (lambda (g ch pos)
      (let ((items (aref ch pos)) (i 0) (added nil))
        (while (< i (length items))
          (let ((sym (funcall 'neovm--epe-next g (nth i items))))
            (when (and sym (not (funcall 'neovm--epe-termp sym)))
              (let ((ri 0))
                (while (< ri (length g))
                  (when (eq (car (nth ri g)) sym)
                    (when (funcall 'neovm--epe-add ch pos (list ri 0 pos))
                      (setq added t)))
                  (setq ri (1+ ri))))))
          (setq i (1+ i)))
        added)))
  (fset 'neovm--epe-scan
    (lambda (g ch pos tokens)
      (when (< pos (length tokens))
        (let ((tok (nth pos tokens)))
          (dolist (item (aref ch pos))
            (let ((sym (funcall 'neovm--epe-next g item)))
              (when (and sym (funcall 'neovm--epe-termp sym) (string= sym tok))
                (funcall 'neovm--epe-add ch (1+ pos)
                         (list (nth 0 item) (1+ (nth 1 item)) (nth 2 item))))))))))
  (fset 'neovm--epe-complete
    (lambda (g ch pos)
      (let ((items (aref ch pos)) (i 0) (added nil))
        (while (< i (length items))
          (let ((item (nth i items)))
            (when (funcall 'neovm--epe-done-p g item)
              (let ((lhs (car (nth (nth 0 item) g)))
                    (origin (nth 2 item)))
                (dolist (oi (aref ch origin))
                  (let ((sym (funcall 'neovm--epe-next g oi)))
                    (when (and sym (eq sym lhs))
                      (when (funcall 'neovm--epe-add ch pos
                                     (list (nth 0 oi) (1+ (nth 1 oi)) (nth 2 oi)))
                        (setq added t))))))))
          (setq i (1+ i)))
        added)))
  (fset 'neovm--epe-parse
    (lambda (g tokens start)
      (let* ((n (length tokens))
             (ch (make-vector (1+ n) nil)))
        (let ((ri 0))
          (while (< ri (length g))
            (when (eq (car (nth ri g)) start)
              (funcall 'neovm--epe-add ch 0 (list ri 0 0)))
            (setq ri (1+ ri))))
        (let ((pos 0))
          (while (<= pos n)
            (let ((changed t))
              (while changed
                (setq changed nil)
                (when (funcall 'neovm--epe-predict g ch pos) (setq changed t))
                (when (funcall 'neovm--epe-complete g ch pos) (setq changed t))))
            (funcall 'neovm--epe-scan g ch pos tokens)
            (setq pos (1+ pos))))
        ch)))
  ;; Find the furthest position that has any items
  (fset 'neovm--epe-furthest
    (lambda (ch n)
      (let ((furthest 0) (pos 0))
        (while (<= pos n)
          (when (aref ch pos)
            (setq furthest pos))
          (setq pos (1+ pos)))
        furthest)))

  (unwind-protect
      (let ((grammar '((S A B C)
                        (A "a")
                        (B "b")
                        (C "c"))))
        (list
         ;; "a b c" -> furthest = 3 (complete parse)
         (let ((c (funcall 'neovm--epe-parse grammar '("a" "b" "c") 'S)))
           (funcall 'neovm--epe-furthest c 3))
         ;; "a b" -> furthest = 2 (got through a and b, but no c)
         (let ((c (funcall 'neovm--epe-parse grammar '("a" "b") 'S)))
           (funcall 'neovm--epe-furthest c 2))
         ;; "a" -> furthest = 1
         (let ((c (funcall 'neovm--epe-parse grammar '("a") 'S)))
           (funcall 'neovm--epe-furthest c 1))
         ;; "x y z" -> furthest = 0 (no match at all)
         (let ((c (funcall 'neovm--epe-parse grammar '("x" "y" "z") 'S)))
           (funcall 'neovm--epe-furthest c 3))
         ;; "a x c" -> furthest = 1 (matched "a", failed on "x")
         (let ((c (funcall 'neovm--epe-parse grammar '("a" "x" "c") 'S)))
           (funcall 'neovm--epe-furthest c 3))
         ;; "a b c d" -> furthest = 3 (parsed ok but extra token at end)
         (let ((c (funcall 'neovm--epe-parse grammar '("a" "b" "c" "d") 'S)))
           (funcall 'neovm--epe-furthest c 4))
         ;; Empty input -> furthest = 0
         (let ((c (funcall 'neovm--epe-parse grammar '() 'S)))
           (funcall 'neovm--epe-furthest c 0))))
    (fmakunbound 'neovm--epe-next)
    (fmakunbound 'neovm--epe-done-p)
    (fmakunbound 'neovm--epe-termp)
    (fmakunbound 'neovm--epe-add)
    (fmakunbound 'neovm--epe-predict)
    (fmakunbound 'neovm--epe-scan)
    (fmakunbound 'neovm--epe-complete)
    (fmakunbound 'neovm--epe-parse)
    (fmakunbound 'neovm--epe-furthest)))"#;
    let expect = expect_test::expect![[r#""OK (3 2 1 0 1 3 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 6. Right-recursive vs left-recursive: both accept same language
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_earley_advanced_right_vs_left_recursive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compare left-recursive L -> L "+" ATOM | ATOM
    //      vs right-recursive R -> ATOM "+" R | ATOM
    // Both should accept the same inputs.
    let form = r#"(progn
  (fset 'neovm--epf-next
    (lambda (g item) (nth (nth 1 item) (cdr (nth (nth 0 item) g)))))
  (fset 'neovm--epf-done-p
    (lambda (g item) (>= (nth 1 item) (length (cdr (nth (nth 0 item) g))))))
  (fset 'neovm--epf-termp (lambda (s) (stringp s)))
  (fset 'neovm--epf-add
    (lambda (ch pos item)
      (unless (member item (aref ch pos))
        (aset ch pos (append (aref ch pos) (list item))) t)))
  (fset 'neovm--epf-predict
    (lambda (g ch pos)
      (let ((items (aref ch pos)) (i 0) (added nil))
        (while (< i (length items))
          (let ((sym (funcall 'neovm--epf-next g (nth i items))))
            (when (and sym (not (funcall 'neovm--epf-termp sym)))
              (let ((ri 0))
                (while (< ri (length g))
                  (when (eq (car (nth ri g)) sym)
                    (when (funcall 'neovm--epf-add ch pos (list ri 0 pos))
                      (setq added t)))
                  (setq ri (1+ ri))))))
          (setq i (1+ i)))
        added)))
  (fset 'neovm--epf-scan
    (lambda (g ch pos tokens)
      (when (< pos (length tokens))
        (let ((tok (nth pos tokens)))
          (dolist (item (aref ch pos))
            (let ((sym (funcall 'neovm--epf-next g item)))
              (when (and sym (funcall 'neovm--epf-termp sym) (string= sym tok))
                (funcall 'neovm--epf-add ch (1+ pos)
                         (list (nth 0 item) (1+ (nth 1 item)) (nth 2 item))))))))))
  (fset 'neovm--epf-complete
    (lambda (g ch pos)
      (let ((items (aref ch pos)) (i 0) (added nil))
        (while (< i (length items))
          (let ((item (nth i items)))
            (when (funcall 'neovm--epf-done-p g item)
              (let ((lhs (car (nth (nth 0 item) g)))
                    (origin (nth 2 item)))
                (dolist (oi (aref ch origin))
                  (let ((sym (funcall 'neovm--epf-next g oi)))
                    (when (and sym (eq sym lhs))
                      (when (funcall 'neovm--epf-add ch pos
                                     (list (nth 0 oi) (1+ (nth 1 oi)) (nth 2 oi)))
                        (setq added t))))))))
          (setq i (1+ i)))
        added)))
  (fset 'neovm--epf-parse
    (lambda (g tokens start)
      (let* ((n (length tokens))
             (ch (make-vector (1+ n) nil)))
        (let ((ri 0))
          (while (< ri (length g))
            (when (eq (car (nth ri g)) start)
              (funcall 'neovm--epf-add ch 0 (list ri 0 0)))
            (setq ri (1+ ri))))
        (let ((pos 0))
          (while (<= pos n)
            (let ((changed t))
              (while changed
                (setq changed nil)
                (when (funcall 'neovm--epf-predict g ch pos) (setq changed t))
                (when (funcall 'neovm--epf-complete g ch pos) (setq changed t))))
            (funcall 'neovm--epf-scan g ch pos tokens)
            (setq pos (1+ pos))))
        ch)))
  (fset 'neovm--epf-ok-p
    (lambda (g ch n start)
      (let ((found nil))
        (dolist (item (aref ch n))
          (when (and (funcall 'neovm--epf-done-p g item)
                     (= (nth 2 item) 0)
                     (eq (car (nth (nth 0 item) g)) start))
            (setq found t)))
        found)))

  (unwind-protect
      (let ((left-g  '((L L "+" ATOM) (L ATOM) (ATOM "n")))
            (right-g '((R ATOM "+" R) (R ATOM) (ATOM "n"))))
        (let ((inputs '(("n")
                         ("n" "+" "n")
                         ("n" "+" "n" "+" "n")
                         ("n" "+" "n" "+" "n" "+" "n")
                         ("+")
                         ("n" "+")
                         ("+" "n")
                         ())))
          (mapcar
           (lambda (toks)
             (let ((n (length toks)))
               (list
                (funcall 'neovm--epf-ok-p left-g
                         (funcall 'neovm--epf-parse left-g toks 'L) n 'L)
                (funcall 'neovm--epf-ok-p right-g
                         (funcall 'neovm--epf-parse right-g toks 'R) n 'R))))
           inputs)))
    (fmakunbound 'neovm--epf-next)
    (fmakunbound 'neovm--epf-done-p)
    (fmakunbound 'neovm--epf-termp)
    (fmakunbound 'neovm--epf-add)
    (fmakunbound 'neovm--epf-predict)
    (fmakunbound 'neovm--epf-scan)
    (fmakunbound 'neovm--epf-complete)
    (fmakunbound 'neovm--epf-parse)
    (fmakunbound 'neovm--epf-ok-p)))"#;
    let expect = expect_test::expect![[
        r#""OK ((t t) (t t) (t t) (t t) (nil nil) (nil nil) (nil nil) (nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 7. Grammar with indirect left recursion (A -> B ..., B -> A ...)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_earley_advanced_indirect_left_recursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Indirect left recursion:
    //   A -> B "x"
    //   B -> A "y" | "z"
    // Language: z(yx)* -- z, zyx, zyxyx, ...
    let form = r#"(progn
  (fset 'neovm--epg-next
    (lambda (g item) (nth (nth 1 item) (cdr (nth (nth 0 item) g)))))
  (fset 'neovm--epg-done-p
    (lambda (g item) (>= (nth 1 item) (length (cdr (nth (nth 0 item) g))))))
  (fset 'neovm--epg-termp (lambda (s) (stringp s)))
  (fset 'neovm--epg-add
    (lambda (ch pos item)
      (unless (member item (aref ch pos))
        (aset ch pos (append (aref ch pos) (list item))) t)))
  (fset 'neovm--epg-predict
    (lambda (g ch pos)
      (let ((items (aref ch pos)) (i 0) (added nil))
        (while (< i (length items))
          (let ((sym (funcall 'neovm--epg-next g (nth i items))))
            (when (and sym (not (funcall 'neovm--epg-termp sym)))
              (let ((ri 0))
                (while (< ri (length g))
                  (when (eq (car (nth ri g)) sym)
                    (when (funcall 'neovm--epg-add ch pos (list ri 0 pos))
                      (setq added t)))
                  (setq ri (1+ ri))))))
          (setq i (1+ i)))
        added)))
  (fset 'neovm--epg-scan
    (lambda (g ch pos tokens)
      (when (< pos (length tokens))
        (let ((tok (nth pos tokens)))
          (dolist (item (aref ch pos))
            (let ((sym (funcall 'neovm--epg-next g item)))
              (when (and sym (funcall 'neovm--epg-termp sym) (string= sym tok))
                (funcall 'neovm--epg-add ch (1+ pos)
                         (list (nth 0 item) (1+ (nth 1 item)) (nth 2 item))))))))))
  (fset 'neovm--epg-complete
    (lambda (g ch pos)
      (let ((items (aref ch pos)) (i 0) (added nil))
        (while (< i (length items))
          (let ((item (nth i items)))
            (when (funcall 'neovm--epg-done-p g item)
              (let ((lhs (car (nth (nth 0 item) g)))
                    (origin (nth 2 item)))
                (dolist (oi (aref ch origin))
                  (let ((sym (funcall 'neovm--epg-next g oi)))
                    (when (and sym (eq sym lhs))
                      (when (funcall 'neovm--epg-add ch pos
                                     (list (nth 0 oi) (1+ (nth 1 oi)) (nth 2 oi)))
                        (setq added t))))))))
          (setq i (1+ i)))
        added)))
  (fset 'neovm--epg-parse
    (lambda (g tokens start)
      (let* ((n (length tokens))
             (ch (make-vector (1+ n) nil)))
        (let ((ri 0))
          (while (< ri (length g))
            (when (eq (car (nth ri g)) start)
              (funcall 'neovm--epg-add ch 0 (list ri 0 0)))
            (setq ri (1+ ri))))
        (let ((pos 0))
          (while (<= pos n)
            (let ((changed t))
              (while changed
                (setq changed nil)
                (when (funcall 'neovm--epg-predict g ch pos) (setq changed t))
                (when (funcall 'neovm--epg-complete g ch pos) (setq changed t))))
            (funcall 'neovm--epg-scan g ch pos tokens)
            (setq pos (1+ pos))))
        ch)))
  (fset 'neovm--epg-ok-p
    (lambda (g ch n start)
      (let ((found nil))
        (dolist (item (aref ch n))
          (when (and (funcall 'neovm--epg-done-p g item)
                     (= (nth 2 item) 0)
                     (eq (car (nth (nth 0 item) g)) start))
            (setq found t)))
        found)))

  (unwind-protect
      (let ((grammar '((A B "x")    ;; 0: A -> B x
                        (B A "y")    ;; 1: B -> A y  (indirect left recursion via A)
                        (B "z"))))   ;; 2: B -> z
        (list
         ;; "z x" -> accepted (B->z, A->Bx)
         (let ((c (funcall 'neovm--epg-parse grammar '("z" "x") 'A)))
           (funcall 'neovm--epg-ok-p grammar c 2 'A))
         ;; "z x y x" -> accepted (z, then yx pattern once)
         (let ((c (funcall 'neovm--epg-parse grammar '("z" "x" "y" "x") 'A)))
           (funcall 'neovm--epg-ok-p grammar c 4 'A))
         ;; "z x y x y x" -> accepted (z, then yx pattern twice)
         (let ((c (funcall 'neovm--epg-parse grammar '("z" "x" "y" "x" "y" "x") 'A)))
           (funcall 'neovm--epg-ok-p grammar c 6 'A))
         ;; "z" alone -> rejected (need A -> B "x", so need trailing x)
         (let ((c (funcall 'neovm--epg-parse grammar '("z") 'A)))
           (funcall 'neovm--epg-ok-p grammar c 1 'A))
         ;; "x" alone -> rejected
         (let ((c (funcall 'neovm--epg-parse grammar '("x") 'A)))
           (funcall 'neovm--epg-ok-p grammar c 1 'A))
         ;; "z x y" -> rejected (trailing y without x)
         (let ((c (funcall 'neovm--epg-parse grammar '("z" "x" "y") 'A)))
           (funcall 'neovm--epg-ok-p grammar c 3 'A))
         ;; Empty -> rejected
         (let ((c (funcall 'neovm--epg-parse grammar '() 'A)))
           (funcall 'neovm--epg-ok-p grammar c 0 'A))))
    (fmakunbound 'neovm--epg-next)
    (fmakunbound 'neovm--epg-done-p)
    (fmakunbound 'neovm--epg-termp)
    (fmakunbound 'neovm--epg-add)
    (fmakunbound 'neovm--epg-predict)
    (fmakunbound 'neovm--epg-scan)
    (fmakunbound 'neovm--epg-complete)
    (fmakunbound 'neovm--epg-parse)
    (fmakunbound 'neovm--epg-ok-p)))"#;
    let expect = expect_test::expect![[r#""OK (t t t nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
