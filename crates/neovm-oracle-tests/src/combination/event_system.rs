//! Oracle parity tests for a full event system implementation in Elisp.
//!
//! Implements an event emitter with on/off/emit, priority-based listeners,
//! once-only listeners, event bubbling with stopPropagation, wildcard
//! listeners, event history/replay, and asynchronous-like event queue
//! processing.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Event emitter: on, off, emit with multiple event types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_event_system_on_off_emit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a full event emitter supporting on/off/emit with named handlers.
    // Verify that off removes the correct handler, and emit dispatches to
    // all registered handlers for the event type.
    let form = r#"(progn
  (defvar neovm--test-es-listeners nil)
  (defvar neovm--test-es-log nil)

  (fset 'neovm--test-es-on
    (lambda (event-type name handler)
      "Register HANDLER named NAME for EVENT-TYPE."
      (let ((entry (assq event-type neovm--test-es-listeners)))
        (if entry
            (setcdr entry (append (cdr entry) (list (cons name handler))))
          (setq neovm--test-es-listeners
                (cons (list event-type (cons name handler))
                      neovm--test-es-listeners))))))

  (fset 'neovm--test-es-off
    (lambda (event-type name)
      "Remove handler named NAME from EVENT-TYPE."
      (let ((entry (assq event-type neovm--test-es-listeners)))
        (when entry
          (setcdr entry
                  (let ((result nil))
                    (dolist (h (cdr entry))
                      (unless (eq (car h) name)
                        (setq result (cons h result))))
                    (nreverse result)))))))

  (fset 'neovm--test-es-emit
    (lambda (event-type &rest args)
      "Emit EVENT-TYPE with ARGS. Returns list of handler results."
      (let ((entry (assq event-type neovm--test-es-listeners))
            (results nil))
        (when entry
          (dolist (h (cdr entry))
            (let ((result (apply (cdr h) event-type args)))
              (setq neovm--test-es-log
                    (cons (list (car h) event-type result)
                          neovm--test-es-log))
              (setq results (cons result results)))))
        (nreverse results))))

  (unwind-protect
      (progn
        (setq neovm--test-es-listeners nil)
        (setq neovm--test-es-log nil)

        ;; Register handlers
        (funcall 'neovm--test-es-on 'click 'logger
                 (lambda (evt &rest args)
                   (format "LOG click: %S" args)))
        (funcall 'neovm--test-es-on 'click 'counter
                 (lambda (evt &rest args) (length args)))
        (funcall 'neovm--test-es-on 'click 'echo
                 (lambda (evt &rest args) (car args)))
        (funcall 'neovm--test-es-on 'keydown 'key-handler
                 (lambda (evt &rest args)
                   (format "KEY: %s" (car args))))

        ;; Emit click with 3 args: all 3 handlers run
        (let ((r1 (funcall 'neovm--test-es-emit 'click 'x 100 'y)))
          ;; Remove counter handler
          (funcall 'neovm--test-es-off 'click 'counter)

          ;; Emit again: only 2 handlers
          (let ((r2 (funcall 'neovm--test-es-emit 'click 'a 'b)))
            ;; Emit keydown
            (let ((r3 (funcall 'neovm--test-es-emit 'keydown "Enter")))
              ;; Emit unknown: no handlers
              (let ((r4 (funcall 'neovm--test-es-emit 'mousemove 50 50)))
                (list r1 (length r1)
                      r2 (length r2)
                      r3 r4
                      (length neovm--test-es-log)
                      (nreverse neovm--test-es-log)))))))
    (fmakunbound 'neovm--test-es-on)
    (fmakunbound 'neovm--test-es-off)
    (fmakunbound 'neovm--test-es-emit)
    (makunbound 'neovm--test-es-listeners)
    (makunbound 'neovm--test-es-log)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"LOG click: (x 100 y)\" 3 x) 3 (\"LOG click: (a b)\" a) 2 (\"KEY: Enter\") nil 6 ((logger click \"LOG click: (x 100 y)\") (counter click 3) (echo click x) (logger click \"LOG click: (a b)\") (echo click a) (key-handler keydown \"KEY: Enter\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Priority-based listeners: handlers execute in priority order
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_event_system_priority_listeners() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Each listener has a numeric priority. Lower number = higher priority.
    // When an event is emitted, handlers execute in priority order.
    let form = r#"(progn
  (defvar neovm--test-ep-listeners nil)

  (fset 'neovm--test-ep-on
    (lambda (event-type name priority handler)
      "Register handler with PRIORITY (lower = runs first)."
      (let ((entry (assq event-type neovm--test-ep-listeners)))
        (let ((new-handler (list :name name :priority priority :fn handler)))
          (if entry
              (let ((sorted (sort (cons new-handler (cdr entry))
                                  (lambda (a b)
                                    (< (plist-get a :priority)
                                       (plist-get b :priority))))))
                (setcdr entry sorted))
            (setq neovm--test-ep-listeners
                  (cons (list event-type new-handler)
                        neovm--test-ep-listeners)))))))

  (fset 'neovm--test-ep-emit
    (lambda (event-type data)
      "Emit event, run handlers in priority order, return execution trace."
      (let ((entry (assq event-type neovm--test-ep-listeners))
            (trace nil))
        (when entry
          (dolist (h (cdr entry))
            (let ((result (funcall (plist-get h :fn) data)))
              (setq trace
                    (cons (list (plist-get h :name)
                                (plist-get h :priority)
                                result)
                          trace)))))
        (nreverse trace))))

  (unwind-protect
      (progn
        (setq neovm--test-ep-listeners nil)

        ;; Register handlers with different priorities (out of order)
        (funcall 'neovm--test-ep-on 'request 'auth 10
                 (lambda (data)
                   (if (equal (plist-get data :token) "valid")
                       (list 'pass 'auth)
                     (list 'fail 'auth))))
        (funcall 'neovm--test-ep-on 'request 'logging 50
                 (lambda (data)
                   (format "LOG: %s %s" (plist-get data :method)
                           (plist-get data :path))))
        (funcall 'neovm--test-ep-on 'request 'rate-limit 5
                 (lambda (data)
                   (if (> (plist-get data :count) 100)
                       (list 'blocked 'rate-limit)
                     (list 'pass 'rate-limit))))
        (funcall 'neovm--test-ep-on 'request 'handler 30
                 (lambda (data)
                   (format "200 OK: %s" (plist-get data :path))))
        (funcall 'neovm--test-ep-on 'request 'metrics 99
                 (lambda (data)
                   (list 'metric (plist-get data :method))))

        ;; Emit: handlers should run in priority order: 5, 10, 30, 50, 99
        (let ((trace1 (funcall 'neovm--test-ep-emit 'request
                               '(:method "GET" :path "/api/users"
                                 :token "valid" :count 42)))
              ;; Emit with failing auth
              (trace2 (funcall 'neovm--test-ep-emit 'request
                               '(:method "POST" :path "/api/admin"
                                 :token "invalid" :count 5)))
              ;; Emit with rate limit exceeded
              (trace3 (funcall 'neovm--test-ep-emit 'request
                               '(:method "GET" :path "/api/data"
                                 :token "valid" :count 200))))
          ;; Verify execution order by checking names in trace
          (list (mapcar 'car trace1)   ;; should be (rate-limit auth handler logging metrics)
                trace1
                trace2
                trace3)))
    (fmakunbound 'neovm--test-ep-on)
    (fmakunbound 'neovm--test-ep-emit)
    (makunbound 'neovm--test-ep-listeners)))"#;
    let expect = expect_test::expect![[
        r#""OK ((rate-limit auth handler logging metrics) ((rate-limit 5 (pass rate-limit)) (auth 10 (pass auth)) (handler 30 \"200 OK: /api/users\") (logging 50 \"LOG: GET /api/users\") (metrics 99 (metric \"GET\"))) ((rate-limit 5 (pass rate-limit)) (auth 10 (fail auth)) (handler 30 \"200 OK: /api/admin\") (logging 50 \"LOG: POST /api/admin\") (metrics 99 (metric \"POST\"))) ((rate-limit 5 (blocked rate-limit)) (auth 10 (pass auth)) (handler 30 \"200 OK: /api/data\") (logging 50 \"LOG: GET /api/data\") (metrics 99 (metric \"GET\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Once-only listeners that auto-remove after first invocation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_event_system_once_listeners() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Listeners registered with `once` flag are automatically removed
    // after their first invocation. Mix once and persistent listeners.
    let form = r#"(progn
  (defvar neovm--test-eo-listeners nil)
  (defvar neovm--test-eo-results nil)

  (fset 'neovm--test-eo-on
    (lambda (event name handler once)
      "Register handler. If ONCE is non-nil, auto-remove after first call."
      (setq neovm--test-eo-listeners
            (cons (list :event event :name name :fn handler :once once)
                  neovm--test-eo-listeners))))

  (fset 'neovm--test-eo-emit
    (lambda (event data)
      "Emit EVENT with DATA. Once-handlers are removed after firing."
      (let ((remaining nil)
            (fired nil))
        (dolist (listener (reverse neovm--test-eo-listeners))
          (if (eq (plist-get listener :event) event)
              (progn
                (let ((result (funcall (plist-get listener :fn) data)))
                  (setq fired (cons (list (plist-get listener :name) result) fired)))
                ;; Keep only if not a once-handler
                (unless (plist-get listener :once)
                  (setq remaining (cons listener remaining))))
            ;; Non-matching event types are always kept
            (setq remaining (cons listener remaining))))
        (setq neovm--test-eo-listeners (nreverse remaining))
        (nreverse fired))))

  (unwind-protect
      (progn
        (setq neovm--test-eo-listeners nil)
        (setq neovm--test-eo-results nil)

        ;; Persistent handler
        (funcall 'neovm--test-eo-on 'data 'persistent
                 (lambda (d) (format "persistent: %s" d)) nil)
        ;; Two once-only handlers
        (funcall 'neovm--test-eo-on 'data 'init-once
                 (lambda (d) (format "init: %s" d)) t)
        (funcall 'neovm--test-eo-on 'data 'setup-once
                 (lambda (d) (format "setup: %s" d)) t)
        ;; Another persistent on different event
        (funcall 'neovm--test-eo-on 'error 'error-handler
                 (lambda (d) (format "error: %s" d)) nil)

        ;; First emit: all 3 data handlers fire
        (let ((r1 (funcall 'neovm--test-eo-emit 'data "msg1"))
              (count1 (length neovm--test-eo-listeners)))
          ;; Second emit: only persistent fires (once-handlers removed)
          (let ((r2 (funcall 'neovm--test-eo-emit 'data "msg2"))
                (count2 (length neovm--test-eo-listeners)))
            ;; Third emit: still only persistent
            (let ((r3 (funcall 'neovm--test-eo-emit 'data "msg3"))
                  ;; Error event should still work
                  (r4 (funcall 'neovm--test-eo-emit 'error "boom")))
              (list r1 count1     ;; 3 results, 2 listeners remaining (persistent + error-handler)
                    r2 count2     ;; 1 result, 2 listeners remaining
                    r3 r4
                    (length neovm--test-eo-listeners))))))
    (fmakunbound 'neovm--test-eo-on)
    (fmakunbound 'neovm--test-eo-emit)
    (makunbound 'neovm--test-eo-listeners)
    (makunbound 'neovm--test-eo-results)))"#;
    let expect = expect_test::expect![[
        r#""OK (((persistent \"persistent: msg1\") (init-once \"init: msg1\") (setup-once \"setup: msg1\")) 2 ((persistent \"persistent: msg2\")) 2 ((persistent \"persistent: msg3\")) ((error-handler \"error: boom\")) 2)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Event bubbling with stopPropagation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_event_system_bubbling_stop_propagation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Events bubble from child to parent through a node hierarchy.
    // Any handler can return 'stop to prevent further propagation.
    let form = r#"(progn
  (fset 'neovm--test-eb-make-node
    (lambda (name parent handlers)
      "Create a node with NAME, PARENT ref, and list of HANDLERS."
      (list :name name :parent parent :handlers handlers)))

  (fset 'neovm--test-eb-bubble
    (lambda (node event)
      "Bubble EVENT from NODE up to root. Returns trace of (node-name result) pairs."
      (let ((trace nil)
            (current node)
            (stopped nil))
        (while (and current (not stopped))
          (let ((name (plist-get current :name))
                (handlers (plist-get current :handlers)))
            (dolist (h handlers)
              (unless stopped
                (let ((result (funcall h name event)))
                  (setq trace (cons (list name (cdr result)) trace))
                  (when (eq (car result) 'stop)
                    (setq stopped t))))))
          (unless stopped
            (setq current (plist-get current :parent))))
        (list :trace (nreverse trace) :stopped stopped))))

  (unwind-protect
      (let* ((root (funcall 'neovm--test-eb-make-node 'root nil
                            (list (lambda (name evt)
                                    (cons 'continue
                                          (format "root handled %s" evt))))))
             (panel (funcall 'neovm--test-eb-make-node 'panel root
                             (list (lambda (name evt)
                                     (if (equal evt "restricted")
                                         (cons 'stop
                                               (format "panel blocked %s" evt))
                                       (cons 'continue
                                             (format "panel passed %s" evt))))
                                   (lambda (name evt)
                                     (cons 'continue
                                           (format "panel-logger: %s" evt))))))
             (button (funcall 'neovm--test-eb-make-node 'button panel
                              (list (lambda (name evt)
                                      (cons 'continue
                                            (format "button clicked: %s" evt))))))
             (icon (funcall 'neovm--test-eb-make-node 'icon button
                            (list (lambda (name evt)
                                    (cons 'continue
                                          (format "icon touched: %s" evt)))))))
        (list
          ;; Normal event bubbles all the way: icon -> button -> panel -> root
          (funcall 'neovm--test-eb-bubble icon "click")
          ;; Restricted event stopped at panel: icon -> button -> panel(stop)
          (funcall 'neovm--test-eb-bubble icon "restricted")
          ;; Start from button (skip icon)
          (funcall 'neovm--test-eb-bubble button "tap")
          ;; Start from root (no parent to bubble to)
          (funcall 'neovm--test-eb-bubble root "resize")
          ;; Panel has two handlers: both run unless first stops
          (funcall 'neovm--test-eb-bubble panel "normal")))
    (fmakunbound 'neovm--test-eb-make-node)
    (fmakunbound 'neovm--test-eb-bubble)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:trace ((icon \"icon touched: click\") (button \"button clicked: click\") (panel \"panel passed click\") (panel \"panel-logger: click\") (root \"root handled click\")) :stopped nil) (:trace ((icon \"icon touched: restricted\") (button \"button clicked: restricted\") (panel \"panel blocked restricted\")) :stopped t) (:trace ((button \"button clicked: tap\") (panel \"panel passed tap\") (panel \"panel-logger: tap\") (root \"root handled tap\")) :stopped nil) (:trace ((root \"root handled resize\")) :stopped nil) (:trace ((panel \"panel passed normal\") (panel \"panel-logger: normal\") (root \"root handled normal\")) :stopped nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Wildcard listeners: match all event types with "*"
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_event_system_wildcard_listeners() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Wildcard listeners (registered for event type '*') receive all events.
    // They run after type-specific listeners.
    let form = r#"(progn
  (defvar neovm--test-ew-listeners nil)

  (fset 'neovm--test-ew-on
    (lambda (event-type name handler)
      (setq neovm--test-ew-listeners
            (cons (list :type event-type :name name :fn handler)
                  neovm--test-ew-listeners))))

  (fset 'neovm--test-ew-emit
    (lambda (event-type data)
      "Emit event. Run type-specific handlers first, then wildcards."
      (let ((specific nil)
            (wildcards nil)
            (results nil))
        ;; Separate specific and wildcard listeners
        (dolist (l (reverse neovm--test-ew-listeners))
          (cond
            ((eq (plist-get l :type) event-type)
             (setq specific (cons l specific)))
            ((eq (plist-get l :type) '*)
             (setq wildcards (cons l wildcards)))))
        ;; Run specific first
        (dolist (l (nreverse specific))
          (setq results
                (cons (list (plist-get l :name)
                            (funcall (plist-get l :fn) event-type data))
                      results)))
        ;; Then wildcards
        (dolist (l (nreverse wildcards))
          (setq results
                (cons (list (plist-get l :name)
                            (funcall (plist-get l :fn) event-type data))
                      results)))
        (nreverse results))))

  (unwind-protect
      (progn
        (setq neovm--test-ew-listeners nil)

        ;; Type-specific handlers
        (funcall 'neovm--test-ew-on 'click 'click-handler
                 (lambda (evt data) (format "click: %S" data)))
        (funcall 'neovm--test-ew-on 'keypress 'key-handler
                 (lambda (evt data) (format "key: %S" data)))

        ;; Wildcard handlers
        (funcall 'neovm--test-ew-on '* 'audit-log
                 (lambda (evt data) (format "AUDIT[%s]: %S" evt data)))
        (funcall 'neovm--test-ew-on '* 'metrics
                 (lambda (evt data) (list 'metric evt)))

        (let ((r1 (funcall 'neovm--test-ew-emit 'click '(x 100 y 200)))
              (r2 (funcall 'neovm--test-ew-emit 'keypress '(key "Enter")))
              ;; Unknown event type: only wildcards fire
              (r3 (funcall 'neovm--test-ew-emit 'scroll '(delta 50))))
          (list r1 (length r1)    ;; click-handler + 2 wildcards = 3
                r2 (length r2)    ;; key-handler + 2 wildcards = 3
                r3 (length r3)))) ;; 0 specific + 2 wildcards = 2
    (fmakunbound 'neovm--test-ew-on)
    (fmakunbound 'neovm--test-ew-emit)
    (makunbound 'neovm--test-ew-listeners)))"#;
    let expect = expect_test::expect![[
        r#""OK (((click-handler \"click: (x 100 y 200)\") (audit-log \"AUDIT[click]: (x 100 y 200)\") (metrics (metric click))) 3 ((key-handler \"key: (key \\\"Enter\\\")\") (audit-log \"AUDIT[keypress]: (key \\\"Enter\\\")\") (metrics (metric keypress))) 3 ((audit-log \"AUDIT[scroll]: (delta 50)\") (metrics (metric scroll))) 2)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Event history and replay
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_event_system_history_replay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Record all emitted events into a history log. Support replaying
    // the history through a new set of handlers. Verify that replay
    // produces consistent results.
    let form = r#"(progn
  (defvar neovm--test-eh-listeners nil)
  (defvar neovm--test-eh-history nil)

  (fset 'neovm--test-eh-on
    (lambda (event-type name handler)
      (setq neovm--test-eh-listeners
            (cons (list :type event-type :name name :fn handler)
                  neovm--test-eh-listeners))))

  (fset 'neovm--test-eh-clear-listeners
    (lambda ()
      (setq neovm--test-eh-listeners nil)))

  (fset 'neovm--test-eh-emit
    (lambda (event-type data)
      "Emit event, record in history, return results."
      (setq neovm--test-eh-history
            (cons (list :type event-type :data data)
                  neovm--test-eh-history))
      (let ((results nil))
        (dolist (l (reverse neovm--test-eh-listeners))
          (when (eq (plist-get l :type) event-type)
            (setq results
                  (cons (list (plist-get l :name)
                              (funcall (plist-get l :fn) event-type data))
                        results))))
        (nreverse results))))

  (fset 'neovm--test-eh-replay
    (lambda ()
      "Replay all events from history through current listeners."
      (let ((all-results nil))
        (dolist (event (reverse neovm--test-eh-history))
          (let ((evt-type (plist-get event :type))
                (evt-data (plist-get event :data))
                (results nil))
            (dolist (l (reverse neovm--test-eh-listeners))
              (when (eq (plist-get l :type) evt-type)
                (setq results
                      (cons (list (plist-get l :name)
                                  (funcall (plist-get l :fn) evt-type evt-data))
                            results))))
            (setq all-results
                  (cons (list evt-type (nreverse results))
                        all-results))))
        (nreverse all-results))))

  (unwind-protect
      (progn
        (setq neovm--test-eh-listeners nil)
        (setq neovm--test-eh-history nil)

        ;; Phase 1: emit events with handler set A
        (funcall 'neovm--test-eh-on 'login 'auth-v1
                 (lambda (evt data)
                   (format "v1-auth: %s" (plist-get data :user))))
        (funcall 'neovm--test-eh-on 'purchase 'billing-v1
                 (lambda (evt data)
                   (format "v1-bill: $%d" (plist-get data :amount))))

        (let ((orig-r1 (funcall 'neovm--test-eh-emit 'login '(:user "alice")))
              (orig-r2 (funcall 'neovm--test-eh-emit 'purchase '(:amount 50)))
              (orig-r3 (funcall 'neovm--test-eh-emit 'login '(:user "bob")))
              (orig-r4 (funcall 'neovm--test-eh-emit 'purchase '(:amount 120))))

          ;; Phase 2: swap handlers to v2 and replay
          (funcall 'neovm--test-eh-clear-listeners)
          (funcall 'neovm--test-eh-on 'login 'auth-v2
                   (lambda (evt data)
                     (format "v2-auth: user=%s" (plist-get data :user))))
          (funcall 'neovm--test-eh-on 'purchase 'billing-v2
                   (lambda (evt data)
                     (format "v2-bill: amount=%d" (plist-get data :amount))))

          (let ((replay-results (funcall 'neovm--test-eh-replay)))
            (list (length neovm--test-eh-history)
                  orig-r1 orig-r2 orig-r3 orig-r4
                  replay-results
                  (length replay-results)))))
    (fmakunbound 'neovm--test-eh-on)
    (fmakunbound 'neovm--test-eh-clear-listeners)
    (fmakunbound 'neovm--test-eh-emit)
    (fmakunbound 'neovm--test-eh-replay)
    (makunbound 'neovm--test-eh-listeners)
    (makunbound 'neovm--test-eh-history)))"#;
    let expect = expect_test::expect![[
        r#""OK (4 ((auth-v1 \"v1-auth: alice\")) ((billing-v1 \"v1-bill: $50\")) ((auth-v1 \"v1-auth: bob\")) ((billing-v1 \"v1-bill: $120\")) ((login ((auth-v2 \"v2-auth: user=alice\"))) (purchase ((billing-v2 \"v2-bill: amount=50\"))) (login ((auth-v2 \"v2-auth: user=bob\"))) (purchase ((billing-v2 \"v2-bill: amount=120\")))) 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Asynchronous-like event queue with deferred processing and cascading
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_event_system_async_queue() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulated async event processing: events go into a queue and are
    // processed in batches. Handlers can enqueue new events (cascading).
    // A tick limit prevents infinite loops.
    let form = r#"(progn
  (defvar neovm--test-eq-queue nil)
  (defvar neovm--test-eq-handlers nil)
  (defvar neovm--test-eq-processed nil)

  (fset 'neovm--test-eq-enqueue
    (lambda (event)
      (setq neovm--test-eq-queue
            (append neovm--test-eq-queue (list event)))))

  (fset 'neovm--test-eq-register
    (lambda (event-type handler)
      (setq neovm--test-eq-handlers
            (cons (cons event-type handler) neovm--test-eq-handlers))))

  (fset 'neovm--test-eq-tick
    (lambda ()
      "Process one event from the queue. Returns processed event or nil."
      (when neovm--test-eq-queue
        (let* ((event (car neovm--test-eq-queue))
               (evt-type (plist-get event :type)))
          (setq neovm--test-eq-queue (cdr neovm--test-eq-queue))
          (let ((results nil))
            (dolist (h neovm--test-eq-handlers)
              (when (eq (car h) evt-type)
                (let ((result (funcall (cdr h) event)))
                  (setq results (cons result results))
                  ;; If result is a new event (has :type), enqueue it
                  (when (and (listp result) (plist-get result :type))
                    (funcall 'neovm--test-eq-enqueue result)))))
            (let ((record (list :event event :results (nreverse results))))
              (setq neovm--test-eq-processed
                    (cons record neovm--test-eq-processed))
              record))))))

  (fset 'neovm--test-eq-drain
    (lambda (max-ticks)
      "Process all events up to MAX-TICKS. Returns number processed."
      (let ((count 0))
        (while (and neovm--test-eq-queue (< count max-ticks))
          (funcall 'neovm--test-eq-tick)
          (setq count (1+ count)))
        count)))

  (unwind-protect
      (progn
        (setq neovm--test-eq-queue nil)
        (setq neovm--test-eq-handlers nil)
        (setq neovm--test-eq-processed nil)

        ;; Handler: order creates an invoice event
        (funcall 'neovm--test-eq-register 'order
                 (lambda (evt)
                   (list :type 'invoice
                         :order-id (plist-get evt :id)
                         :total (plist-get evt :amount))))
        ;; Handler: invoice creates a notification event
        (funcall 'neovm--test-eq-register 'invoice
                 (lambda (evt)
                   (list :type 'notification
                         :msg (format "Invoice for order %s: $%d"
                                      (plist-get evt :order-id)
                                      (plist-get evt :total)))))
        ;; Handler: notification is terminal (no cascade)
        (funcall 'neovm--test-eq-register 'notification
                 (lambda (evt)
                   (format "SENT: %s" (plist-get evt :msg))))
        ;; Handler: refund is terminal
        (funcall 'neovm--test-eq-register 'refund
                 (lambda (evt)
                   (format "REFUND: $%d for %s"
                           (plist-get evt :amount)
                           (plist-get evt :id))))

        ;; Enqueue initial events
        (funcall 'neovm--test-eq-enqueue '(:type order :id "O1" :amount 100))
        (funcall 'neovm--test-eq-enqueue '(:type order :id "O2" :amount 250))
        (funcall 'neovm--test-eq-enqueue '(:type refund :id "R1" :amount 50))

        ;; Drain: order -> invoice -> notification = 3 events per order
        ;; Plus 1 refund = total 3+3+1 = 7
        (let ((processed-count (funcall 'neovm--test-eq-drain 20))
              (remaining (length neovm--test-eq-queue)))
          (list processed-count
                remaining
                (length neovm--test-eq-processed)
                (nreverse neovm--test-eq-processed))))
    (fmakunbound 'neovm--test-eq-enqueue)
    (fmakunbound 'neovm--test-eq-register)
    (fmakunbound 'neovm--test-eq-tick)
    (fmakunbound 'neovm--test-eq-drain)
    (makunbound 'neovm--test-eq-queue)
    (makunbound 'neovm--test-eq-handlers)
    (makunbound 'neovm--test-eq-processed)))"#;
    let expect = expect_test::expect![[
        r#""OK (7 0 7 ((:event (:type order :id \"O1\" :amount 100) :results ((:type invoice :order-id \"O1\" :total 100))) (:event (:type order :id \"O2\" :amount 250) :results ((:type invoice :order-id \"O2\" :total 250))) (:event (:type refund :id \"R1\" :amount 50) :results (\"REFUND: $50 for R1\")) (:event (:type invoice :order-id \"O1\" :total 100) :results ((:type notification :msg \"Invoice for order O1: $100\"))) (:event (:type invoice :order-id \"O2\" :total 250) :results ((:type notification :msg \"Invoice for order O2: $250\"))) (:event (:type notification :msg \"Invoice for order O1: $100\") :results (\"SENT: Invoice for order O1: $100\")) (:event (:type notification :msg \"Invoice for order O2: $250\") :results (\"SENT: Invoice for order O2: $250\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
