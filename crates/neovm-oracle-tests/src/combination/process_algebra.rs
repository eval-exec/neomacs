//! Oracle parity tests for process algebra (CSP-like) concepts in Elisp.
//!
//! Implements: process representation, sequential composition, parallel
//! composition (interleaving), choice operator, channel communication,
//! process synchronization, deadlock detection, trace semantics,
//! hiding/restriction operator.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Process representation and sequential composition
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_process_algebra_sequential_composition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; A process is a list of events: (event1 event2 ... STOP)
  ;; STOP is the terminated process.
  ;; Sequential composition: P ; Q = concatenate events of P (minus STOP) with Q.

  (fset 'neovm--pa-make-proc
    (lambda (events)
      "Create a process from a list of events, ending with STOP."
      (append events '(STOP))))

  (fset 'neovm--pa-events
    (lambda (proc)
      "Return the event sequence (excluding terminal STOP)."
      (let ((result nil)
            (rest proc))
        (while (and rest (not (eq (car rest) 'STOP)))
          (push (car rest) result)
          (setq rest (cdr rest)))
        (nreverse result))))

  (fset 'neovm--pa-is-stopped
    (lambda (proc)
      "Return t if process is STOP or starts with STOP."
      (or (eq proc 'STOP) (and (consp proc) (eq (car proc) 'STOP)))))

  ;; Sequential composition: P >> Q runs P to completion, then Q
  (fset 'neovm--pa-seq
    (lambda (p q)
      "Sequential composition of processes P and Q."
      (let ((p-events (funcall 'neovm--pa-events p))
            (q-all q))
        (append p-events q-all))))

  ;; Step: consume one event from a process
  (fset 'neovm--pa-step
    (lambda (proc)
      "Return (event . remaining-process) or nil if stopped."
      (if (funcall 'neovm--pa-is-stopped proc)
          nil
        (cons (car proc) (cdr proc)))))

  ;; Execute all steps, collecting trace
  (fset 'neovm--pa-run
    (lambda (proc)
      "Run process to completion, return trace of events."
      (let ((trace nil)
            (current proc))
        (while (not (funcall 'neovm--pa-is-stopped current))
          (let ((s (funcall 'neovm--pa-step current)))
            (when s
              (push (car s) trace)
              (setq current (cdr s)))))
        (nreverse trace))))

  (unwind-protect
      (let ((p1 (funcall 'neovm--pa-make-proc '(a b c)))
            (p2 (funcall 'neovm--pa-make-proc '(x y)))
            (p3 (funcall 'neovm--pa-make-proc nil)))
        (list
         ;; Basic process events
         (funcall 'neovm--pa-events p1)
         (funcall 'neovm--pa-events p2)
         ;; Empty process
         (funcall 'neovm--pa-events p3)
         (funcall 'neovm--pa-is-stopped p3)
         ;; Sequential composition: p1 >> p2 = (a b c x y STOP)
         (funcall 'neovm--pa-run (funcall 'neovm--pa-seq p1 p2))
         ;; Sequential with empty: p3 >> p1 = p1
         (funcall 'neovm--pa-run (funcall 'neovm--pa-seq p3 p1))
         ;; Step-by-step execution
         (let* ((s1 (funcall 'neovm--pa-step p1))
                (s2 (funcall 'neovm--pa-step (cdr s1)))
                (s3 (funcall 'neovm--pa-step (cdr s2))))
           (list (car s1) (car s2) (car s3)
                 (funcall 'neovm--pa-is-stopped (cdr s3))))))
    (fmakunbound 'neovm--pa-make-proc)
    (fmakunbound 'neovm--pa-events)
    (fmakunbound 'neovm--pa-is-stopped)
    (fmakunbound 'neovm--pa-seq)
    (fmakunbound 'neovm--pa-step)
    (fmakunbound 'neovm--pa-run)))"#;
    let expect =
        expect_test::expect![[r#""OK ((a b c) (x y) nil t (a b c x y) (a b c) (a b c t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Parallel composition (interleaving semantics)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_process_algebra_parallel_interleaving() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Parallel composition via interleaving: generate all possible
  ;; interleavings of two event sequences. Result is set of traces.

  (fset 'neovm--pa2-interleave
    (lambda (xs ys)
      "Return all interleavings of lists XS and YS."
      (cond
       ((null xs) (list ys))
       ((null ys) (list xs))
       (t
        (let ((result nil))
          ;; Pick from xs first
          (let ((sub-xs (funcall 'neovm--pa2-interleave (cdr xs) ys)))
            (dolist (s sub-xs)
              (push (cons (car xs) s) result)))
          ;; Pick from ys first
          (let ((sub-ys (funcall 'neovm--pa2-interleave xs (cdr ys))))
            (dolist (s sub-ys)
              (push (cons (car ys) s) result)))
          (nreverse result))))))

  ;; Count total interleavings = C(m+n, m)
  (fset 'neovm--pa2-count-interleavings
    (lambda (m n)
      "Binomial coefficient C(m+n, m) by multiplicative formula."
      (let ((result 1)
            (i 0))
        (while (< i (min m n))
          (setq result (/ (* result (- (+ m n) i)) (1+ i)))
          (setq i (1+ i)))
        result)))

  (unwind-protect
      (list
       ;; Interleave (a) with (1): ((a 1) (1 a))
       (funcall 'neovm--pa2-interleave '(a) '(1))
       ;; Interleave (a b) with (1): 3 interleavings
       (let ((result (funcall 'neovm--pa2-interleave '(a b) '(1))))
         (list (length result) result))
       ;; Interleave empty with (x y)
       (funcall 'neovm--pa2-interleave nil '(x y))
       ;; Interleave (a b) with (1 2): C(4,2) = 6 interleavings
       (let ((result (funcall 'neovm--pa2-interleave '(a b) '(1 2))))
         (list (length result)
               (= (length result)
                  (funcall 'neovm--pa2-count-interleavings 2 2))))
       ;; C(3+2, 2) = 10
       (funcall 'neovm--pa2-count-interleavings 3 2)
       ;; All interleavings preserve element order within each sequence
       (let ((result (funcall 'neovm--pa2-interleave '(a b) '(1 2))))
         (let ((all-valid t))
           (dolist (trace result)
             ;; Check that a comes before b, and 1 comes before 2
             (let ((pos-a (length (memq 'a trace)))
                   (pos-b (length (memq 'b trace)))
                   (pos-1 (length (memq 1 trace)))
                   (pos-2 (length (memq 2 trace))))
               (unless (and (> pos-a pos-b) (> pos-1 pos-2))
                 (setq all-valid nil))))
           all-valid)))
    (fmakunbound 'neovm--pa2-interleave)
    (fmakunbound 'neovm--pa2-count-interleavings)))"#;
    let expect = expect_test::expect![[
        r#""OK (((a 1) (1 a)) (3 ((a b 1) (a 1 b) (1 a b))) ((x y)) (6 t) 10 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Choice (external choice) operator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_process_algebra_choice_operator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; External choice: P [] Q offers initial events of both P and Q.
  ;; Once the environment picks an event, the chosen branch continues.
  ;; We model this as a tree: (:choice branches) where each branch
  ;; is (:prefix event continuation).

  (fset 'neovm--pa3-prefix
    (lambda (event cont)
      "Event prefix: event -> continuation."
      (list :prefix event cont)))

  (fset 'neovm--pa3-stop
    (lambda () '(:stop)))

  (fset 'neovm--pa3-choice
    (lambda (branches)
      "External choice among BRANCHES (list of prefix nodes)."
      (cons :choice branches)))

  ;; Offered events (initial alphabet)
  (fset 'neovm--pa3-initials
    (lambda (proc)
      "Return the set of initially offered events."
      (cond
       ((eq (car proc) :stop) nil)
       ((eq (car proc) :prefix) (list (nth 1 proc)))
       ((eq (car proc) :choice)
        (let ((events nil))
          (dolist (branch (cdr proc))
            (setq events (append events (funcall 'neovm--pa3-initials branch))))
          events))
       (t nil))))

  ;; Resolve a choice by selecting an event
  (fset 'neovm--pa3-resolve
    (lambda (proc event)
      "Given an event selection, return the continuation process."
      (cond
       ((eq (car proc) :stop) nil)
       ((eq (car proc) :prefix)
        (if (eq (nth 1 proc) event)
            (nth 2 proc)
          nil))
       ((eq (car proc) :choice)
        (let ((found nil))
          (dolist (branch (cdr proc))
            (unless found
              (let ((r (funcall 'neovm--pa3-resolve branch event)))
                (when r (setq found r)))))
          found))
       (t nil))))

  ;; Run process with a predetermined event sequence
  (fset 'neovm--pa3-trace
    (lambda (proc events)
      "Execute PROC with EVENTS, returning accepted trace."
      (let ((trace nil)
            (current proc)
            (remaining events))
        (while (and remaining current (not (eq (car current) :stop)))
          (let ((evt (car remaining))
                (next (funcall 'neovm--pa3-resolve current (car remaining))))
            (if next
                (progn
                  (push evt trace)
                  (setq current next)
                  (setq remaining (cdr remaining)))
              (setq remaining nil))))  ;; blocked
        (nreverse trace))))

  (unwind-protect
      (let* ((stop (funcall 'neovm--pa3-stop))
             ;; P = a -> b -> STOP
             (p (funcall 'neovm--pa3-prefix 'a
                  (funcall 'neovm--pa3-prefix 'b stop)))
             ;; Q = c -> d -> STOP
             (q (funcall 'neovm--pa3-prefix 'c
                  (funcall 'neovm--pa3-prefix 'd stop)))
             ;; R = P [] Q (choice between P and Q)
             (r (funcall 'neovm--pa3-choice (list p q))))
        (list
         ;; Initials of P
         (funcall 'neovm--pa3-initials p)
         ;; Initials of Q
         (funcall 'neovm--pa3-initials q)
         ;; Initials of R (choice): both a and c offered
         (funcall 'neovm--pa3-initials r)
         ;; Trace with events (a b): follows P branch
         (funcall 'neovm--pa3-trace r '(a b))
         ;; Trace with events (c d): follows Q branch
         (funcall 'neovm--pa3-trace r '(c d))
         ;; Trace with events (a d): accepts a, then blocks on d
         (funcall 'neovm--pa3-trace r '(a d))
         ;; Trace with unrecognized event
         (funcall 'neovm--pa3-trace r '(z))))
    (fmakunbound 'neovm--pa3-prefix)
    (fmakunbound 'neovm--pa3-stop)
    (fmakunbound 'neovm--pa3-choice)
    (fmakunbound 'neovm--pa3-initials)
    (fmakunbound 'neovm--pa3-resolve)
    (fmakunbound 'neovm--pa3-trace)))"#;
    let expect = expect_test::expect![[r#""OK ((a) (c) (a c) (a b) (c d) (a) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Channel communication (synchronous rendezvous)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_process_algebra_channel_communication() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Model synchronous channels: a sender and receiver must rendezvous.
  ;; Channel state: (:channel name buffer sync-log)
  ;; Operations: send (blocks until matched), recv (blocks until matched),
  ;; sync (try to match a pending send with a pending recv).

  (fset 'neovm--pa4-make-channel
    (lambda (name)
      (list :channel name nil nil)))

  (fset 'neovm--pa4-chan-name
    (lambda (ch) (nth 1 ch)))

  (fset 'neovm--pa4-chan-pending
    (lambda (ch) (nth 2 ch)))

  (fset 'neovm--pa4-chan-log
    (lambda (ch) (nth 3 ch)))

  ;; Post a send request
  (fset 'neovm--pa4-send
    (lambda (ch val)
      (let ((pending (nth 2 ch)))
        (setcar (nthcdr 2 ch) (append pending (list (cons 'send val))))
        ch)))

  ;; Post a recv request
  (fset 'neovm--pa4-recv
    (lambda (ch)
      (let ((pending (nth 2 ch)))
        (setcar (nthcdr 2 ch) (append pending (list (cons 'recv nil))))
        ch)))

  ;; Synchronize: match first send with first recv
  (fset 'neovm--pa4-sync
    (lambda (ch)
      (let ((pending (nth 2 ch))
            (first-send nil) (first-recv nil)
            (send-pos -1) (recv-pos -1)
            (i 0))
        ;; Find first send and first recv
        (dolist (item pending)
          (when (and (eq (car item) 'send) (< send-pos 0))
            (setq first-send item send-pos i))
          (when (and (eq (car item) 'recv) (< recv-pos 0))
            (setq first-recv item recv-pos i))
          (setq i (1+ i)))
        (if (and first-send first-recv)
            ;; Match found: transfer value, log it, remove both
            (let ((val (cdr first-send))
                  (new-pending nil)
                  (j 0))
              (dolist (item pending)
                (unless (or (= j send-pos) (= j recv-pos))
                  (push item new-pending))
                (setq j (1+ j)))
              (setcar (nthcdr 2 ch) (nreverse new-pending))
              (setcar (nthcdr 3 ch)
                      (append (nth 3 ch)
                              (list (list 'synced (funcall 'neovm--pa4-chan-name ch) val))))
              (cons 'matched val))
          'blocked))))

  (unwind-protect
      (let ((ch1 (funcall 'neovm--pa4-make-channel 'pipe)))
        ;; Send then recv then sync
        (funcall 'neovm--pa4-send ch1 42)
        (funcall 'neovm--pa4-recv ch1)
        (let ((r1 (funcall 'neovm--pa4-sync ch1)))
          ;; Channel should be clear now
          (let ((remaining (funcall 'neovm--pa4-chan-pending ch1))
                (log1 (funcall 'neovm--pa4-chan-log ch1)))
            ;; Sync without matching pair
            (funcall 'neovm--pa4-send ch1 99)
            (let ((r2 (funcall 'neovm--pa4-sync ch1)))
              ;; Now add recv and sync again
              (funcall 'neovm--pa4-recv ch1)
              (let ((r3 (funcall 'neovm--pa4-sync ch1)))
                (list
                 r1           ;; (matched . 42)
                 remaining    ;; nil (both consumed)
                 log1         ;; ((synced pipe 42))
                 r2           ;; blocked (no recv)
                 r3           ;; (matched . 99)
                 (funcall 'neovm--pa4-chan-log ch1)))))))
    (fmakunbound 'neovm--pa4-make-channel)
    (fmakunbound 'neovm--pa4-chan-name)
    (fmakunbound 'neovm--pa4-chan-pending)
    (fmakunbound 'neovm--pa4-chan-log)
    (fmakunbound 'neovm--pa4-send)
    (fmakunbound 'neovm--pa4-recv)
    (fmakunbound 'neovm--pa4-sync)))"#;
    let expect = expect_test::expect![[
        r#""OK ((matched . 42) nil ((synced pipe 42)) blocked (matched . 99) ((synced pipe 42) (synced pipe 99)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Deadlock detection in process network
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_process_algebra_deadlock_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Model a network of processes that wait on channels.
  ;; Deadlock occurs when there is a circular wait dependency.
  ;; We use a wait-for graph and check for cycles.

  ;; Build a wait-for graph: alist of (process . waiting-for-process)
  (fset 'neovm--pa5-make-graph
    (lambda (edges)
      "EDGES is a list of (from . to) pairs."
      edges))

  ;; Find all processes reachable from START via the wait-for graph
  (fset 'neovm--pa5-reachable
    (lambda (graph start)
      (let ((visited nil)
            (stack (list start)))
        (while stack
          (let ((node (car stack)))
            (setq stack (cdr stack))
            (unless (memq node visited)
              (push node visited)
              ;; Add all successors
              (dolist (edge graph)
                (when (eq (car edge) node)
                  (push (cdr edge) stack))))))
        visited)))

  ;; Detect cycle: a process is in a deadlock if it can reach itself
  (fset 'neovm--pa5-has-cycle-from
    (lambda (graph start)
      "Check if there is a cycle starting from START."
      (let ((visited nil)
            (stack nil))
        ;; Get direct successors of start
        (dolist (edge graph)
          (when (eq (car edge) start)
            (push (cdr edge) stack)))
        ;; BFS/DFS to see if we reach start again
        (let ((found nil))
          (while (and stack (not found))
            (let ((node (car stack)))
              (setq stack (cdr stack))
              (cond
               ((eq node start) (setq found t))
               ((memq node visited) nil)
               (t
                (push node visited)
                (dolist (edge graph)
                  (when (eq (car edge) node)
                    (push (cdr edge) stack)))))))
          found))))

  ;; Check if entire system is deadlocked (all processes in a cycle)
  (fset 'neovm--pa5-detect-deadlock
    (lambda (graph processes)
      "Return list of processes involved in deadlock cycles."
      (let ((deadlocked nil))
        (dolist (p processes)
          (when (funcall 'neovm--pa5-has-cycle-from graph p)
            (push p deadlocked)))
        (nreverse deadlocked))))

  (unwind-protect
      (list
       ;; No deadlock: A waits for B, B waits for C (no cycle)
       (funcall 'neovm--pa5-detect-deadlock
                (funcall 'neovm--pa5-make-graph '((A . B) (B . C)))
                '(A B C))
       ;; Simple deadlock: A waits for B, B waits for A
       (funcall 'neovm--pa5-detect-deadlock
                (funcall 'neovm--pa5-make-graph '((A . B) (B . A)))
                '(A B))
       ;; Three-way deadlock: A->B->C->A
       (funcall 'neovm--pa5-detect-deadlock
                (funcall 'neovm--pa5-make-graph '((A . B) (B . C) (C . A)))
                '(A B C))
       ;; Partial deadlock: A->B->A cycle, C is free
       (funcall 'neovm--pa5-detect-deadlock
                (funcall 'neovm--pa5-make-graph '((A . B) (B . A) (C . D)))
                '(A B C D))
       ;; Reachability from A in A->B->C
       (funcall 'neovm--pa5-reachable
                '((A . B) (B . C))
                'A)
       ;; No edges: no cycles
       (funcall 'neovm--pa5-detect-deadlock nil '(X Y Z)))
    (fmakunbound 'neovm--pa5-make-graph)
    (fmakunbound 'neovm--pa5-reachable)
    (fmakunbound 'neovm--pa5-has-cycle-from)
    (fmakunbound 'neovm--pa5-detect-deadlock)))"#;
    let expect = expect_test::expect![[r#""OK (nil (A B) (A B C) (A B) (C B A) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Trace semantics and trace refinement
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_process_algebra_trace_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Trace semantics: a process is characterized by the set of all
  ;; possible traces (sequences of events). P refines Q iff
  ;; traces(P) ⊆ traces(Q).

  ;; A process as a tree: (:node event children) or (:stop)
  ;; Traces are extracted by DFS through the tree.

  (fset 'neovm--pa6-stop (lambda () '(:stop)))

  (fset 'neovm--pa6-node
    (lambda (event children)
      (list :node event children)))

  ;; Extract all complete traces from a process tree
  (fset 'neovm--pa6-all-traces
    (lambda (proc)
      "Return list of all possible traces (each trace is a list of events)."
      (if (eq (car proc) :stop)
          '(())  ;; One trace: the empty trace
        (let ((event (nth 1 proc))
              (children (nth 2 proc))
              (result nil))
          (dolist (child children)
            (let ((sub-traces (funcall 'neovm--pa6-all-traces child)))
              (dolist (trace sub-traces)
                (push (cons event trace) result))))
          (nreverse result)))))

  ;; Check if trace-set A is a subset of trace-set B
  (fset 'neovm--pa6-trace-subset
    (lambda (set-a set-b)
      (let ((all-in t))
        (dolist (trace set-a)
          (unless (member trace set-b)
            (setq all-in nil)))
        all-in)))

  ;; Prefix closure: all prefixes of all traces
  (fset 'neovm--pa6-prefixes
    (lambda (trace)
      "Return all prefixes of TRACE (including empty and TRACE itself)."
      (let ((result (list nil))
            (prefix nil)
            (rest trace))
        (while rest
          (setq prefix (append prefix (list (car rest))))
          (push (copy-sequence prefix) result)
          (setq rest (cdr rest)))
        (nreverse result))))

  (unwind-protect
      (let* ((stop (funcall 'neovm--pa6-stop))
             ;; P = a -> (b -> STOP | c -> STOP)
             (p (funcall 'neovm--pa6-node 'a
                  (list (funcall 'neovm--pa6-node 'b (list stop))
                        (funcall 'neovm--pa6-node 'c (list stop)))))
             ;; Q = a -> b -> STOP (deterministic, single path)
             (q (funcall 'neovm--pa6-node 'a
                  (list (funcall 'neovm--pa6-node 'b (list stop)))))
             ;; R = a -> (b -> STOP | c -> STOP | d -> STOP)
             (r (funcall 'neovm--pa6-node 'a
                  (list (funcall 'neovm--pa6-node 'b (list stop))
                        (funcall 'neovm--pa6-node 'c (list stop))
                        (funcall 'neovm--pa6-node 'd (list stop))))))
        (list
         ;; Traces of P: ((a b) (a c))
         (funcall 'neovm--pa6-all-traces p)
         ;; Traces of Q: ((a b))
         (funcall 'neovm--pa6-all-traces q)
         ;; Traces of R: ((a b) (a c) (a d))
         (funcall 'neovm--pa6-all-traces r)
         ;; Q refines P? traces(Q) ⊆ traces(P) => t
         (funcall 'neovm--pa6-trace-subset
                  (funcall 'neovm--pa6-all-traces q)
                  (funcall 'neovm--pa6-all-traces p))
         ;; P refines Q? traces(P) ⊆ traces(Q) => nil (P has (a c))
         (funcall 'neovm--pa6-trace-subset
                  (funcall 'neovm--pa6-all-traces p)
                  (funcall 'neovm--pa6-all-traces q))
         ;; P refines R? traces(P) ⊆ traces(R) => t
         (funcall 'neovm--pa6-trace-subset
                  (funcall 'neovm--pa6-all-traces p)
                  (funcall 'neovm--pa6-all-traces r))
         ;; Prefixes of (a b c)
         (funcall 'neovm--pa6-prefixes '(a b c))))
    (fmakunbound 'neovm--pa6-stop)
    (fmakunbound 'neovm--pa6-node)
    (fmakunbound 'neovm--pa6-all-traces)
    (fmakunbound 'neovm--pa6-trace-subset)
    (fmakunbound 'neovm--pa6-prefixes)))"#;
    let expect = expect_test::expect![[
        r#""OK (((a b) (a c)) ((a b)) ((a b) (a c) (a d)) t nil t (nil (a) (a b) (a b c)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Hiding / restriction operator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_process_algebra_hiding_restriction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Hiding operator: P \ H turns events in set H into internal (tau) events.
  ;; Restriction operator: P ↾ A keeps only events in set A (blocks others).
  ;; We operate on traces (lists of events).

  ;; Hide: replace events in HIDDEN-SET with 'tau
  (fset 'neovm--pa7-hide-trace
    (lambda (trace hidden-set)
      "Replace events in HIDDEN-SET with tau in TRACE."
      (mapcar (lambda (evt)
                (if (memq evt hidden-set) 'tau evt))
              trace)))

  ;; Restrict: keep only events in ALLOWED-SET (remove others entirely)
  (fset 'neovm--pa7-restrict-trace
    (lambda (trace allowed-set)
      "Keep only events in ALLOWED-SET from TRACE."
      (let ((result nil))
        (dolist (evt trace)
          (when (memq evt allowed-set)
            (push evt result)))
        (nreverse result))))

  ;; Remove consecutive tau events (tau abstraction)
  (fset 'neovm--pa7-abstract-tau
    (lambda (trace)
      "Remove consecutive duplicate tau events."
      (let ((result nil)
            (prev nil))
        (dolist (evt trace)
          (unless (and (eq evt 'tau) (eq prev 'tau))
            (push evt result))
          (setq prev evt))
        (nreverse result))))

  ;; Alphabet: set of non-tau events in a trace
  (fset 'neovm--pa7-alphabet
    (lambda (trace)
      "Return sorted unique non-tau events."
      (let ((seen nil))
        (dolist (evt trace)
          (unless (or (eq evt 'tau) (memq evt seen))
            (push evt seen)))
        (sort seen (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))

  (unwind-protect
      (let ((trace1 '(a b c d e)))
        (list
         ;; Hide {b, d}: a tau c tau e
         (funcall 'neovm--pa7-hide-trace trace1 '(b d))
         ;; Hide empty set: no change
         (funcall 'neovm--pa7-hide-trace trace1 nil)
         ;; Hide everything: all tau
         (funcall 'neovm--pa7-hide-trace trace1 '(a b c d e))
         ;; Restrict to {a, c, e}: (a c e)
         (funcall 'neovm--pa7-restrict-trace trace1 '(a c e))
         ;; Restrict to empty: ()
         (funcall 'neovm--pa7-restrict-trace trace1 nil)
         ;; Tau abstraction: (tau tau a tau tau b) -> (tau a tau b)
         (funcall 'neovm--pa7-abstract-tau '(tau tau a tau tau b))
         ;; Alphabet of a trace with tau
         (funcall 'neovm--pa7-alphabet '(a tau b tau c a b))
         ;; Compose hiding then restriction
         (let ((hidden (funcall 'neovm--pa7-hide-trace '(a b c d) '(b c))))
           (funcall 'neovm--pa7-restrict-trace hidden '(a d tau)))))
    (fmakunbound 'neovm--pa7-hide-trace)
    (fmakunbound 'neovm--pa7-restrict-trace)
    (fmakunbound 'neovm--pa7-abstract-tau)
    (fmakunbound 'neovm--pa7-alphabet)))"#;
    let expect = expect_test::expect![[
        r#""OK ((a tau c tau e) (a b c d e) (tau tau tau tau tau) (a c e) nil (tau a tau b) (a b c) (a tau tau d))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
