//! Oracle parity tests for a Petri net simulator in Elisp:
//! places with token counts, transitions with input/output arcs,
//! firing rules, transition execution, dining philosophers model,
//! producer-consumer model, and deadlock detection.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic Petri net: places, transitions, and firing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_petri_net_basic_structure_and_firing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A Petri net is represented as:
    //   places: alist of (place-name . token-count)
    //   transitions: list of (name input-arcs output-arcs)
    //     where arcs are lists of (place-name . weight)
    //
    // A transition is enabled when all input places have >= weight tokens.
    // Firing consumes input tokens and produces output tokens.
    let form = r#"(progn
  ;; Check if a transition is enabled
  (fset 'neovm--pn-enabled-p
    (lambda (places transition)
      "Return t if TRANSITION can fire given PLACES token counts."
      (let ((inputs (nth 1 transition))
            (ok t))
        (dolist (arc inputs ok)
          (let* ((place (car arc))
                 (weight (cdr arc))
                 (tokens (or (cdr (assq place places)) 0)))
            (when (< tokens weight)
              (setq ok nil)))))))

  ;; Fire a transition, returning new places state
  (fset 'neovm--pn-fire
    (lambda (places transition)
      "Fire TRANSITION, consuming inputs and producing outputs. Returns new places."
      (let ((new-places (copy-sequence places))
            (inputs (nth 1 transition))
            (outputs (nth 2 transition)))
        ;; Consume inputs
        (dolist (arc inputs)
          (let* ((place (car arc))
                 (weight (cdr arc))
                 (entry (assq place new-places)))
            (if entry
                (setcdr entry (- (cdr entry) weight))
              (push (cons place (- weight)) new-places))))
        ;; Produce outputs
        (dolist (arc outputs)
          (let* ((place (car arc))
                 (weight (cdr arc))
                 (entry (assq place new-places)))
            (if entry
                (setcdr entry (+ (cdr entry) weight))
              (push (cons place weight) new-places))))
        new-places)))

  ;; Get all enabled transitions
  (fset 'neovm--pn-enabled-transitions
    (lambda (places transitions)
      "Return list of transitions that are enabled."
      (let ((enabled nil))
        (dolist (tr transitions)
          (when (funcall 'neovm--pn-enabled-p places tr)
            (push tr enabled)))
        (nreverse enabled))))

  ;; Get token count for a place
  (fset 'neovm--pn-tokens
    (lambda (places place)
      (or (cdr (assq place places)) 0)))

  (unwind-protect
      (let* (;; Simple net: p1 -> [t1] -> p2 -> [t2] -> p3
             (places '((p1 . 3) (p2 . 0) (p3 . 0)))
             (t1 '(t1 ((p1 . 1)) ((p2 . 1))))
             (t2 '(t2 ((p2 . 1)) ((p3 . 1))))
             (transitions (list t1 t2))
             (results nil))

        ;; Initial state
        (push (list :initial
                    (funcall 'neovm--pn-tokens places 'p1)
                    (funcall 'neovm--pn-tokens places 'p2)
                    (funcall 'neovm--pn-tokens places 'p3))
              results)

        ;; Check enabled transitions
        (push (list :enabled-initially
                    (mapcar #'car (funcall 'neovm--pn-enabled-transitions places transitions)))
              results)

        ;; Fire t1 once
        (setq places (funcall 'neovm--pn-fire places t1))
        (push (list :after-t1-once
                    (funcall 'neovm--pn-tokens places 'p1)
                    (funcall 'neovm--pn-tokens places 'p2)
                    (funcall 'neovm--pn-tokens places 'p3))
              results)

        ;; Fire t1 again
        (setq places (funcall 'neovm--pn-fire places t1))
        (push (list :after-t1-twice
                    (funcall 'neovm--pn-tokens places 'p1)
                    (funcall 'neovm--pn-tokens places 'p2)
                    (funcall 'neovm--pn-tokens places 'p3))
              results)

        ;; Now fire t2 twice (consumes from p2)
        (setq places (funcall 'neovm--pn-fire places t2))
        (setq places (funcall 'neovm--pn-fire places t2))
        (push (list :after-t2-twice
                    (funcall 'neovm--pn-tokens places 'p1)
                    (funcall 'neovm--pn-tokens places 'p2)
                    (funcall 'neovm--pn-tokens places 'p3))
              results)

        ;; Check enabled after all firings
        (push (list :enabled-now
                    (mapcar #'car (funcall 'neovm--pn-enabled-transitions places transitions)))
              results)

        ;; Fire remaining t1, then all t2
        (setq places (funcall 'neovm--pn-fire places t1))
        (setq places (funcall 'neovm--pn-fire places t2))
        (push (list :final
                    (funcall 'neovm--pn-tokens places 'p1)
                    (funcall 'neovm--pn-tokens places 'p2)
                    (funcall 'neovm--pn-tokens places 'p3))
              results)

        (nreverse results))
    (fmakunbound 'neovm--pn-enabled-p)
    (fmakunbound 'neovm--pn-fire)
    (fmakunbound 'neovm--pn-enabled-transitions)
    (fmakunbound 'neovm--pn-tokens)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:initial 3 0 0) (:enabled-initially (t1)) (:after-t1-once 2 1 0) (:after-t1-twice 1 2 0) (:after-t2-twice 1 0 2) (:enabled-now (t1)) (:final 0 0 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Petri net with weighted arcs and multi-input/output transitions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_petri_net_weighted_arcs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Transitions that consume/produce multiple tokens at once.
    let form = r#"(progn
  (fset 'neovm--pn-enabled-p
    (lambda (places transition)
      (let ((inputs (nth 1 transition)) (ok t))
        (dolist (arc inputs ok)
          (when (< (or (cdr (assq (car arc) places)) 0) (cdr arc))
            (setq ok nil))))))

  (fset 'neovm--pn-fire
    (lambda (places transition)
      (let ((new-places (copy-sequence places)))
        (dolist (arc (nth 1 transition))
          (let ((entry (assq (car arc) new-places)))
            (if entry (setcdr entry (- (cdr entry) (cdr arc)))
              (push (cons (car arc) (- (cdr arc))) new-places))))
        (dolist (arc (nth 2 transition))
          (let ((entry (assq (car arc) new-places)))
            (if entry (setcdr entry (+ (cdr entry) (cdr arc)))
              (push (cons (car arc) (cdr arc)) new-places))))
        new-places)))

  (fset 'neovm--pn-tokens
    (lambda (places place) (or (cdr (assq place places)) 0)))

  (unwind-protect
      (let* (;; Chemical reaction: 2H2 + O2 -> 2H2O
             (places '((h2 . 6) (o2 . 3) (h2o . 0)))
             (react '(react ((h2 . 2) (o2 . 1)) ((h2o . 2))))
             (results nil))

        ;; How many times can we fire?
        (let ((fires 0)
              (state places))
          (while (funcall 'neovm--pn-enabled-p state react)
            (setq state (funcall 'neovm--pn-fire state react))
            (setq fires (1+ fires)))
          (push (list :reaction-count fires
                      :h2-remaining (funcall 'neovm--pn-tokens state 'h2)
                      :o2-remaining (funcall 'neovm--pn-tokens state 'o2)
                      :h2o-produced (funcall 'neovm--pn-tokens state 'h2o))
                results))

        ;; Assembly line: 3 parts + 1 base -> 1 product
        (let* ((places2 '((parts . 10) (base . 4) (product . 0)))
               (assemble '(assemble ((parts . 3) (base . 1)) ((product . 1))))
               (fires 0)
               (state places2))
          (while (funcall 'neovm--pn-enabled-p state assemble)
            (setq state (funcall 'neovm--pn-fire state assemble))
            (setq fires (1+ fires)))
          (push (list :assembly-count fires
                      :parts-left (funcall 'neovm--pn-tokens state 'parts)
                      :base-left (funcall 'neovm--pn-tokens state 'base)
                      :products (funcall 'neovm--pn-tokens state 'product))
                results))

        ;; Split transition: 1 input -> 3 outputs
        (let* ((places3 '((raw . 5) (a . 0) (b . 0) (c . 0)))
               (split '(split ((raw . 1)) ((a . 1) (b . 2) (c . 1))))
               (fires 0)
               (state places3))
          (while (funcall 'neovm--pn-enabled-p state split)
            (setq state (funcall 'neovm--pn-fire state split))
            (setq fires (1+ fires)))
          (push (list :split-fires fires
                      :a (funcall 'neovm--pn-tokens state 'a)
                      :b (funcall 'neovm--pn-tokens state 'b)
                      :c (funcall 'neovm--pn-tokens state 'c))
                results))

        (nreverse results))
    (fmakunbound 'neovm--pn-enabled-p)
    (fmakunbound 'neovm--pn-fire)
    (fmakunbound 'neovm--pn-tokens)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:reaction-count 3 :h2-remaining 0 :o2-remaining 0 :h2o-produced 6) (:assembly-count 3 :parts-left 1 :base-left 1 :products 3) (:split-fires 5 :a 5 :b 10 :c 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Petri net simulation: run until no transitions are enabled
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_petri_net_simulation_to_completion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a net by firing the first enabled transition at each step,
    // recording the trace of fired transitions.
    let form = r#"(progn
  (fset 'neovm--pn-enabled-p
    (lambda (places transition)
      (let ((inputs (nth 1 transition)) (ok t))
        (dolist (arc inputs ok)
          (when (< (or (cdr (assq (car arc) places)) 0) (cdr arc))
            (setq ok nil))))))

  (fset 'neovm--pn-fire
    (lambda (places transition)
      (let ((new-places (copy-sequence places)))
        (dolist (arc (nth 1 transition))
          (let ((entry (assq (car arc) new-places)))
            (if entry (setcdr entry (- (cdr entry) (cdr arc)))
              (push (cons (car arc) (- (cdr arc))) new-places))))
        (dolist (arc (nth 2 transition))
          (let ((entry (assq (car arc) new-places)))
            (if entry (setcdr entry (+ (cdr entry) (cdr arc)))
              (push (cons (car arc) (cdr arc)) new-places))))
        new-places)))

  (fset 'neovm--pn-tokens
    (lambda (places place) (or (cdr (assq place places)) 0)))

  (fset 'neovm--pn-simulate
    (lambda (places transitions max-steps)
      "Simulate net, firing first enabled transition each step.
       Returns (trace final-marking steps)."
      (let ((trace nil)
            (steps 0)
            (state places))
        (while (and (< steps max-steps)
                    (let ((found nil))
                      (dolist (tr transitions)
                        (when (and (not found)
                                   (funcall 'neovm--pn-enabled-p state tr))
                          (setq found tr)))
                      found))
          (let ((fired nil))
            (dolist (tr transitions)
              (when (and (not fired)
                         (funcall 'neovm--pn-enabled-p state tr))
                (setq fired tr)))
            (push (car fired) trace)
            (setq state (funcall 'neovm--pn-fire state fired))
            (setq steps (1+ steps))))
        (list :trace (nreverse trace)
              :final-marking state
              :steps steps))))

  (unwind-protect
      (let ((results nil))
        ;; Pipeline: source -> process -> sink
        (let* ((places '((source . 4) (buffer . 0) (sink . 0)))
               (produce '(produce ((source . 1)) ((buffer . 1))))
               (consume '(consume ((buffer . 1)) ((sink . 1))))
               (transitions (list produce consume)))
          (push (funcall 'neovm--pn-simulate places transitions 20) results))

        ;; Fork-join: split then merge
        (let* ((places '((start . 2) (left . 0) (right . 0) (end . 0)))
               (fork '(fork ((start . 1)) ((left . 1) (right . 1))))
               (join '(join ((left . 1) (right . 1)) ((end . 1))))
               (transitions (list fork join)))
          (push (funcall 'neovm--pn-simulate places transitions 20) results))

        ;; Mutual exclusion: two processes sharing a mutex
        (let* ((places '((idle1 . 1) (idle2 . 1) (mutex . 1)
                         (critical1 . 0) (critical2 . 0)))
               (enter1 '(enter1 ((idle1 . 1) (mutex . 1)) ((critical1 . 1))))
               (leave1 '(leave1 ((critical1 . 1)) ((idle1 . 1) (mutex . 1))))
               (enter2 '(enter2 ((idle2 . 1) (mutex . 1)) ((critical2 . 1))))
               (leave2 '(leave2 ((critical2 . 1)) ((idle2 . 1) (mutex . 1))))
               (transitions (list enter1 leave1 enter2 leave2)))
          (push (funcall 'neovm--pn-simulate places transitions 20) results))

        (nreverse results))
    (fmakunbound 'neovm--pn-enabled-p)
    (fmakunbound 'neovm--pn-fire)
    (fmakunbound 'neovm--pn-tokens)
    (fmakunbound 'neovm--pn-simulate)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:trace (produce produce produce produce consume consume consume consume) :final-marking ((source . 0) (buffer . 0) (sink . 4)) :steps 8) (:trace (fork fork join join) :final-marking ((start . 0) (left . 0) (right . 0) (end . 2)) :steps 4) (:trace (enter1 leave1 enter1 leave1 enter1 leave1 enter1 leave1 enter1 leave1 enter1 leave1 enter1 leave1 enter1 leave1 enter1 leave1 enter1 leave1) :final-marking ((idle1 . 1) (idle2 . 1) (mutex . 1) (critical1 . 0) (critical2 . 0)) :steps 20))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: Dining philosophers as Petri net
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_petri_net_dining_philosophers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Classic dining philosophers with 3 philosophers (to keep it manageable).
    // Each philosopher needs two forks (left and right) to eat.
    let form = r#"(progn
  (fset 'neovm--pn-enabled-p
    (lambda (places transition)
      (let ((inputs (nth 1 transition)) (ok t))
        (dolist (arc inputs ok)
          (when (< (or (cdr (assq (car arc) places)) 0) (cdr arc))
            (setq ok nil))))))

  (fset 'neovm--pn-fire
    (lambda (places transition)
      (let ((new-places (copy-sequence places)))
        (dolist (arc (nth 1 transition))
          (let ((entry (assq (car arc) new-places)))
            (if entry (setcdr entry (- (cdr entry) (cdr arc)))
              (push (cons (car arc) (- (cdr arc))) new-places))))
        (dolist (arc (nth 2 transition))
          (let ((entry (assq (car arc) new-places)))
            (if entry (setcdr entry (+ (cdr entry) (cdr arc)))
              (push (cons (car arc) (cdr arc)) new-places))))
        new-places)))

  (fset 'neovm--pn-tokens
    (lambda (places place) (or (cdr (assq place places)) 0)))

  (unwind-protect
      (let* (;; 3 philosophers, 3 forks (circular arrangement)
             ;; Phil i needs fork i and fork (i+1)%3
             (places '((think0 . 1) (think1 . 1) (think2 . 1)
                       (eat0 . 0)   (eat1 . 0)   (eat2 . 0)
                       (fork0 . 1)  (fork1 . 1)  (fork2 . 1)))
             ;; Pickup transitions: philosopher picks up both forks
             (pickup0 '(pickup0 ((think0 . 1) (fork0 . 1) (fork1 . 1))
                                ((eat0 . 1))))
             (pickup1 '(pickup1 ((think1 . 1) (fork1 . 1) (fork2 . 1))
                                ((eat1 . 1))))
             (pickup2 '(pickup2 ((think2 . 1) (fork2 . 1) (fork0 . 1))
                                ((eat2 . 1))))
             ;; Putdown transitions: philosopher puts down both forks
             (putdown0 '(putdown0 ((eat0 . 1))
                                  ((think0 . 1) (fork0 . 1) (fork1 . 1))))
             (putdown1 '(putdown1 ((eat1 . 1))
                                  ((think1 . 1) (fork1 . 1) (fork2 . 1))))
             (putdown2 '(putdown2 ((eat2 . 1))
                                  ((think2 . 1) (fork2 . 1) (fork0 . 1))))
             (transitions (list pickup0 putdown0 pickup1 putdown1 pickup2 putdown2))
             (results nil)
             (state places))

        ;; Initial: all thinking, all forks available
        (push (list :initial
                    :forks (list (funcall 'neovm--pn-tokens state 'fork0)
                                 (funcall 'neovm--pn-tokens state 'fork1)
                                 (funcall 'neovm--pn-tokens state 'fork2))
                    :thinking (list (funcall 'neovm--pn-tokens state 'think0)
                                    (funcall 'neovm--pn-tokens state 'think1)
                                    (funcall 'neovm--pn-tokens state 'think2))
                    :eating (list (funcall 'neovm--pn-tokens state 'eat0)
                                  (funcall 'neovm--pn-tokens state 'eat1)
                                  (funcall 'neovm--pn-tokens state 'eat2)))
              results)

        ;; Phil 0 picks up forks and eats
        (setq state (funcall 'neovm--pn-fire state pickup0))
        (push (list :phil0-eating
                    :forks (list (funcall 'neovm--pn-tokens state 'fork0)
                                 (funcall 'neovm--pn-tokens state 'fork1)
                                 (funcall 'neovm--pn-tokens state 'fork2))
                    :eating (list (funcall 'neovm--pn-tokens state 'eat0)
                                  (funcall 'neovm--pn-tokens state 'eat1)
                                  (funcall 'neovm--pn-tokens state 'eat2)))
              results)

        ;; Phil 1 cannot eat (fork1 taken), but Phil 2 can
        (push (list :phil1-enabled (funcall 'neovm--pn-enabled-p state pickup1)
                    :phil2-enabled (funcall 'neovm--pn-enabled-p state pickup2))
              results)

        ;; Phil 2 picks up forks 2 and 0... but fork0 is taken!
        (push (list :phil2-can-eat (funcall 'neovm--pn-enabled-p state pickup2))
              results)

        ;; Phil 0 puts down forks
        (setq state (funcall 'neovm--pn-fire state putdown0))
        ;; Now others can eat
        (push (list :after-putdown0
                    :phil1-enabled (funcall 'neovm--pn-enabled-p state pickup1)
                    :phil2-enabled (funcall 'neovm--pn-enabled-p state pickup2))
              results)

        ;; Conservation: total tokens (thinking + eating) per philosopher = 1
        ;; total forks in system = 3
        (let ((total-forks (+ (funcall 'neovm--pn-tokens state 'fork0)
                              (funcall 'neovm--pn-tokens state 'fork1)
                              (funcall 'neovm--pn-tokens state 'fork2)))
              ;; Forks held by eating philosophers
              (held-forks (* 2 (+ (funcall 'neovm--pn-tokens state 'eat0)
                                  (funcall 'neovm--pn-tokens state 'eat1)
                                  (funcall 'neovm--pn-tokens state 'eat2)))))
          (push (list :conservation
                      :free-forks total-forks
                      :held-forks held-forks
                      :total (+ total-forks held-forks))
                results))

        (nreverse results))
    (fmakunbound 'neovm--pn-enabled-p)
    (fmakunbound 'neovm--pn-fire)
    (fmakunbound 'neovm--pn-tokens)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:initial :forks (1 1 1) :thinking (1 1 1) :eating (0 0 0)) (:phil0-eating :forks (0 0 1) :eating (1 0 0)) (:phil1-enabled nil :phil2-enabled nil) (:phil2-can-eat nil) (:after-putdown0 :phil1-enabled t :phil2-enabled t) (:conservation :free-forks 3 :held-forks 0 :total 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: Producer-consumer as Petri net
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_petri_net_producer_consumer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Producer-consumer with bounded buffer (capacity 3).
    // Producer can only produce when buffer not full.
    // Consumer can only consume when buffer not empty.
    let form = r#"(progn
  (fset 'neovm--pn-enabled-p
    (lambda (places transition)
      (let ((inputs (nth 1 transition)) (ok t))
        (dolist (arc inputs ok)
          (when (< (or (cdr (assq (car arc) places)) 0) (cdr arc))
            (setq ok nil))))))

  (fset 'neovm--pn-fire
    (lambda (places transition)
      (let ((new-places (copy-sequence places)))
        (dolist (arc (nth 1 transition))
          (let ((entry (assq (car arc) new-places)))
            (if entry (setcdr entry (- (cdr entry) (cdr arc)))
              (push (cons (car arc) (- (cdr arc))) new-places))))
        (dolist (arc (nth 2 transition))
          (let ((entry (assq (car arc) new-places)))
            (if entry (setcdr entry (+ (cdr entry) (cdr arc)))
              (push (cons (car arc) (cdr arc)) new-places))))
        new-places)))

  (fset 'neovm--pn-tokens
    (lambda (places place) (or (cdr (assq place places)) 0)))

  (unwind-protect
      (let* (;; Bounded buffer capacity = 3
             ;; empty-slots tracks available capacity
             ;; full-slots tracks items in buffer
             (places '((producer-ready . 1)
                       (consumer-ready . 1)
                       (empty-slots . 3)
                       (full-slots . 0)
                       (produced . 0)
                       (consumed . 0)))
             ;; Producer: needs ready + empty-slot, produces full-slot
             (produce '(produce
                        ((producer-ready . 1) (empty-slots . 1))
                        ((producer-ready . 1) (full-slots . 1) (produced . 1))))
             ;; Consumer: needs ready + full-slot, produces empty-slot
             (consume '(consume
                        ((consumer-ready . 1) (full-slots . 1))
                        ((consumer-ready . 1) (empty-slots . 1) (consumed . 1))))
             (results nil)
             (state places))

        ;; Produce 3 items (fill buffer)
        (dotimes (_ 3)
          (setq state (funcall 'neovm--pn-fire state produce)))
        (push (list :after-3-produces
                    :full (funcall 'neovm--pn-tokens state 'full-slots)
                    :empty (funcall 'neovm--pn-tokens state 'empty-slots)
                    :produced (funcall 'neovm--pn-tokens state 'produced)
                    :can-produce (funcall 'neovm--pn-enabled-p state produce)
                    :can-consume (funcall 'neovm--pn-enabled-p state consume))
              results)

        ;; Consume 2 items
        (dotimes (_ 2)
          (setq state (funcall 'neovm--pn-fire state consume)))
        (push (list :after-2-consumes
                    :full (funcall 'neovm--pn-tokens state 'full-slots)
                    :empty (funcall 'neovm--pn-tokens state 'empty-slots)
                    :consumed (funcall 'neovm--pn-tokens state 'consumed))
              results)

        ;; Produce 2 more, consume 3
        (dotimes (_ 2)
          (setq state (funcall 'neovm--pn-fire state produce)))
        (dotimes (_ 3)
          (setq state (funcall 'neovm--pn-fire state consume)))
        (push (list :final
                    :full (funcall 'neovm--pn-tokens state 'full-slots)
                    :empty (funcall 'neovm--pn-tokens state 'empty-slots)
                    :produced (funcall 'neovm--pn-tokens state 'produced)
                    :consumed (funcall 'neovm--pn-tokens state 'consumed))
              results)

        ;; Invariant: empty + full = buffer capacity (3)
        (push (list :invariant-holds
                    (= 3 (+ (funcall 'neovm--pn-tokens state 'full-slots)
                             (funcall 'neovm--pn-tokens state 'empty-slots))))
              results)

        (nreverse results))
    (fmakunbound 'neovm--pn-enabled-p)
    (fmakunbound 'neovm--pn-fire)
    (fmakunbound 'neovm--pn-tokens)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:after-3-produces :full 3 :empty 0 :produced 3 :can-produce nil :can-consume t) (:after-2-consumes :full 1 :empty 2 :consumed 2) (:final :full 0 :empty 3 :produced 5 :consumed 5) (:invariant-holds t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: Deadlock detection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_petri_net_deadlock_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Detect deadlock: a marking where no transition is enabled.
    // Test both deadlock-free and deadlock-prone nets.
    let form = r#"(progn
  (fset 'neovm--pn-enabled-p
    (lambda (places transition)
      (let ((inputs (nth 1 transition)) (ok t))
        (dolist (arc inputs ok)
          (when (< (or (cdr (assq (car arc) places)) 0) (cdr arc))
            (setq ok nil))))))

  (fset 'neovm--pn-fire
    (lambda (places transition)
      (let ((new-places (copy-sequence places)))
        (dolist (arc (nth 1 transition))
          (let ((entry (assq (car arc) new-places)))
            (if entry (setcdr entry (- (cdr entry) (cdr arc)))
              (push (cons (car arc) (- (cdr arc))) new-places))))
        (dolist (arc (nth 2 transition))
          (let ((entry (assq (car arc) new-places)))
            (if entry (setcdr entry (+ (cdr entry) (cdr arc)))
              (push (cons (car arc) (cdr arc)) new-places))))
        new-places)))

  (fset 'neovm--pn-tokens
    (lambda (places place) (or (cdr (assq place places)) 0)))

  (fset 'neovm--pn-deadlocked-p
    (lambda (places transitions)
      "Return t if no transition is enabled (deadlock)."
      (let ((any-enabled nil))
        (dolist (tr transitions)
          (when (funcall 'neovm--pn-enabled-p places tr)
            (setq any-enabled t)))
        (not any-enabled))))

  ;; Explore reachable markings up to a limit, check for deadlock
  (fset 'neovm--pn-explore-deadlock
    (lambda (initial-places transitions max-depth)
      "Simulate all possible firing sequences up to MAX-DEPTH.
       Returns (:deadlock-found t/nil :states-explored N :deadlock-marking M)."
      (let ((stack (list (cons initial-places 0)))
            (explored 0)
            (deadlock-found nil)
            (deadlock-marking nil))
        (while (and stack (not deadlock-found) (< explored 200))
          (let* ((item (car stack))
                 (state (car item))
                 (depth (cdr item)))
            (setq stack (cdr stack))
            (setq explored (1+ explored))
            (if (funcall 'neovm--pn-deadlocked-p state transitions)
                (progn
                  (setq deadlock-found t)
                  (setq deadlock-marking state))
              (when (< depth max-depth)
                (dolist (tr transitions)
                  (when (funcall 'neovm--pn-enabled-p state tr)
                    (push (cons (funcall 'neovm--pn-fire state tr) (1+ depth))
                          stack)))))))
        (list :deadlock-found deadlock-found
              :states-explored explored
              :deadlock-marking deadlock-marking))))

  (unwind-protect
      (let ((results nil))
        ;; Net that always deadlocks: consume without produce
        (let* ((places '((p1 . 1)))
               (t1 '(t1 ((p1 . 1)) ()))
               (transitions (list t1)))
          (push (list :always-deadlocks
                      (funcall 'neovm--pn-explore-deadlock places transitions 5))
                results))

        ;; Cyclic net: never deadlocks (token circulates)
        (let* ((places '((p1 . 1) (p2 . 0)))
               (t1 '(t1 ((p1 . 1)) ((p2 . 1))))
               (t2 '(t2 ((p2 . 1)) ((p1 . 1))))
               (transitions (list t1 t2)))
          ;; After any number of firings, one of p1/p2 has the token
          (push (list :cyclic-no-deadlock
                      (funcall 'neovm--pn-explore-deadlock places transitions 6))
                results))

        ;; Resource contention that can deadlock:
        ;; Two processes each need both resources A and B
        ;; Process 1 grabs A first, Process 2 grabs B first
        (let* ((places '((idle1 . 1) (idle2 . 1) (resA . 1) (resB . 1)
                         (has-a1 . 0) (has-b2 . 0)
                         (done1 . 0) (done2 . 0)))
               ;; Process 1: grab A, then grab B, then release both
               (grab-a1 '(grab-a1 ((idle1 . 1) (resA . 1)) ((has-a1 . 1))))
               (grab-b1 '(grab-b1 ((has-a1 . 1) (resB . 1)) ((done1 . 1) (resA . 1) (resB . 1))))
               ;; Process 2: grab B, then grab A, then release both
               (grab-b2 '(grab-b2 ((idle2 . 1) (resB . 1)) ((has-b2 . 1))))
               (grab-a2 '(grab-a2 ((has-b2 . 1) (resA . 1)) ((done2 . 1) (resA . 1) (resB . 1))))
               (transitions (list grab-a1 grab-b1 grab-b2 grab-a2)))
          ;; This CAN deadlock if both grab their first resource
          (push (list :resource-contention
                      (funcall 'neovm--pn-explore-deadlock places transitions 4))
                results))

        ;; Safe version with ordering: both grab A first, then B
        (let* ((places '((idle1 . 1) (idle2 . 1) (resA . 1) (resB . 1)
                         (has-a1 . 0) (has-a2 . 0)
                         (done1 . 0) (done2 . 0)))
               (grab-a1 '(grab-a1 ((idle1 . 1) (resA . 1)) ((has-a1 . 1))))
               (grab-b1 '(grab-b1 ((has-a1 . 1) (resB . 1)) ((done1 . 1) (resA . 1) (resB . 1))))
               (grab-a2 '(grab-a2 ((idle2 . 1) (resA . 1)) ((has-a2 . 1))))
               (grab-b2 '(grab-b2 ((has-a2 . 1) (resB . 1)) ((done2 . 1) (resA . 1) (resB . 1))))
               (transitions (list grab-a1 grab-b1 grab-a2 grab-b2)))
          (push (list :ordered-no-deadlock
                      (funcall 'neovm--pn-explore-deadlock places transitions 6))
                results))

        (nreverse results))
    (fmakunbound 'neovm--pn-enabled-p)
    (fmakunbound 'neovm--pn-fire)
    (fmakunbound 'neovm--pn-tokens)
    (fmakunbound 'neovm--pn-deadlocked-p)
    (fmakunbound 'neovm--pn-explore-deadlock)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:always-deadlocks (:deadlock-found t :states-explored 2 :deadlock-marking ((p1 . 0)))) (:cyclic-no-deadlock (:deadlock-found nil :states-explored 127 :deadlock-marking nil)) (:resource-contention (:deadlock-found t :states-explored 2 :deadlock-marking ((idle1 . 0) (idle2 . 0) (resA . 1) (resB . 1) (has-a1 . 0) (has-b2 . 0) (done1 . 1) (done2 . 1)))) (:ordered-no-deadlock (:deadlock-found t :states-explored 2 :deadlock-marking ((idle1 . 0) (idle2 . 0) (resA . 1) (resB . 1) (has-a1 . 0) (has-a2 . 0) (done1 . 1) (done2 . 1)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Petri net invariant checking and reachability analysis
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_petri_net_invariant_checking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify place invariants (weighted sum of tokens is constant)
    // across all reachable markings.
    let form = r#"(progn
  (fset 'neovm--pn-enabled-p
    (lambda (places transition)
      (let ((inputs (nth 1 transition)) (ok t))
        (dolist (arc inputs ok)
          (when (< (or (cdr (assq (car arc) places)) 0) (cdr arc))
            (setq ok nil))))))

  (fset 'neovm--pn-fire
    (lambda (places transition)
      (let ((new-places (copy-sequence places)))
        (dolist (arc (nth 1 transition))
          (let ((entry (assq (car arc) new-places)))
            (if entry (setcdr entry (- (cdr entry) (cdr arc)))
              (push (cons (car arc) (- (cdr arc))) new-places))))
        (dolist (arc (nth 2 transition))
          (let ((entry (assq (car arc) new-places)))
            (if entry (setcdr entry (+ (cdr entry) (cdr arc)))
              (push (cons (car arc) (cdr arc)) new-places))))
        new-places)))

  (fset 'neovm--pn-tokens
    (lambda (places place) (or (cdr (assq place places)) 0)))

  ;; Compute weighted sum for a place invariant
  (fset 'neovm--pn-invariant-sum
    (lambda (places weights)
      "Compute sum of weight*tokens for each (place . weight) in WEIGHTS."
      (let ((sum 0))
        (dolist (w weights sum)
          (setq sum (+ sum (* (cdr w)
                              (funcall 'neovm--pn-tokens places (car w)))))))))

  (unwind-protect
      (let ((results nil))
        ;; Simple pipeline with token conservation
        ;; Invariant: tokens(p1) + tokens(p2) + tokens(p3) = constant
        (let* ((places '((p1 . 5) (p2 . 0) (p3 . 0)))
               (t1 '(t1 ((p1 . 1)) ((p2 . 1))))
               (t2 '(t2 ((p2 . 1)) ((p3 . 1))))
               (transitions (list t1 t2))
               (invariant '((p1 . 1) (p2 . 1) (p3 . 1)))
               (initial-sum (funcall 'neovm--pn-invariant-sum places invariant))
               (state places)
               (all-ok t)
               (step-sums nil))
          ;; Fire transitions and check invariant at each step
          (dotimes (_ 3)
            (setq state (funcall 'neovm--pn-fire state t1))
            (let ((s (funcall 'neovm--pn-invariant-sum state invariant)))
              (push s step-sums)
              (unless (= s initial-sum) (setq all-ok nil))))
          (dotimes (_ 2)
            (setq state (funcall 'neovm--pn-fire state t2))
            (let ((s (funcall 'neovm--pn-invariant-sum state invariant)))
              (push s step-sums)
              (unless (= s initial-sum) (setq all-ok nil))))
          (push (list :pipeline-invariant
                      :initial-sum initial-sum
                      :step-sums (nreverse step-sums)
                      :all-ok all-ok)
                results))

        ;; Mutual exclusion invariant:
        ;; tokens(mutex) + tokens(critical1) + tokens(critical2) = 1
        (let* ((places '((idle1 . 1) (idle2 . 1) (mutex . 1)
                         (critical1 . 0) (critical2 . 0)))
               (enter1 '(enter1 ((idle1 . 1) (mutex . 1)) ((critical1 . 1))))
               (leave1 '(leave1 ((critical1 . 1)) ((idle1 . 1) (mutex . 1))))
               (enter2 '(enter2 ((idle2 . 1) (mutex . 1)) ((critical2 . 1))))
               (leave2 '(leave2 ((critical2 . 1)) ((idle2 . 1) (mutex . 1))))
               (transitions (list enter1 leave1 enter2 leave2))
               (mutex-inv '((mutex . 1) (critical1 . 1) (critical2 . 1)))
               (state places)
               (initial-sum (funcall 'neovm--pn-invariant-sum state mutex-inv))
               (all-ok t))
          ;; Run a sequence of enters and leaves
          (dolist (tr (list enter1 leave1 enter2 leave2 enter1 leave1))
            (setq state (funcall 'neovm--pn-fire state tr))
            (unless (= (funcall 'neovm--pn-invariant-sum state mutex-inv)
                       initial-sum)
              (setq all-ok nil)))
          (push (list :mutex-invariant
                      :initial-sum initial-sum
                      :final-sum (funcall 'neovm--pn-invariant-sum state mutex-inv)
                      :all-ok all-ok
                      ;; Mutual exclusion: critical1 + critical2 <= 1 always
                      :mutual-exclusion (< (+ (funcall 'neovm--pn-tokens state 'critical1)
                                              (funcall 'neovm--pn-tokens state 'critical2))
                                           2))
                results))

        (nreverse results))
    (fmakunbound 'neovm--pn-enabled-p)
    (fmakunbound 'neovm--pn-fire)
    (fmakunbound 'neovm--pn-tokens)
    (fmakunbound 'neovm--pn-invariant-sum)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:pipeline-invariant :initial-sum 5 :step-sums (5 5 5 5 5) :all-ok t) (:mutex-invariant :initial-sum 1 :final-sum 1 :all-ok t :mutual-exclusion t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
