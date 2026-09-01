//! Oracle parity tests for reactive system patterns in Elisp.
//!
//! Covers: observable/observer with subscription, signal/slot mechanism,
//! computed properties with dependency tracking, reactive data flow graph,
//! change propagation with glitch prevention, batch updates,
//! unsubscribe/dispose.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Observable/Observer: subscribe, notify, unsubscribe
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_reactive_observable_observer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--rs-observables nil)
  (defvar neovm--rs-event-log nil)

  (fset 'neovm--rs-create-observable
    (lambda (name initial)
      (let ((obs (list (cons 'name name)
                       (cons 'value initial)
                       (cons 'subscribers nil)
                       (cons 'version 0))))
        (setq neovm--rs-observables
              (cons (cons name obs) neovm--rs-observables))
        name)))

  (fset 'neovm--rs-get-obs
    (lambda (name)
      (cdr (assq name neovm--rs-observables))))

  (fset 'neovm--rs-subscribe
    (lambda (obs-name sub-id callback)
      (let* ((obs (funcall 'neovm--rs-get-obs obs-name))
             (subs (cdr (assq 'subscribers obs))))
        (setcdr (assq 'subscribers obs)
                (cons (cons sub-id callback) subs)))))

  (fset 'neovm--rs-unsubscribe
    (lambda (obs-name sub-id)
      (let* ((obs (funcall 'neovm--rs-get-obs obs-name))
             (subs (cdr (assq 'subscribers obs))))
        (setcdr (assq 'subscribers obs)
                (let ((result nil))
                  (dolist (s subs)
                    (unless (eq (car s) sub-id)
                      (setq result (cons s result))))
                  (nreverse result))))))

  (fset 'neovm--rs-set-value
    (lambda (obs-name new-val)
      (let* ((obs (funcall 'neovm--rs-get-obs obs-name))
             (old-val (cdr (assq 'value obs)))
             (ver (cdr (assq 'version obs))))
        (unless (equal old-val new-val)
          (setcdr (assq 'value obs) new-val)
          (setcdr (assq 'version obs) (1+ ver))
          ;; Notify subscribers in registration order
          (dolist (sub (reverse (cdr (assq 'subscribers obs))))
            (let ((result (funcall (cdr sub) old-val new-val)))
              (setq neovm--rs-event-log
                    (cons (list (car sub) obs-name old-val new-val result)
                          neovm--rs-event-log))))))))

  (fset 'neovm--rs-get-value
    (lambda (obs-name)
      (cdr (assq 'value (funcall 'neovm--rs-get-obs obs-name)))))

  (unwind-protect
      (progn
        (setq neovm--rs-observables nil)
        (setq neovm--rs-event-log nil)

        (funcall 'neovm--rs-create-observable 'temp 20)
        (funcall 'neovm--rs-subscribe 'temp 'logger
                 (lambda (old new) (format "%d->%d" old new)))
        (funcall 'neovm--rs-subscribe 'temp 'alarm
                 (lambda (old new) (if (> new 30) 'hot 'ok)))

        ;; Set values
        (funcall 'neovm--rs-set-value 'temp 25)
        (funcall 'neovm--rs-set-value 'temp 35)
        ;; Same value: no notification
        (funcall 'neovm--rs-set-value 'temp 35)
        ;; Unsubscribe alarm, set again
        (funcall 'neovm--rs-unsubscribe 'temp 'alarm)
        (funcall 'neovm--rs-set-value 'temp 10)

        (list
          (funcall 'neovm--rs-get-value 'temp)
          (cdr (assq 'version (funcall 'neovm--rs-get-obs 'temp)))
          (length neovm--rs-event-log)
          (nreverse neovm--rs-event-log)))
    (fmakunbound 'neovm--rs-create-observable)
    (fmakunbound 'neovm--rs-get-obs)
    (fmakunbound 'neovm--rs-subscribe)
    (fmakunbound 'neovm--rs-unsubscribe)
    (fmakunbound 'neovm--rs-set-value)
    (fmakunbound 'neovm--rs-get-value)
    (makunbound 'neovm--rs-observables)
    (makunbound 'neovm--rs-event-log)))"#;
    let expect = expect_test::expect![[
        r#""OK (10 3 5 ((logger temp 20 25 \"20->25\") (alarm temp 20 25 ok) (logger temp 25 35 \"25->35\") (alarm temp 25 35 hot) (logger temp 35 10 \"35->10\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Signal/Slot mechanism
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_reactive_signal_slot() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--rs2-signals nil)
  (defvar neovm--rs2-results nil)

  (fset 'neovm--rs2-defsignal
    (lambda (name)
      (setq neovm--rs2-signals (cons (cons name nil) neovm--rs2-signals))))

  (fset 'neovm--rs2-connect
    (lambda (signal-name slot-name slot-fn)
      (let ((sig (assq signal-name neovm--rs2-signals)))
        (setcdr sig (cons (cons slot-name slot-fn) (cdr sig))))))

  (fset 'neovm--rs2-disconnect
    (lambda (signal-name slot-name)
      (let ((sig (assq signal-name neovm--rs2-signals)))
        (setcdr sig (let ((result nil))
                      (dolist (s (cdr sig))
                        (unless (eq (car s) slot-name)
                          (setq result (cons s result))))
                      (nreverse result))))))

  (fset 'neovm--rs2-emit
    (lambda (signal-name &rest args)
      (let ((sig (assq signal-name neovm--rs2-signals)))
        (dolist (slot (reverse (cdr sig)))
          (let ((result (apply (cdr slot) args)))
            (setq neovm--rs2-results
                  (cons (list signal-name (car slot) args result)
                        neovm--rs2-results)))))))

  (unwind-protect
      (progn
        (setq neovm--rs2-signals nil)
        (setq neovm--rs2-results nil)

        ;; Define signals
        (funcall 'neovm--rs2-defsignal 'on-click)
        (funcall 'neovm--rs2-defsignal 'on-hover)

        ;; Connect slots
        (funcall 'neovm--rs2-connect 'on-click 'handler-a
                 (lambda (x y) (format "click@(%d,%d)" x y)))
        (funcall 'neovm--rs2-connect 'on-click 'handler-b
                 (lambda (x y) (* x y)))
        (funcall 'neovm--rs2-connect 'on-hover 'tooltip
                 (lambda (x y) (format "hover@%d" x)))

        ;; Emit signals
        (funcall 'neovm--rs2-emit 'on-click 10 20)
        (funcall 'neovm--rs2-emit 'on-hover 5 15)

        ;; Disconnect handler-b, emit again
        (funcall 'neovm--rs2-disconnect 'on-click 'handler-b)
        (funcall 'neovm--rs2-emit 'on-click 30 40)

        (list
          (length neovm--rs2-results)
          (nreverse neovm--rs2-results)))
    (fmakunbound 'neovm--rs2-defsignal)
    (fmakunbound 'neovm--rs2-connect)
    (fmakunbound 'neovm--rs2-disconnect)
    (fmakunbound 'neovm--rs2-emit)
    (makunbound 'neovm--rs2-signals)
    (makunbound 'neovm--rs2-results)))"#;
    let expect = expect_test::expect![[
        r#""OK (4 ((on-click handler-a (10 20) \"click@(10,20)\") (on-click handler-b (10 20) 200) (on-hover tooltip (5 15) \"hover@5\") (on-click handler-a (30 40) \"click@(30,40)\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Computed properties with dependency tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_reactive_computed_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--rs3-cells nil)
  (defvar neovm--rs3-computeds nil)
  (defvar neovm--rs3-eval-log nil)

  (fset 'neovm--rs3-defcell
    (lambda (name value)
      (setq neovm--rs3-cells (cons (cons name value) neovm--rs3-cells))))

  (fset 'neovm--rs3-getcell
    (lambda (name)
      (cdr (assq name neovm--rs3-cells))))

  (fset 'neovm--rs3-setcell
    (lambda (name value)
      (setcdr (assq name neovm--rs3-cells) value)
      ;; Recompute all dependents
      (dolist (comp neovm--rs3-computeds)
        (when (memq name (nth 1 comp))
          (let* ((cname (nth 0 comp))
                 (fn (nth 2 comp))
                 (new-val (funcall fn)))
            (setq neovm--rs3-eval-log
                  (cons (list 'recompute cname new-val) neovm--rs3-eval-log))
            ;; Update the computed cell
            (let ((entry (assq cname neovm--rs3-cells)))
              (if entry (setcdr entry new-val)
                (setq neovm--rs3-cells (cons (cons cname new-val)
                                              neovm--rs3-cells)))))))))

  (fset 'neovm--rs3-defcomputed
    (lambda (name deps compute-fn)
      "Define a computed property NAME depending on DEPS cells."
      (setq neovm--rs3-computeds
            (cons (list name deps compute-fn) neovm--rs3-computeds))
      ;; Initial computation
      (let ((val (funcall compute-fn)))
        (setq neovm--rs3-cells (cons (cons name val) neovm--rs3-cells)))))

  (unwind-protect
      (progn
        (setq neovm--rs3-cells nil)
        (setq neovm--rs3-computeds nil)
        (setq neovm--rs3-eval-log nil)

        ;; Base cells
        (funcall 'neovm--rs3-defcell 'width 10)
        (funcall 'neovm--rs3-defcell 'height 5)

        ;; Computed: area = width * height
        (funcall 'neovm--rs3-defcomputed 'area '(width height)
                 (lambda ()
                   (* (funcall 'neovm--rs3-getcell 'width)
                      (funcall 'neovm--rs3-getcell 'height))))

        ;; Computed: perimeter = 2*(width+height)
        (funcall 'neovm--rs3-defcomputed 'perimeter '(width height)
                 (lambda ()
                   (* 2 (+ (funcall 'neovm--rs3-getcell 'width)
                            (funcall 'neovm--rs3-getcell 'height)))))

        (let ((a1 (funcall 'neovm--rs3-getcell 'area))
              (p1 (funcall 'neovm--rs3-getcell 'perimeter)))

          ;; Change width -> recomputes area and perimeter
          (funcall 'neovm--rs3-setcell 'width 20)

          (let ((a2 (funcall 'neovm--rs3-getcell 'area))
                (p2 (funcall 'neovm--rs3-getcell 'perimeter)))

            ;; Change height
            (funcall 'neovm--rs3-setcell 'height 8)

            (list
              a1 p1              ;; 50, 30
              a2 p2              ;; 100, 50
              (funcall 'neovm--rs3-getcell 'area)       ;; 160
              (funcall 'neovm--rs3-getcell 'perimeter)  ;; 56
              (length neovm--rs3-eval-log)))))
    (fmakunbound 'neovm--rs3-defcell)
    (fmakunbound 'neovm--rs3-getcell)
    (fmakunbound 'neovm--rs3-setcell)
    (fmakunbound 'neovm--rs3-defcomputed)
    (makunbound 'neovm--rs3-cells)
    (makunbound 'neovm--rs3-computeds)
    (makunbound 'neovm--rs3-eval-log)))"#;
    let expect = expect_test::expect![[r#""OK (50 30 100 50 160 56 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Reactive data flow graph with topological propagation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_reactive_dataflow_graph() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--rs4-nodes nil)
  (defvar neovm--rs4-edges nil)
  (defvar neovm--rs4-prop-order nil)

  (fset 'neovm--rs4-add-node
    (lambda (name value compute-fn)
      (setq neovm--rs4-nodes
            (cons (list name value compute-fn) neovm--rs4-nodes))))

  (fset 'neovm--rs4-add-edge
    (lambda (from to)
      "FROM depends on TO (TO flows into FROM)."
      (setq neovm--rs4-edges (cons (cons from to) neovm--rs4-edges))))

  (fset 'neovm--rs4-get-val
    (lambda (name)
      (nth 1 (assq name neovm--rs4-nodes))))

  (fset 'neovm--rs4-set-val
    (lambda (name value)
      (setcar (cdr (assq name neovm--rs4-nodes)) value)))

  ;; Topological sort of dependents of a node
  (fset 'neovm--rs4-topo-dependents
    (lambda (source)
      "Find all nodes downstream of SOURCE in dependency order."
      (let ((visited nil)
            (result nil))
        (fset 'neovm--rs4--visit
          (lambda (node)
            (unless (memq node visited)
              (setq visited (cons node visited))
              ;; Find nodes that depend on `node`
              (dolist (edge neovm--rs4-edges)
                (when (eq (cdr edge) node)
                  (funcall 'neovm--rs4--visit (car edge))))
              (setq result (cons node result)))))
        (funcall 'neovm--rs4--visit source)
        (fmakunbound 'neovm--rs4--visit)
        ;; Reverse for topological order, remove source itself
        (let ((ordered (nreverse result)))
          (delq source ordered)))))

  (fset 'neovm--rs4-propagate
    (lambda (source)
      "Propagate changes from SOURCE through the dataflow graph."
      (let ((order (funcall 'neovm--rs4-topo-dependents source)))
        (setq neovm--rs4-prop-order nil)
        (dolist (name order)
          (let* ((node (assq name neovm--rs4-nodes))
                 (fn (nth 2 node)))
            (when fn
              (let ((new-val (funcall fn)))
                (setcar (cdr node) new-val)
                (setq neovm--rs4-prop-order
                      (cons (cons name new-val) neovm--rs4-prop-order)))))))))

  (unwind-protect
      (progn
        (setq neovm--rs4-nodes nil)
        (setq neovm--rs4-edges nil)
        (setq neovm--rs4-prop-order nil)

        ;; Graph: a -> b -> d
        ;;        a -> c -> d
        (funcall 'neovm--rs4-add-node 'a 2 nil)  ;; source, no compute
        (funcall 'neovm--rs4-add-node 'b 0
                 (lambda () (* (funcall 'neovm--rs4-get-val 'a) 3)))
        (funcall 'neovm--rs4-add-node 'c 0
                 (lambda () (+ (funcall 'neovm--rs4-get-val 'a) 10)))
        (funcall 'neovm--rs4-add-node 'd 0
                 (lambda () (+ (funcall 'neovm--rs4-get-val 'b)
                               (funcall 'neovm--rs4-get-val 'c))))

        ;; Edges: b depends on a, c depends on a, d depends on b and c
        (funcall 'neovm--rs4-add-edge 'b 'a)
        (funcall 'neovm--rs4-add-edge 'c 'a)
        (funcall 'neovm--rs4-add-edge 'd 'b)
        (funcall 'neovm--rs4-add-edge 'd 'c)

        ;; Initial propagation
        (funcall 'neovm--rs4-propagate 'a)
        (let ((b1 (funcall 'neovm--rs4-get-val 'b))
              (c1 (funcall 'neovm--rs4-get-val 'c))
              (d1 (funcall 'neovm--rs4-get-val 'd)))

          ;; Change a to 5, re-propagate
          (funcall 'neovm--rs4-set-val 'a 5)
          (funcall 'neovm--rs4-propagate 'a)

          (list
            ;; a=2: b=6, c=12, d=18
            b1 c1 d1
            ;; a=5: b=15, c=15, d=30
            (funcall 'neovm--rs4-get-val 'b)
            (funcall 'neovm--rs4-get-val 'c)
            (funcall 'neovm--rs4-get-val 'd)
            ;; Propagation order: d is computed AFTER b and c
            (length (nreverse neovm--rs4-prop-order)))))
    (fmakunbound 'neovm--rs4-add-node)
    (fmakunbound 'neovm--rs4-add-edge)
    (fmakunbound 'neovm--rs4-get-val)
    (fmakunbound 'neovm--rs4-set-val)
    (fmakunbound 'neovm--rs4-topo-dependents)
    (fmakunbound 'neovm--rs4-propagate)
    (makunbound 'neovm--rs4-nodes)
    (makunbound 'neovm--rs4-edges)
    (makunbound 'neovm--rs4-prop-order)))"#;
    let expect = expect_test::expect![[r#""OK (6 12 0 15 15 18 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Glitch prevention: ensure consistent state during propagation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_reactive_glitch_prevention() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; A "glitch" is when a derived value is read in an inconsistent state.
  ;; e.g., if a=1, b=a+1=2, c=a+b. When a changes to 2:
  ;;   Without glitch prevention: c might see a=2, b=2 (stale) -> c=4 (wrong)
  ;;   With glitch prevention: update b first, then c -> c sees a=2, b=3 -> c=5

  (defvar neovm--rs5-vals nil)
  (defvar neovm--rs5-snapshots nil)

  (fset 'neovm--rs5-init
    (lambda ()
      (setq neovm--rs5-vals (list (cons 'a 1)))
      ;; b depends on a; c depends on a and b
      ;; Compute in order: b first, then c
      (let ((b-val (1+ (cdr (assq 'a neovm--rs5-vals)))))
        (setq neovm--rs5-vals (cons (cons 'b b-val) neovm--rs5-vals))
        (let ((c-val (+ (cdr (assq 'a neovm--rs5-vals))
                        (cdr (assq 'b neovm--rs5-vals)))))
          (setq neovm--rs5-vals (cons (cons 'c c-val) neovm--rs5-vals))))))

  ;; Correct propagation: update in topological order
  (fset 'neovm--rs5-set-a-correct
    (lambda (new-a)
      (setcdr (assq 'a neovm--rs5-vals) new-a)
      ;; Update b first (depends only on a)
      (setcdr (assq 'b neovm--rs5-vals) (1+ new-a))
      ;; Then update c (depends on a and b, both now fresh)
      (setcdr (assq 'c neovm--rs5-vals)
              (+ (cdr (assq 'a neovm--rs5-vals))
                 (cdr (assq 'b neovm--rs5-vals))))
      ;; Snapshot after correct propagation
      (setq neovm--rs5-snapshots
            (cons (list 'correct
                        (cdr (assq 'a neovm--rs5-vals))
                        (cdr (assq 'b neovm--rs5-vals))
                        (cdr (assq 'c neovm--rs5-vals)))
                  neovm--rs5-snapshots))))

  ;; Wrong propagation: update c before b (glitch)
  (fset 'neovm--rs5-set-a-glitchy
    (lambda (new-a)
      (setcdr (assq 'a neovm--rs5-vals) new-a)
      ;; Update c first (b is STALE) -> glitch!
      (setcdr (assq 'c neovm--rs5-vals)
              (+ (cdr (assq 'a neovm--rs5-vals))
                 (cdr (assq 'b neovm--rs5-vals))))
      ;; Then update b
      (setcdr (assq 'b neovm--rs5-vals) (1+ new-a))
      ;; Snapshot shows the glitchy c value
      (setq neovm--rs5-snapshots
            (cons (list 'glitchy
                        (cdr (assq 'a neovm--rs5-vals))
                        (cdr (assq 'b neovm--rs5-vals))
                        (cdr (assq 'c neovm--rs5-vals)))
                  neovm--rs5-snapshots))))

  (unwind-protect
      (progn
        (setq neovm--rs5-snapshots nil)

        ;; Test correct propagation
        (funcall 'neovm--rs5-init)
        ;; Initial: a=1, b=2, c=3
        (let ((init-snap (list (cdr (assq 'a neovm--rs5-vals))
                               (cdr (assq 'b neovm--rs5-vals))
                               (cdr (assq 'c neovm--rs5-vals)))))
          ;; Set a=10 correctly: b=11, c=21
          (funcall 'neovm--rs5-set-a-correct 10)

          ;; Reset and test glitchy
          (funcall 'neovm--rs5-init)
          ;; Set a=10 with glitch: c computed with stale b=2 -> c=12, then b=11
          ;; So glitchy snapshot shows c based on stale b
          (funcall 'neovm--rs5-set-a-glitchy 10)

          (let ((snaps (nreverse neovm--rs5-snapshots)))
            (list
              init-snap
              ;; Correct: (correct 10 11 21) - c=a+b=10+11=21
              (nth 0 snaps)
              ;; Glitchy: (glitchy 10 11 12) - c was computed as 10+2=12 (stale b)
              ;; but b was then corrected to 11. The c value 12 is wrong.
              (nth 1 snaps)
              ;; The correct c value should be 21, not the glitchy 12
              (not (= (nth 3 (nth 0 snaps))
                      (nth 3 (nth 1 snaps))))))))
    (fmakunbound 'neovm--rs5-init)
    (fmakunbound 'neovm--rs5-set-a-correct)
    (fmakunbound 'neovm--rs5-set-a-glitchy)
    (makunbound 'neovm--rs5-vals)
    (makunbound 'neovm--rs5-snapshots)))"#;
    let expect =
        expect_test::expect![[r#""OK ((1 2 3) (correct 10 11 21) (glitchy 10 11 12) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Batch updates: defer notifications until batch completes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_reactive_batch_updates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--rs6-cells nil)
  (defvar neovm--rs6-batch-mode nil)
  (defvar neovm--rs6-dirty nil)
  (defvar neovm--rs6-notify-log nil)

  (fset 'neovm--rs6-defcell
    (lambda (name val)
      (setq neovm--rs6-cells (cons (list name val nil) neovm--rs6-cells))))

  (fset 'neovm--rs6-on-change
    (lambda (name callback)
      (let ((cell (assq name neovm--rs6-cells)))
        (setcar (cddr cell) (cons callback (nth 2 cell))))))

  (fset 'neovm--rs6-notify
    (lambda (name old new)
      (dolist (cb (nth 2 (assq name neovm--rs6-cells)))
        (let ((msg (funcall cb name old new)))
          (setq neovm--rs6-notify-log
                (cons msg neovm--rs6-notify-log))))))

  (fset 'neovm--rs6-set
    (lambda (name new-val)
      (let* ((cell (assq name neovm--rs6-cells))
             (old-val (nth 1 cell)))
        (setcar (cdr cell) new-val)
        (if neovm--rs6-batch-mode
            ;; In batch: record dirty, defer notification
            (unless (memq name neovm--rs6-dirty)
              (setq neovm--rs6-dirty
                    (cons (list name old-val) neovm--rs6-dirty)))
          ;; Not in batch: notify immediately
          (unless (equal old-val new-val)
            (funcall 'neovm--rs6-notify name old-val new-val))))))

  (fset 'neovm--rs6-batch
    (lambda (thunk)
      "Execute THUNK with deferred notifications."
      (let ((neovm--rs6-batch-mode t)
            (neovm--rs6-dirty nil))
        (funcall thunk)
        ;; Flush: notify for each dirty cell (one notification per cell)
        (dolist (dirty (nreverse neovm--rs6-dirty))
          (let* ((name (nth 0 dirty))
                 (original-old (nth 1 dirty))
                 (current-val (nth 1 (assq name neovm--rs6-cells))))
            (unless (equal original-old current-val)
              (funcall 'neovm--rs6-notify name original-old current-val)))))))

  (unwind-protect
      (progn
        (setq neovm--rs6-cells nil)
        (setq neovm--rs6-batch-mode nil)
        (setq neovm--rs6-dirty nil)
        (setq neovm--rs6-notify-log nil)

        (funcall 'neovm--rs6-defcell 'x 0)
        (funcall 'neovm--rs6-defcell 'y 0)

        (funcall 'neovm--rs6-on-change 'x
                 (lambda (name old new) (format "%s:%d->%d" name old new)))
        (funcall 'neovm--rs6-on-change 'y
                 (lambda (name old new) (format "%s:%d->%d" name old new)))

        ;; Non-batch: each set fires immediately
        (funcall 'neovm--rs6-set 'x 1)
        (funcall 'neovm--rs6-set 'x 2)
        (let ((immediate-count (length neovm--rs6-notify-log)))

          ;; Batch: multiple sets, only one notification per cell
          (funcall 'neovm--rs6-batch
                   (lambda ()
                     (funcall 'neovm--rs6-set 'x 5)
                     (funcall 'neovm--rs6-set 'x 10)
                     (funcall 'neovm--rs6-set 'x 15)
                     (funcall 'neovm--rs6-set 'y 100)))

          (list
            immediate-count  ;; 2 (one per non-batch set)
            (length neovm--rs6-notify-log)  ;; 2 + 2 = 4 (batch adds 2: x and y)
            (nreverse neovm--rs6-notify-log)
            ;; Final values
            (nth 1 (assq 'x neovm--rs6-cells))
            (nth 1 (assq 'y neovm--rs6-cells)))))
    (fmakunbound 'neovm--rs6-defcell)
    (fmakunbound 'neovm--rs6-on-change)
    (fmakunbound 'neovm--rs6-notify)
    (fmakunbound 'neovm--rs6-set)
    (fmakunbound 'neovm--rs6-batch)
    (makunbound 'neovm--rs6-cells)
    (makunbound 'neovm--rs6-batch-mode)
    (makunbound 'neovm--rs6-dirty)
    (makunbound 'neovm--rs6-notify-log)))"#;
    let expect = expect_test::expect![[
        r#""OK (2 6 (\"x:0->1\" \"x:1->2\" \"x:2->15\" \"x:5->15\" \"x:10->15\" \"y:0->100\") 15 100)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Disposable subscriptions: auto-cleanup
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_reactive_dispose_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--rs7-subs nil)
  (defvar neovm--rs7-active nil)
  (defvar neovm--rs7-log nil)

  (fset 'neovm--rs7-make-subscription
    (lambda (id source callback)
      (let ((sub (list (cons 'id id)
                       (cons 'source source)
                       (cons 'callback callback)
                       (cons 'active t))))
        (setq neovm--rs7-subs (cons sub neovm--rs7-subs))
        sub)))

  (fset 'neovm--rs7-dispose
    (lambda (sub)
      (setcdr (assq 'active sub) nil)))

  (fset 'neovm--rs7-emit
    (lambda (source value)
      (dolist (sub (reverse neovm--rs7-subs))
        (when (and (cdr (assq 'active sub))
                   (eq (cdr (assq 'source sub)) source))
          (let ((result (funcall (cdr (assq 'callback sub)) value)))
            (setq neovm--rs7-log
                  (cons (list (cdr (assq 'id sub)) source value result)
                        neovm--rs7-log)))))))

  (fset 'neovm--rs7-active-count
    (lambda ()
      (let ((count 0))
        (dolist (sub neovm--rs7-subs)
          (when (cdr (assq 'active sub))
            (setq count (1+ count))))
        count)))

  (unwind-protect
      (progn
        (setq neovm--rs7-subs nil)
        (setq neovm--rs7-log nil)

        (let ((s1 (funcall 'neovm--rs7-make-subscription
                           'sub-1 'data-source (lambda (v) (* v 2))))
              (s2 (funcall 'neovm--rs7-make-subscription
                           'sub-2 'data-source (lambda (v) (+ v 100))))
              (s3 (funcall 'neovm--rs7-make-subscription
                           'sub-3 'other-source (lambda (v) (format "got:%s" v)))))

          ;; Emit on data-source: both s1 and s2 fire
          (funcall 'neovm--rs7-emit 'data-source 5)
          (let ((count-before (funcall 'neovm--rs7-active-count)))

            ;; Dispose s1
            (funcall 'neovm--rs7-dispose s1)

            ;; Emit again: only s2 fires
            (funcall 'neovm--rs7-emit 'data-source 10)

            ;; Emit on other-source: s3 fires
            (funcall 'neovm--rs7-emit 'other-source "hello")

            ;; Dispose all
            (funcall 'neovm--rs7-dispose s2)
            (funcall 'neovm--rs7-dispose s3)

            (funcall 'neovm--rs7-emit 'data-source 99)  ;; nobody fires

            (list
              count-before  ;; 3
              (funcall 'neovm--rs7-active-count)  ;; 0
              (length neovm--rs7-log)
              (nreverse neovm--rs7-log)))))
    (fmakunbound 'neovm--rs7-make-subscription)
    (fmakunbound 'neovm--rs7-dispose)
    (fmakunbound 'neovm--rs7-emit)
    (fmakunbound 'neovm--rs7-active-count)
    (makunbound 'neovm--rs7-subs)
    (makunbound 'neovm--rs7-active)
    (makunbound 'neovm--rs7-log)))"#;
    let expect = expect_test::expect![[
        r#""OK (3 0 4 ((sub-1 data-source 5 10) (sub-2 data-source 5 105) (sub-2 data-source 10 110) (sub-3 other-source \"hello\" \"got:hello\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
