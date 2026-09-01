use expect_test::expect;

use super::ParityBatchCase;

fn alert_termux_notify_builds_title_content_unicode_and_output_capture_arguments() -> ParityBatchCase
{
    ParityBatchCase::value(
        "alert_termux_notify_builds_title_content_unicode_and_output_capture_arguments",
        r##"(let ((alert-termux-command
                "/data/data/com.termux/files/usr/bin/termux-notification")
               calls)
         (cl-letf
             (((symbol-function 'alert-encode-string)
               (lambda (text)
                 (concat "<encoded>" text)))
              ((symbol-function 'call-process)
               (lambda (program infile destination display &rest args)
                 (push
                  (list program infile
                        (buffer-name (car destination))
                        (cadr destination)
                        display args)
                  calls)
                 0)))
           (unwind-protect
               (list
                (alert-termux-notify
                 '(:title "Deploy ✓"
                   :message "Finished 100% — 成功"))
                (nreverse calls))
             (when (get-buffer " *termux-notification output*")
               (kill-buffer " *termux-notification output*")))))"##,
        expect![[
            r#"OK (0 (("/data/data/com.termux/files/usr/bin/termux-notification" nil " *termux-notification output*" t nil ("-t" "<encoded>Deploy ✓" "-c" "<encoded>Finished 100% — 成功"))))"#
        ]],
    )
}

