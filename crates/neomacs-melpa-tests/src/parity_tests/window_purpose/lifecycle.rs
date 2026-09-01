use expect_test::expect;

use super::ParityBatchCase;

fn global_mode_installs_routing_once_and_disables_without_restoring_the_override() -> ParityBatchCase
{
    let elisp_form = r##"
(let ((purpose-fix-togglers-hook nil)
      (purpose-mode-hook
       '(neomacs-window-purpose-test-record-mode-hook))
      (display-buffer-overriding-action
       '((display-buffer-same-window)))
      (neomacs-window-purpose-test-mode-trace nil)
      result)
  (unwind-protect
      (progn
        (when purpose-mode
          (purpose-mode -1))
        (let ((before (neomacs-window-purpose-test-mode-state)))
          (purpose-mode 1)
          (let ((enabled (neomacs-window-purpose-test-mode-state))
                (enabled-togglers
                 (sort (mapcar #'symbol-name purpose-fix-togglers-hook)
                       #'string<)))
            (purpose-mode 1)
            (let ((enabled-again
                   (neomacs-window-purpose-test-mode-state)))
              (purpose-mode 0)
              (setq result
                    (list
                     :before before
                     :enabled enabled
                     :enabled-again-same (equal enabled enabled-again)
                     :fix-togglers enabled-togglers
                     :fix-togglers-persist-after-disable
                     (equal enabled-togglers
                            (sort
                             (mapcar #'symbol-name
                                     purpose-fix-togglers-hook)
                             #'string<))
                     :disabled
                     (neomacs-window-purpose-test-mode-state)
                     :mode-hook-trace
                     (nreverse
                      neomacs-window-purpose-test-mode-trace))))))
        result)
    (when purpose-mode
      (purpose-mode -1))))
"##;
    let expect = expect![[
        r#"OK (:before (:mode nil :active nil :advices nil :overriding-action ((display-buffer-same-window)) :switch-key purpose-switch-buffer-overload :modeline " [edit]") :enabled (:mode t :active t :advices (switch-to-buffer switch-to-buffer-other-window switch-to-buffer-other-frame pop-to-buffer pop-to-buffer-same-window display-buffer) :overriding-action (purpose--action-function) :switch-key purpose-switch-buffer-overload :modeline " [edit]") :enabled-again-same t :fix-togglers ("purpose--fix-compilation-next-error-function-advice-toggler" "purpose--fix-edebug-pop-to-buffer-advice-toggler" "purpose--fix-next-error-advice-toggler" "purpose--fix-org-get-location-advice-toggler" "purpose--fix-org-goto-location-advice-toggler" "purpose--fix-org-switch-to-buffer-other-window-advice-toggler" "purpose--fix-popwin:replicate-window-config-advice-toggler" "purpose--fix-whitespace-display-window-advice-toggler") :fix-togglers-persist-after-disable t :disabled (:mode nil :active nil :advices nil :overriding-action (purpose--action-function) :switch-key purpose-switch-buffer-overload :modeline " [edit]") :mode-hook-trace ((t t t) (t t t) (nil nil nil)))"#
    ]];
    ParityBatchCase::value(
        "global_mode_installs_routing_once_and_disables_without_restoring_the_override",
        elisp_form,
        expect,
    )
}

pub(crate) fn lifecycle_batch_cases() -> Vec<ParityBatchCase> {
    vec![global_mode_installs_routing_once_and_disables_without_restoring_the_override()]
}
