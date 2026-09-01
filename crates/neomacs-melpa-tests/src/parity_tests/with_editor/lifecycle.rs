use expect_test::expect;

use super::ParityBatchCase;

fn with_editor_finish_runs_query_pre_return_and_post_hooks_in_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "with_editor_finish_runs_query_pre_return_and_post_hooks_in_order",
        r##"(with-temp-buffer
               (let (events)
                 (setq-local
                  with-editor-finish-query-functions
                  (list (lambda (force)
                          (push (list 'query force) events)
                          t)))
                 (setq-local
                  with-editor-pre-finish-hook
                  (list (lambda () (push 'pre events))))
                 (setq-local
                  with-editor-post-finish-hook
                  (list (lambda () (push 'post events))))
                 (cl-letf (((symbol-function 'with-editor-return)
                            (lambda (cancel)
                              (push (list 'return cancel)
                                    events)))
                           ((symbol-function 'accept-process-output)
                            (lambda (&rest _arguments) nil)))
                   (with-editor-finish 'force))
                 (nreverse events)))"##,
        expect![[r#"OK ((query force) pre (return nil) post)"#]],
    )
}

fn with_editor_finish_stops_when_any_query_rejects_session() -> ParityBatchCase {
    ParityBatchCase::value(
        "with_editor_finish_stops_when_any_query_rejects_session",
        r##"(with-temp-buffer
               (let (events)
                 (setq-local
                  with-editor-finish-query-functions
                  (list
                   (lambda (force)
                     (push (list 'first force) events)
                     t)
                   (lambda (force)
                     (push (list 'second force) events)
                     nil)
                   (lambda (_force)
                     (push 'unreachable events)
                     t)))
                 (setq-local
                  with-editor-pre-finish-hook
                  (list (lambda () (push 'pre events))))
                 (cl-letf (((symbol-function 'with-editor-return)
                            (lambda (_cancel)
                              (push 'return events))))
                   (with-editor-finish nil))
                 (nreverse events)))"##,
        expect![[r#"OK ((first nil) (second nil))"#]],
    )
}

fn with_editor_cancel_runs_cancel_hooks_and_reports_custom_message() -> ParityBatchCase {
    ParityBatchCase::value(
        "with_editor_cancel_runs_cancel_hooks_and_reports_custom_message",
        r##"(with-temp-buffer
               (let (events)
                 (setq-local
                  with-editor-cancel-query-functions
                  (list (lambda (force)
                          (push (list 'query force) events)
                          t)))
                 (setq-local
                  with-editor-pre-cancel-hook
                  (list (lambda () (push 'pre events))))
                 (setq-local
                  with-editor-post-cancel-hook
                  (list (lambda () (push 'post events))))
                 (setq-local
                  with-editor-cancel-message
                  (lambda () "custom cancel"))
                 (cl-letf (((symbol-function 'with-editor-return)
                            (lambda (cancel)
                              (push (list 'return cancel)
                                    events)))
                           ((symbol-function 'accept-process-output)
                            (lambda (&rest _arguments) nil))
                           ((symbol-function 'message)
                            (lambda (format-string &rest arguments)
                              (push
                               (apply #'format
                                      format-string arguments)
                               events))))
                   (with-editor-cancel 'force))
                 (nreverse events)))"##,
        expect![[r#"OK ((query force) pre (return t) post "custom cancel")"#]],
    )
}

fn with_editor_mode_installs_local_kill_guard_and_disabling_keeps_guard() -> ParityBatchCase {
    ParityBatchCase::value(
        "with_editor_mode_installs_local_kill_guard_and_disabling_keeps_guard",
        r##"(with-temp-buffer
               (let ((with-editor-show-usage nil))
                 (with-editor-mode 1)
                 (let ((enabled
                        (list with-editor-mode
                              (and
                               (memq
                                #'with-editor-kill-buffer-noop
                                kill-buffer-query-functions)
                               t)
                              (local-variable-p
                               'kill-buffer-query-functions))))
                   (with-editor-mode -1)
                   (list
                    enabled
                    with-editor-mode
                    (and
                     (memq
                      #'with-editor-kill-buffer-noop
                      kill-buffer-query-functions)
                     t)))))"##,
        expect![[r#"OK ((t t t) nil t)"#]],
    )
}

pub(super) fn lifecycle_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        with_editor_finish_runs_query_pre_return_and_post_hooks_in_order(),
        with_editor_finish_stops_when_any_query_rejects_session(),
        with_editor_cancel_runs_cancel_hooks_and_reports_custom_message(),
        with_editor_mode_installs_local_kill_guard_and_disabling_keeps_guard(),
    ]
}
