use expect_test::expect;

use super::ParityBatchCase;

fn atl_markup_mute_apply_forwards_values_and_binds_message_controls_only_inside_call()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_mute_apply_forwards_values_and_binds_message_controls_only_inside_call",
        r##"(let ((message-log-max
                77)
               (inhibit-message nil)
               observed)
          (list
           (atl-markup--mute-apply
            (lambda (&rest arguments)
              (setq observed
                    (list
                     arguments
                     inhibit-message
                     message-log-max))
              (list
               :result
               (apply #'+ arguments)))
            3
            5
            8)
           observed
           inhibit-message
           message-log-max))"##,
        expect!["OK ((:result 16) ((3 5 8) t nil) nil 77)"],
    )
}

fn atl_markup_mute_apply_suppresses_practical_message_log_output_and_restores_status()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_mute_apply_suppresses_practical_message_log_output_and_restores_status",
        r##"(let* ((messages-buffer
                (get-buffer-create
                 "*Messages*"))
               (message-log-max t)
               (secret
                "ATL-MARKUP-SECRET-MESSAGE")
               before
               after
               result)
          (with-current-buffer messages-buffer
            (let ((inhibit-read-only t))
              (erase-buffer)))
          (message
           "atl-markup-visible-sentinel")
          (setq before
                (current-message))
          (setq result
                (atl-markup--mute-apply
                 (lambda ()
                   (message "%s" secret)
                   :completed)))
          (setq after
                (current-message))
          (list
           result
           (equal before after)
           (with-current-buffer messages-buffer
             (goto-char
              (point-min))
             (and
              (search-forward secret nil t)
              t))
           (with-current-buffer messages-buffer
             (goto-char
              (point-min))
             (and
              (search-forward
               "atl-markup-visible-sentinel"
               nil
               t)
              t))))"##,
        expect!["OK (:completed t nil t)"],
    )
}

fn atl_markup_mute_apply_records_error_propagation_and_message_state_after_unwind()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_mute_apply_records_error_propagation_and_message_state_after_unwind",
        r##"(let ((message-log-max
                12)
               (inhibit-message
                nil)
               inside)
          (list
           (atl-markup-test-error-data
            (lambda ()
              (atl-markup--mute-apply
               (lambda ()
                 (setq inside
                       (list
                        message-log-max
                        inhibit-message))
                 (error
                  "mute failure %s"
                  42)))))
           inside
           message-log-max
           inhibit-message))"##,
        expect![[r#"OK ((:error error ("mute failure 42")) (nil t) 12 nil)"#]],
    )
}

fn atl_markup_mute_apply_supports_symbols_zero_arguments_and_non_local_exit() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_mute_apply_supports_symbols_zero_arguments_and_non_local_exit",
        r##"(let ((message-log-max
                9)
               (inhibit-message nil)
               inside-throw)
          (list
           (atl-markup--mute-apply
            #'list)
           (atl-markup--mute-apply
            #'concat
            "markup"
            "-"
            "value")
           (catch 'done
             (atl-markup--mute-apply
              (lambda (payload)
                (setq inside-throw
                      (list
                       payload
                       message-log-max
                       inhibit-message))
                (throw
                 'done
                 (vector
                  :escaped
                  payload)))
              17))
           inside-throw
           message-log-max
           inhibit-message))"##,
        expect![[r#"OK (nil "markup-value" [:escaped 17] (17 nil t) 9 nil)"#]],
    )
}

pub(super) fn utilities_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atl_markup_mute_apply_forwards_values_and_binds_message_controls_only_inside_call(),
        atl_markup_mute_apply_suppresses_practical_message_log_output_and_restores_status(),
        atl_markup_mute_apply_records_error_propagation_and_message_state_after_unwind(),
        atl_markup_mute_apply_supports_symbols_zero_arguments_and_non_local_exit(),
    ]
}
