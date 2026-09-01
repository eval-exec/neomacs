//! Oracle parity tests implementing channel/message-passing patterns:
//! bounded channel (send/receive), fan-out (one producer, multiple
//! consumers), fan-in (multiple producers, one consumer), request-reply
//! pattern, pub/sub with topic filtering, channel pipeline (compose
//! channels).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Bounded channel: send/receive with capacity, overflow handling
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_channel_bounded_send_receive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((make-channel
           (lambda (capacity)
             "Create a bounded channel with CAPACITY.
              Returns alist with :send :recv :size :full-p :empty-p :drain."
             (let ((buffer nil)
                   (count 0)
                   (total-sent 0)
                   (total-recv 0)
                   (dropped 0))
               (list
                (cons :send
                  (lambda (val)
                    (if (>= count capacity)
                        (progn (setq dropped (1+ dropped)) nil)
                      (setq buffer (append buffer (list val)))
                      (setq count (1+ count))
                      (setq total-sent (1+ total-sent))
                      t)))
                (cons :recv
                  (lambda ()
                    (if (= count 0)
                        (cons nil nil)
                      (let ((val (car buffer)))
                        (setq buffer (cdr buffer))
                        (setq count (1- count))
                        (setq total-recv (1+ total-recv))
                        (cons val t)))))
                (cons :size (lambda () count))
                (cons :full-p (lambda () (>= count capacity)))
                (cons :empty-p (lambda () (= count 0)))
                (cons :drain
                  (lambda ()
                    (let ((result nil))
                      (while (> count 0)
                        (setq result (cons (car buffer) result))
                        (setq buffer (cdr buffer))
                        (setq count (1- count))
                        (setq total-recv (1+ total-recv)))
                      (nreverse result))))
                (cons :stats
                  (lambda () (list (cons 'sent total-sent)
                                   (cons 'recv total-recv)
                                   (cons 'dropped dropped)))))))))
  (let* ((ch (funcall make-channel 3))
         (send (cdr (assq :send ch)))
         (recv (cdr (assq :recv ch)))
         (size (cdr (assq :size ch)))
         (full-p (cdr (assq :full-p ch)))
         (empty-p (cdr (assq :empty-p ch)))
         (drain (cdr (assq :drain ch)))
         (stats (cdr (assq :stats ch))))
    (list
      ;; Initially empty
      (list (funcall empty-p) (funcall size))
      ;; Send 3 items (fills to capacity)
      (list (funcall send 'a) (funcall send 'b) (funcall send 'c))
      ;; Full check
      (list (funcall full-p) (funcall size))
      ;; Overflow: 4th send fails
      (funcall send 'd)
      ;; Receive in order
      (list (funcall recv) (funcall recv))
      ;; Now has room for more
      (list (funcall full-p) (funcall size))
      ;; Send more and drain
      (funcall send 'x)
      (funcall send 'y)
      (funcall drain)
      ;; Stats
      (funcall stats))))"#;
    let expect = expect_test::expect![[
        r#""OK ((t 0) (t t t) (t 3) nil ((a . t) (b . t)) (nil 1) t t (c x y) ((sent . 5) (recv . 5) (dropped . 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Fan-out: one producer dispatches to multiple consumers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_channel_fan_out() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((make-channel
           (lambda (cap)
             (let ((buf nil) (cnt 0))
               (list
                (cons :send (lambda (v)
                  (if (>= cnt cap) nil
                    (setq buf (append buf (list v)))
                    (setq cnt (1+ cnt)) t)))
                (cons :recv (lambda ()
                  (if (= cnt 0) (cons nil nil)
                    (let ((v (car buf)))
                      (setq buf (cdr buf))
                      (setq cnt (1- cnt))
                      (cons v t)))))
                (cons :drain (lambda ()
                  (let ((r nil))
                    (while (> cnt 0)
                      (setq r (cons (car buf) r))
                      (setq buf (cdr buf))
                      (setq cnt (1- cnt)))
                    (nreverse r)))))))))
  ;; Fan-out: round-robin dispatcher sends to N consumer channels
  (let* ((c1 (funcall make-channel 10))
         (c2 (funcall make-channel 10))
         (c3 (funcall make-channel 10))
         (consumers (list c1 c2 c3))
         (idx 0))
    ;; Dispatch items round-robin to consumers
    (dolist (item '(10 20 30 40 50 60 70 80 90))
      (let* ((target (nth (% idx (length consumers)) consumers))
             (send-fn (cdr (assq :send target))))
        (funcall send-fn item))
      (setq idx (1+ idx)))
    ;; Drain each consumer
    (list
      (funcall (cdr (assq :drain c1)))
      (funcall (cdr (assq :drain c2)))
      (funcall (cdr (assq :drain c3)))
      ;; Also test broadcast: send same item to ALL consumers
      (progn
        (dolist (msg '(broadcast-1 broadcast-2))
          (dolist (ch consumers)
            (funcall (cdr (assq :send ch)) msg)))
        (list
          (funcall (cdr (assq :drain c1)))
          (funcall (cdr (assq :drain c2)))
          (funcall (cdr (assq :drain c3))))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((10 40 70) (20 50 80) (30 60 90) ((broadcast-1 broadcast-2) (broadcast-1 broadcast-2) (broadcast-1 broadcast-2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Fan-in: multiple producers, one consumer, merge results
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_channel_fan_in() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((make-channel
           (lambda (cap)
             (let ((buf nil) (cnt 0))
               (list
                (cons :send (lambda (v)
                  (if (>= cnt cap) nil
                    (setq buf (append buf (list v)))
                    (setq cnt (1+ cnt)) t)))
                (cons :recv (lambda ()
                  (if (= cnt 0) (cons nil nil)
                    (let ((v (car buf)))
                      (setq buf (cdr buf))
                      (setq cnt (1- cnt))
                      (cons v t)))))
                (cons :size (lambda () cnt))
                (cons :drain (lambda ()
                  (let ((r nil))
                    (while (> cnt 0)
                      (setq r (cons (car buf) r))
                      (setq buf (cdr buf))
                      (setq cnt (1- cnt)))
                    (nreverse r)))))))))
  ;; Fan-in: multiple producers write to single shared channel
  (let* ((shared (funcall make-channel 50))
         (send-fn (cdr (assq :send shared)))
         (drain-fn (cdr (assq :drain shared)))
         (size-fn (cdr (assq :size shared))))
    ;; Producer 1: sends squares
    (dolist (x '(1 2 3 4 5))
      (funcall send-fn (list 'P1 (* x x))))
    ;; Producer 2: sends cubes
    (dolist (x '(1 2 3))
      (funcall send-fn (list 'P2 (* x x x))))
    ;; Producer 3: sends fibonacci
    (let ((a 0) (b 1))
      (dotimes (_ 6)
        (funcall send-fn (list 'P3 a))
        (let ((next (+ a b)))
          (setq a b b next))))
    (list
      ;; Total messages in channel
      (funcall size-fn)
      ;; Drain all messages
      (funcall drain-fn)
      ;; Channel should be empty now
      (funcall size-fn)
      ;; Consumer processes messages and tallies by producer
      (progn
        ;; Refill
        (funcall send-fn '(P1 100))
        (funcall send-fn '(P2 200))
        (funcall send-fn '(P1 300))
        (funcall send-fn '(P3 400))
        (funcall send-fn '(P2 500))
        (let ((all (funcall drain-fn))
              (counts (make-hash-table :test 'eq)))
          (dolist (msg all)
            (let ((producer (car msg)))
              (puthash producer (1+ (or (gethash producer counts) 0)) counts)))
          (sort (let ((r nil))
                  (maphash (lambda (k v) (setq r (cons (cons k v) r))) counts)
                  r)
                (lambda (a b) (string< (symbol-name (car a))
                                       (symbol-name (car b))))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (14 ((P1 1) (P1 4) (P1 9) (P1 16) (P1 25) (P2 1) (P2 8) (P2 27) (P3 0) (P3 1) (P3 1) (P3 2) (P3 3) (P3 5)) 0 ((P1 . 2) (P2 . 2) (P3 . 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Request-reply pattern: synchronous RPC over channels
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_channel_request_reply() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((make-channel
           (lambda (cap)
             (let ((buf nil) (cnt 0))
               (list
                (cons :send (lambda (v)
                  (if (>= cnt cap) nil
                    (setq buf (append buf (list v)))
                    (setq cnt (1+ cnt)) t)))
                (cons :recv (lambda ()
                  (if (= cnt 0) (cons nil nil)
                    (let ((v (car buf)))
                      (setq buf (cdr buf))
                      (setq cnt (1- cnt))
                      (cons v t))))))))))
  ;; Request-reply: client sends request with reply-channel,
  ;; server processes and sends response back via reply-channel.
  (let* ((request-ch (funcall make-channel 10))
         (req-send (cdr (assq :send request-ch)))
         (req-recv (cdr (assq :recv request-ch))))
    ;; Server handler: processes requests
    (let ((server-process
           (lambda ()
             (let ((req (funcall req-recv)))
               (when (cdr req)
                 (let* ((request (car req))
                        (op (cdr (assq 'op request)))
                        (args (cdr (assq 'args request)))
                        (reply-ch (cdr (assq 'reply request)))
                        (reply-send (cdr (assq :send reply-ch)))
                        (result
                         (cond
                           ((eq op 'add) (apply #'+ args))
                           ((eq op 'mul) (apply #'* args))
                           ((eq op 'len) (length (car args)))
                           ((eq op 'upper) (upcase (car args)))
                           (t (list 'error 'unknown-op op)))))
                   (funcall reply-send (list (cons 'result result)
                                             (cons 'op op)))))))))
      ;; Client: sends request, gets reply
      (let ((call-server
             (lambda (op args)
               (let* ((reply-ch (funcall make-channel 1))
                      (request (list (cons 'op op)
                                     (cons 'args args)
                                     (cons 'reply reply-ch))))
                 (funcall req-send request)
                 (funcall server-process)
                 (let ((response (funcall (cdr (assq :recv reply-ch)))))
                   (when (cdr response)
                     (car response)))))))

        (list
          ;; Add
          (funcall call-server 'add '(10 20 30))
          ;; Multiply
          (funcall call-server 'mul '(2 3 4))
          ;; String length
          (funcall call-server 'len '("hello world"))
          ;; Uppercase
          (funcall call-server 'upper '("hello"))
          ;; Unknown op
          (funcall call-server 'unknown '(1 2 3))
          ;; Multiple calls in sequence
          (let ((results nil))
            (dotimes (i 5)
              (setq results
                    (cons (funcall call-server 'add (list i (* i 10)))
                          results)))
            (nreverse results)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((result . 60) (op . add)) ((result . 24) (op . mul)) ((result . 11) (op . len)) ((result . \"HELLO\") (op . upper)) ((result error unknown-op unknown) (op . unknown)) (((result . 0) (op . add)) ((result . 11) (op . add)) ((result . 22) (op . add)) ((result . 33) (op . add)) ((result . 44) (op . add))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Pub/sub with topic filtering
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_channel_pub_sub() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--ps-subscriptions nil)
  (defvar neovm--ps-mailboxes nil)

  (fset 'neovm--ps-init
    (lambda ()
      (setq neovm--ps-subscriptions (make-hash-table :test 'eq))
      (setq neovm--ps-mailboxes (make-hash-table :test 'eq))))

  (fset 'neovm--ps-subscribe
    (lambda (subscriber topic)
      "Subscribe SUBSCRIBER to TOPIC."
      ;; Add subscriber to topic's subscriber list
      (let ((subs (gethash topic neovm--ps-subscriptions)))
        (unless (memq subscriber subs)
          (puthash topic (cons subscriber subs) neovm--ps-subscriptions)))
      ;; Ensure subscriber has a mailbox
      (unless (gethash subscriber neovm--ps-mailboxes)
        (puthash subscriber nil neovm--ps-mailboxes))))

  (fset 'neovm--ps-publish
    (lambda (topic message)
      "Publish MESSAGE to all subscribers of TOPIC."
      (let ((subs (gethash topic neovm--ps-subscriptions))
            (delivered 0))
        (dolist (sub subs)
          (let ((mbox (gethash sub neovm--ps-mailboxes)))
            (puthash sub (append mbox (list (cons topic message)))
                     neovm--ps-mailboxes)
            (setq delivered (1+ delivered))))
        delivered)))

  (fset 'neovm--ps-receive
    (lambda (subscriber)
      "Get all messages for SUBSCRIBER, clearing the mailbox."
      (let ((msgs (gethash subscriber neovm--ps-mailboxes)))
        (puthash subscriber nil neovm--ps-mailboxes)
        msgs)))

  (fset 'neovm--ps-receive-topic
    (lambda (subscriber topic)
      "Get messages for SUBSCRIBER on specific TOPIC only."
      (let ((msgs (gethash subscriber neovm--ps-mailboxes))
            (matching nil)
            (rest nil))
        (dolist (msg msgs)
          (if (eq (car msg) topic)
              (setq matching (cons msg matching))
            (setq rest (cons msg rest))))
        ;; Leave non-matching messages in mailbox
        (puthash subscriber (nreverse rest) neovm--ps-mailboxes)
        (nreverse matching))))

  (unwind-protect
      (progn
        (funcall 'neovm--ps-init)
        ;; Set up subscriptions
        (funcall 'neovm--ps-subscribe 'alice 'news)
        (funcall 'neovm--ps-subscribe 'alice 'sports)
        (funcall 'neovm--ps-subscribe 'bob 'news)
        (funcall 'neovm--ps-subscribe 'bob 'tech)
        (funcall 'neovm--ps-subscribe 'charlie 'sports)
        (funcall 'neovm--ps-subscribe 'charlie 'tech)
        (funcall 'neovm--ps-subscribe 'charlie 'news)

        ;; Publish messages
        (let ((d1 (funcall 'neovm--ps-publish 'news "Breaking news!"))
              (d2 (funcall 'neovm--ps-publish 'sports "Game results"))
              (d3 (funcall 'neovm--ps-publish 'tech "New release"))
              (d4 (funcall 'neovm--ps-publish 'news "More news"))
              (d5 (funcall 'neovm--ps-publish 'finance "Stock update")))
          (list
            ;; Delivery counts
            (list d1 d2 d3 d4 d5)
            ;; Alice's messages (news + sports)
            (funcall 'neovm--ps-receive 'alice)
            ;; Bob's messages (news + tech)
            (funcall 'neovm--ps-receive 'bob)
            ;; Charlie receives only news topic
            (funcall 'neovm--ps-receive-topic 'charlie 'news)
            ;; Charlie's remaining messages (sports + tech)
            (funcall 'neovm--ps-receive 'charlie)
            ;; Alice already drained, should be empty
            (funcall 'neovm--ps-receive 'alice)
            ;; Publish to no-subscriber topic
            (funcall 'neovm--ps-publish 'cooking "Recipe"))))
    (fmakunbound 'neovm--ps-init)
    (fmakunbound 'neovm--ps-subscribe)
    (fmakunbound 'neovm--ps-publish)
    (fmakunbound 'neovm--ps-receive)
    (fmakunbound 'neovm--ps-receive-topic)
    (makunbound 'neovm--ps-subscriptions)
    (makunbound 'neovm--ps-mailboxes)))"#;
    let expect = expect_test::expect![[
        r#""OK ((3 2 2 3 0) ((news . \"Breaking news!\") (sports . \"Game results\") (news . \"More news\")) ((news . \"Breaking news!\") (tech . \"New release\") (news . \"More news\")) ((news . \"Breaking news!\") (news . \"More news\")) ((sports . \"Game results\") (tech . \"New release\")) nil 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Channel pipeline: compose channels into processing pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_channel_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((make-channel
           (lambda (cap)
             (let ((buf nil) (cnt 0))
               (list
                (cons :send (lambda (v)
                  (if (>= cnt cap) nil
                    (setq buf (append buf (list v)))
                    (setq cnt (1+ cnt)) t)))
                (cons :recv (lambda ()
                  (if (= cnt 0) (cons nil nil)
                    (let ((v (car buf)))
                      (setq buf (cdr buf))
                      (setq cnt (1- cnt))
                      (cons v t)))))
                (cons :size (lambda () cnt))
                (cons :drain (lambda ()
                  (let ((r nil))
                    (while (> cnt 0)
                      (setq r (cons (car buf) r))
                      (setq buf (cdr buf))
                      (setq cnt (1- cnt)))
                    (nreverse r)))))))))
  ;; Pipeline stage: reads from input channel, transforms, writes to output
  (let ((make-stage
         (lambda (name transform-fn in-ch out-ch)
           (list
            (cons :name name)
            (cons :process
              (lambda ()
                "Process all available items from input to output."
                (let ((processed 0)
                      (recv-fn (cdr (assq :recv in-ch)))
                      (send-fn (cdr (assq :send out-ch))))
                  (let ((item (funcall recv-fn)))
                    (while (cdr item)
                      (let ((result (funcall transform-fn (car item))))
                        (funcall send-fn result)
                        (setq processed (1+ processed)))
                      (setq item (funcall recv-fn))))
                  processed)))
            (cons :in in-ch)
            (cons :out out-ch)))))

    ;; Build a 4-stage pipeline:
    ;; Input -> Stage1(double) -> Stage2(add-5) -> Stage3(filter>15) -> Output
    (let* ((ch0 (funcall make-channel 20))  ;; input
           (ch1 (funcall make-channel 20))
           (ch2 (funcall make-channel 20))
           (ch3 (funcall make-channel 20))  ;; output

           (stage1 (funcall make-stage 'double
                            (lambda (x) (* x 2)) ch0 ch1))
           (stage2 (funcall make-stage 'add5
                            (lambda (x) (+ x 5)) ch1 ch2))
           ;; Stage 3: filter - passes through only if > 15, otherwise sends 'skip
           (stage3 (funcall make-stage 'filter-gt15
                            (lambda (x) (if (> x 15) x 'skip)) ch2 ch3)))

      ;; Feed input
      (let ((send0 (cdr (assq :send ch0))))
        (dolist (v '(1 3 5 7 9 2 4 6 8 10))
          (funcall send0 v)))

      ;; Process pipeline stages in order
      (let ((p1 (funcall (cdr (assq :process stage1))))
            (p2 (funcall (cdr (assq :process stage2))))
            (p3 (funcall (cdr (assq :process stage3)))))

        ;; Drain output and separate results from 'skip markers
        (let* ((output (funcall (cdr (assq :drain ch3))))
               (passed (let ((r nil))
                         (dolist (v output) (unless (eq v 'skip) (setq r (cons v r))))
                         (nreverse r)))
               (skipped-count (let ((c 0))
                                (dolist (v output) (when (eq v 'skip) (setq c (1+ c))))
                                c)))

          (list
            ;; Items processed at each stage
            (list p1 p2 p3)
            ;; Final output (only items > 15 after transformations)
            passed
            ;; Count of items that didn't pass filter
            skipped-count
            ;; Verify pipeline is empty
            (list (funcall (cdr (assq :size ch0)))
                  (funcall (cdr (assq :size ch1)))
                  (funcall (cdr (assq :size ch2)))
                  (funcall (cdr (assq :size ch3))))

            ;; Second pass: different data
            (progn
              (dolist (v '(20 30 40))
                (funcall (cdr (assq :send ch0)) v))
              (funcall (cdr (assq :process stage1)))
              (funcall (cdr (assq :process stage2)))
              (funcall (cdr (assq :process stage3)))
              (let ((out2 (funcall (cdr (assq :drain ch3)))))
                ;; All should pass filter since 20*2+5=45, 30*2+5=65, 40*2+5=85
                out2))))))))"#;
    let expect =
        expect_test::expect![[r#""OK ((10 10 10) (19 23 17 21 25) 5 (0 0 0 0) (45 65 85))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiplexer: select-like pattern, read from first available channel
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_channel_multiplexer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((make-channel
           (lambda (cap)
             (let ((buf nil) (cnt 0))
               (list
                (cons :send (lambda (v)
                  (if (>= cnt cap) nil
                    (setq buf (append buf (list v)))
                    (setq cnt (1+ cnt)) t)))
                (cons :recv (lambda ()
                  (if (= cnt 0) (cons nil nil)
                    (let ((v (car buf)))
                      (setq buf (cdr buf))
                      (setq cnt (1- cnt))
                      (cons v t)))))
                (cons :empty-p (lambda () (= cnt 0)))
                (cons :size (lambda () cnt)))))))
  ;; Multiplexer: check channels in priority order, read from first non-empty
  (let ((mux-recv
         (lambda (channels)
           "Try to receive from channels in order. Returns (channel-index . value) or nil."
           (let ((idx 0) (found nil))
             (while (and channels (not found))
               (let* ((ch (car channels))
                      (item (funcall (cdr (assq :recv ch)))))
                 (if (cdr item)
                     (setq found (cons idx (car item)))
                   (setq idx (1+ idx))
                   (setq channels (cdr channels)))))
             found)))
        (mux-recv-all
         (lambda (channels max-reads)
           "Read up to MAX-READS items from channels in priority order."
           (let ((results nil) (reads 0))
             (while (< reads max-reads)
               (let ((idx 0) (found nil) (chs channels))
                 (while (and chs (not found))
                   (let ((item (funcall (cdr (assq :recv (car chs))))))
                     (if (cdr item)
                         (progn
                           (setq found t)
                           (setq results (cons (cons idx (car item)) results)))
                       (setq idx (1+ idx))
                       (setq chs (cdr chs)))))
                 (if found
                     (setq reads (1+ reads))
                   ;; All channels empty, stop
                   (setq reads max-reads))))
             (nreverse results)))))

    ;; Create 3 channels with different data
    (let* ((urgent (funcall make-channel 10))
           (normal (funcall make-channel 10))
           (low (funcall make-channel 10)))
      ;; Load channels
      (funcall (cdr (assq :send low)) 'low-1)
      (funcall (cdr (assq :send low)) 'low-2)
      (funcall (cdr (assq :send normal)) 'normal-1)
      (funcall (cdr (assq :send low)) 'low-3)
      (funcall (cdr (assq :send urgent)) 'urgent-1)
      (funcall (cdr (assq :send normal)) 'normal-2)

      (list
        ;; Priority order: urgent first
        (funcall mux-recv (list urgent normal low))
        ;; Next: normal (urgent now empty)
        (funcall mux-recv (list urgent normal low))
        ;; Read all remaining with priority
        (funcall mux-recv-all (list urgent normal low) 20)
        ;; All empty now
        (funcall mux-recv (list urgent normal low))

        ;; Load more and read all at once
        (progn
          (funcall (cdr (assq :send urgent)) 'u1)
          (funcall (cdr (assq :send urgent)) 'u2)
          (funcall (cdr (assq :send normal)) 'n1)
          (funcall (cdr (assq :send low)) 'l1)
          (funcall (cdr (assq :send low)) 'l2)
          (funcall (cdr (assq :send low)) 'l3)
          (funcall mux-recv-all (list urgent normal low) 100))

        ;; Sizes should all be 0
        (list (funcall (cdr (assq :size urgent)))
              (funcall (cdr (assq :size normal)))
              (funcall (cdr (assq :size low)))))))"#;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
