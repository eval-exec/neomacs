//! Oracle parity tests for monad-like patterns in pure Elisp:
//! Maybe monad (nil as Nothing), Result monad (ok/err), bind/chain
//! operations, monadic pipelines with error propagation, list monad
//! (flatmap/cartesian), and State monad pattern using closures.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Maybe monad: nil as Nothing, non-nil as Just
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_monad_maybe_bind_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement Maybe monad with return/bind, then build multi-step
    // pipelines that short-circuit on nil.
    let form = r#"(progn
  (fset 'neovm--maybe-return (lambda (x) x))
  (fset 'neovm--maybe-bind
    (lambda (val f)
      (if (null val) nil (funcall f val))))
  ;; Monadic pipeline helpers
  (fset 'neovm--maybe-chain
    (lambda (val &rest fns)
      (let ((result val))
        (while (and result fns)
          (setq result (funcall 'neovm--maybe-bind result (car fns)))
          (setq fns (cdr fns)))
        result)))

  (unwind-protect
      (let ((safe-div (lambda (b) (lambda (a) (if (= b 0) nil (/ a b)))))
            (safe-sqrt (lambda (x) (if (< x 0) nil (sqrt x))))
            (safe-log (lambda (x) (if (<= x 0) nil (log x))))
            (add-10 (lambda (x) (+ x 10))))
        (list
          ;; Successful chain: 100 / 5 / 2 = 10
          (funcall 'neovm--maybe-chain 100
                   (funcall safe-div 5)
                   (funcall safe-div 2))
          ;; Fails at division by zero: 100 / 5 / 0 -> nil
          (funcall 'neovm--maybe-chain 100
                   (funcall safe-div 5)
                   (funcall safe-div 0)
                   add-10)
          ;; Starts with nil -> immediately nil
          (funcall 'neovm--maybe-chain nil
                   add-10
                   (funcall safe-div 2))
          ;; sqrt of negative -> nil
          (funcall 'neovm--maybe-chain -4
                   safe-sqrt)
          ;; Successful: sqrt(16) = 4.0, log(4.0) ~= 1.386
          (let ((r (funcall 'neovm--maybe-chain 16
                            safe-sqrt
                            safe-log)))
            (and r (> r 1.38) (< r 1.39)))
          ;; Nested alist lookup via maybe
          (let ((data '((users . ((alice . ((age . 30) (email . "a@b.c")))
                                  (bob . ((age . 25))))))))
            (let ((lookup (lambda (key)
                            (lambda (alist)
                              (let ((pair (assq key alist)))
                                (if pair (cdr pair) nil))))))
              (list
                ;; Successful deep lookup
                (funcall 'neovm--maybe-chain data
                         (funcall lookup 'users)
                         (funcall lookup 'alice)
                         (funcall lookup 'email))
                ;; Missing key -> nil
                (funcall 'neovm--maybe-chain data
                         (funcall lookup 'users)
                         (funcall lookup 'charlie)
                         (funcall lookup 'age))
                ;; Partial success
                (funcall 'neovm--maybe-chain data
                         (funcall lookup 'users)
                         (funcall lookup 'bob)
                         (funcall lookup 'email)))))))
    (fmakunbound 'neovm--maybe-return)
    (fmakunbound 'neovm--maybe-bind)
    (fmakunbound 'neovm--maybe-chain)))"#;
    let expect = expect_test::expect![[r#""OK (10 nil nil nil t (\"a@b.c\" nil nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Result monad: (ok . value) or (err . message)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_monad_result_bind_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Result monad where values are tagged cons cells: (ok . val) or (err . msg).
    // Bind propagates errors without calling the function.
    let form = r#"(progn
  (fset 'neovm--ok (lambda (v) (cons 'ok v)))
  (fset 'neovm--err (lambda (msg) (cons 'err msg)))
  (fset 'neovm--is-ok (lambda (r) (eq (car r) 'ok)))
  (fset 'neovm--is-err (lambda (r) (eq (car r) 'err)))
  (fset 'neovm--unwrap (lambda (r) (cdr r)))

  (fset 'neovm--result-bind
    (lambda (result f)
      (if (funcall 'neovm--is-ok result)
          (funcall f (funcall 'neovm--unwrap result))
        result)))

  (fset 'neovm--result-map
    (lambda (result f)
      (if (funcall 'neovm--is-ok result)
          (funcall 'neovm--ok (funcall f (funcall 'neovm--unwrap result)))
        result)))

  (unwind-protect
      (let ((parse-int (lambda (s)
              (let ((n (string-to-number s)))
                (if (and (= n 0) (not (string= s "0")))
                    (funcall 'neovm--err (format "not a number: %s" s))
                  (funcall 'neovm--ok n)))))
            (check-positive (lambda (n)
              (if (> n 0)
                  (funcall 'neovm--ok n)
                (funcall 'neovm--err (format "not positive: %d" n)))))
            (check-even (lambda (n)
              (if (= (% n 2) 0)
                  (funcall 'neovm--ok n)
                (funcall 'neovm--err (format "not even: %d" n))))))
        (list
          ;; Successful pipeline: "42" -> 42 -> positive -> even -> (* 2) = 84
          (let ((r (funcall 'neovm--result-bind (funcall parse-int "42")
                     (lambda (n)
                       (funcall 'neovm--result-bind (funcall check-positive n)
                         (lambda (n2)
                           (funcall 'neovm--result-map (funcall check-even n2)
                             (lambda (n3) (* n3 2)))))))))
            r)
          ;; Fails at parse: "abc"
          (funcall 'neovm--result-bind (funcall parse-int "abc")
            (lambda (n) (funcall check-positive n)))
          ;; Fails at positive check: "-5"
          (funcall 'neovm--result-bind (funcall parse-int "-5")
            (lambda (n) (funcall check-positive n)))
          ;; Fails at even check: "7"
          (funcall 'neovm--result-bind (funcall parse-int "7")
            (lambda (n)
              (funcall 'neovm--result-bind (funcall check-positive n)
                (lambda (n2) (funcall check-even n2)))))
          ;; Map over ok
          (funcall 'neovm--result-map (funcall 'neovm--ok 10)
            (lambda (x) (* x x)))
          ;; Map over err (no change)
          (funcall 'neovm--result-map (funcall 'neovm--err "oops")
            (lambda (x) (* x x)))))
    (fmakunbound 'neovm--ok)
    (fmakunbound 'neovm--err)
    (fmakunbound 'neovm--is-ok)
    (fmakunbound 'neovm--is-err)
    (fmakunbound 'neovm--unwrap)
    (fmakunbound 'neovm--result-bind)
    (fmakunbound 'neovm--result-map)))"#;
    let expect = expect_test::expect![[
        r#""OK ((ok . 84) (err . \"not a number: abc\") (err . \"not positive: -5\") (err . \"not even: 7\") (ok . 100) (err . \"oops\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Monadic pipeline with error accumulation (validation)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_monad_validation_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Validate a "user record" through multiple checks, accumulating
    // all errors instead of short-circuiting on the first one.
    let form = r#"(progn
  (fset 'neovm--validate
    (lambda (validators value)
      (let ((errors nil))
        (dolist (v validators)
          (let ((err (funcall v value)))
            (when err
              (setq errors (cons err errors)))))
        (if errors
            (cons 'errors (nreverse errors))
          (cons 'ok value)))))

  (unwind-protect
      (let ((check-name (lambda (rec)
              (let ((name (cdr (assq 'name rec))))
                (cond
                  ((null name) "name is required")
                  ((< (length name) 2) "name too short")
                  ((> (length name) 50) "name too long")
                  (t nil)))))
            (check-age (lambda (rec)
              (let ((age (cdr (assq 'age rec))))
                (cond
                  ((null age) "age is required")
                  ((not (integerp age)) "age must be integer")
                  ((< age 0) "age must be non-negative")
                  ((> age 150) "age is unrealistic")
                  (t nil)))))
            (check-email (lambda (rec)
              (let ((email (cdr (assq 'email rec))))
                (cond
                  ((null email) "email is required")
                  ((not (string-match-p "@" email)) "email must contain @")
                  (t nil))))))
        (let ((validators (list check-name check-age check-email)))
          (list
            ;; Valid record
            (funcall 'neovm--validate validators
                     '((name . "Alice") (age . 30) (email . "a@b.c")))
            ;; Missing name
            (funcall 'neovm--validate validators
                     '((age . 25) (email . "b@c.d")))
            ;; Multiple errors: name too short, negative age, bad email
            (funcall 'neovm--validate validators
                     '((name . "X") (age . -5) (email . "nope")))
            ;; All fields missing
            (funcall 'neovm--validate validators nil)
            ;; Edge case: empty name, zero age
            (funcall 'neovm--validate validators
                     '((name . "") (age . 0) (email . "x@y"))))))
    (fmakunbound 'neovm--validate)))"#;
    let expect = expect_test::expect![[
        r#""OK ((ok (name . \"Alice\") (age . 30) (email . \"a@b.c\")) (errors \"name is required\") (errors \"name too short\" \"age must be non-negative\" \"email must contain @\") (errors \"name is required\" \"age is required\" \"email is required\") (errors \"name too short\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// List monad: flatmap and cartesian product
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_monad_list_flatmap_cartesian() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // List monad where return wraps in a singleton list and bind is flatmap.
    // Use it to compute cartesian products, Pythagorean triples, etc.
    let form = r#"(progn
  (fset 'neovm--list-return (lambda (x) (list x)))
  (fset 'neovm--list-bind
    (lambda (lst f)
      (let ((result nil))
        (dolist (x lst)
          (setq result (append result (funcall f x))))
        result)))
  ;; guard: returns singleton or empty list based on predicate
  (fset 'neovm--guard
    (lambda (pred)
      (if pred (list t) nil)))

  (unwind-protect
      (list
        ;; Simple flatmap: double each element
        (funcall 'neovm--list-bind '(1 2 3)
          (lambda (x) (list x (* x 2))))

        ;; Cartesian product of two lists
        (funcall 'neovm--list-bind '(a b c)
          (lambda (x)
            (funcall 'neovm--list-bind '(1 2)
              (lambda (y)
                (funcall 'neovm--list-return (list x y))))))

        ;; Pythagorean triples up to 20
        (let ((triples nil))
          (funcall 'neovm--list-bind (number-sequence 1 20)
            (lambda (a)
              (funcall 'neovm--list-bind (number-sequence a 20)
                (lambda (b)
                  (funcall 'neovm--list-bind (number-sequence b 20)
                    (lambda (c)
                      (funcall 'neovm--list-bind
                        (funcall 'neovm--guard (= (+ (* a a) (* b b)) (* c c)))
                        (lambda (_)
                          (setq triples (cons (list a b c) triples))
                          (list (list a b c))))))))))
          (nreverse triples))

        ;; Flatmap with filtering: even squares
        (funcall 'neovm--list-bind '(1 2 3 4 5 6 7 8 9 10)
          (lambda (x)
            (let ((sq (* x x)))
              (if (= (% sq 2) 0)
                  (list sq)
                nil))))

        ;; Triple cartesian product
        (funcall 'neovm--list-bind '(x y)
          (lambda (a)
            (funcall 'neovm--list-bind '(1 2)
              (lambda (b)
                (funcall 'neovm--list-bind '(+ -)
                  (lambda (c)
                    (funcall 'neovm--list-return (list a b c)))))))))
    (fmakunbound 'neovm--list-return)
    (fmakunbound 'neovm--list-bind)
    (fmakunbound 'neovm--guard)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 2 4 3 6) ((a 1) (a 2) (b 1) (b 2) (c 1) (c 2)) ((3 4 5) (5 12 13) (6 8 10) (8 15 17) (9 12 15) (12 16 20)) (4 16 36 64 100) ((x 1 +) (x 1 -) (x 2 +) (x 2 -) (y 1 +) (y 1 -) (y 2 +) (y 2 -)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// State monad: thread state through computations via closures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_monad_state_closures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // State monad: a stateful computation is a function (state -> (value . new-state)).
    // Implement return, bind, get, put, modify, and run.
    let form = r#"(progn
  ;; state-return: value -> (state -> (value . state))
  (fset 'neovm--state-return
    (lambda (val) (lambda (s) (cons val s))))

  ;; state-bind: m -> (a -> m b) -> m b
  (fset 'neovm--state-bind
    (lambda (m f)
      (lambda (s)
        (let* ((result (funcall m s))
               (val (car result))
               (new-state (cdr result)))
          (funcall (funcall f val) new-state)))))

  ;; state-get: () -> m state  (returns state as value)
  (fset 'neovm--state-get
    (lambda () (lambda (s) (cons s s))))

  ;; state-put: new-state -> m nil  (replaces state)
  (fset 'neovm--state-put
    (lambda (new-s) (lambda (_s) (cons nil new-s))))

  ;; state-modify: (state -> state) -> m nil
  (fset 'neovm--state-modify
    (lambda (f) (lambda (s) (cons nil (funcall f s)))))

  ;; run-state: m a -> initial-state -> (value . final-state)
  (fset 'neovm--run-state
    (lambda (m init) (funcall m init)))

  (unwind-protect
      (list
        ;; Simple: counter that increments 3 times, returning intermediate values
        (funcall 'neovm--run-state
          (funcall 'neovm--state-bind
            (funcall 'neovm--state-modify (lambda (s) (1+ s)))
            (lambda (_)
              (funcall 'neovm--state-bind
                (funcall 'neovm--state-modify (lambda (s) (1+ s)))
                (lambda (_)
                  (funcall 'neovm--state-bind
                    (funcall 'neovm--state-modify (lambda (s) (1+ s)))
                    (lambda (_)
                      (funcall 'neovm--state-get)))))))
          0)  ;; -> (3 . 3)

        ;; Accumulator: sum a list of numbers using state
        (funcall 'neovm--run-state
          (let ((computation (funcall 'neovm--state-return nil)))
            (dolist (n '(10 20 30 40 50))
              (setq computation
                    (funcall 'neovm--state-bind computation
                      (lambda (_)
                        (funcall 'neovm--state-modify
                          (lambda (s) (+ s n)))))))
            (funcall 'neovm--state-bind computation
              (lambda (_) (funcall 'neovm--state-get))))
          0)  ;; -> (150 . 150)

        ;; Stack operations: push/pop using a list as state
        (funcall 'neovm--run-state
          (funcall 'neovm--state-bind
            ;; push 10
            (funcall 'neovm--state-modify (lambda (s) (cons 10 s)))
            (lambda (_)
              (funcall 'neovm--state-bind
                ;; push 20
                (funcall 'neovm--state-modify (lambda (s) (cons 20 s)))
                (lambda (_)
                  (funcall 'neovm--state-bind
                    ;; push 30
                    (funcall 'neovm--state-modify (lambda (s) (cons 30 s)))
                    (lambda (_)
                      (funcall 'neovm--state-bind
                        ;; pop (get top, remove it)
                        (funcall 'neovm--state-bind
                          (funcall 'neovm--state-get)
                          (lambda (st)
                            (funcall 'neovm--state-bind
                              (funcall 'neovm--state-put (cdr st))
                              (lambda (_)
                                (funcall 'neovm--state-return (car st))))))
                        (lambda (popped)
                          (funcall 'neovm--state-bind
                            (funcall 'neovm--state-get)
                            (lambda (remaining)
                              (funcall 'neovm--state-return
                                       (list 'popped popped
                                             'remaining remaining))))))))))))
          nil)  ;; -> ((popped 30 remaining (20 10)) . (20 10))

        ;; State-based RNG (linear congruential)
        (funcall 'neovm--run-state
          (let ((next-rand (lambda ()
                  (funcall 'neovm--state-bind
                    (funcall 'neovm--state-get)
                    (lambda (seed)
                      (let ((new-seed (% (+ (* seed 1103515245) 12345)
                                         2147483648)))
                        (funcall 'neovm--state-bind
                          (funcall 'neovm--state-put new-seed)
                          (lambda (_)
                            (funcall 'neovm--state-return
                                     (% new-seed 100))))))))))
            ;; Generate 5 random numbers
            (funcall 'neovm--state-bind (funcall next-rand)
              (lambda (r1)
                (funcall 'neovm--state-bind (funcall next-rand)
                  (lambda (r2)
                    (funcall 'neovm--state-bind (funcall next-rand)
                      (lambda (r3)
                        (funcall 'neovm--state-bind (funcall next-rand)
                          (lambda (r4)
                            (funcall 'neovm--state-bind (funcall next-rand)
                              (lambda (r5)
                                (funcall 'neovm--state-return
                                         (list r1 r2 r3 r4 r5)))))))))))))
          42))  ;; seed = 42
    (fmakunbound 'neovm--state-return)
    (fmakunbound 'neovm--state-bind)
    (fmakunbound 'neovm--state-get)
    (fmakunbound 'neovm--state-put)
    (fmakunbound 'neovm--state-modify)
    (fmakunbound 'neovm--run-state)))"#;
    let expect = expect_test::expect![[
        r#""OK ((3 . 3) (150 . 150) ((popped 30 remaining (20 10)) 20 10) ((27 64 53 6 35) . 908095735))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Writer monad: accumulate a log alongside computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_monad_writer_logging() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Writer monad where each computation produces (value . log-entries).
    // Bind concatenates logs. Useful for tracing computations.
    let form = r#"(progn
  ;; Writer value is (value . log-list)
  (fset 'neovm--writer-return
    (lambda (val) (cons val nil)))
  (fset 'neovm--writer-bind
    (lambda (writer f)
      (let* ((val (car writer))
             (log1 (cdr writer))
             (result (funcall f val))
             (val2 (car result))
             (log2 (cdr result)))
        (cons val2 (append log1 log2)))))
  (fset 'neovm--writer-tell
    (lambda (msg) (cons nil (list msg))))
  ;; Run a computation with logging
  (fset 'neovm--writer-logged
    (lambda (msg val)
      (cons val (list msg))))

  (unwind-protect
      (list
        ;; Simple logged computation
        (funcall 'neovm--writer-bind
          (funcall 'neovm--writer-logged "start with 10" 10)
          (lambda (x)
            (funcall 'neovm--writer-bind
              (funcall 'neovm--writer-logged
                       (format "doubled to %d" (* x 2)) (* x 2))
              (lambda (y)
                (funcall 'neovm--writer-bind
                  (funcall 'neovm--writer-logged
                           (format "added 5 to get %d" (+ y 5)) (+ y 5))
                  (lambda (z)
                    (funcall 'neovm--writer-return z)))))))

        ;; Pipeline with conditional logging
        (let ((safe-div-logged
                (lambda (a b)
                  (if (= b 0)
                      (funcall 'neovm--writer-logged
                               (format "ERROR: div by zero: %d/%d" a b) nil)
                    (funcall 'neovm--writer-logged
                             (format "%d / %d = %d" a b (/ a b))
                             (/ a b))))))
          (funcall 'neovm--writer-bind
            (funcall safe-div-logged 100 5)
            (lambda (r1)
              (if (null r1)
                  (funcall 'neovm--writer-logged "aborted" nil)
                (funcall 'neovm--writer-bind
                  (funcall safe-div-logged r1 4)
                  (lambda (r2)
                    (funcall 'neovm--writer-return r2)))))))

        ;; Factorial with full trace
        (let ((result (funcall 'neovm--writer-return 1)))
          (dolist (i '(1 2 3 4 5))
            (setq result
                  (funcall 'neovm--writer-bind result
                    (lambda (acc)
                      (let ((new-acc (* acc i)))
                        (funcall 'neovm--writer-logged
                                 (format "%d * %d = %d" acc i new-acc)
                                 new-acc))))))
          result))
    (fmakunbound 'neovm--writer-return)
    (fmakunbound 'neovm--writer-bind)
    (fmakunbound 'neovm--writer-tell)
    (fmakunbound 'neovm--writer-logged)))"#;
    let expect = expect_test::expect![[
        r#""OK ((25 \"start with 10\" \"doubled to 20\" \"added 5 to get 25\") (5 \"100 / 5 = 20\" \"20 / 4 = 5\") (120 \"1 * 1 = 1\" \"1 * 2 = 2\" \"2 * 3 = 6\" \"6 * 4 = 24\" \"24 * 5 = 120\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combining monads: Maybe + Writer (optional computation with logging)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_monad_combined_maybe_writer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // MaybeWriter: (ok value . log) or (nothing . log).
    // Short-circuits on nothing but still preserves the log up to that point.
    let form = r#"(progn
  (fset 'neovm--mw-ok
    (lambda (val &optional msg)
      (if msg
          (list 'ok val msg)
        (list 'ok val))))
  (fset 'neovm--mw-nothing
    (lambda (msg)
      (list 'nothing nil msg)))
  (fset 'neovm--mw-bind
    (lambda (mw f)
      (let ((tag (car mw))
            (val (cadr mw))
            (log (cddr mw)))
        (if (eq tag 'nothing)
            mw  ;; propagate nothing with its log
          (let ((result (funcall f val)))
            (let ((r-tag (car result))
                  (r-val (cadr result))
                  (r-log (cddr result)))
              (list r-tag r-val (append log r-log))))))))

  (unwind-protect
      (let ((safe-div (lambda (b)
              (lambda (a)
                (if (= b 0)
                    (funcall 'neovm--mw-nothing
                             (format "div by zero: %d/0" a))
                  (funcall 'neovm--mw-ok (/ a b)
                           (format "%d/%d=%d" a b (/ a b)))))))
            (check-pos (lambda (x)
              (if (> x 0)
                  (funcall 'neovm--mw-ok x
                           (format "%d is positive" x))
                (funcall 'neovm--mw-nothing
                         (format "%d is not positive" x))))))
        (list
          ;; Success: 100 / 5 = 20, check positive, / 4 = 5
          (funcall 'neovm--mw-bind
            (funcall 'neovm--mw-bind
              (funcall 'neovm--mw-bind
                (funcall 'neovm--mw-ok 100 "start=100")
                (funcall safe-div 5))
              check-pos)
            (funcall safe-div 4))
          ;; Failure at division: 100 / 5 = 20, / 0 -> nothing
          (funcall 'neovm--mw-bind
            (funcall 'neovm--mw-bind
              (funcall 'neovm--mw-ok 100 "start=100")
              (funcall safe-div 5))
            (funcall safe-div 0))
          ;; Failure at positivity check: 100 / 200 = 0, not positive
          (funcall 'neovm--mw-bind
            (funcall 'neovm--mw-bind
              (funcall 'neovm--mw-ok 100 "start=100")
              (funcall safe-div 200))
            check-pos)
          ;; Start with nothing
          (funcall 'neovm--mw-bind
            (funcall 'neovm--mw-nothing "no input")
            (funcall safe-div 5))))
    (fmakunbound 'neovm--mw-ok)
    (fmakunbound 'neovm--mw-nothing)
    (fmakunbound 'neovm--mw-bind)))"#;
    let expect = expect_test::expect![[
        r#""OK ((ok 5 (((\"start=100\" \"100/5=20\") \"20 is positive\") \"20/4=5\")) (nothing nil ((\"start=100\" \"100/5=20\") \"div by zero: 20/0\")) (nothing nil ((\"start=100\" \"100/200=0\") \"0 is not positive\")) (nothing nil \"no input\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Reader monad: shared environment threaded through computations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_monad_reader_environment() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Reader monad: computation is (env -> value). Bind threads the same
    // environment. ask returns the environment itself. local modifies it
    // for a sub-computation.
    let form = r#"(progn
  ;; reader-return: val -> (env -> val)
  (fset 'neovm--reader-return
    (lambda (val) (lambda (_env) val)))
  ;; reader-bind: m -> (a -> m b) -> m b
  (fset 'neovm--reader-bind
    (lambda (m f)
      (lambda (env)
        (let ((val (funcall m env)))
          (funcall (funcall f val) env)))))
  ;; reader-ask: () -> m env
  (fset 'neovm--reader-ask
    (lambda () (lambda (env) env)))
  ;; reader-local: (env -> env') -> m a -> m a
  (fset 'neovm--reader-local
    (lambda (modify-env m)
      (lambda (env)
        (funcall m (funcall modify-env env)))))
  ;; run-reader: m a -> env -> a
  (fset 'neovm--run-reader
    (lambda (m env) (funcall m env)))

  (unwind-protect
      (let ((env '((db-host . "localhost")
                   (db-port . 5432)
                   (debug . t)
                   (prefix . "/api"))))
        (list
          ;; Simple: read config values
          (funcall 'neovm--run-reader
            (funcall 'neovm--reader-bind
              (funcall 'neovm--reader-ask)
              (lambda (cfg)
                (funcall 'neovm--reader-return
                  (list (cdr (assq 'db-host cfg))
                        (cdr (assq 'db-port cfg))))))
            env)

          ;; Build a URL from config
          (funcall 'neovm--run-reader
            (funcall 'neovm--reader-bind
              (funcall 'neovm--reader-ask)
              (lambda (cfg)
                (funcall 'neovm--reader-return
                  (format "%s:%d%s/users"
                          (cdr (assq 'db-host cfg))
                          (cdr (assq 'db-port cfg))
                          (cdr (assq 'prefix cfg))))))
            env)

          ;; Use local to override debug for a sub-computation
          (funcall 'neovm--run-reader
            (funcall 'neovm--reader-bind
              (funcall 'neovm--reader-ask)
              (lambda (outer-cfg)
                (funcall 'neovm--reader-bind
                  (funcall 'neovm--reader-local
                    (lambda (cfg)
                      (cons '(debug . nil)
                            (assq-delete-all 'debug cfg)))
                    (funcall 'neovm--reader-bind
                      (funcall 'neovm--reader-ask)
                      (lambda (inner-cfg)
                        (funcall 'neovm--reader-return
                          (cdr (assq 'debug inner-cfg))))))
                  (lambda (inner-debug)
                    (funcall 'neovm--reader-return
                      (list 'outer-debug (cdr (assq 'debug outer-cfg))
                            'inner-debug inner-debug))))))
            env)))
    (fmakunbound 'neovm--reader-return)
    (fmakunbound 'neovm--reader-bind)
    (fmakunbound 'neovm--reader-ask)
    (fmakunbound 'neovm--reader-local)
    (fmakunbound 'neovm--run-reader)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"localhost\" 5432) \"localhost:5432/api/users\" (outer-debug nil inner-debug nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
