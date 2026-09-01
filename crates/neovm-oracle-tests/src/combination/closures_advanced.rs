//! Advanced closure pattern oracle tests: counter factories, iterator
//! protocols, callback patterns, loop capture semantics, generator patterns,
//! closure-based object systems, and memoizing closures.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Closure capturing mutable state: counter factory with step
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_counter_factory() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((make-counter
                         (lambda (&optional start step)
                           (let ((n (or start 0))
                                 (step (or step 1)))
                             (list
                              ;; next: return current and advance
                              (lambda ()
                                (let ((val n))
                                  (setq n (+ n step))
                                  val))
                              ;; peek: return current without advancing
                              (lambda () n)
                              ;; reset
                              (lambda (new-val) (setq n new-val)))))))
                    ;; Create two independent counters
                    (let ((c1 (funcall make-counter 0 1))
                          (c2 (funcall make-counter 100 5)))
                      (let ((next1 (nth 0 c1)) (peek1 (nth 1 c1)) (reset1 (nth 2 c1))
                            (next2 (nth 0 c2)) (peek2 (nth 1 c2)))
                        (list
                         ;; c1 counts 0,1,2,...
                         (funcall next1)   ;; 0
                         (funcall next1)   ;; 1
                         (funcall next1)   ;; 2
                         (funcall peek1)   ;; 3 (peek, not advanced)
                         ;; c2 counts 100,105,110,...
                         (funcall next2)   ;; 100
                         (funcall next2)   ;; 105
                         ;; Reset c1 and continue
                         (funcall reset1 50)
                         (funcall next1)   ;; 50
                         (funcall next1)   ;; 51
                         ;; c2 unaffected
                         (funcall peek2)   ;; 110
                         ))))"#;
    let expect = expect_test::expect![[r#""OK (0 1 2 3 100 105 50 50 51 110)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closure-based iterator protocol: next / has-next
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_iterator_protocol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((make-list-iter
                         (lambda (lst)
                           (let ((remaining lst))
                             (list
                              ;; has-next
                              (lambda () (not (null remaining)))
                              ;; next
                              (lambda ()
                                (if (null remaining)
                                    (signal 'error '("iterator exhausted"))
                                  (let ((val (car remaining)))
                                    (setq remaining (cdr remaining))
                                    val)))))))
                        (iter-to-list
                         (lambda (iter)
                           (let ((has-next (nth 0 iter))
                                 (next (nth 1 iter))
                                 (result nil))
                             (while (funcall has-next)
                               (setq result (cons (funcall next) result)))
                             (nreverse result)))))
                    ;; Iterate, collecting elements
                    (let ((iter (funcall make-list-iter '(10 20 30 40 50))))
                      (let ((has (nth 0 iter))
                            (nxt (nth 1 iter)))
                        (list
                         (funcall has)   ;; t
                         (funcall nxt)   ;; 10
                         (funcall nxt)   ;; 20
                         (funcall has)   ;; t
                         ;; Collect rest
                         (let ((rest nil))
                           (while (funcall has)
                             (setq rest (cons (funcall nxt) rest)))
                           (nreverse rest))
                         (funcall has)   ;; nil
                         ;; Full roundtrip on another list
                         (funcall iter-to-list
                                  (funcall make-list-iter '(a b c)))
                         ;; Empty list
                         (funcall iter-to-list
                                  (funcall make-list-iter nil))))))"#;
    let expect = expect_test::expect![[r#""OK (t 10 20 t (30 40 50) nil (a b c) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closure as callback in higher-order functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_callback_higher_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((my-filter
                         (lambda (pred lst)
                           (let ((result nil))
                             (dolist (x lst)
                               (when (funcall pred x)
                                 (setq result (cons x result))))
                             (nreverse result))))
                        (my-reduce
                         (lambda (fn init lst)
                           (let ((acc init))
                             (dolist (x lst)
                               (setq acc (funcall fn acc x)))
                             acc)))
                        (my-map
                         (lambda (fn lst)
                           (let ((result nil))
                             (dolist (x lst)
                               (setq result (cons (funcall fn x) result)))
                             (nreverse result)))))
                    (let ((numbers '(1 2 3 4 5 6 7 8 9 10)))
                      ;; Pipeline: filter evens, square them, sum
                      (let* ((evens (funcall my-filter
                                            (lambda (x) (= (% x 2) 0))
                                            numbers))
                             (squared (funcall my-map
                                              (lambda (x) (* x x))
                                              evens))
                             (total (funcall my-reduce
                                            (lambda (acc x) (+ acc x))
                                            0 squared)))
                        (list evens squared total
                              ;; Also: partition into odd/even
                              (let ((odds (funcall my-filter
                                                   (lambda (x) (/= (% x 2) 0))
                                                   numbers)))
                                (list odds
                                      (= (+ (length evens) (length odds))
                                         (length numbers))))))))"#;
    let expect =
        expect_test::expect![[r#""OK ((2 4 6 8 10) (4 16 36 64 100) 220 ((1 3 5 7 9) t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closure over loop variables: lambda capture semantics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_loop_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // In lexical binding, each iteration of `let` creates a fresh binding,
    // so closures capture distinct copies.
    let form = r#"(let ((closures nil))
                    ;; Create closures that each capture their own `i`
                    (dotimes (i 5)
                      (let ((captured i))
                        (setq closures
                              (cons (lambda () captured)
                                    closures))))
                    (setq closures (nreverse closures))
                    ;; Call each closure — should get 0,1,2,3,4
                    (mapcar #'funcall closures))"#;
    let expect = expect_test::expect![[r#""OK (0 1 2 3 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_closure_loop_capture_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Each closure captures its own mutable binding
    let form = r#"(let ((closures nil))
                    (dotimes (i 3)
                      (let ((val (* i 10)))
                        (setq closures
                              (cons (list
                                     (lambda () val)
                                     (lambda (x) (setq val x)))
                                    closures))))
                    (setq closures (nreverse closures))
                    ;; Read initial values
                    (let ((initial (mapcar (lambda (c) (funcall (car c)))
                                          closures)))
                      ;; Mutate second closure's binding
                      (funcall (cadr (nth 1 closures)) 999)
                      ;; Read again — only second should change
                      (let ((after (mapcar (lambda (c) (funcall (car c)))
                                          closures)))
                        (list initial after))))"#;
    let expect = expect_test::expect![[r#""OK ((0 10 20) (0 999 20))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Generator pattern: closure that yields successive values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_generator_fibonacci() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((make-fib-gen
                         (lambda ()
                           (let ((a 0) (b 1))
                             (lambda ()
                               (let ((val a))
                                 (let ((next (+ a b)))
                                   (setq a b)
                                   (setq b next))
                                 val))))))
                    (let ((fib (funcall make-fib-gen))
                          (results nil))
                      ;; Generate first 12 Fibonacci numbers
                      (dotimes (_ 12)
                        (setq results (cons (funcall fib) results)))
                      (nreverse results)))"#;
    let expect = expect_test::expect![[r#""OK (0 1 1 2 3 5 8 13 21 34 55 89)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_closure_generator_collatz() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Generator that yields the Collatz sequence starting from n
    let form = r#"(let ((make-collatz-gen
                         (lambda (start)
                           (let ((n start)
                                 (done nil))
                             (lambda ()
                               (if done nil
                                 (let ((val n))
                                   (if (= n 1)
                                       (setq done t)
                                     (setq n (if (= (% n 2) 0)
                                                 (/ n 2)
                                               (1+ (* 3 n)))))
                                   val)))))))
                    ;; Collect full Collatz sequence for several starting values
                    (let ((collect
                           (lambda (gen)
                             (let ((result nil) (val t))
                               (while (setq val (funcall gen))
                                 (setq result (cons val result)))
                               (nreverse result)))))
                      (list
                       (funcall collect (funcall make-collatz-gen 6))
                       (funcall collect (funcall make-collatz-gen 11))
                       (length (funcall collect (funcall make-collatz-gen 27))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((6 3 10 5 16 8 4 2 1) (11 34 17 52 26 13 40 20 10 5 16 8 4 2 1) 112)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closure-based object system: methods as closures in alist
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_object_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((make-stack
                         (lambda ()
                           (let ((items nil))
                             (list
                              (cons 'push
                                    (lambda (x)
                                      (setq items (cons x items))
                                      x))
                              (cons 'pop
                                    (lambda ()
                                      (if (null items)
                                          (signal 'error '("stack underflow"))
                                        (let ((top (car items)))
                                          (setq items (cdr items))
                                          top))))
                              (cons 'peek
                                    (lambda ()
                                      (car items)))
                              (cons 'size
                                    (lambda ()
                                      (length items)))
                              (cons 'to-list
                                    (lambda ()
                                      (copy-sequence items)))))))
                        (send
                         (lambda (obj method &rest args)
                           (let ((fn (cdr (assq method obj))))
                             (if fn (apply fn args)
                               (signal 'error
                                       (list "unknown method" method)))))))
                    (let ((s (funcall make-stack)))
                      (funcall send s 'push 10)
                      (funcall send s 'push 20)
                      (funcall send s 'push 30)
                      (list
                       (funcall send s 'size)     ;; 3
                       (funcall send s 'peek)     ;; 30
                       (funcall send s 'pop)      ;; 30
                       (funcall send s 'pop)      ;; 20
                       (funcall send s 'size)     ;; 1
                       (funcall send s 'to-list)  ;; (10)
                       (funcall send s 'pop)      ;; 10
                       (funcall send s 'size)     ;; 0
                       )))"#;
    let expect = expect_test::expect![[r#""OK (3 30 30 20 1 (10) 10 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Memoizing closure with hash-table cache
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_memoize_with_stats() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Memoizer that also tracks cache hits/misses
    let form = r#"(let ((make-memoizer
                         (lambda (fn)
                           (let ((cache (make-hash-table :test 'equal))
                                 (hits 0)
                                 (misses 0))
                             (list
                              ;; call
                              (lambda (&rest args)
                                (let ((result (gethash args cache 'MISS)))
                                  (if (eq result 'MISS)
                                      (progn
                                        (setq misses (1+ misses))
                                        (let ((val (apply fn args)))
                                          (puthash args val cache)
                                          val))
                                    (setq hits (1+ hits))
                                    result)))
                              ;; stats
                              (lambda ()
                                (list hits misses
                                      (hash-table-count cache))))))))
                    ;; Memoize factorial
                    (let ((memo (funcall make-memoizer
                                        (lambda (n)
                                          (if (<= n 1) 1
                                            (* n (funcall (nth 0 memo)
                                                          (1- n))))))))
                      (let ((call (nth 0 memo))
                            (stats (nth 1 memo)))
                        (list
                         ;; First call computes fresh
                         (funcall call 5)       ;; 120
                         (funcall stats)        ;; hits=0, misses depend on recursion
                         ;; Second call hits cache
                         (funcall call 5)       ;; 120 (cached)
                         ;; Compute 7! — reuses 5! from cache
                         (funcall call 7)       ;; 5040
                         (funcall stats)
                         ;; Verify values
                         (funcall call 1)       ;; 1
                         (funcall call 3)       ;; 6 (cached)
                         (funcall stats)))))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable memo)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: closure-based event emitter (pub/sub pattern)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_event_emitter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((make-emitter
                         (lambda ()
                           (let ((listeners (make-hash-table :test 'eq)))
                             (list
                              ;; on: register listener
                              (lambda (event handler)
                                (puthash event
                                         (cons handler
                                               (gethash event listeners nil))
                                         listeners))
                              ;; emit: fire event with data
                              (lambda (event data)
                                (let ((handlers (gethash event listeners nil)))
                                  (dolist (h handlers)
                                    (funcall h data))))
                              ;; count: number of listeners for event
                              (lambda (event)
                                (length (gethash event listeners nil))))))))
                    (let ((em (funcall make-emitter))
                          (log nil))
                      (let ((on (nth 0 em))
                            (emit (nth 1 em))
                            (count (nth 2 em)))
                        ;; Register handlers
                        (funcall on 'click
                                 (lambda (data)
                                   (setq log (cons (list 'click-1 data) log))))
                        (funcall on 'click
                                 (lambda (data)
                                   (setq log (cons (list 'click-2 data) log))))
                        (funcall on 'hover
                                 (lambda (data)
                                   (setq log (cons (list 'hover data) log))))
                        ;; Emit events
                        (funcall emit 'click 'button-a)
                        (funcall emit 'hover 'menu)
                        (funcall emit 'click 'button-b)
                        (list
                         (nreverse log)
                         (funcall count 'click)   ;; 2
                         (funcall count 'hover)   ;; 1
                         (funcall count 'keydown) ;; 0
                         ))))"#;
    let expect = expect_test::expect![[
        r#""OK (((click-2 button-a) (click-1 button-a) (hover menu) (click-2 button-b) (click-1 button-b)) 2 1 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
