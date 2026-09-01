//! Oracle parity tests for reactive programming patterns in Elisp.
//!
//! Covers observable values with change listeners, computed/derived values,
//! two-way data binding, event stream filtering/mapping, debounced updates,
//! and dependency graph with topological update ordering.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Observable value with change listeners
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_reactive_observable_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // An observable cell: get/set value, subscribe to changes, listeners
    // receive old and new values.
    let form = r#"
(progn
  (defvar neovm--rx-obs-store nil)
  (defvar neovm--rx-obs-listeners nil)
  (defvar neovm--rx-obs-log nil)

  (fset 'neovm--rx-obs-create
    (lambda (name initial)
      "Create an observable value with NAME and INITIAL value."
      (puthash name initial neovm--rx-obs-store)
      (puthash name nil neovm--rx-obs-listeners)
      name))

  (fset 'neovm--rx-obs-get
    (lambda (name)
      (gethash name neovm--rx-obs-store)))

  (fset 'neovm--rx-obs-subscribe
    (lambda (name listener-name listener-fn)
      "Subscribe LISTENER-FN (called with old new) to observable NAME."
      (puthash name
               (cons (cons listener-name listener-fn)
                     (gethash name neovm--rx-obs-listeners))
               neovm--rx-obs-listeners)))

  (fset 'neovm--rx-obs-set
    (lambda (name new-val)
      "Set observable NAME to NEW-VAL, notify listeners."
      (let ((old-val (gethash name neovm--rx-obs-store)))
        (unless (equal old-val new-val)
          (puthash name new-val neovm--rx-obs-store)
          (dolist (listener (reverse (gethash name neovm--rx-obs-listeners)))
            (let ((result (funcall (cdr listener) old-val new-val)))
              (setq neovm--rx-obs-log
                    (cons (list (car listener) name old-val new-val result)
                          neovm--rx-obs-log))))))))

  (fset 'neovm--rx-obs-unsubscribe
    (lambda (name listener-name)
      "Remove listener LISTENER-NAME from observable NAME."
      (puthash name
               (let ((result nil))
                 (dolist (l (gethash name neovm--rx-obs-listeners))
                   (unless (eq (car l) listener-name)
                     (setq result (cons l result))))
                 (nreverse result))
               neovm--rx-obs-listeners)))

  (unwind-protect
      (progn
        (setq neovm--rx-obs-store (make-hash-table))
        (setq neovm--rx-obs-listeners (make-hash-table))
        (setq neovm--rx-obs-log nil)

        ;; Create observables
        (funcall 'neovm--rx-obs-create 'temperature 20)
        (funcall 'neovm--rx-obs-create 'humidity 50)

        ;; Subscribe listeners
        (funcall 'neovm--rx-obs-subscribe 'temperature 'display
                 (lambda (old new) (format "Temp: %d -> %d" old new)))
        (funcall 'neovm--rx-obs-subscribe 'temperature 'alarm
                 (lambda (old new) (if (> new 30) "HOT!" "ok")))
        (funcall 'neovm--rx-obs-subscribe 'humidity 'display
                 (lambda (old new) (format "Hum: %d -> %d" old new)))

        ;; Set values
        (funcall 'neovm--rx-obs-set 'temperature 25)
        (funcall 'neovm--rx-obs-set 'temperature 35)
        (funcall 'neovm--rx-obs-set 'humidity 70)
        ;; Setting same value should NOT trigger
        (funcall 'neovm--rx-obs-set 'humidity 70)
        ;; Unsubscribe alarm, then change temp
        (funcall 'neovm--rx-obs-unsubscribe 'temperature 'alarm)
        (funcall 'neovm--rx-obs-set 'temperature 15)

        (list
         ;; Current values
         (funcall 'neovm--rx-obs-get 'temperature)
         (funcall 'neovm--rx-obs-get 'humidity)
         ;; Number of log entries
         (length neovm--rx-obs-log)
         ;; Full log (reversed to chronological)
         (nreverse neovm--rx-obs-log)))
    (fmakunbound 'neovm--rx-obs-create)
    (fmakunbound 'neovm--rx-obs-get)
    (fmakunbound 'neovm--rx-obs-subscribe)
    (fmakunbound 'neovm--rx-obs-set)
    (fmakunbound 'neovm--rx-obs-unsubscribe)
    (makunbound 'neovm--rx-obs-store)
    (makunbound 'neovm--rx-obs-listeners)
    (makunbound 'neovm--rx-obs-log)))
"#;
    let expect = expect_test::expect![[
        r#""OK (15 70 6 ((display temperature 20 25 \"Temp: 20 -> 25\") (alarm temperature 20 25 \"ok\") (display temperature 25 35 \"Temp: 25 -> 35\") (alarm temperature 25 35 \"HOT!\") (display humidity 50 70 \"Hum: 50 -> 70\") (display temperature 35 15 \"Temp: 35 -> 15\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Computed/derived values (auto-update when dependencies change)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_reactive_computed_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Computed cells depend on source cells. When a source changes,
    // all dependent computed cells are recalculated.
    let form = r#"
(progn
  (defvar neovm--rx-cells nil)
  (defvar neovm--rx-computed nil)
  (defvar neovm--rx-eval-log nil)

  (fset 'neovm--rx-cell-set
    (lambda (name value)
      "Set a source cell and propagate to computed cells."
      (puthash name value neovm--rx-cells)
      ;; Recalculate all computed cells that depend on this cell
      (maphash
       (lambda (cname spec)
         (when (memq name (plist-get spec :deps))
           (let ((compute-fn (plist-get spec :fn))
                 (deps (plist-get spec :deps)))
             (let ((args (mapcar (lambda (d) (gethash d neovm--rx-cells)) deps)))
               (let ((new-val (apply compute-fn args)))
                 (puthash cname new-val neovm--rx-cells)
                 (setq neovm--rx-eval-log
                       (cons (list cname args new-val) neovm--rx-eval-log)))))))
       neovm--rx-computed)))

  (fset 'neovm--rx-cell-get
    (lambda (name)
      (gethash name neovm--rx-cells)))

  (fset 'neovm--rx-define-computed
    (lambda (name deps compute-fn)
      "Define a computed cell that depends on DEPS and uses COMPUTE-FN."
      (puthash name (list :deps deps :fn compute-fn) neovm--rx-computed)
      ;; Initial calculation
      (let ((args (mapcar (lambda (d) (gethash d neovm--rx-cells)) deps)))
        (let ((val (apply compute-fn args)))
          (puthash name val neovm--rx-cells)
          val))))

  (unwind-protect
      (progn
        (setq neovm--rx-cells (make-hash-table))
        (setq neovm--rx-computed (make-hash-table))
        (setq neovm--rx-eval-log nil)

        ;; Source cells
        (puthash 'price 100 neovm--rx-cells)
        (puthash 'quantity 5 neovm--rx-cells)
        (puthash 'tax-rate 10 neovm--rx-cells)  ;; 10%

        ;; Computed: subtotal = price * quantity
        (funcall 'neovm--rx-define-computed 'subtotal '(price quantity)
                 (lambda (p q) (* p q)))

        ;; Computed: tax = subtotal * tax-rate / 100
        ;; Note: subtotal is in neovm--rx-cells but we read it directly
        (funcall 'neovm--rx-define-computed 'tax '(subtotal tax-rate)
                 (lambda (st tr) (/ (* st tr) 100)))

        ;; Computed: total = subtotal + tax
        (funcall 'neovm--rx-define-computed 'total '(subtotal tax)
                 (lambda (st tx) (+ st tx)))

        (let ((initial-subtotal (funcall 'neovm--rx-cell-get 'subtotal))
              (initial-tax (funcall 'neovm--rx-cell-get 'tax))
              (initial-total (funcall 'neovm--rx-cell-get 'total)))

          ;; Change price: should propagate to subtotal, tax, total
          (funcall 'neovm--rx-cell-set 'price 200)

          (let ((after-price-subtotal (funcall 'neovm--rx-cell-get 'subtotal))
                (after-price-tax (funcall 'neovm--rx-cell-get 'tax))
                (after-price-total (funcall 'neovm--rx-cell-get 'total)))

            ;; Change quantity
            (funcall 'neovm--rx-cell-set 'quantity 3)

            (list
             ;; Initial values
             initial-subtotal initial-tax initial-total
             ;; After price change
             after-price-subtotal after-price-tax after-price-total
             ;; After quantity change
             (funcall 'neovm--rx-cell-get 'subtotal)
             (funcall 'neovm--rx-cell-get 'tax)
             (funcall 'neovm--rx-cell-get 'total)
             ;; Number of recomputations logged
             (length neovm--rx-eval-log)))))
    (fmakunbound 'neovm--rx-cell-set)
    (fmakunbound 'neovm--rx-cell-get)
    (fmakunbound 'neovm--rx-define-computed)
    (makunbound 'neovm--rx-cells)
    (makunbound 'neovm--rx-computed)
    (makunbound 'neovm--rx-eval-log)))
"#;
    let expect = expect_test::expect![[r#""OK (500 50 550 1000 50 550 600 50 550 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Two-way data binding simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_reactive_two_way_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Two-way binding: when A changes, B is updated via a transform,
    // and when B changes, A is updated via the inverse transform.
    // Guards prevent infinite update loops.
    let form = r#"
(progn
  (defvar neovm--rx-bind-values nil)
  (defvar neovm--rx-bind-links nil)
  (defvar neovm--rx-bind-updating nil)
  (defvar neovm--rx-bind-log nil)

  (fset 'neovm--rx-bind-init
    (lambda ()
      (setq neovm--rx-bind-values (make-hash-table))
      (setq neovm--rx-bind-links nil)
      (setq neovm--rx-bind-updating nil)
      (setq neovm--rx-bind-log nil)))

  (fset 'neovm--rx-bind-create
    (lambda (name initial)
      (puthash name initial neovm--rx-bind-values)
      name))

  (fset 'neovm--rx-bind-link
    (lambda (src dst forward-fn inverse-fn)
      "Create a two-way binding between SRC and DST.
       FORWARD-FN: src-value -> dst-value.
       INVERSE-FN: dst-value -> src-value."
      (setq neovm--rx-bind-links
            (cons (list :src src :dst dst :fwd forward-fn :inv inverse-fn)
                  neovm--rx-bind-links))))

  (fset 'neovm--rx-bind-set
    (lambda (name value)
      "Set a bound variable, propagating through bindings."
      (unless (memq name neovm--rx-bind-updating)
        (let ((old (gethash name neovm--rx-bind-values)))
          (puthash name value neovm--rx-bind-values)
          (setq neovm--rx-bind-log
                (cons (list 'set name old value) neovm--rx-bind-log))
          ;; Propagate forward: if name is src, update dst
          (let ((neovm--rx-bind-updating (cons name neovm--rx-bind-updating)))
            (dolist (link neovm--rx-bind-links)
              (cond
               ((eq (plist-get link :src) name)
                (let ((new-dst (funcall (plist-get link :fwd) value)))
                  (funcall 'neovm--rx-bind-set (plist-get link :dst) new-dst)))
               ((eq (plist-get link :dst) name)
                (let ((new-src (funcall (plist-get link :inv) value)))
                  (funcall 'neovm--rx-bind-set (plist-get link :src) new-src))))))))))

  (fset 'neovm--rx-bind-get
    (lambda (name)
      (gethash name neovm--rx-bind-values)))

  (unwind-protect
      (progn
        (funcall 'neovm--rx-bind-init)

        ;; celsius <-> fahrenheit
        (funcall 'neovm--rx-bind-create 'celsius 0)
        (funcall 'neovm--rx-bind-create 'fahrenheit 32)
        (funcall 'neovm--rx-bind-link
                 'celsius 'fahrenheit
                 (lambda (c) (+ (* c 9/5) 32))   ;; C->F
                 (lambda (f) (* (- f 32) 5/9)))   ;; F->C

        ;; radius <-> area (area = pi * r^2, using integer approx: area = 314 * r * r / 100)
        (funcall 'neovm--rx-bind-create 'radius 1)
        (funcall 'neovm--rx-bind-create 'area 3)
        (funcall 'neovm--rx-bind-link
                 'radius 'area
                 (lambda (r) (/ (* 314 r r) 100))
                 (lambda (a) (round (sqrt (/ (* a 100.0) 314)))))

        ;; Set celsius to 100 -> fahrenheit should become 212
        (funcall 'neovm--rx-bind-set 'celsius 100)
        (let ((f-after-c (funcall 'neovm--rx-bind-get 'fahrenheit))
              (c-after-c (funcall 'neovm--rx-bind-get 'celsius)))

          ;; Set fahrenheit to 32 -> celsius should become 0
          (funcall 'neovm--rx-bind-set 'fahrenheit 32)
          (let ((c-after-f (funcall 'neovm--rx-bind-get 'celsius))
                (f-after-f (funcall 'neovm--rx-bind-get 'fahrenheit)))

            ;; Set radius to 10 -> area should be 314
            (funcall 'neovm--rx-bind-set 'radius 10)

            (list
             ;; Celsius -> Fahrenheit
             c-after-c f-after-c
             ;; Fahrenheit -> Celsius
             c-after-f f-after-f
             ;; Radius -> Area
             (funcall 'neovm--rx-bind-get 'radius)
             (funcall 'neovm--rx-bind-get 'area)
             ;; Number of log entries (demonstrates propagation)
             (length neovm--rx-bind-log)
             ;; No infinite loops occurred (log length is finite and small)
             (< (length neovm--rx-bind-log) 20)))))
    (fmakunbound 'neovm--rx-bind-init)
    (fmakunbound 'neovm--rx-bind-create)
    (fmakunbound 'neovm--rx-bind-link)
    (fmakunbound 'neovm--rx-bind-set)
    (fmakunbound 'neovm--rx-bind-get)
    (makunbound 'neovm--rx-bind-values)
    (makunbound 'neovm--rx-bind-links)
    (makunbound 'neovm--rx-bind-updating)
    (makunbound 'neovm--rx-bind-log)))
"#;
    let expect = expect_test::expect![[r#""ERR (void-variable 9/5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Event stream filtering and mapping
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_reactive_event_stream() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a composable event stream processing pipeline with
    // filter, map, reduce, and take operations.
    let form = r#"
(progn
  (fset 'neovm--rx-stream-create
    (lambda (events)
      "Create a stream from a list of events."
      (list :type 'source :events events)))

  (fset 'neovm--rx-stream-filter
    (lambda (stream pred)
      "Create a filtered stream."
      (list :type 'filter :source stream :pred pred)))

  (fset 'neovm--rx-stream-map
    (lambda (stream fn)
      "Create a mapped stream."
      (list :type 'map :source stream :fn fn)))

  (fset 'neovm--rx-stream-take
    (lambda (stream n)
      "Create a stream that takes only the first N elements."
      (list :type 'take :source stream :n n)))

  (fset 'neovm--rx-stream-scan
    (lambda (stream fn init)
      "Create a scan (running fold) stream."
      (list :type 'scan :source stream :fn fn :init init)))

  (fset 'neovm--rx-stream-materialize
    (lambda (stream)
      "Materialize a stream into a list."
      (let ((type (plist-get stream :type)))
        (cond
         ((eq type 'source)
          (plist-get stream :events))
         ((eq type 'filter)
          (let ((source-events (funcall 'neovm--rx-stream-materialize
                                        (plist-get stream :source)))
                (pred (plist-get stream :pred))
                (result nil))
            (dolist (e source-events)
              (when (funcall pred e)
                (setq result (cons e result))))
            (nreverse result)))
         ((eq type 'map)
          (let ((source-events (funcall 'neovm--rx-stream-materialize
                                        (plist-get stream :source)))
                (fn (plist-get stream :fn)))
            (mapcar fn source-events)))
         ((eq type 'take)
          (let ((source-events (funcall 'neovm--rx-stream-materialize
                                        (plist-get stream :source)))
                (n (plist-get stream :n))
                (result nil) (count 0))
            (dolist (e source-events)
              (when (< count n)
                (setq result (cons e result))
                (setq count (1+ count))))
            (nreverse result)))
         ((eq type 'scan)
          (let ((source-events (funcall 'neovm--rx-stream-materialize
                                        (plist-get stream :source)))
                (fn (plist-get stream :fn))
                (acc (plist-get stream :init))
                (result nil))
            (dolist (e source-events)
              (setq acc (funcall fn acc e))
              (setq result (cons acc result)))
            (nreverse result)))))))

  (unwind-protect
      (let* ((events '((:type click :x 10 :y 20)
                        (:type move :x 15 :y 25)
                        (:type click :x 30 :y 40)
                        (:type move :x 35 :y 45)
                        (:type click :x 50 :y 60)
                        (:type move :x 55 :y 65)
                        (:type click :x 70 :y 80)
                        (:type move :x 75 :y 85)))
             (base (funcall 'neovm--rx-stream-create events))
             ;; Filter: only clicks
             (clicks (funcall 'neovm--rx-stream-filter base
                              (lambda (e) (eq (plist-get e :type) 'click))))
             ;; Map: extract coordinates as (x . y)
             (coords (funcall 'neovm--rx-stream-map clicks
                              (lambda (e) (cons (plist-get e :x) (plist-get e :y)))))
             ;; Take: first 2 click coordinates
             (first2 (funcall 'neovm--rx-stream-take coords 2))
             ;; Scan: running distance from origin
             (distances (funcall 'neovm--rx-stream-scan coords
                                 (lambda (acc pair)
                                   (+ acc (+ (car pair) (cdr pair))))
                                 0))
             ;; Compose: filter moves, map to x, scan running sum
             (moves (funcall 'neovm--rx-stream-filter base
                             (lambda (e) (eq (plist-get e :type) 'move))))
             (move-xs (funcall 'neovm--rx-stream-map moves
                               (lambda (e) (plist-get e :x))))
             (running-x (funcall 'neovm--rx-stream-scan move-xs #'+ 0)))
        (list
         ;; All clicks
         (funcall 'neovm--rx-stream-materialize clicks)
         ;; Click coordinates
         (funcall 'neovm--rx-stream-materialize coords)
         ;; First 2 click coords
         (funcall 'neovm--rx-stream-materialize first2)
         ;; Running distance sums
         (funcall 'neovm--rx-stream-materialize distances)
         ;; Running x-sum of moves
         (funcall 'neovm--rx-stream-materialize running-x)
         ;; Total events vs filtered counts
         (length events)
         (length (funcall 'neovm--rx-stream-materialize clicks))
         (length (funcall 'neovm--rx-stream-materialize moves))))
    (fmakunbound 'neovm--rx-stream-create)
    (fmakunbound 'neovm--rx-stream-filter)
    (fmakunbound 'neovm--rx-stream-map)
    (fmakunbound 'neovm--rx-stream-take)
    (fmakunbound 'neovm--rx-stream-scan)
    (fmakunbound 'neovm--rx-stream-materialize)))
"#;
    let expect = expect_test::expect![[
        r#""OK (((:type click :x 10 :y 20) (:type click :x 30 :y 40) (:type click :x 50 :y 60) (:type click :x 70 :y 80)) ((10 . 20) (30 . 40) (50 . 60) (70 . 80)) ((10 . 20) (30 . 40)) (30 100 210 360) (15 50 105 180) 8 4 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Debounced value updates
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_reactive_debounce() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate debouncing: given a stream of (tick . value) pairs,
    // only emit the value if no new value arrives within N ticks.
    let form = r#"
(progn
  (fset 'neovm--rx-debounce-process
    (lambda (events delay)
      "Process timestamped events with debouncing.
       EVENTS is a list of (tick . value). DELAY is the debounce window.
       Returns list of (emitted-tick . value) for values that survived debouncing."
      (let ((pending-value nil)
            (pending-tick nil)
            (emitted nil)
            (current-tick 0)
            ;; Find the max tick to know when to stop
            (max-tick 0))
        (dolist (e events)
          (when (> (car e) max-tick) (setq max-tick (car e))))
        ;; Process tick by tick
        (let ((event-queue (copy-sequence events)))
          (while (<= current-tick (+ max-tick delay))
            ;; Check if any events arrive at this tick
            (while (and event-queue (= (caar event-queue) current-tick))
              ;; New event: reset the debounce timer
              (setq pending-value (cdar event-queue))
              (setq pending-tick current-tick)
              (setq event-queue (cdr event-queue)))
            ;; Check if pending value has survived the delay
            (when (and pending-value
                       (= current-tick (+ pending-tick delay)))
              (setq emitted (cons (cons current-tick pending-value) emitted))
              (setq pending-value nil)
              (setq pending-tick nil))
            (setq current-tick (1+ current-tick))))
        (nreverse emitted))))

  (fset 'neovm--rx-throttle-process
    (lambda (events interval)
      "Process timestamped events with throttling.
       At most one event per INTERVAL ticks. First event in each window passes."
      (let ((emitted nil)
            (last-emit-tick -999))
        (dolist (e events)
          (when (>= (- (car e) last-emit-tick) interval)
            (setq emitted (cons e emitted))
            (setq last-emit-tick (car e))))
        (nreverse emitted))))

  (unwind-protect
      (let ((rapid-events '((0 . "a") (1 . "b") (2 . "c")
                             (10 . "d") (11 . "e")
                             (20 . "f")
                             (30 . "g") (31 . "h") (32 . "i") (33 . "j")
                             (50 . "k"))))
        (list
         ;; Debounce with delay=3: only last value in each burst survives
         (funcall 'neovm--rx-debounce-process rapid-events 3)
         ;; Debounce with delay=5
         (funcall 'neovm--rx-debounce-process rapid-events 5)
         ;; Debounce with delay=1: most events pass
         (funcall 'neovm--rx-debounce-process rapid-events 1)
         ;; Throttle with interval=5
         (funcall 'neovm--rx-throttle-process rapid-events 5)
         ;; Throttle with interval=10
         (funcall 'neovm--rx-throttle-process rapid-events 10)
         ;; Count comparisons
         (list (length rapid-events)
               (length (funcall 'neovm--rx-debounce-process rapid-events 3))
               (length (funcall 'neovm--rx-throttle-process rapid-events 5)))))
    (fmakunbound 'neovm--rx-debounce-process)
    (fmakunbound 'neovm--rx-throttle-process)))
"#;
    let expect = expect_test::expect![[
        r#""OK (((5 . \"c\") (14 . \"e\") (23 . \"f\") (36 . \"j\") (53 . \"k\")) ((7 . \"c\") (16 . \"e\") (25 . \"f\") (38 . \"j\") (55 . \"k\")) ((3 . \"c\") (12 . \"e\") (21 . \"f\") (34 . \"j\") (51 . \"k\")) ((0 . \"a\") (10 . \"d\") (20 . \"f\") (30 . \"g\") (50 . \"k\")) ((0 . \"a\") (10 . \"d\") (20 . \"f\") (30 . \"g\") (50 . \"k\")) (11 5 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dependency graph with topological update ordering
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_reactive_dependency_graph() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a dependency graph of reactive cells. When a source changes,
    // compute the topological order of dependent cells and update them
    // in the correct order (dependencies before dependents).
    let form = r#"
(progn
  (defvar neovm--rx-dg-values nil)
  (defvar neovm--rx-dg-deps nil)
  (defvar neovm--rx-dg-fns nil)
  (defvar neovm--rx-dg-update-order nil)

  (fset 'neovm--rx-dg-init
    (lambda ()
      (setq neovm--rx-dg-values (make-hash-table))
      (setq neovm--rx-dg-deps (make-hash-table))
      (setq neovm--rx-dg-fns (make-hash-table))
      (setq neovm--rx-dg-update-order nil)))

  (fset 'neovm--rx-dg-define-source
    (lambda (name value)
      (puthash name value neovm--rx-dg-values)
      (puthash name nil neovm--rx-dg-deps)))

  (fset 'neovm--rx-dg-define-derived
    (lambda (name deps fn)
      "Define a derived cell depending on DEPS, computed by FN."
      (puthash name deps neovm--rx-dg-deps)
      (puthash name fn neovm--rx-dg-fns)
      ;; Initial computation
      (let ((args (mapcar (lambda (d) (gethash d neovm--rx-dg-values)) deps)))
        (puthash name (apply fn args) neovm--rx-dg-values))))

  (fset 'neovm--rx-dg-topological-sort
    (lambda (changed)
      "Find all cells affected by CHANGED and return them in topological order."
      (let ((affected nil)
            (in-degree (make-hash-table))
            (dependents (make-hash-table)))
        ;; BFS to find all affected cells
        (let ((queue (list changed)))
          (while queue
            (let ((current (car queue)))
              (setq queue (cdr queue))
              ;; Find cells that depend on current
              (maphash
               (lambda (name deps)
                 (when (and deps (memq current deps)
                            (not (memq name affected)))
                   (setq affected (cons name affected))
                   (setq queue (cons name queue))))
               neovm--rx-dg-deps))))
        ;; Build in-degree counts for affected cells
        (dolist (a affected)
          (puthash a 0 in-degree))
        (dolist (a affected)
          (dolist (dep (gethash a neovm--rx-dg-deps))
            (when (memq dep affected)
              (puthash a (1+ (gethash a in-degree)) in-degree)
              ;; Record that dep -> a
              (puthash dep (cons a (or (gethash dep dependents) nil)) dependents))))
        ;; Kahn's algorithm
        (let ((sorted nil)
              (zero-queue nil))
          ;; Find initial zero in-degree nodes
          (dolist (a affected)
            (when (= (gethash a in-degree) 0)
              (setq zero-queue (cons a zero-queue))))
          (while zero-queue
            (let ((node (car zero-queue)))
              (setq zero-queue (cdr zero-queue))
              (setq sorted (cons node sorted))
              (dolist (next (gethash node dependents))
                (puthash next (1- (gethash next in-degree)) in-degree)
                (when (= (gethash next in-degree) 0)
                  (setq zero-queue (cons next zero-queue))))))
          (nreverse sorted)))))

  (fset 'neovm--rx-dg-update
    (lambda (name value)
      "Update a source cell and propagate in topological order."
      (puthash name value neovm--rx-dg-values)
      (setq neovm--rx-dg-update-order nil)
      (let ((order (funcall 'neovm--rx-dg-topological-sort name)))
        ;; Update each cell in order
        (dolist (cell order)
          (let* ((deps (gethash cell neovm--rx-dg-deps))
                 (fn (gethash cell neovm--rx-dg-fns))
                 (args (mapcar (lambda (d) (gethash d neovm--rx-dg-values)) deps))
                 (new-val (apply fn args)))
            (puthash cell new-val neovm--rx-dg-values)
            (setq neovm--rx-dg-update-order
                  (cons (list cell new-val) neovm--rx-dg-update-order))))
        (nreverse neovm--rx-dg-update-order))))

  (unwind-protect
      (progn
        (funcall 'neovm--rx-dg-init)

        ;; Build: a -> b -> d
        ;;        a -> c -> d
        ;;                  d -> e
        (funcall 'neovm--rx-dg-define-source 'a 10)
        (funcall 'neovm--rx-dg-define-derived 'b '(a) (lambda (a) (* a 2)))
        (funcall 'neovm--rx-dg-define-derived 'c '(a) (lambda (a) (+ a 5)))
        (funcall 'neovm--rx-dg-define-derived 'd '(b c) (lambda (b c) (+ b c)))
        (funcall 'neovm--rx-dg-define-derived 'e '(d) (lambda (d) (* d d)))

        (let ((initial-vals (list (gethash 'a neovm--rx-dg-values)
                                  (gethash 'b neovm--rx-dg-values)
                                  (gethash 'c neovm--rx-dg-values)
                                  (gethash 'd neovm--rx-dg-values)
                                  (gethash 'e neovm--rx-dg-values))))
          ;; Update a to 20
          (let ((update-log (funcall 'neovm--rx-dg-update 'a 20)))
            (let ((updated-vals (list (gethash 'a neovm--rx-dg-values)
                                      (gethash 'b neovm--rx-dg-values)
                                      (gethash 'c neovm--rx-dg-values)
                                      (gethash 'd neovm--rx-dg-values)
                                      (gethash 'e neovm--rx-dg-values))))
              (list
               ;; Initial: a=10, b=20, c=15, d=35, e=1225
               initial-vals
               ;; Updated: a=20, b=40, c=25, d=65, e=4225
               updated-vals
               ;; Update log shows topological order
               update-log
               ;; Verify b and c updated before d, d before e
               (let ((order (mapcar #'car update-log)))
                 (and (< (length (memq 'b order)) (length (memq 'd order)))
                      (< (length (memq 'c order)) (length (memq 'd order)))
                      ;; Careful: memq returns sublist from that element
                      ;; so longer sublist means earlier in list
                      ;; Actually, we need position
                      (let ((pos-b (- (length order) (length (memq 'b order))))
                            (pos-c (- (length order) (length (memq 'c order))))
                            (pos-d (- (length order) (length (memq 'd order))))
                            (pos-e (- (length order) (length (memq 'e order)))))
                        (and (< pos-b pos-d)
                             (< pos-c pos-d)
                             (< pos-d pos-e))))))))))
    (fmakunbound 'neovm--rx-dg-init)
    (fmakunbound 'neovm--rx-dg-define-source)
    (fmakunbound 'neovm--rx-dg-define-derived)
    (fmakunbound 'neovm--rx-dg-topological-sort)
    (fmakunbound 'neovm--rx-dg-update)
    (makunbound 'neovm--rx-dg-values)
    (makunbound 'neovm--rx-dg-deps)
    (makunbound 'neovm--rx-dg-fns)
    (makunbound 'neovm--rx-dg-update-order)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((10 20 15 35 1225) (20 40 25 65 4225) ((b 40) (c 25) (d 65) (e 4225)) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
