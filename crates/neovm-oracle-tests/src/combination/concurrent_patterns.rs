//! Complex oracle parity tests simulating concurrent/cooperative processing
//! patterns in Elisp: generator/iterator protocols, cooperative multitasking
//! with catch/throw, producer-consumer with buffer-as-queue, coroutine
//! simulation with continuation passing, event loop dispatch, and
//! pipeline transformers.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Generator/iterator protocol using closures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_concurrent_generator_protocol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A generator that yields Fibonacci numbers on demand, with reset capability
    let form = r#"(let ((make-fib-gen
           (lambda ()
             (let ((a 0) (b 1) (done nil))
               (list
                ;; next: return current value and advance
                (lambda ()
                  (if done 'exhausted
                    (let ((val a))
                      (let ((next (+ a b)))
                        (setq a b b next))
                      val)))
                ;; take-n: collect n values
                (lambda (n)
                  (let ((result nil) (i 0))
                    (while (< i n)
                      (let ((val a)
                            (next (+ a b)))
                        (setq result (cons val result))
                        (setq a b b next))
                      (setq i (1+ i)))
                    (nreverse result)))
                ;; reset
                (lambda ()
                  (setq a 0 b 1 done nil)))))))
  ;; Test the generator
  (let* ((gen (funcall make-fib-gen))
         (next-fn (nth 0 gen))
         (take-fn (nth 1 gen))
         (reset-fn (nth 2 gen)))
    ;; Take first 8 Fibonacci numbers
    (let ((first-8 (funcall take-fn 8)))
      ;; Continuing from where we left off, next few
      (let ((next1 (funcall next-fn))
            (next2 (funcall next-fn)))
        ;; Reset and take again
        (funcall reset-fn)
        (let ((after-reset (funcall take-fn 5)))
          (list first-8 next1 next2 after-reset))))))"#;
    let expect = expect_test::expect![[r#""OK ((0 1 1 2 3 5 8 13) 21 34 (0 1 1 2 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cooperative multitasking with catch/throw round-robin
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_concurrent_round_robin_scheduler() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate round-robin scheduling: each "task" runs for a slice then yields
    let form = r#"(let ((tasks nil)
      (log nil)
      (make-task nil)
      (scheduler nil))
  ;; A task is a closure over (name, counter, max)
  ;; It "runs" by incrementing counter and yielding after each step
  (setq make-task
    (lambda (name max)
      (let ((counter 0))
        (lambda ()
          (if (>= counter max)
              'done
            (setq counter (1+ counter))
            (list name counter))))))
  ;; Scheduler: round-robin until all tasks are done
  (setq scheduler
    (lambda (task-list)
      (let ((active (copy-sequence task-list))
            (output nil)
            (rounds 0))
        (while (and active (< rounds 50))
          (setq rounds (1+ rounds))
          (let ((remaining nil))
            (dolist (task active)
              (let ((result (funcall task)))
                (if (eq result 'done)
                    nil  ;; drop completed task
                  (setq output (cons result output))
                  (setq remaining (cons task remaining)))))
            (setq active (nreverse remaining))))
        (list rounds (nreverse output)))))
  ;; Create 3 tasks with different workloads
  (let ((t1 (funcall make-task 'A 3))
        (t2 (funcall make-task 'B 2))
        (t3 (funcall make-task 'C 4)))
    (funcall scheduler (list t1 t2 t3))))"#;
    let expect = expect_test::expect![[
        r#""OK (5 ((A 1) (B 1) (C 1) (A 2) (B 2) (C 2) (A 3) (C 3) (C 4)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Producer-consumer with list as queue
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_concurrent_producer_consumer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Producer generates items, consumer processes them, using a shared queue
    let form = r#"(let ((queue nil)
      (processed nil)
      (make-producer nil)
      (make-consumer nil))
  ;; Producer: push items onto queue
  (setq make-producer
    (lambda (items)
      (lambda ()
        (if (null items)
            'producer-done
          (let ((item (car items)))
            (setq items (cdr items))
            (setq queue (append queue (list item)))
            (list 'produced item))))))
  ;; Consumer: pop from queue and transform
  (setq make-consumer
    (lambda (transform-fn)
      (lambda ()
        (if (null queue)
            'consumer-idle
          (let ((item (car queue)))
            (setq queue (cdr queue))
            (let ((result (funcall transform-fn item)))
              (setq processed (cons result processed))
              (list 'consumed item '-> result)))))))
  ;; Run: alternate producer and consumer steps
  (let ((producer (funcall make-producer '(1 2 3 4 5)))
        (consumer (funcall make-consumer (lambda (x) (* x x))))
        (log nil)
        (steps 0))
    ;; Interleave: produce 2, consume 1, produce 1, consume 2, etc.
    (let ((p-result nil) (c-result nil))
      ;; Produce 2
      (setq p-result (funcall producer))
      (setq log (cons p-result log))
      (setq p-result (funcall producer))
      (setq log (cons p-result log))
      ;; Consume 1
      (setq c-result (funcall consumer))
      (setq log (cons c-result log))
      ;; Produce rest
      (while (not (eq (setq p-result (funcall producer)) 'producer-done))
        (setq log (cons p-result log)))
      ;; Consume rest
      (while (not (eq (setq c-result (funcall consumer)) 'consumer-idle))
        (setq log (cons c-result log)))
      (list (nreverse processed)
            (length (nreverse log))
            (null queue)))))"#;
    let expect = expect_test::expect![[r#""OK ((1 4 9 16 25) 10 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Coroutine simulation with continuation passing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_concurrent_coroutine_cps() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate coroutines via CPS: each step takes a continuation
    let form = r#"(progn
  (defvar neovm--cps-log nil)
  (fset 'neovm--cps-step1
    (lambda (data k)
      (setq neovm--cps-log (cons (list 'step1 data) neovm--cps-log))
      (funcall k (* data 2))))
  (fset 'neovm--cps-step2
    (lambda (data k)
      (setq neovm--cps-log (cons (list 'step2 data) neovm--cps-log))
      (funcall k (+ data 10))))
  (fset 'neovm--cps-step3
    (lambda (data k)
      (setq neovm--cps-log (cons (list 'step3 data) neovm--cps-log))
      (funcall k (list 'final data))))
  ;; Chain the coroutine steps
  (fset 'neovm--cps-pipeline
    (lambda (initial)
      (setq neovm--cps-log nil)
      (funcall 'neovm--cps-step1 initial
        (lambda (r1)
          (funcall 'neovm--cps-step2 r1
            (lambda (r2)
              (funcall 'neovm--cps-step3 r2
                #'identity)))))))
  (unwind-protect
      (list
       (funcall 'neovm--cps-pipeline 5)
       (nreverse neovm--cps-log)
       (progn
         (setq neovm--cps-log nil)
         (funcall 'neovm--cps-pipeline 0))
       (nreverse neovm--cps-log)
       (progn
         (setq neovm--cps-log nil)
         (funcall 'neovm--cps-pipeline 100))
       (nreverse neovm--cps-log))
    (fmakunbound 'neovm--cps-step1)
    (fmakunbound 'neovm--cps-step2)
    (fmakunbound 'neovm--cps-step3)
    (fmakunbound 'neovm--cps-pipeline)
    (makunbound 'neovm--cps-log)))"#;
    let expect = expect_test::expect![[
        r#""OK ((final 20) ((step1 5) (step2 10) (step3 20)) (final 10) ((step1 0) (step2 0) (step3 10)) (final 210) ((step1 100) (step2 200) (step3 210)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Event loop with timer-based dispatch
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_concurrent_event_loop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate an event loop: events scheduled at specific ticks, dispatched in order
    let form = r#"(let ((events nil)
      (handlers (make-hash-table :test 'eq))
      (log nil))
  ;; Register event handlers
  (puthash 'tick
    (lambda (data) (list 'tick-handled data))
    handlers)
  (puthash 'message
    (lambda (data) (list 'msg-handled (upcase data)))
    handlers)
  (puthash 'compute
    (lambda (data) (list 'computed (* data data)))
    handlers)
  ;; Schedule events: (tick . (type . data))
  (setq events
    (list
     '(0 . (tick . 0))
     '(1 . (message . "hello"))
     '(2 . (compute . 7))
     '(3 . (tick . 3))
     '(4 . (message . "world"))
     '(5 . (compute . 12))
     '(7 . (tick . 7))
     '(10 . (message . "done"))))
  ;; Run event loop for 12 ticks
  (let ((tick 0)
        (remaining events))
    (while (and (<= tick 12) remaining)
      ;; Process all events scheduled for this tick
      (while (and remaining (= (caar remaining) tick))
        (let* ((event (cdar remaining))
               (type (car event))
               (data (cdr event))
               (handler (gethash type handlers)))
          (when handler
            (setq log (cons (list tick (funcall handler data)) log))))
        (setq remaining (cdr remaining)))
      (setq tick (1+ tick)))
    (list (length (nreverse log))
          ;; Return the log in chronological order
          (nreverse log))))"#;
    let expect = expect_test::expect![[r#""OK (8 ((10 (msg-handled \"DONE\"))))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Pipeline of transformers with intermediate buffers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_concurrent_pipeline_transformers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Chain of transform stages, each with its own buffer, push-based
    let form = r#"(let ((make-stage nil)
      (connect-stages nil))
  ;; A stage has: input buffer, transform fn, output buffer (shared with next stage)
  (setq make-stage
    (lambda (name transform-fn)
      (let ((input-buf nil)
            (output-buf nil)
            (processed 0))
        (list
         ;; push: add item to input buffer
         (lambda (item) (setq input-buf (append input-buf (list item))))
         ;; process: transform all buffered items, push to output
         (lambda ()
           (while input-buf
             (let* ((item (car input-buf))
                    (result (funcall transform-fn item)))
               (setq input-buf (cdr input-buf))
               (setq processed (1+ processed))
               (when output-buf
                 (funcall output-buf result)))))
         ;; set-output: connect to next stage's push function
         (lambda (out-fn) (setq output-buf out-fn))
         ;; stats
         (lambda () (list name processed))))))
  ;; Build a 3-stage pipeline: double -> add-10 -> to-string
  (let* ((s1 (funcall make-stage 'double (lambda (x) (* x 2))))
         (s2 (funcall make-stage 'add10 (lambda (x) (+ x 10))))
         (s3-results nil)
         (s3 (funcall make-stage 'stringify
               (lambda (x)
                 (let ((s (number-to-string x)))
                   (setq s3-results (cons s s3-results))
                   s)))))
    ;; Connect: s1 -> s2 -> s3
    (funcall (nth 2 s1) (nth 0 s2))
    (funcall (nth 2 s2) (nth 0 s3))
    ;; Push items into stage 1
    (dolist (item '(1 2 3 4 5))
      (funcall (nth 0 s1) item))
    ;; Process each stage
    (funcall (nth 1 s1))
    (funcall (nth 1 s2))
    (funcall (nth 1 s3))
    (list
     (nreverse s3-results)
     (funcall (nth 3 s1))
     (funcall (nth 3 s2))
     (funcall (nth 3 s3)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"12\" \"14\" \"16\" \"18\" \"20\") (double 5) (add10 5) (stringify 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: actor-model message passing simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_concurrent_actor_model() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Actors with mailboxes, processing messages in order
    let form = r#"(let ((make-actor nil)
      (send nil)
      (process-all nil))
  ;; Actor: name, behavior-fn, mailbox
  (setq make-actor
    (lambda (name behavior)
      (let ((mailbox nil)
            (state nil))
        (list
         name
         ;; receive: add to mailbox
         (lambda (msg) (setq mailbox (append mailbox (list msg))))
         ;; process: handle all messages
         (lambda ()
           (let ((results nil))
             (while mailbox
               (let ((msg (car mailbox)))
                 (setq mailbox (cdr mailbox))
                 (let ((result (funcall behavior msg state)))
                   (setq state (car result))
                   (setq results (cons (cdr result) results)))))
             (nreverse results)))
         ;; get-state
         (lambda () state)))))
  ;; Counter actor: state is a number, messages are :inc/:dec/:get
  (let ((counter (funcall make-actor 'counter
          (lambda (msg state)
            (let ((st (or state 0)))
              (cond
               ((eq msg 'inc) (cons (1+ st) (list 'incremented (1+ st))))
               ((eq msg 'dec) (cons (1- st) (list 'decremented (1- st))))
               ((eq msg 'get) (cons st (list 'value st)))
               (t (cons st (list 'unknown msg)))))))))
    (let ((receive-fn (nth 1 counter))
          (process-fn (nth 2 counter))
          (state-fn (nth 3 counter)))
      ;; Send messages
      (funcall receive-fn 'inc)
      (funcall receive-fn 'inc)
      (funcall receive-fn 'inc)
      (funcall receive-fn 'dec)
      (funcall receive-fn 'get)
      ;; Process all messages
      (let ((results (funcall process-fn)))
        (list results
              (funcall state-fn)
              ;; Send more messages after processing
              (progn
                (funcall receive-fn 'inc)
                (funcall receive-fn 'inc)
                (funcall process-fn))
              (funcall state-fn))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((incremented 1) (incremented 2) (incremented 3) (decremented 2) (value 2)) 2 ((incremented 3) (incremented 4)) 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: work-stealing deque simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_concurrent_work_stealing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate work-stealing: workers with local deques, stealing from others when idle
    let form = r#"(let ((make-worker nil)
      (log nil))
  (setq make-worker
    (lambda (name)
      (let ((deque nil)
            (completed nil))
        (list
         name
         ;; push-work: add to front of deque
         (lambda (work) (setq deque (cons work deque)))
         ;; pop-work: take from front (LIFO for owner)
         (lambda ()
           (if (null deque) nil
             (let ((w (car deque)))
               (setq deque (cdr deque))
               w)))
         ;; steal-work: take from back (FIFO for thief)
         (lambda ()
           (if (null deque) nil
             (let ((w (car (last deque))))
               (setq deque (butlast deque))
               w)))
         ;; do-work: process a work item
         (lambda (item)
           (setq completed (cons (list name item (* item item)) completed))
           (setq log (cons (list name 'did item) log)))
         ;; get-completed
         (lambda () (nreverse completed))
         ;; deque-size
         (lambda () (length deque))))))
  ;; Create 2 workers
  (let* ((w1 (funcall make-worker 'W1))
         (w2 (funcall make-worker 'W2))
         (push1 (nth 1 w1)) (pop1 (nth 2 w1)) (steal1 (nth 3 w1))
         (do1 (nth 4 w1)) (completed1 (nth 5 w1)) (size1 (nth 6 w1))
         (push2 (nth 1 w2)) (pop2 (nth 2 w2)) (steal2 (nth 3 w2))
         (do2 (nth 4 w2)) (completed2 (nth 5 w2)) (size2 (nth 6 w2)))
    ;; Load W1 with work items 1-8
    (dolist (i '(1 2 3 4 5 6 7 8))
      (funcall push1 i))
    ;; W1 does 2 items from its own deque
    (funcall do1 (funcall pop1))
    (funcall do1 (funcall pop1))
    ;; W2 steals 3 items from W1 (from the back)
    (funcall do2 (funcall steal1))
    (funcall do2 (funcall steal1))
    (funcall do2 (funcall steal1))
    ;; W1 does the rest of its items
    (let ((item nil))
      (while (setq item (funcall pop1))
        (funcall do1 item)))
    (list
     (funcall completed1)
     (funcall completed2)
     (funcall size1)
     (funcall size2)
     (length (nreverse log)))))"#;
    let expect = expect_test::expect![[
        r#""OK (((W1 8 64) (W1 7 49) (W1 6 36) (W1 5 25) (W1 4 16)) ((W2 1 1) (W2 2 4) (W2 3 9)) 0 0 8)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