fn alert_termux_notify_without_title_emits_only_content_switch() -> ParityBatchCase {
    ParityBatchCase::value(
        "alert_termux_notify_without_title_emits_only_content_switch",
        r##"(let ((alert-termux-command "termux-notification")
                calls)
         (cl-letf
             (((symbol-function 'alert-encode-string)
               (lambda (text) (concat "[" text "]")))
              ((symbol-function 'call-process)
               (lambda (_program _infile _destination _display
                                &rest args)
                 (push args calls)
                 23)))
           (unwind-protect
               (list
                (alert-termux-notify
                 '(:title nil :message "Background task"))
                (nreverse calls))
             (when (get-buffer " *termux-notification output*")
               (kill-buffer " *termux-notification output*")))))"##,
        expect![[r#"OK (23 (("-c" "[Background task]")))"#]],
    )
}

fn alert_termux_missing_command_delegates_complete_info_to_message_backend() -> ParityBatchCase {
    ParityBatchCase::value(
        "alert_termux_missing_command_delegates_complete_info_to_message_backend",
        r##"(let ((alert-termux-command nil)
                received)
         (cl-letf
             (((symbol-function 'alert-message-notify)
               (lambda (info)
                 (setq received info)
                 'fallback))
              ((symbol-function 'call-process)
               (lambda (&rest _)
                 (error "must not launch"))))
           (let ((info
                  '(:title "No API"
                    :message "Show in minibuffer"
                    :severity moderate
                    :data (:source timer))))
             (list
              (alert-termux-notify info)
              (eq received info)
              received))))"##,
        expect![[
            r#"OK (fallback t (:title "No API" :message "Show in minibuffer" :severity moderate :data (:source timer)))"#
        ]],
    )
}

fn alert_termux_reuses_hidden_output_buffer_across_multiple_notifications() -> ParityBatchCase {
    ParityBatchCase::value(
        "alert_termux_reuses_hidden_output_buffer_across_multiple_notifications",
        r##"(let ((alert-termux-command "termux-notification")
                buffers)
         (cl-letf
             (((symbol-function 'alert-encode-string) #'identity)
              ((symbol-function 'call-process)
               (lambda (_program _infile destination _display
                                &rest _)
                 (push (car destination) buffers)
                 0)))
           (unwind-protect
               (progn
                 (alert-termux-notify
                  '(:title "One" :message "First"))
                 (alert-termux-notify
                  '(:title "Two" :message "Second"))
                 (list
                  (length buffers)
                  (eq (car buffers) (cadr buffers))
                  (buffer-name (car buffers))
                  (buffer-live-p (car buffers))))
             (when (get-buffer " *termux-notification output*")
               (kill-buffer " *termux-notification output*")))))"##,
        expect![[r#"OK (2 t " *termux-notification output*" t)"#]],
    )
}

fn alert_termux_runtime_command_override_is_used_for_each_delivery() -> ParityBatchCase {
    ParityBatchCase::value(
        "alert_termux_runtime_command_override_is_used_for_each_delivery",
        r##"(let (programs)
         (cl-letf
             (((symbol-function 'alert-encode-string) #'identity)
              ((symbol-function 'call-process)
               (lambda (program &rest _)
                 (push program programs)
                 0)))
           (unwind-protect
               (progn
                 (let ((alert-termux-command "/custom/first"))
                   (alert-termux-notify
                    '(:message "one")))
                 (let ((alert-termux-command "/custom/second"))
                   (alert-termux-notify
                    '(:message "two")))
                 (nreverse programs))
             (when (get-buffer " *termux-notification output*")
               (kill-buffer " *termux-notification output*")))))"##,
        expect![[r#"OK ("/custom/first" "/custom/second")"#]],
    )
}

fn alert_termux_end_to_end_explicit_style_dispatches_and_tracks_origin() -> ParityBatchCase {
    ParityBatchCase::value(
        "alert_termux_end_to_end_explicit_style_dispatches_and_tracks_origin",
        r##"(let ((alert-termux-command "termux-notification")
                (alert-active-alerts nil)
                (alert-log-messages nil)
                calls)
         (cl-letf
             (((symbol-function 'alert-encode-string) #'identity)
              ((symbol-function 'call-process)
               (lambda (program _infile _destination _display
                                &rest args)
                 (push (list program args) calls)
                 0)))
           (unwind-protect
               (with-temp-buffer
                 (rename-buffer "termux-alert-origin" t)
                 (let ((origin (current-buffer)))
                   (list
                    (alert "Battery low"
                           :title "Termux"
                           :style 'termux
                           :severity 'urgent)
                    (nreverse calls)
                    (mapcar
                     (lambda (entry)
                       (list
                        (eq (nth 0 entry) origin)
                        (plist-get (nth 1 entry) :message)
                        (functionp (nth 2 entry))))
                     alert-active-alerts)
                    (memq #'alert-remove-on-command
                          post-command-hook))))
             (when (get-buffer " *termux-notification output*")
               (kill-buffer " *termux-notification output*")))))"##,
        expect![[
            r#"OK (nil (("termux-notification" ("-t" "Termux" "-c" "Battery low"))) ((t "Battery low" nil)) (alert-remove-on-command t))"#
        ]],
    )
}

fn alert_termux_encode_boundary_receives_title_before_message_exactly_once() -> ParityBatchCase {
    ParityBatchCase::value(
        "alert_termux_encode_boundary_receives_title_before_message_exactly_once",
        r##"(let ((alert-termux-command "termux-notification")
                encoded)
         (cl-letf
             (((symbol-function 'alert-encode-string)
               (lambda (value)
                 (push value encoded)
                 (upcase value)))
              ((symbol-function 'call-process)
               (lambda (&rest _) 0)))
           (unwind-protect
               (progn
                 (alert-termux-notify
                  '(:title "title" :message "message"))
                 (nreverse encoded))
             (when (get-buffer " *termux-notification output*")
               (kill-buffer " *termux-notification output*")))))"##,
        expect![[r#"OK ("title" "message")"#]],
    )
}

fn alert_termux_process_exit_status_is_returned_to_caller() -> ParityBatchCase {
    ParityBatchCase::value(
        "alert_termux_process_exit_status_is_returned_to_caller",
        r##"(let ((alert-termux-command "termux-notification"))
         (cl-letf
             (((symbol-function 'alert-encode-string) #'identity)
              ((symbol-function 'call-process)
               (lambda (&rest _) 17)))
           (unwind-protect
               (alert-termux-notify
                '(:title "Failure" :message "API unavailable"))
             (when (get-buffer " *termux-notification output*")
               (kill-buffer " *termux-notification output*")))))"##,
        expect!["OK 17"],
    )
}

pub(super) fn workflow_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        alert_termux_notify_builds_title_content_unicode_and_output_capture_arguments(),
        alert_termux_notify_without_title_emits_only_content_switch(),
        alert_termux_missing_command_delegates_complete_info_to_message_backend(),
        alert_termux_reuses_hidden_output_buffer_across_multiple_notifications(),
        alert_termux_runtime_command_override_is_used_for_each_delivery(),
        alert_termux_end_to_end_explicit_style_dispatches_and_tracks_origin(),
        alert_termux_encode_boundary_receives_title_before_message_exactly_once(),
        alert_termux_process_exit_status_is_returned_to_caller(),
    ]
}
