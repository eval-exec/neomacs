//! Oracle parity tests for protocol state machine:
//! model a communication protocol as a finite state machine with states,
//! transitions, guards, actions, timeout simulation, error recovery,
//! and protocol trace logging.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Simple handshake protocol FSM with guard conditions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proto_fsm_handshake() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Three-way handshake protocol: CLOSED -> SYN_SENT -> ESTABLISHED -> FIN_WAIT -> CLOSED
    // With guard conditions on transitions
    let form = r#"(progn
  (defvar neovm--pf-transitions nil)
  (defvar neovm--pf-trace nil)

  ;; Transition table: list of (from-state event guard-fn action-fn to-state)
  ;; guard-fn and action-fn receive the context hash-table
  (setq neovm--pf-transitions
    (list
     ;; CLOSED -> SYN_SENT on 'connect (no guard)
     (list 'closed 'connect nil
           (lambda (ctx) (puthash 'syn-seq (1+ (gethash 'seq ctx 0)) ctx)
                         (puthash 'seq (1+ (gethash 'seq ctx 0)) ctx))
           'syn-sent)
     ;; SYN_SENT -> ESTABLISHED on 'syn-ack (guard: ack matches our seq)
     (list 'syn-sent 'syn-ack
           (lambda (ctx) (= (gethash 'ack-num ctx 0) (gethash 'syn-seq ctx 0)))
           (lambda (ctx) (puthash 'established-at (gethash 'tick ctx 0) ctx))
           'established)
     ;; SYN_SENT -> CLOSED on 'timeout
     (list 'syn-sent 'timeout nil
           (lambda (ctx) (puthash 'error "connection timeout" ctx))
           'closed)
     ;; SYN_SENT -> CLOSED on 'rst
     (list 'syn-sent 'rst nil
           (lambda (ctx) (puthash 'error "connection refused" ctx))
           'closed)
     ;; ESTABLISHED -> ESTABLISHED on 'data (guard: data-len > 0)
     (list 'established 'data
           (lambda (ctx) (> (gethash 'data-len ctx 0) 0))
           (lambda (ctx)
             (puthash 'bytes-sent
                      (+ (gethash 'bytes-sent ctx 0) (gethash 'data-len ctx 0))
                      ctx)
             (puthash 'packets (1+ (gethash 'packets ctx 0)) ctx))
           'established)
     ;; ESTABLISHED -> FIN_WAIT on 'close
     (list 'established 'close nil
           (lambda (ctx) (puthash 'close-at (gethash 'tick ctx 0) ctx))
           'fin-wait)
     ;; FIN_WAIT -> CLOSED on 'fin-ack
     (list 'fin-wait 'fin-ack nil
           (lambda (ctx) (puthash 'closed-at (gethash 'tick ctx 0) ctx))
           'closed)
     ;; FIN_WAIT -> CLOSED on 'timeout
     (list 'fin-wait 'timeout nil
           (lambda (ctx) (puthash 'error "fin timeout" ctx))
           'closed)
     ;; Any state -> CLOSED on 'reset
     (list 'any 'reset nil
           (lambda (ctx) (puthash 'error "hard reset" ctx))
           'closed)))

  ;; Find first matching transition
  (fset 'neovm--pf-find-transition
    (lambda (state event ctx)
      (let ((result nil)
            (remaining neovm--pf-transitions))
        (while (and remaining (not result))
          (let ((tr (car remaining)))
            (when (and (or (eq (nth 0 tr) state) (eq (nth 0 tr) 'any))
                       (eq (nth 1 tr) event)
                       (or (null (nth 2 tr))
                           (funcall (nth 2 tr) ctx)))
              (setq result tr)))
          (setq remaining (cdr remaining)))
        result)))

  ;; Run protocol FSM with a sequence of (event . params) inputs
  (fset 'neovm--pf-run
    (lambda (events)
      (let ((state 'closed)
            (ctx (make-hash-table))
            (tick 0))
        (setq neovm--pf-trace nil)
        (puthash 'seq 100 ctx)
        (puthash 'bytes-sent 0 ctx)
        (puthash 'packets 0 ctx)
        (dolist (ev events)
          (setq tick (1+ tick))
          (puthash 'tick tick ctx)
          ;; Set event-specific context
          (when (consp ev)
            (dolist (param (cdr ev))
              (puthash (car param) (cdr param) ctx)))
          (let* ((event-name (if (consp ev) (car ev) ev))
                 (tr (funcall 'neovm--pf-find-transition state event-name ctx)))
            (if tr
                (progn
                  (when (nth 3 tr) (funcall (nth 3 tr) ctx))
                  (setq neovm--pf-trace
                        (cons (list tick state event-name (nth 4 tr)) neovm--pf-trace))
                  (setq state (nth 4 tr)))
              (setq neovm--pf-trace
                    (cons (list tick state event-name 'invalid) neovm--pf-trace)))))
        (list 'final-state state
              'bytes-sent (gethash 'bytes-sent ctx 0)
              'packets (gethash 'packets ctx 0)
              'error (gethash 'error ctx)
              'trace (nreverse neovm--pf-trace)))))

  (unwind-protect
      (list
       ;; Happy path: connect, syn-ack, data, data, close, fin-ack
       (funcall 'neovm--pf-run
                (list 'connect
                      '(syn-ack (ack-num . 101))
                      '(data (data-len . 1024))
                      '(data (data-len . 512))
                      'close
                      'fin-ack))
       ;; Connection refused
       (funcall 'neovm--pf-run
                (list 'connect 'rst))
       ;; Connection timeout
       (funcall 'neovm--pf-run
                (list 'connect 'timeout))
       ;; Wrong ack number (guard fails, stays in syn-sent)
       (funcall 'neovm--pf-run
                (list 'connect
                      '(syn-ack (ack-num . 999))
                      'timeout))
       ;; Hard reset during data transfer
       (funcall 'neovm--pf-run
                (list 'connect
                      '(syn-ack (ack-num . 101))
                      '(data (data-len . 100))
                      'reset)))
    (fmakunbound 'neovm--pf-find-transition)
    (fmakunbound 'neovm--pf-run)
    (makunbound 'neovm--pf-transitions)
    (makunbound 'neovm--pf-trace)))"#;
    let expect = expect_test::expect![[
        r#""OK ((final-state closed bytes-sent 1536 packets 2 error nil trace ((1 closed connect syn-sent) (2 syn-sent syn-ack established) (3 established data established) (4 established data established) (5 established close fin-wait) (6 fin-wait fin-ack closed))) (final-state closed bytes-sent 0 packets 0 error \"connection refused\" trace ((1 closed connect syn-sent) (2 syn-sent rst closed))) (final-state closed bytes-sent 0 packets 0 error \"connection timeout\" trace ((1 closed connect syn-sent) (2 syn-sent timeout closed))) (final-state closed bytes-sent 0 packets 0 error \"connection timeout\" trace ((1 closed connect syn-sent) (2 syn-sent syn-ack invalid) (3 syn-sent timeout closed))) (final-state closed bytes-sent 100 packets 1 error \"hard reset\" trace ((1 closed connect syn-sent) (2 syn-sent syn-ack established) (3 established data established) (4 established reset closed))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// HTTP request/response protocol FSM
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proto_fsm_http_request() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Model HTTP request processing as a state machine
    // States: idle, reading-headers, reading-body, processing, responding, done
    let form = r#"(progn
  (fset 'neovm--pf-http-process
    (lambda (inputs)
      (let ((state 'idle)
            (headers (make-hash-table :test 'equal))
            (body "")
            (method nil)
            (path nil)
            (response-code nil)
            (response-body nil)
            (log nil))
        (dolist (input inputs)
          (let ((kind (car input))
                (data (cdr input)))
            (cond
             ;; IDLE: expect request-line
             ((and (eq state 'idle) (eq kind 'request-line))
              (setq method (nth 0 data)
                    path (nth 1 data))
              (setq log (cons (list 'transition 'idle 'reading-headers) log))
              (setq state 'reading-headers))

             ;; READING-HEADERS: accept header or end-of-headers
             ((and (eq state 'reading-headers) (eq kind 'header))
              (puthash (car data) (cadr data) headers)
              (setq log (cons (list 'header (car data) (cadr data)) log)))

             ((and (eq state 'reading-headers) (eq kind 'end-headers))
              (let ((content-length (gethash "Content-Length" headers nil)))
                (if (and content-length (> (string-to-number content-length) 0))
                    (progn
                      (setq log (cons (list 'transition 'reading-headers 'reading-body) log))
                      (setq state 'reading-body))
                  (setq log (cons (list 'transition 'reading-headers 'processing) log))
                  (setq state 'processing))))

             ;; READING-BODY: accumulate body data
             ((and (eq state 'reading-body) (eq kind 'body-chunk))
              (setq body (concat body (car data)))
              (setq log (cons (list 'body-chunk (length (car data))) log))
              (let ((expected (string-to-number (gethash "Content-Length" headers "0"))))
                (when (>= (length body) expected)
                  (setq log (cons (list 'transition 'reading-body 'processing) log))
                  (setq state 'processing))))

             ;; PROCESSING: generate response
             ((eq state 'processing)
              ;; Route based on method and path
              (cond
               ((and (string= method "GET") (string= path "/"))
                (setq response-code 200
                      response-body "Welcome"))
               ((and (string= method "GET") (string= path "/health"))
                (setq response-code 200
                      response-body "OK"))
               ((and (string= method "POST") (string= path "/data"))
                (setq response-code 201
                      response-body (concat "Received: " body)))
               ((string= method "DELETE")
                (setq response-code 204
                      response-body ""))
               (t
                (setq response-code 404
                      response-body "Not Found")))
              (setq log (cons (list 'transition 'processing 'responding response-code) log))
              (setq state 'responding))

             ;; RESPONDING -> DONE
             ((and (eq state 'responding) (eq kind 'send-response))
              (setq log (cons (list 'transition 'responding 'done) log))
              (setq state 'done))

             ;; Invalid transition
             (t
              (setq log (cons (list 'invalid-transition state kind) log))))))

        ;; Auto-process if in processing state
        (when (eq state 'processing)
          (cond
           ((and (string= method "GET") (string= path "/"))
            (setq response-code 200 response-body "Welcome"))
           ((and (string= method "POST") (string= path "/data"))
            (setq response-code 201 response-body (concat "Received: " body)))
           (t (setq response-code 404 response-body "Not Found")))
          (setq state 'responding)
          (setq log (cons (list 'auto-transition 'processing 'responding) log)))

        (list 'state state
              'method method
              'path path
              'response-code response-code
              'response-body response-body
              'body-length (length body)
              'header-count (hash-table-count headers)
              'log (nreverse log)))))

  (unwind-protect
      (list
       ;; Simple GET request
       (funcall 'neovm--pf-http-process
                '((request-line "GET" "/")
                  (header "Host" "example.com")
                  (header "Accept" "text/html")
                  (end-headers)
                  (send-response)))

       ;; POST with body
       (funcall 'neovm--pf-http-process
                '((request-line "POST" "/data")
                  (header "Content-Length" "11")
                  (header "Content-Type" "text/plain")
                  (end-headers)
                  (body-chunk "hello world")
                  (send-response)))

       ;; GET health check
       (funcall 'neovm--pf-http-process
                '((request-line "GET" "/health")
                  (end-headers)
                  (send-response)))

       ;; 404 Not Found
       (funcall 'neovm--pf-http-process
                '((request-line "GET" "/nonexistent")
                  (end-headers)
                  (send-response)))

       ;; DELETE request
       (funcall 'neovm--pf-http-process
                '((request-line "DELETE" "/items/42")
                  (end-headers)
                  (send-response))))
    (fmakunbound 'neovm--pf-http-process)))"#;
    let expect = expect_test::expect![[
        r#""OK ((state responding method \"GET\" path \"/\" response-code 200 response-body \"Welcome\" body-length 0 header-count 2 log ((transition idle reading-headers) (header \"Host\" \"example.com\") (header \"Accept\" \"text/html\") (transition reading-headers processing) (transition processing responding 200))) (state responding method \"POST\" path \"/data\" response-code 201 response-body \"Received: hello world\" body-length 11 header-count 2 log ((transition idle reading-headers) (header \"Content-Length\" \"11\") (header \"Content-Type\" \"text/plain\") (transition reading-headers reading-body) (body-chunk 11) (transition reading-body processing) (transition processing responding 201))) (state responding method \"GET\" path \"/health\" response-code 200 response-body \"OK\" body-length 0 header-count 0 log ((transition idle reading-headers) (transition reading-headers processing) (transition processing responding 200))) (state responding method \"GET\" path \"/nonexistent\" response-code 404 response-body \"Not Found\" body-length 0 header-count 0 log ((transition idle reading-headers) (transition reading-headers processing) (transition processing responding 404))) (state responding method \"DELETE\" path \"/items/42\" response-code 204 response-body \"\" body-length 0 header-count 0 log ((transition idle reading-headers) (transition reading-headers processing) (transition processing responding 204))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Protocol with timeout simulation and retry logic
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proto_fsm_timeout_retry() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Protocol that retries on timeout with exponential backoff
    let form = r#"(progn
  (fset 'neovm--pf-retry-protocol
    (lambda (events max-retries)
      (let ((state 'idle)
            (retries 0)
            (backoff 1)
            (total-wait 0)
            (log nil)
            (data-sent nil)
            (tick 0))
        (dolist (ev events)
          (setq tick (1+ tick))
          (cond
           ;; IDLE -> CONNECTING on 'connect
           ((and (eq state 'idle) (eq ev 'connect))
            (setq log (cons (list tick 'connect 'idle 'connecting) log))
            (setq state 'connecting)
            (setq retries 0 backoff 1))

           ;; CONNECTING -> CONNECTED on 'connected
           ((and (eq state 'connecting) (eq ev 'connected))
            (setq log (cons (list tick 'connected retries) log))
            (setq state 'connected))

           ;; CONNECTING -> retry or fail on 'timeout
           ((and (eq state 'connecting) (eq ev 'timeout))
            (if (< retries max-retries)
                (progn
                  (setq retries (1+ retries))
                  (setq total-wait (+ total-wait backoff))
                  (setq log (cons (list tick 'timeout 'retry retries 'backoff backoff) log))
                  (setq backoff (* backoff 2)))  ;; exponential backoff
              (setq log (cons (list tick 'timeout 'max-retries-exceeded) log))
              (setq state 'failed)))

           ;; CONNECTED -> SENDING on 'send
           ((and (eq state 'connected) (consp ev) (eq (car ev) 'send))
            (setq data-sent (cons (cdr ev) data-sent))
            (setq log (cons (list tick 'send (cdr ev)) log))
            (setq state 'sending))

           ;; SENDING -> CONNECTED on 'ack
           ((and (eq state 'sending) (eq ev 'ack))
            (setq log (cons (list tick 'ack) log))
            (setq state 'connected))

           ;; SENDING -> retry on 'timeout
           ((and (eq state 'sending) (eq ev 'timeout))
            (if (< retries max-retries)
                (progn
                  (setq retries (1+ retries))
                  (setq log (cons (list tick 'send-timeout 'retry retries) log)))
              (setq log (cons (list tick 'send-timeout 'failed) log))
              (setq state 'failed)))

           ;; CONNECTED -> IDLE on 'disconnect
           ((and (eq state 'connected) (eq ev 'disconnect))
            (setq log (cons (list tick 'disconnect) log))
            (setq state 'idle))

           ;; Invalid
           (t (setq log (cons (list tick 'invalid state ev) log)))))

        (list 'state state
              'retries retries
              'total-wait total-wait
              'data-sent (nreverse data-sent)
              'log (nreverse log)))))

  (unwind-protect
      (list
       ;; Happy path: no timeouts
       (funcall 'neovm--pf-retry-protocol
                '(connect connected (send . "hello") ack disconnect)
                3)
       ;; Two timeouts then success
       (funcall 'neovm--pf-retry-protocol
                '(connect timeout timeout connected (send . "data") ack disconnect)
                3)
       ;; Max retries exceeded
       (funcall 'neovm--pf-retry-protocol
                '(connect timeout timeout timeout timeout)
                3)
       ;; Send timeout with retry
       (funcall 'neovm--pf-retry-protocol
                '(connect connected (send . "msg1") timeout ack disconnect)
                3)
       ;; Multiple sends
       (funcall 'neovm--pf-retry-protocol
                '(connect connected
                  (send . "first") ack
                  (send . "second") ack
                  (send . "third") ack
                  disconnect)
                3))
    (fmakunbound 'neovm--pf-retry-protocol)))"#;
    let expect = expect_test::expect![[
        r#""OK ((state idle retries 0 total-wait 0 data-sent (\"hello\") log ((1 connect idle connecting) (2 connected 0) (3 send \"hello\") (4 ack) (5 disconnect))) (state idle retries 2 total-wait 3 data-sent (\"data\") log ((1 connect idle connecting) (2 timeout retry 1 backoff 1) (3 timeout retry 2 backoff 2) (4 connected 2) (5 send \"data\") (6 ack) (7 disconnect))) (state failed retries 3 total-wait 7 data-sent nil log ((1 connect idle connecting) (2 timeout retry 1 backoff 1) (3 timeout retry 2 backoff 2) (4 timeout retry 3 backoff 4) (5 timeout max-retries-exceeded))) (state idle retries 1 total-wait 0 data-sent (\"msg1\") log ((1 connect idle connecting) (2 connected 0) (3 send \"msg1\") (4 send-timeout retry 1) (5 ack) (6 disconnect))) (state idle retries 0 total-wait 0 data-sent (\"first\" \"second\" \"third\") log ((1 connect idle connecting) (2 connected 0) (3 send \"first\") (4 ack) (5 send \"second\") (6 ack) (7 send \"third\") (8 ack) (9 disconnect))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Protocol with error recovery and fallback states
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proto_fsm_error_recovery() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Protocol with error recovery: on error, transition to recovery state,
    // attempt cleanup, then return to a safe state
    let form = r#"(progn
  (fset 'neovm--pf-recovery-fsm
    (lambda (events)
      (let ((state 'init)
            (resources nil)  ;; list of acquired resources
            (errors nil)
            (log nil)
            (tick 0))
        (dolist (ev events)
          (setq tick (1+ tick))
          (condition-case err
              (cond
               ;; INIT -> ACQUIRING on 'begin
               ((and (eq state 'init) (eq ev 'begin))
                (setq log (cons (list tick 'begin) log))
                (setq state 'acquiring))

               ;; ACQUIRING: acquire resources
               ((and (eq state 'acquiring) (consp ev) (eq (car ev) 'acquire))
                (let ((resource (cdr ev)))
                  (if (member resource resources)
                      (signal 'error (list "duplicate resource" resource))
                    (setq resources (cons resource resources))
                    (setq log (cons (list tick 'acquired resource) log)))))

               ;; ACQUIRING -> PROCESSING on 'start
               ((and (eq state 'acquiring) (eq ev 'start))
                (if (null resources)
                    (signal 'error '("no resources acquired"))
                  (setq log (cons (list tick 'start-processing (length resources)) log))
                  (setq state 'processing)))

               ;; PROCESSING: do work
               ((and (eq state 'processing) (consp ev) (eq (car ev) 'process))
                (let ((item (cdr ev)))
                  (if (eq item 'bad-data)
                      (signal 'error (list "bad data encountered"))
                    (setq log (cons (list tick 'processed item) log)))))

               ;; PROCESSING -> RELEASING on 'finish
               ((and (eq state 'processing) (eq ev 'finish))
                (setq log (cons (list tick 'finishing) log))
                (setq state 'releasing))

               ;; RELEASING: release resources one by one
               ((eq state 'releasing)
                (when resources
                  (let ((r (car resources)))
                    (setq resources (cdr resources))
                    (setq log (cons (list tick 'released r) log))))
                (when (null resources)
                  (setq state 'done)
                  (setq log (cons (list tick 'done) log))))

               ;; RECOVERY: release all resources and go to init
               ((eq state 'recovery)
                (while resources
                  (setq log (cons (list tick 'recovery-release (car resources)) log))
                  (setq resources (cdr resources)))
                (setq state 'init)
                (setq log (cons (list tick 'recovered) log)))

               ;; Invalid
               (t (setq log (cons (list tick 'ignored state ev) log))))

            ;; Error handler: enter recovery state
            (error
             (setq errors (cons (list tick (cadr err)) errors))
             (setq log (cons (list tick 'error (cadr err)) log))
             (setq state 'recovery))))

        (list 'state state
              'resources resources
              'errors (nreverse errors)
              'log (nreverse log)))))

  (unwind-protect
      (list
       ;; Happy path: acquire, process, finish, release
       (funcall 'neovm--pf-recovery-fsm
                '(begin
                  (acquire . db-conn)
                  (acquire . file-handle)
                  start
                  (process . item1)
                  (process . item2)
                  finish
                  release release))

       ;; Error during processing -> recovery -> retry
       (funcall 'neovm--pf-recovery-fsm
                '(begin
                  (acquire . resource-a)
                  start
                  (process . item1)
                  (process . bad-data)
                  recover
                  begin
                  (acquire . resource-b)
                  start
                  (process . item3)
                  finish
                  release))

       ;; Duplicate resource error
       (funcall 'neovm--pf-recovery-fsm
                '(begin
                  (acquire . mutex)
                  (acquire . mutex)
                  recover))

       ;; Start without resources
       (funcall 'neovm--pf-recovery-fsm
                '(begin start recover))

       ;; Error recovery then successful completion
       (funcall 'neovm--pf-recovery-fsm
                '(begin
                  (acquire . lock)
                  start
                  (process . bad-data)
                  recover
                  begin
                  (acquire . lock)
                  start
                  (process . good-data)
                  finish
                  release)))
    (fmakunbound 'neovm--pf-recovery-fsm)))"#;
    let expect = expect_test::expect![[
        r#""OK ((state done resources nil errors nil log ((1 begin) (2 acquired db-conn) (3 acquired file-handle) (4 start-processing 2) (5 processed item1) (6 processed item2) (7 finishing) (8 released file-handle) (9 released db-conn) (9 done))) (state done resources nil errors ((5 \"bad data encountered\")) log ((1 begin) (2 acquired resource-a) (3 start-processing 1) (4 processed item1) (5 error \"bad data encountered\") (6 recovery-release resource-a) (6 recovered) (7 begin) (8 acquired resource-b) (9 start-processing 1) (10 processed item3) (11 finishing) (12 released resource-b) (12 done))) (state init resources nil errors ((3 \"duplicate resource\")) log ((1 begin) (2 acquired mutex) (3 error \"duplicate resource\") (4 recovery-release mutex) (4 recovered))) (state init resources nil errors ((2 \"no resources acquired\")) log ((1 begin) (2 error \"no resources acquired\") (3 recovered))) (state done resources nil errors ((4 \"bad data encountered\")) log ((1 begin) (2 acquired lock) (3 start-processing 1) (4 error \"bad data encountered\") (5 recovery-release lock) (5 recovered) (6 begin) (7 acquired lock) (8 start-processing 1) (9 processed good-data) (10 finishing) (11 released lock) (11 done))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Protocol trace analysis: compute metrics from trace log
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proto_fsm_trace_analysis() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Given a protocol trace (list of state transitions), compute metrics:
    // time in each state, transition counts, throughput, error rate
    let form = r#"(progn
  (fset 'neovm--pf-analyze-trace
    (lambda (trace)
      (let ((state-times (make-hash-table))
            (transition-counts (make-hash-table :test 'equal))
            (error-count 0)
            (total-transitions 0)
            (prev-state nil)
            (prev-tick 0))
        ;; Each trace entry: (tick from-state event to-state)
        (dolist (entry trace)
          (let ((tick (nth 0 entry))
                (from (nth 1 entry))
                (event (nth 2 entry))
                (to (nth 3 entry)))
            ;; Accumulate time in from-state
            (when prev-state
              (puthash prev-state
                       (+ (gethash prev-state state-times 0) (- tick prev-tick))
                       state-times))
            (setq prev-state to prev-tick tick)
            ;; Count transitions
            (let ((key (cons from to)))
              (puthash key (1+ (gethash key transition-counts 0)) transition-counts))
            (setq total-transitions (1+ total-transitions))
            ;; Count errors
            (when (eq event 'error) (setq error-count (1+ error-count)))))

        ;; Collect state times as sorted alist
        (let ((times nil))
          (maphash (lambda (k v) (setq times (cons (cons k v) times))) state-times)
          (setq times (sort times (lambda (a b)
                                    (string< (symbol-name (car a))
                                             (symbol-name (car b))))))
          ;; Collect transition counts
          (let ((tcounts nil))
            (maphash (lambda (k v) (setq tcounts (cons (list (car k) (cdr k) v) tcounts)))
                     transition-counts)
            (setq tcounts (sort tcounts
                                (lambda (a b)
                                  (or (string< (symbol-name (car a)) (symbol-name (car b)))
                                      (and (eq (car a) (car b))
                                           (string< (symbol-name (cadr a))
                                                    (symbol-name (cadr b))))))))
            (list 'total-transitions total-transitions
                  'error-count error-count
                  'error-rate (if (> total-transitions 0)
                                  (/ (* 100 error-count) total-transitions)
                                0)
                  'state-times times
                  'transition-counts tcounts))))))

  (unwind-protect
      (list
       ;; Normal session trace
       (funcall 'neovm--pf-analyze-trace
                '((1 idle connect connecting)
                  (3 connecting connected established)
                  (4 established send sending)
                  (5 sending ack established)
                  (6 established send sending)
                  (8 sending ack established)
                  (10 established close closing)
                  (12 closing fin-ack idle)))
       ;; Session with errors
       (funcall 'neovm--pf-analyze-trace
                '((1 idle connect connecting)
                  (3 connecting error recovery)
                  (4 recovery reset idle)
                  (5 idle connect connecting)
                  (7 connecting connected established)
                  (8 established send sending)
                  (9 sending error recovery)
                  (10 recovery reset idle)))
       ;; Empty trace
       (funcall 'neovm--pf-analyze-trace nil))
    (fmakunbound 'neovm--pf-analyze-trace)))"#;
    let expect = expect_test::expect![[
        r#""OK ((total-transitions 8 error-count 0 error-rate 0 state-times ((closing . 2) (connecting . 2) (established . 4) (sending . 3)) transition-counts ((closing idle 1) (connecting established 1) (established closing 1) (established sending 2) (idle connecting 1) (sending established 2))) (total-transitions 8 error-count 2 error-rate 25 state-times ((connecting . 4) (established . 1) (idle . 1) (recovery . 2) (sending . 1)) transition-counts ((connecting established 1) (connecting recovery 1) (established sending 1) (idle connecting 2) (recovery idle 2) (sending recovery 1))) (total-transitions 0 error-count 0 error-rate 0 state-times nil transition-counts nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-party protocol: client-server message exchange simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proto_fsm_client_server() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a client-server protocol where both sides have their own FSM
    // Messages are queued and processed in order
    let form = r#"(progn
  (fset 'neovm--pf-run-protocol
    (lambda (script)
      "Run a client-server protocol simulation.
       Script: list of (actor event . data) entries."
      (let ((client-state 'idle)
            (server-state 'idle)
            (client-data (make-hash-table :test 'equal))
            (server-data (make-hash-table :test 'equal))
            (log nil)
            (tick 0))
        (puthash "request-count" 0 server-data)
        (dolist (step script)
          (setq tick (1+ tick))
          (let ((actor (nth 0 step))
                (event (nth 1 step))
                (data (cddr step)))
            (cond
             ;; CLIENT FSM
             ((eq actor 'client)
              (cond
               ;; idle -> connecting
               ((and (eq client-state 'idle) (eq event 'connect))
                (setq client-state 'connecting)
                (setq log (cons (list tick 'client 'idle '-> 'connecting) log)))
               ;; connecting -> connected
               ((and (eq client-state 'connecting) (eq event 'server-hello))
                (puthash "session-id" (car data) client-data)
                (setq client-state 'connected)
                (setq log (cons (list tick 'client 'connecting '-> 'connected
                                      (car data)) log)))
               ;; connected -> waiting-response
               ((and (eq client-state 'connected) (eq event 'request))
                (puthash "last-request" (car data) client-data)
                (setq client-state 'waiting-response)
                (setq log (cons (list tick 'client 'request (car data)) log)))
               ;; waiting-response -> connected
               ((and (eq client-state 'waiting-response) (eq event 'response))
                (puthash "last-response" (car data) client-data)
                (setq client-state 'connected)
                (setq log (cons (list tick 'client 'response (car data)) log)))
               ;; connected -> idle (disconnect)
               ((and (eq client-state 'connected) (eq event 'disconnect))
                (setq client-state 'idle)
                (setq log (cons (list tick 'client 'disconnected) log)))
               (t (setq log (cons (list tick 'client 'invalid client-state event) log)))))

             ;; SERVER FSM
             ((eq actor 'server)
              (cond
               ;; idle -> listening
               ((and (eq server-state 'idle) (eq event 'listen))
                (setq server-state 'listening)
                (setq log (cons (list tick 'server 'listening) log)))
               ;; listening -> connected (accept connection)
               ((and (eq server-state 'listening) (eq event 'accept))
                (puthash "session-id" (car data) server-data)
                (setq server-state 'connected)
                (setq log (cons (list tick 'server 'accepted (car data)) log)))
               ;; connected -> processing
               ((and (eq server-state 'connected) (eq event 'receive))
                (puthash "request-count"
                         (1+ (gethash "request-count" server-data 0))
                         server-data)
                (puthash "current-request" (car data) server-data)
                (setq server-state 'processing)
                (setq log (cons (list tick 'server 'processing (car data)) log)))
               ;; processing -> connected (send response)
               ((and (eq server-state 'processing) (eq event 'respond))
                (setq server-state 'connected)
                (setq log (cons (list tick 'server 'responded (car data)) log)))
               ;; connected -> listening (client disconnected)
               ((and (eq server-state 'connected) (eq event 'client-gone))
                (setq server-state 'listening)
                (setq log (cons (list tick 'server 'client-disconnected) log)))
               (t (setq log (cons (list tick 'server 'invalid server-state event) log))))))))

        (list 'client-state client-state
              'server-state server-state
              'session-id (gethash "session-id" client-data)
              'request-count (gethash "request-count" server-data 0)
              'last-response (gethash "last-response" client-data)
              'log (nreverse log)))))

  (unwind-protect
      (list
       ;; Full session: connect, request, response, disconnect
       (funcall 'neovm--pf-run-protocol
                '((server listen)
                  (server accept "sess-001")
                  (client connect)
                  (client server-hello "sess-001")
                  (client request "GET /index")
                  (server receive "GET /index")
                  (server respond "200 OK")
                  (client response "200 OK")
                  (client request "GET /about")
                  (server receive "GET /about")
                  (server respond "200 OK")
                  (client response "200 OK")
                  (client disconnect)
                  (server client-gone)))

       ;; Minimal session
       (funcall 'neovm--pf-run-protocol
                '((server listen)
                  (server accept "s1")
                  (client connect)
                  (client server-hello "s1")
                  (client disconnect)
                  (server client-gone)))

       ;; Invalid transitions
       (funcall 'neovm--pf-run-protocol
                '((client request "data")
                  (server respond "error"))))
    (fmakunbound 'neovm--pf-run-protocol)))"#;
    let expect = expect_test::expect![[
        r#""OK ((client-state idle server-state listening session-id \"sess-001\" request-count 2 last-response \"200 OK\" log ((1 server listening) (2 server accepted \"sess-001\") (3 client idle -> connecting) (4 client connecting -> connected \"sess-001\") (5 client request \"GET /index\") (6 server processing \"GET /index\") (7 server responded \"200 OK\") (8 client response \"200 OK\") (9 client request \"GET /about\") (10 server processing \"GET /about\") (11 server responded \"200 OK\") (12 client response \"200 OK\") (13 client disconnected) (14 server client-disconnected))) (client-state idle server-state listening session-id \"s1\" request-count 0 last-response nil log ((1 server listening) (2 server accepted \"s1\") (3 client idle -> connecting) (4 client connecting -> connected \"s1\") (5 client disconnected) (6 server client-disconnected))) (client-state idle server-state idle session-id nil request-count 0 last-response nil log ((1 client invalid idle request) (2 server invalid idle respond))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Protocol sequence validation: check if trace follows legal transitions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proto_fsm_sequence_validator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Define legal protocol sequences and validate traces against them
    let form = r#"(progn
  (fset 'neovm--pf-validate-sequence
    (lambda (rules trace)
      "Validate a protocol trace against transition rules.
       RULES: hash-table of state -> list of (event . next-state).
       TRACE: list of (state event) pairs.
       Returns (valid errors visited-states)."
      (let ((state (caar trace))
            (errors nil)
            (visited (list (caar trace)))
            (valid t)
            (step 0))
        (dolist (entry trace)
          (setq step (1+ step))
          (let ((expected-state (car entry))
                (event (cadr entry)))
            ;; Check state consistency
            (unless (eq state expected-state)
              (setq valid nil)
              (setq errors (cons (list step 'state-mismatch
                                       'expected expected-state
                                       'actual state) errors)))
            ;; Find transition
            (let* ((transitions (gethash state rules))
                   (next (cdr (assq event transitions))))
              (if next
                  (progn
                    (setq state next)
                    (unless (memq next visited)
                      (setq visited (cons next visited))))
                (setq valid nil)
                (setq errors (cons (list step 'no-transition state event) errors))))))
        (list 'valid valid
              'final-state state
              'errors (nreverse errors)
              'visited (sort visited
                             (lambda (a b)
                               (string< (symbol-name a) (symbol-name b))))))))

  (unwind-protect
      (let ((rules (make-hash-table)))
        ;; Define protocol rules
        (puthash 'idle (list '(connect . connecting)) rules)
        (puthash 'connecting (list '(success . connected) '(fail . idle)) rules)
        (puthash 'connected (list '(send . sending) '(close . closing)) rules)
        (puthash 'sending (list '(ack . connected) '(error . connected)) rules)
        (puthash 'closing (list '(closed . idle)) rules)

        (list
         ;; Valid trace
         (funcall 'neovm--pf-validate-sequence rules
                  '((idle connect) (connecting success) (connected send)
                    (sending ack) (connected close) (closing closed)))
         ;; Valid trace with send error
         (funcall 'neovm--pf-validate-sequence rules
                  '((idle connect) (connecting success) (connected send)
                    (sending error) (connected send) (sending ack)
                    (connected close) (closing closed)))
         ;; Invalid: wrong event in state
         (funcall 'neovm--pf-validate-sequence rules
                  '((idle connect) (connecting success) (connected close)
                    (closing send)))
         ;; Invalid: connection failure then retry
         (funcall 'neovm--pf-validate-sequence rules
                  '((idle connect) (connecting fail) (idle connect)
                    (connecting success) (connected close) (closing closed)))
         ;; Minimal valid: connect and fail
         (funcall 'neovm--pf-validate-sequence rules
                  '((idle connect) (connecting fail)))))
    (fmakunbound 'neovm--pf-validate-sequence)))"#;
    let expect = expect_test::expect![[
        r#""OK ((valid t final-state idle errors nil visited (closing connected connecting idle sending)) (valid t final-state idle errors nil visited (closing connected connecting idle sending)) (valid nil final-state closing errors ((4 no-transition closing send)) visited (closing connected connecting idle)) (valid t final-state idle errors nil visited (closing connected connecting idle)) (valid t final-state idle errors nil visited (connecting idle)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Protocol with message serialization/deserialization
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proto_fsm_message_codec() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Encode protocol messages as strings and decode them back
    // Format: "TYPE:FIELD1=VALUE1;FIELD2=VALUE2"
    let form = r#"(progn
  ;; Encode a message alist into a protocol string
  (fset 'neovm--pf-encode
    (lambda (msg-type fields)
      (let ((parts nil))
        (dolist (f fields)
          (setq parts (cons (concat (symbol-name (car f)) "=" (cdr f)) parts)))
        (concat (symbol-name msg-type) ":"
                (mapconcat #'identity (nreverse parts) ";")))))

  ;; Decode a protocol string into (type . fields-alist)
  (fset 'neovm--pf-decode
    (lambda (str)
      (let* ((colon-pos (string-match ":" str))
             (msg-type (intern (substring str 0 colon-pos)))
             (rest (substring str (1+ colon-pos)))
             (fields nil))
        (when (> (length rest) 0)
          (let ((parts nil)
                (i 0)
                (start 0)
                (len (length rest)))
            ;; Split by semicolon
            (while (< i len)
              (when (= (aref rest i) ?\;)
                (setq parts (cons (substring rest start i) parts))
                (setq start (1+ i)))
              (setq i (1+ i)))
            (setq parts (cons (substring rest start) parts))
            (setq parts (nreverse parts))
            ;; Parse each part as key=value
            (dolist (part parts)
              (let ((eq-pos (string-match "=" part)))
                (when eq-pos
                  (setq fields
                        (cons (cons (intern (substring part 0 eq-pos))
                                    (substring part (1+ eq-pos)))
                              fields)))))))
        (cons msg-type (nreverse fields)))))

  ;; Process a sequence of encoded messages through protocol FSM
  (fset 'neovm--pf-process-messages
    (lambda (encoded-messages)
      (let ((state 'ready)
            (session nil)
            (results nil))
        (dolist (msg-str encoded-messages)
          (let* ((decoded (funcall 'neovm--pf-decode msg-str))
                 (msg-type (car decoded))
                 (fields (cdr decoded)))
            (cond
             ((and (eq state 'ready) (eq msg-type 'HELLO))
              (setq session (cdr (assq 'id fields)))
              (setq state 'authenticated)
              (setq results (cons (list 'hello session) results)))
             ((and (eq state 'authenticated) (eq msg-type 'CMD))
              (let ((cmd (cdr (assq 'action fields)))
                    (target (cdr (assq 'target fields))))
                (setq results (cons (list 'cmd cmd target) results))))
             ((eq msg-type 'BYE)
              (setq state 'ready)
              (setq results (cons (list 'bye session) results))
              (setq session nil))
             (t (setq results (cons (list 'unknown msg-type state) results))))))
        (list 'state state
              'results (nreverse results)))))

  (unwind-protect
      (list
       ;; Encode/decode roundtrip
       (let ((encoded (funcall 'neovm--pf-encode 'HELLO '((id . "sess-42") (version . "1.0")))))
         (list encoded (funcall 'neovm--pf-decode encoded)))

       (let ((encoded (funcall 'neovm--pf-encode 'CMD '((action . "get") (target . "/data")))))
         (list encoded (funcall 'neovm--pf-decode encoded)))

       ;; Process a session
       (funcall 'neovm--pf-process-messages
                (list (funcall 'neovm--pf-encode 'HELLO '((id . "s1") (version . "2")))
                      (funcall 'neovm--pf-encode 'CMD '((action . "list") (target . "/users")))
                      (funcall 'neovm--pf-encode 'CMD '((action . "get") (target . "/users/1")))
                      (funcall 'neovm--pf-encode 'BYE '((reason . "done")))))

       ;; Invalid sequence: CMD before HELLO
       (funcall 'neovm--pf-process-messages
                (list (funcall 'neovm--pf-encode 'CMD '((action . "get") (target . "/")))))

       ;; Empty fields
       (funcall 'neovm--pf-decode "PING:"))
    (fmakunbound 'neovm--pf-encode)
    (fmakunbound 'neovm--pf-decode)
    (fmakunbound 'neovm--pf-process-messages)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"HELLO:id=sess-42;version=1.0\" (HELLO (id . \"sess-42\") (version . \"1.0\"))) (\"CMD:action=get;target=/data\" (CMD (action . \"get\") (target . \"/data\"))) (state ready results ((hello \"s1\") (cmd \"list\" \"/users\") (cmd \"get\" \"/users/1\") (bye \"s1\"))) (state ready results ((unknown CMD ready))) (PING))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
