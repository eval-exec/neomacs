use expect_test::expect;

use super::ParityBatchCase;

fn atl_long_lines_toggle_is_a_true_noop_when_minor_mode_is_disabled() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_toggle_is_a_true_noop_when_minor_mode_is_disabled",
        r##"(with-temp-buffer
         (let ((atl-long-lines-mode nil)
               (measurements 0)
               (toggles nil))
           (cl-letf
               (((symbol-function
                  'atl-long-lines--end-line-column)
                 (lambda ()
                   (setq
                    measurements
                    (1+ measurements))
                   100))
                ((symbol-function
                  'toggle-truncate-lines)
                 (lambda (&optional value)
                   (push value toggles))))
             (list
              (atl-long-lines-do-toggle)
              measurements
              toggles
              truncate-lines))))"##,
        expect!["OK (nil 0 nil nil)"],
    )
}

fn atl_long_lines_toggle_chooses_truncation_at_or_below_width_and_wrapping_above_width()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_toggle_chooses_truncation_at_or_below_width_and_wrapping_above_width",
        r##"(mapcar
         (lambda (case)
           (pcase-let
               ((`(,width ,column)
                 case))
             (with-temp-buffer
               (let ((atl-long-lines-mode t)
                     calls)
                 (cl-letf
                     (((symbol-function
                        'window-width)
                       (lambda (&optional _window)
                         width))
                      ((symbol-function
                        'atl-long-lines--end-line-column)
                       (lambda ()
                         column))
                      ((symbol-function
                        'toggle-truncate-lines)
                       (lambda (&optional value)
                         (push value calls)
                         :toggled)))
                   (list
                    width
                    column
                    (atl-long-lines-do-toggle)
                    (nreverse calls)))))))
         '((80 0)
           (80 79)
           (80 80)
           (80 81)
           (1 1)
           (0 1)))"##,
        expect![
            "OK ((80 0 :toggled (1)) (80 79 :toggled (1)) (80 80 :toggled (1)) (80 81 :toggled (-1)) (1 1 :toggled (1)) (0 1 :toggled (-1)))"
        ],
    )
}

fn atl_long_lines_toggle_changes_real_buffer_local_truncation_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_toggle_changes_real_buffer_local_truncation_state",
        r##"(with-temp-buffer
         (setq-local
          truncate-lines
          t)
         (let ((atl-long-lines-mode t)
               results)
           (cl-letf
               (((symbol-function
                  'window-width)
                 (lambda (&optional _window)
                   10)))
             (erase-buffer)
             (insert "short")
             (goto-char
              (point-min))
             (atl-long-lines-do-toggle)
             (push truncate-lines results)
             (erase-buffer)
             (insert
              "this line is longer than ten")
             (goto-char
              (point-min))
             (atl-long-lines-do-toggle)
             (push truncate-lines results)
             (erase-buffer)
             (insert "equalwidth")
             (goto-char
              (point-min))
             (atl-long-lines-do-toggle)
             (push truncate-lines results))
           (list
            (nreverse results)
            (local-variable-p
             'truncate-lines))))"##,
        expect!["OK ((t nil t) t)"],
    )
}

fn atl_long_lines_toggle_reacts_to_the_line_at_point_in_a_mixed_document() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_toggle_reacts_to_the_line_at_point_in_a_mixed_document",
        r##"(with-temp-buffer
         (insert
          "tiny\n"
          "this line is deliberately much longer\n"
          "middle\n")
         (let ((atl-long-lines-mode t)
               (truncate-lines nil)
               states)
           (cl-letf
               (((symbol-function
                  'window-width)
                 (lambda (&optional _window)
                   12)))
             (dolist (line
                      '(0 1 2 1 0))
               (goto-char
                (point-min))
               (forward-line line)
               (atl-long-lines-do-toggle)
               (push
                (list
                 (line-number-at-pos)
                 (atl-long-lines--end-line-column)
                 truncate-lines)
                states)))
           (nreverse states)))"##,
        expect!["OK ((1 4 t) (2 37 nil) (3 6 t) (2 37 nil) (1 4 t))"],
    )
}

fn atl_long_lines_toggle_measures_and_toggles_exactly_once_per_enabled_invocation()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_toggle_measures_and_toggles_exactly_once_per_enabled_invocation",
        r##"(with-temp-buffer
         (let ((atl-long-lines-mode t)
               (measurements 0)
               (width-reads 0)
               (toggles 0))
           (cl-letf
               (((symbol-function
                  'window-width)
                 (lambda (&optional _window)
                   (setq
                    width-reads
                    (1+ width-reads))
                   20))
                ((symbol-function
                  'atl-long-lines--end-line-column)
                 (lambda ()
                   (setq
                    measurements
                    (1+ measurements))
                   21))
                ((symbol-function
                  'toggle-truncate-lines)
                 (lambda (&optional _value)
                   (setq
                    toggles
                    (1+ toggles)))))
             (dotimes
                 (_ 4)
               (atl-long-lines-do-toggle))
             (list
              width-reads
              measurements
              toggles))))"##,
        expect!["OK (4 4 4)"],
    )
}

fn atl_long_lines_mute_apply_returns_last_value_and_preserves_body_evaluation_order()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_mute_apply_returns_last_value_and_preserves_body_evaluation_order",
        r##"(let (events)
         (list
          (atl-long-lines--mute-apply
            (push :first events)
            (push :second events)
            (list :result
                  (length events)))
          (nreverse events)))"##,
        expect!["OK ((:result 2) (:first :second))"],
    )
}

fn atl_long_lines_mute_apply_establishes_quiet_bindings_and_restores_outer_message_policy()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_mute_apply_establishes_quiet_bindings_and_restores_outer_message_policy",
        r##"(let ((message-log-max t)
               (inhibit-message nil)
               (messages nil))
         (cl-letf
             (((symbol-function 'message)
               (lambda (format-string &rest args)
                 (push
                  (apply
                   #'format
                   format-string
                   args)
                  messages)
                 "fixture-message")))
           (let ((result
                  (atl-long-lines--mute-apply
                    (list
                     message-log-max
                     inhibit-message
                     (message
                      "hidden %s"
                      7)))))
             (list
              result
              (nreverse messages)
              message-log-max
              inhibit-message))))"##,
        expect![[r#"OK ((nil t "fixture-message") ("hidden 7") t nil)"#]],
    )
}

fn atl_long_lines_mute_apply_error_path_preserves_defvar_bool_truthy_canonicalization()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_mute_apply_error_path_preserves_defvar_bool_truthy_canonicalization",
        r##"(let ((message-log-max 321)
               (inhibit-message :outer))
         (let ((before inhibit-message))
           (condition-case error-data
               (atl-long-lines--mute-apply
                 (error
                  "fixture failure %s/%s"
                  message-log-max
                  inhibit-message))
             (error
              (list
               before
               (car error-data)
               (cadr error-data)
               message-log-max
               inhibit-message)))))"##,
        expect![[r#"OK (t error "fixture failure nil/t" 321 t)"#]],
    )
}

fn atl_long_lines_mute_apply_macro_declaration_and_expansion_keep_all_body_forms() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atl_long_lines_mute_apply_macro_declaration_and_expansion_keep_all_body_forms",
        r##"(let ((expansion
                (macroexpand
                 '(atl-long-lines--mute-apply
                    (setq first 1)
                    (+ first 2)))))
         (list
          (get
           'atl-long-lines--mute-apply
           'lisp-indent-function)
          (get
           'atl-long-lines--mute-apply
           'edebug-form-spec)
          (car expansion)
          (and
           (string-match-p
            "message-log-max"
            (prin1-to-string expansion))
           t)
          (and
           (string-match-p
            "inhibit-message"
            (prin1-to-string expansion))
           t)
          (let (first)
            (eval expansion t))))"##,
        expect!["OK (0 t let t t 3)"],
    )
}

pub(super) fn toggling_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atl_long_lines_toggle_is_a_true_noop_when_minor_mode_is_disabled(),
        atl_long_lines_toggle_chooses_truncation_at_or_below_width_and_wrapping_above_width(),
        atl_long_lines_toggle_changes_real_buffer_local_truncation_state(),
        atl_long_lines_toggle_reacts_to_the_line_at_point_in_a_mixed_document(),
        atl_long_lines_toggle_measures_and_toggles_exactly_once_per_enabled_invocation(),
        atl_long_lines_mute_apply_returns_last_value_and_preserves_body_evaluation_order(),
        atl_long_lines_mute_apply_establishes_quiet_bindings_and_restores_outer_message_policy(),
        atl_long_lines_mute_apply_error_path_preserves_defvar_bool_truthy_canonicalization(),
        atl_long_lines_mute_apply_macro_declaration_and_expansion_keep_all_body_forms(),
    ]
}
