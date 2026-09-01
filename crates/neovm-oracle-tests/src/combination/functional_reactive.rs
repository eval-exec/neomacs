//! Oracle parity tests for functional reactive programming concepts in Elisp.
//!
//! Covers signals as closures, derived signals, signal combinators (map,
//! filter, merge, scan), event streams, reactive counter with
//! increment/decrement, and reactive form validation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Signals: time-varying values as closures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_frp_signals_as_closures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A signal is a closure that returns the current value.
    // Signals support get, set, and watch (callback on change).
    let form = r#"
(progn
  (fset 'neovm--frp-make-signal
    (lambda (initial)
      "Create a mutable signal. Returns (get-fn set-fn watch-fn)."
      (let ((value initial)
            (watchers nil))
        (list
         ;; get
         (lambda () value)
         ;; set
         (lambda (new-val)
           (let ((old value))
             (setq value new-val)
             (dolist (w watchers)
               (funcall w old new-val))
             new-val))
         ;; watch: returns unwatch function
         (lambda (callback)
           (setq watchers (cons callback watchers))
           (lambda ()
             (setq watchers
                   (let ((result nil))
                     (dolist (w watchers)
                       (unless (eq w callback)
                         (setq result (cons w result))))
                     (nreverse result)))))))))

  (fset 'neovm--frp-sig-get
    (lambda (sig) (funcall (nth 0 sig))))
  (fset 'neovm--frp-sig-set
    (lambda (sig val) (funcall (nth 1 sig) val)))
  (fset 'neovm--frp-sig-watch
    (lambda (sig callback) (funcall (nth 2 sig) callback)))

  (unwind-protect
      (let ((log nil)
            (counter (funcall 'neovm--frp-make-signal 0))
            (name (funcall 'neovm--frp-make-signal "Alice")))
        ;; Watch counter
        (let ((unwatch-counter
               (funcall 'neovm--frp-sig-watch counter
                        (lambda (old new)
                          (setq log (cons (list 'counter old new) log))))))
          ;; Watch name
          (funcall 'neovm--frp-sig-watch name
                   (lambda (old new)
                     (setq log (cons (list 'name old new) log))))

          ;; Set values
          (funcall 'neovm--frp-sig-set counter 1)
          (funcall 'neovm--frp-sig-set counter 2)
          (funcall 'neovm--frp-sig-set name "Bob")
          (funcall 'neovm--frp-sig-set counter 3)

          ;; Unwatch counter
          (funcall unwatch-counter)

          ;; This should NOT be logged for counter
          (funcall 'neovm--frp-sig-set counter 99)
          ;; But name still watched
          (funcall 'neovm--frp-sig-set name "Carol")

          (list
           ;; Current values
           (funcall 'neovm--frp-sig-get counter)  ;; 99
           (funcall 'neovm--frp-sig-get name)     ;; "Carol"
           ;; Log (reversed to chronological)
           (nreverse log)
           ;; Number of log entries: 3 counter + 1 name + 1 name = 5
           ;; (counter at 99 not logged, counter 1,2,3 = 3 entries)
           (length log))))
    (fmakunbound 'neovm--frp-make-signal)
    (fmakunbound 'neovm--frp-sig-get)
    (fmakunbound 'neovm--frp-sig-set)
    (fmakunbound 'neovm--frp-sig-watch)))
"#;
    let expect = expect_test::expect![[
        r#""OK (99 \"Carol\" ((counter 0 1) (counter 1 2) (name \"Alice\" \"Bob\") (counter 2 3) (name \"Bob\" \"Carol\")) 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Derived signals: computed from other signals
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_frp_derived_signals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Derived signals auto-update when their source signals change
    let form = r#"
(progn
  (defvar neovm--frp-d-signals nil "Hash: name -> (value . watchers)")

  (fset 'neovm--frp-d-create
    (lambda (name initial)
      "Create a named signal."
      (puthash name (cons initial nil) neovm--frp-d-signals)
      name))

  (fset 'neovm--frp-d-get
    (lambda (name)
      (car (gethash name neovm--frp-d-signals))))

  (fset 'neovm--frp-d-set
    (lambda (name value)
      (let ((cell (gethash name neovm--frp-d-signals)))
        (let ((old (car cell)))
          (setcar cell value)
          ;; Notify watchers
          (dolist (w (cdr cell))
            (funcall w old value))))))

  (fset 'neovm--frp-d-watch
    (lambda (name callback)
      (let ((cell (gethash name neovm--frp-d-signals)))
        (setcdr cell (cons callback (cdr cell))))))

  (fset 'neovm--frp-d-derive
    (lambda (name sources compute-fn)
      "Create a derived signal from SOURCES using COMPUTE-FN."
      (let ((initial (apply compute-fn
                            (mapcar (lambda (s) (funcall 'neovm--frp-d-get s))
                                    sources))))
        (funcall 'neovm--frp-d-create name initial)
        ;; Watch all sources
        (dolist (src sources)
          (funcall 'neovm--frp-d-watch src
                   (lambda (old new)
                     (let ((new-val (apply compute-fn
                                          (mapcar (lambda (s) (funcall 'neovm--frp-d-get s))
                                                  sources))))
                       (funcall 'neovm--frp-d-set name new-val)))))
        name)))

  (unwind-protect
      (progn
        (setq neovm--frp-d-signals (make-hash-table))

        ;; Source signals
        (funcall 'neovm--frp-d-create 'width 100)
        (funcall 'neovm--frp-d-create 'height 50)

        ;; Derived: area = width * height
        (funcall 'neovm--frp-d-derive 'area '(width height)
                 (lambda (w h) (* w h)))

        ;; Derived: perimeter = 2*(width + height)
        (funcall 'neovm--frp-d-derive 'perimeter '(width height)
                 (lambda (w h) (* 2 (+ w h))))

        ;; Derived: aspect-ratio = width * 100 / height (integer)
        (funcall 'neovm--frp-d-derive 'aspect '(width height)
                 (lambda (w h) (/ (* w 100) h)))

        (let ((initial (list (funcall 'neovm--frp-d-get 'area)
                             (funcall 'neovm--frp-d-get 'perimeter)
                             (funcall 'neovm--frp-d-get 'aspect))))
          ;; Change width
          (funcall 'neovm--frp-d-set 'width 200)

          (let ((after-width (list (funcall 'neovm--frp-d-get 'area)
                                   (funcall 'neovm--frp-d-get 'perimeter)
                                   (funcall 'neovm--frp-d-get 'aspect))))
            ;; Change height
            (funcall 'neovm--frp-d-set 'height 100)

            (list
             ;; Initial: 5000, 300, 200
             initial
             ;; After width=200: 10000, 500, 400
             after-width
             ;; After height=100: 20000, 600, 200
             (funcall 'neovm--frp-d-get 'area)
             (funcall 'neovm--frp-d-get 'perimeter)
             (funcall 'neovm--frp-d-get 'aspect)))))
    (fmakunbound 'neovm--frp-d-create)
    (fmakunbound 'neovm--frp-d-get)
    (fmakunbound 'neovm--frp-d-set)
    (fmakunbound 'neovm--frp-d-watch)
    (fmakunbound 'neovm--frp-d-derive)
    (makunbound 'neovm--frp-d-signals)))
"#;
    let expect = expect_test::expect![[r#""OK ((5000 300 200) (10000 500 400) 20000 600 200)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Signal combinators: map, filter, merge, scan
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_frp_signal_combinators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement signal combinators that operate on event lists
    // (batch processing simulation of FRP)
    let form = r#"
(progn
  ;; Signal combinator: map
  (fset 'neovm--frp-sig-map
    (lambda (fn events)
      "Apply FN to each event value."
      (mapcar fn events)))

  ;; Signal combinator: filter
  (fset 'neovm--frp-sig-filter
    (lambda (pred events)
      "Keep only events satisfying PRED."
      (let ((result nil))
        (dolist (e events)
          (when (funcall pred e) (setq result (cons e result))))
        (nreverse result))))

  ;; Signal combinator: merge (interleave two event streams by timestamp)
  (fset 'neovm--frp-sig-merge
    (lambda (stream-a stream-b)
      "Merge two timestamped event streams. Each event is (time . value).
       Result is sorted by time."
      (let ((merged (append (copy-sequence stream-a) (copy-sequence stream-b))))
        (sort merged (lambda (a b) (< (car a) (car b)))))))

  ;; Signal combinator: scan (fold with intermediate results)
  (fset 'neovm--frp-sig-scan
    (lambda (fn init events)
      "Running fold: return list of accumulated values."
      (let ((acc init) (result nil))
        (dolist (e events)
          (setq acc (funcall fn acc e))
          (setq result (cons acc result)))
        (nreverse result))))

  ;; Signal combinator: distinct (skip consecutive duplicates)
  (fset 'neovm--frp-sig-distinct
    (lambda (events)
      "Remove consecutive duplicate values."
      (let ((result nil) (last-val 'neovm--frp-sentinel))
        (dolist (e events)
          (unless (equal e last-val)
            (setq result (cons e result))
            (setq last-val e)))
        (nreverse result))))

  ;; Signal combinator: window (sliding window of size n)
  (fset 'neovm--frp-sig-window
    (lambda (n events)
      "Produce sliding windows of size N."
      (let ((result nil) (buf nil) (count 0))
        (dolist (e events)
          (setq buf (append buf (list e)))
          (setq count (1+ count))
          (when (> count n)
            (setq buf (cdr buf))
            (setq count n))
          (when (= count n)
            (setq result (cons (copy-sequence buf) result))))
        (nreverse result))))

  (unwind-protect
      (let ((numbers '(1 2 3 4 5 6 7 8 9 10))
            (stream-a '((1 . click) (3 . click) (5 . click)))
            (stream-b '((2 . move) (4 . move) (6 . move))))
        (list
         ;; map: double each
         (funcall 'neovm--frp-sig-map (lambda (x) (* x 2)) numbers)
         ;; filter: keep even
         (funcall 'neovm--frp-sig-filter (lambda (x) (= (% x 2) 0)) numbers)
         ;; scan: running sum
         (funcall 'neovm--frp-sig-scan #'+ 0 numbers)
         ;; scan: running max
         (funcall 'neovm--frp-sig-scan #'max 0 numbers)
         ;; merge two streams
         (funcall 'neovm--frp-sig-merge stream-a stream-b)
         ;; distinct
         (funcall 'neovm--frp-sig-distinct '(1 1 2 2 2 3 1 1 4 4))
         ;; window of size 3
         (funcall 'neovm--frp-sig-window 3 numbers)
         ;; Compose: filter even, map to square, scan running sum
         (funcall 'neovm--frp-sig-scan #'+ 0
                  (funcall 'neovm--frp-sig-map (lambda (x) (* x x))
                           (funcall 'neovm--frp-sig-filter
                                    (lambda (x) (= (% x 2) 0))
                                    numbers)))
         ;; Compose: map to mod 3, distinct
         (funcall 'neovm--frp-sig-distinct
                  (funcall 'neovm--frp-sig-map (lambda (x) (% x 3)) numbers))))
    (fmakunbound 'neovm--frp-sig-map)
    (fmakunbound 'neovm--frp-sig-filter)
    (fmakunbound 'neovm--frp-sig-merge)
    (fmakunbound 'neovm--frp-sig-scan)
    (fmakunbound 'neovm--frp-sig-distinct)
    (fmakunbound 'neovm--frp-sig-window)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((2 4 6 8 10 12 14 16 18 20) (2 4 6 8 10) (1 3 6 10 15 21 28 36 45 55) (1 2 3 4 5 6 7 8 9 10) ((1 . click) (2 . move) (3 . click) (4 . move) (5 . click) (6 . move)) (1 2 3 1 4) ((1 2 3) (2 3 4) (3 4 5) (4 5 6) (5 6 7) (6 7 8) (7 8 9) (8 9 10)) (4 20 56 120 220) (1 2 0 1 2 0 1 2 0 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Event streams with handlers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_frp_event_streams() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Event system with named channels, subscribe/emit, and processing pipeline
    let form = r#"
(progn
  (defvar neovm--frp-es-channels nil)
  (defvar neovm--frp-es-log nil)

  (fset 'neovm--frp-es-create-channel
    (lambda (name)
      "Create an event channel."
      (puthash name nil neovm--frp-es-channels)
      name))

  (fset 'neovm--frp-es-subscribe
    (lambda (channel handler)
      "Subscribe HANDLER to CHANNEL."
      (puthash channel
               (cons handler (gethash channel neovm--frp-es-channels))
               neovm--frp-es-channels)))

  (fset 'neovm--frp-es-emit
    (lambda (channel event)
      "Emit EVENT on CHANNEL, calling all handlers."
      (dolist (handler (reverse (gethash channel neovm--frp-es-channels)))
        (funcall handler event))))

  (fset 'neovm--frp-es-pipe
    (lambda (from-channel to-channel transform)
      "Pipe events from FROM-CHANNEL to TO-CHANNEL, applying TRANSFORM."
      (funcall 'neovm--frp-es-subscribe from-channel
               (lambda (event)
                 (let ((transformed (funcall transform event)))
                   (when transformed
                     (funcall 'neovm--frp-es-emit to-channel transformed)))))))

  (unwind-protect
      (progn
        (setq neovm--frp-es-channels (make-hash-table))
        (setq neovm--frp-es-log nil)

        ;; Create channels
        (funcall 'neovm--frp-es-create-channel 'raw-input)
        (funcall 'neovm--frp-es-create-channel 'validated)
        (funcall 'neovm--frp-es-create-channel 'processed)

        ;; Pipeline: raw-input -> validated (filter) -> processed (transform)
        (funcall 'neovm--frp-es-pipe 'raw-input 'validated
                 (lambda (event)
                   (when (and (integerp event) (> event 0))
                     event)))

        (funcall 'neovm--frp-es-pipe 'validated 'processed
                 (lambda (event)
                   (list :value event :squared (* event event))))

        ;; Log processed events
        (funcall 'neovm--frp-es-subscribe 'processed
                 (lambda (event)
                   (setq neovm--frp-es-log (cons event neovm--frp-es-log))))

        ;; Emit various events
        (funcall 'neovm--frp-es-emit 'raw-input 5)      ;; valid
        (funcall 'neovm--frp-es-emit 'raw-input -3)     ;; filtered out
        (funcall 'neovm--frp-es-emit 'raw-input "bad")  ;; filtered out
        (funcall 'neovm--frp-es-emit 'raw-input 10)     ;; valid
        (funcall 'neovm--frp-es-emit 'raw-input 0)      ;; filtered out
        (funcall 'neovm--frp-es-emit 'raw-input 7)      ;; valid

        (list
         ;; Number of processed events (only valid ones pass)
         (length neovm--frp-es-log)
         ;; Log in chronological order
         (nreverse neovm--frp-es-log)
         ;; Verify squares are correct
         (let ((ok t))
           (dolist (entry neovm--frp-es-log)
             (unless (= (plist-get entry :squared)
                        (* (plist-get entry :value) (plist-get entry :value)))
               (setq ok nil)))
           ok)))
    (fmakunbound 'neovm--frp-es-create-channel)
    (fmakunbound 'neovm--frp-es-subscribe)
    (fmakunbound 'neovm--frp-es-emit)
    (fmakunbound 'neovm--frp-es-pipe)
    (makunbound 'neovm--frp-es-channels)
    (makunbound 'neovm--frp-es-log)))
"#;
    let expect = expect_test::expect![[
        r#""OK (3 ((:value 5 :squared 25) (:value 10 :squared 100) (:value 7 :squared 49)) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: reactive counter with increment/decrement/reset
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_frp_reactive_counter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A reactive counter system with multiple counters, derived totals,
    // min/max tracking, and history
    let form = r#"
(progn
  (defvar neovm--frp-rc-counters nil)
  (defvar neovm--frp-rc-history nil)
  (defvar neovm--frp-rc-derived nil)

  (fset 'neovm--frp-rc-create
    (lambda (name initial)
      "Create a named counter."
      (puthash name initial neovm--frp-rc-counters)
      ;; History: list of (action . value) entries
      (puthash name (list (cons 'init initial)) neovm--frp-rc-history)
      name))

  (fset 'neovm--frp-rc-get
    (lambda (name) (gethash name neovm--frp-rc-counters)))

  (fset 'neovm--frp-rc-update
    (lambda (name action delta)
      "Update counter by DELTA, record history, recompute derived."
      (let ((old (gethash name neovm--frp-rc-counters))
            (new (+ (gethash name neovm--frp-rc-counters) delta)))
        (puthash name new neovm--frp-rc-counters)
        (puthash name
                 (cons (cons action new) (gethash name neovm--frp-rc-history))
                 neovm--frp-rc-history)
        ;; Recompute derived values
        (funcall 'neovm--frp-rc-recompute)
        new)))

  (fset 'neovm--frp-rc-inc
    (lambda (name) (funcall 'neovm--frp-rc-update name 'inc 1)))
  (fset 'neovm--frp-rc-dec
    (lambda (name) (funcall 'neovm--frp-rc-update name 'dec -1)))
  (fset 'neovm--frp-rc-add
    (lambda (name n) (funcall 'neovm--frp-rc-update name 'add n)))

  (fset 'neovm--frp-rc-reset
    (lambda (name)
      (puthash name 0 neovm--frp-rc-counters)
      (puthash name
               (cons (cons 'reset 0) (gethash name neovm--frp-rc-history))
               neovm--frp-rc-history)
      (funcall 'neovm--frp-rc-recompute)))

  (fset 'neovm--frp-rc-recompute
    (lambda ()
      "Recompute all derived values."
      (let ((total 0) (min-val 999999) (max-val -999999) (count 0))
        (maphash (lambda (name val)
                   (setq total (+ total val))
                   (when (< val min-val) (setq min-val val))
                   (when (> val max-val) (setq max-val val))
                   (setq count (1+ count)))
                 neovm--frp-rc-counters)
        (setq neovm--frp-rc-derived
              (list :total total :min min-val :max max-val :count count
                    :avg (if (> count 0) (/ total count) 0))))))

  (unwind-protect
      (progn
        (setq neovm--frp-rc-counters (make-hash-table))
        (setq neovm--frp-rc-history (make-hash-table))
        (setq neovm--frp-rc-derived nil)

        ;; Create counters
        (funcall 'neovm--frp-rc-create 'clicks 0)
        (funcall 'neovm--frp-rc-create 'views 100)
        (funcall 'neovm--frp-rc-create 'errors 0)

        ;; Perform operations
        (funcall 'neovm--frp-rc-inc 'clicks)
        (funcall 'neovm--frp-rc-inc 'clicks)
        (funcall 'neovm--frp-rc-inc 'clicks)
        (funcall 'neovm--frp-rc-add 'views 50)
        (funcall 'neovm--frp-rc-inc 'errors)
        (funcall 'neovm--frp-rc-dec 'errors)
        (funcall 'neovm--frp-rc-add 'clicks 10)

        (let ((state1 (list
                       (funcall 'neovm--frp-rc-get 'clicks)
                       (funcall 'neovm--frp-rc-get 'views)
                       (funcall 'neovm--frp-rc-get 'errors)
                       (copy-sequence neovm--frp-rc-derived))))
          ;; Reset clicks
          (funcall 'neovm--frp-rc-reset 'clicks)

          (list
           state1
           ;; After reset
           (funcall 'neovm--frp-rc-get 'clicks)
           ;; Derived values updated
           (plist-get neovm--frp-rc-derived :total)
           (plist-get neovm--frp-rc-derived :min)
           (plist-get neovm--frp-rc-derived :max)
           ;; Click history length
           (length (gethash 'clicks neovm--frp-rc-history))
           ;; First and last history entries for clicks
           (car (last (gethash 'clicks neovm--frp-rc-history)))  ;; init
           (car (gethash 'clicks neovm--frp-rc-history)))))      ;; reset
    (fmakunbound 'neovm--frp-rc-create)
    (fmakunbound 'neovm--frp-rc-get)
    (fmakunbound 'neovm--frp-rc-update)
    (fmakunbound 'neovm--frp-rc-inc)
    (fmakunbound 'neovm--frp-rc-dec)
    (fmakunbound 'neovm--frp-rc-add)
    (fmakunbound 'neovm--frp-rc-reset)
    (fmakunbound 'neovm--frp-rc-recompute)
    (makunbound 'neovm--frp-rc-counters)
    (makunbound 'neovm--frp-rc-history)
    (makunbound 'neovm--frp-rc-derived)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((13 150 0 (:total 163 :min 0 :max 150 :count 3 :avg 54)) 0 150 0 150 6 (init . 0) (reset . 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: reactive form validation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_frp_reactive_form_validation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Reactive form with fields, validators, and derived validity state.
    // Changing any field triggers re-validation of that field and the
    // overall form validity.
    let form = r#"
(progn
  (defvar neovm--frp-fv-fields nil)
  (defvar neovm--frp-fv-validators nil)
  (defvar neovm--frp-fv-errors nil)
  (defvar neovm--frp-fv-valid nil)
  (defvar neovm--frp-fv-change-log nil)

  (fset 'neovm--frp-fv-add-field
    (lambda (name initial validator)
      "Add a form field with initial value and validator function.
       Validator returns nil for valid, or error message string."
      (puthash name initial neovm--frp-fv-fields)
      (puthash name validator neovm--frp-fv-validators)
      (puthash name nil neovm--frp-fv-errors)
      ;; Validate initial value
      (funcall 'neovm--frp-fv-validate-field name)))

  (fset 'neovm--frp-fv-validate-field
    (lambda (name)
      "Validate a single field and update errors."
      (let* ((value (gethash name neovm--frp-fv-fields))
             (validator (gethash name neovm--frp-fv-validators))
             (error (funcall validator value)))
        (puthash name error neovm--frp-fv-errors)
        (funcall 'neovm--frp-fv-recompute-validity))))

  (fset 'neovm--frp-fv-recompute-validity
    (lambda ()
      "Recompute overall form validity."
      (let ((valid t))
        (maphash (lambda (name error)
                   (when error (setq valid nil)))
                 neovm--frp-fv-errors)
        (setq neovm--frp-fv-valid valid))))

  (fset 'neovm--frp-fv-set-field
    (lambda (name value)
      "Set field value and re-validate."
      (let ((old (gethash name neovm--frp-fv-fields)))
        (puthash name value neovm--frp-fv-fields)
        (funcall 'neovm--frp-fv-validate-field name)
        (setq neovm--frp-fv-change-log
              (cons (list name old value
                          (gethash name neovm--frp-fv-errors)
                          neovm--frp-fv-valid)
                    neovm--frp-fv-change-log)))))

  (fset 'neovm--frp-fv-snapshot
    (lambda ()
      "Get form state snapshot."
      (let ((field-vals nil) (field-errors nil))
        (maphash (lambda (k v) (setq field-vals (cons (cons k v) field-vals)))
                 neovm--frp-fv-fields)
        (maphash (lambda (k v) (setq field-errors (cons (cons k v) field-errors)))
                 neovm--frp-fv-errors)
        (list :valid neovm--frp-fv-valid
              :values (sort field-vals (lambda (a b) (string< (symbol-name (car a))
                                                                (symbol-name (car b)))))
              :errors (sort field-errors (lambda (a b) (string< (symbol-name (car a))
                                                                  (symbol-name (car b)))))))))

  (unwind-protect
      (progn
        (setq neovm--frp-fv-fields (make-hash-table))
        (setq neovm--frp-fv-validators (make-hash-table))
        (setq neovm--frp-fv-errors (make-hash-table))
        (setq neovm--frp-fv-valid t)
        (setq neovm--frp-fv-change-log nil)

        ;; Define form fields with validators
        (funcall 'neovm--frp-fv-add-field 'username ""
                 (lambda (v)
                   (cond
                    ((not (stringp v)) "Must be a string")
                    ((< (length v) 3) "Too short (min 3)")
                    ((> (length v) 20) "Too long (max 20)")
                    (t nil))))

        (funcall 'neovm--frp-fv-add-field 'age 0
                 (lambda (v)
                   (cond
                    ((not (integerp v)) "Must be integer")
                    ((< v 1) "Must be positive")
                    ((> v 150) "Unrealistic age")
                    (t nil))))

        (funcall 'neovm--frp-fv-add-field 'email ""
                 (lambda (v)
                   (cond
                    ((not (stringp v)) "Must be a string")
                    ((= (length v) 0) "Required")
                    ((not (string-match-p "@" v)) "Must contain @")
                    (t nil))))

        ;; Initial state: all invalid (empty fields)
        (let ((initial-valid neovm--frp-fv-valid))

          ;; Fill in valid username
          (funcall 'neovm--frp-fv-set-field 'username "alice")
          (let ((after-username neovm--frp-fv-valid))

            ;; Fill in valid age
            (funcall 'neovm--frp-fv-set-field 'age 25)
            (let ((after-age neovm--frp-fv-valid))

              ;; Fill in valid email
              (funcall 'neovm--frp-fv-set-field 'email "alice@example.com")
              (let ((after-email neovm--frp-fv-valid))

                ;; Now set invalid username
                (funcall 'neovm--frp-fv-set-field 'username "ab")
                (let ((after-bad-username neovm--frp-fv-valid))

                  ;; Fix username
                  (funcall 'neovm--frp-fv-set-field 'username "alice2")

                  (list
                   ;; Validity progression
                   initial-valid         ;; nil (all empty)
                   after-username        ;; nil (age/email still invalid)
                   after-age             ;; nil (email still invalid)
                   after-email           ;; t (all valid now)
                   after-bad-username    ;; nil (username too short)
                   neovm--frp-fv-valid   ;; t (fixed)
                   ;; Change log length
                   (length neovm--frp-fv-change-log)
                   ;; Final snapshot
                   (funcall 'neovm--frp-fv-snapshot)))))))
    (fmakunbound 'neovm--frp-fv-add-field)
    (fmakunbound 'neovm--frp-fv-validate-field)
    (fmakunbound 'neovm--frp-fv-recompute-validity)
    (fmakunbound 'neovm--frp-fv-set-field)
    (fmakunbound 'neovm--frp-fv-snapshot)
    (makunbound 'neovm--frp-fv-fields)
    (makunbound 'neovm--frp-fv-validators)
    (makunbound 'neovm--frp-fv-errors)
    (makunbound 'neovm--frp-fv-valid)
    (makunbound 'neovm--frp-fv-change-log)))
"#;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
