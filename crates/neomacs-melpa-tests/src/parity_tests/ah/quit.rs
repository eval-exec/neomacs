use expect_test::expect;

use super::ParityBatchCase;

fn keyboard_quit_and_real_isearch_abort_deliver_deferred_after_hooks() -> ParityBatchCase {
    ParityBatchCase::value(
        "keyboard_quit_and_real_isearch_abort_deliver_deferred_after_hooks",
        r##"
(let* ((events nil)
       (ah-before-c-g-hook
        (list
         (lambda ()
           (push
            (list
             'before this-command
             (and isearch-mode t)
             (point))
            events))))
       (ah-after-c-g-hook
        (list
         (lambda ()
           (push
            (list
             'after this-command
             (and isearch-mode t)
             (point))
            events))))
       (post-command-hook nil)
       (search-buffer (generate-new-buffer " *ah-real-isearch*"))
       keyboard-result
       isearch-result
       search-point)
  (unwind-protect
      (progn
        (ah-mode 1)

        (setq
         keyboard-result
         (condition-case signal-data
             (let ((this-command 'keyboard-quit))
               (call-interactively #'keyboard-quit))
           (quit
            (list (car signal-data) (cdr signal-data)))))
        ;; In the command loop the deferred callback runs after C-g.
        (let ((this-command 'keyboard-quit))
          (run-hooks 'post-command-hook))

        (switch-to-buffer search-buffer)
        (insert "alpha beta alpha")
        (goto-char (point-min))
        (isearch-mode t nil nil nil)
        (setq isearch-string "beta"
              isearch-message "beta")
        (isearch-search)
        (setq
         isearch-result
         (condition-case signal-data
             (let ((this-command 'isearch-abort))
               (call-interactively #'isearch-abort))
           (quit
            (list (car signal-data) (cdr signal-data)))))
        (let ((this-command 'isearch-abort))
          (run-hooks 'post-command-hook))
        (setq search-point (point))

        (list
         keyboard-result
         isearch-result
         search-point
         isearch-mode
         (nreverse events)
         post-command-hook))
    (when isearch-mode
      (isearch-cancel))
    (ah-mode -1)
    (when (buffer-live-p search-buffer)
      (kill-buffer search-buffer))))
"##,
        expect![
            "OK ((quit nil) (quit nil) 1 nil ((before keyboard-quit nil 1) (after keyboard-quit nil 1) (before isearch-abort t 11) (after isearch-abort nil 1)) nil)"
        ],
    )
}

pub(super) fn quit_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![keyboard_quit_and_real_isearch_abort_deliver_deferred_after_hooks()]
}
