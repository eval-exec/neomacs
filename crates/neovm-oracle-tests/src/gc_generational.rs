//! Explicit-GC contracts and old-object weak/finalizer lifecycles.
//!
//! ONE nextest test runs ten independent oracle cases sequentially. This
//! keeps expect-test's RT/Patchwork in one process when refreshing this file.
//! Generate every expectation from the attested GNU 31.1 binary:
//! NEOVM_FORCE_ORACLE_PATH=/home/exec/.local/bin/emacs
//! NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1
//! Select gc_generational_contract_and_old_lifecycle_match_gnu in isolation.
//!
//! Return-field checks cover category/shape/type contracts. Absolute USED,
//! FREE, and SIZE parity is outside this battery: Neomacs currently reports
//! cumulative USED and zero FREE, and its object layouts differ from GNU's.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

const GENERATIONAL_MODES: &[&[(&str, &str)]] = &[
    &[("NEOVM_GC_GENERATIONAL", "0")],
    &[("NEOVM_GC_GENERATIONAL", "1")],
];

fn check_case(label: &str, form: &str, expected: expect_test::Expect) {
    tracing::info!(case = label, "starting generational GNU oracle case");
    crate::common::assert_oracle_parity_under_envs_expect(form, GENERATIONAL_MODES, expected);
    tracing::info!(case = label, "completed generational GNU oracle case");
}

fn old_weak_entries_form(weakness: &str) -> String {
    format!(
        r#"
(let ((gc-cons-threshold most-positive-fixnum)
      (gc-cons-percentage 1000)
      (post-gc-hook nil))
  ;; Finish the initial partition before creating session objects. They
  ;; must become ordinary old objects, rather than loadup permanents.
  (garbage-collect)
  (defun u34b--weak-setup ()
    (let ((table (make-hash-table :test 'eq :weakness '{weakness}))
          (hold (make-vector 8 nil))
          (i 0))
      (while (< i 4)
        (let ((key (cons i nil))
              (value (vector i)))
          (aset hold (* 2 i) key)
          (aset hold (1+ (* 2 i)) value)
          (puthash key value table))
        (setq i (1+ i)))
      (vector table hold)))
  (defun u34b--weak-release-selected (state)
    (let ((hold (aref state 1)))
      ;; Entry 0 keeps both roots; entry 1 keeps only its key; entry 2
      ;; keeps only its value; entry 3 loses both external roots.
      (aset hold 3 nil)
      (aset hold 4 nil)
      (aset hold 6 nil)
      (aset hold 7 nil))
    nil)
  (defun u34b--weak-release-all (state)
    (fillarray (aref state 1) nil)
    nil)
  ;; The setup frame has returned. The state vector roots the table and
  ;; every key/value while three explicit collections age them.
  (let ((state (u34b--weak-setup)))
    (garbage-collect)
    (garbage-collect)
    (garbage-collect)
    (let ((before (hash-table-count (aref state 0))))
      (u34b--weak-release-selected state)
      ;; Release frames have returned before reclamation. No dead key or
      ;; value remains in an evaluator argument array or lexical binding.
      (garbage-collect)
      (let ((selected (hash-table-count (aref state 0))))
        (u34b--weak-release-all state)
        (garbage-collect)
        (list (hash-table-weakness (aref state 0))
              before selected (hash-table-count (aref state 0)))))))
"#
    )
}

#[test]
fn gc_generational_contract_and_old_lifecycle_match_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Case 1: preserve the GNU return contract without disguising the
    // pre-existing cumulative-USED/zero-FREE statistics divergence.
    check_case(
        "explicit_gc_return_contract",
        r#"
(let ((gc-cons-threshold most-positive-fixnum)
      (gc-cons-percentage 1000)
      (post-gc-hook nil))
  (let ((stats (garbage-collect)))
    (mapcar
     (lambda (name)
       (let ((bucket (assq name stats)))
         (list name
               (length bucket)
               (eq (car bucket) name)
               (integerp (cadr bucket))
               (> (cadr bucket) 0)
               (mapcar (lambda (count)
                         (and (integerp count) (>= count 0)))
                       (cddr bucket)))))
     '(conses symbols strings string-bytes vectors vector-slots
              floats intervals buffers))))
