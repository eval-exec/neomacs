use expect_test::expect;

use super::ParityBatchCase;

fn async_functions_await_each_other_and_the_caller_gets_the_final_value() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_functions_await_each_other_and_the_caller_gets_the_final_value",
        r##"
(progn
  ;; Two async functions, one awaiting the other, exactly as the README
  ;; composes them.
  (aio-defun aio-test-double (n)
    (aio-await (aio-sleep 0.01))
    (* n 2))
  (aio-defun aio-test-total (n)
    (let ((first (aio-await (aio-test-double n)))
          (second (aio-await (aio-test-double 5))))
      (+ first second)))
  (let ((pending (aio-test-total 3)))
    (list
     ;; Calling an async function returns a promise immediately; the value
     ;; only exists once it resolves.
     :returns-a-promise (aio-promise-p pending)
     :unresolved-at-first (null (aio-result pending))
     :value (aio-wait-for pending)
     :resolved-afterwards (and (aio-result pending) t)
     ;; The stored result is a *function* that yields the value, which is
     ;; how a promise can carry a signal as easily as a value.
     :result-is-a-function (functionp (aio-result pending))
     :calling-it-again (funcall (aio-result pending))
     ;; Awaiting something that is not a promise yields it unchanged.
     :awaiting-a-plain-value
     (aio-wait-for (funcall (aio-lambda () (aio-await 42))))
     ;; A whole list of promises can be awaited in order.
     :awaiting-many
     (aio-wait-for
      (funcall (aio-lambda ()
                 (let ((results nil))
                   (dolist (promise (list (aio-sleep 0.01 'a)
                                          (aio-sleep 0 'b)
                                          (aio-sleep 0.02 'c)))
                     (push (aio-await promise) results))
                   (nreverse results))))))))
"##,
        expect![
            "OK (:returns-a-promise t :unresolved-at-first t :value 16 :resolved-afterwards t :result-is-a-function t :calling-it-again 16 :awaiting-a-plain-value 42 :awaiting-many (a b c))"
        ],
    )
}

fn an_error_inside_an_async_function_reaches_whoever_awaits_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "an_error_inside_an_async_function_reaches_whoever_awaits_it",
        r##"
(progn
  (aio-defun aio-test-boom ()
    (aio-await (aio-sleep 0))
    (error "kaboom %d" 7))
  (aio-defun aio-test-fine ()
    (aio-await (aio-sleep 0))
    'ok)
  ;; An async function that awaits a failing one inherits the failure
  ;; unless it catches it.
  (aio-defun aio-test-caller ()
    (aio-await (aio-test-boom)))
  (aio-defun aio-test-guarded ()
    (let ((outcome (aio-await (aio-catch (aio-test-boom)))))
      (list :tag (car outcome) :data (aio-test-plain (cdr outcome)))))
  (list
   ;; The signal is re-raised in the awaiting thread, with its data intact.
   :signalled (condition-case error (aio-wait-for (aio-test-boom))
                (error (aio-test-plain error)))
   :through-a-caller (condition-case error (aio-wait-for (aio-test-caller))
                       (error (aio-test-plain error)))
   ;; `aio-catch' turns either outcome into a value that never signals.
   :caught-failure (aio-test-plain (aio-wait-for (aio-catch (aio-test-boom))))
   :caught-success (aio-test-plain (aio-wait-for (aio-catch (aio-test-fine))))
   :handled-inside (aio-test-plain (aio-wait-for (aio-test-guarded)))
   ;; The promise keeps the signal, so asking twice signals twice rather
   ;; than yielding nil the second time.
   :signals-every-time
   (let ((promise (aio-test-boom)))
     (list (condition-case error (aio-wait-for promise)
             (error (aio-test-plain error)))
           (condition-case error (funcall (aio-result promise))
             (error (aio-test-plain error)))))))
"##,
        expect![[
            r#"OK (:signalled (error "kaboom 7") :through-a-caller (error "kaboom 7") :caught-failure (:error error "kaboom 7") :caught-success (:success . ok) :handled-inside (:tag :error :data (error "kaboom 7")) :signals-every-time ((error "kaboom 7") (error "kaboom 7")))"#
        ]],
    )
}

fn racing_promises_against_each_other_and_against_a_timeout() -> ParityBatchCase {
    ParityBatchCase::value(
        "racing_promises_against_each_other_and_against_a_timeout",
        r##"
(progn
  (aio-defun aio-test-race ()
    (let* ((slow (aio-sleep 0.4 'slow))
           (fast (aio-sleep 0.01 'fast))
           (select (aio-make-select (list slow fast))))
      ;; `aio-select' resolves to whichever member promise settled first;
      ;; awaiting that promise gives its value.
      (let ((winner (aio-await (aio-await (aio-select select))))
            (runner-up (aio-await (aio-await (aio-select select)))))
        (list winner runner-up))))
  (aio-defun aio-test-against-timeout (delay timeout)
    (let* ((work (aio-sleep delay 'finished))
           (limit (aio-timeout timeout))
           (select (aio-make-select (list work limit))))
      (aio-await (aio-catch (aio-await (aio-select select))))))
  (list
   ;; Both promises are eventually delivered, fastest first, regardless of
   ;; the order they were handed to the select.
   :both-in-finishing-order (aio-wait-for (aio-test-race))
   ;; Work that beats the clock wins; work that does not gets the timeout's
   ;; signal instead, carrying the number of seconds it waited.
   :beats-the-clock
   (aio-test-plain (aio-wait-for (aio-test-against-timeout 0.01 0.4)))
   :misses-the-clock
   (aio-test-plain (aio-wait-for (aio-test-against-timeout 0.4 0.02)))))
"##,
        expect![
            "OK (:both-in-finishing-order (fast slow) :beats-the-clock (:success . finished) :misses-the-clock (:error aio-timeout . 0.02))"
        ],
    )
}

fn a_real_subprocess_feeds_a_chain_of_promises_through_one_callback() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_real_subprocess_feeds_a_chain_of_promises_through_one_callback",
        r##"
(progn
  (aio-test-script "emit.sh"
                   "#!/bin/sh\nprintf 'alpha\\n'\nprintf 'beta\\n'\nprintf 'gamma\\n'\nexit 3\n")
  (aio-defun aio-test-run ()
    (let* ((output (aio-make-callback))
           (emit (car output))
           (chunks (cdr output))
           (finished (aio-make-callback :once t :tag 'exited))
           (done (car finished))
           (exit (cdr finished))
           (text ""))
      (make-process
       :name "aio-test-emit" :noquery t
       :command (list (aio-test-path "emit.sh"))
       :connection-type 'pipe
       :filter (lambda (_process chunk) (funcall emit chunk))
       :sentinel (lambda (process event)
                   (when (memq (process-status process) '(exit signal))
                     (funcall done (process-exit-status process)
                              (substring-no-properties event 0 -1)))))
      ;; Wait on the sentinel, which fires exactly once.  The filter may be
      ;; called any number of times -- that is the kernel's choice, not the
      ;; package's -- so its chunks are joined rather than counted.
      (let ((ended (aio-await exit)))
        (while (aio-result chunks)
          (setq text (concat text (car (aio-chain chunks)))))
        (list :text text :exit ended))))
  (aio-test-plain (aio-wait-for (aio-test-run))))
"##,
        expect![[
            r#"OK (:text "alpha\nbeta\ngamma\n" :exit (exited 3 "exited abnormally with code 3"))"#
        ]],
    )
}

fn a_promise_settles_once_and_cancel_never_reports_that_it_worked() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_promise_settles_once_and_cancel_never_reports_that_it_worked",
        r##"
(list
 ;; The first resolution wins and later ones are silently dropped, which is
 ;; what makes a promise safe to hand to several producers.
 :resolved-twice
 (let ((promise (aio-promise)))
   (aio-resolve promise (lambda () 'first))
   (aio-resolve promise (lambda () 'second))
   (aio-wait-for promise))
 ;; Cancelling an unresolved promise makes every awaiter receive the
 ;; `aio-cancel' signal, carrying the reason it was given.
 :cancelled
 (let ((promise (aio-promise)))
   (list :return-value (aio-cancel promise 'because)
         :awaiting (condition-case error (aio-wait-for promise)
                     (error (aio-test-plain error)))))
 ;; Cancelling one that has already settled must not disturb it.
 :cancel-after-settling
 (let ((promise (aio-sleep 0 'already)))
   (aio-wait-for promise)
   (list :return-value (aio-cancel promise 'too-late)
         :value (aio-wait-for promise)))
 ;; `aio-cancel' documents itself as "returning non-nil if successful", but
 ;; it returns the value of `aio-resolve', which is always nil.  So the
 ;; return value is nil whether the cancel took effect or not, and a caller
 ;; following the docstring cannot tell the two apart.
 :cancel-return-values-are-indistinguishable
 (let ((fresh (aio-promise))
       (settled (aio-sleep 0 'done)))
   (aio-wait-for settled)
   (list :on-a-fresh-promise (aio-cancel fresh)
         :on-a-settled-promise (aio-cancel settled)
         :but-the-fresh-one-did-cancel
         (condition-case error (aio-wait-for fresh)
           (error (car (aio-test-plain error))))
         :and-the-settled-one-kept-its-value (aio-wait-for settled)))
 ;; A listener added after resolution still runs, on a later turn.
 :late-listener
 (let ((promise (aio-sleep 0 'value))
       (seen nil))
   (aio-wait-for promise)
   (aio-listen promise (lambda (value) (setq seen (funcall value))))
   (aio-wait-for (aio-sleep 0.05))
   seen))
"##,
        expect![
            "OK (:resolved-twice first :cancelled (:return-value nil :awaiting (aio-cancel . because)) :cancel-after-settling (:return-value nil :value already) :cancel-return-values-are-indistinguishable (:on-a-fresh-promise nil :on-a-settled-promise nil :but-the-fresh-one-did-cancel aio-cancel :and-the-settled-one-kept-its-value done) :late-listener value)"
        ],
    )
}

fn aio_with_async_forces_its_result_and_drops_the_bindings_around_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "aio_with_async_forces_its_result_and_drops_the_bindings_around_it",
        r##"
(progn
  (defvar aio-test-dynamic 'global)
  (defun aio-test-inside-a-binding ()
    (let ((aio-test-dynamic 'inner))
      (aio-with-async aio-test-dynamic)))
  (list
   ;; The block runs asynchronously and yields a promise for its value.
   :value (aio-wait-for (aio-with-async (+ 1 2)))
   ;; `aio-await' is available inside it.
   :awaiting (aio-wait-for (aio-with-async (aio-await (aio-sleep 0.01 'slept))))
   ;; The documented surprise, quoted in the macro's own docstring: the body
   ;; runs after the `let' has unwound, so a dynamic binding lexically
   ;; around the block has no effect and the outer one is seen instead.
   :dynamic-binding-does-not-reach-it
   (let ((aio-test-dynamic 'outer))
     (aio-wait-for (aio-test-inside-a-binding)))
   :without-any-binding (aio-wait-for (aio-test-inside-a-binding))
   ;; It listens to its own promise so that an error nobody awaits is still
   ;; realised rather than being swallowed.
   :error-is-realised
   (condition-case error
       (aio-wait-for (aio-with-async (error "unattended %s" 'failure)))
     (error (aio-test-plain error)))))
"##,
        expect![[
            r#"OK (:value 3 :awaiting slept :dynamic-binding-does-not-reach-it outer :without-any-binding global :error-is-realised (error "unattended failure"))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        async_functions_await_each_other_and_the_caller_gets_the_final_value(),
        an_error_inside_an_async_function_reaches_whoever_awaits_it(),
        racing_promises_against_each_other_and_against_a_timeout(),
        a_real_subprocess_feeds_a_chain_of_promises_through_one_callback(),
        a_promise_settles_once_and_cancel_never_reports_that_it_worked(),
        aio_with_async_forces_its_result_and_drops_the_bindings_around_it(),
    ]
}
