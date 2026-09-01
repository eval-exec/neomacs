//! Oracle parity tests for an algebraic effect system simulation in Elisp.
//!
//! Implements effect declaration, effect handlers (alist mapping effect names
//! to handler functions), perform effect (lookup and invoke), handler composition,
//! state effects (get/put), exception effects with resume, and logging effects
//! that accumulate messages.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Effect declaration and basic perform
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_effect_system_declare_and_perform() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Declare effects as a list of effect names. Handlers are alists
    // mapping effect names to handler functions. Perform looks up and invokes.
    let form = r#"(progn
  (defvar neovm--eff-handlers nil)

  ;; Declare effects: just a list of valid effect names
  (defvar neovm--eff-declared '(read-line print-line get-time random-int))

  (fset 'neovm--eff-install-handler
    (lambda (effect-name handler-fn)
      "Install a handler for EFFECT-NAME."
      (if (memq effect-name neovm--eff-declared)
          (progn
            (setq neovm--eff-handlers
                  (cons (cons effect-name handler-fn)
                        (assq-delete-all effect-name neovm--eff-handlers)))
            t)
        (list 'error (format "undeclared effect: %s" effect-name)))))

  (fset 'neovm--eff-perform
    (lambda (effect-name &rest args)
      "Perform EFFECT-NAME with ARGS by invoking the installed handler."
      (let ((entry (assq effect-name neovm--eff-handlers)))
        (if entry
            (apply (cdr entry) args)
          (if (memq effect-name neovm--eff-declared)
              (list 'unhandled effect-name)
            (list 'error (format "unknown effect: %s" effect-name)))))))

  (unwind-protect
      (progn
        (setq neovm--eff-handlers nil)

        ;; Install handlers
        (funcall 'neovm--eff-install-handler 'read-line
                 (lambda () "mocked-input"))
        (funcall 'neovm--eff-install-handler 'print-line
                 (lambda (msg) (format "[OUT] %s" msg)))
        (funcall 'neovm--eff-install-handler 'get-time
                 (lambda () 1234567890))
        (funcall 'neovm--eff-install-handler 'random-int
                 (lambda (lo hi) (+ lo (% 42 (1+ (- hi lo))))))

        (list
          ;; Perform handled effects
          (funcall 'neovm--eff-perform 'read-line)
          (funcall 'neovm--eff-perform 'print-line "hello world")
          (funcall 'neovm--eff-perform 'get-time)
          (funcall 'neovm--eff-perform 'random-int 1 100)
          ;; Perform undeclared effect
          (funcall 'neovm--eff-perform 'fly)
          ;; Install handler for undeclared effect fails
          (funcall 'neovm--eff-install-handler 'fly (lambda () "soar"))
          ;; Remove handler and try performing
          (progn
            (setq neovm--eff-handlers
                  (assq-delete-all 'read-line neovm--eff-handlers))
            (funcall 'neovm--eff-perform 'read-line))
          ;; Reinstall with different handler
          (progn
            (funcall 'neovm--eff-install-handler 'read-line
                     (lambda () "new-mocked-input"))
            (funcall 'neovm--eff-perform 'read-line))))
    (fmakunbound 'neovm--eff-install-handler)
    (fmakunbound 'neovm--eff-perform)
    (makunbound 'neovm--eff-handlers)
    (makunbound 'neovm--eff-declared)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"mocked-input\" \"[OUT] hello world\" 1234567890 43 (error \"unknown effect: fly\") (error \"undeclared effect: fly\") (unhandled read-line) \"new-mocked-input\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Handler composition: combine multiple handler sets
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_effect_system_handler_composition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple handler sets can be composed. A composed handler checks
    // each set in order, using the first match found.
    let form = r#"(progn
  (fset 'neovm--eff2-make-handler-set
    (lambda (&rest pairs)
      "Create a handler set from alternating effect-name handler-fn pairs."
      (let ((result nil))
        (while pairs
          (setq result (cons (cons (car pairs) (cadr pairs)) result))
          (setq pairs (cddr pairs)))
        (nreverse result))))

  (fset 'neovm--eff2-compose
    (lambda (&rest handler-sets)
      "Compose multiple handler sets. Earlier sets take priority."
      (let ((combined nil))
        (dolist (hs (reverse handler-sets))
          (dolist (entry hs)
            (unless (assq (car entry) combined)
              (setq combined (cons entry combined)))))
        combined)))

  (fset 'neovm--eff2-perform
    (lambda (handlers effect-name &rest args)
      (let ((entry (assq effect-name handlers)))
        (if entry
            (apply (cdr entry) args)
          (list 'unhandled effect-name args)))))

  (unwind-protect
      (let ((io-handlers
             (funcall 'neovm--eff2-make-handler-set
                      'read (lambda () "io-read")
                      'write (lambda (s) (format "io-write: %s" s))
                      'flush (lambda () "io-flushed")))
            (state-handlers
             (funcall 'neovm--eff2-make-handler-set
                      'get-state (lambda (key) (format "state[%s]" key))
                      'set-state (lambda (key val) (format "set %s=%s" key val))))
            (override-handlers
             (funcall 'neovm--eff2-make-handler-set
                      'read (lambda () "override-read")
                      'log (lambda (msg) (format "LOG: %s" msg)))))

        ;; Compose: override > state > io
        (let ((composed (funcall 'neovm--eff2-compose
                                 override-handlers state-handlers io-handlers)))
          (list
            ;; 'read from override (takes priority over io)
            (funcall 'neovm--eff2-perform composed 'read)
            ;; 'write from io (not in override or state)
            (funcall 'neovm--eff2-perform composed 'write "test")
            ;; 'get-state from state
            (funcall 'neovm--eff2-perform composed 'get-state 'counter)
            ;; 'set-state from state
            (funcall 'neovm--eff2-perform composed 'set-state 'counter 42)
            ;; 'log from override
            (funcall 'neovm--eff2-perform composed 'log "event happened")
            ;; 'flush from io
            (funcall 'neovm--eff2-perform composed 'flush)
            ;; unhandled effect
            (funcall 'neovm--eff2-perform composed 'unknown-eff 1 2 3)
            ;; Number of handlers in composed set
            (length composed)
            ;; Compose with empty set doesn't change anything
            (let ((c2 (funcall 'neovm--eff2-compose nil composed)))
              (= (length c2) (length composed))))))
    (fmakunbound 'neovm--eff2-make-handler-set)
    (fmakunbound 'neovm--eff2-compose)
    (fmakunbound 'neovm--eff2-perform)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"io-read\" \"io-write: test\" \"state[counter]\" \"set counter=42\" \"LOG: event happened\" \"io-flushed\" (unhandled unknown-eff (1 2 3)) 6 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// State effect: get/put operations via handlers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_effect_system_state_effect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a stateful effect handler that maintains a mutable state
    // (implemented as an alist). Programs can get and put state values.
    let form = r#"(progn
  (defvar neovm--eff3-state nil)

  (fset 'neovm--eff3-make-state-handler
    (lambda ()
      "Return a handler set for state effects backed by neovm--eff3-state."
      (list
        (cons 'state-get
              (lambda (key)
                (let ((entry (assq key neovm--eff3-state)))
                  (if entry (cdr entry) nil))))
        (cons 'state-put
              (lambda (key value)
                (let ((entry (assq key neovm--eff3-state)))
                  (if entry
                      (setcdr entry value)
                    (setq neovm--eff3-state
                          (cons (cons key value) neovm--eff3-state))))
                value))
        (cons 'state-delete
              (lambda (key)
                (setq neovm--eff3-state
                      (assq-delete-all key neovm--eff3-state))
                t))
        (cons 'state-keys
              (lambda ()
                (mapcar 'car neovm--eff3-state)))
        (cons 'state-snapshot
              (lambda ()
                (copy-sequence neovm--eff3-state))))))

  (fset 'neovm--eff3-perform
    (lambda (handlers effect &rest args)
      (let ((entry (assq effect handlers)))
        (if entry (apply (cdr entry) args)
          (list 'unhandled effect)))))

  (unwind-protect
      (progn
        (setq neovm--eff3-state nil)
        (let ((h (funcall 'neovm--eff3-make-state-handler)))
          ;; Put some values
          (funcall 'neovm--eff3-perform h 'state-put 'x 10)
          (funcall 'neovm--eff3-perform h 'state-put 'y 20)
          (funcall 'neovm--eff3-perform h 'state-put 'name "Alice")

          (let ((snap1 (funcall 'neovm--eff3-perform h 'state-snapshot))
                (val-x (funcall 'neovm--eff3-perform h 'state-get 'x))
                (val-y (funcall 'neovm--eff3-perform h 'state-get 'y))
                (val-name (funcall 'neovm--eff3-perform h 'state-get 'name))
                (val-missing (funcall 'neovm--eff3-perform h 'state-get 'zzz))
                (keys1 (funcall 'neovm--eff3-perform h 'state-keys)))

            ;; Overwrite x, delete y, add z
            (funcall 'neovm--eff3-perform h 'state-put 'x 999)
            (funcall 'neovm--eff3-perform h 'state-delete 'y)
            (funcall 'neovm--eff3-perform h 'state-put 'z 30)

            (let ((val-x2 (funcall 'neovm--eff3-perform h 'state-get 'x))
                  (val-y2 (funcall 'neovm--eff3-perform h 'state-get 'y))
                  (val-z (funcall 'neovm--eff3-perform h 'state-get 'z))
                  (keys2 (funcall 'neovm--eff3-perform h 'state-keys))
                  (snap2 (funcall 'neovm--eff3-perform h 'state-snapshot)))

              ;; Run a "program" using state effects: counter increment loop
              (funcall 'neovm--eff3-perform h 'state-put 'counter 0)
              (dotimes (i 5)
                (let ((cur (funcall 'neovm--eff3-perform h 'state-get 'counter)))
                  (funcall 'neovm--eff3-perform h 'state-put 'counter (1+ cur))))

              (list val-x val-y val-name val-missing
                    (length keys1)
                    val-x2 val-y2 val-z
                    (length keys2)
                    (funcall 'neovm--eff3-perform h 'state-get 'counter))))))
    (fmakunbound 'neovm--eff3-make-state-handler)
    (fmakunbound 'neovm--eff3-perform)
    (makunbound 'neovm--eff3-state)))"#;
    let expect = expect_test::expect![[r#""OK (10 20 \"Alice\" nil 3 999 nil 30 3 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Exception effect with resume capability
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_effect_system_exception_with_resume() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement an exception effect where handlers can choose to
    // resume with a replacement value or propagate the exception.
    let form = r#"(progn
  (defvar neovm--eff4-exception-handlers nil)
  (defvar neovm--eff4-exception-log nil)

  (fset 'neovm--eff4-install-exception-handler
    (lambda (exception-type handler-fn)
      "Install handler for EXCEPTION-TYPE. Handler receives (type data resume-fn).
       Call resume-fn with a value to resume, or return (propagate . data) to propagate."
      (setq neovm--eff4-exception-handlers
            (cons (cons exception-type handler-fn)
                  (assq-delete-all exception-type neovm--eff4-exception-handlers)))))

  (fset 'neovm--eff4-raise
    (lambda (exception-type data)
      "Raise an exception. Returns resumed value or propagated error."
      (setq neovm--eff4-exception-log
            (cons (list 'raised exception-type data) neovm--eff4-exception-log))
      (let ((entry (assq exception-type neovm--eff4-exception-handlers)))
        (if entry
            (let ((resumed nil)
                  (resume-fn (lambda (val)
                               (setq resumed (cons 'resumed val))
                               val)))
              (let ((result (funcall (cdr entry) exception-type data resume-fn)))
                (if resumed
                    (progn
                      (setq neovm--eff4-exception-log
                            (cons (list 'resumed exception-type (cdr resumed))
                                  neovm--eff4-exception-log))
                      (cdr resumed))
                  (progn
                    (setq neovm--eff4-exception-log
                          (cons (list 'propagated exception-type result)
                                neovm--eff4-exception-log))
                    (list 'propagated result)))))
          (progn
            (setq neovm--eff4-exception-log
                  (cons (list 'unhandled exception-type data)
                        neovm--eff4-exception-log))
            (list 'unhandled exception-type data))))))

  (unwind-protect
      (progn
        (setq neovm--eff4-exception-handlers nil)
        (setq neovm--eff4-exception-log nil)

        ;; Install handlers:
        ;; division-by-zero: resume with infinity
        (funcall 'neovm--eff4-install-exception-handler 'division-by-zero
                 (lambda (type data resume-fn)
                   (funcall resume-fn 'infinity)))

        ;; out-of-bounds: resume with clamped value
        (funcall 'neovm--eff4-install-exception-handler 'out-of-bounds
                 (lambda (type data resume-fn)
                   (let ((idx (plist-get data :index))
                         (max-idx (plist-get data :max)))
                     (funcall resume-fn (min idx max-idx)))))

        ;; type-error: propagate (don't resume)
        (funcall 'neovm--eff4-install-exception-handler 'type-error
                 (lambda (type data resume-fn)
                   (list 'type-error-propagated data)))

        ;; Safe divide using exception effect
        (let ((safe-div
               (lambda (a b)
                 (if (= b 0)
                     (funcall 'neovm--eff4-raise 'division-by-zero
                              (list :numerator a :denominator b))
                   (/ a b))))
              ;; Safe array access
              (safe-nth
               (lambda (idx lst)
                 (if (or (< idx 0) (>= idx (length lst)))
                     (funcall 'neovm--eff4-raise 'out-of-bounds
                              (list :index idx :max (1- (length lst))))
                   (nth idx lst)))))

          (list
            ;; Normal division
            (funcall safe-div 10 3)
            ;; Division by zero: resumes with 'infinity
            (funcall safe-div 10 0)
            ;; Normal access
            (funcall safe-nth 1 '(a b c d))
            ;; Out of bounds: resumes with clamped index
            (funcall safe-nth 10 '(a b c d))
            ;; Type error: propagates
            (funcall 'neovm--eff4-raise 'type-error '(:expected number :got "string"))
            ;; Unhandled exception
            (funcall 'neovm--eff4-raise 'network-error '(:code 404))
            ;; Log should capture all events
            (length neovm--eff4-exception-log))))
    (fmakunbound 'neovm--eff4-install-exception-handler)
    (fmakunbound 'neovm--eff4-raise)
    (makunbound 'neovm--eff4-exception-handlers)
    (makunbound 'neovm--eff4-exception-log)))"#;
    let expect = expect_test::expect![[
        r#""OK (3 (propagated infinity) b (propagated 3) (propagated (type-error-propagated (:expected number :got \"string\"))) (unhandled network-error (:code 404)) 8)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Logging effect that accumulates messages
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_effect_system_logging_effect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a logging effect with multiple log levels (debug, info, warn, error),
    // filtering by minimum level, and structured log accumulation.
    let form = r#"(progn
  (defvar neovm--eff5-log-entries nil)
  (defvar neovm--eff5-min-level nil)

  (fset 'neovm--eff5-level-value
    (lambda (level)
      (cond ((eq level 'debug) 0)
            ((eq level 'info) 1)
            ((eq level 'warn) 2)
            ((eq level 'error) 3)
            (t -1))))

  (fset 'neovm--eff5-make-log-handler
    (lambda (min-level)
      "Create log handler set with minimum level filter."
      (setq neovm--eff5-min-level min-level)
      (list
        (cons 'log
              (lambda (level msg &rest context)
                (when (>= (funcall 'neovm--eff5-level-value level)
                          (funcall 'neovm--eff5-level-value min-level))
                  (let ((entry (list :level level
                                     :msg msg
                                     :context context
                                     :seq (length neovm--eff5-log-entries))))
                    (setq neovm--eff5-log-entries
                          (append neovm--eff5-log-entries (list entry)))
                    entry))))
        (cons 'get-logs
              (lambda (&optional filter-level)
                (if filter-level
                    (let ((result nil))
                      (dolist (e neovm--eff5-log-entries)
                        (when (eq (plist-get e :level) filter-level)
                          (setq result (cons e result))))
                      (nreverse result))
                  neovm--eff5-log-entries)))
        (cons 'clear-logs
              (lambda ()
                (setq neovm--eff5-log-entries nil)
                t))
        (cons 'log-count
              (lambda ()
                (length neovm--eff5-log-entries)))
        (cons 'set-level
              (lambda (new-level)
                (setq neovm--eff5-min-level new-level)
                new-level)))))

  (fset 'neovm--eff5-perform
    (lambda (handlers effect &rest args)
      (let ((entry (assq effect handlers)))
        (if entry (apply (cdr entry) args)
          (list 'unhandled effect)))))

  (unwind-protect
      (progn
        (setq neovm--eff5-log-entries nil)
        (setq neovm--eff5-min-level 'debug)

        (let ((h (funcall 'neovm--eff5-make-log-handler 'info)))
          ;; debug should be filtered out (min is info)
          (funcall 'neovm--eff5-perform h 'log 'debug "debug msg")
          ;; info and above should be logged
          (funcall 'neovm--eff5-perform h 'log 'info "starting up" :module 'main)
          (funcall 'neovm--eff5-perform h 'log 'info "connected" :host "localhost")
          (funcall 'neovm--eff5-perform h 'log 'warn "high latency" :ms 500)
          (funcall 'neovm--eff5-perform h 'log 'error "timeout" :code 504)
          (funcall 'neovm--eff5-perform h 'log 'info "retrying")

          (let ((total-count (funcall 'neovm--eff5-perform h 'log-count))
                (all-logs (funcall 'neovm--eff5-perform h 'get-logs))
                (warn-logs (funcall 'neovm--eff5-perform h 'get-logs 'warn))
                (error-logs (funcall 'neovm--eff5-perform h 'get-logs 'error)))

            ;; Change level to warn: only warn/error should pass now
            (funcall 'neovm--eff5-perform h 'set-level 'warn)
            (funcall 'neovm--eff5-perform h 'log 'info "this should be filtered")
            (funcall 'neovm--eff5-perform h 'log 'warn "another warning")
            (funcall 'neovm--eff5-perform h 'log 'error "critical failure")

            (let ((count-after (funcall 'neovm--eff5-perform h 'log-count)))
              ;; Clear and verify
              (funcall 'neovm--eff5-perform h 'clear-logs)
              (let ((count-cleared (funcall 'neovm--eff5-perform h 'log-count)))
                (list
                  total-count         ;; 5 (info, info, warn, error, info)
                  (length warn-logs)  ;; 1
                  (length error-logs) ;; 1
                  count-after         ;; 7 (5 + warn + error)
                  count-cleared       ;; 0
                  ;; Verify log structure
                  (plist-get (car all-logs) :msg)
                  (plist-get (car all-logs) :level)
                  (plist-get (car (last all-logs)) :msg)))))))
    (fmakunbound 'neovm--eff5-level-value)
    (fmakunbound 'neovm--eff5-make-log-handler)
    (fmakunbound 'neovm--eff5-perform)
    (makunbound 'neovm--eff5-log-entries)
    (makunbound 'neovm--eff5-min-level)))"#;
    let expect = expect_test::expect![[r#""OK (5 1 1 8 0 \"starting up\" info \"retrying\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: running a program with composed effect handlers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_effect_system_composed_program() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compose state, logging, and exception effects to run a program
    // that simulates a simple bank account transaction system.
    let form = r#"(progn
  (defvar neovm--eff6-state nil)
  (defvar neovm--eff6-logs nil)
  (defvar neovm--eff6-errors nil)

  (fset 'neovm--eff6-make-handlers
    (lambda ()
      "Build combined handlers for state + log + exception."
      (list
        ;; State effects
        (cons 'get (lambda (key)
                     (let ((e (assq key neovm--eff6-state)))
                       (if e (cdr e) 0))))
        (cons 'put (lambda (key val)
                     (let ((e (assq key neovm--eff6-state)))
                       (if e (setcdr e val)
                         (setq neovm--eff6-state (cons (cons key val) neovm--eff6-state))))
                     val))
        ;; Log effect
        (cons 'log (lambda (msg)
                     (setq neovm--eff6-logs (append neovm--eff6-logs (list msg)))
                     nil))
        ;; Exception effect
        (cons 'raise (lambda (err-type data)
                       (setq neovm--eff6-errors
                             (append neovm--eff6-errors (list (list err-type data))))
                       (list 'error err-type data))))))

  (fset 'neovm--eff6-do
    (lambda (h eff &rest args)
      (let ((entry (assq eff h)))
        (if entry (apply (cdr entry) args) (list 'unhandled eff)))))

  ;; Bank operations using effects
  (fset 'neovm--eff6-create-account
    (lambda (h name initial-balance)
      (funcall 'neovm--eff6-do h 'put name initial-balance)
      (funcall 'neovm--eff6-do h 'log
               (format "Created account %s with balance %d" name initial-balance))
      name))

  (fset 'neovm--eff6-transfer
    (lambda (h from to amount)
      (let ((from-bal (funcall 'neovm--eff6-do h 'get from))
            (to-bal (funcall 'neovm--eff6-do h 'get to)))
        (cond
          ((< amount 0)
           (funcall 'neovm--eff6-do h 'raise 'invalid-amount amount))
          ((< from-bal amount)
           (funcall 'neovm--eff6-do h 'log
                    (format "FAILED: %s->%s $%d (insufficient funds)" from to amount))
           (funcall 'neovm--eff6-do h 'raise 'insufficient-funds
                    (list :from from :balance from-bal :requested amount)))
          (t
           (funcall 'neovm--eff6-do h 'put from (- from-bal amount))
           (funcall 'neovm--eff6-do h 'put to (+ to-bal amount))
           (funcall 'neovm--eff6-do h 'log
                    (format "Transfer %s->%s: $%d" from to amount))
           (list 'ok amount))))))

  (fset 'neovm--eff6-get-balance
    (lambda (h name)
      (funcall 'neovm--eff6-do h 'get name)))

  (unwind-protect
      (progn
        (setq neovm--eff6-state nil)
        (setq neovm--eff6-logs nil)
        (setq neovm--eff6-errors nil)

        (let ((h (funcall 'neovm--eff6-make-handlers)))
          ;; Create accounts
          (funcall 'neovm--eff6-create-account h 'alice 1000)
          (funcall 'neovm--eff6-create-account h 'bob 500)
          (funcall 'neovm--eff6-create-account h 'charlie 200)

          ;; Successful transfers
          (let ((t1 (funcall 'neovm--eff6-transfer h 'alice 'bob 300))
                (t2 (funcall 'neovm--eff6-transfer h 'bob 'charlie 150)))
            ;; Failed transfer: insufficient funds
            (let ((t3 (funcall 'neovm--eff6-transfer h 'charlie 'alice 5000)))
              ;; Failed: negative amount
              (let ((t4 (funcall 'neovm--eff6-transfer h 'alice 'bob -50)))
                (list
                  t1 t2 t3 t4
                  ;; Final balances
                  (funcall 'neovm--eff6-get-balance h 'alice)
                  (funcall 'neovm--eff6-get-balance h 'bob)
                  (funcall 'neovm--eff6-get-balance h 'charlie)
                  ;; Log count
                  (length neovm--eff6-logs)
                  ;; Error count
                  (length neovm--eff6-errors)
                  ;; Last few logs
                  neovm--eff6-logs))))))
    (fmakunbound 'neovm--eff6-make-handlers)
    (fmakunbound 'neovm--eff6-do)
    (fmakunbound 'neovm--eff6-create-account)
    (fmakunbound 'neovm--eff6-transfer)
    (fmakunbound 'neovm--eff6-get-balance)
    (makunbound 'neovm--eff6-state)
    (makunbound 'neovm--eff6-logs)
    (makunbound 'neovm--eff6-errors)))"#;
    let expect = expect_test::expect![[
        r#""OK ((ok 300) (ok 150) (error insufficient-funds (:from charlie :balance 350 :requested 5000)) (error invalid-amount -50) 700 650 350 6 2 (\"Created account alice with balance 1000\" \"Created account bob with balance 500\" \"Created account charlie with balance 200\" \"Transfer alice->bob: $300\" \"Transfer bob->charlie: $150\" \"FAILED: charlie->alice $5000 (insufficient funds)\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
