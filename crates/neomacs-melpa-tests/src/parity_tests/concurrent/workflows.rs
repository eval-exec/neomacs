use expect_test::expect;

use super::ParityBatchCase;

fn generator_resumes_a_fibonacci_workflow_one_yield_at_a_time() -> ParityBatchCase {
    ParityBatchCase::value(
        "generator_resumes_a_fibonacci_workflow_one_yield_at_a_time",
        r####"
(progn
  (deferred:clear-queue)
  (let ((values nil)
        (a 0)
        (b 1)
        generator)
    (setq generator
          (cc:generator
           (lambda (value) (push value values))
           (yield a)
           (yield b)
           (while (< (length values) 7)
             (let ((next (+ a b)))
               (setq a b b next)
               (yield next)))))
    (dotimes (_ 7) (funcall generator) (deferred:flush-queue!))
    (list :values (nreverse values)
          :callable (functionp generator))))
"####,
        expect!["OK (:values (0 1 1 2 3 5 8) :callable t)"],
    )
}

fn pseudo_thread_runs_statements_and_loop_iterations_in_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "pseudo_thread_runs_statements_and_loop_iterations_in_order",
        r####"
(progn
  (deferred:clear-queue)
  (let ((count 0) events)
    (neomacs-concurrent-test-sync
     (cc:thread
      1
      (push 'start events)
      (while (< count 4)
        (setq count (1+ count))
        (push (list 'iteration count) events))
      (push 'finish events)))
    (list :count count :events (nreverse events))))
"####,
        expect!["OK (:count 0 :events (start))"],
    )
}

fn semaphore_enforces_fifo_permits_release_all_and_over_release_errors() -> ParityBatchCase {
    ParityBatchCase::value(
        "semaphore_enforces_fifo_permits_release_all_and_over_release_errors",
        r####"
(progn
  (deferred:clear-queue)
  (let* ((semaphore (cc:semaphore-create 1))
         (first (cc:semaphore-acquire semaphore))
         (second (cc:semaphore-acquire semaphore))
         (third (cc:semaphore-acquire semaphore))
         events)
    (deferred:nextc first (lambda (_) (push 'first events)))
    (deferred:nextc second (lambda (_) (push 'second events)))
    (deferred:nextc third (lambda (_) (push 'third events)))
    (deferred:flush-queue!)
    (let ((initial (list :permits (cc:semaphore-permits semaphore)
                         :waiting (length (cc:semaphore-waiting-deferreds semaphore))
                         :events (reverse events))))
      (cc:semaphore-release semaphore)
      (deferred:flush-queue!)
      (let ((released (list :permits (cc:semaphore-permits semaphore)
                            :waiting (length (cc:semaphore-waiting-deferreds semaphore))
                            :events (reverse events)))
            (canceled (cc:semaphore-release-all semaphore)))
        (list :initial initial :released released
              :canceled-count (length canceled)
              :reset (list (cc:semaphore-permits semaphore)
                           (cc:semaphore-waiting-deferreds semaphore))
              :over-release
              (neomacs-concurrent-test-error
               (lambda () (cc:semaphore-release semaphore))))))))
"####,
        expect![[
            r#"OK (:initial (:permits 0 :waiting 2 :events (first)) :released (:permits 0 :waiting 1 :events (first second)) :canceled-count 1 :reset (1 nil) :over-release (:signal error :message "Too many calling semaphore-release. [max:1 <= permits:1]"))"#
        ]],
    )
}

fn dataflow_unblocks_waiters_supports_parent_values_and_reports_duplicate_sets() -> ParityBatchCase
{
    ParityBatchCase::value(
        "dataflow_unblocks_waiters_supports_parent_values_and_reports_duplicate_sets",
        r####"
(progn
  (deferred:clear-queue)
  (let* ((parent (cc:dataflow-environment))
         (child (cc:dataflow-environment parent))
         results events)
    (dolist (event '(get-first get-waiting set get clear clear-all))
      (cc:dataflow-connect child event
                           (lambda (payload) (push payload events))))
    (deferred:nextc (cc:dataflow-get child '("artifact" 42))
                    (lambda (value) (push (list 'first value) results)))
    (deferred:nextc (cc:dataflow-get child '("artifact" 42))
                    (lambda (value) (push (list 'second value) results)))
    (deferred:flush-queue!)
    (let ((waiting (cc:dataflow-get-waiting-keys child)))
      (cc:dataflow-set child '("artifact" 42) 'ready)
      (cc:dataflow-set parent "fallback" 99)
      (deferred:flush-queue!)
      (let ((available (cc:dataflow-get-avalable-pairs child))
            (duplicate
             (neomacs-concurrent-test-error
              (lambda () (cc:dataflow-set child '("artifact" 42) 'again)))))
        (cc:dataflow-clear child '("artifact" 42))
        (deferred:flush-queue!)
        (list :waiting waiting
              :results (reverse results)
              :sync (list (cc:dataflow-get-sync child '("artifact" 42))
                          (cc:dataflow-get-sync child "fallback"))
              :available available
              :duplicate duplicate
              :events (reverse events))))))
"####,
        expect![[
            r#"OK (:waiting (#1=("artifact" 42)) :results ((second ready) (first ready)) :sync (nil 99) :available ((#1# . ready) ("fallback" . 99)) :duplicate (:signal error :message "Can not set a dataflow value. The key [(artifact 42)] has already had a value. NEW:[again] OLD:[ready]") :events ((get-first (#1#)) (get-waiting (#1#)) (get-waiting (("artifact" 42))) (set (("artifact" 42))) (set ("fallback")) (clear (("artifact" 42)))))"#
        ]],
    )
}

fn signal_channels_route_local_parent_global_and_disconnected_observers() -> ParityBatchCase {
    ParityBatchCase::value(
        "signal_channels_route_local_parent_global_and_disconnected_observers",
        r####"
(progn
  (deferred:clear-queue)
  (let* ((parent (cc:signal-channel "parent"))
         (child (cc:signal-channel "child" parent))
         events
         (local (cc:signal-connect child 'deploy
                                   (lambda (event) (push (cons 'local event) events))))
         (all (cc:signal-connect child t
                                 (lambda (event) (push (cons 'all event) events))))
         (upstream (cc:signal-connect parent t
                                     (lambda (event) (push (cons 'upstream event) events)))))
    (cc:signal-send child 'deploy "blue" 42)
    (cc:signal-send parent 'parent-only "notice")
    (cc:signal-send-global child 'global "broadcast")
    (deferred:flush-queue!)
    (let ((before (reverse events))
          (removed (cc:signal-disconnect child local)))
      (setq events nil)
      (cc:signal-send child 'deploy "green")
      (deferred:flush-queue!)
      (cc:signal-disconnect-all child)
      (list :before before :removed-count (length removed)
            :after (reverse events)
            :child-observers (cc:signal-observers child)
            :parent-observers (length (cc:signal-observers parent))
            :objects (mapcar #'deferred-p (list local all upstream))))))
"####,
        expect![[
            r#"OK (:before ((all . #1=(deploy ("blue" 42))) (local . #1#) (upstream parent-only ("notice")) (upstream global ("broadcast")) (all parent-only ("notice")) (all global ("broadcast"))) :removed-count 1 :after ((all deploy ("green"))) :child-observers nil :parent-observers 2 :objects (t t t))"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        generator_resumes_a_fibonacci_workflow_one_yield_at_a_time(),
        pseudo_thread_runs_statements_and_loop_iterations_in_order(),
        semaphore_enforces_fifo_permits_release_all_and_over_release_errors(),
        dataflow_unblocks_waiters_supports_parent_values_and_reports_duplicate_sets(),
        signal_channels_route_local_parent_global_and_disconnected_observers(),
    ]
}
