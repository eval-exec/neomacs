use expect_test::expect;

use super::ParityBatchCase;

fn two_slot_scheduler_preserves_fifo_under_saturation_and_cleans_every_job() -> ParityBatchCase {
    ParityBatchCase::value(
        "two_slot_scheduler_preserves_fifo_under_saturation_and_cleans_every_job",
        r##"
(let (table jobs starts finishes events cancelled
      (future-counter 0))
  (cl-letf (((symbol-function 'async-start)
             (lambda (start finish)
               (setq future-counter
                     (1+ future-counter))
               (push
                (list future-counter
                      (car start)
                      (and
                       (string-match-p
                        "\\(+\\|list\\)"
                        (prin1-to-string start))
                       t))
                starts)
               (push
                (cons future-counter finish)
                finishes)
               (intern
                (format
                 "future-%d"
                 future-counter))))
            ((symbol-function 'async-job-queue--make-timer)
             (lambda (&rest _)
               'fixture-timer))
            ((symbol-function 'cancel-timer)
             (lambda (timer)
               (push timer cancelled))))
    (setq table
          (async-job-queue-make-job-queue
           0.01 2
           (lambda (queue)
             (push
              (list
               'empty
               (async-job-queue-parity-table-state queue))
              events))
           nil nil nil 'fixed-two))
    (dolist (spec
             '((job-1 (+ 1 10))
               (job-2 (+ 2 20))
               (job-3 (list 3 30))
               (job-4 (list 4 40))))
      (let ((job
             (async-job-queue-schedule-job
              table
              (cadr spec)
              (car spec)
              (lambda (dispatched)
                (push
                 (list
                  'dispatch
                  (async-job-queue--job-id dispatched)
                  (async-job-queue--job-run-slot dispatched)
                  (async-job-queue--table-in-use table)
                  (queue-length
                   (async-job-queue--table-queue table)))
                 events))
              (lambda (finished value)
                (push
                 (list
                  'finish
                  (async-job-queue--job-id finished)
                  value
                  (async-job-queue--job-returned finished)
                  (async-job-queue--job-result finished)
                  (async-job-queue--job-table finished)
                  (async-job-queue--job-run-slot finished))
                 events)))))
        (push job jobs)))
    (let ((saturated
           (async-job-queue-parity-table-state table)))
      (funcall
       (cdr (assq 2 finishes))
       'value-2)
      (let ((after-second
             (async-job-queue-parity-table-state table)))
        (funcall
         (cdr (assq 1 finishes))
         'value-1)
        (funcall
         (cdr (assq 3 finishes))
         'value-3)
        (funcall
         (cdr (assq 4 finishes))
         'value-4)
        (list
         saturated
         after-second
         (async-job-queue-parity-table-state table)
         (nreverse starts)
         (nreverse events)
         (mapcar
          #'async-job-queue-parity-job-state
          (nreverse jobs))
         cancelled)))))
"##,
        expect![
            "OK ((:id fixed-two :active t :in-use 2 :free 0 :used-slots (0 1) :free-slots nil :queued 2 :timer t) (:id fixed-two :active t :in-use 2 :free 0 :used-slots (0 1) :free-slots nil :queued 1 :timer t) (:id fixed-two :active t :in-use 0 :free 2 :used-slots nil :free-slots (1 0) :queued 0 :timer nil) ((1 lambda t) (2 lambda t) (3 lambda t) (4 lambda t)) ((dispatch job-1 0 1 0) (dispatch job-2 1 2 0) (finish job-2 value-2 t value-2 nil nil) (dispatch job-3 1 2 1) (finish job-1 value-1 t value-1 nil nil) (dispatch job-4 0 2 0) (finish job-3 value-3 t value-3 nil nil) (finish job-4 value-4 t value-4 nil nil) (empty (:id fixed-two :active t :in-use 0 :free 2 :used-slots nil :free-slots (1 0) :queued 0 :timer t))) ((:id job-1 :table nil :run-slot nil :started t :future nil :ended t :returned t :result value-1) (:id job-2 :table nil :run-slot nil :started t :future nil :ended t :returned t :result value-2) (:id job-3 :table nil :run-slot nil :started t :future nil :ended t :returned t :result value-3) (:id job-4 :table nil :run-slot nil :started t :future nil :ended t :returned t :result value-4)) (fixture-timer))"
        ],
    )
}

fn inactive_queue_activation_and_deactivation_preserve_fifo_and_hook_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "inactive_queue_activation_and_deactivation_preserve_fifo_and_hook_order",
        r##"
(let (table jobs finishes events cancelled
      (future-counter 0))
  (cl-letf (((symbol-function 'async-start)
             (lambda (_start finish)
               (setq future-counter
                     (1+ future-counter))
               (push
                (cons future-counter finish)
                finishes)
               (intern
                (format
                 "activation-future-%d"
                 future-counter))))
            ((symbol-function 'async-job-queue--make-timer)
             (lambda (&rest _)
               'activation-timer))
            ((symbol-function 'cancel-timer)
             (lambda (timer)
               (push timer cancelled))))
    (setq table
          (async-job-queue-make-job-queue
           0.01 2 nil t nil nil 'gated))
    (async-job-queue-add-activation
     table
     (lambda (_ key)
       (push (list 'activate-a key) events)))
    (async-job-queue-add-activation
     table
     (lambda (_ key)
       (push (list 'activate-b key) events)))
    (async-job-queue-add-deactivation
     table
     (lambda (_ key)
       (push (list 'deactivate-a key) events)))
    (async-job-queue-add-deactivation
     table
     (lambda (_ key)
       (push (list 'deactivate-b key) events)))
    (dolist (id '(first second third))
      (push
       (async-job-queue-schedule-job
        table
        `(list ',id)
        id
        (lambda (job)
          (push
           (list
            'dispatch
            (async-job-queue--job-id job)
            (async-job-queue--job-run-slot job))
           events))
        (lambda (job value)
          (push
           (list
            'finish
            (async-job-queue--job-id job)
            value)
           events)))
       jobs))
    (let ((inactive
           (async-job-queue-parity-table-state table)))
      (async-job-queue-activate-queue table 'open-one)
      (let ((first-activation
             (async-job-queue-parity-table-state table)))
        (async-job-queue-deactivate-queue table 'pause)
        (funcall
         (cdr (assq 1 finishes))
         'first-value)
        (let ((paused
               (async-job-queue-parity-table-state table)))
          (async-job-queue-activate-queue table 'open-two)
          (funcall
           (cdr (assq 2 finishes))
           'second-value)
          (funcall
           (cdr (assq 3 finishes))
           'third-value)
          (list
           inactive
           first-activation
           paused
           (async-job-queue-parity-table-state table)
           (nreverse events)
           (mapcar
            #'async-job-queue-parity-job-state
            (nreverse jobs))
           cancelled))))))
"##,
        expect![
            "OK ((:id gated :active nil :in-use 0 :free 2 :used-slots nil :free-slots (0 1) :queued 3 :timer nil) (:id gated :active t :in-use 2 :free 0 :used-slots (0 1) :free-slots nil :queued 1 :timer t) (:id gated :active nil :in-use 1 :free 1 :used-slots (1) :free-slots (0) :queued 1 :timer t) (:id gated :active t :in-use 0 :free 2 :used-slots nil :free-slots (1 0) :queued 0 :timer nil) ((activate-b open-one) (activate-a open-one) (dispatch first 0) (dispatch second 1) (deactivate-b pause) (deactivate-a pause) (finish first first-value) (activate-b open-two) (activate-a open-two) (dispatch third 0) (finish second second-value) (finish third third-value)) ((:id first :table nil :run-slot nil :started t :future nil :ended t :returned t :result first-value) (:id second :table nil :run-slot nil :started t :future nil :ended t :returned t :result second-value) (:id third :table nil :run-slot nil :started t :future nil :ended t :returned t :result third-value)) (activation-timer))"
        ],
    )
}

fn dispatch_queued_fills_only_available_slots_and_never_bypasses_older_jobs() -> ParityBatchCase {
    ParityBatchCase::value(
        "dispatch_queued_fills_only_available_slots_and_never_bypasses_older_jobs",
        r##"
(let* ((table
        (async-job-queue-make-job-queue
         1 3 nil t nil nil 'manual))
       (queue
        (async-job-queue--table-queue table))
       starts)
  (dolist (id '(oldest middle newest))
    (queue-enqueue
     queue
     (async-job-queue--job-create
      :id id :table table
      :program `(list ',id))))
  (cl-letf (((symbol-function 'async-start)
             (lambda (_start _finish)
               (let* ((slot-index
                       (async-job-queue--table-last-used table))
                      (slot
                       (aref
                        (async-job-queue--table-slots table)
                        slot-index))
                      (job
                       (async-job-queue--slot-job slot)))
                 (push
                  (list
                   (async-job-queue--job-id job)
                   slot-index)
                  starts)
                 (intern
                  (format
                   "manual-%s"
                   (async-job-queue--job-id job)))))))
    (setf
     (async-job-queue--table-active table)
     nil)
    (let ((inactive-result
           (async-job-queue--dispatch-queued table)))
      (setf
       (async-job-queue--table-active table)
       t)
      (async-job-queue--dispatch-queued table)
      (list
       inactive-result
       (nreverse starts)
       (async-job-queue-parity-table-state table)
       (mapcar
        (lambda (index)
          (async-job-queue--job-id
           (async-job-queue--slot-job
            (aref
             (async-job-queue--table-slots table)
             index))))
        (async-job-queue--slots-in-use-list table))))))
"##,
        expect![
            "OK (nil ((oldest 0) (middle 1) (newest 2)) (:id manual :active t :in-use 3 :free 0 :used-slots (0 1 2) :free-slots nil :queued 0 :timer nil) (oldest middle newest))"
        ],
    )
}

fn callback_wrapper_returns_success_and_converts_each_user_error_to_warning() -> ParityBatchCase {
    ParityBatchCase::value(
        "callback_wrapper_returns_success_and_converts_each_user_error_to_warning",
        r##"
(let (calls warnings)
  (cl-letf (((symbol-function 'fixture-success)
             (lambda (&rest values)
               (push values calls)
               (apply #'+ values)))
            ((symbol-function 'fixture-failure)
             (lambda (value)
               (error
                "rejected callback %s"
                value)))
            ((symbol-function 'display-warning)
             (lambda (&rest args)
               (push args warnings)
               :warned)))
    (list
     (async-job-queue--call-with-warn
      #'fixture-success 1 2 3)
     (async-job-queue--call-with-warn
      #'fixture-failure 'payload)
     (nreverse calls)
     (nreverse warnings))))
"##,
        expect![[
            r#"OK (6 :warned ((1 2 3)) ((:error "error: (\"rejected callback payload\")")))"#
        ]],
    )
}

pub(super) fn dispatch_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        two_slot_scheduler_preserves_fifo_under_saturation_and_cleans_every_job(),
        inactive_queue_activation_and_deactivation_preserve_fifo_and_hook_order(),
        dispatch_queued_fills_only_available_slots_and_never_bypasses_older_jobs(),
        callback_wrapper_returns_success_and_converts_each_user_error_to_warning(),
    ]
}
