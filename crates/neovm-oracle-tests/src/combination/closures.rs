//! Complex oracle tests for closure patterns: lexical closures,
//! closure as objects, closure factories, closure-based iteration
//! protocols, and closure over mutable state.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Closure as counter (mutable closed-over state)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_counter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((make-counter
                         (lambda (&optional start)
                           (let ((n (or start 0)))
                             (lambda (&optional reset)
                               (if reset (setq n 0)
                                 (setq n (1+ n))
                                 n))))))
                    (let ((c1 (funcall make-counter))
                          (c2 (funcall make-counter 100)))
                      (list (funcall c1)     ;; 1
                            (funcall c1)     ;; 2
                            (funcall c2)     ;; 101
                            (funcall c1)     ;; 3
                            (funcall c2)     ;; 102
                            ;; Reset c1
                            (funcall c1 t)   ;; 0
                            (funcall c1)     ;; 1
                            (funcall c2))))" ;; 103
    ;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// Closure as accumulator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_accumulator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((make-accumulator
                         (lambda ()
                           (let ((items nil))
                             (list
                              (lambda (item) (setq items (cons item items)))
                              (lambda () (nreverse items))
                              (lambda () (length items)))))))
                    (let ((acc (funcall make-accumulator)))
                      (let ((add (nth 0 acc))
                            (get (nth 1 acc))
                            (count (nth 2 acc)))
                        (funcall add 'a)
                        (funcall add 'b)
                        (funcall add 'c)
                        (list (funcall count)
                              (funcall get)
                              ;; Add more
                              (progn (funcall add 'd) (funcall count))
                              (funcall get)))))"#;
    let expect = expect_test::expect![[r#""OK (3 (a b c d) 2 (c d))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closure factory: compose / pipe
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_compose() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((compose
                         (lambda (f g)
                           (lambda (x)
                             (funcall f (funcall g x)))))
                        (pipe
                         (lambda (&rest fns)
                           (seq-reduce
                            (lambda (f g)
                              (lambda (x)
                                (funcall g (funcall f x))))
                            (cdr fns) (car fns)))))
                    (let ((double (lambda (x) (* 2 x)))
                          (inc (lambda (x) (1+ x)))
                          (square (lambda (x) (* x x))))
                      (list
                       ;; compose: f(g(x))
                       (funcall (funcall compose double inc) 3)  ;; (3+1)*2=8
                       (funcall (funcall compose inc double) 3)  ;; 3*2+1=7
                       ;; pipe: left-to-right
                       (funcall (funcall pipe inc double square) 3))))"#;
    let expect = expect_test::expect![[r#""OK (8 7 64)""#]];
    // ((3+1)*2)^2=64
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closure-based iterator protocol
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_iterator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((make-range-iter
                         (lambda (start end &optional step)
                           (let ((current start)
                                 (step (or step 1)))
                             (lambda ()
                               (if (< current end)
                                   (let ((val current))
                                     (setq current (+ current step))
                                     val)
                                 'done)))))
                        (iter-collect
                         (lambda (iter)
                           (let ((result nil) (val nil))
                             (while (not (eq (setq val (funcall iter))
                                             'done))
                               (setq result (cons val result)))
                             (nreverse result)))))
                    (list
                     (funcall iter-collect
                              (funcall make-range-iter 0 5))
                     (funcall iter-collect
                              (funcall make-range-iter 0 10 2))
                     (funcall iter-collect
                              (funcall make-range-iter 10 10))))"#;
    let expect = expect_test::expect![[r#""OK ((0 1 2 3 4) (0 2 4 6 8) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closure-based memoization
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_memoize() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((memoize
                         (lambda (fn)
                           (let ((cache (make-hash-table :test 'equal)))
                             (lambda (&rest args)
                               (let ((cached (gethash args cache 'miss)))
                                 (if (eq cached 'miss)
                                     (let ((result (apply fn args)))
                                       (puthash args result cache)
                                       result)
                                   cached)))))))
                    ;; Memoize a "slow" function
                    (let ((call-count 0))
                      (let ((expensive
                             (funcall memoize
                                      (lambda (x y)
                                        (setq call-count (1+ call-count))
                                        (+ (* x x) (* y y))))))
                        (list
                         (funcall expensive 3 4)  ;; 25, call
                         call-count               ;; 1
                         (funcall expensive 3 4)  ;; 25, cached
                         call-count               ;; still 1
                         (funcall expensive 5 12) ;; 169, call
                         call-count               ;; 2
                         (funcall expensive 3 4)  ;; 25, cached
                         call-count))))"#;
    let expect = expect_test::expect![[r#""OK (25 1 25 1 169 2 25 2)""#]];
    // still 2
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closure as observable/signal
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_observable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Observable value with watchers
    let form = r#"(let ((make-observable
                         (lambda (initial)
                           (let ((value initial)
                                 (watchers nil))
                             (list
                              ;; get
                              (lambda () value)
                              ;; set
                              (lambda (new-val)
                                (let ((old value))
                                  (setq value new-val)
                                  (dolist (w watchers)
                                    (funcall w old new-val)))
                                new-val)
                              ;; watch
                              (lambda (fn)
                                (setq watchers
                                      (cons fn watchers))))))))
                    (let ((obs (funcall make-observable 0))
                          (log nil))
                      (let ((get-val (nth 0 obs))
                            (set-val (nth 1 obs))
                            (watch (nth 2 obs)))
                        ;; Add watcher
                        (funcall watch
                                 (lambda (old new)
                                   (setq log
                                         (cons (list old '-> new) log))))
                        ;; Mutate
                        (funcall set-val 10)
                        (funcall set-val 20)
                        (funcall set-val 30)
                        (list (funcall get-val)
                              (nreverse log)))))"#;
    let expect = expect_test::expect![[r#""OK (30 ((0 -> 10) (10 -> 20) (20 -> 30)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closure-based middleware chain
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_middleware() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // HTTP-style middleware: each wraps the next handler
    let form = r#"(let ((make-logger
                         (lambda (next log)
                           (lambda (request)
                             (setq log (cons (list 'log request) log))
                             (let ((response (funcall next request)))
                               (setq log
                                     (cons (list 'log-resp response) log))
                               response))))
                        (make-uppercaser
                         (lambda (next)
                           (lambda (request)
                             (funcall next (upcase request)))))
                        (make-prefixer
                         (lambda (next prefix)
                           (lambda (request)
                             (funcall next
                                      (concat prefix request))))))
                    (let ((log nil))
                      (let* ((handler (lambda (req)
                                        (concat "OK:" req)))
                             (stack handler))
                        (setq stack (funcall make-uppercaser stack))
                        (setq stack (funcall make-prefixer stack "["))
                        (setq stack (funcall make-logger stack log))
                        (list
                         (funcall stack "hello")
                         (funcall stack "world")))))"#;
    let expect = expect_test::expect![[r#""OK (\"OK:[HELLO\" \"OK:[WORLD\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
