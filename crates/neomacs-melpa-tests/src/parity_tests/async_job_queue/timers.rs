use expect_test::expect;

use super::ParityBatchCase;

fn timer_factory_routes_numeric_and_absolute_frequencies_with_exact_repeat_policy()
-> ParityBatchCase {
    ParityBatchCase::value(
        "timer_factory_routes_numeric_and_absolute_frequencies_with_exact_repeat_policy",
        r##"
(let (calls)
  (cl-letf (((symbol-function 'run-with-timer)
             (lambda (&rest args)
               (push
                (cons 'run-with-timer args)
                calls)
               'numeric-timer))
            ((symbol-function 'run-at-time)
             (lambda (&rest args)
               (push
                (cons 'run-at-time args)
                calls)
               'absolute-timer)))
    (list
     (async-job-queue--make-timer
      0.25 t #'ignore 'alpha 2)
     (async-job-queue--make-timer
      4 nil #'list 'beta)
     (async-job-queue--make-timer
      '(26000 12 0 0) t #'vector 'gamma)
     (nreverse calls))))
"##,
        expect![
            "OK (numeric-timer numeric-timer absolute-timer ((run-with-timer 0.25 0.25 ignore alpha 2) (run-with-timer 4 nil list beta) (run-at-time #1=(26000 12 0 0) #1# vector gamma)))"
        ],
    )
}

fn timer_info_is_nonrecursive_and_covers_every_timer_field() -> ParityBatchCase {
    ParityBatchCase::value(
        "timer_info_is_nonrecursive_and_covers_every_timer_field",
        r##"
(let ((timer
       (timer-create)))
  (setf
   (timer--triggered timer)
   t)
  (setf
   (timer--high-seconds timer)
   12)
  (setf
   (timer--low-seconds timer)
   34)
  (setf
   (timer--usecs timer)
   56)
  (setf
   (timer--psecs timer)
   78)
  (setf
   (timer--repeat-delay timer)
   0.5)
  (setf
   (timer--function timer)
   'fixture-function)
  (setf
   (timer--idle-delay timer)
   nil)
  (when
      (and
       (>= emacs-major-version 28)
       (fboundp 'timer--integral-multiple))
    (setf
     (timer--integral-multiple timer)
     4))
  (list
   (async-job-queue--timer-info nil)
   (async-job-queue--timer-info timer)))
"##,
        expect![
            "OK (nil [timer (triggered t) (high-seconds 12) (low-seconds 34) (micro-seconds 56) (pico-seconds 78) (repeat-delay 0.5) #'fixture-function (idle-delay nil) (integral-multiple 4)])"
        ],
    )
}

fn ensure_running_creates_one_timer_for_work_and_keeps_it_while_inactive() -> ParityBatchCase {
    ParityBatchCase::value(
        "ensure_running_creates_one_timer_for_work_and_keeps_it_while_inactive",
        r##"
(let* ((table
        (async-job-queue-make-job-queue
         0.1 1 nil t nil nil 'ensure-work))
       (slot
        (async-job-queue--alloc-slot table))
       made cancelled)
  (setf
   (async-job-queue--slot-job slot)
   (async-job-queue--job-create
    :id 'work :table table
    :run-slot 0 :future 'future))
  (setf
   (async-job-queue--table-active table)
   t)
  (cl-letf (((symbol-function 'async-job-queue--make-timer)
             (lambda (&rest args)
               (push args made)
               'ensure-timer))
            ((symbol-function 'cancel-timer)
             (lambda (timer)
               (push timer cancelled))))
    (let ((first
           (async-job-queue--ensure-queue-running table))
          second inactive)
      (setq second
            (async-job-queue--ensure-queue-running table))
      (setf
       (async-job-queue--table-active table)
       nil)
      (setq inactive
            (async-job-queue--ensure-queue-running table))
      (list
       first second inactive
       (length made)
       (mapcar
        (lambda (args)
          (list
           (car args)
           (cadr args)
           (eq
            (nth 2 args)
            #'async-job-queue--process-queue)
           (eq
            (nth 3 args)
            table)))
        made)
       cancelled
       (async-job-queue-parity-table-state table)))))
"##,
        expect![
            "OK (ensure-timer ensure-timer ensure-timer 1 ((0.1 0.1 t t)) nil (:id ensure-work :active nil :in-use 1 :free 0 :used-slots (0) :free-slots nil :queued 0 :timer t))"
        ],
    )
}

fn ensure_running_calls_on_empty_before_cancelling_the_last_timer() -> ParityBatchCase {
    ParityBatchCase::value(
        "ensure_running_calls_on_empty_before_cancelling_the_last_timer",
        r##"
(let (events cancelled)
  (let ((table
         (async-job-queue-make-job-queue
          0.1 2
          (lambda (queue)
            (push
             (list
              'empty
              (async-job-queue--table-id queue)
              (async-job-queue--table-in-use queue)
              (queue-length
               (async-job-queue--table-queue queue))
              (and
               (async-job-queue--table-timer queue)
               t))
             events))
          nil nil nil 'ensure-empty)))
    (setf
     (async-job-queue--table-timer table)
     'last-timer)
    (cl-letf (((symbol-function 'cancel-timer)
               (lambda (timer)
                 (push
                  (list 'cancel timer)
                  events)
                 (push timer cancelled))))
      (let ((result
             (async-job-queue--ensure-queue-running table)))
        (list
         result
         (nreverse events)
         cancelled
         (async-job-queue-parity-table-state table))))))
"##,
        expect![
            "OK (nil ((empty ensure-empty 0 0 t) (cancel last-timer)) (last-timer) (:id ensure-empty :active t :in-use 0 :free 2 :used-slots nil :free-slots (0 1) :queued 0 :timer nil))"
        ],
    )
}

pub(super) fn timers_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        timer_factory_routes_numeric_and_absolute_frequencies_with_exact_repeat_policy(),
        timer_info_is_nonrecursive_and_covers_every_timer_field(),
        ensure_running_creates_one_timer_for_work_and_keeps_it_while_inactive(),
        ensure_running_calls_on_empty_before_cancelling_the_last_timer(),
    ]
}
