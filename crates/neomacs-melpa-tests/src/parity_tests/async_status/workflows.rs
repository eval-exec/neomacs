use super::ParityBatchCase;
use expect_test::expect;

fn complete_single_job_lifecycle_allocates_displays_updates_and_cleans_up() -> ParityBatchCase {
    ParityBatchCase::value(
        "complete_single_job_lifecycle_allocates_displays_updates_and_cleans_up",
        r##"(let ((id (async-status-req-id "compile"))
      (watch-sequence 0)
      events)
  (setq async-status--shown-items nil)
  (cl-letf (((symbol-function 'file-notify-add-watch)
             (lambda (path flags callback)
               (setq watch-sequence (1+ watch-sequence))
               (push
                (list :watch-add
                      (file-name-base path)
                      flags callback)
                events)
               watch-sequence))
            ((symbol-function 'file-notify-rm-watch)
             (lambda (watch)
               (push (list :watch-remove watch) events)))
            ((symbol-function 'async-status-show)
             (lambda ()
               (push :show events)))
            ((symbol-function 'async-status-hide)
             (lambda (&optional force)
               (push (list :hide force) events)))
            ((symbol-function 'async-status--refresh-status-bar)
             (lambda ()
               (push :refresh events))))
    (unwind-protect
        (progn
          (async-status-add-item-to-bar id "Compile project")
          (async-status-show)
          (async-status-set-msg-val id 0.5)
          (async-status--update-items
           (list 1 'changed
                 (async-status--get-absolute-path-by-id id)))
          (let ((during
                 (list
                  (length async-status--shown-items)
                  (async-status--item-label
                   (car async-status--shown-items))
                  (async-status--item-progress
                   (car async-status--shown-items))
                  (async-status--get-msg-val id))))
            (async-status-remove-item-from-bar id)
            (async-status-clean-up id)
            (async-status-hide)
            (list
             during
             (length async-status--shown-items)
             (file-exists-p
              (async-status--get-absolute-path-by-id id))
             (mapcar
              (lambda (event)
                (if
                    (and
                     (listp event)
                     (eq (car event) :watch-add))
                    (list :watch-add :normalized
                          (nth 2 event)
                          (nth 3 event))
                  event))
              (nreverse events)))))
      (when
          (file-exists-p
           (async-status--get-absolute-path-by-id id))
        (async-status-clean-up id))
      (setq async-status--shown-items nil))))"##,
        expect![[
            r#"OK ((1 "Compile project" "0.5" "0.5") 0 nil ((:watch-add :normalized (change) async-status--update-items) :show :refresh :show (:watch-remove 1) :refresh (:hide nil)))"#
        ]],
    )
}

fn multiple_jobs_support_interleaved_progress_and_independent_completion() -> ParityBatchCase {
    ParityBatchCase::value(
        "multiple_jobs_support_interleaved_progress_and_independent_completion",
        r##"(let ((first (async-status-req-id "compile"))
      (second (async-status-req-id "tests"))
      (watch-id 0)
      removed)
  (setq async-status--shown-items nil)
  (unwind-protect
      (cl-letf (((symbol-function 'file-notify-add-watch)
                 (lambda (&rest _arguments)
                   (setq watch-id (1+ watch-id))))
                ((symbol-function 'file-notify-rm-watch)
                 (lambda (watch)
                   (push watch removed)))
                ((symbol-function 'async-status--refresh-status-bar)
                 #'ignore)
                ((symbol-function 'async-status-show)
                 #'ignore))
        (async-status-add-item-to-bar first "Compile")
        (async-status-add-item-to-bar second "Tests")
        (async-status-set-msg-val first 0.2)
        (async-status--update-items
         (list 1 'changed
               (async-status--get-absolute-path-by-id first)))
        (async-status-set-msg-val second 0.8)
        (async-status--update-items
         (list 2 'changed
               (async-status--get-absolute-path-by-id second)))
        (let ((before
               (mapcar
                (lambda (item)
                  (list
                   (async-status--item-label item)
                   (async-status--item-progress item)))
                async-status--shown-items)))
          (async-status-remove-item-from-bar second)
          (async-status-set-msg-val first 1.0)
          (async-status--update-items
           (list 1 'changed
                 (async-status--get-absolute-path-by-id first)))
          (list
           before
           (mapcar
            (lambda (item)
              (list
               (async-status--item-label item)
               (async-status--item-progress item)))
            async-status--shown-items)
           removed)))
    (setq async-status--shown-items nil)
    (async-status-clean-up first)
    (async-status-clean-up second)))"##,
        expect![[r#"OK ((("Tests" "0.8") ("Compile" "0.2")) (("Compile" "1.0")) (2))"#]],
    )
}

fn thresholded_child_updates_publish_only_meaningful_progress_steps() -> ParityBatchCase {
    ParityBatchCase::value(
        "thresholded_child_updates_publish_only_meaningful_progress_steps",
        r##"(let ((id (async-status-req-id "stream")))
  (unwind-protect
      (let (trace)
        (dolist (value '(0.0 0.005 0.011 0.015 0.021 0.022 0.5 0.999 1.0))
          (let ((before (async-status--get-msg-val id)))
            (async-status-safely-set-msg-val id value)
            (let ((after (async-status--get-msg-val id)))
              (push
               (list value
                     (not (equal before after))
                     after)
               trace))))
        (nreverse trace))
    (async-status-clean-up id)))"##,
        expect![[
            r#"OK ((0.0 nil "0") (0.005 nil "0") (0.011 t "0.011") (0.015 nil "0.011") (0.021 t "0.021") (0.022 nil "0.021") (0.5 t "0.5") (0.999 t "0.999") (1.0 nil "0.999"))"#
        ]],
    )
}

fn actual_subprocess_can_publish_progress_through_the_message_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "actual_subprocess_can_publish_progress_through_the_message_file",
        r##"(let ((id (async-status-req-id "subprocess"))
      process)
  (setq async-status--shown-items nil)
  (unwind-protect
      (cl-letf (((symbol-function 'file-notify-add-watch)
                 (lambda (&rest _arguments) :watch))
                ((symbol-function 'async-status--refresh-status-bar)
                 #'ignore)
                ((symbol-function 'async-status-show)
                 #'ignore))
        (async-status-add-item-to-bar id "Subprocess")
        (setq process
              (make-process
               :name "async-status-test-writer"
               :buffer nil
               :command
               (list
                "sh" "-c"
                "printf '0.75' > \"$1\""
                "async-status-test-writer"
                (async-status--get-absolute-path-by-id id))
               :noquery t))
        (while (process-live-p process)
          (accept-process-output process 0.05))
        (async-status--update-items
         (list :watch 'changed
               (async-status--get-absolute-path-by-id id)))
        (list
         (process-exit-status process)
         (async-status--get-msg-val id)
         (async-status--item-progress
          (car async-status--shown-items))))
    (when (and process (process-live-p process))
      (delete-process process))
    (setq async-status--shown-items nil)
    (async-status-clean-up id)))"##,
        expect!["OK (0 \"0.75\" \"0.75\")"],
    )
}

fn actual_async_child_api_and_file_notification_drive_parent_progress_end_to_end() -> ParityBatchCase
{
    ParityBatchCase::value(
        "actual_async_child_api_and_file_notification_drive_parent_progress_end_to_end",
        r##"(let* ((id (async-status-req-id "async-child"))
       (path (async-status--get-absolute-path-by-id id))
       (parent-load-path (copy-sequence load-path))
       (parent-temp-directory temporary-file-directory)
       (buffer (get-buffer-create "*async-status*"))
       item future child-result)
  (setq
   async-status--shown-items nil
   async-status-test-posframe-calls nil)
  (unwind-protect
      (cl-letf (((symbol-function 'insert-image)
                 (lambda (&rest _arguments)
                   (insert "[progress]"))))
        (require 'async)
        (async-status-add-item-to-bar id "Async child API")
        (setq item (car async-status--shown-items))
        (setq future
              (async-start
               `(lambda ()
                  (setq
                   load-path ',parent-load-path
                   temporary-file-directory
                   ,parent-temp-directory)
                  (require 'async-status)
                  (async-status-safely-set-msg-val
                   ,id 0.75 0.0)
                  (list
                   :child
                   (async-status--get-msg-val ,id)))
               (lambda (value)
                 (setq child-result value))))
        (let ((deadline (+ (float-time) 10)))
          (while
              (and
               (not
                (and
                 child-result
                 (equal
                  (async-status--item-progress item)
                  "0.75")))
               (< (float-time) deadline))
            (accept-process-output nil 0.02)
            (sit-for 0.01)
            (read-event nil nil 0.01)))
        (list
         child-result
         (async-status--item-progress item)
         (async-status--get-msg-val id)
         (and
          async-status-test-posframe-calls
          t)
         (process-live-p future)
         (process-status future)
         (with-current-buffer buffer
           (and
            (string-match-p
             "Async child API"
             (buffer-string))
            (string-match-p
             "\\[progress\\]"
             (buffer-string))
            t))))
    (when (and future (process-live-p future))
      (delete-process future))
    (when
        (async-status--find-item-by-msgid id)
      (async-status-remove-item-from-bar id))
    (when (file-exists-p path)
      (async-status-clean-up id))
    (setq async-status--shown-items nil)
    (when (buffer-live-p buffer)
      (kill-buffer buffer))))"##,
        expect!["OK ((:child \"0.75\") \"0.75\" \"0.75\" t nil exit t)"],
    )
}

fn duplicate_registrations_are_removed_one_watch_at_a_time_by_id() -> ParityBatchCase {
    ParityBatchCase::value(
        "duplicate_registrations_are_removed_one_watch_at_a_time_by_id",
        r##"(let ((id (async-status-req-id "duplicate"))
      (next-watch 0)
      removed)
  (setq async-status--shown-items nil)
  (unwind-protect
      (cl-letf (((symbol-function 'file-notify-add-watch)
                 (lambda (&rest _arguments)
                   (setq next-watch (1+ next-watch))))
                ((symbol-function 'file-notify-rm-watch)
                 (lambda (watch)
                   (push watch removed)))
                ((symbol-function 'async-status--refresh-status-bar)
                 #'ignore))
        (async-status-add-item-to-bar id "First")
        (async-status-add-item-to-bar id "Second")
        (let ((initial
               (mapcar
                (lambda (item)
                  (list
                   (async-status--item-label item)
                   (async-status--item-fs-watcher-id item)))
                async-status--shown-items)))
          (async-status-remove-item-from-bar id)
          (let ((after-one
                 (mapcar
                  (lambda (item)
                    (list
                     (async-status--item-label item)
                     (async-status--item-fs-watcher-id item)))
                  async-status--shown-items)))
            (async-status-remove-item-from-bar id)
            (list
             initial
             after-one
             async-status--shown-items
             (nreverse removed)))))
    (setq async-status--shown-items nil)
    (async-status-clean-up id)))"##,
        expect![[r#"OK ((("Second" 2) ("First" 1)) (("First" 1)) nil (2 1))"#]],
    )
}

fn cleanup_order_allows_removing_ui_state_after_the_message_file_is_deleted() -> ParityBatchCase {
    ParityBatchCase::value(
        "cleanup_order_allows_removing_ui_state_after_the_message_file_is_deleted",
        r##"(let ((id (async-status-req-id "cleanup-order"))
      calls)
  (setq async-status--shown-items nil)
  (cl-letf (((symbol-function 'file-notify-add-watch)
             (lambda (&rest _arguments) :watch))
            ((symbol-function 'file-notify-rm-watch)
             (lambda (watch)
               (push (list :removed watch) calls)))
            ((symbol-function 'async-status--refresh-status-bar)
             (lambda ()
               (push :refresh calls))))
    (async-status-add-item-to-bar id "Cleanup")
    (async-status-clean-up id)
    (let ((file-after-cleanup
           (file-exists-p
            (async-status--get-absolute-path-by-id id))))
      (async-status-remove-item-from-bar id)
      (list
       file-after-cleanup
       async-status--shown-items
       (nreverse calls)))))"##,
        expect!["OK (nil nil ((:removed :watch) :refresh))"],
    )
}

fn workflow_cleanup_can_restore_all_resources_after_a_midstream_error() -> ParityBatchCase {
    ParityBatchCase::value(
        "workflow_cleanup_can_restore_all_resources_after_a_midstream_error",
        r##"(let ((id (async-status-req-id "failure"))
      removed
      outcome)
  (setq async-status--shown-items nil)
  (cl-letf (((symbol-function 'file-notify-add-watch)
             (lambda (&rest _arguments) :watch))
            ((symbol-function 'file-notify-rm-watch)
             (lambda (watch)
               (push watch removed)))
            ((symbol-function 'async-status--refresh-status-bar)
             #'ignore))
    (setq outcome
          (async-status-test-error
           (lambda ()
             (unwind-protect
                 (progn
                   (async-status-add-item-to-bar id "Failing job")
                   (async-status-set-msg-val id 0.4)
                   (error "worker failed"))
               (async-status-remove-item-from-bar id)
               (async-status-clean-up id)))))
    (list
     (car outcome)
     (cadr outcome)
     (and
      (string-match-p "worker failed" (format "%S" outcome))
      t)
     async-status--shown-items
     (file-exists-p
      (async-status--get-absolute-path-by-id id))
     removed)))"##,
        expect!["OK (:error error t nil nil (:watch))"],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        complete_single_job_lifecycle_allocates_displays_updates_and_cleans_up(),
        multiple_jobs_support_interleaved_progress_and_independent_completion(),
        thresholded_child_updates_publish_only_meaningful_progress_steps(),
        actual_subprocess_can_publish_progress_through_the_message_file(),
        actual_async_child_api_and_file_notification_drive_parent_progress_end_to_end(),
        duplicate_registrations_are_removed_one_watch_at_a_time_by_id(),
        cleanup_order_allows_removing_ui_state_after_the_message_file_is_deleted(),
        workflow_cleanup_can_restore_all_resources_after_a_midstream_error(),
    ]
}
