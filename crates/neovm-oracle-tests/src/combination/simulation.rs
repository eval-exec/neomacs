//! Complex oracle parity tests for simulation implementations in Elisp.
//!
//! Tests 1D cellular automaton (Rule 30), bank account simulation with
//! transactions and interest, inventory management system, projectile
//! motion physics, weather state machine with transitions, and a task
//! scheduler with priority and deadlines.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// 1D Cellular automaton (Rule 30)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sim_cellular_automaton_rule30() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(progn
  ;; Rule 30: 1D cellular automaton
  ;; Rule 30 in binary: 00011110
  ;; neighborhood (L,C,R) -> new state
  (fset 'neovm--test-rule30
    (lambda (l c r)
      (let ((idx (+ (* l 4) (* c 2) r)))
        ;; Rule 30 = 30 decimal = 00011110 binary
        ;; Bit positions: 7=0,6=0,5=0,4=1,3=1,2=1,1=1,0=0
        (if (= (logand (ash 30 (- idx)) 1) 1) 1 0))))
  (fset 'neovm--test-ca-step
    (lambda (cells)
      (let* ((len (length cells))
             (new (make-vector len 0))
             (i 0))
        (while (< i len)
          (let ((l (if (> i 0) (aref cells (1- i)) 0))
                (c (aref cells i))
                (r (if (< i (1- len)) (aref cells (1+ i)) 0)))
            (aset new i (funcall 'neovm--test-rule30 l c r)))
          (setq i (1+ i)))
        new)))
  (fset 'neovm--test-ca-to-string
    (lambda (cells)
      (let ((s "") (i 0) (len (length cells)))
        (while (< i len)
          (setq s (concat s (if (= (aref cells i) 1) "#" ".")))
          (setq i (1+ i)))
        s)))
  (unwind-protect
      (let* ((width 21)
             (cells (make-vector width 0))
             ;; Start with single cell in center
             (dummy (aset cells (/ width 2) 1))
             (generations nil))
        ;; Run 10 generations
        (dotimes (gen 10)
          (setq generations (cons (funcall 'neovm--test-ca-to-string cells) generations))
          (setq cells (funcall 'neovm--test-ca-step cells)))
        (setq generations (cons (funcall 'neovm--test-ca-to-string cells) generations))
        (nreverse generations))
    (fmakunbound 'neovm--test-rule30)
    (fmakunbound 'neovm--test-ca-step)
    (fmakunbound 'neovm--test-ca-to-string)))"####;
    let expect = expect_test::expect![[
        r###""OK (\"..........#..........\" \".........###.........\" \"........##..#........\" \".......##.####.......\" \"......##..#...#......\" \".....##.####.###.....\" \"....##..#....#..#....\" \"...##.####..######...\" \"..##..#...###.....#..\" \".##.####.##..#...###.\" \"##..#....#.####.##..#\")""###
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bank simulation: accounts, transactions, interest
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sim_bank_accounts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(progn
  ;; Account: (id balance transaction-log)
  (fset 'neovm--test-bank-create
    (lambda (id initial)
      (list id initial (list (list 'open initial)))))
  (fset 'neovm--test-bank-deposit
    (lambda (acct amount)
      (let ((id (car acct))
            (bal (cadr acct))
            (log (caddr acct)))
        (list id (+ bal amount)
              (cons (list 'deposit amount (+ bal amount)) log)))))
  (fset 'neovm--test-bank-withdraw
    (lambda (acct amount)
      (let ((id (car acct))
            (bal (cadr acct))
            (log (caddr acct)))
        (if (> amount bal)
            (list id bal (cons (list 'rejected amount bal) log))
          (list id (- bal amount)
                (cons (list 'withdraw amount (- bal amount)) log))))))
  (fset 'neovm--test-bank-interest
    (lambda (acct rate)
      (let* ((id (car acct))
             (bal (cadr acct))
             (log (caddr acct))
             (interest (/ (* bal rate) 100))
             (new-bal (+ bal interest)))
        (list id new-bal
              (cons (list 'interest interest new-bal) log)))))
  (fset 'neovm--test-bank-transfer
    (lambda (from-acct to-acct amount)
      (if (> amount (cadr from-acct))
          (list from-acct to-acct 'failed)
        (list (funcall 'neovm--test-bank-withdraw from-acct amount)
              (funcall 'neovm--test-bank-deposit to-acct amount)
              'ok))))
  (unwind-protect
      (let* ((acct-a (funcall 'neovm--test-bank-create 'checking 1000))
             (acct-b (funcall 'neovm--test-bank-create 'savings 5000))
             ;; Deposits
             (acct-a (funcall 'neovm--test-bank-deposit acct-a 500))
             (acct-b (funcall 'neovm--test-bank-deposit acct-b 2000))
             ;; Withdrawals
             (acct-a (funcall 'neovm--test-bank-withdraw acct-a 200))
             ;; Rejected withdrawal (overdraft)
             (acct-a (funcall 'neovm--test-bank-withdraw acct-a 9999))
             ;; Interest on savings (5%)
             (acct-b (funcall 'neovm--test-bank-interest acct-b 5))
             ;; Transfer
             (xfer (funcall 'neovm--test-bank-transfer acct-a acct-b 300))
             (acct-a (car xfer))
             (acct-b (cadr xfer))
             (xfer-status (caddr xfer))
             ;; Failed transfer (too much)
             (xfer2 (funcall 'neovm--test-bank-transfer acct-a acct-b 99999))
             (xfer2-status (caddr xfer2)))
        (list
          (cadr acct-a)         ;; checking balance
          (cadr acct-b)         ;; savings balance
          xfer-status           ;; first transfer status
          xfer2-status          ;; second transfer status
          (length (caddr acct-a)) ;; checking log entries
          (length (caddr acct-b)))) ;; savings log entries
    (fmakunbound 'neovm--test-bank-create)
    (fmakunbound 'neovm--test-bank-deposit)
    (fmakunbound 'neovm--test-bank-withdraw)
    (fmakunbound 'neovm--test-bank-interest)
    (fmakunbound 'neovm--test-bank-transfer)))"####;
    let expect = expect_test::expect![[r#""OK (1000 7650 ok failed 5 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Inventory management system
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sim_inventory_management() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(progn
  ;; Inventory: alist of (item-name . (quantity price reorder-level))
  (fset 'neovm--test-inv-create (lambda () nil))
  (fset 'neovm--test-inv-add
    (lambda (inv name qty price reorder)
      (let ((existing (assoc name inv)))
        (if existing
            (let ((data (cdr existing)))
              (setcdr existing (list (+ (car data) qty) (cadr data) (caddr data)))
              inv)
          (cons (cons name (list qty price reorder)) inv)))))
  (fset 'neovm--test-inv-sell
    (lambda (inv name qty)
      (let ((item (assoc name inv)))
        (if (and item (>= (car (cdr item)) qty))
            (progn
              (setcar (cdr item) (- (car (cdr item)) qty))
              (list inv 'ok (* qty (cadr (cdr item)))))
          (list inv 'insufficient 0)))))
  (fset 'neovm--test-inv-value
    (lambda (inv)
      (let ((total 0))
        (dolist (item inv)
          (setq total (+ total (* (cadr item) (caddr item)))))
        total)))
  (fset 'neovm--test-inv-needs-reorder
    (lambda (inv)
      (let ((result nil))
        (dolist (item inv)
          (when (<= (cadr item) (cadddr item))
            (setq result (cons (car item) result))))
        (nreverse result))))
  (fset 'neovm--test-inv-report
    (lambda (inv)
      (mapcar (lambda (item)
                (list (car item) (cadr item) (* (cadr item) (caddr item))))
              inv)))
  (unwind-protect
      (let* ((inv (funcall 'neovm--test-inv-create))
             (inv (funcall 'neovm--test-inv-add inv "widget" 100 10 20))
             (inv (funcall 'neovm--test-inv-add inv "gadget" 50 25 10))
             (inv (funcall 'neovm--test-inv-add inv "doohickey" 30 15 5))
             ;; Total inventory value
             (val1 (funcall 'neovm--test-inv-value inv))
             ;; Sell some items
             (r1 (funcall 'neovm--test-inv-sell inv "widget" 85))
             (inv (car r1))
             (s1-status (cadr r1))
             (s1-revenue (caddr r1))
             ;; Try to sell more than available
             (r2 (funcall 'neovm--test-inv-sell inv "widget" 50))
             (s2-status (cadr r2))
             ;; Restock
             (inv (funcall 'neovm--test-inv-add inv "widget" 200 10 20))
             ;; Check reorder needs
             (reorder-before (funcall 'neovm--test-inv-needs-reorder inv))
             ;; Sell gadgets to trigger reorder
             (r3 (funcall 'neovm--test-inv-sell inv "gadget" 45))
             (inv (car r3))
             (reorder-after (funcall 'neovm--test-inv-needs-reorder inv))
             ;; Final report
             (report (funcall 'neovm--test-inv-report inv))
             (final-val (funcall 'neovm--test-inv-value inv)))
        (list val1 s1-status s1-revenue s2-status
              reorder-before reorder-after report final-val))
    (fmakunbound 'neovm--test-inv-create)
    (fmakunbound 'neovm--test-inv-add)
    (fmakunbound 'neovm--test-inv-sell)
    (fmakunbound 'neovm--test-inv-value)
    (fmakunbound 'neovm--test-inv-needs-reorder)
    (fmakunbound 'neovm--test-inv-report)))"####;
    let expect = expect_test::expect![[
        r#""OK (2700 ok 850 insufficient nil (\"gadget\") ((\"doohickey\" 30 450) (\"gadget\" 5 125) (\"widget\" 215 2150)) 2725)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Simple physics simulation: projectile motion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sim_projectile_motion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(progn
  ;; Projectile with drag-free ballistic trajectory
  ;; State: (x y vx vy t)
  ;; Using integer arithmetic scaled by 1000 to avoid float precision issues
  (fset 'neovm--test-proj-create
    (lambda (vx0 vy0)
      ;; velocity in units/sec * 1000
      (list 0 0 vx0 vy0 0)))
  (fset 'neovm--test-proj-step
    (lambda (state dt g)
      ;; dt in milliseconds, g = gravity * 1000
      (let* ((x (car state))
             (y (cadr state))
             (vx (caddr state))
             (vy (cadddr state))
             (t-val (car (cddddr state)))
             ;; new position: x + vx*dt/1000, y + vy*dt/1000
             (new-x (+ x (/ (* vx dt) 1000)))
             (new-y (+ y (/ (* vy dt) 1000)))
             ;; new velocity: vy - g*dt/1000
             (new-vy (- vy (/ (* g dt) 1000)))
             (new-t (+ t-val dt)))
        (list new-x (max 0 new-y) vx new-vy new-t))))
  (fset 'neovm--test-proj-simulate
    (lambda (vx0 vy0 dt g max-steps)
      (let ((state (funcall 'neovm--test-proj-create vx0 vy0))
            (trajectory nil)
            (steps 0)
            (landed nil))
        (while (and (< steps max-steps) (not landed))
          (setq trajectory (cons (list (car state) (cadr state)) trajectory))
          (setq state (funcall 'neovm--test-proj-step state dt g))
          (setq steps (1+ steps))
          ;; landed if y <= 0 and we've moved at least one step
          (when (and (> steps 1) (<= (cadr state) 0))
            (setq landed t)))
        (setq trajectory (cons (list (car state) (cadr state)) trajectory))
        (list (nreverse trajectory)
              (car state)        ;; final x
              steps              ;; total steps
              (car (cddddr state))))))  ;; total time
  (unwind-protect
      (let* (;; Projectile 1: 45 degree (equal vx, vy), scaled * 1000
             (sim1 (funcall 'neovm--test-proj-simulate 100 100 100 10 200))
             ;; Projectile 2: high arc (vy >> vx)
             (sim2 (funcall 'neovm--test-proj-simulate 50 200 100 10 200))
             ;; Projectile 3: flat trajectory (vx >> vy)
             (sim3 (funcall 'neovm--test-proj-simulate 200 50 100 10 200)))
        (list
          ;; Final x positions (range)
          (cadr sim1) (cadr sim2) (cadr sim3)
          ;; Number of steps
          (caddr sim1) (caddr sim2) (caddr sim3)
          ;; Trajectory lengths
          (length (car sim1)) (length (car sim2)) (length (car sim3))
          ;; Symmetry: 45-deg trajectory should be roughly symmetric
          ;; (final x should be approximately 2 * vx * vy / g)
          ;; Just verify relative ordering: high-arc goes less far than 45-deg
          (< (cadr sim2) (cadr sim1))))
    (fmakunbound 'neovm--test-proj-create)
    (fmakunbound 'neovm--test-proj-step)
    (fmakunbound 'neovm--test-proj-simulate)))"####;
    let expect = expect_test::expect![[r#""OK (2000 1000 2020 200 200 101 201 201 102 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Weather state machine with probabilistic transitions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sim_weather_state_machine() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Deterministic "probabilistic" transitions using a fixed seed LCG
    let form = r####"(progn
  ;; Simple LCG random number generator (deterministic)
  ;; seed = (seed * 1103515245 + 12345) mod 2^31
  ;; We use modular arithmetic with logand to keep in 28-bit range
  (fset 'neovm--test-lcg-next
    (lambda (seed)
      (logand (+ (* seed 1103) 12345) 268435455)))  ;; 2^28 - 1
  (fset 'neovm--test-weather-transition
    (lambda (state seed)
      ;; Returns (new-state new-seed)
      ;; Deterministic transitions based on current state and seed
      (let* ((new-seed (funcall 'neovm--test-lcg-next seed))
             (roll (% new-seed 100))
             (new-state
              (cond
                ((eq state 'sunny)
                 (cond ((< roll 60) 'sunny)
                       ((< roll 85) 'cloudy)
                       (t 'rainy)))
                ((eq state 'cloudy)
                 (cond ((< roll 30) 'sunny)
                       ((< roll 60) 'cloudy)
                       ((< roll 85) 'rainy)
                       (t 'stormy)))
                ((eq state 'rainy)
                 (cond ((< roll 20) 'sunny)
                       ((< roll 50) 'cloudy)
                       ((< roll 80) 'rainy)
                       (t 'stormy)))
                ((eq state 'stormy)
                 (cond ((< roll 10) 'sunny)
                       ((< roll 30) 'cloudy)
                       ((< roll 70) 'rainy)
                       (t 'stormy)))
                (t 'sunny))))
        (list new-state new-seed))))
  (fset 'neovm--test-weather-simulate
    (lambda (initial-state seed days)
      (let ((state initial-state)
            (current-seed seed)
            (history nil)
            (counts (list (cons 'sunny 0) (cons 'cloudy 0)
                          (cons 'rainy 0) (cons 'stormy 0))))
        (dotimes (d days)
          (setq history (cons state history))
          ;; count this state
          (let ((entry (assq state counts)))
            (setcdr entry (1+ (cdr entry))))
          ;; transition
          (let ((result (funcall 'neovm--test-weather-transition state current-seed)))
            (setq state (car result))
            (setq current-seed (cadr result))))
        (list (nreverse history) counts))))
  (unwind-protect
      (let* ((sim (funcall 'neovm--test-weather-simulate 'sunny 42 30))
             (history (car sim))
             (counts (cadr sim))
             ;; Count consecutive same-weather streaks
             (streaks nil)
             (current-streak 1)
             (prev (car history))
             (rest (cdr history)))
        (while rest
          (if (eq (car rest) prev)
              (setq current-streak (1+ current-streak))
            (setq streaks (cons (cons prev current-streak) streaks))
            (setq current-streak 1)
            (setq prev (car rest)))
          (setq rest (cdr rest)))
        (setq streaks (cons (cons prev current-streak) streaks))
        (list
          (length history)
          counts
          ;; first 10 weather states
          (let ((first10 nil) (h history) (i 0))
            (while (and (< i 10) h)
              (setq first10 (cons (car h) first10))
              (setq h (cdr h))
              (setq i (1+ i)))
            (nreverse first10))
          ;; max streak
          (let ((max-s 0))
            (dolist (s streaks)
              (when (> (cdr s) max-s)
                (setq max-s (cdr s))))
            max-s)
          ;; number of distinct streaks
          (length streaks)))
    (fmakunbound 'neovm--test-lcg-next)
    (fmakunbound 'neovm--test-weather-transition)
    (fmakunbound 'neovm--test-weather-simulate)))"####;
    let expect = expect_test::expect![[
        r#""OK (30 ((sunny . 16) (cloudy . 10) (rainy . 4) (stormy . 0)) (sunny cloudy cloudy rainy rainy cloudy rainy sunny cloudy sunny) 5 17)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Task scheduler with priority and deadlines
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sim_task_scheduler() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(progn
  ;; Task: (name priority deadline duration status)
  ;; Priority: higher number = higher priority
  ;; Schedule tasks by earliest-deadline-first with priority tiebreak
  (fset 'neovm--test-sched-make-task
    (lambda (name priority deadline duration)
      (list name priority deadline duration 'pending)))
  (fset 'neovm--test-sched-task-name (lambda (t) (car t)))
  (fset 'neovm--test-sched-task-priority (lambda (t) (cadr t)))
  (fset 'neovm--test-sched-task-deadline (lambda (t) (caddr t)))
  (fset 'neovm--test-sched-task-duration (lambda (t) (cadddr t)))
  (fset 'neovm--test-sched-task-status (lambda (t) (car (cddddr t))))
  (fset 'neovm--test-sched-set-status
    (lambda (t s) (list (car t) (cadr t) (caddr t) (cadddr t) s)))
  (fset 'neovm--test-sched-compare
    (lambda (a b)
      ;; EDF: earlier deadline first, then higher priority
      (or (< (caddr a) (caddr b))
          (and (= (caddr a) (caddr b))
               (> (cadr a) (cadr b))))))
  (fset 'neovm--test-sched-run
    (lambda (tasks)
      (let* ((sorted (sort (copy-sequence tasks) 'neovm--test-sched-compare))
             (current-time 0)
             (schedule nil)
             (missed nil))
        (dolist (task sorted)
          (let* ((name (funcall 'neovm--test-sched-task-name task))
                 (deadline (funcall 'neovm--test-sched-task-deadline task))
                 (duration (funcall 'neovm--test-sched-task-duration task))
                 (start current-time)
                 (end (+ start duration))
                 (on-time (<= end deadline)))
            (setq schedule
                  (cons (list name start end on-time) schedule))
            (unless on-time
              (setq missed (cons name missed)))
            (setq current-time end)))
        (list (nreverse schedule) (nreverse missed) current-time))))
  (unwind-protect
      (let* ((tasks
              (list
                (funcall 'neovm--test-sched-make-task "compile" 5 10 3)
                (funcall 'neovm--test-sched-make-task "test" 4 15 5)
                (funcall 'neovm--test-sched-make-task "deploy" 3 20 2)
                (funcall 'neovm--test-sched-make-task "backup" 2 8 4)
                (funcall 'neovm--test-sched-make-task "report" 1 25 3)
                (funcall 'neovm--test-sched-make-task "urgent-fix" 10 5 2)
                (funcall 'neovm--test-sched-make-task "review" 4 12 3)))
             (result (funcall 'neovm--test-sched-run tasks))
             (schedule (car result))
             (missed (cadr result))
             (total-time (caddr result)))
        (list
          ;; scheduled order (names)
          (mapcar 'car schedule)
          ;; which tasks were on time
          (mapcar (lambda (s) (list (car s) (cadddr s))) schedule)
          ;; missed deadlines
          missed
          ;; total time
          total-time
          ;; number of tasks
          (length schedule)))
    (fmakunbound 'neovm--test-sched-make-task)
    (fmakunbound 'neovm--test-sched-task-name)
    (fmakunbound 'neovm--test-sched-task-priority)
    (fmakunbound 'neovm--test-sched-task-deadline)
    (fmakunbound 'neovm--test-sched-task-duration)
    (fmakunbound 'neovm--test-sched-task-status)
    (fmakunbound 'neovm--test-sched-set-status)
    (fmakunbound 'neovm--test-sched-compare)
    (fmakunbound 'neovm--test-sched-run)))"####;
    let expect = expect_test::expect![[
        r#""OK ((\"urgent-fix\" \"backup\" \"compile\" \"review\" \"test\" \"deploy\" \"report\") ((\"urgent-fix\" t) (\"backup\" t) (\"compile\" t) (\"review\" t) (\"test\" nil) (\"deploy\" t) (\"report\" t)) (\"test\") 22 7)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Game of Life 1D (elementary cellular automaton, Rule 110)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sim_cellular_automaton_rule110() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Rule 110 is known to be Turing-complete
    let form = r####"(progn
  (fset 'neovm--test-rule110
    (lambda (l c r)
      (let ((idx (+ (* l 4) (* c 2) r)))
        ;; Rule 110 = 01101110 binary
        (if (= (logand (ash 110 (- idx)) 1) 1) 1 0))))
  (fset 'neovm--test-ca110-step
    (lambda (cells)
      (let* ((len (length cells))
             (new (make-vector len 0))
             (i 0))
        (while (< i len)
          (let ((l (if (> i 0) (aref cells (1- i)) 0))
                (c (aref cells i))
                (r (if (< i (1- len)) (aref cells (1+ i)) 0)))
            (aset new i (funcall 'neovm--test-rule110 l c r)))
          (setq i (1+ i)))
        new)))
  (fset 'neovm--test-ca-count-live
    (lambda (cells)
      (let ((count 0) (i 0) (len (length cells)))
        (while (< i len)
          (when (= (aref cells i) 1)
            (setq count (1+ count)))
          (setq i (1+ i)))
        count)))
  (unwind-protect
      (let* ((width 25)
             (cells (make-vector width 0))
             ;; Start with rightmost cell active (classic Rule 110 start)
             (dummy (aset cells (1- width) 1))
             (gen-counts nil)
             (gen-strings nil))
        ;; Run 12 generations
        (dotimes (gen 12)
          (setq gen-counts (cons (funcall 'neovm--test-ca-count-live cells) gen-counts))
          (let ((s "") (i 0))
            (while (< i width)
              (setq s (concat s (if (= (aref cells i) 1) "#" ".")))
              (setq i (1+ i)))
            (setq gen-strings (cons s gen-strings)))
          (setq cells (funcall 'neovm--test-ca110-step cells)))
        (list
          (nreverse gen-strings)
          (nreverse gen-counts)
          ;; Total live cells across all generations
          (apply '+ (mapcar 'identity gen-counts))))
    (fmakunbound 'neovm--test-rule110)
    (fmakunbound 'neovm--test-ca110-step)
    (fmakunbound 'neovm--test-ca-count-live)))"####;
    let expect = expect_test::expect![[
        r#""OK ((\"........................#\" \".......................##\" \"......................###\" \".....................##.#\" \"....................#####\" \"...................##...#\" \"..................###..##\" \".................##.#.###\" \"................#######.#\" \"...............##.....###\" \"..............###....##.#\" \".............##.#...#####\") (1 2 3 3 5 3 5 6 8 5 6 8) 8)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
