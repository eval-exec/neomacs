use expect_test::expect;

use super::ParityBatchCase;

fn enabled_mode_observes_real_navigation_but_not_programmatic_or_disabled_motion() -> ParityBatchCase
{
    ParityBatchCase::value(
        "enabled_mode_observes_real_navigation_but_not_programmatic_or_disabled_motion",
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "alphaBeta gamma\nsecond line\nthird")
  (goto-char (point-min))
  (let* ((events nil)
         (phase nil)
         (ah-before-move-cursor-hook
          (list
           (lambda ()
             (push
              (list
               'before phase (point)
               (line-number-at-pos) (current-column))
              events))))
         (ah-after-move-cursor-hook
          (list
           (lambda ()
             (push
              (list
               'after phase (point)
               (line-number-at-pos) (current-column))
              events))))
         motion-error
         enabled-events)
    (unwind-protect
        (progn
          (ah-mode 1)

          (setq phase 'forward-char)
          (call-interactively #'forward-char)
          (setq phase 'backward-char)
          (call-interactively #'backward-char)
          (setq phase 'next-line)
          (call-interactively #'next-line)
          (setq phase 'end-of-line)
          (call-interactively #'move-end-of-line)
          (setq phase 'previous-line)
          (call-interactively #'previous-line)
          (setq phase 'beginning-of-line)
          (call-interactively #'move-beginning-of-line)
          (setq phase 'end-of-buffer)
          (call-interactively #'end-of-buffer)
          (setq phase 'beginning-of-buffer)
          (call-interactively #'beginning-of-buffer)

          ;; A Lisp caller can move without looking like a user command.
          (forward-char 3)

          ;; A real failed motion runs the before hook but never the after hook.
          (goto-char (point-max))
          (setq phase 'failed-forward)
          (setq
           motion-error
           (condition-case signal-data
               (call-interactively #'forward-char)
             (error
              (list (car signal-data) (cdr signal-data)))))
          (setq enabled-events (nreverse events))
          (setq events nil)

          (ah-mode -1)
          (setq phase 'disabled)
          (goto-char (point-min))
          (call-interactively #'forward-char)
          (list
           (buffer-string)
           (point)
           motion-error
           enabled-events
           (nreverse events)
           ah-mode))
      (ah-mode -1))))
"##,
        expect![[
            r#"OK ("alphaBeta gamma\nsecond line\nthird" 2 (end-of-buffer nil) ((before forward-char 1 1 0) (after forward-char 2 1 1) (before backward-char 2 1 1) (after backward-char 1 1 0) (before next-line 1 1 0) (after next-line 17 2 0) (before end-of-line 17 2 0) (after end-of-line 28 2 11) (before previous-line 28 2 11) (after previous-line 12 1 11) (before beginning-of-line 12 1 11) (after beginning-of-line 1 1 0) (before end-of-buffer 1 1 0) (after end-of-buffer 34 3 5) (before beginning-of-buffer 34 3 5) (after beginning-of-buffer 1 1 0) (before failed-forward 34 3 5)) nil nil)"#
        ]],
    )
}

pub(super) fn cursor_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![enabled_mode_observes_real_navigation_but_not_programmatic_or_disabled_motion()]
}
