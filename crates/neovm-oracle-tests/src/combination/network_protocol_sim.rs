//! Oracle parity tests simulating network protocol concepts in Elisp:
//! packet routing with hop count, TCP-like reliable delivery (sequence
//! numbers, ACK, retransmission), sliding window flow control,
//! congestion control (AIMD), DNS resolution simulation, ARP table
//! management, and NAT translation table.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Packet routing with hop count and TTL
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_network_protocol_packet_routing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate packet routing through a network of routers.
    // Each router has a routing table mapping destination prefixes to next-hop.
    // Packets carry TTL that decrements at each hop; dropped if TTL=0.
    let form = r#"(progn
  ;; Routing table: alist of (router . ((dest-prefix . next-hop) ...))
  ;; Packet: (src dst ttl payload trace)

  (fset 'neovm--nps-lookup-route
    (lambda (routing-table router dest)
      ;; Find longest-prefix match in router's table
      (let ((table (cdr (assq router routing-table)))
            (best-hop nil)
            (best-len 0))
        (dolist (entry table)
          (let ((prefix (car entry))
                (next (cdr entry)))
            (when (and (string-prefix-p prefix dest)
                       (> (length prefix) best-len))
              (setq best-hop next
                    best-len (length prefix)))))
        best-hop)))

  (fset 'neovm--nps-route-packet
    (lambda (routing-table packet)
      ;; Route a packet through the network until it reaches destination or TTL expires.
      ;; Returns the final packet with trace of hops.
      (let* ((src (nth 0 packet))
             (dst (nth 1 packet))
             (ttl (nth 2 packet))
             (payload (nth 3 packet))
             (trace (nth 4 packet))
             (current-router src)
             (max-hops 20)
             (hops 0))
        (while (and (> ttl 0)
                    (not (string= (symbol-name current-router) (symbol-name dst)))
                    (< hops max-hops))
          (let ((next-hop (funcall 'neovm--nps-lookup-route
                                   routing-table current-router
                                   (symbol-name dst))))
            (if next-hop
                (progn
                  (setq trace (append trace (list current-router))
                        current-router next-hop
                        ttl (1- ttl)
                        hops (1+ hops)))
              ;; No route found: drop
              (setq ttl 0))))
        (when (eq current-router dst)
          (setq trace (append trace (list current-router))))
        (list current-router dst ttl payload trace
              (eq current-router dst)))))

  (unwind-protect
      (let* ((routes '((R1 . (("10.0" . R2) ("10.1" . R3) ("" . R2)))
                        (R2 . (("10.0.1" . R4) ("10.1" . R3) ("" . R1)))
                        (R3 . (("10.0" . R2) ("10.1.1" . R5) ("" . R1)))
                        (R4 . (("" . R2)))
                        (R5 . (("" . R3)))))
             ;; Packet from R1 to R5 with TTL=10
             (p1 (funcall 'neovm--nps-route-packet routes
                          (list 'R1 'R5 10 "data1" nil)))
             ;; Packet from R1 to R4 with TTL=10
             (p2 (funcall 'neovm--nps-route-packet routes
                          (list 'R1 'R4 10 "data2" nil)))
             ;; Packet with TTL=1 (should be dropped after one hop)
             (p3 (funcall 'neovm--nps-route-packet routes
                          (list 'R1 'R5 1 "data3" nil)))
             ;; Packet already at destination
             (p4 (funcall 'neovm--nps-route-packet routes
                          (list 'R5 'R5 5 "data4" nil))))
        (list p1 p2 p3 p4
              ;; Verify: p1 reached R5, p3 did not
              (nth 5 p1) (nth 5 p3)))
    (fmakunbound 'neovm--nps-lookup-route)
    (fmakunbound 'neovm--nps-route-packet)))"#;
    let expect = expect_test::expect![[
        r#""OK ((R1 R5 0 \"data1\" nil nil) (R1 R4 0 \"data2\" nil nil) (R1 R5 0 \"data3\" nil nil) (R5 R5 5 \"data4\" (R5) t) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// TCP-like reliable delivery: sequence numbers, ACK, retransmission
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_network_protocol_tcp_reliable_delivery() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a TCP sender with sequence numbers.
    // Sender transmits segments; receiver ACKs received segments.
    // Sender retransmits unacknowledged segments after timeout.
    let form = r#"(progn
  (fset 'neovm--nps-tcp-send
    (lambda (data segment-size)
      ;; Split data into segments with sequence numbers.
      ;; Returns list of (seq-num payload)
      (let ((segments nil)
            (seq 0)
            (i 0)
            (len (length data)))
        (while (< i len)
          (let ((end (min (+ i segment-size) len)))
            (push (list seq (substring data i end)) segments)
            (setq seq (+ seq (- end i))
                  i end)))
        (nreverse segments))))

  (fset 'neovm--nps-tcp-receive
    (lambda (segments drop-list)
      ;; Simulate receiver that drops segments in drop-list (by seq number).
      ;; Returns (ack-list received-buffer) where ack-list is seq-nums ACKed.
      (let ((received (make-hash-table))
            (acks nil)
            (next-expected 0))
        (dolist (seg segments)
          (let ((seq (car seg))
                (payload (cadr seg)))
            (unless (memq seq drop-list)
              (puthash seq payload received)
              (push seq acks))))
        ;; Compute contiguous received data
        (setq acks (sort acks #'<))
        ;; Find the highest contiguous ACK
        (let ((contiguous-ack 0))
          (dolist (seg segments)
            (let ((seq (car seg))
                  (payload (cadr seg)))
              (when (gethash seq received)
                (when (= seq contiguous-ack)
                  (setq contiguous-ack (+ seq (length (gethash seq received))))))))
          (list acks contiguous-ack
                (hash-table-count received))))))

  (fset 'neovm--nps-tcp-retransmit
    (lambda (segments acked-seqs)
      ;; Find segments not yet acknowledged for retransmission.
      (let ((unacked nil))
        (dolist (seg segments)
          (unless (memq (car seg) acked-seqs)
            (push seg unacked)))
        (nreverse unacked))))

  (unwind-protect
      (let* ((data "Hello, this is a TCP simulation test message!")
             (segments (funcall 'neovm--nps-tcp-send data 10))
             ;; Normal delivery: no drops
             (rx1 (funcall 'neovm--nps-tcp-receive segments nil))
             ;; Drop segment with seq=10
             (rx2 (funcall 'neovm--nps-tcp-receive segments '(10)))
             ;; Retransmit unacked
             (retx (funcall 'neovm--nps-tcp-retransmit segments (car rx2)))
             ;; Drop multiple segments
             (rx3 (funcall 'neovm--nps-tcp-receive segments '(10 30)))
             (retx3 (funcall 'neovm--nps-tcp-retransmit segments (car rx3))))
        (list
         (length segments)
         (mapcar #'car segments)  ;; seq numbers
         ;; Normal: all acked
         (car rx1) (nth 1 rx1)
         ;; With drop: partial ack
         (car rx2) (nth 1 rx2)
         ;; Retransmit list
         (length retx) (mapcar #'car retx)
         ;; Multi-drop
         (length retx3) (mapcar #'car retx3)))
    (fmakunbound 'neovm--nps-tcp-send)
    (fmakunbound 'neovm--nps-tcp-receive)
    (fmakunbound 'neovm--nps-tcp-retransmit)))"#;
    let expect = expect_test::expect![[
        r#""OK (5 (0 10 20 30 40) (0 10 20 30 40) 45 (0 20 30 40) 10 1 (10) 2 (10 30))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sliding window flow control
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_network_protocol_sliding_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a sliding window protocol with window size W.
    // Sender can have at most W unacknowledged packets in flight.
    // Returns the transmission log showing window state at each step.
    let form = r#"(progn
  (fset 'neovm--nps-sliding-window
    (lambda (num-packets window-size ack-delays)
      ;; ack-delays: list of (packet-id . delay) indicating when ACK arrives.
      ;; delay=0 means immediate, delay=N means after N send cycles.
      ;; Returns: transmission log entries (time action details window-base window-next)
      (let ((log nil)
            (time 0)
            (base 0)           ;; oldest unacknowledged
            (next-seq 0)       ;; next to send
            (pending-acks nil) ;; (arrival-time . packet-id)
            (max-time 100))
        (while (and (< base num-packets) (< time max-time))
          ;; Check for arriving ACKs
          (let ((new-pending nil))
            (dolist (pa pending-acks)
              (if (<= (car pa) time)
                  (progn
                    (push (list time 'ACK (cdr pa) base next-seq) log)
                    ;; Advance base if this ACK is for the oldest
                    (when (= (cdr pa) base)
                      (setq base (1+ base))
                      ;; Slide past consecutive acked
                      (while (and (< base next-seq)
                                  (let ((found nil))
                                    (dolist (l log)
                                      (when (and (eq (nth 1 l) 'ACK)
                                                 (= (nth 2 l) base))
                                        (setq found t)))
                                    found))
                        (setq base (1+ base)))))
                (push pa new-pending)))
            (setq pending-acks new-pending))
          ;; Send packets within window
          (while (and (< next-seq num-packets)
                      (< (- next-seq base) window-size))
            (push (list time 'SEND next-seq base next-seq) log)
            ;; Schedule ACK
            (let ((delay (or (cdr (assq next-seq ack-delays)) 1)))
              (push (cons (+ time delay) next-seq) pending-acks))
            (setq next-seq (1+ next-seq)))
          (setq time (1+ time)))
        (nreverse log))))

  (unwind-protect
      (let* (;; 6 packets, window=3, all ACKs arrive after 1 cycle
             (log1 (funcall 'neovm--nps-sliding-window 6 3 nil))
             ;; 5 packets, window=2, packet 1 has delayed ACK
             (log2 (funcall 'neovm--nps-sliding-window 5 2 '((1 . 3))))
             ;; Count sends and acks
             (sends1 (length (seq-filter (lambda (e) (eq (nth 1 e) 'SEND)) log1)))
             (acks1 (length (seq-filter (lambda (e) (eq (nth 1 e) 'ACK)) log1))))
        (list (length log1) sends1 acks1
              (length log2)
              ;; First few entries of each log
              (seq-take log1 6)
              (seq-take log2 6)))
    (fmakunbound 'neovm--nps-sliding-window)))"#;
    let expect = expect_test::expect![[
        r#""OK (12 6 6 10 ((0 SEND 0 0 0) (0 SEND 1 0 1) (0 SEND 2 0 2) (1 ACK 2 0 3) (1 ACK 1 0 3) (1 ACK 0 0 3)) ((0 SEND 0 0 0) (0 SEND 1 0 1) (1 ACK 0 0 2) (1 SEND 2 1 2) (2 ACK 2 1 3) (3 ACK 1 1 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Congestion control: AIMD (Additive Increase, Multiplicative Decrease)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_network_protocol_congestion_aimd() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate AIMD congestion control:
    // - On successful transmission: window += 1 (additive increase)
    // - On congestion/loss event: window = window / 2 (multiplicative decrease)
    // - Window has minimum of 1.
    let form = r#"(progn
  (fset 'neovm--nps-aimd-simulate
    (lambda (events initial-window)
      ;; events: list of 'success or 'loss
      ;; Returns list of (event window-before window-after) for each step
      (let ((window initial-window)
            (log nil))
        (dolist (event events)
          (let ((before window))
            (cond
             ((eq event 'success)
              (setq window (1+ window)))
             ((eq event 'loss)
              (setq window (max 1 (/ window 2)))))
            (push (list event before window) log)))
        (nreverse log))))

  (fset 'neovm--nps-aimd-with-slow-start
    (lambda (events initial-window ssthresh)
      ;; Slow start: double window until ssthresh, then linear increase.
      ;; On loss: ssthresh = window/2, window = 1 (TCP Tahoe style).
      (let ((window initial-window)
            (ss ssthresh)
            (log nil))
        (dolist (event events)
          (let ((before window)
                (phase (if (< window ss) 'slow-start 'congestion-avoidance)))
            (cond
             ((eq event 'success)
              (if (< window ss)
                  (setq window (* window 2))  ;; exponential in slow start
                (setq window (1+ window))))   ;; linear in CA
             ((eq event 'loss)
              (setq ss (max 1 (/ window 2))
                    window 1)))
            (push (list event phase before window ss) log)))
        (nreverse log))))

  (unwind-protect
      (let* (;; Simple AIMD: 5 successes then a loss then 3 successes
             (events1 '(success success success success success
                        loss success success success))
             (log1 (funcall 'neovm--nps-aimd-simulate events1 1))
             ;; Slow start with ssthresh=16
             (events2 '(success success success success success
                        loss success success success success success))
             (log2 (funcall 'neovm--nps-aimd-with-slow-start events2 1 16))
             ;; Multiple losses
             (events3 '(success success success loss loss success success))
             (log3 (funcall 'neovm--nps-aimd-simulate events3 10)))
        (list log1 log2 log3
              ;; Final window sizes
              (nth 2 (car (last log1)))
              (nth 3 (car (last log2)))
              (nth 2 (car (last log3)))))
    (fmakunbound 'neovm--nps-aimd-simulate)
    (fmakunbound 'neovm--nps-aimd-with-slow-start)))"#;
    let expect = expect_test::expect![[
        r#""OK (((success 1 2) (success 2 3) (success 3 4) (success 4 5) (success 5 6) (loss 6 3) (success 3 4) (success 4 5) (success 5 6)) ((success slow-start 1 2 16) (success slow-start 2 4 16) (success slow-start 4 8 16) (success slow-start 8 16 16) (success congestion-avoidance 16 17 16) (loss congestion-avoidance 17 1 8) (success slow-start 1 2 8) (success slow-start 2 4 8) (success slow-start 4 8 8) (success congestion-avoidance 8 9 8) (success congestion-avoidance 9 10 8)) ((success 10 11) (success 11 12) (success 12 13) (loss 13 6) (loss 6 3) (success 3 4) (success 4 5)) 6 10 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// DNS resolution simulation (recursive/iterative)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_network_protocol_dns_resolution() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate DNS resolution with a hierarchy of name servers.
    // Root -> TLD -> Authoritative -> final IP.
    // Both recursive and iterative query modes.
    let form = r#"(progn
  ;; DNS database: alist of (server-name . ((domain . (type . value)) ...))
  ;; type: 'A for address, 'NS for name-server referral

  (fset 'neovm--nps-dns-query
    (lambda (db server domain query-log)
      ;; Iterative: return answer or referral
      (let ((records (cdr (assoc server db)))
            (answer nil))
        (dolist (rec records)
          (when (string= (car rec) domain)
            (setq answer (cdr rec))))
        (let ((new-log (append query-log (list (list 'query server domain answer)))))
          (if answer
              (if (eq (car answer) 'A)
                  (list 'resolved (cdr answer) new-log)
                ;; NS referral: follow the chain
                (list 'referral (cdr answer) new-log))
            (list 'nxdomain nil new-log))))))

  (fset 'neovm--nps-dns-resolve
    (lambda (db domain)
      ;; Iteratively resolve by following referrals from root
      (let ((server "root")
            (log nil)
            (max-steps 10)
            (steps 0)
            (result nil))
        (while (and (not result) (< steps max-steps))
          (let ((resp (funcall 'neovm--nps-dns-query db server domain log)))
            (setq log (nth 2 resp))
            (cond
             ((eq (car resp) 'resolved)
              (setq result (list 'ok (nth 1 resp) log)))
             ((eq (car resp) 'referral)
              (setq server (nth 1 resp)))
             (t
              (setq result (list 'nxdomain nil log)))))
          (setq steps (1+ steps)))
        (or result (list 'timeout nil log)))))

  (unwind-protect
      (let* ((dns-db '(("root" . (("example.com" . (NS . "tld-com"))
                                   ("example.org" . (NS . "tld-org"))
                                   ("test.net" . (NS . "tld-net"))))
                        ("tld-com" . (("example.com" . (NS . "auth-example"))))
                        ("tld-org" . (("example.org" . (A . "93.184.216.34"))))
                        ("tld-net" . (("test.net" . (NS . "auth-test"))))
                        ("auth-example" . (("example.com" . (A . "93.184.216.34"))))
                        ("auth-test" . (("test.net" . (A . "198.51.100.1"))))))
             ;; Resolve example.com: root -> tld-com -> auth-example -> IP
             (r1 (funcall 'neovm--nps-dns-resolve dns-db "example.com"))
             ;; Resolve example.org: root -> tld-org -> IP (shorter chain)
             (r2 (funcall 'neovm--nps-dns-resolve dns-db "example.org"))
             ;; Resolve test.net
             (r3 (funcall 'neovm--nps-dns-resolve dns-db "test.net"))
             ;; Resolve nonexistent domain
             (r4 (funcall 'neovm--nps-dns-resolve dns-db "nosuch.com")))
        (list
         (car r1) (nth 1 r1) (length (nth 2 r1))
         (car r2) (nth 1 r2) (length (nth 2 r2))
         (car r3) (nth 1 r3) (length (nth 2 r3))
         (car r4)))
    (fmakunbound 'neovm--nps-dns-query)
    (fmakunbound 'neovm--nps-dns-resolve)))"#;
    let expect = expect_test::expect![[
        r#""OK (ok \"93.184.216.34\" 3 ok \"93.184.216.34\" 2 ok \"198.51.100.1\" 3 nxdomain)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// ARP table management
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_network_protocol_arp_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate ARP (Address Resolution Protocol) table management:
    // - Add/update entries mapping IP -> MAC with timestamps
    // - Lookup IP to get MAC
    // - Age out stale entries beyond a TTL
    // - Handle ARP request/reply sequences
    let form = r#"(progn
  (fset 'neovm--nps-arp-create
    (lambda ()
      ;; ARP table: hash-table of IP -> (MAC . timestamp)
      (make-hash-table :test 'equal)))

  (fset 'neovm--nps-arp-update
    (lambda (table ip mac timestamp)
      (puthash ip (cons mac timestamp) table)
      table))

  (fset 'neovm--nps-arp-lookup
    (lambda (table ip)
      (let ((entry (gethash ip table)))
        (when entry (car entry)))))

  (fset 'neovm--nps-arp-age-out
    (lambda (table current-time ttl)
      ;; Remove entries older than ttl seconds
      (let ((to-remove nil))
        (maphash (lambda (ip entry)
                   (when (> (- current-time (cdr entry)) ttl)
                     (push ip to-remove)))
                 table)
        (dolist (ip to-remove)
          (remhash ip table))
        (length to-remove))))

  (fset 'neovm--nps-arp-entries
    (lambda (table)
      ;; Return sorted list of (ip . mac) pairs
      (let ((entries nil))
        (maphash (lambda (ip entry)
                   (push (cons ip (car entry)) entries))
                 table)
        (sort entries (lambda (a b) (string< (car a) (car b)))))))

  (unwind-protect
      (let* ((tbl (funcall 'neovm--nps-arp-create))
             ;; Add entries at different times
             (_ (funcall 'neovm--nps-arp-update tbl "10.0.0.1" "AA:BB:CC:DD:EE:01" 100))
             (_ (funcall 'neovm--nps-arp-update tbl "10.0.0.2" "AA:BB:CC:DD:EE:02" 105))
             (_ (funcall 'neovm--nps-arp-update tbl "10.0.0.3" "AA:BB:CC:DD:EE:03" 110))
             (_ (funcall 'neovm--nps-arp-update tbl "10.0.0.4" "AA:BB:CC:DD:EE:04" 120))
             ;; Lookup
             (mac1 (funcall 'neovm--nps-arp-lookup tbl "10.0.0.1"))
             (mac3 (funcall 'neovm--nps-arp-lookup tbl "10.0.0.3"))
             (mac-miss (funcall 'neovm--nps-arp-lookup tbl "10.0.0.99"))
             ;; All entries before aging
             (before-age (funcall 'neovm--nps-arp-entries tbl))
             ;; Age out entries older than 15 seconds at time=120
             (removed (funcall 'neovm--nps-arp-age-out tbl 120 15))
             (after-age (funcall 'neovm--nps-arp-entries tbl))
             ;; Update existing entry (refresh)
             (_ (funcall 'neovm--nps-arp-update tbl "10.0.0.3" "FF:FF:FF:FF:FF:03" 125))
             (mac3-new (funcall 'neovm--nps-arp-lookup tbl "10.0.0.3"))
             (final (funcall 'neovm--nps-arp-entries tbl)))
        (list mac1 mac3 mac-miss
              (length before-age) before-age
              removed
              (length after-age) after-age
              mac3-new final))
    (fmakunbound 'neovm--nps-arp-create)
    (fmakunbound 'neovm--nps-arp-update)
    (fmakunbound 'neovm--nps-arp-lookup)
    (fmakunbound 'neovm--nps-arp-age-out)
    (fmakunbound 'neovm--nps-arp-entries)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"AA:BB:CC:DD:EE:01\" \"AA:BB:CC:DD:EE:03\" nil 4 ((\"10.0.0.1\" . \"AA:BB:CC:DD:EE:01\") (\"10.0.0.2\" . \"AA:BB:CC:DD:EE:02\") (\"10.0.0.3\" . \"AA:BB:CC:DD:EE:03\") (\"10.0.0.4\" . \"AA:BB:CC:DD:EE:04\")) 1 3 ((\"10.0.0.2\" . \"AA:BB:CC:DD:EE:02\") (\"10.0.0.3\" . \"AA:BB:CC:DD:EE:03\") (\"10.0.0.4\" . \"AA:BB:CC:DD:EE:04\")) \"FF:FF:FF:FF:FF:03\" ((\"10.0.0.2\" . \"AA:BB:CC:DD:EE:02\") (\"10.0.0.3\" . \"FF:FF:FF:FF:FF:03\") (\"10.0.0.4\" . \"AA:BB:CC:DD:EE:04\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// NAT translation table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_network_protocol_nat_translation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate Network Address Translation (NAT):
    // Internal hosts have private IPs. The NAT gateway maps
    // (internal-ip, internal-port) -> (public-ip, mapped-port).
    // Outbound packets get source rewritten; inbound packets get dest rewritten.
    let form = r#"(progn
  (fset 'neovm--nps-nat-create
    (lambda (public-ip)
      ;; NAT state: (public-ip next-port outbound-table inbound-table)
      ;; outbound: hash "internal-ip:port" -> mapped-port
      ;; inbound: hash mapped-port -> "internal-ip:port"
      (list public-ip 10000
            (make-hash-table :test 'equal)
            (make-hash-table :test 'eql))))

  (fset 'neovm--nps-nat-outbound
    (lambda (nat-state src-ip src-port dst-ip dst-port)
      ;; Translate outbound packet. Returns (nat-state translated-packet).
      (let* ((pub-ip (nth 0 nat-state))
             (next-port (nth 1 nat-state))
             (out-tbl (nth 2 nat-state))
             (in-tbl (nth 3 nat-state))
             (key (format "%s:%d" src-ip src-port))
             (mapped (gethash key out-tbl)))
        (unless mapped
          (setq mapped next-port)
          (puthash key mapped out-tbl)
          (puthash mapped key in-tbl)
          (setcar (nthcdr 1 nat-state) (1+ next-port)))
        ;; Translated packet: (new-src-ip new-src-port dst-ip dst-port)
        (list nat-state (list pub-ip mapped dst-ip dst-port)))))

  (fset 'neovm--nps-nat-inbound
    (lambda (nat-state dst-port src-ip src-port)
      ;; Translate inbound packet directed to public-ip:dst-port.
      ;; Returns translated packet or nil if no mapping.
      (let* ((in-tbl (nth 3 nat-state))
             (internal (gethash dst-port in-tbl)))
        (if internal
            (let* ((parts (split-string internal ":"))
                   (int-ip (nth 0 parts))
                   (int-port (string-to-number (nth 1 parts))))
              (list src-ip src-port int-ip int-port))
          nil))))

  (fset 'neovm--nps-nat-table-entries
    (lambda (nat-state)
      (let ((entries nil))
        (maphash (lambda (k v)
                   (push (cons k v) entries))
                 (nth 2 nat-state))
        (sort entries (lambda (a b) (string< (car a) (car b)))))))

  (unwind-protect
      (let* ((nat (funcall 'neovm--nps-nat-create "203.0.113.1"))
             ;; Host 192.168.1.10:5000 -> 8.8.8.8:53 (DNS query)
             (r1 (funcall 'neovm--nps-nat-outbound nat "192.168.1.10" 5000 "8.8.8.8" 53))
             (_ (setq nat (car r1)))
             (pkt1 (cadr r1))
             ;; Same host different port
             (r2 (funcall 'neovm--nps-nat-outbound nat "192.168.1.10" 5001 "8.8.8.8" 53))
             (_ (setq nat (car r2)))
             (pkt2 (cadr r2))
             ;; Different host
             (r3 (funcall 'neovm--nps-nat-outbound nat "192.168.1.20" 3000 "1.1.1.1" 80))
             (_ (setq nat (car r3)))
             (pkt3 (cadr r3))
             ;; Repeat same mapping (should reuse port)
             (r4 (funcall 'neovm--nps-nat-outbound nat "192.168.1.10" 5000 "8.8.4.4" 53))
             (_ (setq nat (car r4)))
             (pkt4 (cadr r4))
             ;; Inbound: reply comes back to mapped port
             (in1 (funcall 'neovm--nps-nat-inbound nat (nth 1 pkt1) "8.8.8.8" 53))
             ;; Inbound to unmapped port
             (in-miss (funcall 'neovm--nps-nat-inbound nat 99999 "8.8.8.8" 53))
             ;; Translation table
             (tbl (funcall 'neovm--nps-nat-table-entries nat)))
        (list pkt1 pkt2 pkt3 pkt4
              ;; pkt1 and pkt4 should have same mapped port (same internal endpoint)
              (= (nth 1 pkt1) (nth 1 pkt4))
              ;; pkt1 and pkt2 should have different mapped ports
              (/= (nth 1 pkt1) (nth 1 pkt2))
              in1 in-miss
              (length tbl) tbl))
    (fmakunbound 'neovm--nps-nat-create)
    (fmakunbound 'neovm--nps-nat-outbound)
    (fmakunbound 'neovm--nps-nat-inbound)
    (fmakunbound 'neovm--nps-nat-table-entries)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"203.0.113.1\" 10000 \"8.8.8.8\" 53) (\"203.0.113.1\" 10001 \"8.8.8.8\" 53) (\"203.0.113.1\" 10002 \"1.1.1.1\" 80) (\"203.0.113.1\" 10000 \"8.8.4.4\" 53) t t (\"8.8.8.8\" 53 \"192.168.1.10\" 5000) nil 3 ((\"192.168.1.10:5000\" . 10000) (\"192.168.1.10:5001\" . 10001) (\"192.168.1.20:3000\" . 10002)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
