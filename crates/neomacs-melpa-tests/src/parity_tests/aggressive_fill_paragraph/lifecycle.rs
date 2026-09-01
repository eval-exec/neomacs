use expect_test::expect;

use super::ParityBatchCase;

fn aggressive_fill_paragraph_recommended_setup_keeps_live_buffers_independent() -> ParityBatchCase {
    ParityBatchCase::value(
        "aggressive_fill_paragraph_recommended_setup_keeps_live_buffers_independent",
        r####"(let ((text-mode-hook
                                  nil)
                                 (prog-mode-hook
                                  nil)
                                 prose-buffer
                                 code-buffer)
                             (afp-setup-recommended-hooks)
                             (setq
                              prose-buffer
                              (generate-new-buffer
                               " *afp-prose-lifecycle*")
                              code-buffer
                              (generate-new-buffer
                               " *afp-code-lifecycle*"))
                             (unwind-protect
                                 (progn
                                   (with-current-buffer
                                       prose-buffer
                                     (text-mode)
                                     (setq
                                      fill-column
                                      34)
                                     (insert
                                      "The release operator verifies recovery state before promoting the parser")
                                     (let ((last-command-event
                                            ?\s)
                                           (this-command
                                            'self-insert-command)
                                           (real-this-command
                                            'self-insert-command))
                                       (self-insert-command
                                        1)))
                                   (with-current-buffer
                                       code-buffer
                                     (emacs-lisp-mode)
                                     (setq
                                      fill-column
                                      40)
                                     (insert
                                      ";; The parser preserves every request while retrying a failed recovery transition")
                                     (let ((last-command-event
                                            ?\s)
                                           (this-command
                                            'self-insert-command)
                                           (real-this-command
                                            'self-insert-command))
                                       (self-insert-command
                                        1)))
                                   (let ((enabled-state
                                          (list
                                           (with-current-buffer
                                               prose-buffer
                                             (list
                                              aggressive-fill-paragraph-mode
                                              (buffer-string)
                                              (point)))
                                           (with-current-buffer
                                               code-buffer
                                             (list
                                              aggressive-fill-paragraph-mode
                                              (buffer-string)
                                              (point))))))
                                     (with-current-buffer
                                         prose-buffer
                                       (aggressive-fill-paragraph-mode
                                        -1)
                                       (goto-char
                                        (point-max))
                                       (insert
                                        "\n\nA disabled buffer leaves this second deliberately long paragraph completely unchanged")
                                       (let ((last-command-event
                                              ?\s)
                                             (this-command
                                              'self-insert-command)
                                             (real-this-command
                                              'self-insert-command))
                                         (self-insert-command
                                          1)))
                                     (list
                                      enabled-state
                                      (with-current-buffer
                                          prose-buffer
                                        (list
                                         aggressive-fill-paragraph-mode
                                         (buffer-string)
                                         (point)))
                                      (with-current-buffer
                                          code-buffer
                                        (list
                                         aggressive-fill-paragraph-mode
                                         (buffer-string)
                                         (point))))))
                               (dolist (buffer
                                        (list
                                         prose-buffer
                                         code-buffer))
                                 (when
                                     (buffer-live-p
                                      buffer)
                                   (with-current-buffer
                                       buffer
                                     (set-buffer-modified-p
                                      nil))
                                   (kill-buffer
                                    buffer)))))"####,
        expect![[
            r#"OK (((t "The release operator verifies\nrecovery state before promoting\nthe parser " 74) (t ";; The parser preserves every request\n;; while retrying a failed recovery\n;; transition " 89)) (nil "The release operator verifies\nrecovery state before promoting\nthe parser \n\nA disabled buffer leaves this second deliberately long paragraph completely unchanged " 162) (t ";; The parser preserves every request\n;; while retrying a failed recovery\n;; transition " 89))"#
        ]],
    )
}

pub(super) fn lifecycle_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![aggressive_fill_paragraph_recommended_setup_keeps_live_buffers_independent()]
}
