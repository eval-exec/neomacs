//! Oracle parity tests for protocol verification framework.
//!
//! Implements protocol verification patterns: protocol states as symbols,
//! transitions as (state event guard next-state action) tuples, reachability
//! analysis via BFS, safety property checking (bad states unreachable),
//! liveness checking, trace generation, sequence diagram extraction,
//! deadlock-freedom verification, protocol composition, bisimulation checking.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Protocol definition and single-step transition
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_protocol_definition_and_step() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Protocol: list of (from-state event guard-fn next-state action-fn)
  ;; guard-fn: (lambda (ctx) -> bool), nil means always true
  ;; action-fn: (lambda (ctx) -> ctx-modified), nil means no-op

  (fset 'neovm--pv-make-protocol
    (lambda (transitions initial-state)
      (list 'protocol transitions initial-state)))

  (fset 'neovm--pv-transitions (lambda (proto) (nth 1 proto)))
  (fset 'neovm--pv-initial (lambda (proto) (nth 2 proto)))

  ;; Find matching transition for (state, event) with guard
  (fset 'neovm--pv-find-transition
    (lambda (proto state event ctx)
      (let ((result nil)
            (trs (funcall 'neovm--pv-transitions proto)))
        (while (and trs (not result))
          (let ((tr (car trs)))
            (when (and (eq (nth 0 tr) state)
                       (eq (nth 1 tr) event)
                       (or (null (nth 2 tr))
                           (funcall (nth 2 tr) ctx)))
              (setq result tr)))
          (setq trs (cdr trs)))
        result)))

  ;; Single step: returns (new-state . new-ctx) or nil if no transition
  (fset 'neovm--pv-step
    (lambda (proto state event ctx)
      (let ((tr (funcall 'neovm--pv-find-transition proto state event ctx)))
        (when tr
          (when (nth 4 tr)
            (funcall (nth 4 tr) ctx))
          (cons (nth 3 tr) ctx)))))

  (unwind-protect
      (let ((proto (funcall 'neovm--pv-make-protocol
                     (list
                       (list 'idle 'start nil 'running nil)
                       (list 'running 'pause nil 'paused nil)
                       (list 'paused 'resume nil 'running nil)
                       (list 'running 'stop nil 'stopped nil)
                       (list 'paused 'stop nil 'stopped nil))
                     'idle)))
        (let ((ctx (make-hash-table)))
          (list
            ;; Step from idle on start
            (car (funcall 'neovm--pv-step proto 'idle 'start ctx))
            ;; Step from running on pause
            (car (funcall 'neovm--pv-step proto 'running 'pause ctx))
            ;; Step from paused on resume
            (car (funcall 'neovm--pv-step proto 'paused 'resume ctx))
            ;; Step from running on stop
            (car (funcall 'neovm--pv-step proto 'running 'stop ctx))
            ;; Invalid transition returns nil
            (funcall 'neovm--pv-step proto 'idle 'pause ctx)
            (funcall 'neovm--pv-step proto 'stopped 'start ctx)
            ;; Step with actions
            (let ((proto2 (funcall 'neovm--pv-make-protocol
                            (list
                              (list 'init 'go nil 'done
                                    (lambda (ctx) (puthash 'visited t ctx))))
                            'init)))
              (let ((c2 (make-hash-table)))
                (funcall 'neovm--pv-step proto2 'init 'go c2)
                (gethash 'visited c2))))))
    (fmakunbound 'neovm--pv-make-protocol)
    (fmakunbound 'neovm--pv-transitions)
    (fmakunbound 'neovm--pv-initial)
    (fmakunbound 'neovm--pv-find-transition)
    (fmakunbound 'neovm--pv-step)))"#;
    let expect = expect_test::expect![[r#""OK (running paused running stopped nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Reachability analysis via BFS
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_protocol_reachability_bfs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Collect all states reachable from initial state via BFS
  ;; Simplified: ignore guards, just look at (from-state -> next-state) edges
  (fset 'neovm--pv-reachable-states
    (lambda (transitions initial)
      (let ((visited (list initial))
            (queue (list initial)))
        (while queue
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (dolist (tr transitions)
              (when (eq (nth 0 tr) current)
                (let ((next (nth 3 tr)))
                  (unless (memq next visited)
                    (setq visited (cons next visited))
                    (setq queue (append queue (list next)))))))))
        (sort visited (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))

  ;; Collect all events possible from a given state
  (fset 'neovm--pv-events-from
    (lambda (transitions state)
      (let ((events nil))
        (dolist (tr transitions)
          (when (and (eq (nth 0 tr) state)
                     (not (memq (nth 1 tr) events)))
            (setq events (cons (nth 1 tr) events))))
        (sort events (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))

  (unwind-protect
      (let ((trs (list
                   (list 'idle 'connect nil 'connecting nil)
                   (list 'connecting 'success nil 'connected nil)
                   (list 'connecting 'fail nil 'idle nil)
                   (list 'connected 'send nil 'sending nil)
                   (list 'sending 'ack nil 'connected nil)
                   (list 'sending 'timeout nil 'error nil)
                   (list 'error 'retry nil 'connecting nil)
                   (list 'error 'abort nil 'idle nil)
                   (list 'connected 'close nil 'closing nil)
                   (list 'closing 'done nil 'idle nil))))
        (list
          ;; All reachable states from 'idle
          (funcall 'neovm--pv-reachable-states trs 'idle)
          ;; Reachable from 'connecting (subset)
          (funcall 'neovm--pv-reachable-states trs 'connecting)
          ;; Events from each state
          (funcall 'neovm--pv-events-from trs 'idle)
          (funcall 'neovm--pv-events-from trs 'connecting)
          (funcall 'neovm--pv-events-from trs 'connected)
          (funcall 'neovm--pv-events-from trs 'sending)
          (funcall 'neovm--pv-events-from trs 'error)
          (funcall 'neovm--pv-events-from trs 'closing)
          ;; Unreachable test: create a disconnected state
          (let ((trs2 (append trs (list (list 'orphan 'go nil 'orphan2 nil)))))
            ;; orphan and orphan2 should NOT be reachable from idle
            (let ((reach (funcall 'neovm--pv-reachable-states trs2 'idle)))
              (list (memq 'orphan reach)
                    (memq 'orphan2 reach))))))
    (fmakunbound 'neovm--pv-reachable-states)
    (fmakunbound 'neovm--pv-events-from)))"#;
    let expect = expect_test::expect![[
        r#""OK ((closing connected connecting error idle sending) (closing connected connecting error idle sending) (connect) (fail success) (close send) (ack timeout) (abort retry) (done) (nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Safety property checking: bad states unreachable
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_protocol_safety_checking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--pv-reachable
    (lambda (transitions initial)
      (let ((visited (list initial))
            (queue (list initial)))
        (while queue
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (dolist (tr transitions)
              (when (eq (nth 0 tr) current)
                (let ((next (nth 3 tr)))
                  (unless (memq next visited)
                    (setq visited (cons next visited))
                    (setq queue (append queue (list next)))))))))
        visited)))

  ;; Check safety: none of the bad-states are reachable
  (fset 'neovm--pv-check-safety
    (lambda (transitions initial bad-states)
      (let ((reachable (funcall 'neovm--pv-reachable transitions initial))
            (violations nil))
        (dolist (bad bad-states)
          (when (memq bad reachable)
            (setq violations (cons bad violations))))
        (list 'safe (null violations)
              'violations (nreverse violations)
              'reachable-count (length reachable)))))

  (unwind-protect
      (list
        ;; Safe protocol: no bad states reachable
        (funcall 'neovm--pv-check-safety
          (list
            (list 'init 'start nil 'running nil)
            (list 'running 'complete nil 'done nil)
            (list 'running 'fail nil 'error nil)
            (list 'error 'recover nil 'init nil))
          'init
          '(crashed corrupted))

        ;; Unsafe protocol: error leads to corrupted
        (funcall 'neovm--pv-check-safety
          (list
            (list 'init 'start nil 'running nil)
            (list 'running 'fail nil 'error nil)
            (list 'error 'cascade nil 'corrupted nil))
          'init
          '(crashed corrupted))

        ;; Multiple bad states reachable
        (funcall 'neovm--pv-check-safety
          (list
            (list 'init 'go nil 'a nil)
            (list 'a 'x nil 'bad1 nil)
            (list 'a 'y nil 'b nil)
            (list 'b 'z nil 'bad2 nil))
          'init
          '(bad1 bad2 bad3))

        ;; Empty protocol: trivially safe
        (funcall 'neovm--pv-check-safety nil 'init '(bad))

        ;; All states are bad except init
        (funcall 'neovm--pv-check-safety
          (list
            (list 'init 'go nil 'a nil)
            (list 'a 'next nil 'b nil))
          'init
          '(a b)))
    (fmakunbound 'neovm--pv-reachable)
    (fmakunbound 'neovm--pv-check-safety)))"#;
    let expect = expect_test::expect![[
        r#""OK ((safe t violations nil reachable-count 4) (safe nil violations (corrupted) reachable-count 4) (safe nil violations (bad1 bad2) reachable-count 5) (safe t violations nil reachable-count 1) (safe nil violations (a b) reachable-count 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Deadlock freedom: every non-terminal state has at least one outgoing event
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_protocol_deadlock_freedom() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Check deadlock freedom: every reachable non-terminal state has outgoing transitions
  (fset 'neovm--pv-check-deadlock-free
    (lambda (transitions initial terminal-states)
      (let ((visited (list initial))
            (queue (list initial))
            (deadlocks nil))
        ;; BFS to find all reachable states
        (while queue
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (dolist (tr transitions)
              (when (eq (nth 0 tr) current)
                (let ((next (nth 3 tr)))
                  (unless (memq next visited)
                    (setq visited (cons next visited))
                    (setq queue (append queue (list next)))))))))
        ;; Check each reachable non-terminal state
        (dolist (s visited)
          (unless (memq s terminal-states)
            (let ((has-outgoing nil))
              (dolist (tr transitions)
                (when (eq (nth 0 tr) s)
                  (setq has-outgoing t)))
              (unless has-outgoing
                (setq deadlocks (cons s deadlocks))))))
        (list 'deadlock-free (null deadlocks)
              'deadlocked-states (sort (copy-sequence deadlocks)
                                       (lambda (a b) (string< (symbol-name a) (symbol-name b))))
              'total-states (length visited)
              'terminal-states (length (let ((ts nil))
                                         (dolist (s visited)
                                           (when (memq s terminal-states)
                                             (setq ts (cons s ts))))
                                         ts))))))

  (unwind-protect
      (list
        ;; Deadlock-free protocol
        (funcall 'neovm--pv-check-deadlock-free
          (list
            (list 'init 'start nil 'running nil)
            (list 'running 'done nil 'finished nil))
          'init
          '(finished))

        ;; Protocol with deadlock: 'stuck has no outgoing transitions
        (funcall 'neovm--pv-check-deadlock-free
          (list
            (list 'init 'go nil 'a nil)
            (list 'a 'x nil 'b nil)
            (list 'a 'y nil 'stuck nil)
            (list 'b 'done nil 'end nil))
          'init
          '(end))

        ;; Multiple deadlocks
        (funcall 'neovm--pv-check-deadlock-free
          (list
            (list 'start 'a nil 'p nil)
            (list 'start 'b nil 'q nil)
            (list 'p 'c nil 'dead1 nil)
            (list 'q 'd nil 'dead2 nil))
          'start
          '())

        ;; All states terminal: trivially deadlock-free
        (funcall 'neovm--pv-check-deadlock-free
          (list
            (list 'init 'go nil 'done nil))
          'init
          '(init done))

        ;; Complex: circular with escape
        (funcall 'neovm--pv-check-deadlock-free
          (list
            (list 'a 'tick nil 'b nil)
            (list 'b 'tick nil 'c nil)
            (list 'c 'tick nil 'a nil)
            (list 'c 'exit nil 'done nil))
          'a
          '(done)))
    (fmakunbound 'neovm--pv-check-deadlock-free)))"#;
    let expect = expect_test::expect![[
        r#""OK ((deadlock-free t deadlocked-states nil total-states 3 terminal-states 1) (deadlock-free nil deadlocked-states (stuck) total-states 5 terminal-states 1) (deadlock-free nil deadlocked-states (dead1 dead2) total-states 5 terminal-states 0) (deadlock-free t deadlocked-states nil total-states 2 terminal-states 2) (deadlock-free t deadlocked-states nil total-states 4 terminal-states 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Trace generation: run event sequence and record transitions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_protocol_trace_generation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Run a list of events through a protocol and record the trace
  (fset 'neovm--pv-run-trace
    (lambda (transitions initial events)
      (let ((state initial)
            (trace nil)
            (step 0))
        (dolist (ev events)
          (setq step (1+ step))
          (let ((found nil))
            (dolist (tr transitions)
              (when (and (not found)
                         (eq (nth 0 tr) state)
                         (eq (nth 1 tr) ev))
                (setq found tr)))
            (if found
                (let ((next (nth 3 found)))
                  (setq trace (cons (list step state ev next 'ok) trace))
                  (setq state next))
              (setq trace (cons (list step state ev nil 'rejected) trace)))))
        (list 'final-state state
              'trace (nreverse trace)
              'steps step
              'accepted (let ((ok-count 0))
                          (dolist (t1 trace)
                            (when (eq (nth 4 t1) 'ok) (setq ok-count (1+ ok-count))))
                          ok-count)
              'rejected (let ((rej-count 0))
                          (dolist (t1 trace)
                            (when (eq (nth 4 t1) 'rejected) (setq rej-count (1+ rej-count))))
                          rej-count)))))

  (unwind-protect
      (let ((trs (list
                   (list 'idle 'connect nil 'connecting nil)
                   (list 'connecting 'ok nil 'ready nil)
                   (list 'connecting 'fail nil 'idle nil)
                   (list 'ready 'send nil 'busy nil)
                   (list 'busy 'ack nil 'ready nil)
                   (list 'ready 'close nil 'idle nil))))
        (list
          ;; Happy path
          (funcall 'neovm--pv-run-trace trs 'idle
                   '(connect ok send ack send ack close))
          ;; Path with rejection
          (funcall 'neovm--pv-run-trace trs 'idle
                   '(connect ok send send))  ;; second send rejected (in busy)
          ;; Connection failure then retry
          (funcall 'neovm--pv-run-trace trs 'idle
                   '(connect fail connect ok close))
          ;; All rejected (wrong events)
          (funcall 'neovm--pv-run-trace trs 'idle
                   '(ok fail ack close))
          ;; Empty event list
          (funcall 'neovm--pv-run-trace trs 'idle '())))
    (fmakunbound 'neovm--pv-run-trace)))"#;
    let expect = expect_test::expect![[
        r#""OK ((final-state idle trace ((1 idle connect connecting ok) (2 connecting ok ready ok) (3 ready send busy ok) (4 busy ack ready ok) (5 ready send busy ok) (6 busy ack ready ok) (7 ready close idle ok)) steps 7 accepted 1 rejected 0) (final-state busy trace ((1 idle connect connecting ok) (2 connecting ok ready ok) (3 ready send busy ok) (4 busy send nil rejected)) steps 4 accepted 0 rejected 1) (final-state idle trace ((1 idle connect connecting ok) (2 connecting fail idle ok) (3 idle connect connecting ok) (4 connecting ok ready ok) (5 ready close idle ok)) steps 5 accepted 1 rejected 0) (final-state idle trace ((1 idle ok nil rejected) (2 idle fail nil rejected) (3 idle ack nil rejected) (4 idle close nil rejected)) steps 4 accepted 0 rejected 1) (final-state idle trace nil steps 0 accepted 0 rejected 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sequence diagram extraction from trace
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_protocol_sequence_diagram() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Generate a text-based sequence diagram from a message trace.
  ;; Trace entries: (tick sender receiver message)
  (fset 'neovm--pv-sequence-diagram
    (lambda (actors trace)
      (let ((lines nil))
        ;; Header: actor names
        (setq lines (cons (mapconcat #'symbol-name actors "    ") lines))
        ;; Separator
        (setq lines (cons (make-string (* (length actors) 8) ?-) lines))
        ;; For each message, create a line showing the arrow
        (dolist (entry trace)
          (let* ((tick (nth 0 entry))
                 (sender (nth 1 entry))
                 (receiver (nth 2 entry))
                 (msg (nth 3 entry))
                 (sender-idx (let ((i 0) (found nil))
                               (dolist (a actors)
                                 (when (eq a sender) (setq found i))
                                 (setq i (1+ i)))
                               found))
                 (receiver-idx (let ((i 0) (found nil))
                                 (dolist (a actors)
                                   (when (eq a receiver) (setq found i))
                                   (setq i (1+ i)))
                                 found)))
            (when (and sender-idx receiver-idx)
              (setq lines
                    (cons (format "  %d: %s -> %s : %s"
                                  tick
                                  (symbol-name sender)
                                  (symbol-name receiver)
                                  (symbol-name msg))
                          lines)))))
        (mapconcat #'identity (nreverse lines) "\n"))))

  (unwind-protect
      (list
        ;; Simple client-server exchange
        (funcall 'neovm--pv-sequence-diagram
          '(client server)
          '((1 client server syn)
            (2 server client syn-ack)
            (3 client server ack)
            (4 client server data)
            (5 server client data-ack)
            (6 client server fin)
            (7 server client fin-ack)))

        ;; Three-party protocol
        (funcall 'neovm--pv-sequence-diagram
          '(alice bob carol)
          '((1 alice bob hello)
            (2 bob alice hello-back)
            (3 alice carol invite)
            (4 carol alice accept)
            (5 carol bob greeting)
            (6 bob carol greeting-back)))

        ;; Empty trace
        (funcall 'neovm--pv-sequence-diagram '(a b) nil)

        ;; Single message
        (funcall 'neovm--pv-sequence-diagram
          '(sender receiver)
          '((1 sender receiver ping))))
    (fmakunbound 'neovm--pv-sequence-diagram)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"client    server\\n----------------\\n  1: client -> server : syn\\n  2: server -> client : syn-ack\\n  3: client -> server : ack\\n  4: client -> server : data\\n  5: server -> client : data-ack\\n  6: client -> server : fin\\n  7: server -> client : fin-ack\" \"alice    bob    carol\\n------------------------\\n  1: alice -> bob : hello\\n  2: bob -> alice : hello-back\\n  3: alice -> carol : invite\\n  4: carol -> alice : accept\\n  5: carol -> bob : greeting\\n  6: bob -> carol : greeting-back\" \"a    b\\n----------------\" \"sender    receiver\\n----------------\\n  1: sender -> receiver : ping\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Liveness checking: desired states eventually reachable
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_protocol_liveness_checking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Check liveness: from every reachable non-terminal state,
  ;; at least one of the goal states must be reachable.
  (fset 'neovm--pv-check-liveness
    (lambda (transitions initial goal-states terminal-states)
      (let ((all-reachable nil)
            (queue (list initial))
            (visited (list initial)))
        ;; BFS from initial
        (while queue
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (dolist (tr transitions)
              (when (eq (nth 0 tr) current)
                (let ((next (nth 3 tr)))
                  (unless (memq next visited)
                    (setq visited (cons next visited))
                    (setq queue (append queue (list next)))))))))
        (setq all-reachable visited)
        ;; For each non-terminal reachable state, check if a goal is reachable from it
        (let ((violations nil))
          (dolist (s all-reachable)
            (unless (memq s terminal-states)
              ;; BFS from s
              (let ((sub-visited (list s))
                    (sub-queue (list s))
                    (found-goal nil))
                (while (and sub-queue (not found-goal))
                  (let ((cur (car sub-queue)))
                    (setq sub-queue (cdr sub-queue))
                    (when (memq cur goal-states)
                      (setq found-goal t))
                    (dolist (tr transitions)
                      (when (eq (nth 0 tr) cur)
                        (let ((next (nth 3 tr)))
                          (unless (memq next sub-visited)
                            (setq sub-visited (cons next sub-visited))
                            (setq sub-queue (append sub-queue (list next)))))))))
                (unless found-goal
                  (setq violations (cons s violations))))))
          (list 'live (null violations)
                'violations (sort (copy-sequence violations)
                                  (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))))

  (unwind-protect
      (list
        ;; Live protocol: every state can reach 'done
        (funcall 'neovm--pv-check-liveness
          (list
            (list 'init 'go nil 'working nil)
            (list 'working 'progress nil 'working nil)
            (list 'working 'finish nil 'done nil))
          'init '(done) '(done))

        ;; Not live: 'stuck cannot reach any goal
        (funcall 'neovm--pv-check-liveness
          (list
            (list 'init 'a nil 'working nil)
            (list 'init 'b nil 'stuck nil)
            (list 'working 'finish nil 'done nil)
            (list 'stuck 'loop nil 'stuck nil))
          'init '(done) '(done))

        ;; Circular but live: A->B->C->A, C->done
        (funcall 'neovm--pv-check-liveness
          (list
            (list 'a 'x nil 'b nil)
            (list 'b 'x nil 'c nil)
            (list 'c 'x nil 'a nil)
            (list 'c 'y nil 'done nil))
          'a '(done) '(done))

        ;; Multiple goals: at least one must be reachable
        (funcall 'neovm--pv-check-liveness
          (list
            (list 'init 'go nil 'branch nil)
            (list 'branch 'left nil 'goal1 nil)
            (list 'branch 'right nil 'goal2 nil))
          'init '(goal1 goal2) '(goal1 goal2)))
    (fmakunbound 'neovm--pv-check-liveness)))"#;
    let expect = expect_test::expect![[
        r#""OK ((live t violations nil) (live nil violations (stuck)) (live t violations nil) (live t violations nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Protocol composition: parallel product of two protocols
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_protocol_composition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Parallel product: two protocols run concurrently,
  ;; synchronizing on shared events.
  ;; State is (state1 . state2). An event fires if BOTH can take it (shared)
  ;; or exactly one can take it (independent).
  (fset 'neovm--pv-compose
    (lambda (trs1 init1 trs2 init2 shared-events)
      (let ((visited nil)
            (queue (list (cons init1 init2)))
            (product-trs nil))
        (setq visited (list (cons init1 init2)))
        (while queue
          (let* ((pair (car queue))
                 (s1 (car pair))
                 (s2 (cdr pair)))
            (setq queue (cdr queue))
            ;; Shared events: both must participate
            (dolist (ev shared-events)
              (dolist (t1 trs1)
                (when (and (eq (nth 0 t1) s1) (eq (nth 1 t1) ev))
                  (dolist (t2 trs2)
                    (when (and (eq (nth 0 t2) s2) (eq (nth 1 t2) ev))
                      (let ((next (cons (nth 3 t1) (nth 3 t2))))
                        (setq product-trs
                              (cons (list pair ev next) product-trs))
                        (unless (member next visited)
                          (setq visited (cons next visited))
                          (setq queue (append queue (list next))))))))))
            ;; Independent events for protocol 1
            (dolist (t1 trs1)
              (when (and (eq (nth 0 t1) s1)
                         (not (memq (nth 1 t1) shared-events)))
                (let ((next (cons (nth 3 t1) s2)))
                  (setq product-trs
                        (cons (list pair (nth 1 t1) next) product-trs))
                  (unless (member next visited)
                    (setq visited (cons next visited))
                    (setq queue (append queue (list next)))))))
            ;; Independent events for protocol 2
            (dolist (t2 trs2)
              (when (and (eq (nth 0 t2) s2)
                         (not (memq (nth 1 t2) shared-events)))
                (let ((next (cons s1 (nth 3 t2))))
                  (setq product-trs
                        (cons (list pair (nth 1 t2) next) product-trs))
                  (unless (member next visited)
                    (setq visited (cons next visited))
                    (setq queue (append queue (list next)))))))))
        (list 'initial (cons init1 init2)
              'states (length visited)
              'transitions (length product-trs)
              'reachable-states visited))))

  (unwind-protect
      (list
        ;; Two simple protocols sharing 'sync event
        (let ((trs1 (list (list 'a 'x nil 'b nil)
                          (list 'b 'sync nil 'c nil)))
              (trs2 (list (list 'p 'y nil 'q nil)
                          (list 'q 'sync nil 'r nil))))
          (let ((result (funcall 'neovm--pv-compose trs1 'a trs2 'p '(sync))))
            (list (nth 1 result)   ;; initial
                  (nth 3 result)   ;; states count
                  (nth 5 result))));; transitions count

        ;; No shared events: fully interleaved
        (let ((trs1 (list (list 'a 'x nil 'b nil)))
              (trs2 (list (list 'p 'y nil 'q nil))))
          (let ((result (funcall 'neovm--pv-compose trs1 'a trs2 'p nil)))
            (list (nth 1 result)
                  (nth 3 result)
                  (nth 5 result))))

        ;; All events shared: fully synchronized
        (let ((trs1 (list (list 'a 'go nil 'b nil)))
              (trs2 (list (list 'p 'go nil 'q nil))))
          (let ((result (funcall 'neovm--pv-compose trs1 'a trs2 'p '(go))))
            (list (nth 1 result)
                  (nth 3 result)
                  (nth 5 result)))))
    (fmakunbound 'neovm--pv-compose)))"#;
    let expect = expect_test::expect![[r#""OK (((a . p) 5 5) ((a . p) 4 4) ((a . p) 2 1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bisimulation checking: two protocols are behaviorally equivalent
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_protocol_bisimulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Naive bisimulation check: two protocols are bisimilar if their
  ;; reachable state transition graphs have the same event structure
  ;; when mapped through a relation.
  ;; Simplified: check if enabled events are identical at each corresponding state pair.
  (fset 'neovm--pv-enabled-events
    (lambda (transitions state)
      (let ((events nil))
        (dolist (tr transitions)
          (when (and (eq (nth 0 tr) state)
                     (not (memq (nth 1 tr) events)))
            (setq events (cons (nth 1 tr) events))))
        (sort events (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))

  ;; Check if two protocols have matching event structures via BFS
  ;; relation: list of (state1 . state2) pairs
  (fset 'neovm--pv-check-bisimilar
    (lambda (trs1 init1 trs2 init2)
      (let ((queue (list (cons init1 init2)))
            (visited (list (cons init1 init2)))
            (ok t)
            (reason nil))
        (while (and queue ok)
          (let* ((pair (car queue))
                 (s1 (car pair))
                 (s2 (cdr pair))
                 (ev1 (funcall 'neovm--pv-enabled-events trs1 s1))
                 (ev2 (funcall 'neovm--pv-enabled-events trs2 s2)))
            (setq queue (cdr queue))
            (if (not (equal ev1 ev2))
                (progn
                  (setq ok nil)
                  (setq reason (list 'event-mismatch s1 ev1 s2 ev2)))
              ;; For each shared event, find successors and enqueue
              (dolist (ev ev1)
                (let ((next1 nil) (next2 nil))
                  (dolist (tr trs1)
                    (when (and (eq (nth 0 tr) s1) (eq (nth 1 tr) ev) (not next1))
                      (setq next1 (nth 3 tr))))
                  (dolist (tr trs2)
                    (when (and (eq (nth 0 tr) s2) (eq (nth 1 tr) ev) (not next2))
                      (setq next2 (nth 3 tr))))
                  (when (and next1 next2)
                    (let ((next-pair (cons next1 next2)))
                      (unless (member next-pair visited)
                        (setq visited (cons next-pair visited))
                        (setq queue (append queue (list next-pair)))))))))))
        (list 'bisimilar ok
              'visited-pairs (length visited)
              'reason reason))))

  (unwind-protect
      (list
        ;; Identical protocols: bisimilar
        (funcall 'neovm--pv-check-bisimilar
          (list (list 'a 'x nil 'b nil) (list 'b 'y nil 'a nil))
          'a
          (list (list 'p 'x nil 'q nil) (list 'q 'y nil 'p nil))
          'p)

        ;; Different events: not bisimilar
        (funcall 'neovm--pv-check-bisimilar
          (list (list 'a 'x nil 'b nil))
          'a
          (list (list 'p 'y nil 'q nil))
          'p)

        ;; Same structure different names: bisimilar
        (funcall 'neovm--pv-check-bisimilar
          (list (list 's0 'go nil 's1 nil) (list 's1 'stop nil 's0 nil))
          's0
          (list (list 'q0 'go nil 'q1 nil) (list 'q1 'stop nil 'q0 nil))
          'q0)

        ;; One has extra transition: not bisimilar
        (funcall 'neovm--pv-check-bisimilar
          (list (list 'a 'x nil 'b nil) (list 'a 'y nil 'c nil))
          'a
          (list (list 'p 'x nil 'q nil))
          'p)

        ;; Both empty (no transitions): bisimilar
        (funcall 'neovm--pv-check-bisimilar nil 'a nil 'p))
    (fmakunbound 'neovm--pv-enabled-events)
    (fmakunbound 'neovm--pv-check-bisimilar)))"#;
    let expect = expect_test::expect![[
        r#""OK ((bisimilar t visited-pairs 2 reason nil) (bisimilar nil visited-pairs 1 reason (event-mismatch a (x) p (y))) (bisimilar t visited-pairs 2 reason nil) (bisimilar nil visited-pairs 1 reason (event-mismatch a (x y) p (x))) (bisimilar t visited-pairs 1 reason nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Path finding: find a path from state A to state B
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_protocol_path_finding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; BFS to find shortest event path from start to goal
  (fset 'neovm--pv-find-path
    (lambda (transitions start goal)
      (if (eq start goal)
          (list 'found t 'path nil 'length 0)
        (let ((visited (list start))
              (queue (list (list start nil)))  ;; (state path-so-far)
              (found nil))
          (while (and queue (not found))
            (let* ((entry (car queue))
                   (state (car entry))
                   (path (cadr entry)))
              (setq queue (cdr queue))
              (dolist (tr transitions)
                (when (and (not found) (eq (nth 0 tr) state))
                  (let ((next (nth 3 tr))
                        (event (nth 1 tr)))
                    (let ((new-path (append path (list event))))
                      (if (eq next goal)
                          (setq found new-path)
                        (unless (memq next visited)
                          (setq visited (cons next visited))
                          (setq queue (append queue
                                              (list (list next new-path))))))))))))
          (if found
              (list 'found t 'path found 'length (length found))
            (list 'found nil 'path nil 'length -1))))))

  (unwind-protect
      (let ((trs (list
                   (list 'a 'e1 nil 'b nil)
                   (list 'b 'e2 nil 'c nil)
                   (list 'c 'e3 nil 'd nil)
                   (list 'a 'shortcut nil 'd nil)
                   (list 'd 'e4 nil 'e nil)
                   (list 'b 'branch nil 'e nil))))
        (list
          ;; Direct path
          (funcall 'neovm--pv-find-path trs 'a 'b)
          ;; Shortest path a->d is the shortcut
          (funcall 'neovm--pv-find-path trs 'a 'd)
          ;; Path a->e: shortest is a->b->branch->e (len 2 via shortcut->e4)
          ;; or a->shortcut->d->e4->e... let's see
          (funcall 'neovm--pv-find-path trs 'a 'e)
          ;; Self path
          (funcall 'neovm--pv-find-path trs 'a 'a)
          ;; No path
          (funcall 'neovm--pv-find-path trs 'e 'a)
          ;; Path from middle
          (funcall 'neovm--pv-find-path trs 'c 'e)))
    (fmakunbound 'neovm--pv-find-path)))"#;
    let expect = expect_test::expect![[
        r#""OK ((found t path (e1) length 1) (found t path (shortcut) length 1) (found t path (e1 branch) length 2) (found t path nil length 0) (found nil path nil length -1) (found t path (e3 e4) length 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Invariant checking: verify property holds at every reachable state
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_protocol_invariant_checking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Check that an invariant holds at every reachable state.
  ;; The invariant is a function: (lambda (state ctx) -> bool)
  ;; We simulate running all possible event sequences (BFS on state space).
  (fset 'neovm--pv-check-invariant
    (lambda (transitions initial invariant-fn)
      (let ((visited (list initial))
            (queue (list (cons initial (make-hash-table))))
            (violations nil)
            (checked 0))
        (while queue
          (let* ((entry (car queue))
                 (state (car entry))
                 (ctx (cdr entry)))
            (setq queue (cdr queue))
            (setq checked (1+ checked))
            ;; Check invariant
            (unless (funcall invariant-fn state ctx)
              (setq violations (cons state violations)))
            ;; Explore transitions
            (dolist (tr transitions)
              (when (eq (nth 0 tr) state)
                (let ((next (nth 3 tr)))
                  (unless (memq next visited)
                    (let ((new-ctx (copy-hash-table ctx)))
                      (when (nth 4 tr)
                        (funcall (nth 4 tr) new-ctx))
                      (setq visited (cons next visited))
                      (setq queue (append queue (list (cons next new-ctx)))))))))))
        (list 'invariant-holds (null violations)
              'checked checked
              'violations (nreverse violations)))))

  (unwind-protect
      (list
        ;; Invariant: state is never 'error
        (funcall 'neovm--pv-check-invariant
          (list
            (list 'init 'go nil 'running nil)
            (list 'running 'done nil 'complete nil))
          'init
          (lambda (state ctx) (not (eq state 'error))))

        ;; Invariant violated: 'error is reachable
        (funcall 'neovm--pv-check-invariant
          (list
            (list 'init 'go nil 'running nil)
            (list 'running 'fail nil 'error nil)
            (list 'error 'recover nil 'init nil))
          'init
          (lambda (state ctx) (not (eq state 'error))))

        ;; Counter invariant: counter is always tracked
        (funcall 'neovm--pv-check-invariant
          (list
            (list 'start 'tick nil 'middle
                  (lambda (ctx) (puthash 'count 1 ctx)))
            (list 'middle 'tick nil 'end
                  (lambda (ctx) (puthash 'count 2 ctx))))
          'start
          (lambda (state ctx) t))  ;; trivially true

        ;; All states satisfy invariant
        (funcall 'neovm--pv-check-invariant
          (list
            (list 'a 'x nil 'b nil)
            (list 'b 'x nil 'c nil)
            (list 'c 'x nil 'a nil))
          'a
          (lambda (state ctx)
            (memq state '(a b c)))))
    (fmakunbound 'neovm--pv-check-invariant)))"#;
    let expect = expect_test::expect![[
        r#""OK ((invariant-holds t checked 3 violations nil) (invariant-holds nil checked 3 violations (error)) (invariant-holds t checked 3 violations nil) (invariant-holds t checked 3 violations nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Protocol state space statistics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_protocol_state_space_stats() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Compute various statistics about a protocol's state space
  (fset 'neovm--pv-state-stats
    (lambda (transitions initial)
      (let ((all-states nil)
            (in-degree (make-hash-table))
            (out-degree (make-hash-table))
            (events-set nil))
        ;; Collect all states and count degrees
        (dolist (tr transitions)
          (let ((from (nth 0 tr))
                (ev (nth 1 tr))
                (to (nth 3 tr)))
            (unless (memq from all-states) (setq all-states (cons from all-states)))
            (unless (memq to all-states) (setq all-states (cons to all-states)))
            (unless (memq ev events-set) (setq events-set (cons ev events-set)))
            (puthash from (1+ (gethash from out-degree 0)) out-degree)
            (puthash to (1+ (gethash to in-degree 0)) in-degree)))
        ;; Find sources (in-degree 0 among reachable) and sinks (out-degree 0)
        (let ((sources nil) (sinks nil))
          (dolist (s all-states)
            (when (= (gethash s in-degree 0) 0) (setq sources (cons s sources)))
            (when (= (gethash s out-degree 0) 0) (setq sinks (cons s sinks))))
          ;; Find max out-degree state
          (let ((max-out 0) (max-out-state nil))
            (dolist (s all-states)
              (let ((deg (gethash s out-degree 0)))
                (when (> deg max-out)
                  (setq max-out deg max-out-state s))))
            (list 'total-states (length all-states)
                  'total-transitions (length transitions)
                  'total-events (length events-set)
                  'sources (sort sources (lambda (a b) (string< (symbol-name a) (symbol-name b))))
                  'sinks (sort sinks (lambda (a b) (string< (symbol-name a) (symbol-name b))))
                  'max-out-degree max-out
                  'max-out-state max-out-state))))))

  (unwind-protect
      (list
        ;; Linear protocol
        (funcall 'neovm--pv-state-stats
          (list
            (list 'a 'e1 nil 'b nil)
            (list 'b 'e2 nil 'c nil)
            (list 'c 'e3 nil 'd nil))
          'a)

        ;; Branching protocol
        (funcall 'neovm--pv-state-stats
          (list
            (list 'start 'left nil 'l1 nil)
            (list 'start 'right nil 'r1 nil)
            (list 'start 'middle nil 'm1 nil)
            (list 'l1 'go nil 'end nil)
            (list 'r1 'go nil 'end nil)
            (list 'm1 'go nil 'end nil))
          'start)

        ;; Circular protocol
        (funcall 'neovm--pv-state-stats
          (list
            (list 'a 'x nil 'b nil)
            (list 'b 'x nil 'c nil)
            (list 'c 'x nil 'a nil))
          'a)

        ;; Single state self-loop
        (funcall 'neovm--pv-state-stats
          (list (list 'loop 'tick nil 'loop nil))
          'loop))
    (fmakunbound 'neovm--pv-state-stats)))"#;
    let expect = expect_test::expect![[
        r#""OK ((total-states 4 total-transitions 3 total-events 3 sources (a) sinks (d) max-out-degree 1 max-out-state c) (total-states 5 total-transitions 6 total-events 4 sources (start) sinks (end) max-out-degree 3 max-out-state start) (total-states 3 total-transitions 3 total-events 1 sources nil sinks nil max-out-degree 1 max-out-state c) (total-states 1 total-transitions 1 total-events 1 sources nil sinks nil max-out-degree 1 max-out-state loop))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
