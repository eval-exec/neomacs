use expect_test::expect;

use super::ParityBatchCase;

fn queue_creation_builds_fixed_doubly_linked_slots_and_stable_display_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "queue_creation_builds_fixed_doubly_linked_slots_and_stable_display_state",
        r##"
(let ((async-job-queue--num-tables-created 0)
      (async-job-queue-default-size 3))
  (let ((active
         (async-job-queue-make-job-queue
          0.25 nil #'ignore nil '(activate-a) '(deactivate-a)))
        (inactive
         (async-job-queue-make-job-queue
          1 2 nil t nil nil 'fixture-inactive)))
    (list
     (async-job-queue-parity-table-state active)
     (mapcar
      #'async-job-queue-displayable-slot
      (append
       (async-job-queue--table-slots active)
       nil))
     (async-job-queue-displayable-table inactive)
     async-job-queue--num-tables-created)))
"##,
        expect![
            "OK ((:id async-job-queue-table-1 :active t :in-use 0 :free 3 :used-slots nil :free-slots (0 1 2) :queued 0 :timer nil) ((async-job-queue--slot (table async-job-queue-table-1) (index 0) (next 1) (prev nil) (job nil)) (async-job-queue--slot (table async-job-queue-table-1) (index 1) (next 2) (prev 0) (job nil)) (async-job-queue--slot (table async-job-queue-table-1) (index 2) (next nil) (prev 1) (job nil))) (async-job-queue--table (id fixture-inactive) (slots 2) (active nil) (in-use 0 nil nil nil) (free 2 0 1 (0 1)) (queue 0) (on-empty nil) (freq 1) (timer nil)) 2)"
        ],
    )
}

fn zero_slot_public_construction_signals_before_a_queue_can_be_scheduled() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_slot_public_construction_signals_before_a_queue_can_be_scheduled",
        r##"
(let ((async-job-queue--num-tables-created 0))
  (let ((explicit
         (condition-case error-data
             (progn
               (async-job-queue-make-job-queue
                1 0 nil nil nil nil 'zero-explicit)
               :missing-error)
           (error
            error-data)))
        (defaulted
         (let ((async-job-queue-default-size 0))
           (condition-case error-data
               (progn
                 (async-job-queue-make-job-queue
                  1 nil nil nil nil nil 'zero-default)
                 :missing-error)
             (error
              error-data)))))
    (list
     explicit
     defaulted
     async-job-queue--num-tables-created)))
"##,
        expect![
            "OK ((wrong-type-argument async-job-queue--slot nil) (wrong-type-argument async-job-queue--slot nil) 2)"
        ],
    )
}

fn slot_allocation_reclamation_and_fifo_reuse_preserve_all_list_invariants() -> ParityBatchCase {
    ParityBatchCase::value(
        "slot_allocation_reclamation_and_fifo_reuse_preserve_all_list_invariants",
        r##"
(let* ((table
        (async-job-queue-make-job-queue
         1 3 nil t nil nil 'slots))
       (slot-0
        (async-job-queue--alloc-slot table))
       state-after-zero
       slot-1 slot-2 reclaimed-1
       state-full state-middle-free
       reused)
  (setf
   (async-job-queue--slot-job slot-0)
   (async-job-queue--job-create
    :id 'job-zero))
  (setq state-after-zero
        (async-job-queue-parity-table-state table))
  (setq slot-1
        (async-job-queue--alloc-slot table))
  (setf
   (async-job-queue--slot-job slot-1)
   (async-job-queue--job-create
    :id 'job-one))
  (setq slot-2
        (async-job-queue--alloc-slot table))
  (setf
   (async-job-queue--slot-job slot-2)
   (async-job-queue--job-create
    :id 'job-two))
  (setq state-full
        (async-job-queue-parity-table-state table))
  (setq reclaimed-1
        (async-job-queue--reclaim-slot slot-1))
  (setq state-middle-free
        (async-job-queue-parity-table-state table))
  (async-job-queue--reclaim-slot slot-0)
  (setq reused
        (async-job-queue--alloc-slot table))
  (setf
   (async-job-queue--slot-job reused)
   (async-job-queue--job-create
    :id 'job-reused))
  (list
   (mapcar
    #'async-job-queue--slot-index
    (list slot-0 slot-1 slot-2 reused))
   reclaimed-1
   state-after-zero
   state-full
   state-middle-free
   (async-job-queue-parity-table-state table)
   (mapcar
    #'async-job-queue-displayable-slot
    (append
     (async-job-queue--table-slots table)
     nil))))
"##,
        expect![
            "OK ((0 1 2 1) #s(async-job-queue--job job-one nil nil nil nil nil nil nil nil nil nil nil nil nil) (:id slots :active nil :in-use 1 :free 2 :used-slots (0) :free-slots (1 2) :queued 0 :timer nil) (:id slots :active nil :in-use 3 :free 0 :used-slots (0 1 2) :free-slots nil :queued 0 :timer nil) (:id slots :active nil :in-use 2 :free 1 :used-slots (0 2) :free-slots (1) :queued 0 :timer nil) (:id slots :active nil :in-use 2 :free 1 :used-slots (2 1) :free-slots (0) :queued 0 :timer nil) ((async-job-queue--slot (table slots) (index 0) (next nil) (prev nil) (job nil)) (async-job-queue--slot (table slots) (index 1) (next nil) (prev 2) (job job-reused)) (async-job-queue--slot (table slots) (index 2) (next 1) (prev nil) (job job-two))))"
        ],
    )
}

fn allocation_and_double_reclamation_fail_atomically_with_named_conditions() -> ParityBatchCase {
    ParityBatchCase::value(
        "allocation_and_double_reclamation_fail_atomically_with_named_conditions",
        r##"
(let* ((table
        (async-job-queue-make-job-queue
         1 1 nil t nil nil 'errors))
       (slot
        (async-job-queue--alloc-slot table))
       allocation-signal
       reclaim-signal)
  (setq allocation-signal
        (condition-case error-data
            (progn
              (async-job-queue--alloc-slot table)
              :missing)
          (error
           (car error-data))))
  (setf
   (async-job-queue--slot-job slot)
   'only-job)
  (let ((returned
         (async-job-queue--reclaim-slot slot)))
    (setq reclaim-signal
          (condition-case error-data
              (progn
                (async-job-queue--reclaim-slot slot)
                :missing)
            (error
             (car error-data))))
    (list
     allocation-signal
     returned
     reclaim-signal
     (async-job-queue-parity-table-state table)
     (async-job-queue-displayable-slot slot))))
"##,
        expect![
            "OK (async-job-queue--table-no-free-slot only-job async-job-queue-slot-already-free (:id errors :active nil :in-use 0 :free 1 :used-slots nil :free-slots (0) :queued 0 :timer nil) (async-job-queue--slot (table errors) (index 0) (next nil) (prev nil) (job nil)))"
        ],
    )
}

fn generated_struct_constructors_copies_accessors_and_setters_have_value_semantics()
-> ParityBatchCase {
    ParityBatchCase::value(
        "generated_struct_constructors_copies_accessors_and_setters_have_value_semantics",
        r##"
(let* ((queue
        (async-job-queue--queue-create
         :head 'alpha :last 'omega))
       (queue-copy
        (async-job-queue--queue-copy queue))
       (table
        (async-job-queue-make-job-queue
         2 1 nil t nil nil 'original))
       (table-copy
        (async-job-queue--table-copy table))
       (slot
        (aref
         (async-job-queue--table-slots table)
         0))
       (slot-copy
        (async-job-queue--slot-copy slot))
       (job
        (async-job-queue--job-create
         :id 'job-a :table table
         :program '(+ 1 2)
         :max-time 7))
       (job-copy
        (async-job-queue--job-copy job)))
  (setf
   (async-job-queue--queue-head queue-copy)
   'changed)
  (setf
   (async-job-queue--table-id table-copy)
   'copy)
  (setf
   (async-job-queue--slot-index slot-copy)
   9)
  (setf
   (async-job-queue--job-id job-copy)
   'job-copy)
  (list
   (list
    (async-job-queue--queue-p queue)
    (async-job-queue--queue-head queue)
    (async-job-queue--queue-last queue)
    (async-job-queue--queue-head queue-copy))
   (list
    (async-job-queue--table-p table)
    (async-job-queue--table-id table)
    (async-job-queue--table-id table-copy)
    (eq
     (async-job-queue--table-slots table)
     (async-job-queue--table-slots table-copy)))
   (list
    (async-job-queue--slot-p slot)
    (async-job-queue--slot-index slot)
    (async-job-queue--slot-index slot-copy)
    (eq
     (async-job-queue--slot-table slot)
     (async-job-queue--slot-table slot-copy)))
   (list
    (async-job-queue--job-p job)
    (async-job-queue--job-id job)
    (async-job-queue--job-id job-copy)
    (equal
     (async-job-queue--job-program job)
     (async-job-queue--job-program job-copy)))))
"##,
        expect!["OK ((t alpha omega changed) (t original copy t) (t 0 9 t) (t job-a job-copy t))"],
    )
}

fn display_helpers_and_job_slot_cover_nil_queued_running_and_completed_shapes() -> ParityBatchCase {
    ParityBatchCase::value(
        "display_helpers_and_job_slot_cover_nil_queued_running_and_completed_shapes",
        r##"
(let* ((table
        (async-job-queue-make-job-queue
         0.5 2 nil t nil nil 'display))
       (job
        (async-job-queue--job-create
         :id 'report
         :table table
         :program '(list 1 2)
         :started '(26000 10 0 0)
         :max-time 12
         :future 'future-token
         :ended '(26000 20 0 0)
         :returned t
         :result '(:ok 42)
         :dispatched #'ignore
         :succeed #'ignore
         :timeout #'ignore
         :quit #'ignore))
       (slot
        (async-job-queue--alloc-slot table)))
  (setf
   (async-job-queue--slot-job slot)
   job)
  (setf
   (async-job-queue--job-run-slot job)
   (async-job-queue--slot-index slot))
  (list
   (async-job-queue-displayable-table nil)
   (async-job-queue-displayable-slot nil)
   (async-job-queue-displayable-job nil)
   (async-job-queue-displayable-table table)
   (async-job-queue-displayable-slot slot)
   (async-job-queue-displayable-job job)
   (eq
    slot
    (async-job-queue--job-slot job))
   (progn
     (setf
      (async-job-queue--job-run-slot job)
      nil)
     (async-job-queue--job-slot job))))
"##,
        expect![
            "OK ((async-job-queue--table nil) (async-job-queue--slot nil) (async-job-queue--job nil) (async-job-queue--table (id display) (slots 2) (active nil) (in-use 1 0 0 (0)) (free 1 1 1 (1)) (queue 0) (on-empty nil) (freq 0.5) (timer nil)) (async-job-queue--slot (table display) (index 0) (next nil) (prev nil) (job report)) (async-job-queue--job (id report) (table display) (run-slot 0) (started (26000 10 0 0)) (ended (26000 20 0 0)) (max-time 12) (future future-token) (returned t) (result (:ok 42)) (dispatched t) (succeed t) (timeout t) (quit t)) t nil)"
        ],
    )
}

fn expression_conversion_and_cycle_safe_slot_walks_match_exactly() -> ParityBatchCase {
    ParityBatchCase::value(
        "expression_conversion_and_cycle_safe_slot_walks_match_exactly",
        r##"
(let* ((closure
        (let ((captured 4))
          (lambda ()
            captured)))
       (lambda-form
        '(lambda () :lambda))
       (function-form
        '(function (lambda () :function)))
       (table
        (async-job-queue-make-job-queue
         1 3 nil t nil nil 'cycle))
       (slot-0
        (async-job-queue--alloc-slot table))
       (slot-1
        (async-job-queue--alloc-slot table)))
  (setf
   (async-job-queue--slot-next slot-1)
   0)
  (list
   (eq
    lambda-form
    (async-job-queue--expr-to-async
     lambda-form))
   (eq
    function-form
    (async-job-queue--expr-to-async
     function-form))
   (eq
    closure
    (async-job-queue--expr-to-async
     closure))
   (async-job-queue--expr-to-async
    'named-symbol)
   (async-job-queue--expr-to-async
    '(+ 1 2))
   (async-job-queue--expr-to-async
    42)
   (async-job-queue--slots-in-use-list
    table)
   (async-job-queue--slots-free-list
    table)))
"##,
        expect![
            "OK (t t t (lambda nil named-symbol) (lambda nil (+ 1 2)) (lambda nil 42) (0 1 0) (2))"
        ],
    )
}

pub(super) fn structures_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        queue_creation_builds_fixed_doubly_linked_slots_and_stable_display_state(),
        zero_slot_public_construction_signals_before_a_queue_can_be_scheduled(),
        slot_allocation_reclamation_and_fifo_reuse_preserve_all_list_invariants(),
        allocation_and_double_reclamation_fail_atomically_with_named_conditions(),
        generated_struct_constructors_copies_accessors_and_setters_have_value_semantics(),
        display_helpers_and_job_slot_cover_nil_queued_running_and_completed_shapes(),
        expression_conversion_and_cycle_safe_slot_walks_match_exactly(),
    ]
}
