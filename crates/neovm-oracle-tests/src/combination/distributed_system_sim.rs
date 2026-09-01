//! Oracle parity tests for distributed system simulation patterns in Elisp:
//! vector clocks, Lamport timestamps, two-phase commit, leader election
//! (bully algorithm), consistent hashing ring, gossip protocol, and
//! split-brain detection.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Vector clocks for causality tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_dist_sys_vector_clocks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-vc-new
    (lambda (nodes)
      (let ((vc nil))
        (dolist (n nodes) (setq vc (cons (cons n 0) vc)))
        (nreverse vc))))

  (fset 'neovm--test-vc-get
    (lambda (vc node)
      (let ((entry (assoc node vc)))
        (if entry (cdr entry) 0))))

  (fset 'neovm--test-vc-increment
    (lambda (vc node)
      (mapcar (lambda (entry)
                (if (equal (car entry) node)
                    (cons (car entry) (1+ (cdr entry)))
                  (cons (car entry) (cdr entry))))
              vc)))

  (fset 'neovm--test-vc-merge
    (lambda (vc1 vc2)
      (mapcar (lambda (entry)
                (let ((node (car entry))
                      (v1 (cdr entry)))
                  (let ((v2 (funcall 'neovm--test-vc-get vc2 node)))
                    (cons node (max v1 v2)))))
              vc1)))

  ;; Compare: 'before, 'after, 'concurrent, 'equal
  (fset 'neovm--test-vc-compare
    (lambda (vc1 vc2)
      (let ((has-less nil) (has-greater nil))
        (dolist (entry vc1)
          (let ((v1 (cdr entry))
                (v2 (funcall 'neovm--test-vc-get vc2 (car entry))))
            (when (< v1 v2) (setq has-less t))
            (when (> v1 v2) (setq has-greater t))))
        (cond
         ((and has-less (not has-greater)) 'before)
         ((and has-greater (not has-less)) 'after)
         ((and (not has-less) (not has-greater)) 'equal)
         (t 'concurrent)))))

  (fset 'neovm--test-vc-send
    (lambda (sender-vc sender-node)
      (funcall 'neovm--test-vc-increment sender-vc sender-node)))

  (fset 'neovm--test-vc-receive
    (lambda (receiver-vc sender-vc receiver-node)
      (let ((merged (funcall 'neovm--test-vc-merge receiver-vc sender-vc)))
        (funcall 'neovm--test-vc-increment merged receiver-node))))

  (unwind-protect
      (let* ((nodes '("A" "B" "C"))
             (a-vc (funcall 'neovm--test-vc-new nodes))
             (b-vc (funcall 'neovm--test-vc-new nodes))
             (c-vc (funcall 'neovm--test-vc-new nodes)))
        ;; A does local event
        (setq a-vc (funcall 'neovm--test-vc-increment a-vc "A"))
        ;; A sends to B
        (let ((msg-vc (funcall 'neovm--test-vc-send a-vc "A")))
          (setq b-vc (funcall 'neovm--test-vc-receive b-vc msg-vc "B")))
        ;; B does local event
        (setq b-vc (funcall 'neovm--test-vc-increment b-vc "B"))
        ;; C does independent local event
        (setq c-vc (funcall 'neovm--test-vc-increment c-vc "C"))
        ;; C does another local event
        (setq c-vc (funcall 'neovm--test-vc-increment c-vc "C"))
        ;; B sends to C
        (let ((msg-vc (funcall 'neovm--test-vc-send b-vc "B")))
          (setq c-vc (funcall 'neovm--test-vc-receive c-vc msg-vc "C")))
        ;; Now compare
        (list :a-vc a-vc
              :b-vc b-vc
              :c-vc c-vc
              :a-vs-b (funcall 'neovm--test-vc-compare a-vc b-vc)
              :b-vs-c (funcall 'neovm--test-vc-compare b-vc c-vc)
              :a-vs-c (funcall 'neovm--test-vc-compare a-vc c-vc)))
    (fmakunbound 'neovm--test-vc-new)
    (fmakunbound 'neovm--test-vc-get)
    (fmakunbound 'neovm--test-vc-increment)
    (fmakunbound 'neovm--test-vc-merge)
    (fmakunbound 'neovm--test-vc-compare)
    (fmakunbound 'neovm--test-vc-send)
    (fmakunbound 'neovm--test-vc-receive)))"#;
    let expect = expect_test::expect![[
        r#""OK (:a-vc ((\"A\" . 1) (\"B\" . 0) (\"C\" . 0)) :b-vc ((\"A\" . 2) (\"B\" . 2) (\"C\" . 0)) :c-vc ((\"A\" . 2) (\"B\" . 3) (\"C\" . 3)) :a-vs-b before :b-vs-c before :a-vs-c before)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Lamport timestamps: logical clock ordering
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_dist_sys_lamport_timestamps() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--test-lt-events nil)
  (unwind-protect
      (let* ((make-process
              (lambda (id) (list :id id :clock 0)))
             (local-event
              (lambda (proc event-name)
                (let* ((new-clock (1+ (plist-get proc :clock)))
                       (updated (plist-put (copy-sequence proc) :clock new-clock)))
                  (setq neovm--test-lt-events
                        (cons (list :process (plist-get proc :id)
                                    :event event-name
                                    :timestamp new-clock)
                              neovm--test-lt-events))
                  updated)))
             (send-event
              (lambda (sender event-name)
                (let* ((new-clock (1+ (plist-get sender :clock)))
                       (updated (plist-put (copy-sequence sender) :clock new-clock)))
                  (setq neovm--test-lt-events
                        (cons (list :process (plist-get sender :id)
                                    :event event-name
                                    :timestamp new-clock
                                    :msg-clock new-clock)
                              neovm--test-lt-events))
                  updated)))
             (receive-event
              (lambda (receiver msg-clock event-name)
                (let* ((new-clock (1+ (max (plist-get receiver :clock) msg-clock)))
                       (updated (plist-put (copy-sequence receiver) :clock new-clock)))
                  (setq neovm--test-lt-events
                        (cons (list :process (plist-get receiver :id)
                                    :event event-name
                                    :timestamp new-clock)
                              neovm--test-lt-events))
                  updated))))
        (setq neovm--test-lt-events nil)
        (let ((p1 (funcall make-process "P1"))
              (p2 (funcall make-process "P2"))
              (p3 (funcall make-process "P3")))
          ;; P1: local event
          (setq p1 (funcall local-event p1 "compute"))
          ;; P1 sends to P2
          (let ((msg-ts (plist-get p1 :clock)))
            (setq p1 (funcall send-event p1 "send-to-P2"))
            ;; P2 does local work first
            (setq p2 (funcall local-event p2 "init"))
            (setq p2 (funcall local-event p2 "prepare"))
            ;; P2 receives from P1
            (setq p2 (funcall receive-event p2 (1+ msg-ts) "recv-from-P1")))
          ;; P3 does independent work
          (setq p3 (funcall local-event p3 "start"))
          (setq p3 (funcall local-event p3 "process"))
          (setq p3 (funcall local-event p3 "finish"))
          ;; P2 sends to P3
          (let ((msg-ts (plist-get p2 :clock)))
            (setq p2 (funcall send-event p2 "send-to-P3"))
            (setq p3 (funcall receive-event p3 (1+ msg-ts) "recv-from-P2")))
          ;; Sort events by timestamp
          (let ((sorted (sort (nreverse neovm--test-lt-events)
                              (lambda (a b)
                                (< (plist-get a :timestamp)
                                   (plist-get b :timestamp))))))
            (list :p1-clock (plist-get p1 :clock)
                  :p2-clock (plist-get p2 :clock)
                  :p3-clock (plist-get p3 :clock)
                  :total-events (length sorted)
                  :event-order (mapcar (lambda (e)
                                         (list (plist-get e :process)
                                               (plist-get e :event)
                                               (plist-get e :timestamp)))
                                       sorted)))))
    (makunbound 'neovm--test-lt-events)))"#;
    let expect = expect_test::expect![[
        r#""OK (:p1-clock 2 :p2-clock 4 :p3-clock 5 :total-events 10 :event-order ((\"P1\" \"compute\" 1) (\"P2\" \"init\" 1) (\"P3\" \"start\" 1) (\"P1\" \"send-to-P2\" 2) (\"P2\" \"prepare\" 2) (\"P3\" \"process\" 2) (\"P2\" \"recv-from-P1\" 3) (\"P3\" \"finish\" 3) (\"P2\" \"send-to-P3\" 4) (\"P3\" \"recv-from-P2\" 5)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Two-phase commit protocol
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_dist_sys_two_phase_commit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-2pc-run
    (lambda (coordinator-id participants transaction)
      (let ((phase1-log nil)
            (phase2-log nil)
            (all-voted-yes t)
            (final-decision nil))
        ;; Phase 1: PREPARE - ask each participant to vote
        (dolist (p participants)
          (let* ((p-id (plist-get p :id))
                 (can-commit (plist-get p :can-commit))
                 (vote (if (funcall can-commit transaction) 'yes 'no)))
            (setq phase1-log (cons (list :participant p-id :vote vote) phase1-log))
            (when (eq vote 'no)
              (setq all-voted-yes nil))))
        (setq phase1-log (nreverse phase1-log))
        ;; Phase 2: COMMIT or ABORT based on votes
        (setq final-decision (if all-voted-yes 'commit 'abort))
        (dolist (p participants)
          (let* ((p-id (plist-get p :id))
                 (action (if all-voted-yes
                             (funcall (plist-get p :do-commit) transaction)
                           (funcall (plist-get p :do-abort) transaction))))
            (setq phase2-log (cons (list :participant p-id
                                         :action final-decision
                                         :result action)
                                   phase2-log))))
        (setq phase2-log (nreverse phase2-log))
        (list :coordinator coordinator-id
              :transaction transaction
              :decision final-decision
              :phase1 phase1-log
              :phase2 phase2-log))))

  (unwind-protect
      (let* ((db-participant
              (list :id "db-node"
                    :can-commit (lambda (txn)
                                  ;; DB can commit if amount <= 1000
                                  (<= (plist-get txn :amount) 1000))
                    :do-commit (lambda (txn)
                                 (format "DB committed %d" (plist-get txn :amount)))
                    :do-abort (lambda (txn) "DB rolled back")))
             (cache-participant
              (list :id "cache-node"
                    :can-commit (lambda (txn)
                                  ;; Cache always ready
                                  t)
                    :do-commit (lambda (txn)
                                 (format "Cache updated for %s" (plist-get txn :key)))
                    :do-abort (lambda (txn) "Cache invalidated")))
             (queue-participant
              (list :id "queue-node"
                    :can-commit (lambda (txn)
                                  ;; Queue rejects if key starts with "restricted"
                                  (not (string-prefix-p "restricted"
                                                        (plist-get txn :key))))
                    :do-commit (lambda (txn)
                                 (format "Queue enqueued %s" (plist-get txn :key)))
                    :do-abort (lambda (txn) "Queue purged pending")))
             (participants (list db-participant cache-participant queue-participant)))
        (list
         ;; All agree: commit
         (funcall 'neovm--test-2pc-run "coord-1" participants
                  '(:key "order-42" :amount 500))
         ;; DB rejects (amount too high): abort all
         (funcall 'neovm--test-2pc-run "coord-1" participants
                  '(:key "big-order" :amount 5000))
         ;; Queue rejects (restricted key): abort all
         (funcall 'neovm--test-2pc-run "coord-1" participants
                  '(:key "restricted-data" :amount 100))
         ;; Small valid transaction
         (funcall 'neovm--test-2pc-run "coord-1" participants
                  '(:key "item-7" :amount 10))))
    (fmakunbound 'neovm--test-2pc-run)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:coordinator \"coord-1\" :transaction (:key \"order-42\" :amount 500) :decision commit :phase1 ((:participant \"db-node\" :vote yes) (:participant \"cache-node\" :vote yes) (:participant \"queue-node\" :vote yes)) :phase2 ((:participant \"db-node\" :action commit :result \"DB committed 500\") (:participant \"cache-node\" :action commit :result \"Cache updated for order-42\") (:participant \"queue-node\" :action commit :result \"Queue enqueued order-42\"))) (:coordinator \"coord-1\" :transaction (:key \"big-order\" :amount 5000) :decision abort :phase1 ((:participant \"db-node\" :vote no) (:participant \"cache-node\" :vote yes) (:participant \"queue-node\" :vote yes)) :phase2 ((:participant \"db-node\" :action abort :result \"DB rolled back\") (:participant \"cache-node\" :action abort :result \"Cache invalidated\") (:participant \"queue-node\" :action abort :result \"Queue purged pending\"))) (:coordinator \"coord-1\" :transaction (:key \"restricted-data\" :amount 100) :decision abort :phase1 ((:participant \"db-node\" :vote yes) (:participant \"cache-node\" :vote yes) (:participant \"queue-node\" :vote no)) :phase2 ((:participant \"db-node\" :action abort :result \"DB rolled back\") (:participant \"cache-node\" :action abort :result \"Cache invalidated\") (:participant \"queue-node\" :action abort :result \"Queue purged pending\"))) (:coordinator \"coord-1\" :transaction (:key \"item-7\" :amount 10) :decision commit :phase1 ((:participant \"db-node\" :vote yes) (:participant \"cache-node\" :vote yes) (:participant \"queue-node\" :vote yes)) :phase2 ((:participant \"db-node\" :action commit :result \"DB committed 10\") (:participant \"cache-node\" :action commit :result \"Cache updated for item-7\") (:participant \"queue-node\" :action commit :result \"Queue enqueued item-7\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Leader election: bully algorithm
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_dist_sys_bully_election() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Bully algorithm: node with highest ID among alive nodes wins.
  ;; A node initiates election by sending ELECTION to all higher-ID nodes.
  ;; If no higher node responds, it becomes leader.
  ;; If a higher node responds, that node takes over the election.

  (fset 'neovm--test-bully-elect
    (lambda (nodes alive-set initiator)
      (let ((election-log nil)
            (leader nil)
            (round 0)
            (active-elections (list initiator)))
        ;; Process elections round by round
        (while (and active-elections (< round 20))
          (setq round (1+ round))
          (let ((next-active nil))
            (dolist (candidate active-elections)
              (when (memq candidate alive-set)
                ;; Send ELECTION to all nodes with higher IDs
                (let ((higher-responders nil))
                  (dolist (n nodes)
                    (when (and (> n candidate)
                               (memq n alive-set))
                      (setq higher-responders (cons n higher-responders))
                      (setq election-log
                            (cons (list :round round :from candidate
                                        :to n :msg 'ELECTION)
                                  election-log))))
                  (if higher-responders
                      ;; Higher nodes take over
                      (dolist (h higher-responders)
                        (unless (memq h next-active)
                          (setq next-active (cons h next-active)))
                        (setq election-log
                              (cons (list :round round :from h
                                          :to candidate :msg 'OK)
                                    election-log)))
                    ;; No higher responders -> this node is leader
                    (setq leader candidate)
                    (setq election-log
                          (cons (list :round round :from candidate
                                      :msg 'COORDINATOR)
                                election-log))))))
            (setq active-elections next-active)))
        (list :leader leader
              :rounds round
              :log-length (length election-log)
              :election-log (nreverse election-log)))))

  (unwind-protect
      (let ((all-nodes '(1 2 3 4 5)))
        (list
         ;; All alive, node 1 initiates -> node 5 wins
         (funcall 'neovm--test-bully-elect all-nodes '(1 2 3 4 5) 1)
         ;; Node 5 down, node 2 initiates -> node 4 wins
         (funcall 'neovm--test-bully-elect all-nodes '(1 2 3 4) 2)
         ;; Only nodes 1 and 3 alive, node 1 initiates -> node 3 wins
         (funcall 'neovm--test-bully-elect all-nodes '(1 3) 1)
         ;; Highest node initiates, it immediately wins
         (funcall 'neovm--test-bully-elect all-nodes '(1 2 3 4 5) 5)
         ;; Single node alive
         (funcall 'neovm--test-bully-elect all-nodes '(3) 3)))
    (fmakunbound 'neovm--test-bully-elect)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:leader 5 :rounds 5 :log-length 32 :election-log ((:round 1 :from 1 :to 2 :msg ELECTION) (:round 1 :from 1 :to 3 :msg ELECTION) (:round 1 :from 1 :to 4 :msg ELECTION) (:round 1 :from 1 :to 5 :msg ELECTION) (:round 1 :from 5 :to 1 :msg OK) (:round 1 :from 4 :to 1 :msg OK) (:round 1 :from 3 :to 1 :msg OK) (:round 1 :from 2 :to 1 :msg OK) (:round 2 :from 2 :to 3 :msg ELECTION) (:round 2 :from 2 :to 4 :msg ELECTION) (:round 2 :from 2 :to 5 :msg ELECTION) (:round 2 :from 5 :to 2 :msg OK) (:round 2 :from 4 :to 2 :msg OK) (:round 2 :from 3 :to 2 :msg OK) (:round 2 :from 3 :to 4 :msg ELECTION) (:round 2 :from 3 :to 5 :msg ELECTION) (:round 2 :from 5 :to 3 :msg OK) (:round 2 :from 4 :to 3 :msg OK) (:round 2 :from 4 :to 5 :msg ELECTION) (:round 2 :from 5 :to 4 :msg OK) (:round 2 :from 5 :msg COORDINATOR) (:round 3 :from 3 :to 4 :msg ELECTION) (:round 3 :from 3 :to 5 :msg ELECTION) (:round 3 :from 5 :to 3 :msg OK) (:round 3 :from 4 :to 3 :msg OK) (:round 3 :from 4 :to 5 :msg ELECTION) (:round 3 :from 5 :to 4 :msg OK) (:round 3 :from 5 :msg COORDINATOR) (:round 4 :from 4 :to 5 :msg ELECTION) (:round 4 :from 5 :to 4 :msg OK) (:round 4 :from 5 :msg COORDINATOR) (:round 5 :from 5 :msg COORDINATOR))) (:leader 4 :rounds 3 :log-length 8 :election-log ((:round 1 :from 2 :to 3 :msg ELECTION) (:round 1 :from 2 :to 4 :msg ELECTION) (:round 1 :from 4 :to 2 :msg OK) (:round 1 :from 3 :to 2 :msg OK) (:round 2 :from 3 :to 4 :msg ELECTION) (:round 2 :from 4 :to 3 :msg OK) (:round 2 :from 4 :msg COORDINATOR) (:round 3 :from 4 :msg COORDINATOR))) (:leader 3 :rounds 2 :log-length 3 :election-log ((:round 1 :from 1 :to 3 :msg ELECTION) (:round 1 :from 3 :to 1 :msg OK) (:round 2 :from 3 :msg COORDINATOR))) (:leader 5 :rounds 1 :log-length 1 :election-log ((:round 1 :from 5 :msg COORDINATOR))) (:leader 3 :rounds 1 :log-length 1 :election-log ((:round 1 :from 3 :msg COORDINATOR))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Consistent hashing ring
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_dist_sys_consistent_hashing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Simple hash function for strings: sum of char codes mod ring-size
  (fset 'neovm--test-ch-hash
    (lambda (key ring-size)
      (let ((sum 0))
        (dotimes (i (length key))
          (setq sum (+ sum (aref key i))))
        (% sum ring-size))))

  ;; Add a node to the ring with virtual nodes
  (fset 'neovm--test-ch-add-node
    (lambda (ring node-name num-vnodes ring-size)
      (let ((new-ring ring))
        (dotimes (i num-vnodes)
          (let* ((vnode-key (format "%s-vnode-%d" node-name i))
                 (pos (funcall 'neovm--test-ch-hash vnode-key ring-size)))
            (setq new-ring (cons (cons pos node-name) new-ring))))
        (sort new-ring (lambda (a b) (< (car a) (car b)))))))

  ;; Find which node owns a key (first node clockwise from key's position)
  (fset 'neovm--test-ch-lookup
    (lambda (ring key ring-size)
      (let* ((pos (funcall 'neovm--test-ch-hash key ring-size))
             (owner nil))
        ;; Find first ring entry >= pos
        (dolist (entry ring)
          (when (and (not owner) (>= (car entry) pos))
            (setq owner (cdr entry))))
        ;; If none found, wrap around to first entry
        (unless owner
          (setq owner (cdr (car ring))))
        owner)))

  ;; Remove a node from ring
  (fset 'neovm--test-ch-remove-node
    (lambda (ring node-name)
      (let ((result nil))
        (dolist (entry ring)
          (unless (equal (cdr entry) node-name)
            (setq result (cons entry result))))
        (nreverse result))))

  (unwind-protect
      (let* ((ring-size 360)
             (ring nil))
        ;; Add 3 nodes with 3 virtual nodes each
        (setq ring (funcall 'neovm--test-ch-add-node ring "node-A" 3 ring-size))
        (setq ring (funcall 'neovm--test-ch-add-node ring "node-B" 3 ring-size))
        (setq ring (funcall 'neovm--test-ch-add-node ring "node-C" 3 ring-size))
        ;; Lookup various keys
        (let ((keys '("user:1" "user:2" "user:3" "session:abc" "session:xyz"
                       "data:foo" "data:bar" "config:main"))
              (before-remove nil)
              (after-remove nil))
          ;; Lookup before removing a node
          (dolist (k keys)
            (setq before-remove
                  (cons (cons k (funcall 'neovm--test-ch-lookup ring k ring-size))
                        before-remove)))
          ;; Remove node-B
          (let ((ring2 (funcall 'neovm--test-ch-remove-node ring "node-B")))
            ;; Lookup after removing
            (dolist (k keys)
              (setq after-remove
                    (cons (cons k (funcall 'neovm--test-ch-lookup ring2 k ring-size))
                          after-remove)))
            ;; Count how many keys changed owner
            (let ((changed 0))
              (dolist (k keys)
                (let ((b (cdr (assoc k before-remove)))
                      (a (cdr (assoc k after-remove))))
                  (unless (equal a b) (setq changed (1+ changed)))))
              (list :ring-size (length ring)
                    :ring-after-remove (length ring2)
                    :before (nreverse before-remove)
                    :after (nreverse after-remove)
                    :keys-changed changed)))))
    (fmakunbound 'neovm--test-ch-hash)
    (fmakunbound 'neovm--test-ch-add-node)
    (fmakunbound 'neovm--test-ch-lookup)
    (fmakunbound 'neovm--test-ch-remove-node)))"#;
    let expect = expect_test::expect![[
        r#""OK (:ring-size 9 :ring-after-remove 6 :before ((\"user:1\" . \"node-A\") (\"user:2\" . \"node-A\") (\"user:3\" . \"node-A\") (\"session:abc\" . \"node-A\") (\"session:xyz\" . \"node-A\") (\"data:foo\" . \"node-A\") (\"data:bar\" . \"node-A\") (\"config:main\" . \"node-A\")) :after ((\"user:1\" . \"node-A\") (\"user:2\" . \"node-A\") (\"user:3\" . \"node-A\") (\"session:abc\" . \"node-A\") (\"session:xyz\" . \"node-A\") (\"data:foo\" . \"node-A\") (\"data:bar\" . \"node-A\") (\"config:main\" . \"node-A\")) :keys-changed 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Gossip protocol simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_dist_sys_gossip_protocol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  ;; Gossip: each node periodically shares its knowledge with a random peer.
  ;; We simulate with a deterministic peer selection (round-robin) for testability.

  (fset 'neovm--test-gossip-run
    (lambda (node-ids initial-knowledge max-rounds)
      (let ((knowledge (make-hash-table :test 'equal))
            (round-log nil))
        ;; Initialize: each node knows only its own datum
        (dolist (id node-ids)
          (puthash id (list (assoc id initial-knowledge)) knowledge))
        ;; Run gossip rounds
        (dotimes (round max-rounds)
          (let ((round-transfers nil))
            ;; Each node picks next node (round-robin based on round) to gossip with
            (dolist (sender node-ids)
              (let* ((sender-idx (cl-position sender node-ids))
                     (peer-idx (% (+ sender-idx 1 round) (length node-ids)))
                     (receiver (nth peer-idx node-ids))
                     (sender-data (gethash sender knowledge))
                     (receiver-data (gethash receiver knowledge))
                     ;; Merge: receiver learns what sender knows
                     (merged receiver-data)
                     (new-items 0))
                (dolist (item sender-data)
                  (unless (assoc (car item) merged)
                    (setq merged (cons item merged))
                    (setq new-items (1+ new-items))))
                (puthash receiver merged knowledge)
                (when (> new-items 0)
                  (setq round-transfers
                        (cons (list :from sender :to receiver :new-items new-items)
                              round-transfers)))))
            (when round-transfers
              (setq round-log (cons (list :round round
                                          :transfers (nreverse round-transfers))
                                    round-log)))))
        ;; Check convergence: does every node know everything?
        (let ((fully-converged t)
              (total-items (length initial-knowledge))
              (per-node nil))
          (dolist (id node-ids)
            (let ((node-knows (length (gethash id knowledge))))
              (setq per-node (cons (cons id node-knows) per-node))
              (when (< node-knows total-items)
                (setq fully-converged nil))))
          (list :converged fully-converged
                :per-node (nreverse per-node)
                :rounds-with-activity (length round-log)
                :log (nreverse round-log))))))

  (unwind-protect
      (list
       ;; 4 nodes, each with unique data, 6 rounds should converge
       (funcall 'neovm--test-gossip-run
                '("n1" "n2" "n3" "n4")
                '(("n1" . "data-A") ("n2" . "data-B")
                  ("n3" . "data-C") ("n4" . "data-D"))
                6)
       ;; 3 nodes, 2 rounds (might not fully converge)
       (funcall 'neovm--test-gossip-run
                '("x" "y" "z")
                '(("x" . 100) ("y" . 200) ("z" . 300))
                2))
    (fmakunbound 'neovm--test-gossip-run)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:converged t :per-node ((\"n1\" . 4) (\"n2\" . 4) (\"n3\" . 4) (\"n4\" . 4)) :rounds-with-activity 2 :log ((:round 0 :transfers ((:from \"n1\" :to \"n2\" :new-items 1) (:from \"n2\" :to \"n3\" :new-items 2) (:from \"n3\" :to \"n4\" :new-items 3) (:from \"n4\" :to \"n1\" :new-items 3))) (:round 1 :transfers ((:from \"n1\" :to \"n3\" :new-items 1) (:from \"n4\" :to \"n2\" :new-items 2))))) (:converged t :per-node ((\"x\" . 3) (\"y\" . 3) (\"z\" . 3)) :rounds-with-activity 2 :log ((:round 0 :transfers ((:from \"x\" :to \"y\" :new-items 1) (:from \"y\" :to \"z\" :new-items 2) (:from \"z\" :to \"x\" :new-items 2))) (:round 1 :transfers ((:from \"z\" :to \"y\" :new-items 1))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Split-brain detection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_dist_sys_split_brain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Detect split-brain: given a set of nodes and their connectivity,
  ;; determine if the cluster has partitioned into disconnected components.
  ;; Each component elects its own leader, causing split-brain if >1 component.

  (fset 'neovm--test-sb-find-components
    (lambda (nodes edges)
      ;; BFS-based connected components
      (let ((visited (make-hash-table :test 'equal))
            (adj (make-hash-table :test 'equal))
            (components nil))
        ;; Build adjacency list (undirected)
        (dolist (edge edges)
          (let ((a (car edge)) (b (cdr edge)))
            (puthash a (cons b (gethash a adj nil)) adj)
            (puthash b (cons a (gethash b adj nil)) adj)))
        ;; BFS from each unvisited node
        (dolist (start nodes)
          (unless (gethash start visited)
            (let ((component nil)
                  (queue (list start)))
              (puthash start t visited)
              (while queue
                (let ((current (car queue)))
                  (setq queue (cdr queue))
                  (setq component (cons current component))
                  (dolist (neighbor (gethash current adj nil))
                    (unless (gethash neighbor visited)
                      (puthash neighbor t visited)
                      (setq queue (append queue (list neighbor)))))))
              (setq components (cons (sort component #'string<) components)))))
        (nreverse components))))

  (fset 'neovm--test-sb-detect
    (lambda (nodes edges quorum-size)
      (let* ((components (funcall 'neovm--test-sb-find-components nodes edges))
             (num-components (length components))
             (has-split-brain (> num-components 1))
             ;; Each component picks its leader (highest ID by string<)
             (component-info
              (mapcar (lambda (comp)
                        (let ((leader (car (last comp)))  ;; last in sorted = highest
                              (size (length comp))
                              (has-quorum (>= (length comp) quorum-size)))
                          (list :nodes comp :leader leader
                                :size size :has-quorum has-quorum)))
                      components))
             ;; Count components with quorum
             (quorum-components 0))
        (dolist (ci component-info)
          (when (plist-get ci :has-quorum)
            (setq quorum-components (1+ quorum-components))))
        (list :split-brain has-split-brain
              :num-partitions num-components
              :quorum-components quorum-components
              :components component-info
              :danger (and has-split-brain (> quorum-components 1))))))

  (unwind-protect
      (let ((nodes '("n1" "n2" "n3" "n4" "n5")))
        (list
         ;; Fully connected: no split brain
         (funcall 'neovm--test-sb-detect nodes
                  '(("n1" . "n2") ("n2" . "n3") ("n3" . "n4")
                    ("n4" . "n5") ("n1" . "n5"))
                  3)
         ;; Network partition: {n1,n2,n3} and {n4,n5}
         (funcall 'neovm--test-sb-detect nodes
                  '(("n1" . "n2") ("n2" . "n3") ("n4" . "n5"))
                  3)
         ;; Complete split: each node isolated
         (funcall 'neovm--test-sb-detect nodes nil 3)
         ;; Two equal-sized partitions with quorum=2 (dangerous!)
         (funcall 'neovm--test-sb-detect '("n1" "n2" "n3" "n4")
                  '(("n1" . "n2") ("n3" . "n4"))
                  2)
         ;; Star topology: all connected through n1
         (funcall 'neovm--test-sb-detect nodes
                  '(("n1" . "n2") ("n1" . "n3") ("n1" . "n4") ("n1" . "n5"))
                  3)))
    (fmakunbound 'neovm--test-sb-find-components)
    (fmakunbound 'neovm--test-sb-detect)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:split-brain nil :num-partitions 1 :quorum-components 1 :components ((:nodes (\"n1\" \"n2\" \"n3\" \"n4\" \"n5\") :leader \"n5\" :size 5 :has-quorum t)) :danger nil) (:split-brain t :num-partitions 2 :quorum-components 1 :components ((:nodes (\"n1\" \"n2\" \"n3\") :leader \"n3\" :size 3 :has-quorum t) (:nodes (\"n4\" \"n5\") :leader \"n5\" :size 2 :has-quorum nil)) :danger nil) (:split-brain t :num-partitions 5 :quorum-components 0 :components ((:nodes (\"n1\") :leader \"n1\" :size 1 :has-quorum nil) (:nodes (\"n2\") :leader \"n2\" :size 1 :has-quorum nil) (:nodes (\"n3\") :leader \"n3\" :size 1 :has-quorum nil) (:nodes (\"n4\") :leader \"n4\" :size 1 :has-quorum nil) (:nodes (\"n5\") :leader \"n5\" :size 1 :has-quorum nil)) :danger nil) (:split-brain t :num-partitions 2 :quorum-components 2 :components ((:nodes (\"n1\" \"n2\") :leader \"n2\" :size 2 :has-quorum t) (:nodes (\"n3\" \"n4\") :leader \"n4\" :size 2 :has-quorum t)) :danger t) (:split-brain nil :num-partitions 1 :quorum-components 1 :components ((:nodes (\"n1\" \"n2\" \"n3\" \"n4\" \"n5\") :leader \"n5\" :size 5 :has-quorum t)) :danger nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
