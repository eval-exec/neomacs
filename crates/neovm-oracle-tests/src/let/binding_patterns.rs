//! Advanced oracle parity tests for let/let* binding patterns:
//! nested let/let* combinations, let over lambda (closures), let with
//! dynamic binding interaction, complex multi-variable bindings,
//! let as loop variable scope, mutual recursion with letrec, and
//! destructuring-bind patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Nested let/let* combinations: evaluation order and scoping
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_bind_nested_let_letstar() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; let inside let*: inner let sees let* sequential bindings
  (let* ((a 1)
         (b (+ a 10))
         (c (+ b 100)))
    (let ((x (+ a b c))
          (y (* a b c))
          (z (- c a)))
      (list a b c x y z)))

  ;; let* inside let: inner let* sees let parallel bindings
  (let ((a 5) (b 10) (c 15))
    (let* ((sum (+ a b c))
           (avg (/ sum 3))
           (deviation (mapcar (lambda (x) (- x avg)) (list a b c))))
      (list sum avg deviation)))

  ;; Alternating let/let* with shadowing
  (let ((x 1))
    (let* ((x (+ x 10))     ; x = 11
           (y (* x 2)))     ; y = 22
      (let ((x (+ x y))     ; x = 33, y still 22
            (y (- x 5)))    ; y = 11-5 = 6 (uses outer x=11!)
        (let* ((z (+ x y))  ; z = 33 + 6 = 39 (uses inner x and y)
               (w (* z 2))) ; w = 78
          (list x y z w)))))

  ;; Deep nesting: each level transforms previous
  (let ((v '(1 2 3 4 5)))
    (let ((v (mapcar #'1+ v)))              ; (2 3 4 5 6)
      (let* ((v (mapcar (lambda (x) (* x x)) v))  ; (4 9 16 25 36)
             (s (apply #'+ v)))              ; 90
        (let ((v (mapcar (lambda (x) (/ (* x 100) s)) v)))  ; percentages
          (list v s))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 11 111 123 1221 110) (30 10 (-5 0 5)) (33 6 39 78) ((4 10 17 27 40) 90))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Let over lambda: closures capturing mutable state
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_bind_let_over_lambda() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Stack implemented via let-over-lambda
  (fset 'neovm--lb-make-stack
    (lambda ()
      (let ((data nil)
            (size 0)
            (max-size 0))
        (list
         (cons 'push (lambda (x)
                       (setq data (cons x data))
                       (setq size (1+ size))
                       (when (> size max-size)
                         (setq max-size size))
                       x))
         (cons 'pop (lambda ()
                      (if data
                          (let ((top (car data)))
                            (setq data (cdr data))
                            (setq size (1- size))
                            top)
                        'empty)))
         (cons 'peek (lambda ()
                       (if data (car data) 'empty)))
         (cons 'size (lambda () size))
         (cons 'max-size (lambda () max-size))
         (cons 'to-list (lambda () (copy-sequence data)))))))

  ;; Helper to call stack method
  (fset 'neovm--lb-send
    (lambda (obj method &rest args)
      (apply (cdr (assq method obj)) args)))

  (unwind-protect
      (let ((s (funcall 'neovm--lb-make-stack)))
        ;; Push several items
        (funcall 'neovm--lb-send s 'push 10)
        (funcall 'neovm--lb-send s 'push 20)
        (funcall 'neovm--lb-send s 'push 30)
        (let ((after-3-pushes (funcall 'neovm--lb-send s 'size))
              (top (funcall 'neovm--lb-send s 'peek)))
          ;; Pop two
          (let ((p1 (funcall 'neovm--lb-send s 'pop))
                (p2 (funcall 'neovm--lb-send s 'pop)))
            ;; Push more
            (funcall 'neovm--lb-send s 'push 40)
            (funcall 'neovm--lb-send s 'push 50)
            (funcall 'neovm--lb-send s 'push 60)
            (let ((final-size (funcall 'neovm--lb-send s 'size))
                  (max-sz (funcall 'neovm--lb-send s 'max-size))
                  (contents (funcall 'neovm--lb-send s 'to-list)))
              ;; Pop everything
              (let ((all nil))
                (dotimes (_ final-size)
                  (setq all (cons (funcall 'neovm--lb-send s 'pop) all)))
                ;; Pop from empty
                (let ((empty-pop (funcall 'neovm--lb-send s 'pop)))
                  (list after-3-pushes top
                        p1 p2
                        final-size max-sz
                        contents (nreverse all) empty-pop)))))))
    (fmakunbound 'neovm--lb-make-stack)
    (fmakunbound 'neovm--lb-send)))"#;
    let expect =
        expect_test::expect![[r#""OK (3 30 30 20 4 4 (60 50 40 10) (60 50 40 10) empty)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Let with dynamic binding interaction (defvar + let)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_bind_dynamic_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; defvar makes a variable dynamically scoped
  (defvar neovm--lb-dyn-x 100)
  (defvar neovm--lb-dyn-y 200)

  ;; Function that reads dynamic variable
  (fset 'neovm--lb-read-dyn
    (lambda ()
      (list neovm--lb-dyn-x neovm--lb-dyn-y)))

  ;; Function that modifies dynamic variable
  (fset 'neovm--lb-with-context
    (lambda (new-x body-fn)
      (let ((neovm--lb-dyn-x new-x))
        (funcall body-fn))))

  (unwind-protect
      (list
        ;; Baseline
        (funcall 'neovm--lb-read-dyn)

        ;; let on dynamic var: visible to callees
        (let ((neovm--lb-dyn-x 999))
          (funcall 'neovm--lb-read-dyn))

        ;; After let exits: restored
        (funcall 'neovm--lb-read-dyn)

        ;; Nested dynamic binding
        (let ((neovm--lb-dyn-x 10))
          (let ((neovm--lb-dyn-y 20))
            (let ((neovm--lb-dyn-x 30))
              (funcall 'neovm--lb-read-dyn))))

        ;; Dynamic binding through funcall chain
        (funcall 'neovm--lb-with-context 42
                 (lambda ()
                   (let ((outer (funcall 'neovm--lb-read-dyn)))
                     ;; Nest another dynamic binding
                     (funcall 'neovm--lb-with-context 84
                              (lambda ()
                                (list outer (funcall 'neovm--lb-read-dyn)))))))

        ;; let* with dynamic vars: sequential binding visible
        (let* ((neovm--lb-dyn-x 500)
               (neovm--lb-dyn-y (+ neovm--lb-dyn-x 50)))
          (funcall 'neovm--lb-read-dyn))

        ;; Verify restoration after everything
        (funcall 'neovm--lb-read-dyn))
    (fmakunbound 'neovm--lb-read-dyn)
    (fmakunbound 'neovm--lb-with-context)
    (makunbound 'neovm--lb-dyn-x)
    (makunbound 'neovm--lb-dyn-y)))"#;
    let expect = expect_test::expect![[
        r#""OK ((100 200) (999 200) (100 200) (30 20) ((42 200) (84 200)) (500 550) (100 200))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex multi-variable bindings: destructuring via pcase-let
// and manual destructuring
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_bind_destructuring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Manual destructuring helper: unpack list into variables
  (fset 'neovm--lb-destructure
    (lambda (pattern value)
      "Return alist of (name . value) from matching PATTERN against VALUE."
      (cond
        ((null pattern) nil)
        ((symbolp pattern) (list (cons pattern value)))
        ((and (consp pattern) (consp value))
         (append (funcall 'neovm--lb-destructure (car pattern) (car value))
                 (funcall 'neovm--lb-destructure (cdr pattern) (cdr value))))
        (t nil))))

  (unwind-protect
      (list
        ;; Simple pair destructuring
        (let ((pair '(hello . world)))
          (let ((a (car pair))
                (b (cdr pair)))
            (list a b)))

        ;; Triple via nested car/cdr
        (let ((triple '(1 2 3)))
          (let ((first (nth 0 triple))
                (second (nth 1 triple))
                (third (nth 2 triple)))
            (list (* first 100) (* second 10) third)))

        ;; Manual destructuring of nested structures
        (let ((record '((name . "Alice") (age . 30) (scores 95 87 92))))
          (let ((name (cdr (assq 'name record)))
                (age (cdr (assq 'age record)))
                (scores (cdr (assq 'scores record))))
            (list name age (apply #'+ scores)
                  (/ (apply #'+ scores) (length scores)))))

        ;; Destructuring with rest: (a b . rest)
        (let ((data '(1 2 3 4 5 6 7)))
          (let ((first (car data))
                (second (cadr data))
                (rest (cddr data)))
            (list first second rest (apply #'+ rest))))

        ;; Using our destructure helper
        (let ((bindings (funcall 'neovm--lb-destructure
                                 '(a (b c) d)
                                 '(1 (2 3) 4))))
          (mapcar (lambda (b) (list (car b) (cdr b))) bindings))

        ;; Multiple return values via list destructuring
        (let ((result (let ((data '(10 20 30 40 50)))
                        (list (apply #'min data)
                              (apply #'max data)
                              (/ (apply #'+ data) (length data))))))
          (let ((mn (nth 0 result))
                (mx (nth 1 result))
                (avg (nth 2 result)))
            (list mn mx avg (- mx mn))))

        ;; Nested let bindings that build on each other
        (let ((matrix '((1 2 3) (4 5 6) (7 8 9))))
          (let ((row0 (nth 0 matrix))
                (row1 (nth 1 matrix))
                (row2 (nth 2 matrix)))
            (let ((diagonal (list (nth 0 row0) (nth 1 row1) (nth 2 row2)))
                  (anti-diag (list (nth 2 row0) (nth 1 row1) (nth 0 row2))))
              (list diagonal anti-diag
                    (apply #'+ diagonal)
                    (apply #'+ anti-diag))))))
    (fmakunbound 'neovm--lb-destructure)))"#;
    let expect = expect_test::expect![[
        r#""OK ((hello world) (100 20 3) (\"Alice\" 30 274 91) (1 2 (3 4 5 6 7) 25) ((a 1) (b 2) (c 3) (d 4)) (10 50 30 40) ((1 5 9) (3 5 7) 15 15))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Let as loop variable scope: iteration patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_bind_loop_scope() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Capture loop variables correctly: each closure captures its own value
  (let ((closures nil))
    (let ((i 0))
      (while (< i 5)
        (let ((captured i))
          (setq closures (cons (lambda () captured) closures)))
        (setq i (1+ i))))
    (mapcar #'funcall (nreverse closures)))

  ;; Without let capture: all closures share same i (dynamic pitfall)
  ;; But with lexical binding (default in these tests), it's per-iteration
  (let ((fns nil))
    (dolist (x '(a b c d e))
      (let ((val x))
        (setq fns (cons (lambda () val) fns))))
    (mapcar #'funcall (nreverse fns)))

  ;; Accumulator pattern: let initializes, loop body mutates
  (let ((sum 0) (count 0) (min-val most-positive-fixnum) (max-val most-negative-fixnum))
    (dolist (x '(42 17 93 8 56 71 33 25))
      (setq sum (+ sum x))
      (setq count (1+ count))
      (when (< x min-val) (setq min-val x))
      (when (> x max-val) (setq max-val x)))
    (list sum count (/ sum count) min-val max-val))

  ;; Nested loops with independent let scopes
  (let ((result nil))
    (let ((i 1))
      (while (<= i 4)
        (let ((j 1) (row nil))
          (while (<= j 4)
            (setq row (cons (* i j) row))
            (setq j (1+ j)))
          (setq result (cons (nreverse row) result)))
        (setq i (1+ i))))
    (nreverse result))

  ;; Let in recursive function: each frame has its own bindings
  (letrec ((flatten
            (lambda (tree)
              (if (consp tree)
                  (let ((left (funcall flatten (car tree)))
                        (right (funcall flatten (cdr tree))))
                    (append left right))
                (if tree (list tree) nil)))))
    (funcall flatten '((1 (2 3)) (4 nil 5) ((6 7) 8)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 1 2 3 4) (a b c d e) (345 8 43 8 93) ((1 2 3 4) (2 4 6 8) (3 6 9 12) (4 8 12 16)) (1 2 3 4 5 6 7 8))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Mutual recursion with letrec
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_bind_letrec_mutual() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; even?/odd? mutual recursion
  (letrec ((my-even (lambda (n)
                      (if (= n 0) t
                        (funcall my-odd (1- n)))))
           (my-odd (lambda (n)
                     (if (= n 0) nil
                       (funcall my-even (1- n))))))
    (list (mapcar (lambda (n) (funcall my-even n)) '(0 1 2 3 4 5 6 7 8 9 10))
          (mapcar (lambda (n) (funcall my-odd n)) '(0 1 2 3 4 5 6 7 8 9 10))))

  ;; Mutual recursion: Fibonacci-like with two interlocked sequences
  ;; f(n) = g(n-1) + g(n-2), g(n) = f(n-1) + 1
  (letrec ((f (lambda (n)
                (cond ((= n 0) 1)
                      ((= n 1) 1)
                      (t (+ (funcall g (- n 1))
                            (funcall g (- n 2)))))))
           (g (lambda (n)
                (cond ((= n 0) 1)
                      (t (1+ (funcall f (- n 1))))))))
    (list (mapcar f '(0 1 2 3 4 5 6 7 8))
          (mapcar g '(0 1 2 3 4 5 6 7 8))))

  ;; State machine via mutual recursion: parse simple arithmetic
  ;; Grammar: expr = term (('+' | '-') term)*
  ;;          term = number
  ;; Input is a list of tokens: numbers and operator symbols
  (letrec ((parse-expr
            (lambda (tokens)
              "Parse additive expression. Return (value . remaining-tokens)."
              (let ((left-result (funcall parse-term tokens)))
                (funcall parse-expr-rest (car left-result) (cdr left-result)))))
           (parse-expr-rest
            (lambda (left tokens)
              (cond
                ((and tokens (eq (car tokens) '+))
                 (let ((right-result (funcall parse-term (cdr tokens))))
                   (funcall parse-expr-rest
                            (+ left (car right-result))
                            (cdr right-result))))
                ((and tokens (eq (car tokens) '-))
                 (let ((right-result (funcall parse-term (cdr tokens))))
                   (funcall parse-expr-rest
                            (- left (car right-result))
                            (cdr right-result))))
                (t (cons left tokens)))))
           (parse-term
            (lambda (tokens)
              (if (numberp (car tokens))
                  (cons (car tokens) (cdr tokens))
                (cons 0 tokens)))))
    (list
      (car (funcall parse-expr '(5)))
      (car (funcall parse-expr '(3 + 4)))
      (car (funcall parse-expr '(10 - 3 + 2)))
      (car (funcall parse-expr '(1 + 2 + 3 + 4 + 5)))
      (car (funcall parse-expr '(100 - 50 - 25 - 10)))))

  ;; letrec with closures that share mutable state
  (letrec ((counter 0)
           (inc (lambda () (setq counter (1+ counter)) counter))
           (dec (lambda () (setq counter (1- counter)) counter))
           (get (lambda () counter)))
    (list (funcall inc) (funcall inc) (funcall inc)
          (funcall dec)
          (funcall get)
          (funcall inc) (funcall inc)
          (funcall get))))"#;
    let expect = expect_test::expect![[
        r#""OK (((t nil t nil t nil t nil t nil t) (nil t nil t nil t nil t nil t nil)) ((1 1 3 4 6 9 12 17 23) (1 2 2 4 5 7 10 13 18)) (5 7 9 15 15) (1 2 3 2 2 3 4 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: let bindings building a mini-interpreter environment
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_bind_environment_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement environment chains (like interpreter symbol tables)
    // using nested let bindings and closures
    let form = r#"(progn
  ;; Environment: list of frames, each frame is an alist
  (fset 'neovm--lb-make-env
    (lambda (parent)
      (cons nil parent)))

  (fset 'neovm--lb-env-define
    (lambda (env name val)
      (setcar env (cons (cons name val) (car env)))
      env))

  (fset 'neovm--lb-env-lookup
    (lambda (env name)
      (if (null env) (list 'error name)
        (let ((binding (assq name (car env))))
          (if binding
              (list 'ok (cdr binding))
            (funcall 'neovm--lb-env-lookup (cdr env) name))))))

  (fset 'neovm--lb-env-set
    (lambda (env name val)
      (if (null env) nil
        (let ((binding (assq name (car env))))
          (if binding
              (progn (setcdr binding val) t)
            (funcall 'neovm--lb-env-set (cdr env) name val))))))

  (unwind-protect
      ;; Create nested scopes like let bindings
      (let ((global (funcall 'neovm--lb-make-env nil)))
        (funcall 'neovm--lb-env-define global 'x 10)
        (funcall 'neovm--lb-env-define global 'y 20)

        ;; First nested scope (like a let block)
        (let ((scope1 (funcall 'neovm--lb-make-env global)))
          (funcall 'neovm--lb-env-define scope1 'x 100)  ; shadows global x
          (funcall 'neovm--lb-env-define scope1 'z 30)

          ;; Second nested scope
          (let ((scope2 (funcall 'neovm--lb-make-env scope1)))
            (funcall 'neovm--lb-env-define scope2 'w 40)

            (list
              ;; Lookup in scope2: x from scope1, y from global, z from scope1, w from scope2
              (funcall 'neovm--lb-env-lookup scope2 'x)
              (funcall 'neovm--lb-env-lookup scope2 'y)
              (funcall 'neovm--lb-env-lookup scope2 'z)
              (funcall 'neovm--lb-env-lookup scope2 'w)
              ;; Not found
              (funcall 'neovm--lb-env-lookup scope2 'missing)
              ;; Set x in parent scope via scope2
              (funcall 'neovm--lb-env-set scope2 'x 999)
              ;; Verify: scope1's x changed, global's x unchanged
              (funcall 'neovm--lb-env-lookup scope1 'x)
              (funcall 'neovm--lb-env-lookup global 'x)
              ;; Set y (only in global) via scope2
              (funcall 'neovm--lb-env-set scope2 'y 777)
              (funcall 'neovm--lb-env-lookup global 'y)))))
    (fmakunbound 'neovm--lb-make-env)
    (fmakunbound 'neovm--lb-env-define)
    (fmakunbound 'neovm--lb-env-lookup)
    (fmakunbound 'neovm--lb-env-set)))"#;
    let expect = expect_test::expect![[
        r#""OK ((ok 100) (ok 20) (ok 30) (ok 40) (error missing) t (ok 999) (ok 10) t (ok 777))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
