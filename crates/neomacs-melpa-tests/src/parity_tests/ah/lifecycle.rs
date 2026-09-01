use expect_test::expect;

use super::ParityBatchCase;

fn autoloaded_public_mode_enables_hooks_updates_its_lighter_and_disables_cleanly() -> ParityBatchCase
{
    ParityBatchCase::value(
        "autoloaded_public_mode_enables_hooks_updates_its_lighter_and_disables_cleanly",
        r##"
(let ((was-autoloaded (autoloadp (symbol-function 'ah-mode)))
      (loaded-before (featurep 'ah))
      (events nil)
      enabled-state
      customized-state
      final-point)
  (unwind-protect
      (progn
        ;; Invoke the generated public autoload rather than loading ah.el
        ;; directly.  The command must activate the complete package.
        (ah-mode 1)
        (let ((ah-before-move-cursor-hook
               (list (lambda () (push (list 'before (point)) events))))
              (ah-after-move-cursor-hook
               (list (lambda () (push (list 'after (point)) events)))))
          (with-temp-buffer
            (insert "abc")
            (goto-char (point-min))
            (call-interactively #'forward-char)
            (setq
             enabled-state
             (list
              ah-mode
              (featurep 'ah)
              ah-lighter
              (cdr (assq 'ah-mode minor-mode-alist))
              (point)))

            (setq ah-lighter " AH!")
            (setq
             customized-state
             (list
              ah-lighter
              (cdr (assq 'ah-mode minor-mode-alist))))

            (ah-mode -1)
            (call-interactively #'backward-char)
            (setq final-point (point))))
        (list
         was-autoloaded
         loaded-before
         enabled-state
         customized-state
         final-point
         (nreverse events)
         ah-mode))
    (ah-mode -1)))
"##,
        expect![[
            r#"OK (t nil (t t " Hooks" #1=((:eval (format "%s" ah-lighter))) 2) (" AH!" #1#) 1 ((before 1) (after 2)) nil)"#
        ]],
    )
}

pub(super) fn lifecycle_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![autoloaded_public_mode_enables_hooks_updates_its_lighter_and_disables_cleanly()]
}
