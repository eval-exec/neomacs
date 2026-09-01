//! Oracle parity tests for OS scheduler simulation in pure Elisp.
//!
//! Simulates a multi-level feedback queue (MLFQ) with 4 priority levels,
//! priority aging, I/O burst handling, context switch overhead tracking,
//! CPU utilization calculation, starvation detection, and fair-share
//! scheduling between process groups.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Multi-level feedback queue with 4 priority levels
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_os_scheduler_mlfq_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Processes start at highest priority (queue 0). If they use their
    // entire time quantum they drop to the next lower queue. I/O-bound
    // processes that yield early stay at their current level.
    let form = r#"(progn (require (quote cl-lib))
(progn
  ;; Process: (pid name queue cpu-used io-waiting burst-remaining time-quantum-used)
  (fset 'neovm--sched-make-proc
    (lambda (pid name burst)
      (list pid name 0 0 nil burst 0)))

  (fset 'neovm--sched-pid (lambda (p) (nth 0 p)))
  (fset 'neovm--sched-name (lambda (p) (nth 1 p)))
  (fset 'neovm--sched-queue (lambda (p) (nth 2 p)))
  (fset 'neovm--sched-cpu (lambda (p) (nth 3 p)))
  (fset 'neovm--sched-io (lambda (p) (nth 4 p)))
  (fset 'neovm--sched-burst (lambda (p) (nth 5 p)))
  (fset 'neovm--sched-tq-used (lambda (p) (nth 6 p)))

  ;; Time quantums per queue level: 2, 4, 8, 16
  (fset 'neovm--sched-quantum
    (lambda (level) (nth level '(2 4 8 16))))

  ;; Run a process for one time unit. Returns updated process.
  (fset 'neovm--sched-tick
    (lambda (proc)
      (let* ((queue (funcall 'neovm--sched-queue proc))
             (cpu (1+ (funcall 'neovm--sched-cpu proc)))
             (burst (1- (funcall 'neovm--sched-burst proc)))
             (tq (1+ (funcall 'neovm--sched-tq-used proc)))
             (quantum (funcall 'neovm--sched-quantum queue))
             ;; Demote if used full quantum and burst remaining
             (new-queue (if (and (>= tq quantum) (> burst 0))
                            (min 3 (1+ queue))
                          queue))
             (new-tq (if (>= tq quantum) 0 tq)))
        (list (funcall 'neovm--sched-pid proc)
              (funcall 'neovm--sched-name proc)
              new-queue cpu nil burst new-tq))))

  ;; Run a process to completion, tracking queue transitions
  (fset 'neovm--sched-run-proc
    (lambda (proc)
      (let ((history nil)
            (p proc))
        (while (> (funcall 'neovm--sched-burst p) 0)
          (push (list (funcall 'neovm--sched-pid p)
                      (funcall 'neovm--sched-queue p)
                      (funcall 'neovm--sched-burst p))
                history)
          (setq p (funcall 'neovm--sched-tick p)))
        (list (nreverse history)
              (funcall 'neovm--sched-queue p)
              (funcall 'neovm--sched-cpu p)))))

  (unwind-protect
      (let* (;; Process with burst=3: starts at Q0 (quantum=2),
             ;; uses 2 ticks in Q0 then demoted to Q1
             (p1 (funcall 'neovm--sched-make-proc 1 'short 3))
             (r1 (funcall 'neovm--sched-run-proc p1))
             ;; Process with burst=7: Q0(2)->Q1(4)->Q2(1)
             (p2 (funcall 'neovm--sched-make-proc 2 'medium 7))
             (r2 (funcall 'neovm--sched-run-proc p2))
             ;; Process with burst=1: finishes in Q0
             (p3 (funcall 'neovm--sched-make-proc 3 'tiny 1))
             (r3 (funcall 'neovm--sched-run-proc p3)))
        (list
          ;; p1: 3 ticks total, should reach Q1
          (nth 1 r1) (nth 2 r1)
          ;; p2: 7 ticks total, should reach Q2
          (nth 1 r2) (nth 2 r2)
          ;; p3: 1 tick, stays at Q0
          (nth 1 r3) (nth 2 r3)
          ;; History lengths match burst sizes
          (length (nth 0 r1))
          (length (nth 0 r2))
          (length (nth 0 r3))))

    (fmakunbound 'neovm--sched-make-proc)
    (fmakunbound 'neovm--sched-pid)
    (fmakunbound 'neovm--sched-name)
    (fmakunbound 'neovm--sched-queue)
    (fmakunbound 'neovm--sched-cpu)
    (fmakunbound 'neovm--sched-io)
    (fmakunbound 'neovm--sched-burst)
    (fmakunbound 'neovm--sched-tq-used)
    (fmakunbound 'neovm--sched-quantum)
    (fmakunbound 'neovm--sched-tick)
    (fmakunbound 'neovm--sched-run-proc))))"#;
    let expect = expect_test::expect![[r#""OK (1 3 2 7 0 1 3 7 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Priority aging: boost starved processes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_os_scheduler_priority_aging() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Processes that wait too long at low priority get boosted back up
    let form = r#"(progn (require (quote cl-lib))
(progn
  ;; Process: (pid queue wait-ticks age-threshold)
  ;; age-threshold: after this many wait ticks, promote by 1 level
  (fset 'neovm--age-make (lambda (pid queue) (list pid queue 0)))
  (fset 'neovm--age-pid (lambda (p) (nth 0 p)))
  (fset 'neovm--age-queue (lambda (p) (nth 1 p)))
  (fset 'neovm--age-wait (lambda (p) (nth 2 p)))

  (fset 'neovm--age-tick-wait
    (lambda (proc threshold)
      "Increment wait counter. If exceeds threshold, promote."
      (let* ((pid (funcall 'neovm--age-pid proc))
             (queue (funcall 'neovm--age-queue proc))
             (wait (1+ (funcall 'neovm--age-wait proc))))
        (if (>= wait threshold)
            ;; Promote: decrease queue level (higher priority), reset wait
            (list pid (max 0 (1- queue)) 0)
          (list pid queue wait)))))

  (fset 'neovm--age-simulate
    (lambda (procs ticks threshold)
      "Simulate TICKS waiting ticks on all PROCS with aging."
      (let ((current procs)
            (tick 0))
        (while (< tick ticks)
          (setq current
                (mapcar (lambda (p) (funcall 'neovm--age-tick-wait p threshold))
                        current))
          (setq tick (1+ tick)))
        current)))

  (unwind-protect
      (let* ((threshold 5)
             ;; 3 processes at queue levels 1, 2, 3
             (procs (list (funcall 'neovm--age-make 1 1)
                          (funcall 'neovm--age-make 2 2)
                          (funcall 'neovm--age-make 3 3)))
             ;; After 5 ticks: each promoted by 1
             (after5 (funcall 'neovm--age-simulate procs 5 threshold))
             ;; After 10 ticks: each promoted by 2
             (after10 (funcall 'neovm--age-simulate procs 10 threshold))
             ;; After 15 ticks: all at queue 0 (clamped)
             (after15 (funcall 'neovm--age-simulate procs 15 threshold)))
        (list
          ;; After 5 ticks: queues are 0, 1, 2
          (mapcar 'neovm--age-queue after5)
          ;; After 10 ticks: queues are 0, 0, 1
          (mapcar 'neovm--age-queue after10)
          ;; After 15 ticks: all at 0
          (mapcar 'neovm--age-queue after15)
          ;; Wait counters reset after each promotion
          (mapcar 'neovm--age-wait after5)))

    (fmakunbound 'neovm--age-make)
    (fmakunbound 'neovm--age-pid)
    (fmakunbound 'neovm--age-queue)
    (fmakunbound 'neovm--age-wait)
    (fmakunbound 'neovm--age-tick-wait)
    (fmakunbound 'neovm--age-simulate))))"#;
    let expect = expect_test::expect![[r#""OK ((0 1 2) (0 0 1) (0 0 0) (0 0 0))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// I/O burst handling: processes yielding for I/O
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_os_scheduler_io_burst() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // I/O-bound processes voluntarily yield before their quantum expires.
    // They should keep their current priority level (not be demoted).
    let form = r#"(progn (require (quote cl-lib))
(progn
  ;; Process: (pid queue cpu-total bursts) where bursts = list of (type . duration)
  ;; type: cpu or io
  (fset 'neovm--iob-simulate
    (lambda (bursts)
      "Simulate a process with alternating CPU/IO bursts.
       Returns (final-queue total-cpu-time io-waits queue-history)."
      (let ((queue 0)
            (cpu-total 0)
            (io-waits 0)
            (history nil)
            (quantums '(2 4 8 16)))
        (dolist (burst bursts)
          (let ((type (car burst))
                (duration (cdr burst)))
            (cond
              ((eq type 'cpu)
               (let ((remaining duration)
                     (tq-used 0))
                 (while (> remaining 0)
                   (push queue history)
                   (setq tq-used (1+ tq-used))
                   (setq cpu-total (1+ cpu-total))
                   (setq remaining (1- remaining))
                   ;; Check if quantum exhausted
                   (when (and (>= tq-used (nth queue quantums))
                              (> remaining 0))
                     ;; Demote
                     (setq queue (min 3 (1+ queue)))
                     (setq tq-used 0)))))
              ((eq type 'io)
               ;; I/O burst: process yields, keeps priority
               (setq io-waits (1+ io-waits))))))
        (list queue cpu-total io-waits (nreverse history)))))

  (unwind-protect
      (let* (;; CPU-bound: 10 CPU ticks straight
             (cpu-bound (funcall 'neovm--iob-simulate
                          '((cpu . 10))))
             ;; I/O-bound: short CPU bursts interleaved with I/O
             (io-bound (funcall 'neovm--iob-simulate
                         '((cpu . 1) (io . 5) (cpu . 1) (io . 3)
                           (cpu . 1) (io . 2) (cpu . 1))))
             ;; Mixed: CPU burst then I/O then more CPU
             (mixed (funcall 'neovm--iob-simulate
                      '((cpu . 3) (io . 4) (cpu . 5)))))
        (list
          ;; CPU-bound: demoted to lower queue
          (nth 0 cpu-bound)  ;; final queue > 0
          (nth 1 cpu-bound)  ;; total cpu = 10
          ;; I/O-bound: stays at high priority (each CPU burst < quantum)
          (nth 0 io-bound)   ;; final queue = 0
          (nth 1 io-bound)   ;; total cpu = 4
          (nth 2 io-bound)   ;; io-waits = 3
          ;; Mixed
          (nth 0 mixed)
          (nth 1 mixed)))

    (fmakunbound 'neovm--iob-simulate))))"#;
    let expect = expect_test::expect![[r#""OK (2 10 0 4 3 2 8)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Context switch overhead tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_os_scheduler_context_switch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Track the overhead of context switches in a round-robin scheduler
    let form = r#"(progn (require (quote cl-lib))
(progn
  (fset 'neovm--csw-round-robin
    (lambda (processes quantum switch-cost)
      "Run round-robin scheduling. Each process is (pid . remaining-burst).
       SWITCH-COST is ticks lost per context switch.
       Returns (total-time useful-time switch-count completion-order)."
      (let ((queue (mapcar (lambda (p) (cons (car p) (cdr p))) processes))
            (total-time 0)
            (useful-time 0)
            (switch-count 0)
            (completion-order nil)
            (last-pid nil))
        (while queue
          (let* ((proc (car queue))
                 (pid (car proc))
                 (remaining (cdr proc)))
            (setq queue (cdr queue))
            ;; Context switch if different process
            (when (and last-pid (not (= last-pid pid)))
              (setq total-time (+ total-time switch-cost))
              (setq switch-count (1+ switch-count)))
            (setq last-pid pid)
            ;; Run for min(quantum, remaining)
            (let ((run-time (min quantum remaining)))
              (setq total-time (+ total-time run-time))
              (setq useful-time (+ useful-time run-time))
              (setq remaining (- remaining run-time))
              (if (> remaining 0)
                  ;; Not done, re-enqueue
                  (setq queue (append queue (list (cons pid remaining))))
                ;; Done
                (push pid completion-order)))))
        (list total-time useful-time switch-count (nreverse completion-order)))))

  (unwind-protect
      (let* (;; 3 processes: bursts 4, 6, 2; quantum=3, switch-cost=1
             (procs '((1 . 4) (2 . 6) (3 . 2)))
             (result (funcall 'neovm--csw-round-robin procs 3 1))
             ;; Same with no switch cost
             (result-no-cost (funcall 'neovm--csw-round-robin procs 3 0))
             ;; Large quantum (no preemption)
             (result-large-q (funcall 'neovm--csw-round-robin procs 100 1)))
        (list
          ;; With switch cost
          (nth 0 result)   ;; total time
          (nth 1 result)   ;; useful time = sum of bursts = 12
          (nth 2 result)   ;; switch count
          (nth 3 result)   ;; completion order
          ;; Without switch cost: useful = total
          (= (nth 0 result-no-cost) (nth 1 result-no-cost))
          (nth 3 result-no-cost)
          ;; Large quantum: FCFS behavior, minimal switches
          (nth 2 result-large-q)
          (nth 3 result-large-q)))

    (fmakunbound 'neovm--csw-round-robin))))"#;
    let expect = expect_test::expect![[r#""OK (16 12 4 (3 1 2) t (3 1 2) 2 (1 2 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// CPU utilization tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_os_scheduler_cpu_utilization() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Track CPU busy vs idle time over a scheduling window
    let form = r#"(progn (require (quote cl-lib))
(progn
  (fset 'neovm--util-schedule
    (lambda (arrivals total-time quantum)
      "Schedule processes with arrival times.
       ARRIVALS: list of (pid arrival-time burst).
       Returns (busy-ticks idle-ticks utilization-percent timeline)."
      (let ((time 0)
            (ready-queue nil)
            (remaining-arrivals (copy-sequence arrivals))
            (busy 0)
            (idle 0)
            (timeline nil)
            (current nil)
            (current-tq 0))
        (while (< time total-time)
          ;; Add newly arriving processes
          (dolist (a remaining-arrivals)
            (when (= (nth 1 a) time)
              (setq ready-queue
                    (append ready-queue
                            (list (cons (nth 0 a) (nth 2 a)))))))
          (setq remaining-arrivals
                (cl-remove-if (lambda (a) (<= (nth 1 a) time))
                              remaining-arrivals))
          ;; If no current process, pick from queue
          (unless current
            (when ready-queue
              (setq current (car ready-queue))
              (setq ready-queue (cdr ready-queue))
              (setq current-tq 0)))
          (if current
              (progn
                (push (list time 'run (car current)) timeline)
                (setq busy (1+ busy))
                (setcdr current (1- (cdr current)))
                (setq current-tq (1+ current-tq))
                (cond
                  ;; Process finished
                  ((<= (cdr current) 0)
                   (setq current nil)
                   (setq current-tq 0))
                  ;; Quantum expired: preempt
                  ((>= current-tq quantum)
                   (setq ready-queue
                         (append ready-queue (list current)))
                   (setq current nil)
                   (setq current-tq 0))))
            ;; CPU idle
            (push (list time 'idle) timeline)
            (setq idle (1+ idle)))
          (setq time (1+ time)))
        (list busy idle
              (/ (* busy 100) total-time)
              (nreverse timeline)))))

  (unwind-protect
      (let* (;; Two processes: P1 arrives at t=0 burst=3, P2 arrives at t=1 burst=2
             (arrivals '((1 0 3) (2 1 2)))
             (r1 (funcall 'neovm--util-schedule arrivals 8 2))
             ;; Gap: P1 at t=0 burst=2, P2 at t=5 burst=2 => idle in between
             (arrivals2 '((1 0 2) (2 5 2)))
             (r2 (funcall 'neovm--util-schedule arrivals2 10 10))
             ;; Full utilization: continuous work
             (arrivals3 '((1 0 5) (2 0 5)))
             (r3 (funcall 'neovm--util-schedule arrivals3 10 3)))
        (list
          ;; r1: busy=5, idle=3
          (nth 0 r1) (nth 1 r1) (nth 2 r1)
          ;; r2: busy=4, idle=6
          (nth 0 r2) (nth 1 r2) (nth 2 r2)
          ;; r3: busy=10, idle=0, utilization=100%
          (nth 0 r3) (nth 1 r3) (nth 2 r3)))

    (fmakunbound 'neovm--util-schedule))))"#;
    let expect = expect_test::expect![[r#""OK (5 3 62 4 6 40 10 0 100)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Starvation detection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_os_scheduler_starvation_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Detect starvation: a process waiting longer than a threshold
    let form = r#"(progn (require (quote cl-lib))
(progn
  (fset 'neovm--starv-detect
    (lambda (schedule-log threshold)
      "Given a schedule log of (time pid action) entries, detect starved processes.
       A process is starved if it goes more than THRESHOLD ticks between runs.
       Returns list of (pid max-wait-time starved-p)."
      (let ((proc-data (make-hash-table :test 'eql))
            (result nil))
        ;; Collect per-process run times
        (dolist (entry schedule-log)
          (let ((time (nth 0 entry))
                (pid (nth 1 entry))
                (action (nth 2 entry)))
            (when (eq action 'run)
              (let ((existing (gethash pid proc-data)))
                (puthash pid (cons time (or existing nil)) proc-data)))))
        ;; Analyze gaps
        (maphash
          (lambda (pid times)
            (let* ((sorted (sort (copy-sequence times) #'<))
                   (max-gap 0)
                   (prev nil))
              (dolist (t sorted)
                (when prev
                  (let ((gap (- t prev)))
                    (when (> gap max-gap)
                      (setq max-gap gap))))
                (setq prev t))
              (push (list pid max-gap (> max-gap threshold)) result)))
          proc-data)
        (sort result (lambda (a b) (< (car a) (car b)))))))

  (unwind-protect
      (let* (;; Schedule: P1 runs continuously, P2 gets a few slots
             (log '((0 1 run) (1 1 run) (2 1 run) (3 2 run)
                    (4 1 run) (5 1 run) (6 1 run) (7 1 run)
                    (8 1 run) (9 2 run)))
             (r1 (funcall 'neovm--starv-detect log 3))
             ;; Fair schedule: alternating
             (fair-log '((0 1 run) (1 2 run) (2 1 run) (3 2 run)
                         (4 1 run) (5 2 run) (6 1 run) (7 2 run)))
             (r2 (funcall 'neovm--starv-detect fair-log 3)))
        (list
          ;; P2 has max gap of 6 (from t=3 to t=9), starved with threshold=3
          r1
          ;; Fair: max gap is 2 for both, not starved
          r2))

    (fmakunbound 'neovm--starv-detect))))"#;
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Fair-share scheduling between process groups
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_os_scheduler_fair_share() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Groups get proportional CPU shares. Within a group, round-robin.
    let form = r#"(progn (require (quote cl-lib))
(progn
  ;; Group: (group-id share processes)
  ;; Process: (pid . remaining)
  (fset 'neovm--fs-schedule
    (lambda (groups total-ticks)
      "Fair-share scheduler. GROUPS: list of (gid share procs).
       SHARE is relative weight (e.g., 2 = twice the share of 1).
       Returns (per-group-cpu-ticks group-fairness-ratios)."
      (let* ((total-share (apply '+ (mapcar (lambda (g) (nth 1 g)) groups)))
             (group-cpu (make-hash-table :test 'eql))
             (group-queues (make-hash-table :test 'eql))
             (group-ids (mapcar #'car groups))
             (tick 0)
             (round-idx 0))
        ;; Initialize
        (dolist (g groups)
          (puthash (nth 0 g) 0 group-cpu)
          (puthash (nth 0 g)
                   (mapcar (lambda (p) (cons (car p) (cdr p)))
                           (nth 2 g))
                   group-queues))
        ;; Simple proportional scheduling: in each round of total-share ticks,
        ;; give each group its share number of ticks
        (while (< tick total-ticks)
          (dolist (g groups)
            (let* ((gid (nth 0 g))
                   (share (nth 1 g))
                   (s 0))
              (while (and (< s share) (< tick total-ticks))
                (let ((q (gethash gid group-queues)))
                  (when q
                    (let ((proc (car q)))
                      (puthash gid (1+ (gethash gid group-cpu)) group-cpu)
                      (setcdr proc (1- (cdr proc)))
                      (if (<= (cdr proc) 0)
                          ;; Process done, remove
                          (puthash gid (cdr q) group-queues)
                        ;; Rotate
                        (puthash gid (append (cdr q) (list proc))
                                 group-queues)))))
                (setq s (1+ s))
                (setq tick (1+ tick))))))
        ;; Compute fairness ratios
        (let ((results nil))
          (dolist (g groups)
            (let* ((gid (nth 0 g))
                   (share (nth 1 g))
                   (cpu (gethash gid group-cpu))
                   (expected (/ (* total-ticks share) total-share))
                   (ratio (if (> expected 0)
                              (/ (* cpu 100) expected)
                            0)))
              (push (list gid cpu expected ratio) results)))
          (nreverse results)))))

  (unwind-protect
      (let* (;; Group A: share=1, 2 processes (burst 5 each)
             ;; Group B: share=2, 1 process (burst 10)
             ;; Total share=3, so A gets 1/3, B gets 2/3 of CPU
             (groups (list
                       (list 'a 1 '((1 . 5) (2 . 5)))
                       (list 'b 2 '((3 . 10)))))
             (r1 (funcall 'neovm--fs-schedule groups 12))
             ;; Equal share groups
             (equal-groups (list
                            (list 'x 1 '((1 . 5)))
                            (list 'y 1 '((2 . 5)))))
             (r2 (funcall 'neovm--fs-schedule equal-groups 10)))
        (list
          ;; Group results: (gid cpu expected ratio)
          r1
          ;; Equal groups should get equal CPU
          r2
          ;; Verify proportionality: group B should get ~2x group A's CPU
          (let ((a-cpu (nth 1 (car r1)))
                (b-cpu (nth 1 (cadr r1))))
            (list a-cpu b-cpu (>= b-cpu (* 2 (1- a-cpu)))))))

    (fmakunbound 'neovm--fs-schedule))))"#;
    let expect = expect_test::expect![[
        r#""OK (((a 4 4 100) (b 8 8 100)) ((x 5 5 100) (y 5 5 100)) (4 8 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
