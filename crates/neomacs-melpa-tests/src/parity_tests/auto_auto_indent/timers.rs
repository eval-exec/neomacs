use expect_test::expect;

use super::ParityBatchCase;

fn auto_auto_indent_timer_callback_indents_at_marker_then_surfaces_upstream_setq_failure()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_timer_callback_indents_at_marker_then_surfaces_upstream_setq_failure",
        r##"(let ((target
                                (generate-new-buffer
                                 " *aai-timer-target*"))
                               events)
          (unwind-protect
              (with-temp-buffer
                (insert "caller")
                (set-buffer-modified-p t)
                (with-current-buffer target
                  (insert "first\nsecond\nthird")
                  (goto-char (point-min))
                  (forward-line 1)
                  (setq-local
                   aai-indent-function
                   (lambda ()
                     (push
                      (list
                       (buffer-name)
                       (point)
                       (line-number-at-pos))
                      events))))
                (let ((marker
                       (with-current-buffer target
                         (point-marker)))
                      (aai--timer
                       :pending))
                  (list
                   (auto-auto-indent-test-error-data
                    (lambda ()
                      (aai-on-timer marker)))
                   aai--timer
                   (nreverse events)
                   (with-current-buffer target
                     (point)))))
            (when (buffer-live-p target)
              (kill-buffer target))))"##,
        expect![[
            r#"OK ((:error wrong-number-of-arguments (setq 1)) :pending ((" *aai-timer-target*" 7 2)) 7)"#
        ]],
    )
}

fn auto_auto_indent_timer_modified_check_uses_calling_buffer_before_final_setq_failure()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_timer_modified_check_uses_calling_buffer_before_final_setq_failure",
        r##"(mapcar
          (lambda (states)
            (let ((target
                   (generate-new-buffer
                    " *aai-timer-state*"))
                  events)
              (unwind-protect
                  (with-temp-buffer
                    (insert "caller")
                    (set-buffer-modified-p
                     (car states))
                    (with-current-buffer target
                      (insert "target")
                      (set-buffer-modified-p
                       (cadr states))
                      (setq-local
                       aai-indent-function
                       (lambda ()
                         (push
                          (list
                           :indented
                           (buffer-modified-p))
                          events))))
                    (let ((marker
                           (with-current-buffer target
                             (point-marker)))
                          (aai--timer
                           :pending))
                      (list
                       states
                       (auto-auto-indent-test-error-data
                        (lambda ()
                          (aai-on-timer marker)))
                       aai--timer
                       (nreverse events))))
                (when (buffer-live-p target)
                  (kill-buffer target)))))
          '((nil t)
            (t nil)
            (nil nil)
            (t t)))"##,
        expect![
            "OK (((nil t) (:error wrong-number-of-arguments #1=(setq 1)) :pending nil) ((t nil) (:error wrong-number-of-arguments #1#) :pending ((:indented nil))) ((nil nil) (:error wrong-number-of-arguments #1#) :pending nil) ((t t) (:error wrong-number-of-arguments #1#) :pending ((:indented t))))"
        ],
    )
}

fn auto_auto_indent_timer_dead_marker_error_leaves_pending_timer_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_timer_dead_marker_error_leaves_pending_timer_state",
        r##"(let* ((target
                                 (generate-new-buffer
                                  " *aai-dead-marker*"))
                                (marker
                                 (with-current-buffer target
                                   (point-marker))))
          (kill-buffer target)
          (with-temp-buffer
            (insert "modified caller")
            (let ((aai--timer
                   :still-pending))
              (list
               (marker-buffer marker)
               (auto-auto-indent-test-error-data
                (lambda ()
                  (aai-on-timer marker)))
               aai--timer))))"##,
        expect!["OK (nil (:error wrong-type-argument (stringp nil)) :still-pending)"],
    )
}

fn auto_auto_indent_scheduled_idle_callback_indents_then_surfaces_upstream_setq_failure()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_scheduled_idle_callback_indents_then_surfaces_upstream_setq_failure",
        r##"(with-temp-buffer
          (emacs-lisp-mode)
          (insert
           "(defun delayed ()\n"
           "(let ((value 3))\n"
           "(message \"%s\" value)))\n")
          (goto-char (point-min))
          (forward-line 1)
          (auto-auto-indent-mode 1)
          (let ((this-command
                 'self-insert-command)
                (last-command
                 'self-insert-command)
                (last-input-event ?x)
                (aai--change-flag t)
                callback
                scheduled)
            (cl-letf
                (((symbol-function
                   'run-with-idle-timer)
                  (lambda (delay repeat function)
                    (setq callback function
                          scheduled
                          (list
                           delay
                           repeat
                           (functionp function)))
                    :deterministic-timer)))
              (aai-post-command-hook)
              (let ((before
                     (buffer-string))
                    (timer-before
                     aai--timer))
                (list
                 scheduled
                 timer-before
                 (auto-auto-indent-test-error-data
                  (lambda ()
                    (funcall callback)))
                 aai--timer
                 before
                 (buffer-string)
                 (point))))))"##,
        expect![[
            r#"OK ((0.5 nil t) :deterministic-timer (:error wrong-number-of-arguments (setq 1)) :deterministic-timer "(defun delayed ()\n(let ((value 3))\n(message \"%s\" value)))\n" "(defun delayed ()\n(let ((value 3))\n  (message \"%s\" value)))\n" 19)"#
        ]],
    )
    .fresh_process()
}

fn auto_auto_indent_scheduled_marker_tracks_buffer_edits_before_callback() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_scheduled_marker_tracks_buffer_edits_before_callback",
        r##"(with-temp-buffer
          (insert "first\nsecond\n")
          (goto-char (point-min))
          (forward-line 1)
          (let ((aai-mode t)
                (aai--change-flag t)
                (this-command
                 'self-insert-command)
                (last-command
                 'self-insert-command)
                (last-input-event ?x)
                callback
                observed)
            (cl-letf
                (((symbol-function
                   'run-with-idle-timer)
                  (lambda (_delay _repeat function)
                    (setq callback function)
                    :timer)))
              (aai-post-command-hook))
            (goto-char (point-min))
            (insert "prefix\n")
            (cl-letf
                (((symbol-function 'aai-on-timer)
                  (lambda (marker)
                    (setq observed
                          (list
                           (marker-position marker)
                           (line-number-at-pos
                            marker)
                           (eq
                            (marker-buffer marker)
                            (current-buffer)))))))
              (funcall callback))
            (list
             (buffer-string)
             observed)))"##,
        expect![[r#"OK ("prefix\nfirst\nsecond\n" (14 3 t))"#]],
    )
    .fresh_process()
}

pub(super) fn timers_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_auto_indent_timer_callback_indents_at_marker_then_surfaces_upstream_setq_failure(),
        auto_auto_indent_timer_modified_check_uses_calling_buffer_before_final_setq_failure(),
        auto_auto_indent_timer_dead_marker_error_leaves_pending_timer_state(),
        auto_auto_indent_scheduled_idle_callback_indents_then_surfaces_upstream_setq_failure(),
        auto_auto_indent_scheduled_marker_tracks_buffer_edits_before_callback(),
    ]
}
