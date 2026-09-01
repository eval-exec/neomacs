//! Advanced scheduling algorithm oracle parity tests:
//! round-robin with time quantum, priority scheduling with aging,
//! shortest job first (SJF), shortest remaining time first (SRTF),
//! multi-level feedback queue, deadline scheduling (EDF),
//! Gantt chart generation, turnaround/waiting time computation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Round-robin scheduling with configurable time quantum
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sched_adv_round_robin() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Round-robin: processes execute in circular order, each getting at most
    // `quantum` time units before being preempted.
    let form = r#"(progn
  (fset 'neovm--test-round-robin
    (lambda (processes quantum)
      "Round-robin scheduling.
       PROCESSES: list of (name burst-time arrival-time).
       QUANTUM: time slice.
       Returns (gantt completion-times)."
      (let* ((sorted (sort (copy-sequence processes)
                           (lambda (a b) (< (nth 2 a) (nth 2 b)))))
             (queue nil)
             (remaining (make-hash-table :test 'eq))
             (completion (make-hash-table :test 'eq))
             (gantt nil)
             (time 0)
             (waiting (copy-sequence sorted))
             (done 0)
             (n (length processes)))
        ;; Initialize remaining times
        (dolist (p sorted)
          (puthash (nth 0 p) (nth 1 p) remaining))
        ;; Add initially available processes
        (while (and waiting (<= (nth 2 (car waiting)) time))
          (setq queue (append queue (list (car waiting))))
          (setq waiting (cdr waiting)))
        ;; Main loop
        (while (< done n)
          (if (null queue)
              ;; CPU idle: advance to next arrival
              (progn
                (when waiting
                  (setq time (nth 2 (car waiting)))
                  (while (and waiting (<= (nth 2 (car waiting)) time))
                    (setq queue (append queue (list (car waiting))))
                    (setq waiting (cdr waiting)))))
            ;; Execute front of queue
            (let* ((proc (car queue))
                   (name (nth 0 proc))
                   (rem (gethash name remaining))
                   (exec-time (min rem quantum))
                   (start time))
              (setq queue (cdr queue))
              (setq time (+ time exec-time))
              (puthash name (- rem exec-time) remaining)
              (setq gantt (cons (list name start time) gantt))
              ;; Add newly arrived processes
              (while (and waiting (<= (nth 2 (car waiting)) time))
                (setq queue (append queue (list (car waiting))))
                (setq waiting (cdr waiting)))
              ;; If process not done, re-enqueue
              (if (> (gethash name remaining) 0)
                  (setq queue (append queue (list proc)))
                (puthash name time completion)
                (setq done (1+ done))))))
        ;; Build completion list
        (let ((comp-list nil))
          (dolist (p sorted)
            (setq comp-list (cons (list (nth 0 p) (gethash (nth 0 p) completion))
                                  comp-list)))
          (list (nreverse gantt) (nreverse comp-list))))))
  (unwind-protect
      (let ((procs '((P1 10 0) (P2 4 1) (P3 6 2) (P4 3 3)))
            (quantum 3))
        (let ((result (neovm--test-round-robin procs quantum)))
          (let ((gantt (nth 0 result))
                (completions (nth 1 result)))
            (list gantt
                  completions
                  (length gantt)
                  ;; Compute average turnaround time * 10 (integer)
                  (let ((total 0))
                    (dolist (p procs)
                      (let ((turnaround (- (cadr (assq (nth 0 p) completions))
                                           (nth 2 p))))
                        (setq total (+ total turnaround))))
                    total)))))
    (fmakunbound 'neovm--test-round-robin)))"#;
    let expect = expect_test::expect![[
        r#""OK (((P1 0 3) (P2 3 6) (P3 6 9) (P4 9 12) (P1 12 15) (P2 15 16) (P3 16 19) (P1 19 22) (P1 22 23)) ((P1 23) (P2 16) (P3 19) (P4 12)) 9 64)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Priority scheduling with aging to prevent starvation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sched_adv_priority_with_aging() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Non-preemptive priority scheduling: pick highest priority (lowest number)
    // ready process. Aging: every `aging-interval` units, all waiting processes
    // have their effective priority decreased by 1 (boosted).
    let form = r#"(progn
  (fset 'neovm--test-priority-aging
    (lambda (processes aging-interval)
      "Priority scheduling with aging.
       PROCESSES: list of (name burst priority arrival).
       AGING-INTERVAL: time units between priority boosts.
       Returns (execution-order completion-times)."
      (let* ((n (length processes))
             (remaining (copy-sequence processes))
             (time 0)
             (order nil)
             (completions nil)
             (done 0)
             ;; Track effective priorities
             (eff-priority (make-hash-table :test 'eq))
             (last-age-time 0))
        ;; Initialize effective priorities
        (dolist (p processes)
          (puthash (nth 0 p) (nth 2 p) eff-priority))
        (while (< done n)
          ;; Apply aging: boost priorities of waiting processes
          (when (and (> aging-interval 0)
                     (>= (- time last-age-time) aging-interval)
                     (> time 0))
            (dolist (p remaining)
              (let* ((name (nth 0 p))
                     (cur (gethash name eff-priority)))
                (when (and cur (> cur 0) (<= (nth 3 p) time))
                  (puthash name (1- cur) eff-priority))))
            (setq last-age-time time))
          ;; Find ready processes (arrived and not completed)
          (let ((ready nil))
            (dolist (p remaining)
              (when (<= (nth 3 p) time)
                (setq ready (cons p ready))))
            (if (null ready)
                ;; Idle: advance to next arrival
                (let ((next-arrival nil))
                  (dolist (p remaining)
                    (when (or (null next-arrival) (< (nth 3 p) next-arrival))
                      (setq next-arrival (nth 3 p))))
                  (setq time (or next-arrival (1+ time))))
              ;; Pick highest priority (lowest effective priority number)
              (let ((best nil) (best-prio 999999))
                (dolist (p ready)
                  (let ((ep (gethash (nth 0 p) eff-priority)))
                    (when (< ep best-prio)
                      (setq best p)
                      (setq best-prio ep))))
                ;; Execute the chosen process (non-preemptive)
                (let* ((name (nth 0 best))
                       (burst (nth 1 best))
                       (start time)
                       (finish (+ time burst)))
                  (setq order (cons (list name start finish) order))
                  (setq completions (cons (list name finish) completions))
                  (setq time finish)
                  (setq remaining (delq best remaining))
                  (setq done (1+ done)))))))
        (list (nreverse order) (nreverse completions)))))
  (unwind-protect
      (let ((procs '((P1 6 3 0)    ;; name burst priority arrival
                     (P2 3 1 1)
                     (P3 8 4 2)
                     (P4 2 2 3)
                     (P5 4 5 0)))
            (aging-interval 5))
        (let ((result (neovm--test-priority-aging procs aging-interval)))
          (list (nth 0 result)   ;; execution order with times
                (nth 1 result)   ;; completion times
                ;; Total waiting time
                (let ((total-wait 0))
                  (dolist (p procs)
                    (let* ((name (nth 0 p))
                           (arrival (nth 3 p))
                           (burst (nth 1 p))
                           (comp (cadr (assq name (nth 1 result))))
                           (wait (- comp arrival burst)))
                      (setq total-wait (+ total-wait wait))))
                  total-wait))))
    (fmakunbound 'neovm--test-priority-aging)))"#;
    let expect = expect_test::expect![[
        r#""OK (((P1 0 6) (P2 6 9) (P4 9 11) (P3 11 19) (P5 19 23)) ((P1 6) (P2 9) (P4 11) (P3 19) (P5 23)) 39)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Shortest Job First (SJF) non-preemptive
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sched_adv_shortest_job_first() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-sjf
    (lambda (processes)
      "Shortest Job First (non-preemptive).
       PROCESSES: list of (name burst arrival).
       Returns (gantt turnaround-times waiting-times)."
      (let* ((remaining (copy-sequence processes))
             (time 0) (gantt nil) (completions nil) (done 0) (n (length processes)))
        (while (< done n)
          ;; Find ready processes
          (let ((ready nil))
            (dolist (p remaining)
              (when (<= (nth 2 p) time)
                (setq ready (cons p ready))))
            (if (null ready)
                ;; Idle: advance to earliest arrival
                (let ((earliest 999999))
                  (dolist (p remaining)
                    (when (< (nth 2 p) earliest)
                      (setq earliest (nth 2 p))))
                  (setq time earliest))
              ;; Pick shortest burst among ready
              (let ((best nil) (best-burst 999999))
                (dolist (p ready)
                  (when (< (nth 1 p) best-burst)
                    (setq best p)
                    (setq best-burst (nth 1 p))))
                (let* ((name (nth 0 best))
                       (burst (nth 1 best))
                       (start time)
                       (finish (+ time burst)))
                  (setq gantt (cons (list name start finish) gantt))
                  (setq completions (cons (list name finish) completions))
                  (setq time finish)
                  (setq remaining (delq best remaining))
                  (setq done (1+ done)))))))
        ;; Compute turnaround and waiting times
        (let ((tt nil) (wt nil))
          (dolist (p processes)
            (let* ((name (nth 0 p))
                   (burst (nth 1 p))
                   (arrival (nth 2 p))
                   (comp (cadr (assq name completions)))
                   (turnaround (- comp arrival))
                   (waiting (- turnaround burst)))
              (setq tt (cons (list name turnaround) tt))
              (setq wt (cons (list name waiting) wt))))
          (list (nreverse gantt) (nreverse tt) (nreverse wt))))))
  (unwind-protect
      (let ((procs '((P1 6 0) (P2 2 1) (P3 8 2) (P4 3 3) (P5 1 4))))
        (let ((result (neovm--test-sjf procs)))
          (list (nth 0 result)
                (nth 1 result)
                (nth 2 result)
                ;; Average turnaround time * length (sum)
                (apply #'+ (mapcar #'cadr (nth 1 result)))
                ;; Average waiting time * length (sum)
                (apply #'+ (mapcar #'cadr (nth 2 result))))))
    (fmakunbound 'neovm--test-sjf)))"#;
    let expect = expect_test::expect![[
        r#""OK (((P1 0 6) (P5 6 7) (P2 7 9) (P4 9 12) (P3 12 20)) ((P1 6) (P2 8) (P3 18) (P4 9) (P5 3)) ((P1 0) (P2 6) (P3 10) (P4 6) (P5 2)) 44 24)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Shortest Remaining Time First (SRTF) — preemptive SJF
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sched_adv_srtf() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // SRTF: at each time unit, the process with the least remaining time runs.
    // This is the preemptive version of SJF.
    let form = r#"(progn
  (fset 'neovm--test-srtf
    (lambda (processes)
      "Shortest Remaining Time First (preemptive SJF).
       PROCESSES: list of (name burst arrival).
       Returns (gantt turnaround-times waiting-times)."
      (let* ((n (length processes))
             (rem (make-hash-table :test 'eq))
             (completion (make-hash-table :test 'eq))
             (time 0)
             (done 0)
             (gantt nil)
             (max-time (+ (apply #'max (mapcar #'nth-1-plus-2 processes)) 1))
             (prev-proc nil))
        ;; Helper stored as dynamic
        (dolist (p processes)
          (puthash (nth 0 p) (nth 1 p) rem))
        ;; Find max possible time
        (setq max-time (apply #'+ (mapcar (lambda (p) (+ (nth 1 p) (nth 2 p))) processes)))
        (while (and (< done n) (< time (+ max-time 1)))
          ;; Find ready process with shortest remaining time
          (let ((best nil) (best-rem 999999))
            (dolist (p processes)
              (let ((name (nth 0 p))
                    (arrival (nth 2 p)))
                (when (and (<= arrival time)
                           (> (gethash name rem 0) 0)
                           (< (gethash name rem) best-rem))
                  (setq best (nth 0 p))
                  (setq best-rem (gethash name rem)))))
            (if (null best)
                (setq time (1+ time))
              ;; Record gantt entry (merge consecutive same-process entries)
              (when (not (eq best prev-proc))
                (when prev-proc
                  ;; Close previous gantt segment if needed
                  nil)
                (setq gantt (cons (list best time nil) gantt)))
              (setq prev-proc best)
              ;; Execute one time unit
              (puthash best (1- (gethash best rem)) rem)
              (setq time (1+ time))
              ;; Update end time of current gantt entry
              (setcar (cddr (car gantt)) time)
              ;; Check if completed
              (when (= (gethash best rem) 0)
                (puthash best time completion)
                (setq done (1+ done))
                (setq prev-proc nil)))))
        ;; Compute results
        (let ((tt nil) (wt nil))
          (dolist (p processes)
            (let* ((name (nth 0 p))
                   (burst (nth 1 p))
                   (arrival (nth 2 p))
                   (comp (gethash name completion))
                   (turnaround (- comp arrival))
                   (waiting (- turnaround burst)))
              (setq tt (cons (list name turnaround) tt))
              (setq wt (cons (list name waiting) wt))))
          (list (nreverse gantt) (nreverse tt) (nreverse wt))))))
  ;; Helper for max-time calculation
  (fset 'nth-1-plus-2 (lambda (p) (+ (nth 1 p) (nth 2 p))))
  (unwind-protect
      (let ((procs '((P1 7 0) (P2 4 2) (P3 1 4) (P4 4 5))))
        (let ((result (neovm--test-srtf procs)))
          (list (nth 0 result)
                (nth 1 result)
                (nth 2 result)
                (apply #'+ (mapcar #'cadr (nth 1 result)))
                (apply #'+ (mapcar #'cadr (nth 2 result))))))
    (fmakunbound 'neovm--test-srtf)
    (fmakunbound 'nth-1-plus-2)))"#;
    let expect = expect_test::expect![[
        r#""OK (((P1 0 2) (P2 2 4) (P3 4 5) (P2 5 7) (P4 7 11) (P1 11 16)) ((P1 16) (P2 5) (P3 1) (P4 6)) ((P1 9) (P2 1) (P3 0) (P4 2)) 28 12)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-level feedback queue (simplified 3-level)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sched_adv_multi_level_feedback_queue() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // 3-level MLFQ: Q0 (quantum=2), Q1 (quantum=4), Q2 (FCFS).
    // New processes enter Q0. If preempted, move down a level.
    let form = r#"(progn
  (fset 'neovm--test-mlfq
    (lambda (processes)
      "Multi-level feedback queue (3 levels).
       Q0: RR quantum=2, Q1: RR quantum=4, Q2: FCFS.
       PROCESSES: list of (name burst arrival).
       Returns (gantt completions)."
      (let* ((n (length processes))
             (sorted (sort (copy-sequence processes)
                           (lambda (a b) (< (nth 2 a) (nth 2 b)))))
             (rem (make-hash-table :test 'eq))
             (level (make-hash-table :test 'eq))
             (completion (make-hash-table :test 'eq))
             (q0 nil) (q1 nil) (q2 nil)
             (time 0) (done 0) (gantt nil)
             (waiting (copy-sequence sorted))
             (quantums '(2 4 999999)))
        ;; Initialize
        (dolist (p sorted)
          (puthash (nth 0 p) (nth 1 p) rem)
          (puthash (nth 0 p) 0 level))
        ;; Admit initial arrivals
        (while (and waiting (<= (nth 2 (car waiting)) time))
          (setq q0 (append q0 (list (nth 0 (car waiting)))))
          (setq waiting (cdr waiting)))
        (while (< done n)
          ;; Pick from highest priority non-empty queue
          (let ((current-q nil) (current-level nil) (quantum nil))
            (cond
             (q0 (setq current-q 'q0 current-level 0 quantum 2))
             (q1 (setq current-q 'q1 current-level 1 quantum 4))
             (q2 (setq current-q 'q2 current-level 2 quantum 999999))
             (t  ;; All queues empty: advance time
                 (when waiting
                   (setq time (nth 2 (car waiting)))
                   (while (and waiting (<= (nth 2 (car waiting)) time))
                     (setq q0 (append q0 (list (nth 0 (car waiting)))))
                     (setq waiting (cdr waiting)))
                   (setq current-q 'q0 current-level 0 quantum 2))))
            (when current-q
              ;; Dequeue front process from appropriate queue
              (let* ((proc (cond ((eq current-q 'q0) (let ((p (car q0))) (setq q0 (cdr q0)) p))
                                 ((eq current-q 'q1) (let ((p (car q1))) (setq q1 (cdr q1)) p))
                                 ((eq current-q 'q2) (let ((p (car q2))) (setq q2 (cdr q2)) p)))))
                (when proc
                  (let* ((r (gethash proc rem))
                         (exec (min r quantum))
                         (start time))
                    (setq time (+ time exec))
                    (puthash proc (- r exec) rem)
                    (setq gantt (cons (list proc start time current-level) gantt))
                    ;; Admit new arrivals
                    (while (and waiting (<= (nth 2 (car waiting)) time))
                      (setq q0 (append q0 (list (nth 0 (car waiting)))))
                      (setq waiting (cdr waiting)))
                    ;; Check completion or demotion
                    (if (= (gethash proc rem) 0)
                        (progn
                          (puthash proc time completion)
                          (setq done (1+ done)))
                      ;; Demote: move to next lower queue
                      (let ((new-level (min 2 (1+ current-level))))
                        (puthash proc new-level level)
                        (cond ((= new-level 0) (setq q0 (append q0 (list proc))))
                              ((= new-level 1) (setq q1 (append q1 (list proc))))
                              ((= new-level 2) (setq q2 (append q2 (list proc)))))))))))))
        (let ((comp-list nil))
          (dolist (p sorted)
            (setq comp-list (cons (list (nth 0 p) (gethash (nth 0 p) completion))
                                  comp-list)))
          (list (nreverse gantt) (nreverse comp-list))))))
  (unwind-protect
      (let ((procs '((P1 12 0) (P2 5 1) (P3 3 3) (P4 8 5))))
        (let ((result (neovm--test-mlfq procs)))
          (list (nth 0 result)
                (nth 1 result)
                (length (nth 0 result)))))
    (fmakunbound 'neovm--test-mlfq)))"#;
    let expect = expect_test::expect![[
        r#""OK (((P1 0 2 0) (P2 2 4 0) (P3 4 6 0) (P4 6 8 0) (P1 8 12 1) (P2 12 15 1) (P3 15 16 1) (P4 16 20 1) (P1 20 26 2) (P4 26 28 2)) ((P1 26) (P2 15) (P3 16) (P4 28)) 10)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Earliest Deadline First (EDF) with preemption
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sched_adv_edf_preemptive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Preemptive EDF: at each time unit, the process with the earliest
    // absolute deadline runs. A newly arriving process with an earlier
    // deadline preempts the current process.
    let form = r#"(progn
  (fset 'neovm--test-edf-preemptive
    (lambda (processes)
      "Preemptive EDF scheduling.
       PROCESSES: list of (name burst arrival deadline).
       Returns (gantt completions missed-deadlines)."
      (let* ((n (length processes))
             (rem (make-hash-table :test 'eq))
             (completion (make-hash-table :test 'eq))
             (deadlines (make-hash-table :test 'eq))
             (time 0) (done 0) (gantt nil)
             (prev-proc nil)
             (max-time 0))
        ;; Initialize
        (dolist (p processes)
          (puthash (nth 0 p) (nth 1 p) rem)
          (puthash (nth 0 p) (nth 3 p) deadlines)
          (setq max-time (max max-time (+ (nth 2 p) (nth 1 p)))))
        (setq max-time (+ max-time 5)) ;; safety margin
        (while (and (< done n) (< time max-time))
          ;; Find ready process with earliest deadline
          (let ((best nil) (best-deadline 999999))
            (dolist (p processes)
              (let ((name (nth 0 p)))
                (when (and (<= (nth 2 p) time)
                           (> (gethash name rem 0) 0)
                           (< (gethash name deadlines) best-deadline))
                  (setq best name)
                  (setq best-deadline (gethash name deadlines)))))
            (if (null best)
                (setq time (1+ time))
              ;; Context switch detection
              (when (not (eq best prev-proc))
                (setq gantt (cons (list best time nil) gantt)))
              (setq prev-proc best)
              ;; Execute one time unit
              (puthash best (1- (gethash best rem)) rem)
              (setq time (1+ time))
              (setcar (cddr (car gantt)) time)
              (when (= (gethash best rem) 0)
                (puthash best time completion)
                (setq done (1+ done))
                (setq prev-proc nil)))))
        ;; Determine missed deadlines
        (let ((missed nil) (comp-list nil))
          (dolist (p processes)
            (let* ((name (nth 0 p))
                   (comp (gethash name completion))
                   (dl (gethash name deadlines)))
              (setq comp-list (cons (list name comp) comp-list))
              (when (and comp (> comp dl))
                (setq missed (cons name missed)))))
          (list (nreverse gantt) (nreverse comp-list) (nreverse missed))))))
  (unwind-protect
      (let ((procs '((P1 3 0 7)     ;; name burst arrival deadline
                     (P2 2 1 5)
                     (P3 4 2 10)
                     (P4 1 4 6)
                     (P5 3 5 12))))
        (let ((result (neovm--test-edf-preemptive procs)))
          (list (nth 0 result)
                (nth 1 result)
                (nth 2 result)
                ;; Number of context switches
                (1- (length (nth 0 result))))))
    (fmakunbound 'neovm--test-edf-preemptive)))"#;
    let expect = expect_test::expect![[
        r#""OK (((P1 0 1) (P2 1 3) (P1 3 4) (P4 4 5) (P1 5 6) (P3 6 10) (P5 10 13)) ((P1 6) (P2 3) (P3 10) (P4 5) (P5 13)) (P5) 6)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Gantt chart generation and turnaround/waiting time computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sched_adv_gantt_and_metrics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Comprehensive scheduling metrics: run FCFS, SJF, and RR on the same
    // workload, then compute and compare turnaround time, waiting time,
    // response time, throughput, and CPU utilization.
    let form = r#"(progn
  ;; FCFS scheduler
  (fset 'neovm--test-fcfs
    (lambda (processes)
      (let* ((sorted (sort (copy-sequence processes)
                           (lambda (a b) (< (nth 2 a) (nth 2 b)))))
             (time 0) (gantt nil) (completions nil))
        (dolist (p sorted)
          (let* ((name (nth 0 p))
                 (burst (nth 1 p))
                 (arrival (nth 2 p))
                 (start (max time arrival))
                 (finish (+ start burst)))
            (setq gantt (cons (list name start finish) gantt))
            (setq completions (cons (list name finish) completions))
            (setq time finish)))
        (list (nreverse gantt) (nreverse completions)))))
  ;; Metrics calculator
  (fset 'neovm--test-metrics
    (lambda (processes completions)
      "Compute turnaround, waiting, response times."
      (let ((metrics nil))
        (dolist (p processes)
          (let* ((name (nth 0 p))
                 (burst (nth 1 p))
                 (arrival (nth 2 p))
                 (comp (cadr (assq name completions)))
                 (turnaround (- comp arrival))
                 (waiting (- turnaround burst)))
            (setq metrics (cons (list name turnaround waiting) metrics))))
        (nreverse metrics))))
  ;; Gantt chart formatter (text representation)
  (fset 'neovm--test-format-gantt
    (lambda (gantt)
      (mapconcat (lambda (entry)
                   (format "%s[%d-%d]" (nth 0 entry) (nth 1 entry) (nth 2 entry)))
                 gantt " ")))
  (unwind-protect
      (let ((procs '((P1 5 0) (P2 3 1) (P3 8 2) (P4 2 3) (P5 4 4))))
        (let* ((fcfs-result (neovm--test-fcfs procs))
               (fcfs-gantt (nth 0 fcfs-result))
               (fcfs-comp (nth 1 fcfs-result))
               (fcfs-metrics (neovm--test-metrics procs fcfs-comp))
               ;; Total times
               (total-turnaround (apply #'+ (mapcar #'cadr fcfs-metrics)))
               (total-waiting (apply #'+ (mapcar #'caddr fcfs-metrics)))
               ;; CPU utilization: total burst / total time
               (total-burst (apply #'+ (mapcar (lambda (p) (nth 1 p)) procs)))
               (total-time (apply #'max (mapcar #'cadr fcfs-comp)))
               ;; Throughput (scaled by 100 for integer arithmetic)
               (throughput-x100 (/ (* (length procs) 100) total-time)))
          (list
            ;; Gantt chart as formatted string
            (neovm--test-format-gantt fcfs-gantt)
            ;; Per-process metrics: (name turnaround waiting)
            fcfs-metrics
            ;; Aggregate stats
            (list 'total-turnaround total-turnaround
                  'total-waiting total-waiting
                  'total-burst total-burst
                  'makespan total-time
                  'throughput-x100 throughput-x100)
            ;; Verify: turnaround = waiting + burst for each process
            (mapcar (lambda (m)
                      (let ((name (nth 0 m))
                            (turnaround (nth 1 m))
                            (waiting (nth 2 m)))
                        (let ((burst (nth 1 (assq name procs))))
                          (= turnaround (+ waiting burst)))))
                    fcfs-metrics))))
    (fmakunbound 'neovm--test-fcfs)
    (fmakunbound 'neovm--test-metrics)
    (fmakunbound 'neovm--test-format-gantt)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"P1[0-5] P2[5-8] P3[8-16] P4[16-18] P5[18-22]\" ((P1 5 0) (P2 7 4) (P3 14 6) (P4 15 13) (P5 18 14)) (total-turnaround 59 total-waiting 37 total-burst 22 makespan 22 throughput-x100 22) (t t t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
