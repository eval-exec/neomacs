//! Oracle parity tests for actor model patterns in Elisp:
//! actors as closures with mailbox, message passing (send/receive),
//! actor creation (spawn), behavior switching (become), supervision
//! patterns (restart on error), actor registry (named actors), and
//! request-reply patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic actor: closure with mailbox, send/receive
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_actor_basic_mailbox() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; An actor is a plist: (:name name :behavior fn :mailbox list :state plist)
  ;; behavior fn: (lambda (self state message) ...) -> (list new-state &rest actions)
  ;; actions: (:send target msg) (:reply value) (:become new-behavior)
  (defvar neovm--test-actor-registry nil)
  (defvar neovm--test-actor-log nil)

  (fset 'neovm--test-spawn
    (lambda (name behavior initial-state)
      (let ((actor (list :name name
                         :behavior behavior
                         :mailbox nil
                         :state initial-state)))
        (setq neovm--test-actor-registry
              (cons (cons name actor) neovm--test-actor-registry))
        name)))

  (fset 'neovm--test-find-actor
    (lambda (name)
      (cdr (assoc name neovm--test-actor-registry))))

  (fset 'neovm--test-send
    (lambda (target-name message)
      (let ((actor (funcall 'neovm--test-find-actor target-name)))
        (when actor
          (plist-put actor :mailbox
                     (append (plist-get actor :mailbox) (list message)))))))

  ;; Process one message from an actor's mailbox
  (fset 'neovm--test-process-one
    (lambda (actor-name)
      (let ((actor (funcall 'neovm--test-find-actor actor-name)))
        (when (and actor (plist-get actor :mailbox))
          (let* ((mailbox (plist-get actor :mailbox))
                 (msg (car mailbox))
                 (behavior (plist-get actor :behavior))
                 (state (plist-get actor :state))
                 (result (funcall behavior actor-name state msg))
                 (new-state (car result))
                 (actions (cdr result)))
            ;; Update mailbox (remove processed message)
            (plist-put actor :mailbox (cdr mailbox))
            ;; Update state
            (plist-put actor :state new-state)
            ;; Log
            (setq neovm--test-actor-log
                  (cons (list actor-name msg new-state) neovm--test-actor-log))
            ;; Process actions
            (dolist (action actions)
              (let ((action-type (car action)))
                (cond
                  ((eq action-type :send)
                   (funcall 'neovm--test-send (nth 1 action) (nth 2 action)))
                  ((eq action-type :become)
                   (plist-put actor :behavior (nth 1 action))))))
            t)))))

  ;; Process all pending messages for all actors (one round)
  (fset 'neovm--test-run-round
    (lambda ()
      (let ((processed 0))
        (dolist (entry neovm--test-actor-registry)
          (while (plist-get (cdr entry) :mailbox)
            (funcall 'neovm--test-process-one (car entry))
            (setq processed (1+ processed))))
        processed)))

  (unwind-protect
      (progn
        (setq neovm--test-actor-registry nil)
        (setq neovm--test-actor-log nil)

        ;; Counter actor: handles :inc, :dec, :get
        (funcall 'neovm--test-spawn 'counter
          (lambda (self state msg)
            (let ((type (plist-get msg :type)))
              (cond
                ((eq type :inc)
                 (list (plist-put (copy-sequence state)
                                  :count (1+ (or (plist-get state :count) 0)))))
                ((eq type :dec)
                 (list (plist-put (copy-sequence state)
                                  :count (1- (or (plist-get state :count) 0)))))
                ((eq type :get)
                 (let ((reply-to (plist-get msg :reply-to)))
                   (list state
                         (list :send reply-to
                               (list :type :counter-value
                                     :value (plist-get state :count))))))
                (t (list state)))))
          '(:count 0))

        ;; Accumulator: collects :counter-value messages
        (funcall 'neovm--test-spawn 'collector
          (lambda (self state msg)
            (let ((type (plist-get msg :type)))
              (if (eq type :counter-value)
                  (list (plist-put (copy-sequence state)
                                   :values (cons (plist-get msg :value)
                                                 (plist-get state :values))))
                (list state))))
          '(:values nil))

        ;; Send messages
        (funcall 'neovm--test-send 'counter '(:type :inc))
        (funcall 'neovm--test-send 'counter '(:type :inc))
        (funcall 'neovm--test-send 'counter '(:type :inc))
        (funcall 'neovm--test-send 'counter '(:type :dec))
        (funcall 'neovm--test-send 'counter '(:type :get :reply-to collector))

        ;; Run until all messages processed
        (let ((total 0))
          (dotimes (i 5)
            (setq total (+ total (funcall 'neovm--test-run-round))))

          (let ((counter-state (plist-get (funcall 'neovm--test-find-actor 'counter) :state))
                (collector-state (plist-get (funcall 'neovm--test-find-actor 'collector) :state)))
            (list
              ;; Counter final count
              (plist-get counter-state :count)
              ;; Collector received value
              (plist-get collector-state :values)
              ;; Total messages processed
              total
              ;; Log length
              (length neovm--test-actor-log)))))
    (fmakunbound 'neovm--test-spawn)
    (fmakunbound 'neovm--test-find-actor)
    (fmakunbound 'neovm--test-send)
    (fmakunbound 'neovm--test-process-one)
    (fmakunbound 'neovm--test-run-round)
    (makunbound 'neovm--test-actor-registry)
    (makunbound 'neovm--test-actor-log)))"#;
    let expect = expect_test::expect![[r#""OK (2 (2) 6 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Behavior switching (become): actor changes its message handler
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_actor_become_behavior() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--test-ab-registry nil)
  (defvar neovm--test-ab-log nil)

  (fset 'neovm--test-ab-spawn
    (lambda (name behavior state)
      (setq neovm--test-ab-registry
            (cons (cons name (list :behavior behavior :mailbox nil :state state))
                  neovm--test-ab-registry))))

  (fset 'neovm--test-ab-send
    (lambda (name msg)
      (let ((actor (cdr (assoc name neovm--test-ab-registry))))
        (when actor
          (plist-put actor :mailbox
                     (append (plist-get actor :mailbox) (list msg)))))))

  (fset 'neovm--test-ab-step
    (lambda (name)
      (let ((actor (cdr (assoc name neovm--test-ab-registry))))
        (when (and actor (plist-get actor :mailbox))
          (let* ((msg (car (plist-get actor :mailbox)))
                 (behavior (plist-get actor :behavior))
                 (state (plist-get actor :state))
                 (result (funcall behavior state msg)))
            (plist-put actor :mailbox (cdr (plist-get actor :mailbox)))
            (plist-put actor :state (nth 0 result))
            ;; Check for :become action
            (when (nth 1 result)
              (plist-put actor :behavior (nth 1 result)))
            (setq neovm--test-ab-log
                  (cons (list name (plist-get msg :type) (nth 0 result))
                        neovm--test-ab-log))
            t)))))

  (unwind-protect
      (progn
        (setq neovm--test-ab-registry nil)
        (setq neovm--test-ab-log nil)

        ;; Traffic light actor: switches behavior on :tick
        ;; Red behavior: after 3 ticks, become green
        ;; Green behavior: after 2 ticks, become yellow
        ;; Yellow behavior: after 1 tick, become red
        (let ((red-behavior nil)
              (green-behavior nil)
              (yellow-behavior nil))

          (setq red-behavior
                (lambda (state msg)
                  (if (eq (plist-get msg :type) :tick)
                      (let ((ticks (1+ (or (plist-get state :ticks) 0))))
                        (if (>= ticks 3)
                            (list (list :color 'green :ticks 0 :transitions
                                        (1+ (or (plist-get state :transitions) 0)))
                                  green-behavior)
                          (list (plist-put (copy-sequence state) :ticks ticks)
                                nil)))
                    (list state nil))))

          (setq green-behavior
                (lambda (state msg)
                  (if (eq (plist-get msg :type) :tick)
                      (let ((ticks (1+ (or (plist-get state :ticks) 0))))
                        (if (>= ticks 2)
                            (list (list :color 'yellow :ticks 0 :transitions
                                        (1+ (or (plist-get state :transitions) 0)))
                                  yellow-behavior)
                          (list (plist-put (copy-sequence state) :ticks ticks)
                                nil)))
                    (list state nil))))

          (setq yellow-behavior
                (lambda (state msg)
                  (if (eq (plist-get msg :type) :tick)
                      (let ((ticks (1+ (or (plist-get state :ticks) 0))))
                        (if (>= ticks 1)
                            (list (list :color 'red :ticks 0 :transitions
                                        (1+ (or (plist-get state :transitions) 0)))
                                  red-behavior)
                          (list (plist-put (copy-sequence state) :ticks ticks)
                                nil)))
                    (list state nil))))

          ;; Spawn with red behavior
          (funcall 'neovm--test-ab-spawn 'traffic-light red-behavior
                   '(:color red :ticks 0 :transitions 0))

          ;; Send 18 ticks (3 full cycles: red=3 + green=2 + yellow=1 = 6 per cycle)
          (dotimes (i 18)
            (funcall 'neovm--test-ab-send 'traffic-light '(:type :tick)))

          ;; Process all messages
          (dotimes (i 18)
            (funcall 'neovm--test-ab-step 'traffic-light))

          (let ((final-state (plist-get (cdr (assoc 'traffic-light neovm--test-ab-registry))
                                        :state)))
            (list
              ;; Final color (after 3 full cycles, back to red with 0 ticks)
              (plist-get final-state :color)
              ;; Total transitions
              (plist-get final-state :transitions)
              ;; Log length
              (length neovm--test-ab-log)
              ;; Extract color sequence from log
              (let ((colors nil))
                (dolist (entry (nreverse neovm--test-ab-log))
                  (let ((state (nth 2 entry)))
                    (setq colors (cons (plist-get state :color) colors))))
                (nreverse colors))))))
    (fmakunbound 'neovm--test-ab-spawn)
    (fmakunbound 'neovm--test-ab-send)
    (fmakunbound 'neovm--test-ab-step)
    (makunbound 'neovm--test-ab-registry)
    (makunbound 'neovm--test-ab-log)))"#;
    let expect = expect_test::expect![[
        r#""OK (red 9 18 (red red green green yellow red red red green green yellow red red red green green yellow red))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Supervision: restart actor on error with backoff
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_actor_supervision() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--test-sup-actors nil)
  (defvar neovm--test-sup-log nil)

  (fset 'neovm--test-sup-spawn
    (lambda (name behavior initial-state max-restarts)
      (setq neovm--test-sup-actors
            (cons (cons name (list :behavior behavior
                                   :state initial-state
                                   :initial-state (copy-alist initial-state)
                                   :mailbox nil
                                   :restarts 0
                                   :max-restarts max-restarts
                                   :status 'running))
                  neovm--test-sup-actors))))

  (fset 'neovm--test-sup-send
    (lambda (name msg)
      (let ((actor (cdr (assoc name neovm--test-sup-actors))))
        (when (and actor (eq (plist-get actor :status) 'running))
          (plist-put actor :mailbox
                     (append (plist-get actor :mailbox) (list msg)))))))

  ;; Process with supervision: catch errors, restart if allowed
  (fset 'neovm--test-sup-process
    (lambda (name)
      (let ((actor (cdr (assoc name neovm--test-sup-actors))))
        (when (and actor
                   (eq (plist-get actor :status) 'running)
                   (plist-get actor :mailbox))
          (let* ((msg (car (plist-get actor :mailbox)))
                 (behavior (plist-get actor :behavior))
                 (state (plist-get actor :state))
                 (result (condition-case err
                             (list 'ok (funcall behavior state msg))
                           (error (list 'error (error-message-string err))))))
            (plist-put actor :mailbox (cdr (plist-get actor :mailbox)))
            (if (eq (car result) 'ok)
                (progn
                  (plist-put actor :state (car (cadr result)))
                  (setq neovm--test-sup-log
                        (cons (list name :processed msg) neovm--test-sup-log)))
              ;; Error: attempt restart
              (let ((restarts (plist-get actor :restarts))
                    (max-restarts (plist-get actor :max-restarts)))
                (if (< restarts max-restarts)
                    (progn
                      (plist-put actor :restarts (1+ restarts))
                      (plist-put actor :state
                                 (copy-alist (plist-get actor :initial-state)))
                      (setq neovm--test-sup-log
                            (cons (list name :restarted (cadr result)
                                        (1+ restarts))
                                  neovm--test-sup-log)))
                  ;; Max restarts exceeded: stop actor
                  (plist-put actor :status 'stopped)
                  (setq neovm--test-sup-log
                        (cons (list name :stopped (cadr result))
                              neovm--test-sup-log)))))
            t)))))

  (unwind-protect
      (progn
        (setq neovm--test-sup-actors nil)
        (setq neovm--test-sup-log nil)

        ;; Worker that crashes on negative numbers
        (funcall 'neovm--test-sup-spawn 'worker
          (lambda (state msg)
            (let ((val (plist-get msg :value)))
              (when (< val 0)
                (error "negative value: %d" val))
              (list (plist-put (copy-sequence state)
                               :total (+ (or (plist-get state :total) 0) val)))))
          '(:total 0)
          3)  ;; max 3 restarts

        ;; Send mix of valid and invalid messages
        (funcall 'neovm--test-sup-send 'worker '(:value 10))
        (funcall 'neovm--test-sup-send 'worker '(:value 20))
        (funcall 'neovm--test-sup-send 'worker '(:value -5))   ;; crash + restart 1
        (funcall 'neovm--test-sup-send 'worker '(:value 30))
        (funcall 'neovm--test-sup-send 'worker '(:value -1))   ;; crash + restart 2
        (funcall 'neovm--test-sup-send 'worker '(:value -1))   ;; crash + restart 3
        (funcall 'neovm--test-sup-send 'worker '(:value -1))   ;; crash + stopped
        (funcall 'neovm--test-sup-send 'worker '(:value 100))  ;; should be dropped (stopped)

        ;; Process all
        (dotimes (i 10)
          (funcall 'neovm--test-sup-process 'worker))

        (let ((actor (cdr (assoc 'worker neovm--test-sup-actors))))
          (list
            ;; Final status
            (plist-get actor :status)
            ;; Restart count
            (plist-get actor :restarts)
            ;; State after last successful processing before stop
            (plist-get actor :state)
            ;; Log
            (nreverse neovm--test-sup-log)
            ;; Messages remaining in mailbox (should be 1: the 100)
            (length (plist-get actor :mailbox)))))
    (fmakunbound 'neovm--test-sup-spawn)
    (fmakunbound 'neovm--test-sup-send)
    (fmakunbound 'neovm--test-sup-process)
    (makunbound 'neovm--test-sup-actors)
    (makunbound 'neovm--test-sup-log)))"#;
    let expect = expect_test::expect![[
        r#""OK (stopped 3 (:total 0) ((worker :processed (:value 10)) (worker :processed (:value 20)) (worker :restarted \"negative value: -5\" 1) (worker :processed (:value 30)) (worker :restarted \"negative value: -1\" 2) (worker :restarted \"negative value: -1\" 3) (worker :stopped \"negative value: -1\")) 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Actor registry: named lookup, broadcast, selective receive
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_actor_registry_broadcast() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--test-reg-actors nil)
  (defvar neovm--test-reg-groups nil)

  (fset 'neovm--test-reg-spawn
    (lambda (name group behavior state)
      (let ((actor (list :name name :group group :behavior behavior
                         :mailbox nil :state state :processed 0)))
        (setq neovm--test-reg-actors
              (cons (cons name actor) neovm--test-reg-actors))
        ;; Add to group
        (let ((g (assoc group neovm--test-reg-groups)))
          (if g
              (setcdr g (cons name (cdr g)))
            (setq neovm--test-reg-groups
                  (cons (cons group (list name)) neovm--test-reg-groups))))
        name)))

  ;; Broadcast to all actors in a group
  (fset 'neovm--test-reg-broadcast
    (lambda (group msg)
      (let ((members (cdr (assoc group neovm--test-reg-groups)))
            (sent 0))
        (dolist (name members)
          (let ((actor (cdr (assoc name neovm--test-reg-actors))))
            (when actor
              (plist-put actor :mailbox
                         (append (plist-get actor :mailbox) (list msg)))
              (setq sent (1+ sent)))))
        sent)))

  ;; Process one message per actor
  (fset 'neovm--test-reg-tick
    (lambda ()
      (let ((processed 0))
        (dolist (entry neovm--test-reg-actors)
          (let ((actor (cdr entry)))
            (when (plist-get actor :mailbox)
              (let* ((msg (car (plist-get actor :mailbox)))
                     (behavior (plist-get actor :behavior))
                     (state (plist-get actor :state))
                     (new-state (funcall behavior state msg)))
                (plist-put actor :mailbox (cdr (plist-get actor :mailbox)))
                (plist-put actor :state new-state)
                (plist-put actor :processed (1+ (plist-get actor :processed)))
                (setq processed (1+ processed))))))
        processed)))

  ;; List all actors in a group with their states
  (fset 'neovm--test-reg-group-states
    (lambda (group)
      (let ((members (cdr (assoc group neovm--test-reg-groups)))
            (result nil))
        (dolist (name members)
          (let ((actor (cdr (assoc name neovm--test-reg-actors))))
            (when actor
              (setq result (cons (list name (plist-get actor :state)
                                       (plist-get actor :processed))
                                 result)))))
        (nreverse result))))

  (unwind-protect
      (progn
        (setq neovm--test-reg-actors nil)
        (setq neovm--test-reg-groups nil)

        ;; Spawn workers in two groups
        (dolist (name '(w1 w2 w3))
          (funcall 'neovm--test-reg-spawn name 'workers
            (lambda (state msg)
              (let ((type (plist-get msg :type)))
                (cond
                  ((eq type :add)
                   (plist-put (copy-sequence state) :sum
                              (+ (or (plist-get state :sum) 0)
                                 (plist-get msg :value))))
                  ((eq type :reset)
                   '(:sum 0))
                  (t state))))
            '(:sum 0)))

        (dolist (name '(m1 m2))
          (funcall 'neovm--test-reg-spawn name 'monitors
            (lambda (state msg)
              (plist-put (copy-sequence state) :last-event
                         (plist-get msg :type)))
            '(:last-event nil)))

        ;; Broadcast to workers
        (funcall 'neovm--test-reg-broadcast 'workers '(:type :add :value 10))
        (funcall 'neovm--test-reg-broadcast 'workers '(:type :add :value 20))
        ;; Broadcast to monitors
        (funcall 'neovm--test-reg-broadcast 'monitors '(:type :health-check))
        ;; Direct send to w1 only
        (let ((w1 (cdr (assoc 'w1 neovm--test-reg-actors))))
          (plist-put w1 :mailbox
                     (append (plist-get w1 :mailbox)
                             (list '(:type :add :value 100)))))

        ;; Process all messages
        (let ((total 0))
          (dotimes (i 5)
            (setq total (+ total (funcall 'neovm--test-reg-tick))))

          (list
            ;; Total messages processed
            total
            ;; Worker group states
            (funcall 'neovm--test-reg-group-states 'workers)
            ;; Monitor group states
            (funcall 'neovm--test-reg-group-states 'monitors)
            ;; Group membership
            (length (cdr (assoc 'workers neovm--test-reg-groups)))
            (length (cdr (assoc 'monitors neovm--test-reg-groups)))
            ;; Broadcast to workers returned 3 each time
            (funcall 'neovm--test-reg-broadcast 'workers '(:type :reset)))))
    (fmakunbound 'neovm--test-reg-spawn)
    (fmakunbound 'neovm--test-reg-broadcast)
    (fmakunbound 'neovm--test-reg-tick)
    (fmakunbound 'neovm--test-reg-group-states)
    (makunbound 'neovm--test-reg-actors)
    (makunbound 'neovm--test-reg-groups)))"#;
    let expect = expect_test::expect![[
        r#""OK (9 ((w3 (:sum 30) 2) (w2 (:sum 30) 2) (w1 (:sum 130) 3)) ((m2 (:last-event :health-check) 1) (m1 (:last-event :health-check) 1)) 3 2 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Request-reply pattern with correlation IDs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_actor_request_reply() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--test-rr-actors nil)
  (defvar neovm--test-rr-next-id 0)

  (fset 'neovm--test-rr-spawn
    (lambda (name handler)
      (setq neovm--test-rr-actors
            (cons (cons name (list :handler handler :inbox nil :outbox nil))
                  neovm--test-rr-actors))))

  (fset 'neovm--test-rr-send
    (lambda (to msg)
      (let ((actor (cdr (assoc to neovm--test-rr-actors))))
        (when actor
          (plist-put actor :inbox
                     (append (plist-get actor :inbox) (list msg)))))))

  ;; Request: send with correlation ID and reply-to
  (fset 'neovm--test-rr-request
    (lambda (from to payload)
      (setq neovm--test-rr-next-id (1+ neovm--test-rr-next-id))
      (let ((msg (list :type :request
                       :id neovm--test-rr-next-id
                       :from from
                       :payload payload)))
        (funcall 'neovm--test-rr-send to msg)
        neovm--test-rr-next-id)))

  ;; Process all messages for an actor
  (fset 'neovm--test-rr-process
    (lambda (name)
      (let ((actor (cdr (assoc name neovm--test-rr-actors))))
        (when actor
          (let ((inbox (plist-get actor :inbox))
                (handler (plist-get actor :handler)))
            (plist-put actor :inbox nil)
            (dolist (msg inbox)
              (let ((type (plist-get msg :type)))
                (cond
                  ((eq type :request)
                   ;; Process request, send reply
                   (let* ((result (funcall handler (plist-get msg :payload)))
                          (reply (list :type :reply
                                       :id (plist-get msg :id)
                                       :result result)))
                     (funcall 'neovm--test-rr-send (plist-get msg :from) reply)))
                  ((eq type :reply)
                   ;; Store reply in outbox keyed by ID
                   (plist-put actor :outbox
                              (cons (cons (plist-get msg :id) (plist-get msg :result))
                                    (plist-get actor :outbox))))))))))))

  ;; Get reply by correlation ID
  (fset 'neovm--test-rr-get-reply
    (lambda (name id)
      (let ((actor (cdr (assoc name neovm--test-rr-actors))))
        (when actor
          (cdr (assoc id (plist-get actor :outbox)))))))

  (unwind-protect
      (progn
        (setq neovm--test-rr-actors nil)
        (setq neovm--test-rr-next-id 0)

        ;; Math service: handles :add, :mul, :square
        (funcall 'neovm--test-rr-spawn 'math-service
          (lambda (payload)
            (let ((op (plist-get payload :op))
                  (args (plist-get payload :args)))
              (cond
                ((eq op :add) (apply '+ args))
                ((eq op :mul) (apply '* args))
                ((eq op :square) (* (car args) (car args)))
                (t (list :error "unknown op"))))))

        ;; String service
        (funcall 'neovm--test-rr-spawn 'string-service
          (lambda (payload)
            (let ((op (plist-get payload :op))
                  (args (plist-get payload :args)))
              (cond
                ((eq op :concat) (apply 'concat args))
                ((eq op :upper) (upcase (car args)))
                ((eq op :length) (length (car args)))
                (t (list :error "unknown op"))))))

        ;; Client actor
        (funcall 'neovm--test-rr-spawn 'client (lambda (p) p))

        ;; Send requests
        (let ((id1 (funcall 'neovm--test-rr-request 'client 'math-service
                            '(:op :add :args (10 20 30))))
              (id2 (funcall 'neovm--test-rr-request 'client 'math-service
                            '(:op :mul :args (2 3 4))))
              (id3 (funcall 'neovm--test-rr-request 'client 'math-service
                            '(:op :square :args (7))))
              (id4 (funcall 'neovm--test-rr-request 'client 'string-service
                            '(:op :concat :args ("hello" " " "world"))))
              (id5 (funcall 'neovm--test-rr-request 'client 'string-service
                            '(:op :upper :args ("hello")))))
          ;; Process services
          (funcall 'neovm--test-rr-process 'math-service)
          (funcall 'neovm--test-rr-process 'string-service)
          ;; Process client to receive replies
          (funcall 'neovm--test-rr-process 'client)

          (list
            ;; Replies by correlation ID
            (funcall 'neovm--test-rr-get-reply 'client id1)   ;; 60
            (funcall 'neovm--test-rr-get-reply 'client id2)   ;; 24
            (funcall 'neovm--test-rr-get-reply 'client id3)   ;; 49
            (funcall 'neovm--test-rr-get-reply 'client id4)   ;; "hello world"
            (funcall 'neovm--test-rr-get-reply 'client id5)   ;; "HELLO"
            ;; Correlation IDs are sequential
            (list id1 id2 id3 id4 id5))))
    (fmakunbound 'neovm--test-rr-spawn)
    (fmakunbound 'neovm--test-rr-send)
    (fmakunbound 'neovm--test-rr-request)
    (fmakunbound 'neovm--test-rr-process)
    (fmakunbound 'neovm--test-rr-get-reply)
    (makunbound 'neovm--test-rr-actors)
    (makunbound 'neovm--test-rr-next-id)))"#;
    let expect = expect_test::expect![[r#""OK (60 24 49 \"hello world\" \"HELLO\" (1 2 3 4 5))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Actor pipeline: chain of actors processing in sequence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_actor_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--test-pipe-actors nil)

  (fset 'neovm--test-pipe-spawn
    (lambda (name transform next)
      (setq neovm--test-pipe-actors
            (cons (cons name (list :transform transform :next next
                                   :inbox nil :results nil))
                  neovm--test-pipe-actors))))

  (fset 'neovm--test-pipe-send
    (lambda (name msg)
      (let ((actor (cdr (assoc name neovm--test-pipe-actors))))
        (when actor
          (plist-put actor :inbox
                     (append (plist-get actor :inbox) (list msg)))))))

  (fset 'neovm--test-pipe-tick
    (lambda ()
      (let ((processed 0))
        (dolist (entry neovm--test-pipe-actors)
          (let ((actor (cdr entry)))
            (when (plist-get actor :inbox)
              (let* ((msg (car (plist-get actor :inbox)))
                     (transform (plist-get actor :transform))
                     (next (plist-get actor :next))
                     (result (funcall transform msg)))
                (plist-put actor :inbox (cdr (plist-get actor :inbox)))
                (if next
                    (funcall 'neovm--test-pipe-send next result)
                  ;; Terminal actor: store result
                  (plist-put actor :results
                             (append (plist-get actor :results) (list result))))
                (setq processed (1+ processed))))))
        processed)))

  (unwind-protect
      (progn
        (setq neovm--test-pipe-actors nil)

        ;; Build pipeline: validate -> transform -> format -> sink
        ;; Sink is the terminal (no next)
        (funcall 'neovm--test-pipe-spawn 'sink
          (lambda (msg) msg)
          nil)

        ;; Formatter: convert to string
        (funcall 'neovm--test-pipe-spawn 'formatter
          (lambda (msg)
            (format "Result: %s = %d" (plist-get msg :label) (plist-get msg :value)))
          'sink)

        ;; Transformer: compute derived values
        (funcall 'neovm--test-pipe-spawn 'transformer
          (lambda (msg)
            (let ((x (plist-get msg :value)))
              (list :label (format "%d^2" x)
                    :value (* x x)
                    :original x)))
          'formatter)

        ;; Validator: filter out negative values, pass through positive
        (funcall 'neovm--test-pipe-spawn 'validator
          (lambda (msg)
            (let ((val (plist-get msg :value)))
              (if (>= val 0)
                  msg
                ;; Send error to sink directly
                (funcall 'neovm--test-pipe-send 'sink
                         (format "Error: negative value %d" val))
                nil)))
          'transformer)

        ;; Feed data into pipeline
        (dolist (v '(3 -1 5 0 -2 7 10))
          (funcall 'neovm--test-pipe-send 'validator
                   (list :value v :source "test")))

        ;; Run enough ticks to process everything through pipeline
        (let ((total 0))
          (dotimes (i 30)
            (setq total (+ total (funcall 'neovm--test-pipe-tick))))

          ;; Get results from sink
          (let ((sink (cdr (assoc 'sink neovm--test-pipe-actors))))
            (list
              ;; Total messages processed across all actors
              total
              ;; Sink results (positive values squared + error messages)
              (plist-get sink :results)
              ;; Number of results
              (length (plist-get sink :results))))))
    (fmakunbound 'neovm--test-pipe-spawn)
    (fmakunbound 'neovm--test-pipe-send)
    (fmakunbound 'neovm--test-pipe-tick)
    (makunbound 'neovm--test-pipe-actors)))"#;
    let expect =
        expect_test::expect![[r#""ERR (error \"Format specifier doesn’t match argument type\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Actor with stash: defer messages for later processing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_actor_stash_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--test-stash-actors nil)

  (fset 'neovm--test-stash-spawn
    (lambda (name handler state)
      (setq neovm--test-stash-actors
            (cons (cons name (list :handler handler :state state
                                   :inbox nil :stash nil :log nil))
                  neovm--test-stash-actors))))

  (fset 'neovm--test-stash-send
    (lambda (name msg)
      (let ((actor (cdr (assoc name neovm--test-stash-actors))))
        (when actor
          (plist-put actor :inbox
                     (append (plist-get actor :inbox) (list msg)))))))

  ;; Unstash: move stashed messages back to inbox (front)
  (fset 'neovm--test-stash-unstash
    (lambda (name)
      (let ((actor (cdr (assoc name neovm--test-stash-actors))))
        (when actor
          (plist-put actor :inbox
                     (append (plist-get actor :stash) (plist-get actor :inbox)))
          (plist-put actor :stash nil)))))

  (fset 'neovm--test-stash-step
    (lambda (name)
      (let ((actor (cdr (assoc name neovm--test-stash-actors))))
        (when (and actor (plist-get actor :inbox))
          (let* ((msg (car (plist-get actor :inbox)))
                 (handler (plist-get actor :handler))
                 (state (plist-get actor :state))
                 (result (funcall handler state msg)))
            (plist-put actor :inbox (cdr (plist-get actor :inbox)))
            (let ((action (car result))
                  (new-state (nth 1 result))
                  (log-entry (nth 2 result)))
              (cond
                ((eq action :process)
                 (plist-put actor :state new-state)
                 (when log-entry
                   (plist-put actor :log
                              (append (plist-get actor :log) (list log-entry)))))
                ((eq action :stash)
                 ;; Put message in stash for later
                 (plist-put actor :stash
                            (append (plist-get actor :stash) (list msg)))
                 (when log-entry
                   (plist-put actor :log
                              (append (plist-get actor :log) (list log-entry)))))
                ((eq action :unstash-and-process)
                 (plist-put actor :state new-state)
                 (funcall 'neovm--test-stash-unstash name)
                 (when log-entry
                   (plist-put actor :log
                              (append (plist-get actor :log) (list log-entry)))))))
            t)))))

  (unwind-protect
      (progn
        (setq neovm--test-stash-actors nil)

        ;; Database actor: must be :init before processing :query/:write
        ;; Stashes queries/writes received before init
        (funcall 'neovm--test-stash-spawn 'database
          (lambda (state msg)
            (let ((type (plist-get msg :type))
                  (initialized (plist-get state :initialized)))
              (cond
                ;; Init message
                ((eq type :init)
                 (list :unstash-and-process
                       (plist-put (copy-sequence state) :initialized t)
                       "initialized"))
                ;; Not initialized: stash
                ((not initialized)
                 (list :stash state
                       (format "stashed %s" type)))
                ;; Query
                ((eq type :query)
                 (list :process
                       (plist-put (copy-sequence state) :queries
                                  (1+ (or (plist-get state :queries) 0)))
                       (format "query: %s" (plist-get msg :sql))))
                ;; Write
                ((eq type :write)
                 (list :process
                       (plist-put (copy-sequence state) :writes
                                  (1+ (or (plist-get state :writes) 0)))
                       (format "write: %s" (plist-get msg :table))))
                (t (list :process state nil)))))
          '(:initialized nil :queries 0 :writes 0))

        ;; Send queries BEFORE init (should be stashed)
        (funcall 'neovm--test-stash-send 'database '(:type :query :sql "SELECT 1"))
        (funcall 'neovm--test-stash-send 'database '(:type :write :table "users"))
        (funcall 'neovm--test-stash-send 'database '(:type :query :sql "SELECT 2"))
        ;; Now send init
        (funcall 'neovm--test-stash-send 'database '(:type :init))
        ;; Send more queries after init
        (funcall 'neovm--test-stash-send 'database '(:type :query :sql "SELECT 3"))

        ;; Process all messages
        (dotimes (i 10)
          (funcall 'neovm--test-stash-step 'database))

        (let ((actor (cdr (assoc 'database neovm--test-stash-actors))))
          (list
            ;; Final state
            (plist-get (plist-get actor :state) :initialized)
            (plist-get (plist-get actor :state) :queries)
            (plist-get (plist-get actor :state) :writes)
            ;; Log shows stash, init, then processing of stashed messages
            (plist-get actor :log)
            ;; Stash should be empty after unstash
            (length (plist-get actor :stash)))))
    (fmakunbound 'neovm--test-stash-spawn)
    (fmakunbound 'neovm--test-stash-send)
    (fmakunbound 'neovm--test-stash-unstash)
    (fmakunbound 'neovm--test-stash-step)
    (makunbound 'neovm--test-stash-actors)))"#;
    let expect = expect_test::expect![[
        r#""OK (t 3 1 (\"stashed :query\" \"stashed :write\" \"stashed :query\" \"initialized\" \"query: SELECT 1\" \"write: users\" \"query: SELECT 2\" \"query: SELECT 3\") 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
