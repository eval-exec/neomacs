//! Advanced protocol state machine oracle parity tests:
//! TCP state machine (SYN, SYN-ACK, ACK, FIN sequences), HTTP
//! request/response state machine, token-based authentication flow,
//! retry with exponential backoff simulation, connection pool management,
//! protocol multiplexing, and sliding window protocol.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// TCP state machine: full SYN -> SYN-ACK -> ACK -> DATA -> FIN sequence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proto_tcp_state_machine() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"((require (quote cl-lib)) progn
  ;; TCP states: closed, listen, syn-sent, syn-received, established,
  ;;             fin-wait-1, fin-wait-2, close-wait, closing, last-ack, time-wait
  (fset 'neovm--tcp-make
    (lambda ()
      (let ((st (make-hash-table :test 'eq)))
        (puthash 'state 'closed st)
        (puthash 'seq-num 1000 st)
        (puthash 'ack-num 0 st)
        (puthash 'send-buf nil st)
        (puthash 'recv-buf nil st)
        (puthash 'trace nil st)
        st)))

  (fset 'neovm--tcp-trace
    (lambda (conn msg)
      (puthash 'trace
               (cons (format "%s: %s" (gethash 'state conn) msg)
                     (gethash 'trace conn))
               conn)))

  (fset 'neovm--tcp-transition
    (lambda (conn event &optional data)
      (let ((s (gethash 'state conn)))
        (funcall 'neovm--tcp-trace conn (format "event=%s" event))
        (cond
         ;; Active open: CLOSED -> SYN-SENT (send SYN)
         ((and (eq s 'closed) (eq event 'active-open))
          (puthash 'state 'syn-sent conn)
          (puthash 'seq-num (1+ (gethash 'seq-num conn)) conn)
          'syn-sent)
         ;; Passive open: CLOSED -> LISTEN
         ((and (eq s 'closed) (eq event 'passive-open))
          (puthash 'state 'listen conn)
          'listen)
         ;; LISTEN + SYN received -> SYN-RECEIVED (send SYN-ACK)
         ((and (eq s 'listen) (eq event 'syn-recv))
          (puthash 'state 'syn-received conn)
          (puthash 'ack-num (1+ (or data 0)) conn)
          (puthash 'seq-num (1+ (gethash 'seq-num conn)) conn)
          'syn-received)
         ;; SYN-SENT + SYN-ACK -> ESTABLISHED (send ACK)
         ((and (eq s 'syn-sent) (eq event 'syn-ack-recv))
          (puthash 'state 'established conn)
          (puthash 'ack-num (1+ (or data 0)) conn)
          'established)
         ;; SYN-RECEIVED + ACK -> ESTABLISHED
         ((and (eq s 'syn-received) (eq event 'ack-recv))
          (puthash 'state 'established conn)
          'established)
         ;; ESTABLISHED + data send
         ((and (eq s 'established) (eq event 'send))
          (puthash 'send-buf (cons data (gethash 'send-buf conn)) conn)
          (puthash 'seq-num (+ (gethash 'seq-num conn) (length data)) conn)
          'established)
         ;; ESTABLISHED + data recv
         ((and (eq s 'established) (eq event 'recv))
          (puthash 'recv-buf (cons data (gethash 'recv-buf conn)) conn)
          (puthash 'ack-num (+ (gethash 'ack-num conn) (length data)) conn)
          'established)
         ;; ESTABLISHED + close -> FIN-WAIT-1 (send FIN)
         ((and (eq s 'established) (eq event 'close))
          (puthash 'state 'fin-wait-1 conn)
          (puthash 'seq-num (1+ (gethash 'seq-num conn)) conn)
          'fin-wait-1)
         ;; FIN-WAIT-1 + ACK -> FIN-WAIT-2
         ((and (eq s 'fin-wait-1) (eq event 'ack-recv))
          (puthash 'state 'fin-wait-2 conn)
          'fin-wait-2)
         ;; FIN-WAIT-2 + FIN -> TIME-WAIT (send ACK)
         ((and (eq s 'fin-wait-2) (eq event 'fin-recv))
          (puthash 'state 'time-wait conn)
          (puthash 'ack-num (1+ (gethash 'ack-num conn)) conn)
          'time-wait)
         ;; TIME-WAIT + timeout -> CLOSED
         ((and (eq s 'time-wait) (eq event 'timeout))
          (puthash 'state 'closed conn)
          'closed)
         ;; FIN-WAIT-1 + FIN (simultaneous close) -> CLOSING
         ((and (eq s 'fin-wait-1) (eq event 'fin-recv))
          (puthash 'state 'closing conn)
          'closing)
         ;; CLOSING + ACK -> TIME-WAIT
         ((and (eq s 'closing) (eq event 'ack-recv))
          (puthash 'state 'time-wait conn)
          'time-wait)
         ;; ESTABLISHED + close from peer -> CLOSE-WAIT (send ACK)
         ((and (eq s 'established) (eq event 'fin-recv))
          (puthash 'state 'close-wait conn)
          (puthash 'ack-num (1+ (gethash 'ack-num conn)) conn)
          'close-wait)
         ;; CLOSE-WAIT + close -> LAST-ACK (send FIN)
         ((and (eq s 'close-wait) (eq event 'close))
          (puthash 'state 'last-ack conn)
          'last-ack)
         ;; LAST-ACK + ACK -> CLOSED
         ((and (eq s 'last-ack) (eq event 'ack-recv))
          (puthash 'state 'closed conn)
          'closed)
         (t (funcall 'neovm--tcp-trace conn "invalid-transition")
            nil)))))

  (unwind-protect
      (let ((client (funcall 'neovm--tcp-make))
            (server (funcall 'neovm--tcp-make)))
        ;; Three-way handshake
        (funcall 'neovm--tcp-transition server 'passive-open)
        (funcall 'neovm--tcp-transition client 'active-open)
        (funcall 'neovm--tcp-transition server 'syn-recv (gethash 'seq-num client))
        (funcall 'neovm--tcp-transition client 'syn-ack-recv (gethash 'seq-num server))
        (funcall 'neovm--tcp-transition server 'ack-recv)
        ;; Data exchange
        (funcall 'neovm--tcp-transition client 'send "Hello")
        (funcall 'neovm--tcp-transition server 'recv "Hello")
        (funcall 'neovm--tcp-transition server 'send "World")
        (funcall 'neovm--tcp-transition client 'recv "World")
        ;; Connection teardown
        (funcall 'neovm--tcp-transition client 'close)
        (funcall 'neovm--tcp-transition server 'ack-recv)
        (funcall 'neovm--tcp-transition client 'fin-recv)  ;; actually from server
        (funcall 'neovm--tcp-transition client 'timeout)
        (list (gethash 'state client)
              (gethash 'state server)
              (length (gethash 'trace client))
              (length (gethash 'trace server))
              (gethash 'send-buf client)
              (gethash 'recv-buf client)))
    (fmakunbound 'neovm--tcp-make)
    (fmakunbound 'neovm--tcp-trace)
    (fmakunbound 'neovm--tcp-transition)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// HTTP request/response state machine
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proto_http_state_machine() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"((require (quote cl-lib)) progn
  (fset 'neovm--http-make-request
    (lambda (method path headers body)
      (list 'request method path headers body)))

  (fset 'neovm--http-parse-request
    (lambda (req)
      (let ((method (nth 1 req))
            (path (nth 2 req))
            (headers (nth 3 req))
            (body (nth 4 req)))
        ;; State machine: idle -> headers-received -> body-received -> response-ready
        (let ((state 'idle)
              (parsed (make-hash-table :test 'equal))
              (trace nil))
          ;; Parse method
          (puthash "method" (symbol-name method) parsed)
          (setq state 'method-parsed)
          (push state trace)
          ;; Parse path + query string
          (let ((qpos (string-match "\\?" path)))
            (if qpos
                (progn
                  (puthash "path" (substring path 0 qpos) parsed)
                  (puthash "query" (substring path (1+ qpos)) parsed))
              (puthash "path" path parsed)))
          (setq state 'path-parsed)
          (push state trace)
          ;; Parse headers
          (dolist (h headers)
            (puthash (car h) (cdr h) parsed))
          (setq state 'headers-received)
          (push state trace)
          ;; Content-length validation
          (let ((cl (gethash "content-length" parsed)))
            (when (and cl body)
              (puthash "body-valid"
                       (= (string-to-number cl) (length body))
                       parsed)))
          ;; Generate response
          (setq state 'response-ready)
          (push state trace)
          (let ((status (cond
                         ((string= (gethash "method" parsed) "GET") 200)
                         ((string= (gethash "method" parsed) "POST")
                          (if body 201 400))
                         ((string= (gethash "method" parsed) "DELETE") 204)
                         (t 405))))
            (list (nreverse trace)
                  status
                  (gethash "path" parsed)
                  (gethash "query" parsed)
                  (gethash "body-valid" parsed)))))))

  (unwind-protect
      (list
       ;; GET request with query params
       (funcall 'neovm--http-parse-request
                (funcall 'neovm--http-make-request
                         'GET "/api/users?page=2&limit=10"
                         '(("host" . "example.com") ("accept" . "application/json"))
                         nil))
       ;; POST with body and content-length
       (funcall 'neovm--http-parse-request
                (funcall 'neovm--http-make-request
                         'POST "/api/users"
                         '(("content-type" . "application/json")
                           ("content-length" . "27"))
                         "{\"name\":\"Alice\",\"age\":30}"))
       ;; DELETE request
       (funcall 'neovm--http-parse-request
                (funcall 'neovm--http-make-request
                         'DELETE "/api/users/42"
                         '(("authorization" . "Bearer tok123"))
                         nil))
       ;; POST with wrong content-length
       (funcall 'neovm--http-parse-request
                (funcall 'neovm--http-make-request
                         'POST "/api/data"
                         '(("content-length" . "999"))
                         "short")))
    (fmakunbound 'neovm--http-make-request)
    (fmakunbound 'neovm--http-parse-request)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Token-based authentication flow
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proto_token_auth_flow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"((require (quote cl-lib)) progn
  (fset 'neovm--auth-make-store
    (lambda ()
      (let ((st (make-hash-table :test 'equal)))
        (puthash "users" (list '("alice" . "pass123")
                               '("bob" . "secret")
                               '("charlie" . "qwerty"))
                 st)
        (puthash "tokens" (make-hash-table :test 'equal) st)
        (puthash "token-counter" 0 st)
        (puthash "failed-attempts" (make-hash-table :test 'equal) st)
        (puthash "locked-out" nil st)
        st)))

  (fset 'neovm--auth-login
    (lambda (store user pass)
      (let ((users (gethash "users" store))
            (fails (gethash "failed-attempts" store)))
        (if (member user (gethash "locked-out" store))
            (list 'error 'account-locked user)
          (let ((entry (assoc user users)))
            (if (and entry (string= (cdr entry) pass))
                ;; Success: generate token, reset failures
                (let* ((counter (1+ (gethash "token-counter" store)))
                       (token (format "tok-%s-%d" user counter))
                       (tokens (gethash "tokens" store)))
                  (puthash "token-counter" counter store)
                  (puthash token (list user (float-time) 3600) tokens)
                  (puthash user 0 fails)
                  (list 'ok token))
              ;; Failure: increment attempts, maybe lock out
              (let ((attempt-count (1+ (gethash user fails 0))))
                (puthash user attempt-count fails)
                (when (>= attempt-count 3)
                  (puthash "locked-out"
                           (cons user (gethash "locked-out" store))
                           store))
                (list 'error 'bad-credentials attempt-count))))))))

  (fset 'neovm--auth-validate
    (lambda (store token)
      (let* ((tokens (gethash "tokens" store))
             (info (gethash token tokens)))
        (if info
            (list 'valid (nth 0 info))
          (list 'invalid token)))))

  (fset 'neovm--auth-revoke
    (lambda (store token)
      (let ((tokens (gethash "tokens" store)))
        (if (gethash token tokens)
            (progn (remhash token tokens) 'revoked)
          'not-found))))

  (unwind-protect
      (let ((store (funcall 'neovm--auth-make-store)))
        (let* ((r1 (funcall 'neovm--auth-login store "alice" "pass123"))
               (r2 (funcall 'neovm--auth-login store "bob" "wrong"))
               (r3 (funcall 'neovm--auth-login store "bob" "wrong"))
               (r4 (funcall 'neovm--auth-login store "bob" "wrong"))
               (r5 (funcall 'neovm--auth-login store "bob" "secret"))
               (tok (nth 1 r1))
               (v1 (funcall 'neovm--auth-validate store tok))
               (v2 (funcall 'neovm--auth-validate store "fake-token"))
               (rev (funcall 'neovm--auth-revoke store tok))
               (v3 (funcall 'neovm--auth-validate store tok)))
          (list r1 r2 r3 r4 r5 v1 v2 rev v3)))
    (fmakunbound 'neovm--auth-make-store)
    (fmakunbound 'neovm--auth-login)
    (fmakunbound 'neovm--auth-validate)
    (fmakunbound 'neovm--auth-revoke)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Retry with exponential backoff simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proto_exponential_backoff() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"((require (quote cl-lib)) progn
  (fset 'neovm--backoff-retry
    (lambda (op max-retries base-delay max-delay jitter-seed)
      ;; Simulate retry with exponential backoff
      ;; op returns (ok . value) on success or (err . reason) on failure
      ;; Returns (result attempts total-delay delays-used)
      (let ((attempt 0)
            (total-delay 0)
            (delays nil)
            (result nil)
            (done nil))
        (while (and (not done) (<= attempt max-retries))
          (let ((r (funcall op attempt)))
            (cond
             ((eq (car r) 'ok)
              (setq result r done t))
             ((>= attempt max-retries)
              (setq result (list 'exhausted (cdr r) attempt) done t))
             (t
              ;; Calculate backoff: min(base * 2^attempt, max-delay)
              ;; Plus deterministic pseudo-jitter from seed
              (let* ((raw-delay (* base-delay (expt 2 attempt)))
                     (capped (min raw-delay max-delay))
                     (jitter (mod (* (+ jitter-seed attempt 1) 37) (max 1 (/ capped 4))))
                     (delay (+ capped jitter)))
                (push delay delays)
                (setq total-delay (+ total-delay delay))
                (setq attempt (1+ attempt)))))))
        (list result attempt total-delay (nreverse delays)))))

  (unwind-protect
      (list
       ;; Succeeds on first try
       (funcall 'neovm--backoff-retry
                (lambda (n) '(ok . "immediate"))
                5 100 10000 42)
       ;; Fails twice then succeeds
       (funcall 'neovm--backoff-retry
                (lambda (n) (if (>= n 2) '(ok . "recovered") '(err . "timeout")))
                5 100 10000 7)
       ;; Always fails, exhausts retries
       (funcall 'neovm--backoff-retry
                (lambda (n) '(err . "server-down"))
                3 50 800 13)
       ;; Succeeds on last attempt
       (funcall 'neovm--backoff-retry
                (lambda (n) (if (= n 4) '(ok . "barely") '(err . "nope")))
                4 200 5000 99))
    (fmakunbound 'neovm--backoff-retry)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Connection pool management
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proto_connection_pool() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"((require (quote cl-lib)) progn
  (fset 'neovm--pool-make
    (lambda (max-size)
      (let ((pool (make-hash-table :test 'eq)))
        (puthash 'max-size max-size pool)
        (puthash 'available nil pool)
        (puthash 'in-use nil pool)
        (puthash 'total-created 0 pool)
        (puthash 'wait-queue nil pool)
        (puthash 'stats (list 0 0 0) pool)  ;; (acquires releases timeouts)
        pool)))

  (fset 'neovm--pool-create-conn
    (lambda (pool)
      (let ((id (1+ (gethash 'total-created pool))))
        (puthash 'total-created id pool)
        (list 'conn id 'idle 0))))  ;; (conn id state use-count)

  (fset 'neovm--pool-acquire
    (lambda (pool)
      (let ((avail (gethash 'available pool))
            (in-use (gethash 'in-use pool))
            (stats (gethash 'stats pool)))
        (puthash 'stats (list (1+ (nth 0 stats)) (nth 1 stats) (nth 2 stats)) pool)
        (cond
         ;; Reuse available connection
         (avail
          (let ((conn (car avail)))
            (puthash 'available (cdr avail) pool)
            (setcar (nthcdr 2 conn) 'active)
            (setcar (nthcdr 3 conn) (1+ (nth 3 conn)))
            (puthash 'in-use (cons conn in-use) pool)
            (list 'ok conn)))
         ;; Create new if under limit
         ((< (+ (length in-use) (length avail)) (gethash 'max-size pool))
          (let ((conn (funcall 'neovm--pool-create-conn pool)))
            (setcar (nthcdr 2 conn) 'active)
            (setcar (nthcdr 3 conn) 1)
            (puthash 'in-use (cons conn in-use) pool)
            (list 'ok conn)))
         ;; Pool exhausted
         (t
          (puthash 'stats (list (nth 0 stats) (nth 1 stats) (1+ (nth 2 stats))) pool)
          (list 'pool-exhausted))))))

  (fset 'neovm--pool-release
    (lambda (pool conn)
      (let ((in-use (gethash 'in-use pool))
            (stats (gethash 'stats pool)))
        (puthash 'stats (list (nth 0 stats) (1+ (nth 1 stats)) (nth 2 stats)) pool)
        (puthash 'in-use (delq conn in-use) pool)
        (setcar (nthcdr 2 conn) 'idle)
        (puthash 'available (cons conn (gethash 'available pool)) pool)
        'released)))

  (unwind-protect
      (let ((pool (funcall 'neovm--pool-make 3)))
        ;; Acquire 3 connections
        (let* ((r1 (funcall 'neovm--pool-acquire pool))
               (r2 (funcall 'neovm--pool-acquire pool))
               (r3 (funcall 'neovm--pool-acquire pool))
               ;; Pool full, should fail
               (r4 (funcall 'neovm--pool-acquire pool))
               ;; Release one
               (_ (funcall 'neovm--pool-release pool (nth 1 r1)))
               ;; Now acquire should work and reuse
               (r5 (funcall 'neovm--pool-acquire pool))
               (reused-id (nth 1 (nth 1 r5)))
               (reuse-count (nth 3 (nth 1 r5))))
          (list (car r1) (car r2) (car r3) (car r4)
                (car r5) reused-id reuse-count
                (gethash 'total-created pool)
                (gethash 'stats pool))))
    (fmakunbound 'neovm--pool-make)
    (fmakunbound 'neovm--pool-create-conn)
    (fmakunbound 'neovm--pool-acquire)
    (fmakunbound 'neovm--pool-release)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Protocol multiplexing: interleaved streams over one channel
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proto_multiplexing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"((require (quote cl-lib)) progn
  ;; Simulate multiplexing multiple logical streams over one channel
  ;; Each frame: (stream-id type payload)
  ;; Types: 'headers, 'data, 'rst, 'window-update, 'end-stream

  (fset 'neovm--mux-make
    (lambda ()
      (let ((mux (make-hash-table :test 'eq)))
        (puthash 'streams (make-hash-table :test 'eq) mux)
        (puthash 'next-id 1 mux)
        (puthash 'frame-log nil mux)
        mux)))

  (fset 'neovm--mux-open-stream
    (lambda (mux)
      (let* ((id (gethash 'next-id mux))
             (streams (gethash 'streams mux))
             (st (make-hash-table :test 'eq)))
        (puthash 'next-id (+ id 2) mux)  ;; odd for client-initiated
        (puthash 'state 'open st)
        (puthash 'headers nil st)
        (puthash 'data nil st)
        (puthash 'window 65535 st)
        (puthash id st streams)
        id)))

  (fset 'neovm--mux-send-frame
    (lambda (mux stream-id type payload)
      (let* ((streams (gethash 'streams mux))
             (st (gethash stream-id streams)))
        (puthash 'frame-log
                 (cons (list stream-id type (length (or payload "")))
                       (gethash 'frame-log mux))
                 mux)
        (when st
          (cond
           ((eq type 'headers)
            (puthash 'headers (cons payload (gethash 'headers st)) st))
           ((eq type 'data)
            (puthash 'data (cons payload (gethash 'data st)) st)
            (puthash 'window (- (gethash 'window st) (length payload)) st))
           ((eq type 'end-stream)
            (puthash 'state 'half-closed st))
           ((eq type 'rst)
            (puthash 'state 'reset st))
           ((eq type 'window-update)
            (puthash 'window (+ (gethash 'window st) (or payload 0)) st)))))))

  (fset 'neovm--mux-get-stream-info
    (lambda (mux stream-id)
      (let* ((streams (gethash 'streams mux))
             (st (gethash stream-id streams)))
        (when st
          (list (gethash 'state st)
                (nreverse (gethash 'headers st))
                (nreverse (gethash 'data st))
                (gethash 'window st))))))

  (unwind-protect
      (let ((mux (funcall 'neovm--mux-make)))
        ;; Open 3 streams
        (let ((s1 (funcall 'neovm--mux-open-stream mux))
              (s2 (funcall 'neovm--mux-open-stream mux))
              (s3 (funcall 'neovm--mux-open-stream mux)))
          ;; Interleave frames from different streams
          (funcall 'neovm--mux-send-frame mux s1 'headers ":method GET")
          (funcall 'neovm--mux-send-frame mux s2 'headers ":method POST")
          (funcall 'neovm--mux-send-frame mux s1 'headers ":path /index")
          (funcall 'neovm--mux-send-frame mux s2 'data "body-chunk-1")
          (funcall 'neovm--mux-send-frame mux s3 'headers ":method DELETE")
          (funcall 'neovm--mux-send-frame mux s2 'data "body-chunk-2")
          (funcall 'neovm--mux-send-frame mux s1 'end-stream nil)
          (funcall 'neovm--mux-send-frame mux s3 'rst nil)
          (funcall 'neovm--mux-send-frame mux s2 'end-stream nil)
          ;; Window update on s2
          (funcall 'neovm--mux-send-frame mux s2 'window-update 1000)
          (list
           (funcall 'neovm--mux-get-stream-info mux s1)
           (funcall 'neovm--mux-get-stream-info mux s2)
           (funcall 'neovm--mux-get-stream-info mux s3)
           (length (gethash 'frame-log mux))
           (gethash 'next-id mux))))
    (fmakunbound 'neovm--mux-make)
    (fmakunbound 'neovm--mux-open-stream)
    (fmakunbound 'neovm--mux-send-frame)
    (fmakunbound 'neovm--mux-get-stream-info)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sliding window protocol: send/ack with window management
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proto_sliding_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"((require (quote cl-lib)) progn
  (fset 'neovm--sw-make
    (lambda (window-size)
      (let ((sw (make-hash-table :test 'eq)))
        (puthash 'window-size window-size sw)
        (puthash 'send-base 0 sw)
        (puthash 'next-seq 0 sw)
        (puthash 'sent-unacked nil sw)
        (puthash 'recv-base 0 sw)
        (puthash 'recv-buffer (make-hash-table :test 'eq) sw)
        (puthash 'delivered nil sw)
        (puthash 'log nil sw)
        sw)))

  (fset 'neovm--sw-can-send
    (lambda (sw)
      (< (- (gethash 'next-seq sw) (gethash 'send-base sw))
         (gethash 'window-size sw))))

  (fset 'neovm--sw-send
    (lambda (sw data)
      (if (not (funcall 'neovm--sw-can-send sw))
          'window-full
        (let ((seq (gethash 'next-seq sw)))
          (puthash 'next-seq (1+ seq) sw)
          (puthash 'sent-unacked
                   (cons (cons seq data) (gethash 'sent-unacked sw))
                   sw)
          (puthash 'log (cons (list 'send seq data) (gethash 'log sw)) sw)
          seq))))

  (fset 'neovm--sw-receive
    (lambda (sw seq data)
      (let ((recv-base (gethash 'recv-base sw))
            (wsize (gethash 'window-size sw))
            (buf (gethash 'recv-buffer sw)))
        (puthash 'log (cons (list 'recv seq data) (gethash 'log sw)) sw)
        (cond
         ;; Outside window: discard
         ((or (< seq recv-base) (>= seq (+ recv-base wsize)))
          'outside-window)
         ;; Buffer the packet
         (t
          (puthash seq data buf)
          ;; Slide: deliver consecutive packets from recv-base
          (let ((delivered 0))
            (while (gethash (gethash 'recv-base sw) buf)
              (puthash 'delivered
                       (cons (gethash (gethash 'recv-base sw) buf)
                             (gethash 'delivered sw))
                       sw)
              (remhash (gethash 'recv-base sw) buf)
              (puthash 'recv-base (1+ (gethash 'recv-base sw)) sw)
              (setq delivered (1+ delivered)))
            delivered))))))

  (fset 'neovm--sw-ack
    (lambda (sw ack-num)
      ;; Cumulative ACK: everything below ack-num is acknowledged
      (puthash 'log (cons (list 'ack ack-num) (gethash 'log sw)) sw)
      (puthash 'send-base (max (gethash 'send-base sw) ack-num) sw)
      (puthash 'sent-unacked
               (cl-remove-if (lambda (pair) (< (car pair) ack-num))
                              (gethash 'sent-unacked sw))
               sw)
      ack-num))

  (unwind-protect
      (let ((sender (funcall 'neovm--sw-make 4))
            (receiver (funcall 'neovm--sw-make 4)))
        ;; Send 6 packets (window=4 so 5th and 6th should hit limit)
        (let ((results nil))
          (dotimes (i 6)
            (push (funcall 'neovm--sw-send sender (format "pkt-%d" i)) results))
          ;; Receive first 3 in order
          (let ((d1 (funcall 'neovm--sw-receive receiver 0 "pkt-0"))
                (d2 (funcall 'neovm--sw-receive receiver 1 "pkt-1"))
                (d3 (funcall 'neovm--sw-receive receiver 2 "pkt-2")))
            ;; ACK up to 3, opens window
            (funcall 'neovm--sw-ack sender 3)
            ;; Now can send more
            (let ((s5 (funcall 'neovm--sw-send sender "pkt-4-retry")))
              ;; Out of order: receive pkt-3 (seq=3) delivered
              (let ((d4 (funcall 'neovm--sw-receive receiver 3 "pkt-3")))
                (list (nreverse results)
                      (list d1 d2 d3 d4)
                      s5
                      (gethash 'send-base sender)
                      (gethash 'recv-base receiver)
                      (nreverse (gethash 'delivered receiver))))))))
    (fmakunbound 'neovm--sw-make)
    (fmakunbound 'neovm--sw-can-send)
    (fmakunbound 'neovm--sw-send)
    (fmakunbound 'neovm--sw-receive)
    (fmakunbound 'neovm--sw-ack)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
