//! Oracle parity tests for event-driven programming patterns:
//! event emitter with subscribe/unsubscribe/emit, event bubbling,
//! event filtering, once-only handlers, event queues with deferred
//! processing, and middleware chains.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Event emitter: subscribe, unsubscribe, emit
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_event_emitter_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--test-evem-registry nil)
  (defvar neovm--test-evem-log nil)
  (unwind-protect
      (let ((make-emitter
             (lambda ()
               (setq neovm--test-evem-registry nil)
               (setq neovm--test-evem-log nil)
               t))
            (subscribe
             (lambda (event-type handler-name handler)
               (let ((entry (assq event-type neovm--test-evem-registry)))
                 (if entry
                     (setcdr entry (cons (cons handler-name handler) (cdr entry)))
                   (setq neovm--test-evem-registry
                         (cons (list event-type (cons handler-name handler))
                               neovm--test-evem-registry))))))
            (unsubscribe
             (lambda (event-type handler-name)
               (let ((entry (assq event-type neovm--test-evem-registry)))
                 (when entry
                   (setcdr entry
                           (let ((result nil))
                             (dolist (h (cdr entry))
                               (unless (eq (car h) handler-name)
                                 (setq result (cons h result))))
                             (nreverse result)))))))
            (emit
             (lambda (event-type &rest data)
               (let ((entry (assq event-type neovm--test-evem-registry)))
                 (when entry
                   (dolist (h (cdr entry))
                     (let ((result (funcall (cdr h) event-type data)))
                       (setq neovm--test-evem-log
                             (cons (list (car h) event-type result)
                                   neovm--test-evem-log)))))))))
        ;; Setup
        (funcall make-emitter)
        ;; Subscribe handlers
        (funcall subscribe 'click 'logger
                 (lambda (evt data) (format "logged click: %S" data)))
        (funcall subscribe 'click 'counter
                 (lambda (evt data) (length data)))
        (funcall subscribe 'keypress 'key-handler
                 (lambda (evt data) (format "key: %S" (car data))))
        ;; Emit events
        (funcall emit 'click 'x 100 'y 200)
        (funcall emit 'keypress "Enter")
        (funcall emit 'mousemove 50 50)  ;; no handlers → no log entry
        ;; Unsubscribe counter from click
        (funcall unsubscribe 'click 'counter)
        (funcall emit 'click 'x 300 'y 400)
        ;; Results
        (list (length neovm--test-evem-log)
              (nreverse neovm--test-evem-log)))
    (makunbound 'neovm--test-evem-registry)
    (makunbound 'neovm--test-evem-log)))"#;
    let expect = expect_test::expect![[
        r#""OK (4 ((counter click 4) (logger click \"logged click: (x 100 y 200)\") (key-handler keypress \"key: \\\"Enter\\\"\") (logger click \"logged click: (x 300 y 400)\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Event bubbling: child → parent propagation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_event_bubbling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Tree of nodes. Events bubble from leaf to root unless stopped.
    let form = r#"(let ((make-node
           (lambda (name parent handler)
             (list :name name :parent parent :handler handler)))
          (node-name (lambda (n) (plist-get n :name)))
          (node-parent (lambda (n) (plist-get n :parent)))
          (node-handler (lambda (n) (plist-get n :handler)))
          (bubble nil))
      ;; bubble: call handler at each node, propagate to parent
      ;; handler returns (stop . result) or (continue . result)
      (setq bubble
            (lambda (node event trace)
              (let* ((handler (plist-get node :handler))
                     (name (plist-get node :name))
                     (result (if handler
                                 (funcall handler name event)
                               (cons 'continue nil)))
                     (new-trace (cons (list name (cdr result)) trace)))
                (if (eq (car result) 'stop)
                    ;; Stop propagation
                    (nreverse new-trace)
                  ;; Continue to parent
                  (let ((parent (plist-get node :parent)))
                    (if parent
                        (funcall bubble parent event new-trace)
                      (nreverse new-trace)))))))
      ;; Build a tree: root → container → button
      (let* ((root (funcall make-node 'root nil
                            (lambda (name evt)
                              (cons 'continue (format "root saw %s" evt)))))
             (container (funcall make-node 'container root
                                 (lambda (name evt)
                                   (if (equal evt "dangerous")
                                       (cons 'stop (format "container blocked %s" evt))
                                     (cons 'continue (format "container passed %s" evt))))))
             (button (funcall make-node 'button container
                              (lambda (name evt)
                                (cons 'continue (format "button handled %s" evt))))))
        (list
         ;; Normal event bubbles through all three
         (funcall bubble button "click" nil)
         ;; Dangerous event stopped at container
         (funcall bubble button "dangerous" nil)
         ;; Event starting at container (no button)
         (funcall bubble container "hover" nil)
         ;; Event at root (no parent to bubble to)
         (funcall bubble root "resize" nil))))"#;
    let expect = expect_test::expect![[
        r#""OK (((button \"button handled click\") (container \"container passed click\") (root \"root saw click\")) ((button \"button handled dangerous\") (container \"container blocked dangerous\")) ((container \"container passed hover\") (root \"root saw hover\")) ((root \"root saw resize\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Event filtering: only specific event types trigger handlers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_event_filtering() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((make-filtered-handler
           (lambda (name accepted-types action)
             (list :name name :accepts accepted-types :action action)))
          (handler-accepts
           (lambda (handler event-type)
             (memq event-type (plist-get handler :accepts))))
          (run-handler
           (lambda (handler event)
             (let ((evt-type (plist-get event :type)))
               (if (memq evt-type (plist-get handler :accepts))
                   (funcall (plist-get handler :action) event)
                 nil))))
          (dispatch-event nil))
      ;; dispatch-event runs all handlers, collects non-nil results
      (setq dispatch-event
            (lambda (handlers event)
              (let ((results nil))
                (dolist (h handlers)
                  (let ((r (funcall run-handler h event)))
                    (when r
                      (setq results (cons (list (plist-get h :name) r) results)))))
                (nreverse results))))
      ;; Define handlers with different filters
      (let ((handlers
             (list
              (funcall make-filtered-handler 'security '(login logout auth-fail)
                       (lambda (e) (format "SECURITY: %s from %s"
                                           (plist-get e :type) (plist-get e :user))))
              (funcall make-filtered-handler 'analytics '(login page-view purchase)
                       (lambda (e) (format "ANALYTICS: %s" (plist-get e :type))))
              (funcall make-filtered-handler 'error-tracker '(auth-fail error crash)
                       (lambda (e) (format "ERROR: %s - %s"
                                           (plist-get e :type) (plist-get e :detail))))
              (funcall make-filtered-handler 'catch-all
                       '(login logout auth-fail page-view purchase error crash)
                       (lambda (e) (plist-get e :type))))))
        ;; Dispatch various events
        (list
         ;; login: security + analytics + catch-all
         (funcall dispatch-event handlers
                  '(:type login :user "alice" :detail nil))
         ;; page-view: analytics + catch-all
         (funcall dispatch-event handlers
                  '(:type page-view :user "bob" :detail nil))
         ;; auth-fail: security + error-tracker + catch-all
         (funcall dispatch-event handlers
                  '(:type auth-fail :user "eve" :detail "bad password"))
         ;; purchase: analytics + catch-all
         (funcall dispatch-event handlers
                  '(:type purchase :user "carol" :detail nil))
         ;; unknown event type: none match
         (funcall dispatch-event handlers
                  '(:type heartbeat :user "system" :detail nil)))))"#;
    let expect = expect_test::expect![[
        r#""OK (((security \"SECURITY: login from alice\") (analytics \"ANALYTICS: login\") (catch-all login)) ((analytics \"ANALYTICS: page-view\") (catch-all page-view)) ((security \"SECURITY: auth-fail from eve\") (error-tracker \"ERROR: auth-fail - bad password\") (catch-all auth-fail)) ((analytics \"ANALYTICS: purchase\") (catch-all purchase)) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Once-only event handlers: auto-remove after first invocation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_event_once_only_handlers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--test-once-handlers nil)
  (defvar neovm--test-once-log nil)
  (unwind-protect
      (let ((register
             (lambda (name handler once)
               (setq neovm--test-once-handlers
                     (cons (list :name name :fn handler :once once :fired nil)
                           neovm--test-once-handlers))))
            (fire nil))
        ;; fire: invoke all handlers, remove once-only after firing
        (setq fire
              (lambda (event)
                (let ((remaining nil))
                  (dolist (h (reverse neovm--test-once-handlers))
                    (let ((result (funcall (plist-get h :fn) event)))
                      (setq neovm--test-once-log
                            (cons (list (plist-get h :name) result) neovm--test-once-log))
                      ;; Keep handler only if it's NOT a once-handler
                      (unless (plist-get h :once)
                        (setq remaining (cons h remaining)))))
                  (setq neovm--test-once-handlers (nreverse remaining)))))
        ;; Setup
        (setq neovm--test-once-handlers nil)
        (setq neovm--test-once-log nil)
        ;; Register: persistent handler
        (funcall register 'persistent
                 (lambda (e) (format "persistent saw %s" e)) nil)
        ;; Register: two once-only handlers
        (funcall register 'init-once
                 (lambda (e) (format "init-once saw %s" e)) t)
        (funcall register 'setup-once
                 (lambda (e) (format "setup-once saw %s" e)) t)
        ;; Fire 1: all three run
        (funcall fire "event-1")
        (let ((count-after-1 (length neovm--test-once-handlers)))
          ;; Fire 2: only persistent remains
          (funcall fire "event-2")
          (let ((count-after-2 (length neovm--test-once-handlers)))
            ;; Fire 3: still only persistent
            (funcall fire "event-3")
            (list count-after-1     ;; 1 (persistent only)
                  count-after-2     ;; 1
                  (length neovm--test-once-log)  ;; 3+1+1 = 5 total invocations
                  (nreverse neovm--test-once-log)))))
    (makunbound 'neovm--test-once-handlers)
    (makunbound 'neovm--test-once-log)))"#;
    let expect = expect_test::expect![[
        r#""OK (1 1 5 ((persistent \"persistent saw event-1\") (init-once \"init-once saw event-1\") (setup-once \"setup-once saw event-1\") (persistent \"persistent saw event-2\") (persistent \"persistent saw event-3\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Event queue with deferred processing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_event_queue_deferred() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Events are enqueued and processed in batch. Processing can
    // enqueue new events (cascading). A max-depth limit prevents
    // infinite cascading.
    let form = r#"(let ((make-event-queue
           (lambda () (list nil nil 0)))  ;; (queue handlers processed-count)
          (eq-queue (lambda (q) (car q)))
          (eq-handlers (lambda (q) (cadr q)))
          (eq-count (lambda (q) (nth 2 q)))
          (eq-enqueue
           (lambda (q event)
             (list (append (car q) (list event)) (cadr q) (nth 2 q))))
          (eq-register
           (lambda (q event-type handler)
             (list (car q)
                   (cons (cons event-type handler) (cadr q))
                   (nth 2 q))))
          (eq-process nil))
      ;; Process all queued events. Handlers may enqueue new events.
      ;; max-depth prevents infinite loops.
      (setq eq-process
            (lambda (q max-depth)
              (let ((depth 0)
                    (queue (car q))
                    (handlers (cadr q))
                    (count (nth 2 q))
                    (all-results nil))
                (while (and queue (< depth max-depth))
                  (let ((event (car queue))
                        (new-events nil))
                    (setq queue (cdr queue))
                    (setq count (1+ count))
                    ;; Run matching handlers
                    (dolist (h handlers)
                      (when (eq (car h) (plist-get event :type))
                        (let ((result (funcall (cdr h) event)))
                          (setq all-results
                                (cons (list (plist-get event :type)
                                            (plist-get event :id)
                                            result)
                                      all-results))
                          ;; If result is a list starting with :cascade, enqueue new event
                          (when (and (listp result)
                                     (eq (car result) :cascade))
                            (setq new-events
                                  (cons (cdr result) new-events))))))
                    ;; Append cascaded events to queue
                    (setq queue (append queue (nreverse new-events))))
                  (setq depth (1+ depth)))
                (list (nreverse all-results) count (length queue)))))
      ;; Build system
      (let ((q (funcall make-event-queue)))
        ;; Register handlers
        (setq q (funcall eq-register q 'order
                         (lambda (e)
                           ;; Processing an order cascades an 'invoice event
                           (list :cascade :type 'invoice
                                 :id (format "inv-%s" (plist-get e :id))
                                 :amount (plist-get e :amount)))))
        (setq q (funcall eq-register q 'invoice
                         (lambda (e)
                           (format "Invoice %s for $%d"
                                   (plist-get e :id)
                                   (plist-get e :amount)))))
        (setq q (funcall eq-register q 'refund
                         (lambda (e)
                           (format "Refund %s: -$%d"
                                   (plist-get e :id)
                                   (plist-get e :amount)))))
        ;; Enqueue events
        (setq q (funcall eq-enqueue q '(:type order :id "O1" :amount 100)))
        (setq q (funcall eq-enqueue q '(:type order :id "O2" :amount 250)))
        (setq q (funcall eq-enqueue q '(:type refund :id "R1" :amount 50)))
        ;; Process with max-depth 10
        (funcall eq-process q 10)))"#;
    let expect = expect_test::expect![[
        r#""OK (((order \"O1\" (:cascade :type invoice :id \"inv-O1\" :amount 100)) (order \"O2\" (:cascade :type invoice :id \"inv-O2\" :amount 250)) (refund \"R1\" \"Refund R1: -$50\") (invoice \"inv-O1\" \"Invoice inv-O1 for $100\") (invoice \"inv-O2\" \"Invoice inv-O2 for $250\")) 5 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Middleware chain: each handler can modify event before next
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_event_middleware_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Each middleware receives the event, can modify it, and passes
    // to next. Like Express.js middleware or Rack middleware.
    let form = r#"(let ((make-middleware
           (lambda (name transform-fn)
             (list :name name :transform transform-fn)))
          (run-chain nil))
      ;; run-chain: fold event through each middleware in order,
      ;; collecting a trace of transformations
      (setq run-chain
            (lambda (middlewares event)
              (let ((current event)
                    (trace nil))
                (dolist (mw middlewares)
                  (let ((name (plist-get mw :name))
                        (xform (plist-get mw :transform)))
                    (let ((result (funcall xform current)))
                      (setq trace (cons (list name
                                              (copy-sequence current)
                                              (copy-sequence result))
                                        trace))
                      (setq current result))))
                (list :final current :trace (nreverse trace)))))
      ;; Build middleware chain
      (let ((chain
             (list
              ;; 1. Timestamp: add :processed-at
              (funcall make-middleware 'timestamper
                       (lambda (evt)
                         (plist-put (copy-sequence evt) :processed-at 1000)))
              ;; 2. Normalizer: downcase the :action string
              (funcall make-middleware 'normalizer
                       (lambda (evt)
                         (let ((action (plist-get evt :action)))
                           (if (stringp action)
                               (plist-put (copy-sequence evt) :action
                                          (downcase action))
                             evt))))
              ;; 3. Enricher: add :priority based on :level
              (funcall make-middleware 'enricher
                       (lambda (evt)
                         (let ((level (plist-get evt :level)))
                           (plist-put (copy-sequence evt) :priority
                                      (cond ((eq level 'critical) 1)
                                            ((eq level 'warning) 2)
                                            ((eq level 'info) 3)
                                            (t 4))))))
              ;; 4. Validator: add :valid flag
              (funcall make-middleware 'validator
                       (lambda (evt)
                         (let ((valid (and (plist-get evt :action)
                                          (plist-get evt :user)
                                          t)))
                           (plist-put (copy-sequence evt) :valid valid))))
              ;; 5. Logger: add :log-entry
              (funcall make-middleware 'logger
                       (lambda (evt)
                         (plist-put (copy-sequence evt) :log-entry
                                    (format "[%s] %s by %s (pri=%s)"
                                            (plist-get evt :processed-at)
                                            (plist-get evt :action)
                                            (plist-get evt :user)
                                            (plist-get evt :priority))))))))
        ;; Process events through the chain
        (let ((r1 (funcall run-chain chain
                           '(:action "DEPLOY" :user "admin" :level critical)))
              (r2 (funcall run-chain chain
                           '(:action "READ" :user "guest" :level info)))
              (r3 (funcall run-chain chain
                           '(:action nil :user nil :level warning))))
          (list
           ;; Final state of each event
           (plist-get r1 :final)
           (plist-get r2 :final)
           ;; r3 should have :valid nil
           (plist-get (plist-get r3 :final) :valid)
           ;; Number of transformations in trace
           (length (plist-get r1 :trace))
           ;; Log entry from r1
           (plist-get (plist-get r1 :final) :log-entry)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((:action \"deploy\" :user \"admin\" :level critical :processed-at 1000 :priority 1 :valid t :log-entry \"[1000] deploy by admin (pri=1)\") (:action \"read\" :user \"guest\" :level info :processed-at 1000 :priority 3 :valid t :log-entry \"[1000] read by guest (pri=3)\") nil 5 \"[1000] deploy by admin (pri=1)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
