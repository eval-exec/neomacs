//! Oracle parity tests for pattern matching implemented in pure Elisp.
//!
//! Implements a `pmatch` macro-like function for matching against literal
//! values, cons/list patterns (destructuring), wildcard (_), guard clauses,
//! nested patterns, or-patterns, binding patterns (capture variables),
//! and match failure. Tests with various data structures and complex nesting.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Core pattern matching engine: literal, wildcard, cons, binding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pattern_match_core_engine() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a pattern matcher as functions. Patterns are:
    //   'wildcard         - matches anything
    //   (quote val)       - matches literal val
    //   (cons p1 p2)      - matches a cons cell, recursively matches car/cdr
    //   (bind name)       - matches anything, captures in bindings alist
    //   (guard pat pred)  - matches pat, then checks pred with bindings
    //   (or p1 p2 ...)    - matches if any sub-pattern matches
    let form = r#"(progn
  (fset 'neovm--pm-match
    (lambda (pattern value bindings)
      "Try to match VALUE against PATTERN. Returns (t . bindings) or (nil . nil)."
      (cond
       ;; Wildcard: matches anything
       ((eq pattern 'wildcard)
        (cons t bindings))
       ;; Literal quote: (quote val) - match exact value
       ((and (consp pattern) (eq (car pattern) 'literal))
        (if (equal (cadr pattern) value)
            (cons t bindings)
          (cons nil nil)))
       ;; Cons pattern: (cpat car-pat cdr-pat)
       ((and (consp pattern) (eq (car pattern) 'cpat))
        (if (consp value)
            (let* ((car-result (funcall 'neovm--pm-match (nth 1 pattern) (car value) bindings)))
              (if (car car-result)
                  (funcall 'neovm--pm-match (nth 2 pattern) (cdr value) (cdr car-result))
                (cons nil nil)))
          (cons nil nil)))
       ;; Bind pattern: (bind name) - capture value
       ((and (consp pattern) (eq (car pattern) 'bind))
        (cons t (cons (cons (cadr pattern) value) bindings)))
       ;; Guard pattern: (guard pat pred-fn)
       ((and (consp pattern) (eq (car pattern) 'guard))
        (let* ((sub-result (funcall 'neovm--pm-match (nth 1 pattern) value bindings)))
          (if (car sub-result)
              (if (funcall (nth 2 pattern) value (cdr sub-result))
                  sub-result
                (cons nil nil))
            (cons nil nil))))
       ;; Or pattern: (orp p1 p2 ...)
       ((and (consp pattern) (eq (car pattern) 'orp))
        (let ((pats (cdr pattern))
              (result (cons nil nil)))
          (while (and pats (not (car result)))
            (setq result (funcall 'neovm--pm-match (car pats) value bindings))
            (setq pats (cdr pats)))
          result))
       ;; No match
       (t (cons nil nil)))))

  (unwind-protect
      (list
        ;; Wildcard matches anything
        (funcall 'neovm--pm-match 'wildcard 42 nil)
        (funcall 'neovm--pm-match 'wildcard "hello" nil)
        (funcall 'neovm--pm-match 'wildcard nil nil)
        ;; Literal matches exact value
        (funcall 'neovm--pm-match '(literal 42) 42 nil)
        (funcall 'neovm--pm-match '(literal 42) 43 nil)
        (funcall 'neovm--pm-match '(literal "hello") "hello" nil)
        (funcall 'neovm--pm-match '(literal "hello") "world" nil)
        ;; Bind captures value
        (funcall 'neovm--pm-match '(bind x) 99 nil)
        (funcall 'neovm--pm-match '(bind name) "Alice" nil)
        ;; Cons pattern destructures pairs
        (funcall 'neovm--pm-match
          '(cpat (bind a) (bind b))
          '(1 . 2)
          nil)
        ;; Cons pattern fails on non-cons
        (funcall 'neovm--pm-match '(cpat (bind a) (bind b)) 42 nil))
    (fmakunbound 'neovm--pm-match)))"#;
    let expect = expect_test::expect![[
        r#""OK ((t) (t) (t) (t) (nil) (t) (nil) (t (x . 99)) (t (name . \"Alice\")) (t (b . 2) (a . 1)) (nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// List pattern matching with nested cons
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pattern_match_list_destructuring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Match against list patterns by encoding them as nested cons patterns.
    // (list a b c) = (cpat a (cpat b (cpat c (literal nil))))
    let form = r#"(progn
  (fset 'neovm--pm-match
    (lambda (pattern value bindings)
      (cond
       ((eq pattern 'wildcard) (cons t bindings))
       ((and (consp pattern) (eq (car pattern) 'literal))
        (if (equal (cadr pattern) value) (cons t bindings) (cons nil nil)))
       ((and (consp pattern) (eq (car pattern) 'cpat))
        (if (consp value)
            (let* ((cr (funcall 'neovm--pm-match (nth 1 pattern) (car value) bindings)))
              (if (car cr)
                  (funcall 'neovm--pm-match (nth 2 pattern) (cdr value) (cdr cr))
                (cons nil nil)))
          (cons nil nil)))
       ((and (consp pattern) (eq (car pattern) 'bind))
        (cons t (cons (cons (cadr pattern) value) bindings)))
       ((and (consp pattern) (eq (car pattern) 'guard))
        (let* ((sr (funcall 'neovm--pm-match (nth 1 pattern) value bindings)))
          (if (car sr)
              (if (funcall (nth 2 pattern) value (cdr sr))
                  sr (cons nil nil))
            (cons nil nil))))
       ((and (consp pattern) (eq (car pattern) 'orp))
        (let ((pats (cdr pattern)) (result (cons nil nil)))
          (while (and pats (not (car result)))
            (setq result (funcall 'neovm--pm-match (car pats) value bindings))
            (setq pats (cdr pats)))
          result))
       (t (cons nil nil)))))

  ;; Helper: build a list pattern from element patterns
  (fset 'neovm--pm-list-pat
    (lambda (pats)
      "Build nested cpat from list of element patterns."
      (if (null pats)
          '(literal nil)
        (list 'cpat (car pats) (funcall 'neovm--pm-list-pat (cdr pats))))))

  (unwind-protect
      (let (;; Match (1 2 3) against (bind a, bind b, bind c)
            (pat3 (funcall 'neovm--pm-list-pat '((bind a) (bind b) (bind c)))))
        (list
          ;; Successful 3-element list match
          (funcall 'neovm--pm-match pat3 '(1 2 3) nil)
          ;; Fail: too few elements
          (funcall 'neovm--pm-match pat3 '(1 2) nil)
          ;; Fail: too many elements
          (funcall 'neovm--pm-match pat3 '(1 2 3 4) nil)
          ;; Match with mixed literal and bind
          (let ((mixed (funcall 'neovm--pm-list-pat
                         '((literal 'ok) (bind val) (literal 'end)))))
            (list
              (funcall 'neovm--pm-match mixed '(ok 42 end) nil)
              (funcall 'neovm--pm-match mixed '(ok 42 start) nil)
              (funcall 'neovm--pm-match mixed '(fail 42 end) nil)))
          ;; Nested list: match ((1 2) (3 4))
          (let ((nested (funcall 'neovm--pm-list-pat
                          (list (funcall 'neovm--pm-list-pat '((bind a) (bind b)))
                                (funcall 'neovm--pm-list-pat '((bind c) (bind d)))))))
            (funcall 'neovm--pm-match nested '((1 2) (3 4)) nil))
          ;; Empty list match
          (funcall 'neovm--pm-match '(literal nil) nil nil)
          (funcall 'neovm--pm-match '(literal nil) '(1) nil)))
    (fmakunbound 'neovm--pm-match)
    (fmakunbound 'neovm--pm-list-pat)))"#;
    let expect = expect_test::expect![[
        r#""OK ((t (c . 3) (b . 2) (a . 1)) (nil) (nil) ((nil) (nil) (nil)) (t (d . 4) (c . 3) (b . 2) (a . 1)) (t) (nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Guard clauses: match with additional predicates
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pattern_match_guards() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Guard patterns check an additional predicate after the structural match.
    let form = r#"(progn
  (fset 'neovm--pm-match
    (lambda (pattern value bindings)
      (cond
       ((eq pattern 'wildcard) (cons t bindings))
       ((and (consp pattern) (eq (car pattern) 'literal))
        (if (equal (cadr pattern) value) (cons t bindings) (cons nil nil)))
       ((and (consp pattern) (eq (car pattern) 'cpat))
        (if (consp value)
            (let* ((cr (funcall 'neovm--pm-match (nth 1 pattern) (car value) bindings)))
              (if (car cr)
                  (funcall 'neovm--pm-match (nth 2 pattern) (cdr value) (cdr cr))
                (cons nil nil)))
          (cons nil nil)))
       ((and (consp pattern) (eq (car pattern) 'bind))
        (cons t (cons (cons (cadr pattern) value) bindings)))
       ((and (consp pattern) (eq (car pattern) 'guard))
        (let* ((sr (funcall 'neovm--pm-match (nth 1 pattern) value bindings)))
          (if (car sr)
              (if (funcall (nth 2 pattern) value (cdr sr))
                  sr (cons nil nil))
            (cons nil nil))))
       ((and (consp pattern) (eq (car pattern) 'orp))
        (let ((pats (cdr pattern)) (result (cons nil nil)))
          (while (and pats (not (car result)))
            (setq result (funcall 'neovm--pm-match (car pats) value bindings))
            (setq pats (cdr pats)))
          result))
       (t (cons nil nil)))))

  (unwind-protect
      (list
        ;; Guard: bind x, but only if x > 10
        (funcall 'neovm--pm-match
          (list 'guard '(bind x) (lambda (v _binds) (> v 10)))
          15 nil)
        ;; Guard fails: value too small
        (funcall 'neovm--pm-match
          (list 'guard '(bind x) (lambda (v _binds) (> v 10)))
          5 nil)
        ;; Guard on string length
        (funcall 'neovm--pm-match
          (list 'guard '(bind s) (lambda (v _binds) (> (length v) 3)))
          "hello" nil)
        (funcall 'neovm--pm-match
          (list 'guard '(bind s) (lambda (v _binds) (> (length v) 3)))
          "hi" nil)
        ;; Guard checking bindings: match (a . b) where a < b
        (funcall 'neovm--pm-match
          (list 'guard
                '(cpat (bind a) (bind b))
                (lambda (_v binds)
                  (let ((a-val (cdr (assq 'a binds)))
                        (b-val (cdr (assq 'b binds))))
                    (< a-val b-val))))
          '(3 . 7) nil)
        ;; Guard fails: a >= b
        (funcall 'neovm--pm-match
          (list 'guard
                '(cpat (bind a) (bind b))
                (lambda (_v binds)
                  (let ((a-val (cdr (assq 'a binds)))
                        (b-val (cdr (assq 'b binds))))
                    (< a-val b-val))))
          '(7 . 3) nil)
        ;; Guard with even? predicate
        (funcall 'neovm--pm-match
          (list 'guard '(bind n) (lambda (v _b) (= 0 (% v 2))))
          42 nil)
        (funcall 'neovm--pm-match
          (list 'guard '(bind n) (lambda (v _b) (= 0 (% v 2))))
          43 nil))
    (fmakunbound 'neovm--pm-match)))"#;
    let expect = expect_test::expect![[
        r#""OK ((t (x . 15)) (nil) (t (s . \"hello\")) (nil) (t (b . 7) (a . 3)) (nil) (t (n . 42)) (nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Or-patterns: match against multiple alternatives
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pattern_match_or_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--pm-match
    (lambda (pattern value bindings)
      (cond
       ((eq pattern 'wildcard) (cons t bindings))
       ((and (consp pattern) (eq (car pattern) 'literal))
        (if (equal (cadr pattern) value) (cons t bindings) (cons nil nil)))
       ((and (consp pattern) (eq (car pattern) 'cpat))
        (if (consp value)
            (let* ((cr (funcall 'neovm--pm-match (nth 1 pattern) (car value) bindings)))
              (if (car cr)
                  (funcall 'neovm--pm-match (nth 2 pattern) (cdr value) (cdr cr))
                (cons nil nil)))
          (cons nil nil)))
       ((and (consp pattern) (eq (car pattern) 'bind))
        (cons t (cons (cons (cadr pattern) value) bindings)))
       ((and (consp pattern) (eq (car pattern) 'guard))
        (let* ((sr (funcall 'neovm--pm-match (nth 1 pattern) value bindings)))
          (if (car sr)
              (if (funcall (nth 2 pattern) value (cdr sr))
                  sr (cons nil nil))
            (cons nil nil))))
       ((and (consp pattern) (eq (car pattern) 'orp))
        (let ((pats (cdr pattern)) (result (cons nil nil)))
          (while (and pats (not (car result)))
            (setq result (funcall 'neovm--pm-match (car pats) value bindings))
            (setq pats (cdr pats)))
          result))
       (t (cons nil nil)))))

  (unwind-protect
      (list
        ;; Or of two literals: match 1 or 2
        (funcall 'neovm--pm-match '(orp (literal 1) (literal 2)) 1 nil)
        (funcall 'neovm--pm-match '(orp (literal 1) (literal 2)) 2 nil)
        (funcall 'neovm--pm-match '(orp (literal 1) (literal 2)) 3 nil)
        ;; Or of three patterns: literal, wildcard, bind
        (funcall 'neovm--pm-match
          '(orp (literal 'special) (bind x))
          'special nil)
        (funcall 'neovm--pm-match
          '(orp (literal 'special) (bind x))
          'other nil)
        ;; Or with cons patterns: match either (ok val) or (err msg)
        (funcall 'neovm--pm-match
          '(orp (cpat (literal ok) (bind v))
                (cpat (literal err) (bind v)))
          '(ok . 42) nil)
        (funcall 'neovm--pm-match
          '(orp (cpat (literal ok) (bind v))
                (cpat (literal err) (bind v)))
          '(err . "oops") nil)
        (funcall 'neovm--pm-match
          '(orp (cpat (literal ok) (bind v))
                (cpat (literal err) (bind v)))
          '(unknown . nil) nil)
        ;; Nested or: (or 1 (or 2 3))
        (funcall 'neovm--pm-match
          '(orp (literal 1) (orp (literal 2) (literal 3)))
          3 nil)
        ;; Or picks first matching branch (bindings from first match)
        (funcall 'neovm--pm-match
          '(orp (bind first) (bind second))
          99 nil))
    (fmakunbound 'neovm--pm-match)))"#;
    let expect = expect_test::expect![[
        r#""OK ((t) (t) (nil) (t (x . special)) (t (x . other)) (t (v . 42)) (t (v . \"oops\")) (nil) (t) (t (first . 99)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-clause match dispatcher: try patterns in order, return result
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pattern_match_dispatcher() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a match-case dispatcher: list of (pattern . action) clauses,
    // try each pattern, run action of first match with captured bindings.
    let form = r#"(progn
  (fset 'neovm--pm-match
    (lambda (pattern value bindings)
      (cond
       ((eq pattern 'wildcard) (cons t bindings))
       ((and (consp pattern) (eq (car pattern) 'literal))
        (if (equal (cadr pattern) value) (cons t bindings) (cons nil nil)))
       ((and (consp pattern) (eq (car pattern) 'cpat))
        (if (consp value)
            (let* ((cr (funcall 'neovm--pm-match (nth 1 pattern) (car value) bindings)))
              (if (car cr)
                  (funcall 'neovm--pm-match (nth 2 pattern) (cdr value) (cdr cr))
                (cons nil nil)))
          (cons nil nil)))
       ((and (consp pattern) (eq (car pattern) 'bind))
        (cons t (cons (cons (cadr pattern) value) bindings)))
       ((and (consp pattern) (eq (car pattern) 'guard))
        (let* ((sr (funcall 'neovm--pm-match (nth 1 pattern) value bindings)))
          (if (car sr)
              (if (funcall (nth 2 pattern) value (cdr sr))
                  sr (cons nil nil))
            (cons nil nil))))
       ((and (consp pattern) (eq (car pattern) 'orp))
        (let ((pats (cdr pattern)) (result (cons nil nil)))
          (while (and pats (not (car result)))
            (setq result (funcall 'neovm--pm-match (car pats) value bindings))
            (setq pats (cdr pats)))
          result))
       (t (cons nil nil)))))

  (fset 'neovm--pm-dispatch
    (lambda (value clauses)
      "Match VALUE against CLAUSES: list of (pattern action-fn).
       Action-fn receives bindings alist. Returns result of first match or 'no-match."
      (let ((remaining clauses)
            (result nil)
            (matched nil))
        (while (and remaining (not matched))
          (let* ((clause (car remaining))
                 (pat (car clause))
                 (action (cadr clause))
                 (mr (funcall 'neovm--pm-match pat value nil)))
            (when (car mr)
              (setq matched t)
              (setq result (funcall action (cdr mr)))))
          (setq remaining (cdr remaining)))
        (if matched result 'no-match))))

  (unwind-protect
      (let ((clauses
             (list
              ;; Clause 1: match nil -> "empty"
              (list '(literal nil)
                    (lambda (_b) "empty"))
              ;; Clause 2: match (ok . val) where val > 0 -> format
              (list (list 'guard '(cpat (literal ok) (bind v))
                          (lambda (_v binds) (> (cdr (assq 'v binds)) 0)))
                    (lambda (b) (format "ok:%d" (cdr (assq 'v b)))))
              ;; Clause 3: match (err . msg) -> format error
              (list '(cpat (literal err) (bind msg))
                    (lambda (b) (format "error:%s" (cdr (assq 'msg b)))))
              ;; Clause 4: wildcard -> "unknown"
              (list 'wildcard
                    (lambda (_b) "unknown")))))
        (list
          (funcall 'neovm--pm-dispatch nil clauses)
          (funcall 'neovm--pm-dispatch '(ok . 42) clauses)
          (funcall 'neovm--pm-dispatch '(ok . -1) clauses)
          (funcall 'neovm--pm-dispatch '(err . "timeout") clauses)
          (funcall 'neovm--pm-dispatch '(other . stuff) clauses)
          (funcall 'neovm--pm-dispatch 999 clauses)))
    (fmakunbound 'neovm--pm-match)
    (fmakunbound 'neovm--pm-dispatch)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"empty\" \"ok:42\" \"unknown\" \"error:timeout\" \"unknown\" \"unknown\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Deeply nested patterns: tree matching and recursive structures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pattern_match_deep_nesting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Match against deeply nested tree structures
    let form = r#"(progn
  (fset 'neovm--pm-match
    (lambda (pattern value bindings)
      (cond
       ((eq pattern 'wildcard) (cons t bindings))
       ((and (consp pattern) (eq (car pattern) 'literal))
        (if (equal (cadr pattern) value) (cons t bindings) (cons nil nil)))
       ((and (consp pattern) (eq (car pattern) 'cpat))
        (if (consp value)
            (let* ((cr (funcall 'neovm--pm-match (nth 1 pattern) (car value) bindings)))
              (if (car cr)
                  (funcall 'neovm--pm-match (nth 2 pattern) (cdr value) (cdr cr))
                (cons nil nil)))
          (cons nil nil)))
       ((and (consp pattern) (eq (car pattern) 'bind))
        (cons t (cons (cons (cadr pattern) value) bindings)))
       ((and (consp pattern) (eq (car pattern) 'guard))
        (let* ((sr (funcall 'neovm--pm-match (nth 1 pattern) value bindings)))
          (if (car sr)
              (if (funcall (nth 2 pattern) value (cdr sr))
                  sr (cons nil nil))
            (cons nil nil))))
       ((and (consp pattern) (eq (car pattern) 'orp))
        (let ((pats (cdr pattern)) (result (cons nil nil)))
          (while (and pats (not (car result)))
            (setq result (funcall 'neovm--pm-match (car pats) value bindings))
            (setq pats (cdr pats)))
          result))
       (t (cons nil nil)))))

  (fset 'neovm--pm-list-pat
    (lambda (pats)
      (if (null pats) '(literal nil)
        (list 'cpat (car pats) (funcall 'neovm--pm-list-pat (cdr pats))))))

  (unwind-protect
      (list
        ;; Match a binary tree node: (node left val right)
        ;; Tree: (node (node nil 1 nil) 2 (node nil 3 nil))
        (let ((leaf-pat (funcall 'neovm--pm-list-pat
                          '((literal node) (literal nil) (bind v) (literal nil))))
              (tree '(node (node nil 1 nil) 2 (node nil 3 nil))))
          ;; Match root: (node left root-val right)
          (let ((root-pat (funcall 'neovm--pm-list-pat
                            (list '(literal node) 'wildcard '(bind root) 'wildcard))))
            (funcall 'neovm--pm-match root-pat tree nil)))
        ;; Match nested: extract left child's value
        (let* ((inner-pat (funcall 'neovm--pm-list-pat
                            '((literal node) (literal nil) (bind lval) (literal nil))))
               (outer-pat (funcall 'neovm--pm-list-pat
                            (list '(literal node) inner-pat '(bind root) 'wildcard)))
               (tree '(node (node nil 1 nil) 2 (node nil 3 nil))))
          (funcall 'neovm--pm-match outer-pat tree nil))
        ;; Match 3-level deep nesting
        (let* ((deep-tree '(a (b (c . 42))))
               (pat '(cpat (literal a)
                      (cpat (cpat (literal b)
                              (cpat (cpat (literal c) (bind val))
                                (literal nil)))
                        (literal nil)))))
          (funcall 'neovm--pm-match pat deep-tree nil))
        ;; Pattern that doesn't match deep structure
        (let* ((deep-tree '(a (b (c . 42))))
               (wrong-pat '(cpat (literal a)
                            (cpat (cpat (literal x)
                                    (cpat (cpat (literal c) (bind val))
                                      (literal nil)))
                              (literal nil)))))
          (funcall 'neovm--pm-match wrong-pat deep-tree nil))
        ;; Match alist-like structure: extract value for key 'b
        ;; Data: ((a . 1) (b . 2) (c . 3))
        ;; Pattern: match (_ (b . bind-val) _)
        (let* ((data '((a . 1) (b . 2) (c . 3)))
               (pat (funcall 'neovm--pm-list-pat
                      (list 'wildcard
                            '(cpat (literal b) (bind val))
                            'wildcard))))
          (funcall 'neovm--pm-match pat data nil)))
    (fmakunbound 'neovm--pm-match)
    (fmakunbound 'neovm--pm-list-pat)))"#;
    let expect = expect_test::expect![[
        r#""OK ((t (root . 2)) (t (root . 2) (lval . 1)) (t (val . 42)) (nil) (t (val . 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Real-world: expression evaluator driven by pattern matching
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pattern_match_expression_evaluator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use pattern matching to build a tiny arithmetic expression evaluator.
    // Expressions: numbers, (+ e1 e2), (- e1 e2), (* e1 e2), (neg e)
    let form = r#"(progn
  (fset 'neovm--pm-match
    (lambda (pattern value bindings)
      (cond
       ((eq pattern 'wildcard) (cons t bindings))
       ((and (consp pattern) (eq (car pattern) 'literal))
        (if (equal (cadr pattern) value) (cons t bindings) (cons nil nil)))
       ((and (consp pattern) (eq (car pattern) 'cpat))
        (if (consp value)
            (let* ((cr (funcall 'neovm--pm-match (nth 1 pattern) (car value) bindings)))
              (if (car cr)
                  (funcall 'neovm--pm-match (nth 2 pattern) (cdr value) (cdr cr))
                (cons nil nil)))
          (cons nil nil)))
       ((and (consp pattern) (eq (car pattern) 'bind))
        (cons t (cons (cons (cadr pattern) value) bindings)))
       ((and (consp pattern) (eq (car pattern) 'guard))
        (let* ((sr (funcall 'neovm--pm-match (nth 1 pattern) value bindings)))
          (if (car sr)
              (if (funcall (nth 2 pattern) value (cdr sr))
                  sr (cons nil nil))
            (cons nil nil))))
       ((and (consp pattern) (eq (car pattern) 'orp))
        (let ((pats (cdr pattern)) (result (cons nil nil)))
          (while (and pats (not (car result)))
            (setq result (funcall 'neovm--pm-match (car pats) value bindings))
            (setq pats (cdr pats)))
          result))
       (t (cons nil nil)))))

  (fset 'neovm--pm-list-pat
    (lambda (pats)
      (if (null pats) '(literal nil)
        (list 'cpat (car pats) (funcall 'neovm--pm-list-pat (cdr pats))))))

  (fset 'neovm--pm-eval-expr
    (lambda (expr)
      "Evaluate arithmetic expression using pattern matching."
      (cond
       ;; Number literal
       ((numberp expr) expr)
       ;; Binary op: (op e1 e2)
       ((consp expr)
        (let* ((binop-pat (funcall 'neovm--pm-list-pat
                            '((bind op) (bind left) (bind right))))
               (unop-pat (funcall 'neovm--pm-list-pat
                           '((bind op) (bind arg))))
               (mr-bin (funcall 'neovm--pm-match binop-pat expr nil)))
          (if (car mr-bin)
              (let* ((b (cdr mr-bin))
                     (op (cdr (assq 'op b)))
                     (l (funcall 'neovm--pm-eval-expr (cdr (assq 'left b))))
                     (r (funcall 'neovm--pm-eval-expr (cdr (assq 'right b)))))
                (cond ((eq op '+) (+ l r))
                      ((eq op '-) (- l r))
                      ((eq op '*) (* l r))
                      (t (error "Unknown binop: %s" op))))
            ;; Try unary op
            (let ((mr-un (funcall 'neovm--pm-match unop-pat expr nil)))
              (if (car mr-un)
                  (let* ((b (cdr mr-un))
                         (op (cdr (assq 'op b)))
                         (a (funcall 'neovm--pm-eval-expr (cdr (assq 'arg b)))))
                    (cond ((eq op 'neg) (- a))
                          (t (error "Unknown unop: %s" op))))
                (error "Cannot evaluate: %S" expr))))))
       (t (error "Bad expression: %S" expr)))))

  (unwind-protect
      (list
        ;; Simple number
        (funcall 'neovm--pm-eval-expr 42)
        ;; Binary: (+ 1 2) = 3
        (funcall 'neovm--pm-eval-expr '(+ 1 2))
        ;; Nested: (* (+ 2 3) (- 10 4)) = 5 * 6 = 30
        (funcall 'neovm--pm-eval-expr '(* (+ 2 3) (- 10 4)))
        ;; Unary: (neg 5) = -5
        (funcall 'neovm--pm-eval-expr '(neg 5))
        ;; Complex: (+ (* 3 4) (neg (- 10 7))) = 12 + (-3) = 9
        (funcall 'neovm--pm-eval-expr '(+ (* 3 4) (neg (- 10 7))))
        ;; Deep nesting: (+ (+ (+ 1 2) 3) (+ 4 (+ 5 6)))
        (funcall 'neovm--pm-eval-expr '(+ (+ (+ 1 2) 3) (+ 4 (+ 5 6))))
        ;; All operators: (- (* (+ 1 2) 3) (neg 1)) = 9 - (-1) = 10
        (funcall 'neovm--pm-eval-expr '(- (* (+ 1 2) 3) (neg 1))))
    (fmakunbound 'neovm--pm-match)
    (fmakunbound 'neovm--pm-list-pat)
    (fmakunbound 'neovm--pm-eval-expr)))"#;
    let expect = expect_test::expect![[r#""OK (42 3 30 -5 9 21 10)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Match failure and error handling
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pattern_match_failure_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test various failure modes and the absence of false positives.
    let form = r#"(progn
  (fset 'neovm--pm-match
    (lambda (pattern value bindings)
      (cond
       ((eq pattern 'wildcard) (cons t bindings))
       ((and (consp pattern) (eq (car pattern) 'literal))
        (if (equal (cadr pattern) value) (cons t bindings) (cons nil nil)))
       ((and (consp pattern) (eq (car pattern) 'cpat))
        (if (consp value)
            (let* ((cr (funcall 'neovm--pm-match (nth 1 pattern) (car value) bindings)))
              (if (car cr)
                  (funcall 'neovm--pm-match (nth 2 pattern) (cdr value) (cdr cr))
                (cons nil nil)))
          (cons nil nil)))
       ((and (consp pattern) (eq (car pattern) 'bind))
        (cons t (cons (cons (cadr pattern) value) bindings)))
       ((and (consp pattern) (eq (car pattern) 'guard))
        (let* ((sr (funcall 'neovm--pm-match (nth 1 pattern) value bindings)))
          (if (car sr)
              (if (funcall (nth 2 pattern) value (cdr sr))
                  sr (cons nil nil))
            (cons nil nil))))
       ((and (consp pattern) (eq (car pattern) 'orp))
        (let ((pats (cdr pattern)) (result (cons nil nil)))
          (while (and pats (not (car result)))
            (setq result (funcall 'neovm--pm-match (car pats) value bindings))
            (setq pats (cdr pats)))
          result))
       (t (cons nil nil)))))

  (unwind-protect
      (let ((failures nil))
        ;; Literal mismatch: different types
        (setq failures (cons (funcall 'neovm--pm-match '(literal 42) "42" nil) failures))
        ;; Literal mismatch: symbol vs string
        (setq failures (cons (funcall 'neovm--pm-match '(literal foo) "foo" nil) failures))
        ;; Cons pattern on atom
        (setq failures (cons (funcall 'neovm--pm-match '(cpat (bind a) (bind b)) 42 nil) failures))
        ;; Cons pattern on string
        (setq failures (cons (funcall 'neovm--pm-match '(cpat (bind a) (bind b)) "hello" nil) failures))
        ;; Cons pattern on nil
        (setq failures (cons (funcall 'neovm--pm-match '(cpat (bind a) (bind b)) nil nil) failures))
        ;; Guard that always fails
        (setq failures (cons (funcall 'neovm--pm-match
                               (list 'guard 'wildcard (lambda (_v _b) nil))
                               "anything" nil)
                             failures))
        ;; Or where nothing matches
        (setq failures (cons (funcall 'neovm--pm-match
                               '(orp (literal 1) (literal 2) (literal 3))
                               4 nil)
                             failures))
        ;; Verify all failures returned (nil . nil)
        (let ((all-nil t))
          (dolist (f failures)
            (when (car f) (setq all-nil nil)))
          (list 'all-failed all-nil
                'count (length failures)
                'results (nreverse failures))))
    (fmakunbound 'neovm--pm-match)))"#;
    let expect = expect_test::expect![[
        r#""OK (all-failed t count 7 results ((nil) (nil) (nil) (nil) (nil) (nil) (nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
