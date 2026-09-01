//! Oracle parity tests for scheduling and planning algorithms in pure Elisp.
//!
//! Covers: earliest deadline first scheduling, interval scheduling (maximum
//! non-overlapping intervals), topological sort with dependency resolution,
//! resource allocation (first-fit bin packing), round-robin scheduler
//! simulation, and priority-based preemptive scheduling.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Earliest Deadline First (EDF) scheduling
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sched_earliest_deadline_first() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Schedule jobs by earliest deadline. Each job: (name duration deadline).
    // Execute in deadline order, track completion times, report which miss deadlines.
    let form = r#"(progn
                    (fset 'neovm--test-edf-schedule
                          (lambda (jobs)
                            "Schedule JOBS by earliest deadline first.
                             JOBS: list of (name duration deadline).
                             Returns (schedule missed-jobs)."
                            (let* ((sorted (sort (copy-sequence jobs)
                                                 (lambda (a b) (< (nth 2 a) (nth 2 b)))))
                                   (current-time 0)
                                   (schedule nil)
                                   (missed nil))
                              (dolist (job sorted)
                                (let* ((name (nth 0 job))
                                       (duration (nth 1 job))
                                       (deadline (nth 2 job))
                                       (start current-time)
                                       (finish (+ current-time duration)))
                                  (setq current-time finish)
                                  (setq schedule
                                        (cons (list name start finish deadline
                                                    (if (<= finish deadline) 'ok 'late))
                                              schedule))
                                  (when (> finish deadline)
                                    (setq missed (cons name missed)))))
                              (list (nreverse schedule) (nreverse missed)))))
                    (unwind-protect
                        (let ((jobs '((task-a 3 10)
                                      (task-b 5 8)
                                      (task-c 2 6)
                                      (task-d 4 15)
                                      (task-e 1 4)
                                      (task-f 6 20))))
                          (let ((result (neovm--test-edf-schedule jobs)))
                            (let ((schedule (nth 0 result))
                                  (missed (nth 1 result)))
                              (list schedule
                                    missed
                                    (length schedule)
                                    (length missed)
                                    ;; Total time used
                                    (apply #'+ (mapcar (lambda (j) (nth 1 j)) jobs))))))
                      (fmakunbound 'neovm--test-edf-schedule)))"#;
    let expect = expect_test::expect![[
        r#""OK (((task-e 0 1 4 ok) (task-c 1 3 6 ok) (task-b 3 8 8 ok) (task-a 8 11 10 late) (task-d 11 15 15 ok) (task-f 15 21 20 late)) (task-a task-f) 6 2 21)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interval scheduling: maximum non-overlapping intervals
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sched_interval_scheduling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Greedy interval scheduling: select maximum number of non-overlapping
    // intervals by sorting by end time and greedily selecting.
    let form = r#"(progn
                    (fset 'neovm--test-interval-schedule
                          (lambda (intervals)
                            "Select max non-overlapping intervals.
                             INTERVALS: list of (name start end).
                             Returns selected intervals."
                            (let* ((sorted (sort (copy-sequence intervals)
                                                 (lambda (a b) (< (nth 2 a) (nth 2 b)))))
                                   (selected nil)
                                   (last-end -1))
                              (dolist (iv sorted)
                                (let ((name (nth 0 iv))
                                      (start (nth 1 iv))
                                      (end (nth 2 iv)))
                                  (when (>= start last-end)
                                    (setq selected (cons iv selected))
                                    (setq last-end end))))
                              (nreverse selected))))
                    (unwind-protect
                        (let ((intervals '((meeting-a 0 3)
                                           (meeting-b 1 4)
                                           (meeting-c 3 6)
                                           (meeting-d 5 7)
                                           (meeting-e 4 8)
                                           (meeting-f 7 9)
                                           (meeting-g 8 10)
                                           (meeting-h 2 5))))
                          (let ((selected (neovm--test-interval-schedule intervals)))
                            (list selected
                                  (length selected)
                                  ;; Verify no overlaps in selected
                                  (let ((ok t)
                                        (prev-end -1))
                                    (dolist (iv selected)
                                      (when (< (nth 1 iv) prev-end)
                                        (setq ok nil))
                                      (setq prev-end (nth 2 iv)))
                                    ok)
                                  ;; Total coverage (sum of durations)
                                  (apply #'+ (mapcar (lambda (iv)
                                                       (- (nth 2 iv) (nth 1 iv)))
                                                     selected)))))
                      (fmakunbound 'neovm--test-interval-schedule)))"#;
    let expect =
        expect_test::expect![[r#""OK (((meeting-a 0 3) (meeting-c 3 6) (meeting-f 7 9)) 3 t 8)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Topological sort with dependency resolution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sched_topological_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Kahn's algorithm: topological sort of tasks with dependencies.
    // Detect cycles. Schedule tasks respecting dependency order.
    let form = r#"(progn
                    (fset 'neovm--test-topo-sort
                          (lambda (tasks deps)
                            "Topological sort of TASKS with DEPS ((from . to) ...).
                             Returns (ordered-tasks cycle-detected)."
                            (let ((in-degree (make-hash-table :test 'eq))
                                  (adj (make-hash-table :test 'eq))
                                  (queue nil)
                                  (result nil))
                              ;; Initialize
                              (dolist (task tasks)
                                (puthash task 0 in-degree)
                                (puthash task nil adj))
                              ;; Build adjacency and in-degrees
                              (dolist (dep deps)
                                (let ((from (car dep))
                                      (to (cdr dep)))
                                  (puthash from (cons to (gethash from adj)) adj)
                                  (puthash to (1+ (gethash to in-degree 0)) in-degree)))
                              ;; Find nodes with in-degree 0
                              (maphash (lambda (k v)
                                         (when (= v 0)
                                           (setq queue (cons k queue))))
                                       in-degree)
                              ;; Sort queue for deterministic order
                              (setq queue (sort queue (lambda (a b)
                                                        (string< (symbol-name a)
                                                                 (symbol-name b)))))
                              ;; Process
                              (while queue
                                (let ((node (car queue)))
                                  (setq queue (cdr queue))
                                  (setq result (cons node result))
                                  (dolist (neighbor (gethash node adj))
                                    (let ((new-deg (1- (gethash neighbor in-degree))))
                                      (puthash neighbor new-deg in-degree)
                                      (when (= new-deg 0)
                                        (setq queue
                                              (sort (cons neighbor queue)
                                                    (lambda (a b)
                                                      (string< (symbol-name a)
                                                               (symbol-name b))))))))))
                              (list (nreverse result)
                                    (/= (length result) (length tasks))))))
                    (unwind-protect
                        (list
                         ;; Acyclic graph
                         (neovm--test-topo-sort
                          '(compile link test deploy setup)
                          '((setup . compile) (compile . link)
                            (link . test) (test . deploy)
                            (setup . test)))
                         ;; Graph with multiple valid orderings (diamond)
                         (neovm--test-topo-sort
                          '(a b c d)
                          '((a . b) (a . c) (b . d) (c . d)))
                         ;; Disconnected components
                         (neovm--test-topo-sort
                          '(x y z p q)
                          '((x . y) (y . z) (p . q))))
                      (fmakunbound 'neovm--test-topo-sort)))"#;
    let expect = expect_test::expect![[
        r#""OK (((setup compile link test deploy) t) ((a b c d) t) ((p q x y z) t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Resource allocation: first-fit bin packing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sched_bin_packing_first_fit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // First-fit bin packing: assign items to bins of fixed capacity,
    // using the first bin that has enough remaining space.
    let form = r#"(progn
                    (fset 'neovm--test-bin-pack
                          (lambda (items capacity)
                            "Pack ITEMS into bins of CAPACITY using first-fit.
                             Returns list of bins, each bin is (remaining-space . items)."
                            (let ((bins nil))
                              (dolist (item items)
                                (let ((size (cdr item))
                                      (name (car item))
                                      (placed nil))
                                  ;; Try each existing bin
                                  (let ((bin-list bins)
                                        (idx 0))
                                    (while (and bin-list (not placed))
                                      (let ((bin (car bin-list)))
                                        (when (>= (car bin) size)
                                          ;; Fits: update remaining space and add item
                                          (setcar bin (- (car bin) size))
                                          (setcdr bin (cons (cons name size) (cdr bin)))
                                          (setq placed t)))
                                      (setq bin-list (cdr bin-list))
                                      (setq idx (1+ idx))))
                                  ;; No bin fits: create new bin
                                  (unless placed
                                    (setq bins
                                          (append bins
                                                  (list (cons (- capacity size)
                                                              (list (cons name size)))))))))
                              ;; Format output: reverse item lists for insertion order
                              (mapcar (lambda (bin)
                                        (cons (car bin) (nreverse (cdr bin))))
                                      bins))))
                    (unwind-protect
                        (let ((items '((file-a . 3) (file-b . 5) (file-c . 2)
                                       (file-d . 7) (file-e . 4) (file-f . 1)
                                       (file-g . 6) (file-h . 3) (file-i . 2)))
                              (capacity 10))
                          (let ((result (neovm--test-bin-pack items capacity)))
                            (list result
                                  (length result)  ;; number of bins used
                                  ;; Verify all items placed
                                  (let ((total-items 0))
                                    (dolist (bin result)
                                      (setq total-items
                                            (+ total-items (length (cdr bin)))))
                                    (= total-items (length items)))
                                  ;; Verify no bin overflows
                                  (let ((all-ok t))
                                    (dolist (bin result)
                                      (when (< (car bin) 0)
                                        (setq all-ok nil)))
                                    all-ok)
                                  ;; Total wasted space
                                  (apply #'+ (mapcar #'car result)))))
                      (fmakunbound 'neovm--test-bin-pack)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 (file-a . 3) (file-b . 5) (file-c . 2)) (0 (file-d . 7) (file-f . 1) (file-i . 2)) (0 (file-e . 4) (file-g . 6)) (7 (file-h . 3))) 4 t t 7)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Round-robin scheduler simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sched_round_robin() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate round-robin scheduling with a time quantum.
    // Each process has a burst time. Track turnaround and wait times.
    let form = r#"(progn
                    (fset 'neovm--test-round-robin
                          (lambda (processes quantum)
                            "Round-robin schedule PROCESSES with time QUANTUM.
                             PROCESSES: list of (name burst-time).
                             Returns (execution-log stats)."
                            (let ((queue (mapcar (lambda (p)
                                                   (list (nth 0 p) (nth 1 p) (nth 1 p)))
                                                 processes))
                                  ;; Each entry: (name remaining original-burst)
                                  (clock 0)
                                  (log nil)
                                  (completion-times (make-hash-table :test 'eq)))
                              (while queue
                                (let* ((proc (car queue))
                                       (name (nth 0 proc))
                                       (remaining (nth 1 proc))
                                       (slice (min quantum remaining))
                                       (new-remaining (- remaining slice)))
                                  (setq clock (+ clock slice))
                                  (setq log (cons (list name clock slice) log))
                                  (setq queue (cdr queue))
                                  (if (> new-remaining 0)
                                      ;; Re-enqueue with reduced remaining
                                      (setq queue
                                            (append queue
                                                    (list (list name new-remaining
                                                                (nth 2 proc)))))
                                    ;; Process finished
                                    (puthash name clock completion-times))))
                              ;; Compute stats
                              (let ((stats nil))
                                (dolist (p processes)
                                  (let* ((name (nth 0 p))
                                         (burst (nth 1 p))
                                         (finish (gethash name completion-times))
                                         (turnaround finish)
                                         (wait (- turnaround burst)))
                                    (setq stats
                                          (cons (list name burst finish turnaround wait)
                                                stats))))
                                (list (nreverse log) (nreverse stats))))))
                    (unwind-protect
                        (let ((processes '((P1 10) (P2 4) (P3 6) (P4 3)))
                              (quantum 3))
                          (let ((result (neovm--test-round-robin processes quantum)))
                            (let ((log (nth 0 result))
                                  (stats (nth 1 result)))
                              (list log stats
                                    ;; Average turnaround
                                    (let ((sum 0))
                                      (dolist (s stats)
                                        (setq sum (+ sum (nth 3 s))))
                                      (/ sum (length stats)))
                                    ;; Average wait
                                    (let ((sum 0))
                                      (dolist (s stats)
                                        (setq sum (+ sum (nth 4 s))))
                                      (/ sum (length stats)))
                                    ;; Total execution time = clock at end
                                    (nth 1 (car log))))))
                      (fmakunbound 'neovm--test-round-robin)))"#;
    let expect = expect_test::expect![[
        r#""OK (((P1 3 3) (P2 6 3) (P3 9 3) (P4 12 3) (P1 15 3) (P2 16 1) (P3 19 3) (P1 22 3) (P1 23 1)) ((P1 10 23 23 13) (P2 4 16 16 12) (P3 6 19 19 13) (P4 3 12 12 9)) 17 11 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Priority-based preemptive scheduling
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sched_priority_preemptive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Priority scheduling with preemption: at each time unit, run the
    // highest-priority (lowest number) available process. Processes arrive
    // at different times.
    let form = r#"(progn
                    (fset 'neovm--test-priority-schedule
                          (lambda (processes)
                            "Priority preemptive scheduling.
                             PROCESSES: list of (name arrival burst priority).
                             Lower priority number = higher priority.
                             Returns (timeline completion-info)."
                            (let* ((max-time (apply #'+ (mapcar (lambda (p) (nth 2 p))
                                                                processes)))
                                   ;; Remaining burst for each process
                                   (remaining (make-hash-table :test 'eq))
                                   (arrivals (make-hash-table :test 'eq))
                                   (priorities (make-hash-table :test 'eq))
                                   (completions (make-hash-table :test 'eq))
                                   (timeline nil))
                              ;; Initialize
                              (dolist (p processes)
                                (puthash (nth 0 p) (nth 2 p) remaining)
                                (puthash (nth 0 p) (nth 1 p) arrivals)
                                (puthash (nth 0 p) (nth 3 p) priorities))
                              ;; Simulate each time unit
                              (let ((t 0)
                                    (all-done nil))
                                (while (and (< t (+ max-time 10)) (not all-done))
                                  ;; Find available processes (arrived and not completed)
                                  (let ((available nil))
                                    (dolist (p processes)
                                      (let ((name (nth 0 p)))
                                        (when (and (<= (gethash name arrivals) t)
                                                   (> (gethash name remaining) 0))
                                          (setq available (cons name available)))))
                                    (if available
                                        ;; Pick highest priority (lowest number)
                                        (let ((best nil)
                                              (best-prio 999999))
                                          (dolist (name available)
                                            (let ((prio (gethash name priorities)))
                                              (when (< prio best-prio)
                                                (setq best name)
                                                (setq best-prio prio))))
                                          (setq timeline (cons (cons t best) timeline))
                                          (puthash best (1- (gethash best remaining)) remaining)
                                          (when (= (gethash best remaining) 0)
                                            (puthash best (1+ t) completions)))
                                      ;; No process available, idle
                                      (setq timeline (cons (cons t 'idle) timeline)))
                                    ;; Check if all done
                                    (setq all-done t)
                                    (dolist (p processes)
                                      (when (> (gethash (nth 0 p) remaining) 0)
                                        (setq all-done nil))))
                                  (setq t (1+ t))))
                              ;; Build completion info
                              (let ((info nil))
                                (dolist (p processes)
                                  (let* ((name (nth 0 p))
                                         (arrival (nth 1 p))
                                         (burst (nth 2 p))
                                         (finish (gethash name completions 0))
                                         (turnaround (- finish arrival))
                                         (wait (- turnaround burst)))
                                    (setq info
                                          (cons (list name arrival burst finish
                                                      turnaround wait)
                                                info))))
                                ;; Compress timeline: group consecutive same-process runs
                                (let ((compressed nil)
                                      (run-start nil)
                                      (run-proc nil))
                                  (dolist (entry (nreverse timeline))
                                    (let ((time (car entry))
                                          (proc (cdr entry)))
                                      (if (eq proc run-proc)
                                          nil  ;; continue run
                                        (when run-proc
                                          (setq compressed
                                                (cons (list run-proc run-start time)
                                                      compressed)))
                                        (setq run-start time)
                                        (setq run-proc proc))))
                                  ;; Final run
                                  (when run-proc
                                    (let ((final-time (1+ (caar timeline))))
                                      (setq compressed
                                            (cons (list run-proc run-start final-time)
                                                  compressed))))
                                  (list (nreverse compressed)
                                        (nreverse info)))))))
                    (unwind-protect
                        (let ((processes '((P1 0 6 3)    ;; arrives at 0, burst 6, priority 3
                                           (P2 1 4 1)    ;; arrives at 1, burst 4, priority 1
                                           (P3 2 2 2)    ;; arrives at 2, burst 2, priority 2
                                           (P4 4 3 4)))) ;; arrives at 4, burst 3, priority 4
                          (let ((result (neovm--test-priority-schedule processes)))
                            (let ((timeline (nth 0 result))
                                  (info (nth 1 result)))
                              (list timeline info
                                    ;; Verify all processes completed
                                    (= (length info) (length processes))))))
                      (fmakunbound 'neovm--test-priority-schedule)))"#;
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
