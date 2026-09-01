//! Oracle parity tests for coroutine-like patterns in Elisp:
//! generators using closures (yield/resume via state machine),
//! cooperative multitasking simulation, producer-consumer with
//! bounded buffer, interleaved execution of multiple generators,
//! pipeline of generators.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic generator: closure-based state machine with yield/resume
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coroutine_basic_generator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-make-range-gen
    (lambda (start end &optional step)
      "Create a generator that yields integers from START to END (exclusive).
       Returns closure with :next -> (value . t) or (nil . nil) when exhausted."
      (let ((current start)
            (s (or step 1)))
        (lambda (op)
          (cond
            ((eq op :next)
             (if (< current end)
                 (let ((val current))
                   (setq current (+ current s))
                   (cons val t))
               (cons nil nil)))
            ((eq op :reset)
             (setq current start)
             nil)
            ((eq op :peek)
             (if (< current end)
                 (cons current t)
               (cons nil nil))))))))

  (fset 'neovm--test-gen-collect
    (lambda (gen &optional max-items)
      "Collect all values from generator GEN into a list.
       Optionally stop after MAX-ITEMS."
      (let ((result nil)
            (count 0)
            (limit (or max-items most-positive-fixnum))
            (done nil))
        (while (and (not done) (< count limit))
          (let ((item (funcall gen :next)))
            (if (cdr item)
                (progn
                  (setq result (cons (car item) result))
                  (setq count (1+ count)))
              (setq done t))))
        (nreverse result))))

  (unwind-protect
      (list
        ;; Basic range generator
        (let ((g (funcall 'neovm--test-make-range-gen 0 5)))
          (funcall 'neovm--test-gen-collect g))
        ;; Range with step
        (let ((g (funcall 'neovm--test-make-range-gen 0 20 3)))
          (funcall 'neovm--test-gen-collect g))
        ;; Collect only first 3
        (let ((g (funcall 'neovm--test-make-range-gen 0 100)))
          (funcall 'neovm--test-gen-collect g 3))
        ;; Peek doesn't consume
        (let ((g (funcall 'neovm--test-make-range-gen 10 15)))
          (list (funcall g :peek)
                (funcall g :peek)
                (funcall g :next)
                (funcall g :peek)
                (funcall g :next)))
        ;; Reset and re-collect
        (let ((g (funcall 'neovm--test-make-range-gen 0 3)))
          (let ((first-run (funcall 'neovm--test-gen-collect g)))
            (funcall g :reset)
            (let ((second-run (funcall 'neovm--test-gen-collect g)))
              (list first-run second-run (equal first-run second-run)))))
        ;; Empty generator
        (let ((g (funcall 'neovm--test-make-range-gen 5 5)))
          (funcall 'neovm--test-gen-collect g))
        ;; Exhaustion: next after done returns (nil . nil)
        (let ((g (funcall 'neovm--test-make-range-gen 0 2)))
          (list (funcall g :next) (funcall g :next)
                (funcall g :next) (funcall g :next))))
    (fmakunbound 'neovm--test-make-range-gen)
    (fmakunbound 'neovm--test-gen-collect)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 1 2 3 4) (0 3 6 9 12 15 18) (0 1 2) ((10 . t) (10 . t) (10 . t) (11 . t) (11 . t)) ((0 1 2) (0 1 2) t) nil ((0 . t) (1 . t) (nil) (nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Stateful generators: Fibonacci, factorial, custom sequences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coroutine_stateful_generators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-make-fib-gen
    (lambda ()
      "Generator that yields Fibonacci numbers indefinitely."
      (let ((a 0) (b 1))
        (lambda (op)
          (cond
            ((eq op :next)
             (let ((val a))
               (let ((next (+ a b)))
                 (setq a b b next))
               (cons val t)))
            ((eq op :reset)
             (setq a 0 b 1) nil))))))

  (fset 'neovm--test-make-factorial-gen
    (lambda ()
      "Generator that yields 0!, 1!, 2!, 3!, ..."
      (let ((n 0) (fact 1))
        (lambda (op)
          (cond
            ((eq op :next)
             (let ((val (if (= n 0) 1 fact)))
               (setq n (1+ n))
               (setq fact (* fact n))
               (cons val t))))))))

  (fset 'neovm--test-make-collatz-gen
    (lambda (start)
      "Generator that yields the Collatz sequence from START until reaching 1."
      (let ((current start)
            (done nil))
        (lambda (op)
          (cond
            ((eq op :next)
             (if done
                 (cons nil nil)
               (let ((val current))
                 (if (= current 1)
                     (setq done t)
                   (setq current
                         (if (= (% current 2) 0)
                             (/ current 2)
                           (+ (* 3 current) 1))))
                 (cons val t)))))))))

  (fset 'neovm--test-gen-take
    (lambda (gen n)
      "Take N values from generator GEN."
      (let ((result nil) (count 0))
        (while (< count n)
          (let ((item (funcall gen :next)))
            (if (cdr item)
                (progn (setq result (cons (car item) result))
                       (setq count (1+ count)))
              (setq count n))))
        (nreverse result))))

  (unwind-protect
      (list
        ;; First 12 Fibonacci numbers
        (let ((g (funcall 'neovm--test-make-fib-gen)))
          (funcall 'neovm--test-gen-take g 12))
        ;; First 8 factorials
        (let ((g (funcall 'neovm--test-make-factorial-gen)))
          (funcall 'neovm--test-gen-take g 8))
        ;; Collatz sequence from 6
        (let ((g (funcall 'neovm--test-make-collatz-gen 6)))
          (funcall 'neovm--test-gen-take g 20))
        ;; Collatz from 27 (famously long)
        (let ((g (funcall 'neovm--test-make-collatz-gen 27)))
          (let ((seq (funcall 'neovm--test-gen-take g 200)))
            (list (length seq)
                  (car seq)
                  (car (last seq))
                  (apply #'max seq))))
        ;; Fibonacci reset
        (let ((g (funcall 'neovm--test-make-fib-gen)))
          (let ((first (funcall 'neovm--test-gen-take g 5)))
            (funcall g :reset)
            (let ((second (funcall 'neovm--test-gen-take g 5)))
              (equal first second)))))
    (fmakunbound 'neovm--test-make-fib-gen)
    (fmakunbound 'neovm--test-make-factorial-gen)
    (fmakunbound 'neovm--test-make-collatz-gen)
    (fmakunbound 'neovm--test-gen-take)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 1 1 2 3 5 8 13 21 34 55 89) (1 1 2 6 24 120 720 5040) (6 3 10 5 16 8 4 2 1) (112 27 1 9232) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cooperative multitasking: round-robin scheduler
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coroutine_cooperative_scheduler() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-make-task
    (lambda (name steps)
      "Create a task that executes STEPS one at a time.
       Each step is a function that returns a value.
       Task returns (:yield name value) or (:done name) on completion."
      (let ((remaining (copy-sequence steps))
            (step-num 0))
        (lambda (op)
          (cond
            ((eq op :step)
             (if remaining
                 (let* ((step-fn (car remaining))
                        (result (funcall step-fn)))
                   (setq remaining (cdr remaining))
                   (setq step-num (1+ step-num))
                   (list :yield name result step-num))
               (list :done name nil step-num)))
            ((eq op :done-p)
             (null remaining))
            ((eq op :name) name))))))

  (fset 'neovm--test-scheduler
    (lambda (tasks max-ticks)
      "Run tasks in round-robin until all done or MAX-TICKS reached.
       Returns execution log."
      (let ((queue (copy-sequence tasks))
            (log nil)
            (tick 0))
        (while (and queue (< tick max-ticks))
          (let* ((task (car queue))
                 (result (funcall task :step)))
            (setq log (cons (cons tick result) log))
            (setq tick (1+ tick))
            ;; Remove completed tasks, rotate queue
            (if (funcall task :done-p)
                (setq queue (cdr queue))
              (setq queue (append (cdr queue) (list task))))))
        (nreverse log))))

  (unwind-protect
      (let* ((t1 (funcall 'neovm--test-make-task 'A
                          (list (lambda () "A-start")
                                (lambda () "A-mid")
                                (lambda () "A-end"))))
             (t2 (funcall 'neovm--test-make-task 'B
                          (list (lambda () "B-start")
                                (lambda () "B-end"))))
             (t3 (funcall 'neovm--test-make-task 'C
                          (list (lambda () "C-only")))))
        (list
          ;; Round-robin execution log
          (funcall 'neovm--test-scheduler (list t1 t2 t3) 20)
          ;; Single task
          (let ((solo (funcall 'neovm--test-make-task 'SOLO
                               (list (lambda () 1)
                                     (lambda () 2)
                                     (lambda () 3)))))
            (funcall 'neovm--test-scheduler (list solo) 10))
          ;; Empty task list
          (funcall 'neovm--test-scheduler nil 10)
          ;; Tasks with computation
          (let ((counter 0))
            (let ((compute-task
                   (funcall 'neovm--test-make-task 'COMPUTE
                            (list (lambda () (setq counter (+ counter 10)) counter)
                                  (lambda () (setq counter (* counter 2)) counter)
                                  (lambda () (setq counter (- counter 5)) counter)))))
              (let ((log (funcall 'neovm--test-scheduler (list compute-task) 10)))
                (list log counter))))))
    (fmakunbound 'neovm--test-make-task)
    (fmakunbound 'neovm--test-scheduler)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 :yield A \"A-start\" 1) (1 :yield B \"B-start\" 1) (2 :yield C \"C-only\" 1) (3 :yield A \"A-mid\" 2) (4 :yield B \"B-end\" 2) (5 :yield A \"A-end\" 3)) ((0 :yield SOLO 1 1) (1 :yield SOLO 2 2) (2 :yield SOLO 3 3)) nil (((0 :yield COMPUTE 10 1) (1 :yield COMPUTE 20 2) (2 :yield COMPUTE 15 3)) 15))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Producer-consumer with bounded buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coroutine_producer_consumer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-make-buffer
    (lambda (capacity)
      "Create a bounded buffer with :put, :get, :full-p, :empty-p, :size, :contents."
      (let ((items nil)
            (count 0))
        (lambda (op &rest args)
          (cond
            ((eq op :put)
             (if (>= count capacity)
                 nil  ;; buffer full, reject
               (setq items (append items (list (car args))))
               (setq count (1+ count))
               t))
            ((eq op :get)
             (if (= count 0)
                 (cons nil nil)  ;; buffer empty
               (let ((val (car items)))
                 (setq items (cdr items))
                 (setq count (1- count))
                 (cons val t))))
            ((eq op :full-p) (>= count capacity))
            ((eq op :empty-p) (= count 0))
            ((eq op :size) count)
            ((eq op :contents) (copy-sequence items)))))))

  (fset 'neovm--test-run-producer-consumer
    (lambda (buf produce-list consume-count)
      "Simulate producer-consumer: produce items, then consume.
       Returns (produced-count consumed-items rejected-count)."
      (let ((produced 0)
            (rejected 0)
            (consumed nil))
        ;; Produce
        (dolist (item produce-list)
          (if (funcall buf :put item)
              (setq produced (1+ produced))
            (setq rejected (1+ rejected))))
        ;; Consume
        (dotimes (_ consume-count)
          (let ((result (funcall buf :get)))
            (when (cdr result)
              (setq consumed (cons (car result) consumed)))))
        (list :produced produced
              :consumed (nreverse consumed)
              :rejected rejected
              :remaining (funcall buf :size)))))

  (unwind-protect
      (list
        ;; Basic producer-consumer with capacity 3
        (let ((buf (funcall 'neovm--test-make-buffer 3)))
          (funcall 'neovm--test-run-producer-consumer
                   buf '(a b c d e) 5))
        ;; Interleaved produce and consume
        (let ((buf (funcall 'neovm--test-make-buffer 2))
              (log nil))
          ;; Produce 2, consume 1, produce 2 more, consume all
          (funcall buf :put 'x)
          (funcall buf :put 'y)
          (setq log (cons (funcall buf :get) log))    ;; get x
          (funcall buf :put 'z)
          (funcall buf :put 'w)                        ;; should succeed (buffer had room)
          (let ((rejected (not (funcall buf :put 'v)))) ;; should fail (full)
            (setq log (cons (funcall buf :get) log))   ;; get y
            (setq log (cons (funcall buf :get) log))   ;; get z
            (setq log (cons (funcall buf :get) log))   ;; get w
            (setq log (cons (funcall buf :get) log))   ;; get nil (empty)
            (list :log (nreverse log)
                  :rejected rejected
                  :empty (funcall buf :empty-p))))
        ;; Capacity 1 buffer (channel-like)
        (let ((buf (funcall 'neovm--test-make-buffer 1)))
          (let ((results nil))
            (dotimes (i 5)
              (funcall buf :put (* i 10))
              (let ((got (funcall buf :get)))
                (setq results (cons (car got) results))))
            (nreverse results)))
        ;; Large capacity, partial fill
        (let ((buf (funcall 'neovm--test-make-buffer 100)))
          (dotimes (i 10) (funcall buf :put i))
          (list :size (funcall buf :size)
                :full (funcall buf :full-p)
                :contents (funcall buf :contents)))
        ;; Empty buffer behavior
        (let ((buf (funcall 'neovm--test-make-buffer 5)))
          (list :empty (funcall buf :empty-p)
                :get-empty (funcall buf :get)
                :size (funcall buf :size))))
    (fmakunbound 'neovm--test-make-buffer)
    (fmakunbound 'neovm--test-run-producer-consumer)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:produced 3 :consumed (a b c) :rejected 2 :remaining 0) (:log ((x . t) (y . t) (z . t) (nil) (nil)) :rejected t :empty t) (0 10 20 30 40) (:size 10 :full nil :contents (0 1 2 3 4 5 6 7 8 9)) (:empty t :get-empty (nil) :size 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interleaved execution of multiple generators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coroutine_interleaved_generators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-make-counter-gen
    (lambda (name start step limit)
      "Generator that yields (name . value) pairs."
      (let ((current start))
        (lambda (op)
          (cond
            ((eq op :next)
             (if (< current limit)
                 (let ((val current))
                   (setq current (+ current step))
                   (cons (cons name val) t))
               (cons nil nil)))
            ((eq op :name) name))))))

  (fset 'neovm--test-interleave
    (lambda (generators)
      "Pull one value from each generator in round-robin.
       Returns interleaved list of all values."
      (let ((active (copy-sequence generators))
            (result nil))
        (while active
          (let ((next-active nil))
            (dolist (gen active)
              (let ((item (funcall gen :next)))
                (when (cdr item)
                  (setq result (cons (car item) result))
                  (setq next-active (cons gen next-active)))))
            (setq active (nreverse next-active))))
        (nreverse result))))

  (fset 'neovm--test-zip-generators
    (lambda (generators)
      "Zip generators: produce tuples until ANY generator is exhausted."
      (let ((result nil)
            (done nil))
        (while (not done)
          (let ((tuple nil)
                (all-ok t))
            (dolist (gen generators)
              (let ((item (funcall gen :next)))
                (if (cdr item)
                    (setq tuple (cons (car item) tuple))
                  (setq all-ok nil))))
            (if all-ok
                (setq result (cons (nreverse tuple) result))
              (setq done t))))
        (nreverse result))))

  (unwind-protect
      (list
        ;; Interleave three generators of different lengths
        (let ((g1 (funcall 'neovm--test-make-counter-gen 'A 0 1 3))
              (g2 (funcall 'neovm--test-make-counter-gen 'B 10 10 40))
              (g3 (funcall 'neovm--test-make-counter-gen 'C 100 100 300)))
          (funcall 'neovm--test-interleave (list g1 g2 g3)))
        ;; Interleave two same-length generators
        (let ((evens (funcall 'neovm--test-make-counter-gen 'even 0 2 10))
              (odds (funcall 'neovm--test-make-counter-gen 'odd 1 2 10)))
          (funcall 'neovm--test-interleave (list evens odds)))
        ;; Single generator
        (let ((g (funcall 'neovm--test-make-counter-gen 'solo 0 1 4)))
          (funcall 'neovm--test-interleave (list g)))
        ;; Zip generators
        (let ((g1 (funcall 'neovm--test-make-counter-gen 'x 1 1 6))
              (g2 (funcall 'neovm--test-make-counter-gen 'y 10 10 60)))
          (funcall 'neovm--test-zip-generators (list g1 g2)))
        ;; Zip stops at shortest
        (let ((short (funcall 'neovm--test-make-counter-gen 'short 0 1 2))
              (long (funcall 'neovm--test-make-counter-gen 'long 0 1 100)))
          (funcall 'neovm--test-zip-generators (list short long)))
        ;; Empty generators
        (let ((empty (funcall 'neovm--test-make-counter-gen 'e 0 1 0)))
          (funcall 'neovm--test-interleave (list empty))))
    (fmakunbound 'neovm--test-make-counter-gen)
    (fmakunbound 'neovm--test-interleave)
    (fmakunbound 'neovm--test-zip-generators)))"#;
    let expect = expect_test::expect![[
        r#""OK (((A . 0) (B . 10) (C . 100) (A . 1) (B . 20) (C . 200) (A . 2) (B . 30)) ((even . 0) (odd . 1) (even . 2) (odd . 3) (even . 4) (odd . 5) (even . 6) (odd . 7) (even . 8) (odd . 9)) ((solo . 0) (solo . 1) (solo . 2) (solo . 3)) (((x . 1) (y . 10)) ((x . 2) (y . 20)) ((x . 3) (y . 30)) ((x . 4) (y . 40)) ((x . 5) (y . 50))) (((short . 0) (long . 0)) ((short . 1) (long . 1))) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Pipeline of generators: map, filter, take, chain
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coroutine_generator_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-gen-from-list
    (lambda (lst)
      "Create a generator from a list."
      (let ((remaining (copy-sequence lst)))
        (lambda (op)
          (cond
            ((eq op :next)
             (if remaining
                 (let ((val (car remaining)))
                   (setq remaining (cdr remaining))
                   (cons val t))
               (cons nil nil))))))))

  (fset 'neovm--test-gen-map
    (lambda (fn source)
      "Create a generator that maps FN over values from SOURCE generator."
      (lambda (op)
        (cond
          ((eq op :next)
           (let ((item (funcall source :next)))
             (if (cdr item)
                 (cons (funcall fn (car item)) t)
               (cons nil nil))))))))

  (fset 'neovm--test-gen-filter
    (lambda (pred source)
      "Create a generator that yields only values matching PRED from SOURCE."
      (lambda (op)
        (cond
          ((eq op :next)
           (let ((found nil) (result nil))
             (while (not found)
               (let ((item (funcall source :next)))
                 (if (not (cdr item))
                     (progn (setq found t)
                            (setq result (cons nil nil)))
                   (when (funcall pred (car item))
                     (setq found t)
                     (setq result item)))))
             result))))))

  (fset 'neovm--test-gen-take
    (lambda (n source)
      "Create a generator that yields at most N values from SOURCE."
      (let ((count 0))
        (lambda (op)
          (cond
            ((eq op :next)
             (if (>= count n)
                 (cons nil nil)
               (setq count (1+ count))
               (funcall source :next))))))))

  (fset 'neovm--test-gen-chain
    (lambda (gen1 gen2)
      "Chain two generators: exhaust GEN1 then continue with GEN2."
      (let ((using-first t))
        (lambda (op)
          (cond
            ((eq op :next)
             (if using-first
                 (let ((item (funcall gen1 :next)))
                   (if (cdr item)
                       item
                     (setq using-first nil)
                     (funcall gen2 :next)))
               (funcall gen2 :next))))))))

  (fset 'neovm--test-gen-collect
    (lambda (gen)
      "Collect all values from a generator."
      (let ((result nil) (done nil))
        (while (not done)
          (let ((item (funcall gen :next)))
            (if (cdr item)
                (setq result (cons (car item) result))
              (setq done t))))
        (nreverse result))))

  (unwind-protect
      (list
        ;; Map: square each number
        (let* ((src (funcall 'neovm--test-gen-from-list '(1 2 3 4 5)))
               (mapped (funcall 'neovm--test-gen-map
                                (lambda (x) (* x x)) src)))
          (funcall 'neovm--test-gen-collect mapped))
        ;; Filter: only even numbers
        (let* ((src (funcall 'neovm--test-gen-from-list '(1 2 3 4 5 6 7 8)))
               (filtered (funcall 'neovm--test-gen-filter
                                  (lambda (x) (= (% x 2) 0)) src)))
          (funcall 'neovm--test-gen-collect filtered))
        ;; Take: first 3 from infinite-like source
        (let* ((src (funcall 'neovm--test-gen-from-list '(10 20 30 40 50 60)))
               (taken (funcall 'neovm--test-gen-take 3 src)))
          (funcall 'neovm--test-gen-collect taken))
        ;; Chain two generators
        (let* ((g1 (funcall 'neovm--test-gen-from-list '(a b c)))
               (g2 (funcall 'neovm--test-gen-from-list '(1 2 3)))
               (chained (funcall 'neovm--test-gen-chain g1 g2)))
          (funcall 'neovm--test-gen-collect chained))
        ;; Full pipeline: source -> filter(even) -> map(square) -> take(3)
        (let* ((src (funcall 'neovm--test-gen-from-list
                             '(1 2 3 4 5 6 7 8 9 10 11 12)))
               (evens (funcall 'neovm--test-gen-filter
                               (lambda (x) (= (% x 2) 0)) src))
               (squared (funcall 'neovm--test-gen-map
                                 (lambda (x) (* x x)) evens))
               (first3 (funcall 'neovm--test-gen-take 3 squared)))
          (funcall 'neovm--test-gen-collect first3))
        ;; Pipeline: chain -> map -> filter
        (let* ((g1 (funcall 'neovm--test-gen-from-list '(1 3 5)))
               (g2 (funcall 'neovm--test-gen-from-list '(2 4 6)))
               (chained (funcall 'neovm--test-gen-chain g1 g2))
               (doubled (funcall 'neovm--test-gen-map
                                 (lambda (x) (* x 2)) chained))
               (big (funcall 'neovm--test-gen-filter
                             (lambda (x) (> x 5)) doubled)))
          (funcall 'neovm--test-gen-collect big))
        ;; Empty source through pipeline
        (let* ((src (funcall 'neovm--test-gen-from-list nil))
               (mapped (funcall 'neovm--test-gen-map #'1+ src))
               (filtered (funcall 'neovm--test-gen-filter #'oddp mapped)))
          (funcall 'neovm--test-gen-collect filtered)))
    (fmakunbound 'neovm--test-gen-from-list)
    (fmakunbound 'neovm--test-gen-map)
    (fmakunbound 'neovm--test-gen-filter)
    (fmakunbound 'neovm--test-gen-take)
    (fmakunbound 'neovm--test-gen-chain)
    (fmakunbound 'neovm--test-gen-collect)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 4 9 16 25) (2 4 6 8) (10 20 30) (a b c 1 2 3) (4 16 36) (6 10 8 12) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// State machine coroutine: multi-phase computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coroutine_state_machine() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-make-state-machine
    (lambda (transitions initial-state)
      "Create a state machine coroutine.
       TRANSITIONS is alist of (state . handler-fn).
       Each handler receives input and returns (next-state . output).
       Machine yields output on each step."
      (let ((current-state initial-state)
            (history nil))
        (lambda (op &rest args)
          (cond
            ((eq op :step)
             (let* ((input (car args))
                    (handler (cdr (assq current-state transitions)))
                    (result (if handler
                                (funcall handler input)
                              (cons current-state (list :error "no handler")))))
               (setq history (cons (list current-state input (cdr result)) history))
               (setq current-state (car result))
               (cdr result)))
            ((eq op :state) current-state)
            ((eq op :history) (nreverse history)))))))

  (unwind-protect
      (list
        ;; Vending machine state machine
        (let* ((transitions
                (list
                 (cons 'idle
                       (lambda (input)
                         (cond
                           ((eq input 'coin)
                            (cons 'has-coin '(:msg "Coin inserted")))
                           (t (cons 'idle '(:msg "Insert coin first"))))))
                 (cons 'has-coin
                       (lambda (input)
                         (cond
                           ((eq input 'select)
                            (cons 'dispensing '(:msg "Dispensing item")))
                           ((eq input 'refund)
                            (cons 'idle '(:msg "Coin refunded")))
                           (t (cons 'has-coin '(:msg "Select item or refund"))))))
                 (cons 'dispensing
                       (lambda (_input)
                         (cons 'idle '(:msg "Please take item"))))))
               (vm (funcall 'neovm--test-make-state-machine transitions 'idle)))
          (list
            (funcall vm :step 'select)   ;; "Insert coin first"
            (funcall vm :state)           ;; idle
            (funcall vm :step 'coin)     ;; "Coin inserted"
            (funcall vm :state)           ;; has-coin
            (funcall vm :step 'select)   ;; "Dispensing item"
            (funcall vm :state)           ;; dispensing
            (funcall vm :step nil)       ;; "Please take item"
            (funcall vm :state)           ;; idle
            ;; Second transaction with refund
            (funcall vm :step 'coin)
            (funcall vm :step 'refund)
            (funcall vm :state)
            (funcall vm :history)))
        ;; Traffic light state machine
        (let* ((transitions
                (list
                 (cons 'red
                       (lambda (_) (cons 'green '(:light green :duration 30))))
                 (cons 'green
                       (lambda (_) (cons 'yellow '(:light yellow :duration 5))))
                 (cons 'yellow
                       (lambda (_) (cons 'red '(:light red :duration 20))))))
               (light (funcall 'neovm--test-make-state-machine transitions 'red)))
          ;; Run through 6 transitions
          (let ((outputs nil))
            (dotimes (_ 6)
              (setq outputs (cons (funcall light :step nil) outputs)))
            (list (nreverse outputs)
                  (funcall light :state)))))
    (fmakunbound 'neovm--test-make-state-machine)))"#;
    let expect = expect_test::expect![[
        r#""OK (((:msg \"Insert coin first\") idle (:msg \"Coin inserted\") has-coin (:msg \"Dispensing item\") dispensing (:msg \"Please take item\") idle (:msg \"Coin inserted\") (:msg \"Coin refunded\") idle ((idle select (:msg \"Insert coin first\")) (idle coin (:msg \"Coin inserted\")) (has-coin select (:msg \"Dispensing item\")) (dispensing nil (:msg \"Please take item\")) (idle coin (:msg \"Coin inserted\")) (has-coin refund (:msg \"Coin refunded\")))) (((:light green :duration 30) (:light yellow :duration 5) (:light red :duration 20) (:light green :duration 30) (:light yellow :duration 5) (:light red :duration 20)) red))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Generator-based accumulator with scan and reduce
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coroutine_gen_accumulator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-gen-from-list
    (lambda (lst)
      (let ((remaining (copy-sequence lst)))
        (lambda (op)
          (if (eq op :next)
              (if remaining
                  (let ((val (car remaining)))
                    (setq remaining (cdr remaining))
                    (cons val t))
                (cons nil nil)))))))

  (fset 'neovm--test-gen-scan
    (lambda (fn init source)
      "Generator that yields running accumulation:
       init, fn(init,x1), fn(fn(init,x1),x2), ..."
      (let ((acc init)
            (yielded-init nil))
        (lambda (op)
          (if (eq op :next)
              (if (not yielded-init)
                  (progn (setq yielded-init t)
                         (cons acc t))
                (let ((item (funcall source :next)))
                  (if (cdr item)
                      (progn
                        (setq acc (funcall fn acc (car item)))
                        (cons acc t))
                    (cons nil nil)))))))))

  (fset 'neovm--test-gen-reduce
    (lambda (fn init source)
      "Consume entire generator, reducing with FN from INIT."
      (let ((acc init)
            (done nil))
        (while (not done)
          (let ((item (funcall source :next)))
            (if (cdr item)
                (setq acc (funcall fn acc (car item)))
              (setq done t))))
        acc)))

  (fset 'neovm--test-gen-collect
    (lambda (gen)
      (let ((result nil) (done nil))
        (while (not done)
          (let ((item (funcall gen :next)))
            (if (cdr item)
                (setq result (cons (car item) result))
              (setq done t))))
        (nreverse result))))

  (fset 'neovm--test-gen-enumerate
    (lambda (source &optional start)
      "Yield (index . value) pairs from SOURCE."
      (let ((idx (or start 0)))
        (lambda (op)
          (if (eq op :next)
              (let ((item (funcall source :next)))
                (if (cdr item)
                    (let ((pair (cons idx (car item))))
                      (setq idx (1+ idx))
                      (cons pair t))
                  (cons nil nil))))))))

  (unwind-protect
      (list
        ;; Scan: running sum
        (let* ((src (funcall 'neovm--test-gen-from-list '(1 2 3 4 5)))
               (running-sum (funcall 'neovm--test-gen-scan #'+ 0 src)))
          (funcall 'neovm--test-gen-collect running-sum))
        ;; Scan: running product
        (let* ((src (funcall 'neovm--test-gen-from-list '(1 2 3 4 5)))
               (running-prod (funcall 'neovm--test-gen-scan #'* 1 src)))
          (funcall 'neovm--test-gen-collect running-prod))
        ;; Scan: running max
        (let* ((src (funcall 'neovm--test-gen-from-list '(3 1 4 1 5 9 2 6)))
               (running-max (funcall 'neovm--test-gen-scan #'max 0 src)))
          (funcall 'neovm--test-gen-collect running-max))
        ;; Reduce: sum
        (let ((src (funcall 'neovm--test-gen-from-list '(1 2 3 4 5))))
          (funcall 'neovm--test-gen-reduce #'+ 0 src))
        ;; Reduce: concatenate strings
        (let ((src (funcall 'neovm--test-gen-from-list '("hello" " " "world"))))
          (funcall 'neovm--test-gen-reduce #'concat "" src))
        ;; Enumerate
        (let* ((src (funcall 'neovm--test-gen-from-list '(a b c d)))
               (enumerated (funcall 'neovm--test-gen-enumerate src)))
          (funcall 'neovm--test-gen-collect enumerated))
        ;; Enumerate with custom start
        (let* ((src (funcall 'neovm--test-gen-from-list '(x y z)))
               (enumerated (funcall 'neovm--test-gen-enumerate src 10)))
          (funcall 'neovm--test-gen-collect enumerated))
        ;; Reduce empty generator
        (let ((src (funcall 'neovm--test-gen-from-list nil)))
          (funcall 'neovm--test-gen-reduce #'+ 0 src)))
    (fmakunbound 'neovm--test-gen-from-list)
    (fmakunbound 'neovm--test-gen-scan)
    (fmakunbound 'neovm--test-gen-reduce)
    (fmakunbound 'neovm--test-gen-collect)
    (fmakunbound 'neovm--test-gen-enumerate)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 1 3 6 10 15) (1 1 2 6 24 120) (0 3 3 4 4 5 9 9 9) 15 \"hello world\" ((0 . a) (1 . b) (2 . c) (3 . d)) ((10 . x) (11 . y) (12 . z)) 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