"#,
        expect_test::expect![[
            r#""OK ((conses 4 t t t (t t)) (symbols 4 t t t (t t)) (strings 4 t t t (t t)) (string-bytes 3 t t t (t)) (vectors 3 t t t (t)) (vector-slots 4 t t t (t t)) (floats 4 t t t (t t)) (intervals 4 t t t (t t)) (buffers 3 t t t (t)))""#
        ]],
    );

    // Case 2: compile before sampling. Consume the constructed list so the
    // byte compiler cannot discard an unused cons expression. The control
    // exposes the snapshot list's own allocation instead of guessing it away.
    check_case(
        "exact_compiled_cons_memory_deltas",
        r#"
(let ((gc-cons-threshold most-positive-fixnum)
      (gc-cons-percentage 1000)
      (post-gc-hook nil))
  (require 'bytecomp)
  (let ((probe
         (byte-compile
          '(lambda (n)
             (let ((i 0) (tail nil) (before (car (memory-use-counts))))
               (while (< i n)
                 (setq tail (cons i tail))
                 (setq i (1+ i)))
               (cons (- (car (memory-use-counts)) before)
                     (length tail)))))))
    (garbage-collect)
    (let ((control (funcall probe 0))
          (small (funcall probe 17))
          (region-crossing (funcall probe 257)))
      (list control small region-crossing
            (- (car small) (car control))
            (- (car region-crossing) (car control))))))
"#,
        expect_test::expect![[r#""OK ((7 . 0) (24 . 17) (264 . 257) 17 257)""#]],
    );

    // Case 3: sample completed counters after setup/priming and exercise
    // both evaluator and bytecode explicit-GC builtin entries.
    check_case(
        "explicit_gcs_done_deltas",
        r#"
(let ((gc-cons-threshold most-positive-fixnum)
      (gc-cons-percentage 1000)
      (post-gc-hook nil))
  (require 'bytecomp)
  (let ((collect (byte-compile '(lambda () (garbage-collect)))))
    (garbage-collect)
    (let ((before gcs-done))
      (garbage-collect)
      (let ((after-eval gcs-done))
        (funcall collect)
        (let ((after-bytecode gcs-done))
          (garbage-collect)
          (list (- after-eval before)
                (- after-bytecode before)
                (- gcs-done before)))))))
"#,
        expect_test::expect![[r#""OK (1 2 3)""#]],
    );

    // Case 4: install the callback after priming. The hook does not
    // recursively collect or allocate a result list inside the observation.
    check_case(
        "post_gc_hook_once_per_explicit_collection",
        r#"
(let ((gc-cons-threshold most-positive-fixnum)
      (gc-cons-percentage 1000)
      (post-gc-hook nil))
  (defvar u34b-hook-count 0)
  (defun u34b--post-hook ()
    (setq u34b-hook-count (1+ u34b-hook-count)))
  (garbage-collect)
  (setq u34b-hook-count 0)
  (setq post-gc-hook (list #'u34b--post-hook))
  (let ((before gcs-done))
    (garbage-collect)
    (garbage-collect)
    (garbage-collect)
    (list u34b-hook-count (- gcs-done before))))
"#,
        expect_test::expect![[r#""OK (3 3)""#]],
    );

    // Cases 5-8: each has a fresh process/sandbox, its own independent
    // objects, three aging GCs, and both selective/all-root drop phases.
    check_case(
        "old_weak_key_after_explicit_major",
        &old_weak_entries_form("key"),
        expect_test::expect![[r#""OK (key 4 2 0)""#]],
    );
    check_case(
        "old_weak_value_after_explicit_major",
        &old_weak_entries_form("value"),
        expect_test::expect![[r#""OK (value 4 2 0)""#]],
    );
    check_case(
        "old_weak_key_or_value_after_explicit_major",
        &old_weak_entries_form("key-or-value"),
        expect_test::expect![[r#""OK (key-or-value 4 3 0)""#]],
    );
    check_case(
        "old_weak_key_and_value_after_explicit_major",
        &old_weak_entries_form("key-and-value"),
        expect_test::expect![[r#""OK (key-and-value 4 1 0)""#]],
    );

    // Case 9: the function refers only to a special counter, never to its
    // own finalizer. The drop helper returns before explicit reclamation.
    check_case(
        "old_finalizer_runs_once_after_explicit_major",
        r#"
(let ((gc-cons-threshold most-positive-fixnum)
      (gc-cons-percentage 1000)
      (post-gc-hook nil))
  (garbage-collect)
  (defvar u34b-finalizer-count 0)
  (defvar u34b-finalizer-root nil)
  (setq u34b-finalizer-count 0)
  (defun u34b--install-finalizer ()
    (setq u34b-finalizer-root
          (make-finalizer
           (lambda ()
             (setq u34b-finalizer-count (1+ u34b-finalizer-count)))))
    nil)
  (defun u34b--drop-finalizer ()
    (setq u34b-finalizer-root nil)
    nil)
  (u34b--install-finalizer)
  (garbage-collect)
  (garbage-collect)
  (garbage-collect)
  (let ((while-rooted u34b-finalizer-count))
    (u34b--drop-finalizer)
    (garbage-collect)
    (let ((after-first u34b-finalizer-count))
      (garbage-collect)
      (list while-rooted after-first u34b-finalizer-count))))
"#,
        expect_test::expect![[r#""OK (0 1 1)""#]],
    );

    // Case 10: observe ordering, not GNU/Neomacs' differing publication
    // time for gcs-done inside the finalizer. Each event snapshot is rooted.
    check_case(
        "old_finalizer_precedes_post_gc_hook",
        r#"
(let ((gc-cons-threshold most-positive-fixnum)
      (gc-cons-percentage 1000)
      (post-gc-hook nil))
  (garbage-collect)
  (defvar u34b-finalizer-events nil)
  (defvar u34b-finalizer-root nil)
  (setq u34b-finalizer-events nil)
  (defun u34b--install-finalizer ()
    (setq u34b-finalizer-root
          (make-finalizer
           (lambda ()
             (setq u34b-finalizer-events
                   (cons 'finalizer u34b-finalizer-events)))))
    nil)
  (defun u34b--drop-finalizer ()
    (setq u34b-finalizer-root nil)
    nil)
  (defun u34b--post-hook ()
    (setq u34b-finalizer-events (cons 'hook u34b-finalizer-events)))
  (u34b--install-finalizer)
  (garbage-collect)
  (garbage-collect)
  (garbage-collect)
  (setq u34b-finalizer-events nil)
  (setq post-gc-hook (list #'u34b--post-hook))
  (u34b--drop-finalizer)
  (garbage-collect)
  (let ((first (reverse u34b-finalizer-events)))
    (garbage-collect)
    (list first (reverse u34b-finalizer-events))))
"#,
        expect_test::expect![[r#""OK ((finalizer hook) (finalizer hook hook))""#]],
    );
}
