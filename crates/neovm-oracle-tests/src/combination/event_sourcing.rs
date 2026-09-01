//! Oracle parity tests for event sourcing patterns in Elisp:
//! append-only event store, aggregate reconstruction, event projection,
//! snapshot with replay, event versioning/migration, saga/process manager,
//! and command validation before event creation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Event store: append-only log with sequence numbers and timestamps
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_event_sourcing_store() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--test-es-store nil)
  (defvar neovm--test-es-seq 0)
  (unwind-protect
      (let ((append-event
             (lambda (aggregate-id event-type payload)
               (setq neovm--test-es-seq (1+ neovm--test-es-seq))
               (let ((event (list :seq neovm--test-es-seq
                                  :aggregate-id aggregate-id
                                  :type event-type
                                  :payload payload
                                  :timestamp neovm--test-es-seq)))
                 (setq neovm--test-es-store
                       (append neovm--test-es-store (list event)))
                 event)))
            (get-events-for
             (lambda (aggregate-id)
               (let ((result nil))
                 (dolist (e neovm--test-es-store)
                   (when (equal (plist-get e :aggregate-id) aggregate-id)
                     (setq result (cons e result))))
                 (nreverse result))))
            (get-events-by-type
             (lambda (event-type)
               (let ((result nil))
                 (dolist (e neovm--test-es-store)
                   (when (eq (plist-get e :type) event-type)
                     (setq result (cons e result))))
                 (nreverse result))))
            (get-events-since
             (lambda (seq-num)
               (let ((result nil))
                 (dolist (e neovm--test-es-store)
                   (when (> (plist-get e :seq) seq-num)
                     (setq result (cons e result))))
                 (nreverse result)))))
        ;; Populate store
        (setq neovm--test-es-store nil)
        (setq neovm--test-es-seq 0)
        (funcall append-event "order-1" 'order-created '(:product "widget" :qty 5))
        (funcall append-event "order-1" 'item-added '(:product "gadget" :qty 2))
        (funcall append-event "order-2" 'order-created '(:product "doohickey" :qty 1))
        (funcall append-event "order-1" 'order-confirmed '(:confirmed t))
        (funcall append-event "order-2" 'item-added '(:product "thingamajig" :qty 3))
        (funcall append-event "order-1" 'item-shipped '(:tracking "TRK001"))
        (funcall append-event "order-2" 'order-confirmed '(:confirmed t))
        (list
         :total-events (length neovm--test-es-store)
         :order-1-events (length (funcall get-events-for "order-1"))
         :order-2-events (length (funcall get-events-for "order-2"))
         :confirmed-events (length (funcall get-events-by-type 'order-confirmed))
         :events-since-4 (mapcar (lambda (e) (plist-get e :seq))
                                 (funcall get-events-since 4))
         :order-1-types (mapcar (lambda (e) (plist-get e :type))
                                (funcall get-events-for "order-1"))))
    (makunbound 'neovm--test-es-store)
    (makunbound 'neovm--test-es-seq)))"#;
    let expect = expect_test::expect![[
        r#""OK (:total-events 7 :order-1-events 4 :order-2-events 3 :confirmed-events 2 :events-since-4 (5 6 7) :order-1-types (order-created item-added order-confirmed item-shipped))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Aggregate reconstruction: rebuild state from events
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_event_sourcing_aggregate_reconstruction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((events '((:type account-opened :payload (:name "Alice" :balance 0))
                          (:type deposited :payload (:amount 500))
                          (:type withdrawn :payload (:amount 100))
                          (:type deposited :payload (:amount 250))
                          (:type withdrawn :payload (:amount 75))
                          (:type interest-applied :payload (:rate 5))
                          (:type withdrawn :payload (:amount 200))))
                  (apply-event
                   (lambda (state event)
                     (let ((type (plist-get event :type))
                           (payload (plist-get event :payload)))
                       (cond
                        ((eq type 'account-opened)
                         (list :name (plist-get payload :name)
                               :balance (plist-get payload :balance)
                               :transactions 0
                               :history nil))
                        ((eq type 'deposited)
                         (let ((amt (plist-get payload :amount)))
                           (plist-put
                            (plist-put
                             (plist-put (copy-sequence state)
                                        :balance (+ (plist-get state :balance) amt))
                             :transactions (1+ (plist-get state :transactions)))
                            :history (append (plist-get state :history)
                                             (list (list 'deposit amt))))))
                        ((eq type 'withdrawn)
                         (let ((amt (plist-get payload :amount)))
                           (plist-put
                            (plist-put
                             (plist-put (copy-sequence state)
                                        :balance (- (plist-get state :balance) amt))
                             :transactions (1+ (plist-get state :transactions)))
                            :history (append (plist-get state :history)
                                             (list (list 'withdrawal amt))))))
                        ((eq type 'interest-applied)
                         (let* ((rate (plist-get payload :rate))
                                (bal (plist-get state :balance))
                                (interest (/ (* bal rate) 100)))
                           (plist-put
                            (plist-put (copy-sequence state)
                                       :balance (+ bal interest))
                            :history (append (plist-get state :history)
                                             (list (list 'interest interest))))))
                        (t state))))))
              ;; Reconstruct state by folding events
              (let ((state nil))
                (dolist (event events)
                  (setq state (funcall apply-event state event)))
                (list :final-balance (plist-get state :balance)
                      :transactions (plist-get state :transactions)
                      :name (plist-get state :name)
                      :history (plist-get state :history))))"#;
    let expect = expect_test::expect![[
        r#""OK (:final-balance 403 :transactions 5 :name \"Alice\" :history ((deposit 500) (withdrawal 100) (deposit 250) (withdrawal 75) (interest 28) (withdrawal 200)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Event projection: build materialized views from event stream
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_event_sourcing_projection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((events '((:type user-registered :data (:user "alice" :email "a@x.com"))
                          (:type user-registered :data (:user "bob" :email "b@x.com"))
                          (:type order-placed :data (:user "alice" :item "book" :price 20))
                          (:type order-placed :data (:user "bob" :item "pen" :price 5))
                          (:type order-placed :data (:user "alice" :item "lamp" :price 45))
                          (:type user-registered :data (:user "carol" :email "c@x.com"))
                          (:type order-placed :data (:user "carol" :item "desk" :price 150))
                          (:type order-placed :data (:user "bob" :item "notebook" :price 12))
                          (:type order-placed :data (:user "alice" :item "mug" :price 8))))
                  ;; Projection 1: user directory (name -> email)
                  (user-dir (make-hash-table :test 'equal))
                  ;; Projection 2: total spend per user
                  (spend (make-hash-table :test 'equal))
                  ;; Projection 3: order count per user
                  (order-count (make-hash-table :test 'equal)))
              ;; Project events into views
              (dolist (e events)
                (let ((type (plist-get e :type))
                      (data (plist-get e :data)))
                  (cond
                   ((eq type 'user-registered)
                    (puthash (plist-get data :user)
                             (plist-get data :email) user-dir))
                   ((eq type 'order-placed)
                    (let ((user (plist-get data :user))
                          (price (plist-get data :price)))
                      (puthash user (+ (gethash user spend 0) price) spend)
                      (puthash user (1+ (gethash user order-count 0)) order-count))))))
              ;; Query projections
              (let ((users nil))
                (maphash (lambda (k v)
                           (setq users (cons (list :user k
                                                   :email v
                                                   :total-spend (gethash k spend 0)
                                                   :order-count (gethash k order-count 0))
                                             users)))
                         user-dir)
                (sort users (lambda (a b)
                              (string< (plist-get a :user)
                                       (plist-get b :user))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((:user \"alice\" :email \"a@x.com\" :total-spend 73 :order-count 3) (:user \"bob\" :email \"b@x.com\" :total-spend 17 :order-count 2) (:user \"carol\" :email \"c@x.com\" :total-spend 150 :order-count 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Snapshot + event replay: reconstruct from snapshot + newer events
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_event_sourcing_snapshot_replay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* (;; All events in order with sequence numbers
                         (all-events '((:seq 1 :type add-item :data (:item "a" :qty 10))
                                       (:seq 2 :type add-item :data (:item "b" :qty 5))
                                       (:seq 3 :type remove-item :data (:item "a" :qty 3))
                                       (:seq 4 :type add-item :data (:item "c" :qty 8))
                                       (:seq 5 :type remove-item :data (:item "b" :qty 2))
                                       (:seq 6 :type add-item :data (:item "a" :qty 7))
                                       (:seq 7 :type remove-item :data (:item "c" :qty 5))))
                         ;; Snapshot taken at seq 3
                         (snapshot '(:seq 3 :state (("a" . 7) ("b" . 5))))
                         ;; Apply a single event to inventory state
                         (apply-inv-event
                          (lambda (state event)
                            (let* ((type (plist-get event :type))
                                   (data (plist-get event :data))
                                   (item (plist-get data :item))
                                   (qty (plist-get data :qty))
                                   (entry (assoc item state)))
                              (cond
                               ((eq type 'add-item)
                                (if entry
                                    (progn (setcdr entry (+ (cdr entry) qty)) state)
                                  (cons (cons item qty) state)))
                               ((eq type 'remove-item)
                                (if entry
                                    (progn (setcdr entry (max 0 (- (cdr entry) qty))) state)
                                  state))
                               (t state)))))
                         ;; Method 1: replay ALL events from scratch
                         (full-replay
                          (let ((state nil))
                            (dolist (e all-events)
                              (setq state (funcall apply-inv-event state e)))
                            state))
                         ;; Method 2: start from snapshot, replay only events after snapshot seq
                         (snapshot-seq (plist-get snapshot :seq))
                         (partial-replay
                          (let ((state (mapcar (lambda (p) (cons (car p) (cdr p)))
                                              (plist-get snapshot :state))))
                            (dolist (e all-events)
                              (when (> (plist-get e :seq) snapshot-seq)
                                (setq state (funcall apply-inv-event state e))))
                            state)))
                    ;; Both methods should produce same result
                    (let ((sort-fn (lambda (a b) (string< (car a) (car b)))))
                      (list :full-replay (sort (copy-sequence full-replay) sort-fn)
                            :snapshot-replay (sort (copy-sequence partial-replay) sort-fn)
                            :match (equal (sort (copy-sequence full-replay) sort-fn)
                                          (sort (copy-sequence partial-replay) sort-fn)))))"#;
    let expect = expect_test::expect![[
        r#""OK (:full-replay ((\"a\" . 14) (\"b\" . 3) (\"c\" . 3)) :snapshot-replay ((\"a\" . 14) (\"b\" . 3) (\"c\" . 3)) :match t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Event versioning/migration: upgrade v1 events to v2 schema
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_event_sourcing_versioning() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* (;; Mix of v1 and v2 events
                         (raw-events
                          '((:version 1 :type purchase :amount 100 :customer "alice")
                            (:version 1 :type purchase :amount 50 :customer "bob")
                            (:version 2 :type purchase :data (:amount 75 :customer "carol"
                                                              :currency "USD" :tax 6))
                            (:version 1 :type refund :amount 30 :customer "alice")
                            (:version 2 :type purchase :data (:amount 200 :customer "alice"
                                                              :currency "EUR" :tax 38))
                            (:version 1 :type purchase :amount 120 :customer "dave")
                            (:version 2 :type refund :data (:amount 50 :customer "bob"
                                                            :currency "USD" :tax 4))))
                         ;; Migrate v1 -> v2
                         (migrate-event
                          (lambda (event)
                            (if (= (plist-get event :version) 2)
                                event  ;; already v2
                              ;; Upgrade v1 to v2: wrap fields into :data, add defaults
                              (list :version 2
                                    :type (plist-get event :type)
                                    :data (list :amount (plist-get event :amount)
                                                :customer (plist-get event :customer)
                                                :currency "USD"
                                                :tax 0)))))
                         ;; Migrate all events
                         (migrated (mapcar migrate-event raw-events))
                         ;; Process all v2 events: compute per-customer balance
                         (balances (make-hash-table :test 'equal)))
                    (dolist (e migrated)
                      (let* ((data (plist-get e :data))
                             (cust (plist-get data :customer))
                             (amount (plist-get data :amount))
                             (tax (plist-get data :tax))
                             (type (plist-get e :type))
                             (total (+ amount tax))
                             (current (gethash cust balances 0)))
                        (puthash cust
                                 (if (eq type 'purchase)
                                     (+ current total)
                                   (- current total))
                                 balances)))
                    ;; Collect results sorted
                    (let ((result nil))
                      (maphash (lambda (k v)
                                 (setq result (cons (list :customer k :balance v) result)))
                               balances)
                      (list :migrated-count (length migrated)
                            :all-v2 (cl-every (lambda (e) (= (plist-get e :version) 2))
                                              migrated)
                            :balances (sort result
                                            (lambda (a b)
                                              (string< (plist-get a :customer)
                                                       (plist-get b :customer)))))))"#;
    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Saga/process manager: orchestrate multi-step process with compensation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_event_sourcing_saga() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--test-saga-log nil)
  (unwind-protect
      (let* ((log-action
              (lambda (step status detail)
                (setq neovm--test-saga-log
                      (cons (list :step step :status status :detail detail)
                            neovm--test-saga-log))))
             ;; Each step: returns (:ok result) or (:fail reason)
             ;; Each step has a compensating action
             (reserve-inventory
              (lambda (order)
                (funcall log-action 'reserve-inventory 'attempt order)
                (if (plist-get order :in-stock)
                    (progn (funcall log-action 'reserve-inventory 'ok nil)
                           (list :ok (list :reserved (plist-get order :item))))
                  (progn (funcall log-action 'reserve-inventory 'fail "out of stock")
                         (list :fail "out of stock")))))
             (charge-payment
              (lambda (order)
                (funcall log-action 'charge-payment 'attempt order)
                (if (>= (plist-get order :funds) (plist-get order :price))
                    (progn (funcall log-action 'charge-payment 'ok nil)
                           (list :ok (list :charged (plist-get order :price))))
                  (progn (funcall log-action 'charge-payment 'fail "insufficient funds")
                         (list :fail "insufficient funds")))))
             (ship-order
              (lambda (order)
                (funcall log-action 'ship-order 'attempt order)
                (if (plist-get order :valid-address)
                    (progn (funcall log-action 'ship-order 'ok nil)
                           (list :ok (list :shipped-to (plist-get order :address))))
                  (progn (funcall log-action 'ship-order 'fail "invalid address")
                         (list :fail "invalid address")))))
             ;; Compensating actions
             (release-inventory
              (lambda (order)
                (funcall log-action 'release-inventory 'compensate order)
                (list :compensated 'inventory)))
             (refund-payment
              (lambda (order)
                (funcall log-action 'refund-payment 'compensate order)
                (list :compensated 'payment)))
             ;; Run saga: execute steps, compensate on failure
             (run-saga
              (lambda (order)
                (setq neovm--test-saga-log nil)
                (let ((step1 (funcall reserve-inventory order)))
                  (if (eq (car step1) :fail)
                      (list :saga-failed :at 'reserve-inventory :reason (cadr step1)
                            :log (nreverse neovm--test-saga-log))
                    (let ((step2 (funcall charge-payment order)))
                      (if (eq (car step2) :fail)
                          (progn
                            (funcall release-inventory order)
                            (list :saga-failed :at 'charge-payment :reason (cadr step2)
                                  :compensations '(release-inventory)
                                  :log (nreverse neovm--test-saga-log)))
                        (let ((step3 (funcall ship-order order)))
                          (if (eq (car step3) :fail)
                              (progn
                                (funcall refund-payment order)
                                (funcall release-inventory order)
                                (list :saga-failed :at 'ship-order :reason (cadr step3)
                                      :compensations '(refund-payment release-inventory)
                                      :log (nreverse neovm--test-saga-log)))
                            (list :saga-ok
                                  :results (list step1 step2 step3)
                                  :log (nreverse neovm--test-saga-log)))))))))))
        (list
         ;; Happy path: all steps succeed
         (funcall run-saga '(:item "widget" :in-stock t :funds 100
                             :price 50 :valid-address t :address "123 Main"))
         ;; Fail at step 1: no compensation needed
         (funcall run-saga '(:item "unicorn" :in-stock nil :funds 100
                             :price 50 :valid-address t :address "123 Main"))
         ;; Fail at step 2: compensate inventory
         (funcall run-saga '(:item "widget" :in-stock t :funds 10
                             :price 50 :valid-address t :address "123 Main"))
         ;; Fail at step 3: compensate payment and inventory
         (funcall run-saga '(:item "widget" :in-stock t :funds 100
                             :price 50 :valid-address nil :address nil))))
    (makunbound 'neovm--test-saga-log)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:saga-ok :results ((:ok (:reserved \"widget\")) (:ok (:charged 50)) (:ok (:shipped-to \"123 Main\"))) :log ((:step reserve-inventory :status attempt :detail (:item \"widget\" :in-stock t :funds 100 :price 50 :valid-address t :address \"123 Main\")) (:step reserve-inventory :status ok :detail nil) (:step charge-payment :status attempt :detail (:item \"widget\" :in-stock t :funds 100 :price 50 :valid-address t :address \"123 Main\")) (:step charge-payment :status ok :detail nil) (:step ship-order :status attempt :detail (:item \"widget\" :in-stock t :funds 100 :price 50 :valid-address t :address \"123 Main\")) (:step ship-order :status ok :detail nil))) (:saga-failed :at reserve-inventory :reason \"out of stock\" :log ((:step reserve-inventory :status attempt :detail (:item \"unicorn\" :in-stock nil :funds 100 :price 50 :valid-address t :address \"123 Main\")) (:step reserve-inventory :status fail :detail \"out of stock\"))) (:saga-failed :at charge-payment :reason \"insufficient funds\" :compensations (release-inventory) :log ((:step reserve-inventory :status attempt :detail (:item \"widget\" :in-stock t :funds 10 :price 50 :valid-address t :address \"123 Main\")) (:step reserve-inventory :status ok :detail nil) (:step charge-payment :status attempt :detail (:item \"widget\" :in-stock t :funds 10 :price 50 :valid-address t :address \"123 Main\")) (:step charge-payment :status fail :detail \"insufficient funds\") (:step release-inventory :status compensate :detail (:item \"widget\" :in-stock t :funds 10 :price 50 :valid-address t :address \"123 Main\")))) (:saga-failed :at ship-order :reason \"invalid address\" :compensations (refund-payment release-inventory) :log ((:step reserve-inventory :status attempt :detail (:item \"widget\" :in-stock t :funds 100 :price 50 :valid-address nil :address nil)) (:step reserve-inventory :status ok :detail nil) (:step charge-payment :status attempt :detail (:item \"widget\" :in-stock t :funds 100 :price 50 :valid-address nil :address nil)) (:step charge-payment :status ok :detail nil) (:step ship-order :status attempt :detail (:item \"widget\" :in-stock t :funds 100 :price 50 :valid-address nil :address nil)) (:step ship-order :status fail :detail \"invalid address\") (:step refund-payment :status compensate :detail (:item \"widget\" :in-stock t :funds 100 :price 50 :valid-address nil :address nil)) (:step release-inventory :status compensate :detail (:item \"widget\" :in-stock t :funds 100 :price 50 :valid-address nil :address nil)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Command validation before event creation (CQRS-style)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_event_sourcing_command_validation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* (;; Current aggregate state (account)
                         (make-account
                          (lambda (id name balance status)
                            (list :id id :name name :balance balance :status status)))
                         ;; Validate a command against current state, produce event or error
                         (validate-command
                          (lambda (account command)
                            (let ((cmd-type (plist-get command :type))
                                  (cmd-data (plist-get command :data)))
                              (cond
                               ;; Deposit command
                               ((eq cmd-type 'deposit)
                                (let ((amount (plist-get cmd-data :amount)))
                                  (cond
                                   ((not (numberp amount))
                                    (list :rejected "amount must be a number"))
                                   ((<= amount 0)
                                    (list :rejected "amount must be positive"))
                                   ((eq (plist-get account :status) 'frozen)
                                    (list :rejected "account is frozen"))
                                   (t
                                    (list :accepted
                                          (list :event-type 'money-deposited
                                                :amount amount
                                                :new-balance
                                                (+ (plist-get account :balance) amount)))))))
                               ;; Withdraw command
                               ((eq cmd-type 'withdraw)
                                (let ((amount (plist-get cmd-data :amount)))
                                  (cond
                                   ((not (numberp amount))
                                    (list :rejected "amount must be a number"))
                                   ((<= amount 0)
                                    (list :rejected "amount must be positive"))
                                   ((eq (plist-get account :status) 'frozen)
                                    (list :rejected "account is frozen"))
                                   ((> amount (plist-get account :balance))
                                    (list :rejected
                                          (format "insufficient funds: have %d, need %d"
                                                  (plist-get account :balance) amount)))
                                   (t
                                    (list :accepted
                                          (list :event-type 'money-withdrawn
                                                :amount amount
                                                :new-balance
                                                (- (plist-get account :balance) amount)))))))
                               ;; Freeze command
                               ((eq cmd-type 'freeze)
                                (if (eq (plist-get account :status) 'frozen)
                                    (list :rejected "already frozen")
                                  (list :accepted
                                        (list :event-type 'account-frozen))))
                               ;; Unknown command
                               (t (list :rejected
                                        (format "unknown command: %s" cmd-type)))))))
                         ;; Test account
                         (acct (funcall make-account "ACC-1" "Alice" 500 'active))
                         ;; Frozen account
                         (frozen-acct (funcall make-account "ACC-2" "Bob" 100 'frozen)))
                    (list
                     ;; Valid deposit
                     (funcall validate-command acct
                              '(:type deposit :data (:amount 200)))
                     ;; Valid withdrawal
                     (funcall validate-command acct
                              '(:type withdraw :data (:amount 300)))
                     ;; Overdraft
                     (funcall validate-command acct
                              '(:type withdraw :data (:amount 999)))
                     ;; Invalid amount
                     (funcall validate-command acct
                              '(:type deposit :data (:amount -50)))
                     ;; Non-numeric amount
                     (funcall validate-command acct
                              '(:type deposit :data (:amount "fifty")))
                     ;; Frozen account deposit
                     (funcall validate-command frozen-acct
                              '(:type deposit :data (:amount 100)))
                     ;; Freeze active account
                     (funcall validate-command acct
                              '(:type freeze :data nil))
                     ;; Double freeze
                     (funcall validate-command frozen-acct
                              '(:type freeze :data nil))
                     ;; Unknown command
                     (funcall validate-command acct
                              '(:type transfer :data (:to "ACC-2")))))"#;
    let expect = expect_test::expect![[
        r#""OK ((:accepted (:event-type money-deposited :amount 200 :new-balance 700)) (:accepted (:event-type money-withdrawn :amount 300 :new-balance 200)) (:rejected \"insufficient funds: have 500, need 999\") (:rejected \"amount must be positive\") (:rejected \"amount must be a number\") (:rejected \"account is frozen\") (:accepted (:event-type account-frozen)) (:rejected \"already frozen\") (:rejected \"unknown command: transfer\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
