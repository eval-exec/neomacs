use expect_test::expect;

use super::ParityBatchCase;

fn finished_and_timed_out_handlers_reclaim_slots_and_set_distinct_terminal_state() -> ParityBatchCase
{
    ParityBatchCase::value(
        "finished_and_timed_out_handlers_reclaim_slots_and_set_distinct_terminal_state",
        r##"
(let* ((table
        (async-job-queue-make-job-queue
         1 2 nil t nil nil 'handlers))
       (success-slot
        (async-job-queue--alloc-slot table))
       (timeout-slot
        (async-job-queue--alloc-slot table))
       events deleted
       (success-job
        (async-job-queue--job-create
         :id 'success :table table
         :run-slot 0 :future 'success-future
         :succeed
         (lambda (job value)
           (push
            (list
             'success
             (async-job-queue--job-id job)
             value
             (async-job-queue--job-returned job)
             (async-job-queue--job-result job))
            events))))
       (timeout-job
        (async-job-queue--job-create
         :id 'timeout :table table
         :run-slot 1 :future 'timeout-future
         :timeout
         (lambda (job)
           (push
            (list
             'timeout
             (async-job-queue--job-id job)
             (async-job-queue--job-returned job)
             (async-job-queue--job-result job))
            events)))))
  (setf
   (async-job-queue--slot-job success-slot)
   success-job)
  (setf
   (async-job-queue--slot-job timeout-slot)
   timeout-job)
  (cl-letf (((symbol-function 'current-time)
             (lambda ()
               '(26000 20 0 0)))
            ((symbol-function 'delete-process)
             (lambda (future)
               (push future deleted))))
    (async-job-queue--handle-finished-job
     success-slot success-job '(:value 42))
    (async-job-queue--handle-terminated-job
     timeout-slot timeout-job 'timeout-future)
    (list
     (nreverse events)
     (nreverse deleted)
     (async-job-queue-parity-job-state success-job)
     (async-job-queue-parity-job-state timeout-job)
     (async-job-queue-parity-table-state table))))
"##,
        expect![
            "OK (((success success #1=(:value 42) t #1#) (timeout timeout nil nil)) (timeout-future) (:id success :table nil :run-slot nil :started nil :future nil :ended t :returned t :result #1#) (:id timeout :table nil :run-slot nil :started nil :future nil :ended t :returned nil :result nil) (:id handlers :active nil :in-use 0 :free 2 :used-slots nil :free-slots (0 1) :queued 0 :timer nil))"
        ],
    )
}

fn absent_terminal_callbacks_warn_after_cleanup_without_losing_terminal_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "absent_terminal_callbacks_warn_after_cleanup_without_losing_terminal_state",
        r##"
(let* ((table
        (async-job-queue-make-job-queue
         1 3 nil t nil nil 'absent-callbacks))
       (finished-slot
        (async-job-queue--alloc-slot table))
       (terminated-slot
        (async-job-queue--alloc-slot table))
       (cancelled-slot
        (async-job-queue--alloc-slot table))
       (finished
        (async-job-queue--job-create
         :id 'finished
         :table table
         :run-slot 0
         :future 'finished-process))
       (terminated
        (async-job-queue--job-create
         :id 'terminated
         :table table
         :run-slot 1
         :future 'terminated-process))
       (cancelled
        (async-job-queue--job-create
         :id 'cancelled
         :table table
         :run-slot 2
         :future 'cancelled-process))
       warnings deleted)
  (setf
   (async-job-queue--slot-job finished-slot)
   finished)
  (setf
   (async-job-queue--slot-job terminated-slot)
   terminated)
  (setf
   (async-job-queue--slot-job cancelled-slot)
   cancelled)
  (cl-letf (((symbol-function 'current-time)
             (lambda ()
               '(26000 25 0 0)))
            ((symbol-function 'delete-process)
             (lambda (future)
               (push future deleted)))
            ((symbol-function 'display-warning)
             (lambda (&rest arguments)
               (push arguments warnings)
               :warning-recorded)))
    (let ((returns
           (list
            (async-job-queue--handle-finished-job
             finished-slot finished '(:done 7))
            (async-job-queue--handle-terminated-job
             terminated-slot
             terminated
             'terminated-process)
            (async-job-queue-cancel-job cancelled))))
      (list
       returns
       (nreverse warnings)
       (nreverse deleted)
       (mapcar
        #'async-job-queue-parity-job-state
        (list finished terminated cancelled))
       (async-job-queue-parity-table-state table)))))
"##,
        expect![
            "OK ((:warning-recorded :warning-recorded :warning-recorded) ((:error \"void-function: (nil)\") (:error \"void-function: (nil)\") (:error \"void-function: (nil)\")) (terminated-process cancelled-process) ((:id finished :table nil :run-slot nil :started nil :future nil :ended t :returned t :result (:done 7)) (:id terminated :table nil :run-slot nil :started nil :future nil :ended t :returned nil :result nil) (:id cancelled :table nil :run-slot nil :started nil :future nil :ended t :returned nil :result nil)) (:id absent-callbacks :active nil :in-use 0 :free 3 :used-slots nil :free-slots (0 1 2) :queued 0 :timer nil))"
        ],
    )
}

fn process_termination_is_best_effort_and_reports_only_delete_failures() -> ParityBatchCase {
    ParityBatchCase::value(
        "process_termination_is_best_effort_and_reports_only_delete_failures",
        r##"
(let ((job
       (async-job-queue--job-create
        :id 'killable))
      deleted warnings)
  (cl-letf (((symbol-function 'delete-process)
             (lambda (future)
               (push future deleted)
               (when (eq future 'reject-delete)
                 (error "cannot delete"))))
            ((symbol-function 'display-warning)
             (lambda (type message &rest args)
               (push
                (list
                 type
                 (and
                  (string-match-p
                   "Could not kill process"
                   message)
                  t)
                 (and
                  (string-match-p
                   "reject-delete"
                   message)
                  t)
                 args)
                warnings)
               :warning-recorded)))
    (list
     (async-job-queue--terminate-job-process
      job 'accept-delete)
     (async-job-queue--terminate-job-process
      job 'reject-delete)
     (nreverse deleted)
     (nreverse warnings))))
"##,
        expect!["OK (nil :warning-recorded (accept-delete reject-delete) ((:warning t t nil)))"],
    )
}

fn direct_cancel_distinguishes_running_and_still_enqueued_job_lifecycle() -> ParityBatchCase {
    ParityBatchCase::value(
        "direct_cancel_distinguishes_running_and_still_enqueued_job_lifecycle",
        r##"
(let* ((table
        (async-job-queue-make-job-queue
         1 1 nil t nil nil 'cancel-direct))
       (slot
        (async-job-queue--alloc-slot table))
       events deleted
       (running
        (async-job-queue--job-create
         :id 'running :table table
         :run-slot 0 :future 'running-future
         :quit
         (lambda (job)
           (push
            (list
             'quit
             (async-job-queue--job-id job)
             (async-job-queue--job-table job)
             (async-job-queue--job-run-slot job))
            events))))
       (queued
        (async-job-queue--job-create
         :id 'queued :table table
         :quit
         (lambda (job)
           (push
            (list
             'quit
             (async-job-queue--job-id job)
             (async-job-queue--job-table job)
             (async-job-queue--job-run-slot job))
            events)))))
  (setf
   (async-job-queue--slot-job slot)
   running)
  (queue-enqueue
   (async-job-queue--table-queue table)
   queued)
  (cl-letf (((symbol-function 'delete-process)
             (lambda (future)
               (push future deleted)))
            ((symbol-function 'current-time)
             (lambda ()
               '(26000 30 0 0))))
    (async-job-queue-cancel-job running)
    (async-job-queue-cancel-job queued)
    (let ((still-queued
           (queue-dequeue
            (async-job-queue--table-queue table))))
      (list
       (nreverse events)
       (nreverse deleted)
       (async-job-queue-parity-job-state running)
       (async-job-queue-parity-job-state queued)
       (eq still-queued queued)
       (async-job-queue-parity-job-state still-queued)
       (async-job-queue-parity-table-state table)))))
"##,
        expect![
            "OK (((quit running nil nil) (quit queued nil nil)) (running-future) (:id running :table nil :run-slot nil :started nil :future nil :ended t :returned nil :result nil) (:id queued :table nil :run-slot nil :started nil :future nil :ended t :returned nil :result nil) t (:id queued :table nil :run-slot nil :started nil :future nil :ended t :returned nil :result nil) (:id cancel-direct :active nil :in-use 0 :free 1 :used-slots nil :free-slots (0) :queued 0 :timer nil))"
        ],
    )
}

fn cancelling_entire_queue_rejects_fifo_pending_then_active_jobs_and_stops_timer() -> ParityBatchCase
{
    ParityBatchCase::value(
        "cancelling_entire_queue_rejects_fifo_pending_then_active_jobs_and_stops_timer",
        r##"
(let* ((table
        (async-job-queue-make-job-queue
         1 2 nil t nil nil 'cancel-all))
       (slots
        (async-job-queue--table-slots table))
       jobs events deleted timers)
  (dolist (spec
           '((active-1 active-future-1)
             (active-2 active-future-2)))
    (let* ((slot
            (async-job-queue--alloc-slot table))
           (job
            (async-job-queue--job-create
             :id (car spec)
             :table table
             :run-slot
             (async-job-queue--slot-index slot)
             :future (cadr spec)
             :quit
             (lambda (cancelled)
               (push
                (list
                 'quit
                 (async-job-queue--job-id cancelled))
                events)))))
      (setf
       (async-job-queue--slot-job slot)
       job)
      (push job jobs)))
  (dolist (id '(queued-1 queued-2))
    (let ((job
           (async-job-queue--job-create
            :id id :table table
            :quit
            (lambda (cancelled)
              (push
               (list
                'quit
                (async-job-queue--job-id cancelled))
               events)))))
      (queue-enqueue
       (async-job-queue--table-queue table)
       job)
      (push job jobs)))
  (setf
   (async-job-queue--table-timer table)
   'queue-timer)
  (cl-letf (((symbol-function 'delete-process)
             (lambda (future)
               (push future deleted)))
            ((symbol-function 'cancel-timer)
             (lambda (timer)
               (push timer timers)))
            ((symbol-function 'current-time)
             (lambda ()
               '(26000 40 0 0))))
    (async-job-queue-cancel-job-queue table)
    (list
     (nreverse events)
     (sort
      deleted
      (lambda (left right)
        (string<
         (symbol-name left)
         (symbol-name right))))
     timers
     (async-job-queue-parity-table-state table)
     (mapcar
      #'async-job-queue-displayable-slot
      (append slots nil))
     (mapcar
      #'async-job-queue-parity-job-state
      (nreverse jobs)))))
"##,
        expect![
            "OK (((quit queued-1) (quit queued-2) (quit active-1) (quit active-2)) (active-future-1 active-future-2) (queue-timer) (:id cancel-all :active nil :in-use 0 :free 2 :used-slots nil :free-slots (0 1) :queued 0 :timer nil) ((async-job-queue--slot (table cancel-all) (index 0) (next 1) (prev nil) (job nil)) (async-job-queue--slot (table cancel-all) (index 1) (next nil) (prev nil) (job nil))) ((:id active-1 :table nil :run-slot nil :started nil :future nil :ended t :returned nil :result nil) (:id active-2 :table nil :run-slot nil :started nil :future nil :ended t :returned nil :result nil) (:id queued-1 :table nil :run-slot nil :started nil :future nil :ended t :returned nil :result nil) (:id queued-2 :table nil :run-slot nil :started nil :future nil :ended t :returned nil :result nil)))"
        ],
    )
}

fn polling_processes_ready_timeout_and_pending_jobs_across_mutating_slot_lists() -> ParityBatchCase
{
    ParityBatchCase::value(
        "polling_processes_ready_timeout_and_pending_jobs_across_mutating_slot_lists",
        r##"
(let* ((table
        (async-job-queue-make-job-queue
         1 3 nil nil nil nil 'poll))
       (slots
        (async-job-queue--table-slots table))
       events deleted timer-created
       jobs)
  (dolist (spec
           '((pending pending-future 20)
             (ready ready-future nil)
             (timeout timeout-future 5)))
    (let* ((slot
            (async-job-queue--alloc-slot table))
           (id (car spec))
           (job
            (async-job-queue--job-create
             :id id :table table
             :run-slot
             (async-job-queue--slot-index slot)
             :future (cadr spec)
             :started (seconds-to-time 90)
             :max-time (nth 2 spec)
             :succeed
             (lambda (finished value)
               (push
                (list
                 'success
                 (async-job-queue--job-id finished)
                 value)
                events))
             :timeout
             (lambda (timed-out)
               (push
                (list
                 'timeout
                 (async-job-queue--job-id timed-out))
                events)))))
      (setf
       (async-job-queue--slot-job slot)
       job)
      (push job jobs)))
  (cl-letf (((symbol-function 'async-ready)
             (lambda (future)
               (eq future 'ready-future)))
            ((symbol-function 'async-get)
             (lambda (future)
               (list 'value-from future)))
            ((symbol-function 'delete-process)
             (lambda (future)
               (push future deleted)))
            ((symbol-function 'current-time)
             (lambda ()
               (seconds-to-time 100)))
            ((symbol-function 'async-job-queue--make-timer)
             (lambda (&rest _)
               (setq timer-created t)
               'poll-timer)))
    (async-job-queue--process-queue table)
    (let ((after-first
           (async-job-queue-parity-table-state table)))
      (async-job-queue--process-queue table)
      (list
       after-first
       (async-job-queue-parity-table-state table)
       (nreverse events)
       (nreverse deleted)
       timer-created
       (mapcar
        #'async-job-queue-parity-job-state
        (nreverse jobs))))))
"##,
        expect![
            "OK ((:id poll :active t :in-use 2 :free 1 :used-slots (0 2) :free-slots (1) :queued 0 :timer t) (:id poll :active t :in-use 1 :free 2 :used-slots (0) :free-slots (1 2) :queued 0 :timer t) ((success ready #1=(value-from ready-future)) (timeout timeout)) (timeout-future) t ((:id pending :table poll :run-slot 0 :started t :future pending-future :ended nil :returned nil :result nil) (:id ready :table nil :run-slot nil :started t :future nil :ended t :returned t :result #1#) (:id timeout :table nil :run-slot nil :started t :future nil :ended t :returned nil :result nil)))"
        ],
    )
}

pub(super) fn lifecycle_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        finished_and_timed_out_handlers_reclaim_slots_and_set_distinct_terminal_state(),
        absent_terminal_callbacks_warn_after_cleanup_without_losing_terminal_state(),
        process_termination_is_best_effort_and_reports_only_delete_failures(),
        direct_cancel_distinguishes_running_and_still_enqueued_job_lifecycle(),
        cancelling_entire_queue_rejects_fifo_pending_then_active_jobs_and_stops_timer(),
        polling_processes_ready_timeout_and_pending_jobs_across_mutating_slot_lists(),
    ]
}
