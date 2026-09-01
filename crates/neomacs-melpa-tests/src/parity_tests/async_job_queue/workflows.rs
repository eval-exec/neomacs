use expect_test::expect;

use super::ParityBatchCase;

fn real_five_job_workflow_never_exceeds_two_slots_and_dispatches_fifo() -> ParityBatchCase {
    ParityBatchCase::value(
        "real_five_job_workflow_never_exceeds_two_slots_and_dispatches_fifo",
        r##"
(let (table jobs dispatch-order completions
      (finished 0)
      (empty-count 0)
      (maximum-in-use 0))
  (setq table
        (async-job-queue-make-job-queue
         0.01 2
         (lambda (_)
           (setq empty-count
                 (1+ empty-count)))
         nil nil nil 'real-success))
  (unwind-protect
      (progn
        (dolist
            (spec
             '((job-1 0.08 (:value 1))
               (job-2 0.01 (:value 2))
               (job-3 0.03 (:value 3))
               (job-4 0.02 (:value 4))
               (job-5 0.01 (:value 5))))
          (let ((job
                 (async-job-queue-schedule-job
                  table
                  `(progn
                     (sleep-for ,(cadr spec))
                     ',(nth 2 spec))
                  (car spec)
                  (lambda (dispatched)
                    (push
                     (async-job-queue--job-id dispatched)
                     dispatch-order)
                    (setq maximum-in-use
                          (max
                           maximum-in-use
                           (async-job-queue--table-in-use table))))
                  (lambda (completed value)
                    (push
                     (list
                      (async-job-queue--job-id completed)
                      value
                      (async-job-queue--job-returned completed)
                      (async-job-queue--job-result completed)
                      (async-job-queue--job-table completed)
                      (async-job-queue--job-run-slot completed))
                     completions)
                    (setq finished
                          (1+ finished))))))
            (push job jobs)))
        (async-job-queue-parity-wait-until
         (lambda ()
           (= finished 5)))
        (async-job-queue-parity-wait-until
         (lambda ()
           (and
            (= 0
               (async-job-queue--table-in-use table))
            (= 0
               (queue-length
                (async-job-queue--table-queue table)))
            (null
             (async-job-queue--table-timer table)))))
        (list
         (nreverse dispatch-order)
         maximum-in-use
         (sort
          completions
          (lambda (left right)
            (string<
             (symbol-name (car left))
             (symbol-name (car right)))))
         empty-count
         (async-job-queue-parity-normalized-table-state table)
         (mapcar
          #'async-job-queue-parity-job-state
          (nreverse jobs))))
    (async-job-queue-cancel-job-queue table)))
"##,
        expect![
            "OK ((job-1 job-2 job-3 job-4 job-5) 2 ((job-1 #1=(:value 1) t #1# nil nil) (job-2 #2=(:value 2) t #2# nil nil) (job-3 #3=(:value 3) t #3# nil nil) (job-4 #4=(:value 4) t #4# nil nil) (job-5 #5=(:value 5) t #5# nil nil)) 1 (:id real-success :active t :in-use 0 :free 2 :used-slots nil :free-slots (0 1) :queued 0 :timer nil) ((:id job-1 :table nil :run-slot nil :started t :future nil :ended t :returned t :result #1#) (:id job-2 :table nil :run-slot nil :started t :future nil :ended t :returned t :result #2#) (:id job-3 :table nil :run-slot nil :started t :future nil :ended t :returned t :result #3#) (:id job-4 :table nil :run-slot nil :started t :future nil :ended t :returned t :result #4#) (:id job-5 :table nil :run-slot nil :started t :future nil :ended t :returned t :result #5#)))"
        ],
    )
}

fn real_timeout_rejects_long_job_without_success_and_releases_the_only_slot() -> ParityBatchCase {
    ParityBatchCase::value(
        "real_timeout_rejects_long_job_without_success_and_releases_the_only_slot",
        r##"
(let (table job events
      (timed-out nil)
      (succeeded nil))
  (setq table
        (async-job-queue-make-job-queue
         0.01 1 nil nil nil nil 'real-timeout))
  (unwind-protect
      (progn
        (setq job
              (async-job-queue-schedule-job
               table
               '(progn
                  (sleep-for 10)
                  :too-late)
               'slow
               (lambda (dispatched)
                 (push
                  (list
                   'dispatch
                   (async-job-queue--job-id dispatched)
                   (async-job-queue--job-run-slot dispatched))
                  events))
               (lambda (completed value)
                 (setq succeeded t)
                 (push
                  (list
                   'success
                   (async-job-queue--job-id completed)
                   value)
                  events))
               0.05
               (lambda (rejected)
                 (setq timed-out t)
                 (push
                  (list
                   'timeout
                   (async-job-queue--job-id rejected)
                   (async-job-queue--job-future rejected)
                   (async-job-queue--job-table rejected)
                   (async-job-queue--job-run-slot rejected))
                  events))))
        (async-job-queue-parity-wait-until
         (lambda ()
           timed-out))
        (async-job-queue-parity-wait-until
         (lambda ()
           (and
            (= 0
               (async-job-queue--table-in-use table))
            (null
             (async-job-queue--table-timer table)))))
        (list
         succeeded
         (nreverse events)
         (async-job-queue-parity-job-state job)
         (async-job-queue-parity-table-state table)))
    (async-job-queue-cancel-job-queue table)))
"##,
        expect![
            "OK (nil ((dispatch slow 0) (timeout slow nil nil nil)) (:id slow :table nil :run-slot nil :started t :future nil :ended t :returned nil :result nil) (:id real-timeout :active t :in-use 0 :free 1 :used-slots nil :free-slots (0) :queued 0 :timer nil))"
        ],
    )
}

fn real_queue_cancellation_rejects_pending_before_running_and_kills_process() -> ParityBatchCase {
    ParityBatchCase::value(
        "real_queue_cancellation_rejects_pending_before_running_and_kills_process",
        r##"
(let (table running queued events)
  (setq table
        (async-job-queue-make-job-queue
         0.01 1 nil nil nil nil 'real-cancel))
  (unwind-protect
      (progn
        (setq running
              (async-job-queue-schedule-job
               table
               '(progn
                  (sleep-for 10)
                  :running-finished)
               'running
               nil nil nil nil
               (lambda (cancelled)
                 (push
                  (list
                   'quit
                   (async-job-queue--job-id cancelled)
                   (async-job-queue--job-future cancelled)
                   (async-job-queue--job-table cancelled))
                  events))))
        (setq queued
              (async-job-queue-schedule-job
               table
               ':queued-finished
               'queued
               nil nil nil nil
               (lambda (cancelled)
                 (push
                  (list
                   'quit
                   (async-job-queue--job-id cancelled)
                   (async-job-queue--job-future cancelled)
                   (async-job-queue--job-table cancelled))
                  events))))
        (async-job-queue-parity-wait-until
         (lambda ()
           (and
            (async-job-queue--job-future running)
            (= 1
               (queue-length
                (async-job-queue--table-queue table))))))
        (async-job-queue-cancel-job-queue table)
        (list
         (nreverse events)
         (async-job-queue-parity-job-state running)
         (async-job-queue-parity-job-state queued)
         (async-job-queue-parity-table-state table)))
    (async-job-queue-cancel-job-queue table)))
"##,
        expect![
            "OK (((quit queued nil nil) (quit running nil nil)) (:id running :table nil :run-slot nil :started t :future nil :ended t :returned nil :result nil) (:id queued :table nil :run-slot nil :started nil :future nil :ended t :returned nil :result nil) (:id real-cancel :active t :in-use 0 :free 1 :used-slots nil :free-slots (0) :queued 0 :timer nil))"
        ],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        real_five_job_workflow_never_exceeds_two_slots_and_dispatches_fifo(),
        real_timeout_rejects_long_job_without_success_and_releases_the_only_slot(),
        real_queue_cancellation_rejects_pending_before_running_and_kills_process(),
    ]
}
