//! Oracle parity tests implementing a pattern matching language in Elisp.
//! Patterns: literal, variable (?x), wildcard (_), cons (pattern . pattern),
//! guard clauses, nested matching, list destructuring with rest patterns,
//! and a full match expression (like ML's match).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Core pattern language: literal, variable, wildcard, cons
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_patlang_core_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Pattern representation:
    //   (lit val)      - matches exact value (via equal)
    //   (var name)     - matches anything, binds to name
    //   _              - wildcard, matches anything, no binding
    //   (pcons p1 p2)  - matches a cons, car against p1, cdr against p2
    //   (pred fn pat)  - guard: match pat, then check (fn value)
    let form = r#"(progn
  (fset 'neovm--pl-match
    (lambda (pat val env)
      "Match VAL against PAT. Returns (t . env) or (nil . nil).
       ENV is an alist of (name . value) bindings."
      (cond
       ;; Wildcard
       ((eq pat '_) (cons t env))
       ;; Literal: (lit value)
       ((and (consp pat) (eq (car pat) 'lit))
        (if (equal (cadr pat) val)
            (cons t env)
          (cons nil nil)))
       ;; Variable: (var name)
       ((and (consp pat) (eq (car pat) 'var))
        (let ((existing (assq (cadr pat) env)))
          (if existing
              ;; Variable already bound: check equality (linear pattern)
              (if (equal (cdr existing) val)
                  (cons t env)
                (cons nil nil))
            ;; Fresh binding
            (cons t (cons (cons (cadr pat) val) env)))))
       ;; Cons: (pcons car-pat cdr-pat)
       ((and (consp pat) (eq (car pat) 'pcons))
        (if (consp val)
            (let ((cr (funcall 'neovm--pl-match (nth 1 pat) (car val) env)))
              (if (car cr)
                  (funcall 'neovm--pl-match (nth 2 pat) (cdr val) (cdr cr))
                (cons nil nil)))
          (cons nil nil)))
       ;; Predicate guard: (pred fn sub-pat)
       ((and (consp pat) (eq (car pat) 'pred))
        (let ((sr (funcall 'neovm--pl-match (nth 2 pat) val env)))
          (if (and (car sr) (funcall (nth 1 pat) val))
              sr
            (cons nil nil))))
       (t (cons nil nil)))))

  (unwind-protect
      (list
        ;; Wildcard matches anything
        (car (funcall 'neovm--pl-match '_ 42 nil))
        (car (funcall 'neovm--pl-match '_ "hello" nil))
        (car (funcall 'neovm--pl-match '_ nil nil))
        ;; Literal matches exact value
        (car (funcall 'neovm--pl-match '(lit 42) 42 nil))
        (car (funcall 'neovm--pl-match '(lit 42) 43 nil))
        (car (funcall 'neovm--pl-match '(lit "hello") "hello" nil))
        (car (funcall 'neovm--pl-match '(lit "hello") "world" nil))
        ;; Variable binds value
        (funcall 'neovm--pl-match '(var x) 42 nil)
        (funcall 'neovm--pl-match '(var name) "Alice" nil)
        ;; Variable linear pattern (repeated var must match same value)
        (funcall 'neovm--pl-match '(var x) 42 '((x . 42)))
        (funcall 'neovm--pl-match '(var x) 99 '((x . 42)))
        ;; Cons pattern
        (funcall 'neovm--pl-match '(pcons (var a) (var b)) '(1 . 2) nil)
        (funcall 'neovm--pl-match '(pcons (lit a) (var rest)) '(a b c) nil)
        ;; Cons fails on atom
        (car (funcall 'neovm--pl-match '(pcons (var a) (var b)) 42 nil))
        (car (funcall 'neovm--pl-match '(pcons (var a) (var b)) nil nil))
        ;; Predicate guard
        (funcall 'neovm--pl-match
          (list 'pred (lambda (v) (> v 0)) '(var x)) 5 nil)
        (funcall 'neovm--pl-match
          (list 'pred (lambda (v) (> v 0)) '(var x)) -5 nil))
    (fmakunbound 'neovm--pl-match)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t nil t nil (t (x . 42)) (t (name . \"Alice\")) (t (x . 42)) (nil) (t (b . 2) (a . 1)) (t (rest b c)) nil nil (t (x . 5)) (nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// List destructuring with rest patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_patlang_list_destructuring_rest() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // (plist p1 p2 ... pN)           - match exact-length list
    // (plist* p1 p2 ... pN rest-pat) - match at least N elements, rest to rest-pat
    let form = r#"(progn
  (fset 'neovm--pl-match
    (lambda (pat val env)
      (cond
       ((eq pat '_) (cons t env))
       ((and (consp pat) (eq (car pat) 'lit))
        (if (equal (cadr pat) val) (cons t env) (cons nil nil)))
       ((and (consp pat) (eq (car pat) 'var))
        (let ((existing (assq (cadr pat) env)))
          (if existing
              (if (equal (cdr existing) val) (cons t env) (cons nil nil))
            (cons t (cons (cons (cadr pat) val) env)))))
       ((and (consp pat) (eq (car pat) 'pcons))
        (if (consp val)
            (let ((cr (funcall 'neovm--pl-match (nth 1 pat) (car val) env)))
              (if (car cr)
                  (funcall 'neovm--pl-match (nth 2 pat) (cdr val) (cdr cr))
                (cons nil nil)))
          (cons nil nil)))
       ((and (consp pat) (eq (car pat) 'pred))
        (let ((sr (funcall 'neovm--pl-match (nth 2 pat) val env)))
          (if (and (car sr) (funcall (nth 1 pat) val)) sr (cons nil nil))))
       ;; Exact-length list pattern: (plist p1 p2 ... pN)
       ((and (consp pat) (eq (car pat) 'plist))
        (funcall 'neovm--pl-match-list (cdr pat) val env nil))
       ;; Rest-list pattern: (plist* p1 p2 ... pN rest-pat)
       ((and (consp pat) (eq (car pat) 'plist*))
        (funcall 'neovm--pl-match-list (cdr pat) val env t))
       (t (cons nil nil)))))

  (fset 'neovm--pl-match-list
    (lambda (pats val env has-rest)
      "Match a list of patterns against a list value.
       If has-rest, last pattern matches the remainder."
      (cond
       ;; No more patterns
       ((null pats)
        (if (null val) (cons t env) (cons nil nil)))
       ;; Last pattern with rest: it matches entire remaining val
       ((and has-rest (null (cdr pats)))
        (funcall 'neovm--pl-match (car pats) val env))
       ;; Normal: match car against first pattern, recurse on rest
       ((consp val)
        (let ((cr (funcall 'neovm--pl-match (car pats) (car val) env)))
          (if (car cr)
              (funcall 'neovm--pl-match-list (cdr pats) (cdr val) (cdr cr) has-rest)
            (cons nil nil))))
       ;; val is not a cons but we have patterns left
       (t (cons nil nil)))))

  (unwind-protect
      (list
        ;; Exact list match: (plist (var a) (var b) (var c))
        (funcall 'neovm--pl-match '(plist (var a) (var b) (var c)) '(1 2 3) nil)
        ;; Too few elements
        (car (funcall 'neovm--pl-match '(plist (var a) (var b) (var c)) '(1 2) nil))
        ;; Too many elements
        (car (funcall 'neovm--pl-match '(plist (var a) (var b)) '(1 2 3) nil))
        ;; Empty list pattern
        (funcall 'neovm--pl-match '(plist) nil nil)
        (car (funcall 'neovm--pl-match '(plist) '(1) nil))
        ;; Rest pattern: (plist* (var head) (var rest))
        (funcall 'neovm--pl-match '(plist* (var head) (var rest)) '(1 2 3) nil)
        ;; Rest pattern with multiple fixed elements
        (funcall 'neovm--pl-match
          '(plist* (var a) (var b) (var rest)) '(1 2 3 4 5) nil)
        ;; Rest matches empty list when exact
        (funcall 'neovm--pl-match '(plist* (var a) (var rest)) '(1) nil)
        ;; Rest with literals
        (funcall 'neovm--pl-match
          '(plist* (lit ok) (var val) (var rest)) '(ok 42 extra1 extra2) nil)
        ;; Rest fails on non-list
        (car (funcall 'neovm--pl-match '(plist* (var a) (var rest)) 42 nil))
        ;; Nested list patterns
        (funcall 'neovm--pl-match
          '(plist (plist (var a) (var b)) (plist (var c) (var d)))
          '((1 2) (3 4)) nil))
    (fmakunbound 'neovm--pl-match)
    (fmakunbound 'neovm--pl-match-list)))"#;
    let expect = expect_test::expect![[
        r#""OK ((t (c . 3) (b . 2) (a . 1)) nil nil (t) nil (t (rest 2 3) (head . 1)) (t (rest 3 4 5) (b . 2) (a . 1)) (t (rest) (a . 1)) (t (rest extra1 extra2) (val . 42)) nil (t (d . 4) (c . 3) (b . 2) (a . 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested pattern matching: patterns within patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_patlang_nested_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--pl-match
    (lambda (pat val env)
      (cond
       ((eq pat '_) (cons t env))
       ((and (consp pat) (eq (car pat) 'lit))
        (if (equal (cadr pat) val) (cons t env) (cons nil nil)))
       ((and (consp pat) (eq (car pat) 'var))
        (let ((existing (assq (cadr pat) env)))
          (if existing
              (if (equal (cdr existing) val) (cons t env) (cons nil nil))
            (cons t (cons (cons (cadr pat) val) env)))))
       ((and (consp pat) (eq (car pat) 'pcons))
        (if (consp val)
            (let ((cr (funcall 'neovm--pl-match (nth 1 pat) (car val) env)))
              (if (car cr)
                  (funcall 'neovm--pl-match (nth 2 pat) (cdr val) (cdr cr))
                (cons nil nil)))
          (cons nil nil)))
       ((and (consp pat) (eq (car pat) 'pred))
        (let ((sr (funcall 'neovm--pl-match (nth 2 pat) val env)))
          (if (and (car sr) (funcall (nth 1 pat) val)) sr (cons nil nil))))
       ((and (consp pat) (eq (car pat) 'plist))
        (funcall 'neovm--pl-match-list (cdr pat) val env nil))
       ((and (consp pat) (eq (car pat) 'plist*))
        (funcall 'neovm--pl-match-list (cdr pat) val env t))
       ;; Or pattern: (por p1 p2 ...)
       ((and (consp pat) (eq (car pat) 'por))
        (let ((alts (cdr pat)) (result (cons nil nil)))
          (while (and alts (not (car result)))
            (setq result (funcall 'neovm--pl-match (car alts) val env))
            (setq alts (cdr alts)))
          result))
       (t (cons nil nil)))))

  (fset 'neovm--pl-match-list
    (lambda (pats val env has-rest)
      (cond
       ((null pats) (if (null val) (cons t env) (cons nil nil)))
       ((and has-rest (null (cdr pats)))
        (funcall 'neovm--pl-match (car pats) val env))
       ((consp val)
        (let ((cr (funcall 'neovm--pl-match (car pats) (car val) env)))
          (if (car cr)
              (funcall 'neovm--pl-match-list (cdr pats) (cdr val) (cdr cr) has-rest)
            (cons nil nil))))
       (t (cons nil nil)))))

  (unwind-protect
      (list
        ;; Match AST node: (if cond then else)
        (funcall 'neovm--pl-match
          '(plist (lit if) (var cond) (var then) (var else))
          '(if (> x 0) "pos" "neg") nil)
        ;; Match nested binary tree: (node (node nil 1 nil) 2 (node nil 3 nil))
        (let ((leaf-pat '(plist (lit node) (lit nil) (var v) (lit nil)))
              (tree '(node (node nil 1 nil) 2 (node nil 3 nil))))
          (funcall 'neovm--pl-match
            (list 'plist '(lit node) leaf-pat '(var root) leaf-pat)
            tree nil))
        ;; Or-pattern: match either (ok val) or (err msg)
        (funcall 'neovm--pl-match
          '(por (plist (lit ok) (var result))
                (plist (lit err) (var result)))
          '(ok 42) nil)
        (funcall 'neovm--pl-match
          '(por (plist (lit ok) (var result))
                (plist (lit err) (var result)))
          '(err "timeout") nil)
        (car (funcall 'neovm--pl-match
          '(por (plist (lit ok) (var result))
                (plist (lit err) (var result)))
          '(unknown 42) nil))
        ;; Deeply nested: match ((a 1) (b 2)) and extract both values
        (funcall 'neovm--pl-match
          '(plist (plist (lit a) (var x)) (plist (lit b) (var y)))
          '((a 1) (b 2)) nil)
        ;; Predicate inside nested pattern
        (funcall 'neovm--pl-match
          (list 'plist
                (list 'pred #'symbolp '(var key))
                (list 'pred #'numberp '(var val)))
          '(age 30) nil)
        ;; Predicate that fails
        (car (funcall 'neovm--pl-match
          (list 'plist
                (list 'pred #'symbolp '(var key))
                (list 'pred #'numberp '(var val)))
          '(age "thirty") nil)))
    (fmakunbound 'neovm--pl-match)
    (fmakunbound 'neovm--pl-match-list)))"#;
    let expect = expect_test::expect![[
        r#""OK ((t (else . \"neg\") (then . \"pos\") (cond > x 0)) (nil) (t (result . 42)) (t (result . \"timeout\")) nil (t (y . 2) (x . 1)) (t (val . 30) (key . age)) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Guard clauses with bound variables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_patlang_guard_clauses() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--pl-match
    (lambda (pat val env)
      (cond
       ((eq pat '_) (cons t env))
       ((and (consp pat) (eq (car pat) 'lit))
        (if (equal (cadr pat) val) (cons t env) (cons nil nil)))
       ((and (consp pat) (eq (car pat) 'var))
        (let ((existing (assq (cadr pat) env)))
          (if existing
              (if (equal (cdr existing) val) (cons t env) (cons nil nil))
            (cons t (cons (cons (cadr pat) val) env)))))
       ((and (consp pat) (eq (car pat) 'pcons))
        (if (consp val)
            (let ((cr (funcall 'neovm--pl-match (nth 1 pat) (car val) env)))
              (if (car cr)
                  (funcall 'neovm--pl-match (nth 2 pat) (cdr val) (cdr cr))
                (cons nil nil)))
          (cons nil nil)))
       ;; Guard with access to bindings: (guard sub-pat check-fn)
       ;; check-fn receives (value bindings) and returns t/nil
       ((and (consp pat) (eq (car pat) 'guard))
        (let ((sr (funcall 'neovm--pl-match (nth 1 pat) val env)))
          (if (car sr)
              (if (funcall (nth 2 pat) val (cdr sr))
                  sr
                (cons nil nil))
            (cons nil nil))))
       ((and (consp pat) (eq (car pat) 'pred))
        (let ((sr (funcall 'neovm--pl-match (nth 2 pat) val env)))
          (if (and (car sr) (funcall (nth 1 pat) val)) sr (cons nil nil))))
       ((and (consp pat) (eq (car pat) 'plist))
        (funcall 'neovm--pl-match-list (cdr pat) val env nil))
       ((and (consp pat) (eq (car pat) 'plist*))
        (funcall 'neovm--pl-match-list (cdr pat) val env t))
       ((and (consp pat) (eq (car pat) 'por))
        (let ((alts (cdr pat)) (result (cons nil nil)))
          (while (and alts (not (car result)))
            (setq result (funcall 'neovm--pl-match (car alts) val env))
            (setq alts (cdr alts)))
          result))
       (t (cons nil nil)))))

  (fset 'neovm--pl-match-list
    (lambda (pats val env has-rest)
      (cond
       ((null pats) (if (null val) (cons t env) (cons nil nil)))
       ((and has-rest (null (cdr pats)))
        (funcall 'neovm--pl-match (car pats) val env))
       ((consp val)
        (let ((cr (funcall 'neovm--pl-match (car pats) (car val) env)))
          (if (car cr)
              (funcall 'neovm--pl-match-list (cdr pats) (cdr val) (cdr cr) has-rest)
            (cons nil nil))))
       (t (cons nil nil)))))

  (unwind-protect
      (list
        ;; Guard: match pair (a . b) where a < b
        (funcall 'neovm--pl-match
          (list 'guard '(pcons (var a) (var b))
                (lambda (_v binds)
                  (< (cdr (assq 'a binds)) (cdr (assq 'b binds)))))
          '(3 . 7) nil)
        ;; Guard fails: a >= b
        (car (funcall 'neovm--pl-match
          (list 'guard '(pcons (var a) (var b))
                (lambda (_v binds)
                  (< (cdr (assq 'a binds)) (cdr (assq 'b binds)))))
          '(7 . 3) nil))
        ;; Guard: match list where sum > 10
        (funcall 'neovm--pl-match
          (list 'guard '(plist (var a) (var b) (var c))
                (lambda (_v binds)
                  (> (+ (cdr (assq 'a binds))
                        (cdr (assq 'b binds))
                        (cdr (assq 'c binds)))
                     10)))
          '(3 4 5) nil)
        ;; Guard: sum not > 10
        (car (funcall 'neovm--pl-match
          (list 'guard '(plist (var a) (var b) (var c))
                (lambda (_v binds)
                  (> (+ (cdr (assq 'a binds))
                        (cdr (assq 'b binds))
                        (cdr (assq 'c binds)))
                     10)))
          '(1 2 3) nil))
        ;; Guard on string: match string of length > 3
        (funcall 'neovm--pl-match
          (list 'guard '(var s)
                (lambda (_v binds)
                  (> (length (cdr (assq 's binds))) 3)))
          "hello" nil)
        ;; Guard chain: even number > 10
        (funcall 'neovm--pl-match
          (list 'guard
                (list 'pred (lambda (v) (= 0 (% v 2))) '(var n))
                (lambda (_v binds) (> (cdr (assq 'n binds)) 10)))
          42 nil)
        (car (funcall 'neovm--pl-match
          (list 'guard
                (list 'pred (lambda (v) (= 0 (% v 2))) '(var n))
                (lambda (_v binds) (> (cdr (assq 'n binds)) 10)))
          8 nil))
        (car (funcall 'neovm--pl-match
          (list 'guard
                (list 'pred (lambda (v) (= 0 (% v 2))) '(var n))
                (lambda (_v binds) (> (cdr (assq 'n binds)) 10)))
          43 nil)))
    (fmakunbound 'neovm--pl-match)
    (fmakunbound 'neovm--pl-match-list)))"#;
    let expect = expect_test::expect![[
        r#""OK ((t (b . 7) (a . 3)) nil (t (c . 5) (b . 4) (a . 3)) nil (t (s . \"hello\")) (t (n . 42)) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: implementing match expression (like ML's match)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_patlang_match_expression() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // match-expr: (value . clauses) where each clause is (pattern . body-fn)
    // body-fn receives bindings alist, returns result
    let form = r#"(progn
  (fset 'neovm--pl-match
    (lambda (pat val env)
      (cond
       ((eq pat '_) (cons t env))
       ((and (consp pat) (eq (car pat) 'lit))
        (if (equal (cadr pat) val) (cons t env) (cons nil nil)))
       ((and (consp pat) (eq (car pat) 'var))
        (let ((existing (assq (cadr pat) env)))
          (if existing
              (if (equal (cdr existing) val) (cons t env) (cons nil nil))
            (cons t (cons (cons (cadr pat) val) env)))))
       ((and (consp pat) (eq (car pat) 'pcons))
        (if (consp val)
            (let ((cr (funcall 'neovm--pl-match (nth 1 pat) (car val) env)))
              (if (car cr)
                  (funcall 'neovm--pl-match (nth 2 pat) (cdr val) (cdr cr))
                (cons nil nil)))
          (cons nil nil)))
       ((and (consp pat) (eq (car pat) 'pred))
        (let ((sr (funcall 'neovm--pl-match (nth 2 pat) val env)))
          (if (and (car sr) (funcall (nth 1 pat) val)) sr (cons nil nil))))
       ((and (consp pat) (eq (car pat) 'plist))
        (funcall 'neovm--pl-match-list (cdr pat) val env nil))
       ((and (consp pat) (eq (car pat) 'plist*))
        (funcall 'neovm--pl-match-list (cdr pat) val env t))
       ((and (consp pat) (eq (car pat) 'por))
        (let ((alts (cdr pat)) (result (cons nil nil)))
          (while (and alts (not (car result)))
            (setq result (funcall 'neovm--pl-match (car alts) val env))
            (setq alts (cdr alts)))
          result))
       ((and (consp pat) (eq (car pat) 'guard))
        (let ((sr (funcall 'neovm--pl-match (nth 1 pat) val env)))
          (if (car sr)
              (if (funcall (nth 2 pat) val (cdr sr)) sr (cons nil nil))
            (cons nil nil))))
       (t (cons nil nil)))))

  (fset 'neovm--pl-match-list
    (lambda (pats val env has-rest)
      (cond
       ((null pats) (if (null val) (cons t env) (cons nil nil)))
       ((and has-rest (null (cdr pats)))
        (funcall 'neovm--pl-match (car pats) val env))
       ((consp val)
        (let ((cr (funcall 'neovm--pl-match (car pats) (car val) env)))
          (if (car cr)
              (funcall 'neovm--pl-match-list (cdr pats) (cdr val) (cdr cr) has-rest)
            (cons nil nil))))
       (t (cons nil nil)))))

  ;; match-expr: try clauses in order, return body of first match
  (fset 'neovm--pl-match-expr
    (lambda (value clauses)
      "Try each (pattern . body-fn) clause. Return result of first match body."
      (let ((cls clauses) (result nil) (found nil))
        (while (and cls (not found))
          (let* ((clause (car cls))
                 (pat (car clause))
                 (body (cdr clause))
                 (mr (funcall 'neovm--pl-match pat value nil)))
            (when (car mr)
              (setq found t)
              (setq result (funcall body (cdr mr)))))
          (setq cls (cdr cls)))
        (if found result 'no-match))))

  (unwind-protect
      (list
        ;; Simple match: classify a number
        (funcall 'neovm--pl-match-expr 0
          (list
            (cons '(lit 0)
                  (lambda (_b) 'zero))
            (cons (list 'pred (lambda (v) (> v 0)) '(var n))
                  (lambda (b) (list 'positive (cdr (assq 'n b)))))
            (cons '(var n)
                  (lambda (b) (list 'negative (cdr (assq 'n b)))))))
        (funcall 'neovm--pl-match-expr 42
          (list
            (cons '(lit 0)
                  (lambda (_b) 'zero))
            (cons (list 'pred (lambda (v) (> v 0)) '(var n))
                  (lambda (b) (list 'positive (cdr (assq 'n b)))))
            (cons '(var n)
                  (lambda (b) (list 'negative (cdr (assq 'n b)))))))
        (funcall 'neovm--pl-match-expr -7
          (list
            (cons '(lit 0)
                  (lambda (_b) 'zero))
            (cons (list 'pred (lambda (v) (> v 0)) '(var n))
                  (lambda (b) (list 'positive (cdr (assq 'n b)))))
            (cons '(var n)
                  (lambda (b) (list 'negative (cdr (assq 'n b)))))))

        ;; Match on a tagged union: (ok val) | (err msg) | nil
        (let ((clauses
                (list
                  (cons '(plist (lit ok) (var v))
                        (lambda (b) (format "Success: %s" (cdr (assq 'v b)))))
                  (cons '(plist (lit err) (var msg))
                        (lambda (b) (format "Error: %s" (cdr (assq 'msg b)))))
                  (cons '(lit nil)
                        (lambda (_b) "Nothing"))
                  (cons '_
                        (lambda (_b) "Unknown")))))
          (list
            (funcall 'neovm--pl-match-expr '(ok 42) clauses)
            (funcall 'neovm--pl-match-expr '(err "timeout") clauses)
            (funcall 'neovm--pl-match-expr nil clauses)
            (funcall 'neovm--pl-match-expr '(wat) clauses)))

        ;; Match expression for evaluating simple arithmetic
        (let ((eval-clauses nil))
          (fset 'neovm--pl-eval-arith
            (lambda (expr)
              (funcall 'neovm--pl-match-expr expr
                (list
                  ;; Number literal
                  (cons (list 'pred #'numberp '(var n))
                        (lambda (b) (cdr (assq 'n b))))
                  ;; (+ a b)
                  (cons '(plist (lit +) (var a) (var b))
                        (lambda (b)
                          (+ (funcall 'neovm--pl-eval-arith (cdr (assq 'a b)))
                             (funcall 'neovm--pl-eval-arith (cdr (assq 'b b))))))
                  ;; (* a b)
                  (cons '(plist (lit *) (var a) (var b))
                        (lambda (b)
                          (* (funcall 'neovm--pl-eval-arith (cdr (assq 'a b)))
                             (funcall 'neovm--pl-eval-arith (cdr (assq 'b b))))))
                  ;; (- a b)
                  (cons '(plist (lit -) (var a) (var b))
                        (lambda (b)
                          (- (funcall 'neovm--pl-eval-arith (cdr (assq 'a b)))
                             (funcall 'neovm--pl-eval-arith (cdr (assq 'b b))))))))))
          (list
            (funcall 'neovm--pl-eval-arith 5)
            (funcall 'neovm--pl-eval-arith '(+ 3 4))
            (funcall 'neovm--pl-eval-arith '(* (+ 2 3) (- 10 4)))
            (funcall 'neovm--pl-eval-arith '(+ (* 3 4) (- 10 7))))))
    (fmakunbound 'neovm--pl-match)
    (fmakunbound 'neovm--pl-match-list)
    (fmakunbound 'neovm--pl-match-expr)
    (fmakunbound 'neovm--pl-eval-arith)))"#;
    let expect = expect_test::expect![[
        r#""OK (zero (positive 42) (negative -7) (\"Success: 42\" \"Error: timeout\" \"Nothing\" \"Unknown\") (5 7 30 15))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: pattern-driven data transformer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_patlang_data_transformer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Transform a list of records using pattern-based rules
    let form = r#"(progn
  (fset 'neovm--pl-match
    (lambda (pat val env)
      (cond
       ((eq pat '_) (cons t env))
       ((and (consp pat) (eq (car pat) 'lit))
        (if (equal (cadr pat) val) (cons t env) (cons nil nil)))
       ((and (consp pat) (eq (car pat) 'var))
        (let ((existing (assq (cadr pat) env)))
          (if existing
              (if (equal (cdr existing) val) (cons t env) (cons nil nil))
            (cons t (cons (cons (cadr pat) val) env)))))
       ((and (consp pat) (eq (car pat) 'pcons))
        (if (consp val)
            (let ((cr (funcall 'neovm--pl-match (nth 1 pat) (car val) env)))
              (if (car cr)
                  (funcall 'neovm--pl-match (nth 2 pat) (cdr val) (cdr cr))
                (cons nil nil)))
          (cons nil nil)))
       ((and (consp pat) (eq (car pat) 'pred))
        (let ((sr (funcall 'neovm--pl-match (nth 2 pat) val env)))
          (if (and (car sr) (funcall (nth 1 pat) val)) sr (cons nil nil))))
       ((and (consp pat) (eq (car pat) 'plist))
        (funcall 'neovm--pl-match-list (cdr pat) val env nil))
       ((and (consp pat) (eq (car pat) 'plist*))
        (funcall 'neovm--pl-match-list (cdr pat) val env t))
       ((and (consp pat) (eq (car pat) 'por))
        (let ((alts (cdr pat)) (result (cons nil nil)))
          (while (and alts (not (car result)))
            (setq result (funcall 'neovm--pl-match (car alts) val env))
            (setq alts (cdr alts)))
          result))
       ((and (consp pat) (eq (car pat) 'guard))
        (let ((sr (funcall 'neovm--pl-match (nth 1 pat) val env)))
          (if (car sr)
              (if (funcall (nth 2 pat) val (cdr sr)) sr (cons nil nil))
            (cons nil nil))))
       (t (cons nil nil)))))

  (fset 'neovm--pl-match-list
    (lambda (pats val env has-rest)
      (cond
       ((null pats) (if (null val) (cons t env) (cons nil nil)))
       ((and has-rest (null (cdr pats)))
        (funcall 'neovm--pl-match (car pats) val env))
       ((consp val)
        (let ((cr (funcall 'neovm--pl-match (car pats) (car val) env)))
          (if (car cr)
              (funcall 'neovm--pl-match-list (cdr pats) (cdr val) (cdr cr) has-rest)
            (cons nil nil))))
       (t (cons nil nil)))))

  ;; Transform: apply first matching rule to each record
  (fset 'neovm--pl-transform
    (lambda (records rules)
      (mapcar
        (lambda (rec)
          (let ((cls rules) (result rec) (found nil))
            (while (and cls (not found))
              (let* ((rule (car cls))
                     (pat (car rule))
                     (fn (cdr rule))
                     (mr (funcall 'neovm--pl-match pat rec nil)))
                (when (car mr)
                  (setq found t)
                  (setq result (funcall fn (cdr mr)))))
              (setq cls (cdr cls)))
            result))
        records)))

  (unwind-protect
      (let* ((records '((person "Alice" 30)
                        (person "Bob" 17)
                        (company "Acme" 50)
                        (person "Carol" 25)
                        (unknown)))
             (rules
               (list
                ;; Adult person (age >= 18): add :adult tag
                (cons (list 'guard
                            '(plist (lit person) (var name) (var age))
                            (lambda (_v binds) (>= (cdr (assq 'age binds)) 18)))
                      (lambda (b) (list 'adult
                                        (cdr (assq 'name b))
                                        (cdr (assq 'age b)))))
                ;; Minor person: add :minor tag
                (cons '(plist (lit person) (var name) (var age))
                      (lambda (b) (list 'minor
                                        (cdr (assq 'name b))
                                        (cdr (assq 'age b)))))
                ;; Company: format as string
                (cons '(plist (lit company) (var name) (var size))
                      (lambda (b) (format "%s (employees: %d)"
                                          (cdr (assq 'name b))
                                          (cdr (assq 'size b)))))
                ;; Fallback: tag as unrecognized
                (cons '_ (lambda (_b) '(unrecognized))))))
        (list
          (funcall 'neovm--pl-transform records rules)
          ;; Count by category
          (let ((results (funcall 'neovm--pl-transform records rules))
                (adults 0) (minors 0) (companies 0) (other 0))
            (dolist (r results)
              (cond ((and (consp r) (eq (car r) 'adult)) (setq adults (1+ adults)))
                    ((and (consp r) (eq (car r) 'minor)) (setq minors (1+ minors)))
                    ((stringp r) (setq companies (1+ companies)))
                    (t (setq other (1+ other)))))
            (list adults minors companies other))))
    (fmakunbound 'neovm--pl-match)
    (fmakunbound 'neovm--pl-match-list)
    (fmakunbound 'neovm--pl-transform)))"#;
    let expect = expect_test::expect![[
        r#""OK (((adult \"Alice\" 30) (minor \"Bob\" 17) \"Acme (employees: 50)\" (adult \"Carol\" 25) (unrecognized)) (2 1 1 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
