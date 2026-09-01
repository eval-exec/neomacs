use expect_test::expect;

use super::ParityBatchCase;

fn goto_chg_navigates_real_insertions_backward_then_forward_by_change_time() -> ParityBatchCase {
    ParityBatchCase::value(
        "goto_chg_navigates_real_insertions_backward_then_forward_by_change_time",
        r##"(with-temp-buffer
               (buffer-enable-undo)
               (insert (make-string 80 ?-))
               (setq buffer-undo-list nil)
               (goto-char 10)
               (insert "A")
               (undo-boundary)
               (goto-char 40)
               (insert "B")
               (undo-boundary)
               (goto-char 70)
               (insert "C")
               (undo-boundary)
               (goto-char 1)
               (let ((glc-default-span 2)
                     positions)
                 (let ((this-command 'goto-last-change)
                       (last-command 'other))
                   (goto-last-change nil)
                   (push (point) positions))
                 (dotimes (_ 2)
                   (let ((this-command 'goto-last-change)
                         (last-command 'goto-last-change))
                     (goto-last-change nil)
                     (push (point) positions)))
                 (dotimes (_ 2)
                   (let ((this-command 'goto-last-change-reverse)
                         (last-command 'goto-last-change-reverse))
                     (goto-last-change-reverse nil)
                     (push (point) positions)))
                 (list
                  (nreverse positions)
                  glc-probe-depth
                  glc-direction
                  glc-current-span)))"##,
        expect!["OK ((71 41 11 41 71) 1 -1 2)"],
    )
}

fn goto_chg_navigates_real_deletion_and_property_change_entries() -> ParityBatchCase {
    ParityBatchCase::value(
        "goto_chg_navigates_real_deletion_and_property_change_entries",
        r##"(with-temp-buffer
               (buffer-enable-undo)
               (insert
                "01234567890123456789012345678901234567890123456789")
               (setq buffer-undo-list nil)
               (delete-region 8 13)
               (undo-boundary)
               (add-text-properties 22 27 '(face bold))
               (undo-boundary)
               (goto-char 1)
               (let ((glc-default-span 0)
                     positions messages)
                 (dotimes (index 2)
                   (let ((this-command 'goto-last-change)
                         (last-command
                          (if (zerop index)
                              'other
                            'goto-last-change)))
                     (cl-letf (((symbol-function 'message)
                                (lambda (format-string &rest args)
                                  (push
                                   (and format-string
                                        (apply #'format
                                               format-string args))
                                   messages))))
                       (goto-last-change 0))
                     (push (point) positions)))
                 (list
                  (nreverse positions)
                  (nreverse messages)
                  glc-probe-depth
                  glc-current-span)))"##,
        expect![[r#"OK ((27 8) ("T-1: Property change" "T-2: Deleted \"78901\"") 2 0)"#]],
    )
}

fn goto_chg_numeric_and_universal_arguments_update_the_active_span() -> ParityBatchCase {
    ParityBatchCase::value(
        "goto_chg_numeric_and_universal_arguments_update_the_active_span",
        r##"(with-temp-buffer
               (buffer-enable-undo)
               (insert (make-string 60 ?x))
               (setq buffer-undo-list nil)
               (goto-char 10)
               (insert "A")
               (undo-boundary)
               (goto-char 40)
               (insert "B")
               (undo-boundary)
               (goto-char 1)
               (let ((glc-default-span 3)
                     spans messages)
                 (let ((this-command 'goto-last-change)
                       (last-command 'other))
                   (goto-last-change 0)
                   (push glc-current-span spans))
                 (let ((this-command 'goto-last-change)
                       (last-command 'goto-last-change))
                   (cl-letf (((symbol-function 'message)
                              (lambda (format-string &rest args)
                                (push
                                 (apply #'format format-string args)
                                 messages))))
                     (goto-last-change '(4)))
                   (push glc-current-span spans))
                 (list
                  (nreverse spans)
                  (nreverse messages)
                  glc-direction)))"##,
        expect![[r#"OK ((0 12) ("Current span is 12 chars") 1)"#]],
    )
}

fn goto_chg_reverse_normalizes_each_prefix_shape_before_delegating() -> ParityBatchCase {
    ParityBatchCase::value(
        "goto_chg_reverse_normalizes_each_prefix_shape_before_delegating",
        r##"(let (calls)
               (cl-letf (((symbol-function 'goto-last-change)
                          (lambda (arg)
                            (push
                             (list arg this-command last-command)
                             calls))))
                 (dolist (case
                          '((nil other reverse)
                            (- other reverse)
                            ((4) other reverse)
                            (7 other reverse)
                            (nil reverse reverse)))
                   (let ((last-command
                          (if (eq (cadr case) 'reverse)
                              'goto-last-change-reverse
                            'other))
                         (this-command
                          'goto-last-change-reverse))
                     (goto-last-change-reverse
                      (car case))))
                 (nreverse calls)))"##,
        expect![
            "OK ((- goto-last-change other) (nil goto-last-change other) ((-4) goto-last-change other) (-7 goto-last-change other) (- goto-last-change goto-last-change))"
        ],
    )
}

fn goto_chg_first_call_skips_current_edit_after_obvious_edit_commands() -> ParityBatchCase {
    ParityBatchCase::value(
        "goto_chg_first_call_skips_current_edit_after_obvious_edit_commands",
        r##"(with-temp-buffer
               (buffer-enable-undo)
               (insert (make-string 50 ?x))
               (setq buffer-undo-list nil)
               (goto-char 10)
               (insert "A")
               (undo-boundary)
               (goto-char 35)
               (insert "B")
               (undo-boundary)
               (goto-char 35)
               (let ((glc-default-span 0))
                 (mapcar
                  (lambda (previous)
                    (setq glc-probe-depth 0)
                    (let ((this-command 'goto-last-change)
                          (last-command previous))
                      (goto-last-change 0)
                      (list previous
                            (point)
                            glc-probe-depth)))
                  '(other yank self-insert-command kill-region))))"##,
        expect!["OK ((other 36 1) (yank 11 2) (self-insert-command 11 2) (kill-region 36 1))"],
    )
}

fn goto_chg_signals_when_the_buffer_has_no_changes() -> ParityBatchCase {
    ParityBatchCase::signal(
        "goto_chg_signals_when_the_buffer_has_no_changes",
        r##"(with-temp-buffer
               (let ((this-command 'goto-last-change)
                     (last-command 'other))
                 (goto-last-change nil)))"##,
        expect![[r#"ERR (error "No change info (undo is disabled)")"#]],
    )
}

fn goto_chg_signals_when_undo_is_disabled() -> ParityBatchCase {
    ParityBatchCase::signal(
        "goto_chg_signals_when_undo_is_disabled",
        r##"(with-temp-buffer
               (setq buffer-undo-list t)
               (let ((this-command 'goto-last-change)
                     (last-command 'other))
                 (goto-last-change nil)))"##,
        expect![[r#"ERR (error "No change info (undo is disabled)")"#]],
    )
}

fn goto_chg_rejects_reverse_direction_as_the_first_operation() -> ParityBatchCase {
    ParityBatchCase::signal(
        "goto_chg_rejects_reverse_direction_as_the_first_operation",
        r##"(with-temp-buffer
               (buffer-enable-undo)
               (insert "changed")
               (let ((this-command 'goto-last-change)
                     (last-command 'other))
                 (goto-last-change -1)))"##,
        expect![[r#"ERR (error "Negative arg: Cannot reverse as the first operation")"#]],
    )
}

fn goto_chg_signals_at_the_older_and_newer_ends_of_history() -> ParityBatchCase {
    ParityBatchCase::value(
        "goto_chg_signals_at_the_older_and_newer_ends_of_history",
        r##"(with-temp-buffer
               (buffer-enable-undo)
               (insert (make-string 40 ?x))
               (setq buffer-undo-list nil)
               (goto-char 10)
               (insert "A")
               (undo-boundary)
               (goto-char 30)
               (insert "B")
               (undo-boundary)
               (goto-char 1)
               (let ((glc-default-span 0)
                     older newer)
                 (let ((this-command 'goto-last-change)
                       (last-command 'other))
                   (goto-last-change 0))
                 (let ((this-command 'goto-last-change)
                       (last-command 'goto-last-change))
                   (goto-last-change 0))
                 (setq older
                       (condition-case err
                           (let ((this-command 'goto-last-change)
                                 (last-command 'goto-last-change))
                             (goto-last-change 0)
                             'no-signal)
                         (error (list (car err) (cdr err)))))
                 (let ((this-command 'goto-last-change-reverse)
                       (last-command 'goto-last-change-reverse))
                   (goto-last-change-reverse nil))
                 (setq newer
                       (condition-case err
                           (let ((this-command
                                  'goto-last-change-reverse)
                                 (last-command
                                  'goto-last-change-reverse))
                             (goto-last-change-reverse nil)
                             'no-signal)
                         (error (list (car err) (cdr err)))))
                 (list older newer glc-probe-depth glc-direction)))"##,
        expect![[
            r#"OK ((error ("No further change info")) (error ("No later change info")) 1 -1)"#
        ]],
    )
}

pub(super) fn navigation_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        goto_chg_navigates_real_insertions_backward_then_forward_by_change_time(),
        goto_chg_navigates_real_deletion_and_property_change_entries(),
        goto_chg_numeric_and_universal_arguments_update_the_active_span(),
        goto_chg_reverse_normalizes_each_prefix_shape_before_delegating(),
        goto_chg_first_call_skips_current_edit_after_obvious_edit_commands(),
        goto_chg_signals_when_the_buffer_has_no_changes(),
        goto_chg_signals_when_undo_is_disabled(),
        goto_chg_rejects_reverse_direction_as_the_first_operation(),
        goto_chg_signals_at_the_older_and_newer_ends_of_history(),
    ]
}
